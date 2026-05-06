use std::collections::HashMap;
use std::path::PathBuf;

#[cfg(target_os = "windows")]
fn data_dir() -> Result<PathBuf, String> {
    let base = std::env::var("LOCALAPPDATA")
        .map_err(|_| "LOCALAPPDATA not set".to_string())?;
    Ok(PathBuf::from(base).join("Brewinget"))
}

#[cfg(target_os = "macos")]
fn data_dir() -> Result<PathBuf, String> {
    let home = std::env::var("HOME")
        .map_err(|_| "HOME not set".to_string())?;
    Ok(PathBuf::from(home).join("Library/Application Support/Brewinget"))
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn data_dir() -> Result<PathBuf, String> {
    Err("unsupported platform".to_string())
}

fn auto_update_path() -> Result<PathBuf, String> {
    Ok(data_dir()?.join("auto-update.json"))
}

/// Load auto-update prefs from disk.
/// Returns an empty map if the file is missing or unreadable — safe to call cold.
pub fn load_auto_update() -> HashMap<String, bool> {
    let path = match auto_update_path() {
        Ok(p) => p,
        Err(_) => return HashMap::new(),
    };
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(_) => return HashMap::new(),
    };
    serde_json::from_slice(&bytes).unwrap_or_default()
}

/// Persist auto-update prefs to disk.
/// Creates %LOCALAPPDATA%\Brewinget\ (or the macOS equivalent) if it doesn't exist.
pub fn save_auto_update(prefs: &HashMap<String, bool>) -> Result<(), String> {
    let path = auto_update_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Cannot create data directory: {}", e))?;
    }
    let json = serde_json::to_string_pretty(prefs)
        .map_err(|e| format!("JSON error: {}", e))?;
    std::fs::write(&path, json)
        .map_err(|e| format!("Cannot write prefs: {}", e))
}
