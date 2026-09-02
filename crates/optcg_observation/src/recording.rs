use crate::error::ObsResult;
use crate::types::{ObservationEnvelope, ObservationEvent, ObservationSource};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Versioned replay session format (schema v1).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplaySessionV1 {
    pub schema_version: u32,
    pub source: String,
    pub session_id: String,
    pub started_at: String,
    pub observations: Vec<ReplayObservationEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayObservationEntry {
    pub sequence: u64,
    pub timestamp_ms: i64,
    pub source: ObservationSource,
    pub observation: ObservationEvent,
    pub confidence: f32,
}

impl ReplaySessionV1 {
    pub fn new(source: ObservationSource) -> Self {
        Self {
            schema_version: 1,
            source: source.label().to_string(),
            session_id: Uuid::new_v4().to_string(),
            started_at: Utc::now().to_rfc3339(),
            observations: Vec::new(),
        }
    }

    pub fn push(&mut self, envelope: &ObservationEnvelope) {
        self.observations.push(ReplayObservationEntry {
            sequence: envelope.sequence,
            timestamp_ms: envelope.timestamp_ms,
            source: envelope.source,
            observation: envelope.event.clone(),
            confidence: envelope.event.confidence(),
        });
    }

    pub fn to_envelopes(&self) -> Vec<ObservationEnvelope> {
        self.observations
            .iter()
            .map(|e| ObservationEnvelope {
                sequence: e.sequence,
                timestamp_ms: e.timestamp_ms,
                source: e.source,
                event: e.observation.clone(),
            })
            .collect()
    }
}

/// Optional local observation recorder — JSONL for streaming, v1 JSON on close.
pub struct ObservationRecorder {
    path: Option<PathBuf>,
    v1_path: Option<PathBuf>,
    enabled: bool,
    session: ReplaySessionV1,
}

impl ObservationRecorder {
    pub fn disabled() -> Self {
        Self {
            path: None,
            v1_path: None,
            enabled: false,
            session: ReplaySessionV1::new(ObservationSource::Mock),
        }
    }

    pub fn open_dir(dir: impl AsRef<Path>) -> ObsResult<Self> {
        std::fs::create_dir_all(&dir)?;
        let session_id = Uuid::new_v4();
        let path = dir.as_ref().join(format!("session-{session_id}.jsonl"));
        let v1_path = dir.as_ref().join(format!("session-{session_id}.v1.json"));
        Ok(Self {
            path: Some(path),
            v1_path: Some(v1_path),
            enabled: true,
            session: ReplaySessionV1::new(ObservationSource::Mock),
        })
    }

    pub fn record(&mut self, envelope: &ObservationEnvelope) -> ObsResult<()> {
        if !self.enabled {
            return Ok(());
        }
        if self.session.observations.is_empty() {
            self.session.source = envelope.source.label().to_string();
        }
        self.session.push(envelope);

        let path = self.path.as_ref().ok_or_else(|| {
            crate::error::ObservationError::Adapter("recorder not initialized".into())
        })?;
        let line = serde_json::json!({
            "timestamp": Utc::now().to_rfc3339(),
            "source": envelope.source,
            "sequence": envelope.sequence,
            "observation": envelope.event,
            "confidence": envelope.event.confidence(),
        });
        let mut file = OpenOptions::new().create(true).append(true).open(path)?;
        writeln!(file, "{}", line)?;
        Ok(())
    }

    pub fn flush_v1(&self) -> ObsResult<Option<PathBuf>> {
        if !self.enabled {
            return Ok(None);
        }
        let path = match &self.v1_path {
            Some(p) => p,
            None => return Ok(None),
        };
        if self.session.observations.is_empty() {
            return Ok(None);
        }
        let json = serde_json::to_string_pretty(&self.session)?;
        std::fs::write(path, json)?;
        Ok(Some(path.clone()))
    }
}

/// Load replay from v1 JSON or legacy JSONL.
pub fn load_replay_lines(path: &Path) -> ObsResult<Vec<ObservationEnvelope>> {
    let text = std::fs::read_to_string(path)?;
    if let Ok(v1) = serde_json::from_str::<ReplaySessionV1>(&text) {
        if v1.schema_version == 1 {
            return Ok(v1.to_envelopes());
        }
    }
    load_jsonl_replay(path)
}

fn load_jsonl_replay(path: &Path) -> ObsResult<Vec<ObservationEnvelope>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut envelopes = Vec::new();
    for (i, line) in reader.lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let v: serde_json::Value = serde_json::from_str(&line)?;
        let event: ObservationEvent =
            serde_json::from_value(v.get("observation").cloned().ok_or_else(|| {
                crate::error::ObservationError::InvalidPayload(format!("line {i}"))
            })?)?;
        let source: ObservationSource = serde_json::from_value(
            v.get("source")
                .cloned()
                .unwrap_or(serde_json::json!("replay")),
        )
        .unwrap_or(ObservationSource::Replay);
        envelopes.push(ObservationEnvelope {
            sequence: i as u64 + 1,
            timestamp_ms: Utc::now().timestamp_millis(),
            source,
            event,
        });
    }
    Ok(envelopes)
}

/// Transform a debug capture into a sanitized regression fixture (v1 replay).
pub fn write_regression_fixture(
    path: &Path,
    source: ObservationSource,
    observations: &[ObservationEnvelope],
) -> ObsResult<()> {
    let mut session = ReplaySessionV1::new(source);
    for envelope in observations {
        session.push(envelope);
    }
    let json = serde_json::to_string_pretty(&session)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, json)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ObservationEvent;
    use tempfile::tempdir;

    #[test]
    fn v1_roundtrip() {
        let dir = tempdir().unwrap();
        let mut recorder = ObservationRecorder::open_dir(dir.path()).unwrap();
        let envelope = ObservationEnvelope {
            sequence: 1,
            timestamp_ms: 1000,
            source: ObservationSource::Mock,
            event: ObservationEvent::StructuredRaw {
                raw: "PHASE_CHANGED|MAIN".into(),
                source: ObservationSource::Mock,
                confidence: 1.0,
            },
        };
        recorder.record(&envelope).unwrap();
        let v1_path = recorder.flush_v1().unwrap().unwrap();
        let loaded = load_replay_lines(&v1_path).unwrap();
        assert_eq!(loaded.len(), 1);
        assert!(matches!(
            loaded[0].event,
            ObservationEvent::StructuredRaw { .. }
        ));
    }

    #[test]
    fn legacy_jsonl_still_loads() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("legacy.jsonl");
        let line = serde_json::json!({
            "source": "mock",
            "observation": {
                "kind": "structured_raw",
                "raw": "PHASE_CHANGED|MAIN",
                "source": "mock",
                "confidence": 1.0
            }
        });
        std::fs::write(&path, format!("{line}\n")).unwrap();
        let loaded = load_replay_lines(&path).unwrap();
        assert_eq!(loaded.len(), 1);
    }
}
