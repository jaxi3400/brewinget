use std::io::{BufRead, BufReader};
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

/// Stream stdout + stderr from a child process back to the frontend via Tauri events.
pub(crate) fn run_streamed(app_handle: tauri::AppHandle, mut child: std::process::Child) {
    // stderr is read on a background thread so it doesn't block stdout
    if let Some(stderr) = child.stderr.take() {
        let handle = app_handle.clone();
        std::thread::spawn(move || {
            BufReader::new(stderr).lines().flatten().for_each(|line| {
                let clean = strip_ansi(&line);
                if !clean.trim().is_empty() {
                    handle.emit("install-output", clean).ok();
                }
            });
        });
    }

    if let Some(stdout) = child.stdout.take() {
        BufReader::new(stdout).lines().flatten().for_each(|line| {
            let clean = strip_ansi(&line);
            if !clean.trim().is_empty() {
                app_handle.emit("install-output", clean).ok();
            }
        });
    }

    match child.wait() {
        Ok(status) if status.success() => {
            app_handle.emit("install-complete", "success").ok();
        }
        _ => {
            app_handle.emit("install-complete", "error").ok();
        }
    }
}

// ── Tauri commands ───────────────────────────────────────────────────────────

#[tauri::command]
fn search_packages(query: String) -> Result<Vec<serde_json::Value>, String> {
    pm::search_packages(query)
}

#[tauri::command]
fn install_package(app_handle: tauri::AppHandle, package: String) {
    pm::install_package(app_handle, package);
}

#[tauri::command]
fn list_installed() -> Result<Vec<serde_json::Value>, String> {
    pm::list_installed()
}

#[tauri::command]
fn update_package(app_handle: tauri::AppHandle, package: String) {
    pm::update_package(app_handle, package);
}

#[tauri::command]
fn update_all_packages(app_handle: tauri::AppHandle) {
    pm::update_all_packages(app_handle);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            search_packages,
            install_package,
            list_installed,
            update_package,
            update_all_packages,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
