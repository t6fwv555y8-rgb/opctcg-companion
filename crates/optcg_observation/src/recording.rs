use crate::error::ObsResult;
use crate::types::{ObservationEnvelope, ObservationEvent, ObservationSource};
use chrono::Utc;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

/// Optional local JSONL observation recorder.
pub struct ObservationRecorder {
    path: Option<PathBuf>,
    enabled: bool,
}

impl ObservationRecorder {
    pub fn disabled() -> Self {
        Self {
            path: None,
            enabled: false,
        }
    }

    pub fn open_dir(dir: impl AsRef<Path>) -> ObsResult<Self> {
        std::fs::create_dir_all(&dir)?;
        let path = dir
            .as_ref()
            .join(format!("session-{}.jsonl", Uuid::new_v4()));
        Ok(Self {
            path: Some(path),
            enabled: true,
        })
    }

    pub fn record(&self, envelope: &ObservationEnvelope) -> ObsResult<()> {
        if !self.enabled {
            return Ok(());
        }
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
}

use uuid::Uuid;

/// Replay recorded observations from JSONL.
pub fn load_replay_lines(path: &Path) -> ObsResult<Vec<ObservationEnvelope>> {
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
