use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use tauri::Emitter;

fn brew() -> &'static str {
    if std::path::Path::new("/opt/homebrew/bin/brew").exists() {
        "/opt/homebrew/bin/brew"
    } else {
        "/usr/local/bin/brew"
    }
}

#[tauri::command]
fn search_packages(query: String) -> Result<Vec<String>, String> {
    let output = Command::new(brew())
        .args(["search", "--formula", &query])
        .output()
        .map_err(|e| format!("Failed to run brew: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let packages: Vec<String> = stdout
        .lines()
        .filter(|l| !l.starts_with("==>") && !l.trim().is_empty())
        .flat_map(|l| l.split_whitespace())
        .map(|s| s.to_string())
        .take(24)
        .collect();

    Ok(packages)
}

#[tauri::command]
fn install_package(app_handle: tauri::AppHandle, package: String) {
    std::thread::spawn(move || {
        let mut child = match Command::new(brew())
            .args(["install", &package])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                app_handle
                    .emit("install-output", format!("Error: {}", e))
                    .ok();
                app_handle.emit("install-complete", "error").ok();
                return;
            }
        };

        // Stream stderr (brew writes progress there)
        if let Some(stderr) = child.stderr.take() {
            let handle = app_handle.clone();
            std::thread::spawn(move || {
                let reader = BufReader::new(stderr);
                for line in reader.lines().flatten() {
                    let clean = strip_ansi(&line);
                    if !clean.trim().is_empty() {
                        handle.emit("install-output", clean).ok();
                    }
                }
            });
        }

        // Stream stdout
        if let Some(stdout) = child.stdout.take() {
            let reader = BufReader::new(stdout);
            for line in reader.lines().flatten() {
                let clean = strip_ansi(&line);
                if !clean.trim().is_empty() {
                    app_handle.emit("install-output", clean).ok();
                }
            }
        }

        match child.wait() {
            Ok(status) if status.success() => {
                app_handle.emit("install-complete", "success").ok();
            }
            _ => {
                app_handle.emit("install-complete", "error").ok();
            }
        }
    });
}

#[tauri::command]
fn list_installed() -> Result<Vec<serde_json::Value>, String> {
    let installed_out = Command::new(brew())
        .args(["list", "--formula"])
        .output()
        .map_err(|e| e.to_string())?;

    let outdated_out = Command::new(brew())
        .args(["outdated", "--formula"])
        .output()
        .map_err(|e| e.to_string())?;

    let installed: Vec<String> = String::from_utf8_lossy(&installed_out.stdout)
        .lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let outdated: Vec<String> = String::from_utf8_lossy(&outdated_out.stdout)
        .lines()
        .filter_map(|l| l.split_whitespace().next().map(|s| s.to_string()))
        .collect();

    let packages = installed
        .into_iter()
        .map(|name| {
            let has_update = outdated.contains(&name);
            serde_json::json!({ "name": name, "hasUpdate": has_update })
        })
        .collect();

    Ok(packages)
}

#[tauri::command]
fn update_package(app_handle: tauri::AppHandle, package: String) {
    std::thread::spawn(move || {
        let mut child = match Command::new(brew())
            .args(["upgrade", &package])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                app_handle
                    .emit("install-output", format!("Error: {}", e))
                    .ok();
                app_handle.emit("install-complete", "error").ok();
                return;
            }
        };

        if let Some(stderr) = child.stderr.take() {
            let handle = app_handle.clone();
            std::thread::spawn(move || {
                let reader = BufReader::new(stderr);
                for line in reader.lines().flatten() {
                    let clean = strip_ansi(&line);
                    if !clean.trim().is_empty() {
                        handle.emit("install-output", clean).ok();
                    }
                }
            });
        }

        if let Some(stdout) = child.stdout.take() {
            let reader = BufReader::new(stdout);
            for line in reader.lines().flatten() {
                let clean = strip_ansi(&line);
                if !clean.trim().is_empty() {
                    app_handle.emit("install-output", clean).ok();
                }
            }
        }

        match child.wait() {
            Ok(status) if status.success() => {
                app_handle.emit("install-complete", "success").ok();
            }
            _ => {
                app_handle.emit("install-complete", "error").ok();
            }
        }
    });
}

// Remove ANSI escape codes so terminal colors don't pollute the log
fn strip_ansi(s: &str) -> String {
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            search_packages,
            install_package,
            list_installed,
            update_package
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
