use std::collections::HashSet;
use std::io::{BufRead, BufReader};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use tauri::Emitter;

#[cfg(target_os = "macos")]
mod brew;
#[cfg(target_os = "windows")]
mod winget;

#[cfg(target_os = "windows")]
mod headless;
#[cfg(target_os = "windows")]
mod mutex;
#[cfg(target_os = "windows")]
mod schedule;

mod prefs;

// Module alias: `pm` always refers to the right backend for the current platform.
#[cfg(target_os = "macos")]
use brew as pm;
#[cfg(target_os = "windows")]
use winget as pm;

// ── Per-package update-all control ───────────────────────────────────────────

/// Shared state between the update-all worker thread and the skip/abort commands.
/// Stored as Arc<QueueControl> in Tauri's managed state so every command can
/// reach it without a global.
pub(crate) struct QueueControl {
    skip:  Mutex<HashSet<String>>,
    abort: AtomicBool,
}

impl QueueControl {
    pub(crate) fn new() -> Self {
        Self {
            skip:  Mutex::new(HashSet::new()),
            abort: AtomicBool::new(false),
        }
    }
    pub(crate) fn reset(&self) {
        self.skip.lock().unwrap().clear();
        self.abort.store(false, Ordering::SeqCst);
    }
    pub(crate) fn should_skip(&self, pkg: &str) -> bool {
        self.skip.lock().unwrap().contains(pkg)
    }
    pub(crate) fn should_abort(&self) -> bool {
        self.abort.load(Ordering::SeqCst)
    }
    pub(crate) fn add_skip(&self, pkg: String) {
        self.skip.lock().unwrap().insert(pkg);
    }
    pub(crate) fn set_abort(&self) {
        self.abort.store(true, Ordering::SeqCst);
    }
}

/// Emitted when a package in the update queue starts running.
#[derive(serde::Serialize, Clone)]
pub(crate) struct PkgStartEvent {
    pub name:  String,
    pub index: usize,
    pub total: usize,
}

/// Emitted when a package in the update queue finishes (or is skipped).
#[derive(serde::Serialize, Clone)]
pub(crate) struct PkgDoneEvent {
    pub name:   String,
    pub status: String, // "success" | "error" | "skipped"
}

// ── Shared utilities ─────────────────────────────────────────────────────────

/// Returns true when the captured output / exit code signals that a running
/// process blocked the install or update.  Checks English and Danish phrases
/// because winget outputs in the system locale.
pub(crate) fn detect_app_running(output: &str, exit_code: Option<i32>) -> bool {
    // 1603 = Windows Installer generic failure (very often "app is running").
    // 0x80070005 as i32 = -2147024891 (ERROR_ACCESS_DENIED from a locked file).
    if matches!(exit_code, Some(1603) | Some(-2147024891)) {
        return true;
    }
    let lower = output.to_lowercase();
    let keywords: &[&str] = &[
        // English (winget output + installer log phrases)
        "currently running",
        "in use",
        "close the application",
        "please close",
        "running process",
        "file is in use",
        "another instance",
        "all instances",      // "close all instances of it"
        "setup has detected", // Inno Setup: "Setup has detected that … is currently running"
        // Danish (winget and installer logs use system locale)
        "kørende",
        "luk programmet",
        "er i brug",
        "er åben",
        "lukke programmet",
        "kører i øjeblikket", // "is currently running" in Danish
        "luk alle forekomster", // "close all instances" in Danish
    ];
    keywords.iter().any(|kw| lower.contains(kw))
}

