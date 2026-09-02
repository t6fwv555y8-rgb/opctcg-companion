use crate::confidence::ConfidenceConfig;
use crate::types::{ObservationEvent, ObservationSource};
use optcg_core::{Phase, PlayerId};
use serde_json::Value;

use super::versions;

/// Version-isolated OPTCGSim log parser.
pub struct OptcgSimLogParser {
    confidence: f32,
}

impl Default for OptcgSimLogParser {
    fn default() -> Self {
        Self::new()
    }
}

impl OptcgSimLogParser {
    pub fn new() -> Self {
        Self {
            confidence: ConfidenceConfig::for_source(ObservationSource::DesktopSimulator),
        }
    }

    pub fn parse_line(&self, line: &str) -> Result<Vec<ObservationEvent>, String> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Ok(vec![]);
        }

        if trimmed.starts_with('{') {
            return self.parse_json_line(trimmed);
        }

        if trimmed.contains('|') {
            return Ok(vec![ObservationEvent::StructuredRaw {
                raw: trimmed.to_string(),
                source: ObservationSource::DesktopSimulator,
                confidence: 1.0,
            }]);
        }

        versions::text_v1::parse_text_line(trimmed, self.confidence)
    }

    fn parse_json_line(&self, line: &str) -> Result<Vec<ObservationEvent>, String> {
        let v: Value = serde_json::from_str(line).map_err(|e| e.to_string())?;
        if let Some(events) = v.get("events").and_then(|e| e.as_array()) {
            let mut out = Vec::new();
            for ev in events {
                if let Some(raw) = ev.get("raw").and_then(|r| r.as_str()) {
                    out.push(ObservationEvent::StructuredRaw {
                        raw: raw.to_string(),
                        source: ObservationSource::DesktopSimulator,
                        confidence: 1.0,
                    });
                } else if let Some(kind) = ev.get("kind").and_then(|k| k.as_str()) {
                    out.extend(map_json_event(kind, ev, self.confidence)?);
                }
            }
            return Ok(out);
        }
        versions::unknown::parse_unknown_json(&v, self.confidence)
    }
}

fn map_json_event(
    kind: &str,
    ev: &Value,
    confidence: f32,
) -> Result<Vec<ObservationEvent>, String> {
    let k = kind.to_ascii_lowercase();
    Ok(match k.as_str() {
        "phase_changed" | "phase" => {
            let phase_str = ev.get("phase").and_then(|p| p.as_str()).unwrap_or("main");
            vec![ObservationEvent::PhaseObserved {
                phase: Phase::from_str_loose(phase_str),
                confidence,
            }]
        }
        "life_changed" | "life" => {
            let player = parse_player(ev.get("player"))?;
            let count = ev
                .get("count")
                .or_else(|| ev.get("life"))
                .and_then(|c| c.as_u64())
                .unwrap_or(0) as u8;
            vec![ObservationEvent::LifeObserved {
                player,
                count,
                confidence,
            }]
        }
        "card_played" => {
            let player = parse_player(ev.get("player"))?;
            let card_id = ev
                .get("card_id")
                .and_then(|c| c.as_str())
                .map(|s| s.to_string());
            vec![ObservationEvent::CardObserved {
                card_id,
                owner: player,
                zone: optcg_core::Zone::Character,
                position: None,
                confidence,
            }]
        }
        _ => vec![ObservationEvent::StructuredRaw {
            raw: ev.to_string(),
            source: ObservationSource::DesktopSimulator,
            confidence: confidence * 0.8,
        }],
    })
}

fn parse_player(v: Option<&Value>) -> Result<PlayerId, String> {
    match v.and_then(|p| p.as_str()).unwrap_or("player_1") {
        "0" | "player_1" | "p1" | "Player 1" => Ok(PlayerId::Player1),
        "1" | "player_2" | "p2" | "Player 2" => Ok(PlayerId::Player2),
        other => PlayerId::parse_token(other).map_err(|e| e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pipe_line() {
        let p = OptcgSimLogParser::new();
        let events = p.parse_line("PHASE_CHANGED|MAIN").unwrap();
        assert!(matches!(events[0], ObservationEvent::StructuredRaw { .. }));
    }

    #[test]
    fn parses_json_event_array() {
        let p = OptcgSimLogParser::new();
        let line = r#"{"events":[{"kind":"phase","phase":"Main"}]}"#;
        let events = p.parse_line(line).unwrap();
        assert!(matches!(events[0], ObservationEvent::PhaseObserved { .. }));
    }
}
