use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::regions::RegionConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ObservationMode {
    StructuredLog,
    VisualFallback,
    ReplayOnly,
    #[default]
    Unavailable,
}

impl ObservationMode {
    pub fn label(&self) -> String {
        match self {
            Self::StructuredLog => "OPTCGSim · LIVE — STRUCTURED".into(),
            Self::VisualFallback => "OPTCGSim · LIVE — VISUAL".into(),
            Self::ReplayOnly => "OPTCGSim · REPLAY LOGS ONLY".into(),
            Self::Unavailable => "OPTCGSim · NOT DETECTED".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CombatLogDiscovery {
    pub path: Option<PathBuf>,
    pub format: Option<String>,
    pub live_capable: bool,
    pub notes: String,
}

impl Default for CombatLogDiscovery {
    fn default() -> Self {
        Self {
            path: None,
            format: None,
            live_capable: false,
            notes: "Not probed".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OptcgSimStatus {
    pub process_detected: bool,
    pub installation: Option<super::detector::DetectedInstallation>,
    pub combat_logs: CombatLogDiscovery,
    pub mode: ObservationMode,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptcgSimConfig {
    pub custom_install_paths: Vec<PathBuf>,
    pub custom_log_paths: Vec<PathBuf>,
    pub vision_regions: RegionConfig,
    pub card_art_cache_dir: Option<PathBuf>,
}

impl Default for OptcgSimConfig {
    fn default() -> Self {
        Self {
            custom_install_paths: Vec::new(),
            custom_log_paths: Vec::new(),
            vision_regions: RegionConfig::default(),
            card_art_cache_dir: None,
        }
    }
}
