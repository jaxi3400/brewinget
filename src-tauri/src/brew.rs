use std::process::{Command, Stdio};
use tauri::Emitter;

fn exe() -> &'static str {
    if std::path::Path::new("/opt/homebrew/bin/brew").exists() {
        "/opt/homebrew/bin/brew"
    } else {
        "/usr/local/bin/brew"
    }
}

pub fn search_packages(query: String) -> Result<Vec<serde_json::Value>, String> {
    let output = Command::new(exe())
        .args(["search", "--formula", &query])
        .output()
        .map_err(|e| format!("Failed to run brew: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let packages = stdout
        .lines()
        .filter(|l| !l.starts_with("==>") && !l.trim().is_empty())
        .flat_map(|l| l.split_whitespace())
        .take(24)
        .map(|name| serde_json::json!({
            "name":    name,
            "id":      name,
            "version": "",
            "source":  "brew",
        }))
        .collect();

    Ok(packages)
}

pub fn install_package(app_handle: tauri::AppHandle, package: String, silent: bool) {
    std::thread::spawn(move || {
        let mut cmd = Command::new(exe());
        cmd.arg("install").arg(&package);
        // brew --quiet reduces output verbosity; no interactive/silent distinction needed.
        if silent { cmd.arg("--quiet"); }
        match cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn() {
            Ok(child) => crate::run_streamed(app_handle, child),
            Err(e) => {
                app_handle.emit("install-output", format!("Error: {}", e)).ok();
                app_handle.emit("install-complete", "error").ok();
            }
        }
    });
}

pub fn list_installed() -> Result<Vec<serde_json::Value>, String> {
    let installed_out = Command::new(exe())
        .args(["list", "--formula"])
        .output()
        .map_err(|e| e.to_string())?;

    let outdated_out = Command::new(exe())
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

    Ok(installed
        .into_iter()
        .map(|name| {
            let has_update = outdated.contains(&name);
            serde_json::json!({
                "name":      name,
                "id":        name,
                "version":   "",
                "available": "",
                "hasUpdate": has_update,
                "isArp":     false,
            })
        })
        .collect())
}

pub fn update_package(app_handle: tauri::AppHandle, package: String, silent: bool) {
    std::thread::spawn(move || {
        let mut cmd = Command::new(exe());
        cmd.arg("upgrade").arg(&package);
        if silent { cmd.arg("--quiet"); }
        match cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn() {
            Ok(child) => crate::run_streamed(app_handle, child),
            Err(e) => {
                app_handle.emit("install-output", format!("Error: {}", e)).ok();
                app_handle.emit("install-complete", "error").ok();
            }
        }
    });
}

pub fn update_all_packages(app_handle: tauri::AppHandle) {
    std::thread::spawn(move || {
        match Command::new(exe())
            .args(["upgrade"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(child) => crate::run_streamed(app_handle, child),
            Err(e) => {
                app_handle.emit("install-output", format!("Error: {}", e)).ok();
                app_handle.emit("install-complete", "error").ok();
            }
        }
    });
}