/// Strip ANSI escape codes so terminal colors don't pollute the log.
pub(crate) fn strip_ansi(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for ch in chars.by_ref() {
                    if ch.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Returns true if a log line is worth showing in the UI.
///
/// winget writes spinner frames (\|/-) and block-character progress bars to stdout
/// separated by \r.  After we take the last \r segment we still may get pure block-
/// char lines (e.g. "████████░░░░") or lone spinner chars.  We keep a line only if
/// it contains at least one ASCII letter — this passes "Downloading…", "1024 KB /
/// 83.2 MB", "Successfully installed", etc., while silently dropping the noise.
fn is_meaningful(line: &str) -> bool {
    line.chars().any(|c| c.is_ascii_alphabetic())
}

/// Emit one line from a raw winget/brew output line, applying the \r-frame and
/// noise filters.  Nothing is emitted if the line carries no useful information.
fn emit_line(handle: &tauri::AppHandle, raw: &str) {
    let clean = strip_ansi(raw);
    // winget overwrites the same terminal line with \r; grab the last frame.
    let last = clean.rsplit('\r').next().unwrap_or(clean.as_str());
    let trimmed = last.trim();
    if is_meaningful(trimmed) {
        handle.emit("install-output", trimmed).ok();
    }
}

/// Stream stdout + stderr from a child process back to the frontend.
/// Returns (success, captured_output, exit_code).
/// Does NOT emit install-complete — lets the caller decide whether to retry before finishing.
pub(crate) fn run_streamed_capture(
    app_handle: &tauri::AppHandle,
    mut child: std::process::Child,
) -> (bool, String, Option<i32>) {
    let captured = Arc::new(Mutex::new(String::new()));

    // stderr on a background thread so it doesn't block stdout
    let stderr_thread = child.stderr.take().map(|stderr| {
        let handle = app_handle.clone();
        let cap = Arc::clone(&captured);
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines().flatten() {
                emit_line(&handle, &line);
                let mut c = cap.lock().unwrap();
                c.push_str(&line);
                c.push('\n');
            }
        })
    });

    if let Some(stdout) = child.stdout.take() {
        for line in BufReader::new(stdout).lines().flatten() {
            emit_line(app_handle, &line);
            let mut c = captured.lock().unwrap();
            c.push_str(&line);
            c.push('\n');
        }
    }

    let status = child.wait();
    if let Some(t) = stderr_thread {
        let _ = t.join();
    }

    let (success, exit_code) = match &status {
        Ok(s) => (s.success(), s.code()),
        Err(_) => (false, None),
    };
    let output = captured.lock().unwrap().clone();
    (success, output, exit_code)
}

/// Stream a child process and emit install-complete when done.
/// Automatically classifies failures as "app-running" or "error".
pub(crate) fn run_streamed(app_handle: tauri::AppHandle, child: std::process::Child) {
    let (ok, output, code) = run_streamed_capture(&app_handle, child);
    let status = if ok {
        "success"
    } else if detect_app_running(&output, code) {
        "app-running"
    } else {
        "error"
    };
    app_handle.emit("install-complete", status).ok();
}

// ── Tauri commands ───────────────────────────────────────────────────────────

#[tauri::command]
fn search_packages(query: String) -> Result<Vec<serde_json::Value>, String> {
    pm::search_packages(query)
}

#[tauri::command]
fn install_package(app_handle: tauri::AppHandle, package: String, silent: bool) {
    pm::install_package(app_handle, package, silent);
}

#[tauri::command]
fn list_installed() -> Result<Vec<serde_json::Value>, String> {
    pm::list_installed()
}

#[tauri::command]
fn update_package(app_handle: tauri::AppHandle, package: String, silent: bool) {
    pm::update_package(app_handle, package, silent);
}

#[tauri::command]
fn update_all_packages(app_handle: tauri::AppHandle) {
    pm::update_all_packages(app_handle);
}

#[tauri::command]
fn uninstall_package(app_handle: tauri::AppHandle, package: String, silent: bool) {
    pm::uninstall_package(app_handle, package, silent);
}

#[tauri::command]
fn get_auto_update_prefs() -> Result<serde_json::Value, String> {
    let prefs = prefs::load_auto_update();
    serde_json::to_value(prefs).map_err(|e| e.to_string())
}

