use std::fs;
use std::io::Write;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use tauri::{AppHandle, Emitter};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

enum Outcome {
    Success,
    AlreadyCurrent,
    AppRunning,
    NetworkError,
    Error,
}

fn logs_dir() -> Result<PathBuf, ()> {
    let base = std::env::var("LOCALAPPDATA").map_err(|_| ())?;
    Ok(PathBuf::from(base).join("Brewinget").join("logs"))
}

fn log_file_path() -> Result<PathBuf, ()> {
    let name = chrono::Local::now()
        .format("auto-update-%Y-%m-%d-%H-%M-%S.log")
        .to_string();
    Ok(logs_dir()?.join(name))
}

fn detect_already_current(output: &str) -> bool {
    let lower = output.to_lowercase();
    [
        "no available upgrade found",
        "no newer package versions are available",
        "no applicable upgrade found",
        "already installed to the latest",
        // Danish
        "ingen tilgængelig opgradering",
        "ingen nyere pakkeversioner er tilgængelige",
    ]
    .iter()
    .any(|kw| lower.contains(kw))
}

// winget error messages for network failures (English and Danish)
fn detect_network_error(output: &str) -> bool {
    let lower = output.to_lowercase();
    [
        "no internet",
        "network",
        "unable to connect",
        "source is not available",
        "failed to update source",
        "internet forbindelse",  // Danish: "internet connection"
        "netværk",               // Danish: "network"
    ]
    .iter()
    .any(|kw| lower.contains(kw))
}

fn upgrade_package(pkg: &str, log: &mut dyn Write) -> Outcome {
    let out = Command::new("cmd")
        .creation_flags(CREATE_NO_WINDOW)
        .args([
            "/c",
            "winget",
            "upgrade",
            "--id",
            pkg,
            "--exact",
            "--silent",
            "--accept-package-agreements",
            "--accept-source-agreements",
        ])
        .output();

    match out {
        Err(e) => {
            writeln!(log, "  [spawn error] {e}").ok();
            Outcome::Error
        }
        Ok(o) => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            let stderr = String::from_utf8_lossy(&o.stderr);
            let mut combined = stdout.to_string();
            combined.push_str(&stderr);

            // Write meaningful stdout lines to the log
            for raw in stdout.lines() {
                let clean = crate::strip_ansi(raw);
                let last = clean.rsplit('\r').next().unwrap_or(clean.as_str()).trim();
                if last.chars().any(|c| c.is_ascii_alphabetic()) {
                    writeln!(log, "  {last}").ok();
                }
            }

            let code = o.status.code();
            if o.status.success() {
                Outcome::Success
            } else if detect_already_current(&combined) {
                Outcome::AlreadyCurrent
            } else if crate::detect_app_running(&combined, code) {
                Outcome::AppRunning
            } else if detect_network_error(&combined) {
                Outcome::NetworkError
            } else {
                Outcome::Error
            }
        }
    }
}

// Writes each line to a log file AND emits it as a Tauri install-output event.
// Used by run_live so the UI log modal shows progress in real time.
struct DualWriter {
    file: fs::File,
    app: AppHandle,
}

impl Write for DualWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let _ = self.file.write(buf);
        if let Ok(s) = std::str::from_utf8(buf) {
            let line = s.trim_end();
            if !line.is_empty() {
                self.app.emit("install-output", line).ok();
            }
        }
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        let _ = self.file.flush();
        Ok(())
    }
}

