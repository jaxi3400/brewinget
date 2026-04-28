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
