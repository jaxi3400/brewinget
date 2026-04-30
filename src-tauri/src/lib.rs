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

/// Stream stdout + stderr from a child process back to the frontend. Returns true on success.
/// Does NOT emit install-complete — lets the caller decide whether to retry before finishing.
pub(crate) fn run_streamed_capture(app_handle: &tauri::AppHandle, mut child: std::process::Child) -> bool {
    // stderr on a background thread so it doesn't block stdout
    if let Some(stderr) = child.stderr.take() {
        let handle = app_handle.clone();
        std::thread::spawn(move || {
            BufReader::new(stderr).lines().flatten()
                .for_each(|line| emit_line(&handle, &line));
        });
    }

    if let Some(stdout) = child.stdout.take() {
        BufReader::new(stdout).lines().flatten()
            .for_each(|line| emit_line(app_handle, &line));
    }

    matches!(child.wait(), Ok(s) if s.success())
}

/// Stream a child process and emit install-complete when done.
pub(crate) fn run_streamed(app_handle: tauri::AppHandle, child: std::process::Child) {
    let ok = run_streamed_capture(&app_handle, child);
    app_handle.emit("install-complete", if ok { "success" } else { "error" }).ok();
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
