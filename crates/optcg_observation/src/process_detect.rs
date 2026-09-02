use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Safe desktop process/window detection result.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DetectedApplication {
    pub process_name: String,
    pub process_id: Option<u32>,
    pub window_title: Option<String>,
    pub executable_path: Option<PathBuf>,
}

/// Known simulator process name hints (configurable, not hardcoded to one product).
pub const SIMULATOR_PROCESS_HINTS: &[&str] = &["optcg", "onepiece", "simulator", "tcg", "cardgame"];

/// Detect whether a configured simulator appears to be running.
pub fn detect_simulator_processes() -> Vec<DetectedApplication> {
    #[cfg(target_os = "windows")]
    {
        return detect_windows();
    }
    #[cfg(target_os = "linux")]
    {
        return detect_linux();
    }
    #[cfg(target_os = "macos")]
    {
        return detect_macos();
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    {
        Vec::new()
    }
}

#[cfg(target_os = "linux")]
fn detect_linux() -> Vec<DetectedApplication> {
    let mut found = Vec::new();
    if let Ok(entries) = std::fs::read_dir("/proc") {
        for entry in entries.flatten() {
            let Ok(name) = entry.file_name().into_string() else {
                continue;
            };
            if !name.chars().all(|c| c.is_ascii_digit()) {
                continue;
            }
            let cmdline_path = entry.path().join("cmdline");
            if let Ok(cmdline) = std::fs::read_to_string(&cmdline_path) {
                let lower = cmdline.to_ascii_lowercase();
                if SIMULATOR_PROCESS_HINTS.iter().any(|h| lower.contains(h)) {
                    found.push(DetectedApplication {
                        process_name: cmdline.split('\0').next().unwrap_or("unknown").into(),
                        process_id: name.parse().ok(),
                        window_title: None,
                        executable_path: None,
                    });
                }
            }
        }
    }
    found
}

#[cfg(target_os = "windows")]
fn detect_windows() -> Vec<DetectedApplication> {
    // Isolated Windows stub — full implementation uses Win32 APIs in future milestone.
    Vec::new()
}

#[cfg(target_os = "macos")]
fn detect_macos() -> Vec<DetectedApplication> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_does_not_panic() {
        let _ = detect_simulator_processes();
    }
}
