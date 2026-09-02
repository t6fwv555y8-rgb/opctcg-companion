use crate::adapters::optcgsim::regions::{default_regions, RegionConfig};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Persisted calibration profile for OPTCGSim visual regions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalibrationProfile {
    pub id: String,
    pub simulator: String,
    pub aspect_ratio: String,
    pub ui_scale: f32,
    pub regions: RegionConfig,
}

impl CalibrationProfile {
    pub fn default_optcgsim() -> Self {
        Self {
            id: "optcgsim-default-16x9".into(),
            simulator: "optcgsim".into(),
            aspect_ratio: "16:9".into(),
            ui_scale: 1.0,
            regions: RegionConfig {
                regions: default_regions(),
                calibrated: false,
                profile_id: "optcgsim-default-16x9".into(),
            },
        }
    }

    pub fn aspect_key(width: u32, height: u32) -> String {
        if height == 0 {
            return "16:9".into();
        }
        let ratio = width as f32 / height as f32;
        if (ratio - 16.0 / 9.0).abs() < 0.05 {
            "16:9".into()
        } else if (ratio - 16.0 / 10.0).abs() < 0.05 {
            "16:10".into()
        } else {
            format!("{ratio:.2}")
        }
    }
}

pub fn calibration_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("optcg-companion")
        .join("calibration")
}

pub fn save_profile(profile: &CalibrationProfile) -> Result<(), String> {
    let dir = calibration_dir();
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{}.json", profile.id));
    let json = serde_json::to_string_pretty(profile).map_err(|e| e.to_string())?;
    fs::write(path, json).map_err(|e| e.to_string())
}

pub fn load_profile(id: &str) -> Option<CalibrationProfile> {
    let path = calibration_dir().join(format!("{id}.json"));
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn load_or_default(window_w: u32, window_h: u32) -> CalibrationProfile {
    let aspect = CalibrationProfile::aspect_key(window_w, window_h);
    let id = format!("optcgsim-default-{aspect}");
    if let Some(mut p) = load_profile(&id) {
        p.regions.profile_id = id;
        return p;
    }
    if let Some(custom) = load_profile("optcgsim-user-custom") {
        return custom;
    }
    CalibrationProfile::default_optcgsim()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aspect_key_detects_16x9() {
        assert_eq!(CalibrationProfile::aspect_key(1920, 1080), "16:9");
        assert_eq!(CalibrationProfile::aspect_key(1280, 720), "16:9");
    }
}
