use chrono::Utc;
use parking_lot::Mutex;
use std::time::Instant;

/// Tracks observation → state → analysis → HUD latencies.
#[derive(Debug, Default, Clone)]
pub struct LatencyTracker {
    inner: std::sync::Arc<Mutex<LatencySnapshot>>,
}

#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct LatencySnapshot {
    pub observation_latency_ms: u64,
    pub analysis_latency_ms: u64,
    pub total_latency_ms: u64,
    pub last_updated: Option<String>,
}

impl LatencyTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn begin_observation(&self) -> ObservationTimer {
        ObservationTimer {
            tracker: self.clone(),
            obs_start: Instant::now(),
            analysis_start: None,
        }
    }

    pub fn snapshot(&self) -> LatencySnapshot {
        self.inner.lock().clone()
    }

    fn record(&self, obs_ms: u64, analysis_ms: u64) {
        let mut snap = self.inner.lock();
        snap.observation_latency_ms = obs_ms;
        snap.analysis_latency_ms = analysis_ms;
        snap.total_latency_ms = obs_ms + analysis_ms;
        snap.last_updated = Some(Utc::now().to_rfc3339());
    }
}

pub struct ObservationTimer {
    tracker: LatencyTracker,
    obs_start: Instant,
    analysis_start: Option<Instant>,
}

impl ObservationTimer {
    pub fn mark_analysis_start(&mut self) {
        self.analysis_start = Some(Instant::now());
    }

    pub fn finish(self) {
        let obs_ms = self.obs_start.elapsed().as_millis() as u64;
        let analysis_ms = self
            .analysis_start
            .map(|t| t.elapsed().as_millis() as u64)
            .unwrap_or(0);
        self.tracker.record(obs_ms, analysis_ms);
    }
}
