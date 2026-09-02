use crate::confidence::ConfidenceConfig;
use crate::types::{ObservationEvent, ObservationSource};
use optcg_core::{Phase, PlayerId};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::debug;

/// Normalized screen region (0.0–1.0) for OPTCGSim window capture.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedRegion {
    pub name: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionConfig {
    pub regions: Vec<NormalizedRegion>,
    pub calibrated: bool,
}

impl Default for RegionConfig {
    fn default() -> Self {
        Self {
            regions: default_regions(),
            calibrated: false,
        }
    }
}

fn default_regions() -> Vec<NormalizedRegion> {
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
        region("combat_indicator", 0.30, 0.42, 0.40, 0.12),
    ]
}

fn region(name: &str, x: f32, y: f32, w: f32, h: f32) -> NormalizedRegion {
    NormalizedRegion {
        name: name.into(),
        x,
        y,
        w,
        h,
    }
}

/// Fixture-friendly visual observation result.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VisionObservation {
    pub phase: Option<String>,
    pub turn: Option<u32>,
    pub self_life: Option<u8>,
    pub opponent_life: Option<u8>,
    pub self_active_don: Option<u8>,
    pub self_rested_don: Option<u8>,
    pub opponent_active_don: Option<u8>,
    pub opponent_rested_don: Option<u8>,
    pub self_hand_count: Option<u8>,
    pub opponent_hand_count: Option<u8>,
    pub board_card_ids: Vec<String>,
    pub combat_power: Option<u32>,
    pub confidence: f32,
}

impl VisionObservation {
    pub fn to_observation_events(&self) -> Vec<ObservationEvent> {
        let base = ConfidenceConfig::for_source(ObservationSource::DesktopSimulator);
        let confidence = (base * self.confidence).min(1.0);
        let mut events = Vec::new();

        if let Some(phase) = &self.phase {
            events.push(ObservationEvent::PhaseObserved {
                phase: Phase::from_str_loose(phase),
                confidence,
            });
        }
        if let Some(turn) = self.turn {
            let player = if turn % 2 == 1 {
                PlayerId::Player1
            } else {
                PlayerId::Player2
            };
            events.push(ObservationEvent::TurnObserved { player, confidence });
        }
        if let Some(life) = self.self_life {
            events.push(ObservationEvent::LifeObserved {
                player: PlayerId::Player1,
                count: life,
                confidence,
            });
        }
        if let Some(life) = self.opponent_life {
            events.push(ObservationEvent::LifeObserved {
                player: PlayerId::Player2,
                count: life,
                confidence,
            });
        }
        if let (Some(active), Some(rested)) = (self.self_active_don, self.self_rested_don) {
            events.push(ObservationEvent::DonObserved {
                player: PlayerId::Player1,
                active,
                rested,
                attached: 0,
                confidence,
            });
        }
        if let (Some(active), Some(rested)) = (self.opponent_active_don, self.opponent_rested_don) {
            events.push(ObservationEvent::DonObserved {
                player: PlayerId::Player2,
                active,
                rested,
                attached: 0,
                confidence,
            });
        }
        if let Some(count) = self.self_hand_count {
            events.push(ObservationEvent::HandCountObserved {
                player: PlayerId::Player1,
                count: count as usize,
                confidence,
            });
        }
        if let Some(count) = self.opponent_hand_count {
            events.push(ObservationEvent::HandCountObserved {
                player: PlayerId::Player2,
                count: count as usize,
                confidence,
            });
        }
        for card_id in &self.board_card_ids {
            events.push(ObservationEvent::CardObserved {
                card_id: Some(card_id.clone()),
                owner: PlayerId::Player1,
                zone: optcg_core::Zone::Character,
                position: None,
                confidence: confidence * 0.85,
            });
        }
        if self.combat_power.is_some() {
            events.push(ObservationEvent::AttackObserved {
                attacker: None,
                attacker_card_id: self.board_card_ids.first().cloned(),
                target: Some(optcg_core::AttackTarget::Leader {
                    player: PlayerId::Player2,
                }),
                observed_power: self.combat_power,
                confidence: confidence * 0.8,
            });
        }

        events
    }
}

/// Selected-window vision pipeline — capture API independent from recognition.
pub struct VisionPipeline {
    regions: RegionConfig,
    fixture_path: Option<PathBuf>,
}

impl VisionPipeline {
    pub fn new(regions: RegionConfig) -> Self {
        Self {
            regions,
            fixture_path: std::env::var("OPTCG_VISION_FIXTURE")
                .ok()
                .map(PathBuf::from),
        }
    }

    pub fn with_fixture(mut self, path: impl AsRef<Path>) -> Self {
        self.fixture_path = Some(path.as_ref().to_path_buf());
        self
    }

    pub fn regions(&self) -> &RegionConfig {
        &self.regions
    }

    /// Capture one observation. Live window capture is platform-specific; fixtures used in CI.
    pub fn capture_observation(&self) -> Option<VisionObservation> {
        if let Some(path) = &self.fixture_path {
            if path.exists() {
                if let Ok(text) = std::fs::read_to_string(path) {
                    if let Ok(obs) = serde_json::from_str::<VisionObservation>(&text) {
                        debug!(path = %path.display(), "vision fixture loaded");
                        return Some(obs);
                    }
                }
            }
        }

        // No capturable window in headless/cloud environments — return None.
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_observation_emits_phase() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vision.json");
        let obs = VisionObservation {
            phase: Some("Main".into()),
            self_life: Some(5),
            confidence: 0.9,
            ..Default::default()
        };
        std::fs::write(&path, serde_json::to_string(&obs).unwrap()).unwrap();
        let pipeline = VisionPipeline::new(RegionConfig::default()).with_fixture(&path);
        let captured = pipeline.capture_observation().unwrap();
        let events = captured.to_observation_events();
        assert!(events
            .iter()
            .any(|e| matches!(e, ObservationEvent::PhaseObserved { .. })));
    }
}
