use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use super::config::CombatLogDiscovery;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedInstallation {
    pub executable: Option<PathBuf>,
    pub data_dir: Option<PathBuf>,
    pub streaming_assets: Option<PathBuf>,
    pub decks_dir: Option<PathBuf>,
}

/// Discover OPTCGSim installation from configured + standard paths.
pub fn discover_installation(
    config: &super::config::OptcgSimConfig,
) -> Option<DetectedInstallation> {
    for path in &config.custom_install_paths {
        if let Some(install) = probe_install_root(path) {
            return Some(install);
        }
    }

    for candidate in standard_install_candidates() {
        if let Some(install) = probe_install_root(&candidate) {
            return Some(install);
        }
    }

    None
}

fn probe_install_root(root: &Path) -> Option<DetectedInstallation> {
    if !root.exists() {
        return None;
    }

    let exe = ["OPTCGSim.exe", "OPTCGSim", "optcgsim"]
        .iter()
        .map(|name| root.join(name))
        .find(|p| p.exists());

    let data = root.join("OPTCGSim_Data");
    let streaming = data.join("StreamingAssets");
    let decks = streaming.join("Decks");

    if exe.is_some() || data.is_dir() {
        Some(DetectedInstallation {
            executable: exe,
            data_dir: data.is_dir().then_some(data.clone()),
            streaming_assets: streaming.is_dir().then_some(streaming),
            decks_dir: decks.is_dir().then_some(decks),
        })
    } else {
        None
    }
}

fn standard_install_candidates() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(home) = dirs::home_dir() {
        paths.push(home.join("OPTCGSim"));
        paths.push(home.join("Games").join("OPTCGSim"));
        paths.push(home.join(".local/share/OPTCGSim"));
        #[cfg(target_os = "linux")]
        paths.push(home.join(".wine/drive_c/OPTCGSim"));
    }
    paths
}

/// Probe Unity LocalLow / AppData for OPTCGSim user data (read-only, bounded).
pub fn discover_user_data_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        if let Some(local) = dirs::data_local_dir() {
            let low = local.join("Low");
            return find_optcgsim_dir(&low, 2);
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = dirs::home_dir() {
            let support = home.join("Library/Application Support");
            return find_optcgsim_dir(&support, 2);
        }
    }
    #[cfg(target_os = "linux")]
    {
        if let Some(home) = dirs::home_dir() {
            let low = home.join(".config/unity3d");
            return find_optcgsim_dir(&low, 3);
        }
    }
    None
}

fn find_optcgsim_dir(root: &Path, max_depth: u32) -> Option<PathBuf> {
    if max_depth == 0 {
        return None;
    }
    if !root.is_dir() {
        return None;
    }
    let name = root.file_name()?.to_string_lossy().to_lowercase();
    if name.contains("optcg") || name.contains("batsu") {
        return Some(root.to_path_buf());
    }
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten().take(64) {
            if entry.path().is_dir() {
                if let Some(found) = find_optcgsim_dir(&entry.path(), max_depth - 1) {
                    return Some(found);
                }
            }
        }
    }
    None
}

/// Discover CombatLogs directory — logs are typically post-game per community tools.
pub fn discover_combat_logs(install: &Option<DetectedInstallation>) -> CombatLogDiscovery {
    let mut candidates: Vec<PathBuf> = Vec::new();

    if let Some(user) = discover_user_data_dir() {
        candidates.push(user.join("CombatLogs"));
    }
    if let Some(install) = install {
        if let Some(data) = &install.data_dir {
            candidates.push(data.join("CombatLogs"));
        }
    }

    for path in candidates {
        if path.is_dir() {
            let format = detect_log_format(&path);
            return CombatLogDiscovery {
                path: Some(path),
                format: Some(format.clone()),
                // Community evidence: logs written after match, not live during play
                live_capable: false,
                notes: "CombatLogs found — post-game replay/regression use; live play uses visual fallback".into(),
            };
        }
    }

    CombatLogDiscovery {
        notes: "No CombatLogs directory found".into(),
        ..Default::default()
    }
}

fn detect_log_format(dir: &Path) -> String {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten().take(20) {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if content.trim_start().starts_with('{') || content.contains("\"events\"") {
                        return "json_structured".into();
                    }
                }
                return "json".into();
            }
            if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                return "jsonl".into();
            }
            if path.extension().and_then(|e| e.to_str()) == Some("txt") {
                return "text".into();
            }
        }
    }
    "unknown".into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn discovers_install_with_data_dir() {
        let dir = tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("OPTCGSim_Data/StreamingAssets")).unwrap();
        let install = probe_install_root(dir.path()).unwrap();
        assert!(install.streaming_assets.is_some());
    }

    #[test]
    fn detects_json_combat_log_format() {
        let dir = tempdir().unwrap();
        let log_dir = dir.path().join("CombatLogs");
        std::fs::create_dir(&log_dir).unwrap();
        let mut f = std::fs::File::create(log_dir.join("match.json")).unwrap();
        writeln!(f, r#"{{"events":[]}}"#).unwrap();
        assert_eq!(detect_log_format(&log_dir), "json_structured");
    }
}
