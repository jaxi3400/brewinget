use std::path::PathBuf;
use std::process::Command;
use std::os::windows::process::CommandExt;
use serde::{Deserialize, Serialize};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const TASK_NAME: &str = "Brewinget Auto-Update";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ScheduleConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_frequency")]
    pub frequency: String, // "daily" | "weekly" | "custom"
    #[serde(default = "default_day")]
    pub day_of_week: u8, // 0=Sunday..6=Saturday, used for weekly
    #[serde(default = "default_hour")]
    pub hour: u8,
    #[serde(default)]
    pub minute: u8,
    #[serde(default = "default_interval")]
    pub interval_hours: u32,
}

fn default_frequency() -> String { "daily".to_string() }
fn default_day() -> u8 { 1 } // Monday
fn default_hour() -> u8 { 2 }
fn default_interval() -> u32 { 12 }

impl Default for ScheduleConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            frequency: default_frequency(),
            day_of_week: default_day(),
            hour: default_hour(),
            minute: 0,
            interval_hours: default_interval(),
        }
    }
}

fn schedule_path() -> Result<PathBuf, String> {
    let base = std::env::var("LOCALAPPDATA")
        .map_err(|_| "LOCALAPPDATA not set".to_string())?;
    Ok(PathBuf::from(base).join("Brewinget").join("schedule.json"))
}

pub fn load() -> ScheduleConfig {
    let path = match schedule_path() {
        Ok(p) => p,
        Err(_) => return ScheduleConfig::default(),
    };
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(_) => return ScheduleConfig::default(),
    };
    serde_json::from_slice(&bytes).unwrap_or_default()
}

pub fn save(config: &ScheduleConfig) -> Result<(), String> {
    let path = schedule_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Cannot create data directory: {}", e))?;
    }
    let json = serde_json::to_string_pretty(config)
        .map_err(|e| format!("JSON error: {}", e))?;
    std::fs::write(&path, json)
        .map_err(|e| format!("Cannot write schedule: {}", e))?;

    if config.enabled {
        create_task(config)
    } else {
        let _ = delete_task();
        Ok(())
    }
}

fn exe_path() -> String {
    std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "brewinget.exe".to_string())
}

fn create_task(config: &ScheduleConfig) -> Result<(), String> {
    // Escape single quotes in the exe path for PowerShell string literal
    let exe = exe_path().replace('\'', "''");
    let time_str = format!("{:02}:{:02}", config.hour, config.minute);

    let trigger_expr = match config.frequency.as_str() {
        "weekly" => {
            let day = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"]
                .get(config.day_of_week as usize)
                .copied()
                .unwrap_or("Monday");
            format!("New-ScheduledTaskTrigger -Weekly -DaysOfWeek {day} -At '{time_str}'")
        }
        "custom" => {
            let h = config.interval_hours.max(1);
            format!(
                "New-ScheduledTaskTrigger -Once -At (Get-Date) \
                 -RepetitionInterval (New-TimeSpan -Hours {h}) \
                 -RepetitionDuration (New-TimeSpan -Days 9999)"
            )
        }
        _ => format!("New-ScheduledTaskTrigger -Daily -At '{time_str}'"),
    };

    let script = format!(
        "$action   = New-ScheduledTaskAction -Execute '{exe}' -Argument '--auto-update'\n\
         $trigger  = {trigger_expr}\n\
         $settings = New-ScheduledTaskSettingsSet -ExecutionTimeLimit (New-TimeSpan -Hours 2) \
                       -MultipleInstances IgnoreNew -StartWhenAvailable\n\
         Register-ScheduledTask -TaskName '{TASK_NAME}' -Action $action -Trigger $trigger \
           -Settings $settings -RunLevel Highest -Force | Out-Null"
    );

    run_ps(&script)
}

fn delete_task() -> Result<(), String> {
    let script = format!(
        "if (Get-ScheduledTask -TaskName '{TASK_NAME}' -ErrorAction SilentlyContinue) {{\n    \
             Unregister-ScheduledTask -TaskName '{TASK_NAME}' -Confirm:$false\n\
         }}"
    );
    run_ps(&script)
}

pub fn open_logs_folder() -> Result<(), String> {
    let base = std::env::var("LOCALAPPDATA")
        .map_err(|_| "LOCALAPPDATA not set".to_string())?;
    let dir = PathBuf::from(base).join("Brewinget").join("logs");
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Cannot create logs dir: {}", e))?;
    // Explorer is a GUI app — don't suppress its window with CREATE_NO_WINDOW
    Command::new("explorer.exe")
        .arg(&dir)
        .spawn()
        .map_err(|e| format!("Cannot open Explorer: {}", e))?;
    Ok(())
}

fn run_ps(script: &str) -> Result<(), String> {
    let out = Command::new("powershell.exe")
        .creation_flags(CREATE_NO_WINDOW)
        .args(["-NonInteractive", "-NoProfile", "-Command", script])
        .output()
        .map_err(|e| format!("PowerShell spawn failed: {}", e))?;

    if out.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let stdout = String::from_utf8_lossy(&out.stdout);
        let msg = if !stderr.trim().is_empty() {
            stderr.trim().to_string()
        } else {
            stdout.trim().to_string()
        };
        Err(format!("Task Scheduler error: {}", msg))
    }
}
