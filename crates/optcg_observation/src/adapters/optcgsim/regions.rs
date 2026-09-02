use crate::capture::PixelRect;
use serde::{Deserialize, Serialize};

/// Normalized screen region (0.0–1.0) for OPTCGSim window capture.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NormalizedRegion {
    pub name: String,
    pub x: f32,
    pub y: f32,
    #[serde(alias = "w")]
    pub width: f32,
    #[serde(alias = "h")]
    pub height: f32,
}

impl NormalizedRegion {
    pub fn to_pixel_rect(&self, frame_w: u32, frame_h: u32) -> PixelRect {
        let x = (self.x * frame_w as f32).round().max(0.0) as u32;
        let y = (self.y * frame_h as f32).round().max(0.0) as u32;
        let width = (self.width * frame_w as f32).round().max(1.0) as u32;
        let height = (self.height * frame_h as f32).round().max(1.0) as u32;
        PixelRect {
            x,
            y,
            width,
            height,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionConfig {
    pub regions: Vec<NormalizedRegion>,
    pub calibrated: bool,
    #[serde(default)]
    pub profile_id: String,
}

impl Default for RegionConfig {
    fn default() -> Self {
        Self {
            regions: default_regions(),
            calibrated: false,
            profile_id: "optcgsim-default-16x9".into(),
        }
    }
}

pub fn default_regions() -> Vec<NormalizedRegion> {
    vec![
        region("self_leader", 0.35, 0.72, 0.12, 0.18),
        region("opponent_leader", 0.35, 0.08, 0.12, 0.18),
        region("self_life", 0.02, 0.55, 0.08, 0.25),
        region("opponent_life", 0.02, 0.18, 0.08, 0.25),
        region("self_don", 0.78, 0.55, 0.18, 0.35),
        region("opponent_don", 0.78, 0.08, 0.18, 0.35),
        region("self_board", 0.18, 0.55, 0.55, 0.22),
        region("opponent_board", 0.18, 0.28, 0.55, 0.22),
        region("phase_turn", 0.40, 0.46, 0.20, 0.06),
        region("combat_area", 0.30, 0.42, 0.40, 0.12),
    ]
}

fn region(name: &str, x: f32, y: f32, width: f32, height: f32) -> NormalizedRegion {
    NormalizedRegion {
        name: name.into(),
        x,
        y,
        width,
        height,
    }
}

#[cfg(test)]
mod region_tests {
    use super::*;

    #[test]
    fn normalized_to_pixels_scales_with_frame() {
        let r = NormalizedRegion {
            name: "test".into(),
            x: 0.5,
            y: 0.5,
            width: 0.1,
            height: 0.1,
        };
        let px = r.to_pixel_rect(1920, 1080);
        assert_eq!(px.x, 960);
        assert_eq!(px.y, 540);
        assert_eq!(px.width, 192);
        assert_eq!(px.height, 108);
    }

    #[test]
    fn dpi_scaling_uses_normalized_coords() {
        let r = region("self_life", 0.02, 0.55, 0.08, 0.25);
        let hd = r.to_pixel_rect(1280, 720);
        let fhd = r.to_pixel_rect(1920, 1080);
        assert!(hd.width > 0 && fhd.width > hd.width);
    }
}
