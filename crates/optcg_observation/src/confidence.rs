/// Configurable confidence thresholds for reconciliation.
#[derive(Debug, Clone)]
pub struct ConfidenceConfig {
    pub structured_log: f32,
    pub dom_observation: f32,
    pub vision_observation: f32,
    pub inferred: f32,
    pub min_apply: f32,
    pub correction_threshold: f32,
}

impl Default for ConfidenceConfig {
    fn default() -> Self {
        Self {
            structured_log: 1.0,
            dom_observation: 0.99,
            vision_observation: 0.75,
            inferred: 0.5,
            min_apply: 0.6,
            correction_threshold: 0.85,
        }
    }
}

impl ConfidenceConfig {
    pub fn for_source(source: crate::types::ObservationSource) -> f32 {
        match source {
            crate::types::ObservationSource::Mock => 1.0,
            crate::types::ObservationSource::Replay => 1.0,
            crate::types::ObservationSource::DesktopSimulator => 0.95,
            crate::types::ObservationSource::BrowserSimulator => 0.99,
            crate::types::ObservationSource::ScreenVision => 0.75,
        }
    }
}