/// Setting enabled=false removes the entry entirely so the JSON doesn't
/// accumulate dead entries for uninstalled or deselected packages.
#[tauri::command]
fn set_auto_update_pref(package_id: String, enabled: bool) -> Result<(), String> {
    let mut prefs = prefs::load_auto_update();
    if enabled {
        prefs.insert(package_id, true);
    } else {
        prefs.remove(&package_id);
    }
    prefs::save_auto_update(&prefs)
}

/// Start a per-package update queue. Resets skip/abort state, then hands off
/// to the platform module which runs each package in sequence on a worker thread.
#[tauri::command]
fn update_all_packages_queued(
    app_handle: tauri::AppHandle,
    packages: Vec<String>,
    silent: bool,
    ctrl: tauri::State<'_, Arc<QueueControl>>,
) {
    let ctrl = Arc::clone(&ctrl);
    ctrl.reset();
    pm::update_all_packages_queued(app_handle, packages, silent, ctrl);
}

/// Mark a package as "to be skipped" in the running update queue.
#[tauri::command]
fn skip_package(pkg: String, ctrl: tauri::State<'_, Arc<QueueControl>>) {
    ctrl.add_skip(pkg);
}

/// Signal the running update queue to stop after the current package finishes.
#[tauri::command]
fn abort_update_all(ctrl: tauri::State<'_, Arc<QueueControl>>) {
    ctrl.set_abort();
}

/// Returns the persisted schedule config.  Windows only; macOS returns an error.
#[tauri::command]
fn get_schedule_config() -> Result<serde_json::Value, String> {
    #[cfg(not(target_os = "windows"))]
    return Err("Scheduled updates are only available on Windows".to_string());

    #[cfg(target_os = "windows")]
    {
        let config = schedule::load();
        serde_json::to_value(&config).map_err(|e| e.to_string())
    }
}

/// Persist schedule config and create/delete the Windows Scheduled Task accordingly.
#[tauri::command]
fn save_schedule_config(config: serde_json::Value) -> Result<(), String> {
    #[cfg(not(target_os = "windows"))]
    return Err("Scheduled updates are only available on Windows".to_string());

    #[cfg(target_os = "windows")]
    {
        let cfg: schedule::ScheduleConfig =
            serde_json::from_value(config).map_err(|e| e.to_string())?;
        schedule::save(&cfg)
    }
}

/// Run the auto-update logic immediately, streaming output to the UI log modal.
#[tauri::command]
fn run_auto_update_now(app_handle: tauri::AppHandle) {
    #[cfg(target_os = "windows")]
    headless::run_live(app_handle);

    #[cfg(not(target_os = "windows"))]
    {
        app_handle
            .emit("install-output", "Auto-updates are only available on Windows.")
            .ok();
        app_handle.emit("install-complete", "error").ok();
    }
}

/// Open the Brewinget logs folder in the OS file manager.
#[tauri::command]
fn open_logs_folder() -> Result<(), String> {
    #[cfg(not(target_os = "windows"))]
    return Err("Not supported on this platform".to_string());

    #[cfg(target_os = "windows")]
    schedule::open_logs_folder()
}

pub fn run_headless() {
    #[cfg(target_os = "windows")]
    headless::run();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(target_os = "windows")]
    mutex::acquire_ui_mutex();

    #[cfg(target_os = "windows")]
    schedule::startup_cleanup();

    tauri::Builder::default()
        .manage(Arc::new(QueueControl::new()))
        .invoke_handler(tauri::generate_handler![
            search_packages,
            install_package,
            list_installed,
            update_package,
            update_all_packages,
            update_all_packages_queued,
            skip_package,
            abort_update_all,
            uninstall_package,
            get_auto_update_prefs,
            set_auto_update_pref,
            get_schedule_config,
            save_schedule_config,
            run_auto_update_now,
            open_logs_folder,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
