use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};
use tauri::Emitter;

// Prevents a black console window from flashing when we spawn cmd.exe
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

// winget.exe is an "app execution alias" that doesn't work reliably when spawned
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

pub fn search_packages(query: String) -> Result<Vec<String>, String> {
    let output = winget(&["search", &query, "--accept-source-agreements"])
        .output()
        .map_err(|e| format!("Failed to run winget: {}", e))?;

    // Include stderr in the error so the UI shows a useful message on failure
    if !output.status.success() && output.stdout.is_empty() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("winget error: {}", err.trim()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(parse_table_ids(&stdout).into_iter().take(24).collect())
}

pub fn install_package(app_handle: tauri::AppHandle, package: String) {
    std::thread::spawn(move || {
        match winget(&[
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
    let all_out = winget(&["list", "--accept-source-agreements"])
        .output()
        .map_err(|e| e.to_string())?;

    let upgrade_out = winget(&["upgrade", "--accept-source-agreements"])
        .output()
        .map_err(|e| e.to_string())?;

    let all = parse_table_ids(&String::from_utf8_lossy(&all_out.stdout));
    let upgradeable: std::collections::HashSet<String> =
        parse_table_ids(&String::from_utf8_lossy(&upgrade_out.stdout))
            .into_iter()
            .collect();

    if all.is_empty() {
        let err = String::from_utf8_lossy(&all_out.stderr);
        if !err.trim().is_empty() {
            return Err(format!("winget error: {}", err.trim()));
        }
    }

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
        match winget(&[
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
        // winget uses \r to animate a progress spinner in-place. When stdout is
        // piped, all spinner frames land on the same \n-delimited line. Taking
        // the last \r-segment gives us the final visible content (header or data).
        let raw_line = raw_line.trim_start_matches('\u{feff}');
        let line = raw_line.rsplit('\r').next().unwrap_or(raw_line);
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