/// Run the auto-update logic interactively: writes to a log file AND streams
/// each output line to the UI via install-output events.  Emits install-complete
/// when done so the log modal closes properly.  Spawns a background thread.
pub fn run_live(app: AppHandle) {
    std::thread::spawn(move || {
        let log_path = match log_file_path() {
            Ok(p) => p,
            Err(_) => {
                app.emit("install-complete", "error").ok();
                return;
            }
        };
        if let Some(parent) = log_path.parent() {
            if fs::create_dir_all(parent).is_err() {
                app.emit("install-complete", "error").ok();
                return;
            }
        }
        let file = match fs::File::create(&log_path) {
            Ok(f) => f,
            Err(_) => {
                app.emit("install-complete", "error").ok();
                return;
            }
        };
        let mut log = DualWriter { file, app: app.clone() };

        writeln!(
            log,
            "Brewinget auto-update — {}",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
        )
        .ok();
        writeln!(log, "{}", "─".repeat(60)).ok();
        writeln!(log).ok();

        let prefs = crate::prefs::load_auto_update();
        let mut flagged: Vec<String> = prefs.into_keys().collect();
        flagged.sort();

        if flagged.is_empty() {
            writeln!(log, "No packages flagged for auto-update. Nothing to do.").ok();
            app.emit("install-complete", "success").ok();
            return;
        }

        writeln!(log, "Packages queued: {}", flagged.join(", ")).ok();
        writeln!(log).ok();

        let total = flagged.len();
        let mut n_ok = 0usize;
        let mut n_current = 0usize;
        let mut n_err = 0usize;

        for (i, pkg) in flagged.iter().enumerate() {
            writeln!(log, "── [{}/{}] {pkg} ──", i + 1, total).ok();
            match upgrade_package(pkg, &mut log) {
                Outcome::Success => {
                    n_ok += 1;
                    writeln!(log, "  ✓ Updated.").ok();
                }
                Outcome::AlreadyCurrent => {
                    n_current += 1;
                    writeln!(log, "  ⓘ Already up to date.").ok();
                }
                Outcome::AppRunning => {
                    n_err += 1;
                    writeln!(log, "  ⚠  Skipped — application is running.").ok();
                }
                Outcome::NetworkError => {
                    n_err += 1;
                    writeln!(log, "  ✕ Network unavailable.").ok();
                }
                Outcome::Error => {
                    n_err += 1;
                    writeln!(log, "  ✕ Update failed.").ok();
                }
            }
            writeln!(log).ok();
        }

        if n_current > 0 {
            writeln!(
                log,
                "── Summary: {n_ok} updated, {n_current} already current, {n_err} failed/skipped ──"
            )
            .ok();
        } else {
            writeln!(log, "── Summary: {n_ok} updated, {n_err} failed/skipped ──").ok();
        }

        app.emit("install-complete", "success").ok();
    });
}

pub fn run() {
    let log_path = match log_file_path() {
        Ok(p) => p,
        Err(_) => return,
    };

    if let Some(parent) = log_path.parent() {
        if fs::create_dir_all(parent).is_err() {
            return;
        }
    }

    let mut log = match fs::File::create(&log_path) {
        Ok(f) => f,
        Err(_) => return,
    };

    writeln!(
        log,
        "Brewinget auto-update — {}",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
    )
    .ok();
    writeln!(log, "{}", "─".repeat(60)).ok();
    writeln!(log).ok();

    // Skip gracefully if the UI is already open — avoids concurrent winget calls
    // fighting over the same packages and polluting the UI's log modal.
    if crate::mutex::ui_is_running() {
        writeln!(log, "Brewinget UI is open — skipping auto-update to avoid conflicts.").ok();
        writeln!(log, "The scheduled task will try again at its next run.").ok();
        return;
    }

    let prefs = crate::prefs::load_auto_update();
    let mut flagged: Vec<String> = prefs.into_keys().collect();
    flagged.sort(); // deterministic order across runs

    if flagged.is_empty() {
        writeln!(log, "No packages flagged for auto-update. Nothing to do.").ok();
        return;
    }

    writeln!(log, "Packages queued: {}", flagged.join(", ")).ok();
    writeln!(log).ok();

    let total = flagged.len();
    let mut n_ok = 0usize;
    let mut n_current = 0usize;
    let mut n_err = 0usize;

    for (i, pkg) in flagged.iter().enumerate() {
        writeln!(log, "── [{}/{}] {pkg} ──", i + 1, total).ok();
        match upgrade_package(pkg, &mut log) {
            Outcome::Success => {
                n_ok += 1;
                writeln!(log, "  ✓ Updated.").ok();
            }
            Outcome::AlreadyCurrent => {
                n_current += 1;
                writeln!(log, "  ⓘ Already up to date.").ok();
            }
            Outcome::AppRunning => {
                n_err += 1;
                writeln!(log, "  ⚠  Skipped — application is running.").ok();
            }
            Outcome::NetworkError => {
                n_err += 1;
                writeln!(log, "  ✕ Network unavailable — will retry next scheduled run.").ok();
            }
            Outcome::Error => {
                n_err += 1;
                writeln!(log, "  ✕ Update failed.").ok();
            }
        }
        writeln!(log).ok();
    }

    // Only show "already current" count when it's non-zero so quiet runs stay quiet.
    if n_current > 0 {
        writeln!(log, "── Summary: {n_ok} updated, {n_current} already current, {n_err} failed/skipped ──").ok();
    } else {
        writeln!(log, "── Summary: {n_ok} updated, {n_err} failed/skipped ──").ok();
    }
}
