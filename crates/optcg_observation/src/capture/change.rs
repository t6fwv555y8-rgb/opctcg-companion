use super::frame::{CapturedFrame, PixelRect};
use crate::adapters::optcgsim::regions::NormalizedRegion;
use std::collections::HashMap;

/// Lightweight per-region change detection via downscaled average hash.
#[derive(Debug, Default)]
pub struct ChangeDetector {
    last_hashes: HashMap<String, u64>,
}

#[derive(Debug, Clone)]
pub struct RegionChange {
    pub name: String,
    pub changed: bool,
    pub hash: u64,
}

impl ChangeDetector {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn detect_changes(
        &mut self,
        frame: &CapturedFrame,
        regions: &[NormalizedRegion],
    ) -> Vec<RegionChange> {
        let mut out = Vec::new();
        for region in regions {
            let rect = region.to_pixel_rect(frame.width, frame.height);
            let hash = region_hash(frame, &rect);
            let prev = self.last_hashes.get(&region.name).copied();
            let changed = prev != Some(hash);
            if changed {
                self.last_hashes.insert(region.name.clone(), hash);
            }
            out.push(RegionChange {
                name: region.name.clone(),
                changed,
                hash,
            });
        }
        out
    }

    pub fn frame_changed(&mut self, frame: &CapturedFrame) -> bool {
        let hash = whole_frame_hash(frame);
        let prev = self.last_hashes.get("__frame__").copied();
        if prev == Some(hash) {
            return false;
        }
        self.last_hashes.insert("__frame__".into(), hash);
        true
    }
}

fn region_hash(frame: &CapturedFrame, rect: &PixelRect) -> u64 {
    let crop = frame.crop_bgra(*rect);
    if crop.is_empty() {
        return 0;
    }
    let step = (crop.len() / 64).max(4);
    let mut hash: u64 = 0;
    for (i, chunk) in crop.chunks(step).take(64).enumerate() {
        let sum: u32 = chunk.iter().map(|&b| b as u32).sum();
        let bit = (sum % 256) > 128;
        if bit {
            hash |= 1u64 << (i % 64);
        }
    }
    hash
}

fn whole_frame_hash(frame: &CapturedFrame) -> u64 {
    let step = (frame.pixels.len() / 128).max(4);
    let mut hash: u64 = 0;
    for (i, &b) in frame.pixels.data.iter().step_by(step).take(128).enumerate() {
        if b > 128 {
            hash |= 1u64 << (i % 64);
        }
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capture::FrameBuffer;
    use crate::window::GameWindowInfo;
    use std::time::Instant;

    fn test_frame(fill: u8) -> CapturedFrame {
        CapturedFrame {
            timestamp: Instant::now(),
            width: 100,
            height: 100,
            stride: 400,
            pixels: FrameBuffer {
                data: vec![fill; 100 * 100 * 4],
            },
            window: GameWindowInfo {
                process_id: 1,
                title: "test".into(),
                x: 0,
                y: 0,
                width: 100,
                height: 100,
                minimized: false,
                visible: true,
                monitor_scale: 1.0,
                hwnd: 0,
            },
        }
    }

    #[test]
    fn suppresses_unchanged_frame() {
        let mut det = ChangeDetector::new();
        let f1 = test_frame(10);
        assert!(det.frame_changed(&f1));
        assert!(!det.frame_changed(&f1));
    }

    #[test]
    fn detects_region_change() {
        let mut det = ChangeDetector::new();
        let f1 = test_frame(10);
        let f2 = test_frame(200);
        let region = NormalizedRegion {
            name: "self_life".into(),
            x: 0.0,
            y: 0.0,
            width: 0.5,
            height: 0.5,
        };
        det.detect_changes(&f1, &[region.clone()]);
        let changes = det.detect_changes(&f2, &[region]);
        assert!(changes[0].changed);
    }
}
