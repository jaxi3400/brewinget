use std::process::{Command, Stdio};
use tauri::Emitter;

pub fn search_packages(query: String) -> Result<Vec<String>, String> {
    let output = Command::new("winget")
        .args(["search", &query, "--accept-source-agreements"])
        .output()
        .map_err(|e| format!("Failed to run winget: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(parse_table_ids(&stdout).into_iter().take(24).collect())
}

pub fn install_package(app_handle: tauri::AppHandle, package: String) {
    std::thread::spawn(move || {
        match Command::new("winget")
            .args([
                "install",
                "--id",
                &package,
                "--exact",
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

pub fn list_installed() -> Result<Vec<serde_json::Value>, String> {
    // All installed packages
    let all_out = Command::new("winget")
        .args(["list", "--accept-source-agreements"])
        .output()
        .map_err(|e| e.to_string())?;

    // Only packages that have an available upgrade (mirrors how brew does it)
    let upgrade_out = Command::new("winget")
        .args(["upgrade", "--accept-source-agreements"])
        .output()
        .map_err(|e| e.to_string())?;

    let all = parse_table_ids(&String::from_utf8_lossy(&all_out.stdout));
    let upgradeable: std::collections::HashSet<String> =
        parse_table_ids(&String::from_utf8_lossy(&upgrade_out.stdout))
            .into_iter()
            .collect();

    Ok(all
        .into_iter()
        .map(|id| {
            let has_update = upgradeable.contains(&id);
            serde_json::json!({ "name": id, "hasUpdate": has_update })
        })
        .collect())
}

pub fn update_package(app_handle: tauri::AppHandle, package: String) {
    std::thread::spawn(move || {
        match Command::new("winget")
            .args([
                "upgrade",
                "--id",
                &package,
                "--exact",
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

// Parse the fixed-width table that winget outputs for `search`, `list`, and `upgrade`.
// Finds the "Id" column by its header position and extracts that field from each data row.
fn parse_table_ids(output: &str) -> Vec<String> {
    let mut id_col: Option<usize> = None;
    let mut version_col: Option<usize> = None;
    let mut past_separator = false;
    let mut results = Vec::new();

    for raw_line in output.lines() {
        // winget sometimes emits a UTF-8 BOM on the first line
        let line = raw_line.trim_start_matches('\u{feff}');
        let trimmed = line.trim();

        if trimmed.is_empty() {
            continue;
        }

        if !past_separator {
            // The header line contains both "Id" and at least one of "Name" / "Version"
            if trimmed.contains("Id")
                && (trimmed.contains("Name") || trimmed.contains("Version"))
            {
                id_col = line.find("Id");
                version_col = line.find("Version");
                continue;
            }
            // The separator is a run of dashes (nothing else)
            if trimmed.len() > 5 && trimmed.chars().all(|c| c == '-') {
                past_separator = true;
                continue;
            }
            continue;
        }

        // Data row — slice the Id column out by character position
        if let Some(id_start) = id_col {
            let chars: Vec<char> = line.chars().collect();
            let len = chars.len();

            if id_start >= len {
                continue;
            }

            let id_end = version_col.unwrap_or(len).min(len);
            let id: String = chars[id_start..id_end]
                .iter()
                .collect::<String>()
                .trim()
                .to_string();

            if !id.is_empty() {
                results.push(id);
            }
        }
    }

    results
}
