use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};
use tauri::Emitter;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

// winget.exe is an app execution alias that doesn't work reliably when spawned
// from a GUI process without a console. Routing through cmd /c is the fix.
fn winget(args: &[&str]) -> Command {
    let mut cmd = Command::new("cmd");
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd.arg("/c").arg("winget");
    for arg in args {
        cmd.arg(arg);
    }
    cmd
}

/// Spawn winget with base_args, optionally append silent flags, stream output.
/// Returns true if the process exited successfully. Does not emit install-complete.
fn run_winget_op(app_handle: &tauri::AppHandle, base_args: &[&str], silent: bool) -> bool {
    let mut cmd = winget(base_args);
    if silent {
        // --silent: request unattended install from the underlying installer.
        // --disable-interactivity: suppress winget's own interactive prompts.
        cmd.arg("--silent").arg("--disable-interactivity");
    }
    match cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn() {
        Ok(child) => crate::run_streamed_capture(app_handle, child),
        Err(e) => {
            app_handle.emit("install-output", format!("Error: {}", e)).ok();
            false
        }
    }
}

// ── Public commands ───────────────────────────────────────────────────────────

pub fn search_packages(query: String) -> Result<Vec<serde_json::Value>, String> {
    let output = winget(&["search", &query, "--accept-source-agreements"])
        .output()
        .map_err(|e| format!("Failed to run winget: {}", e))?;

    if !output.status.success() && output.stdout.is_empty() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("winget error: {}", err.trim()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let rows = parse_table(&stdout);

    let packages: Vec<serde_json::Value> = rows
        .into_iter()
        .take(30)
        .map(|r| {
            serde_json::json!({
                "name":    r.name,
                "id":      r.id,
                "version": r.version,
                "source":  r.source,
            })
        })
        .collect();

    Ok(packages)
}

pub fn install_package(app_handle: tauri::AppHandle, package: String, silent: bool) {
    std::thread::spawn(move || {
        let args = [
            "install", "--id", package.as_str(), "--exact",
            "--accept-package-agreements", "--accept-source-agreements",
        ];
        let mut ok = run_winget_op(&app_handle, &args, silent);
        if !ok && silent {
            // Some installers don't support --silent; retry interactively so the
            // user gets a working install rather than a silent failure.
            app_handle.emit(
                "install-output",
                "⚠  Silent install failed — retrying without --silent (an installer window may appear)…",
            ).ok();
            ok = run_winget_op(&app_handle, &args, false);
        }
        app_handle.emit("install-complete", if ok { "success" } else { "error" }).ok();
    });
}

pub fn list_installed() -> Result<Vec<serde_json::Value>, String> {
    let all_out = winget(&["list", "--accept-source-agreements"])
        .output()
        .map_err(|e| e.to_string())?;

    let upgrade_out = winget(&["upgrade", "--accept-source-agreements"])
        .output()
        .map_err(|e| e.to_string())?;

    let all = parse_table(&String::from_utf8_lossy(&all_out.stdout));

    if all.is_empty() {
        let err = String::from_utf8_lossy(&all_out.stderr);
        if !err.trim().is_empty() {
            return Err(format!("winget error: {}", err.trim()));
        }
    }

    // Map id -> available version from the upgrade output.
    let upgradeable: std::collections::HashMap<String, String> = parse_table(
        &String::from_utf8_lossy(&upgrade_out.stdout),
    )
    .into_iter()
    .filter(|r| !r.id.is_empty())
    .map(|r| (r.id, r.available))
    .collect();

    Ok(all
        .into_iter()
        .map(|r| {
            let available  = upgradeable.get(&r.id).cloned().unwrap_or_default();
            let has_update = !available.is_empty();
            // ARP entries are Windows Add/Remove Programs registry entries;
            // they can't be managed through winget so we flag them for filtering.
            let is_arp = r.id.starts_with("ARP\\");
            serde_json::json!({
                "name":      r.name,
                "id":        r.id,
                "version":   r.version,
                "available": available,
                "hasUpdate": has_update,
                "isArp":     is_arp,
            })
        })
        .collect())
}

pub fn update_all_packages_queued(
    app_handle: tauri::AppHandle,
    packages: Vec<String>,
    silent: bool,
    ctrl: std::sync::Arc<crate::QueueControl>,
) {
    std::thread::spawn(move || {
        let total = packages.len();
        let mut n_ok = 0usize;
        let mut n_err = 0usize;
        let mut n_skip = 0usize;

        for (i, pkg) in packages.iter().enumerate() {
            if ctrl.should_abort() {
                app_handle.emit("install-output", "⛔  Aborted — remaining packages skipped.").ok();
                n_skip += packages.len() - i;
                break;
            }

            if ctrl.should_skip(pkg) {
                app_handle.emit("pkg-done", crate::PkgDoneEvent {
                    name: pkg.clone(), status: "skipped".into(),
                }).ok();
                app_handle.emit("install-output", format!("⏭  Skipped: {pkg}")).ok();
                n_skip += 1;
                continue;
            }

            app_handle.emit("pkg-start", crate::PkgStartEvent {
                name: pkg.clone(), index: i + 1, total,
            }).ok();
            app_handle.emit("install-output",
                format!("\n── Updating {pkg} ({}/{total}) ──", i + 1),
            ).ok();

            let args = [
                "upgrade", "--id", pkg.as_str(), "--exact",
                "--accept-package-agreements", "--accept-source-agreements",
            ];
            let mut ok = run_winget_op(&app_handle, &args, silent);
            if !ok && silent {
                app_handle.emit("install-output",
                    "⚠  Silent update failed — retrying without --silent…",
                ).ok();
                ok = run_winget_op(&app_handle, &args, false);
            }

            if ok {
                n_ok += 1;
                app_handle.emit("install-output", format!("✓ {pkg} updated.")).ok();
            } else {
                n_err += 1;
                app_handle.emit("install-output", format!("✕ {pkg} update failed.")).ok();
            }
            app_handle.emit("pkg-done", crate::PkgDoneEvent {
                name: pkg.clone(),
                status: if ok { "success" } else { "error" }.into(),
            }).ok();
        }

        app_handle.emit("install-output",
            format!("\n── Done: {n_ok} updated, {n_err} failed, {n_skip} skipped ──"),
        ).ok();
        app_handle.emit("install-complete", "success").ok();
    });
}

pub fn update_all_packages(app_handle: tauri::AppHandle) {
    std::thread::spawn(move || {
        match winget(&[
            "upgrade",
            "--all",
            "--accept-package-agreements",
            "--accept-source-agreements",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        {
            Ok(child) => crate::run_streamed(app_handle, child),
            Err(e) => {
                app_handle
                    .emit("install-output", format!("Error: {}", e))
                    .ok();
                app_handle.emit("install-complete", "error").ok();
            }
        }
    });
}

pub fn update_package(app_handle: tauri::AppHandle, package: String, silent: bool) {
    std::thread::spawn(move || {
        let args = [
            "upgrade", "--id", package.as_str(), "--exact",
            "--accept-package-agreements", "--accept-source-agreements",
        ];
        let mut ok = run_winget_op(&app_handle, &args, silent);
        if !ok && silent {
            app_handle.emit(
                "install-output",
                "⚠  Silent update failed — retrying without --silent (an installer window may appear)…",
            ).ok();
            ok = run_winget_op(&app_handle, &args, false);
        }
        app_handle.emit("install-complete", if ok { "success" } else { "error" }).ok();
    });
}

// ── Table parser ──────────────────────────────────────────────────────────────

struct TableRow {
    name:      String,
    id:        String,
    version:   String,
    available: String,
    source:    String,
}

/// Parse any winget fixed-width table (search / list / upgrade).
///
/// winget uses \r to animate a spinner in-place; when stdout is piped all
/// spinner frames land on the same \n-delimited line.  `rsplit('\r').next()`
/// gives us the last overwrite — the actual header or data row.
fn parse_table(output: &str) -> Vec<TableRow> {
    // Column start positions discovered from the header line.
    let mut col_name:      Option<usize> = None;
    let mut col_id:        Option<usize> = None;
    let mut col_version:   Option<usize> = None;
    let mut col_available: Option<usize> = None;
    let mut col_source:    Option<usize> = None;
    let mut past_separator = false;
    let mut rows = Vec::new();

    for raw in output.lines() {
        let raw = raw.trim_start_matches('\u{feff}');
        let line = raw.rsplit('\r').next().unwrap_or(raw);
        let trimmed = line.trim();

        if trimmed.is_empty() {
            continue;
        }

        if !past_separator {
            // Detect the header line by the presence of both "Id" and "Name"/"Version"
            if trimmed.contains("Id")
                && (trimmed.contains("Name") || trimmed.contains("Version"))
            {
                col_name      = line.find("Name");
                col_id        = line.find("Id");
                col_version   = line.find("Version");
                col_available = line.find("Available");
                col_source    = line.find("Source");
                continue;
            }
            if trimmed.len() > 5 && trimmed.chars().all(|c| c == '-') {
                past_separator = true;
                continue;
            }
            continue;
        }

        let Some(id_start) = col_id else { continue };
        let chars: Vec<char> = line.chars().collect();
        let len = chars.len();
        if id_start >= len {
            continue;
        }

        let name      = col_str(&chars, len, col_name.unwrap_or(0),      id_start);
        let id        = col_str(&chars, len, id_start,                   col_version.unwrap_or(len));
        let version   = col_str(&chars, len, col_version.unwrap_or(0),   col_available.or(col_source).unwrap_or(len));
        let available = col_str(&chars, len, col_available.unwrap_or(0), col_source.unwrap_or(len));
        let source    = col_str(&chars, len, col_source.unwrap_or(0),    len);

        if !id.is_empty() {
            rows.push(TableRow { name, id, version, available, source });
        }
    }

    rows
}

fn col_str(chars: &[char], total: usize, start: usize, end: usize) -> String {
    if start >= total {
        return String::new();
    }
    chars[start..end.min(total)]
        .iter()
        .collect::<String>()
        .trim()
        .to_string()
}

// ── UAC elevation for update-all ─────────────────────────────────────────────
//
// Security model: a single UAC prompt fires when "Update All" is clicked.
// After approval, a hidden PowerShell process runs all winget upgrades as
// admin and streams results back through a named pipe.
//
// Pipe names include the PID + a nanosecond timestamp to make them hard to
// guess.  FILE_FLAG_FIRST_PIPE_INSTANCE ensures a pre-created pipe with the
// same name causes CreateNamedPipeW to fail rather than connect to an
// attacker's pipe.  The NULL security descriptor grants access to the
// creating user's SID, which Windows preserves in the elevated token — so
// the elevated PowerShell can connect to the pipe created by the
// non-elevated Tauri process.
//
// Skip / abort signals are forwarded through a second ctrl pipe added in
// commit 5b.  Until then, the Skip / Abort buttons in the UI call the
// existing Tauri commands but have no effect on the elevated process.

use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE},
    System::{IO::OVERLAPPED, Pipes::{ConnectNamedPipe, CreateNamedPipeW}},
    UI::Shell::{ShellExecuteExW, SHELLEXECUTEINFOW},
};

// These constants have well-known numeric values; we define them rather than
// importing from windows-sys to avoid pulling in additional feature gates.
const PIPE_ACCESS_INBOUND: u32       = 1;
const FILE_FLAG_FIRST_PIPE_INSTANCE: u32 = 0x0008_0000;
const PIPE_TYPE_BYTE: u32            = 0;
const PIPE_READMODE_BYTE: u32        = 0;
const PIPE_WAIT: u32                 = 0;
const PIPE_REJECT_REMOTE_CLIENTS: u32 = 8;
const ERROR_PIPE_CONNECTED: u32      = 535;
const SEE_MASK_NOCLOSEPROCESS: u32   = 0x0000_0040;
const SEE_MASK_FLAG_NO_UI: u32       = 0x0000_0400;
const ERROR_CANCELLED: u32           = 1223;

/// RAII wrapper: closes the Windows HANDLE on drop.
struct OwnedHandle(HANDLE);
impl Drop for OwnedHandle {
    fn drop(&mut self) {
        if self.0 != INVALID_HANDLE_VALUE && self.0 != 0 {
            unsafe { CloseHandle(self.0); }
        }
    }
}
impl OwnedHandle {
    /// Transfer HANDLE ownership to a `std::fs::File` so Drop won't double-close.
    fn into_file(self) -> std::fs::File {
        use std::os::windows::io::FromRawHandle;
        let h = self.0;
        std::mem::forget(self);
        unsafe { std::fs::File::from_raw_handle(h as _) }
    }
}

pub fn update_all_elevated(
    app_handle: tauri::AppHandle,
    packages: Vec<String>,
    silent: bool,
    _ctrl: std::sync::Arc<crate::QueueControl>, // skip/abort wired in commit 5b
) {
    std::thread::spawn(move || {
        if let Err(e) = do_elevated_update(&app_handle, &packages, silent) {
            app_handle.emit("install-output", format!("✕ {e}")).ok();
            app_handle.emit("install-complete", "error").ok();
        }
    });
}

fn do_elevated_update(
    app_handle: &tauri::AppHandle,
    packages: &[String],
    silent: bool,
) -> Result<(), String> {
    use std::io::BufRead;

    let pid = std::process::id();
    let t   = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let suffix       = format!("{:08x}-{:016x}", pid, t);
    let out_pipe_path = format!(r"\\.\pipe\brewinget-out-{}", suffix);
    let out_pipe_key  = format!("brewinget-out-{}", suffix); // PS uses just the name, not the path

    let out_handle = OwnedHandle(create_pipe(&out_pipe_path)?);

    let script_path = std::env::temp_dir().join(format!("brewinget-{}.ps1", pid));
    let script      = build_ps_script(&out_pipe_key, packages, silent);
    std::fs::write(&script_path, script.as_bytes())
        .map_err(|e| format!("Failed to write update script: {e}"))?;

    app_handle.emit("install-output", "Requesting administrator permission…").ok();

    // ShellExecuteExW fires the UAC prompt here.  Returns Err if user cancels.
    let proc_handle = OwnedHandle(launch_elevated(&script_path.to_string_lossy())?);
    drop(proc_handle); // process stays alive; we track completion via the pipe

    app_handle.emit("install-output", "Elevated process started…").ok();

    // Block until the elevated PowerShell connects (happens within seconds of start).
    // Limitation: if the elevated process crashes before connecting (e.g. script not
    // found), ConnectNamedPipe blocks indefinitely.  A timeout will be added in 5c.
    let r = unsafe { ConnectNamedPipe(out_handle.0, std::ptr::null_mut::<OVERLAPPED>()) };
    if r == 0 {
        let code = std::io::Error::last_os_error().raw_os_error().unwrap_or(0) as u32;
        if code != ERROR_PIPE_CONNECTED {
            let _ = std::fs::remove_file(&script_path);
            return Err(format!("Pipe connection failed (OS error {})", code));
        }
    }

    // Hand the HANDLE to BufReader; reads lines that the elevated PS writes.
    let reader = std::io::BufReader::new(out_handle.into_file());

    let mut n_ok   = 0usize;
    let mut n_err  = 0usize;
    let mut n_skip = 0usize;

    for raw in reader.lines() {
        let line = match raw {
            Err(_) => break,
            Ok(l) => l.trim_end_matches('\r').to_string(),
        };

        if let Some(rest) = line.strip_prefix("PKG_START\t") {
            let p: Vec<&str> = rest.splitn(3, '\t').collect();
            if p.len() == 3 {
                let idx:   usize = p[1].parse().unwrap_or(0);
                let total: usize = p[2].parse().unwrap_or(0);
                app_handle.emit("pkg-start", crate::PkgStartEvent {
                    name: p[0].to_string(), index: idx, total,
                }).ok();
                app_handle.emit("install-output",
                    format!("\n── Updating {} ({}/{}) ──", p[0], idx, total),
                ).ok();
            }
        } else if let Some(rest) = line.strip_prefix("PKG_DONE\t") {
            let p: Vec<&str> = rest.splitn(2, '\t').collect();
            if p.len() == 2 {
                let (name, status) = (p[0], p[1]);
                if status == "success" {
                    n_ok += 1;
                    app_handle.emit("install-output", format!("✓ {name} updated.")).ok();
                } else {
                    n_err += 1;
                    app_handle.emit("install-output", format!("✕ {name} update failed.")).ok();
                }
                app_handle.emit("pkg-done", crate::PkgDoneEvent {
                    name: name.to_string(), status: status.to_string(),
                }).ok();
            }
        } else if let Some(rest) = line.strip_prefix("ALL_DONE\t") {
            let p: Vec<&str> = rest.splitn(3, '\t').collect();
            if p.len() == 3 {
                n_ok   = p[0].parse().unwrap_or(n_ok);
                n_err  = p[1].parse().unwrap_or(n_err);
                n_skip = p[2].parse().unwrap_or(n_skip);
            }
            break;
        } else if let Some(rest) = line.strip_prefix("LOG\t") {
            crate::emit_line(app_handle, rest);
        }
    }

    let _ = std::fs::remove_file(&script_path);

    app_handle.emit("install-output",
        format!("\n── Done: {n_ok} updated, {n_err} failed, {n_skip} skipped ──"),
    ).ok();
    app_handle.emit("install-complete", "success").ok();

    Ok(())
}

fn create_pipe(name: &str) -> Result<HANDLE, String> {
    let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    let h = unsafe {
        CreateNamedPipeW(
            wide.as_ptr(),
            PIPE_ACCESS_INBOUND | FILE_FLAG_FIRST_PIPE_INSTANCE,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            1,      // max instances
            0,      // out buffer (we only read)
            65536,  // in buffer
            0,      // default timeout
            std::ptr::null(),
        )
    };
    if h == INVALID_HANDLE_VALUE || h == 0 {
        Err(format!(
            "CreateNamedPipeW failed (OS error {})",
            std::io::Error::last_os_error().raw_os_error().unwrap_or(0),
        ))
    } else {
        Ok(h)
    }
}

fn launch_elevated(script_path: &str) -> Result<HANDLE, String> {
    let verb   = to_wide("runas");
    let file   = to_wide("powershell.exe");
    // -WindowStyle Hidden keeps the console invisible after UAC approves.
    // -ExecutionPolicy Bypass allows running unsigned scripts from %TEMP%.
    let params = to_wide(&format!(
        r#"-NonInteractive -ExecutionPolicy Bypass -WindowStyle Hidden -File "{}""#,
        script_path,
    ));

    let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    info.cbSize       = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    info.fMask        = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_FLAG_NO_UI;
    info.lpVerb       = verb.as_ptr();
    info.lpFile       = file.as_ptr();
    info.lpParameters = params.as_ptr();
    info.nShow        = 0; // SW_HIDE

    let ok = unsafe { ShellExecuteExW(&mut info) };
    if ok == 0 {
        let code = std::io::Error::last_os_error().raw_os_error().unwrap_or(0) as u32;
        return Err(if code == ERROR_CANCELLED {
            "Administrator permission was denied. Update cancelled.".to_string()
        } else {
            format!("Could not launch elevated process (OS error {})", code)
        });
    }

    Ok(info.hProcess)
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn build_ps_script(out_pipe_key: &str, packages: &[String], silent: bool) -> String {
    // Embed the package list as a JSON literal inside a PS single-quoted string.
    // Single-quoted PS strings treat " literally, so standard JSON is safe here.
    // Package IDs are alphanumeric with dots/dashes and won't contain ' in practice.
    let packages_json = serde_json::to_string(packages).unwrap_or_else(|_| "[]".to_string());
    let silent_ps     = if silent { "$true" } else { "$false" };

    ELEVATED_SCRIPT
        .replace("BREWINGET_OUT_PIPE", out_pipe_key)
        .replace("BREWINGET_PACKAGES", &packages_json)
        .replace("BREWINGET_SILENT",   silent_ps)
}

// PowerShell script run elevated.  Placeholder tokens are substituted by build_ps_script.
// Uses ProcessStartInfo so winget output is captured and forwarded line-by-line.
const ELEVATED_SCRIPT: &str = r#"
$outPipeName = 'BREWINGET_OUT_PIPE'
$packages    = 'BREWINGET_PACKAGES' | ConvertFrom-Json
$isSilent    = BREWINGET_SILENT
$total       = $packages.Count
$nOk = 0; $nErr = 0

$pipe = New-Object System.IO.Pipes.NamedPipeClientStream('.', $outPipeName, [System.IO.Pipes.PipeDirection]::Out)
try { $pipe.Connect(15000) } catch { exit 1 }
$sw = New-Object System.IO.StreamWriter($pipe, [System.Text.Encoding]::UTF8)
$sw.AutoFlush = $true
$sw.NewLine   = "`n"

for ($i = 0; $i -lt $total; $i++) {
    $pkg = $packages[$i]
    $sw.WriteLine("PKG_START`t$pkg`t$($i+1)`t$total")

    $flags = '--accept-package-agreements --accept-source-agreements'
    if ($isSilent) { $flags += ' --silent --disable-interactivity' }

    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName               = 'cmd.exe'
    $psi.Arguments              = "/c winget upgrade --id `"$pkg`" --exact $flags"
    $psi.UseShellExecute        = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError  = $true
    $psi.CreateNoWindow         = $true

    try {
        $proc    = [System.Diagnostics.Process]::Start($psi)
        $outTask = $proc.StandardOutput.ReadToEndAsync()
        $errTask = $proc.StandardError.ReadToEndAsync()
        $proc.WaitForExit()
        foreach ($ln in (($outTask.Result + "`n" + $errTask.Result) -split "`n")) {
            $ln = $ln.TrimEnd("`r")
            if ($ln.Trim()) { $sw.WriteLine("LOG`t$ln") }
        }
        $ok = ($proc.ExitCode -eq 0)
    } catch {
        $sw.WriteLine("LOG`tFailed to run winget: $_")
        $ok = $false
    }

    if ($ok) { $nOk++ } else { $nErr++ }
    $sw.WriteLine("PKG_DONE`t$pkg`t$(if ($ok) {'success'} else {'error'})")
}

$sw.WriteLine("ALL_DONE`t$nOk`t$nErr`t0")
$sw.Flush()
try { $pipe.WaitForPipeDrain() } catch {}
$pipe.Close()
"#;
