use super::frame::CapturedFrame;
use crate::error::ObsResult;
use crate::window::GameWindowInfo;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::warn;

/// Configurable capture rate — defaults tuned for low CPU, responsive HUD.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CaptureConfig {
    pub idle_fps: f32,
    pub combat_fps: f32,
    pub channel_capacity: usize,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            idle_fps: 8.0,
            combat_fps: 15.0,
            channel_capacity: 1,
        }
    }
}

impl CaptureConfig {
    pub fn idle_interval(&self) -> Duration {
        Duration::from_secs_f32(1.0 / self.idle_fps.max(1.0))
    }

    pub fn combat_interval(&self) -> Duration {
        Duration::from_secs_f32(1.0 / self.combat_fps.max(1.0))
    }
}

/// Runtime capture statistics for debug panel.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CaptureStats {
    pub capture_fps: f32,
    pub recognition_fps: f32,
    pub frames_captured: u64,
    pub frames_dropped: u64,
    pub capture_failures: u64,
    pub last_capture_ms: u64,
}

/// Bounded latest-frame capture pipeline with backpressure.
pub struct CapturePipeline {
    config: CaptureConfig,
    stats: Arc<Mutex<CaptureStats>>,
    combat_mode: Arc<Mutex<bool>>,
}

impl CapturePipeline {
    pub fn new(config: CaptureConfig) -> Self {
        Self {
            config,
            stats: Arc::new(Mutex::new(CaptureStats::default())),
            combat_mode: Arc::new(Mutex::new(false)),
        }
    }

    pub fn stats(&self) -> CaptureStats {
        self.stats.lock().clone()
    }

    pub fn set_combat_mode(&self, active: bool) {
        *self.combat_mode.lock() = active;
    }

    pub fn capture_interval(&self) -> Duration {
        if *self.combat_mode.lock() {
            self.config.combat_interval()
        } else {
            self.config.idle_interval()
        }
    }

    /// Capture one frame; updates stats. Returns None on failure/unavailable.
    pub fn capture_once(&self, window: &GameWindowInfo) -> ObsResult<Option<CapturedFrame>> {
        let start = Instant::now();
        match super::capture_window_frame(window) {
            Ok(Some(frame)) => {
                let mut stats = self.stats.lock();
                stats.frames_captured += 1;
                stats.last_capture_ms = start.elapsed().as_millis() as u64;
                Ok(Some(frame))
            }
            Ok(None) => Ok(None),
            Err(e) => {
                self.stats.lock().capture_failures += 1;
                warn!(error = %e, "capture failed");
                Err(e)
            }
        }
    }

    pub fn record_drop(&self) {
        self.stats.lock().frames_dropped += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_intervals_are_bounded() {
        let cfg = CaptureConfig::default();
        assert!(cfg.idle_interval().as_millis() >= 60);
        assert!(cfg.combat_interval().as_millis() >= 30);
    }
}
