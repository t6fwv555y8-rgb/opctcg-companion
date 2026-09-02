use crate::types::{ObservationEvent, ObservationSource};
use serde_json::Value;

pub fn parse_unknown_json(v: &Value, confidence: f32) -> Result<Vec<ObservationEvent>, String> {
    Ok(vec![ObservationEvent::StructuredRaw {
        raw: v.to_string(),
        source: ObservationSource::DesktopSimulator,
        confidence: confidence * 0.5,
    }])
}
