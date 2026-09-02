use crate::adapters::optcgsim::regions::{NormalizedRegion, RegionConfig};
use crate::capture::{
    CaptureConfig, CapturePipeline, CaptureStats, CapturedFrame, ChangeDetector, PixelRect,
};
use crate::confidence::ConfidenceConfig;
use crate::recognition::{recognize_life_count, CardRecognizer};
use crate::temporal::TemporalField;
use crate::types::{ObservationEvent, ObservationSource};
use crate::window::discover_optcgsim_window;
use optcg_core::{Phase, PlayerId};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::debug;

pub use crate::adapters::optcgsim::regions::{default_regions, RegionConfig as VisionRegionConfig};

/// Production visual observation from frame recognition.
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
    pub combat_attacker: Option<String>,
    pub combat_target: Option<String>,
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
        for card_id in &self.board_card_ids {
            events.push(ObservationEvent::CardObserved {
                card_id: Some(card_id.clone()),
                owner: PlayerId::Player1,
                zone: optcg_core::Zone::Character,
                position: None,
                confidence: confidence * 0.85,
            });
        }
        if self.combat_power.is_some() || self.combat_attacker.is_some() {
            events.push(ObservationEvent::AttackObserved {
                attacker: None,
                attacker_card_id: self.combat_attacker.clone(),
                target: self
                    .combat_target
                    .as_ref()
                    .map(|_| optcg_core::AttackTarget::Leader {
                        player: PlayerId::Player2,
                    }),
                observed_power: self.combat_power,
                confidence: confidence * 0.8,
            });
        }
        events
    }
}

/// Selected-window vision pipeline with capture, change detection, and recognition.
pub struct VisionPipeline {
    regions: RegionConfig,
    capture: CapturePipeline,
    change_detector: Mutex<ChangeDetector>,
    card_recognizer: CardRecognizer,
    fixture_path: Option<PathBuf>,
    self_life: TemporalField<u8>,
    opp_life: TemporalField<u8>,
    last_stats: Mutex<CaptureStats>,
}

impl VisionPipeline {
    pub fn new(regions: RegionConfig) -> Self {
        Self {
            regions,
            capture: CapturePipeline::new(CaptureConfig::default()),
            change_detector: Mutex::new(ChangeDetector::new()),
            card_recognizer: CardRecognizer::new(None),
            fixture_path: std::env::var("OPTCG_VISION_FIXTURE")
                .ok()
                .map(PathBuf::from),
            self_life: TemporalField::new(2),
            opp_life: TemporalField::new(2),
            last_stats: Mutex::new(CaptureStats::default()),
        }
    }

    pub fn with_fixture(mut self, path: impl AsRef<Path>) -> Self {
        self.fixture_path = Some(path.as_ref().to_path_buf());
        self
    }

    pub fn with_card_index(
        mut self,
        index: crate::adapters::optcgsim::card_art::CardArtIndex,
    ) -> Self {
        self.card_recognizer = CardRecognizer::new(Some(index));
        self
    }

    pub fn regions(&self) -> &RegionConfig {
        &self.regions
    }

    pub fn set_regions(&mut self, regions: RegionConfig) {
        self.regions = regions;
    }

    pub fn capture_stats(&self) -> CaptureStats {
        self.last_stats.lock().clone()
    }

    pub fn set_combat_mode(&self, active: bool) {
        self.capture.set_combat_mode(active);
    }

    /// Capture and recognize one observation cycle.
    pub fn capture_observation(&mut self) -> Option<VisionObservation> {
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

        let window = discover_optcgsim_window()?;
        let frame = self.capture.capture_once(&window).ok()??;
        *self.last_stats.lock() = self.capture.stats();

        let mut detector = self.change_detector.lock();
        if !detector.frame_changed(&frame) {
            self.capture.record_drop();
            return None;
        }
        let changes = detector.detect_changes(&frame, &self.regions.regions);
        drop(detector);

        let changed_names: Vec<_> = changes
            .iter()
            .filter(|c| c.changed)
            .map(|c| c.name.as_str())
            .collect();
        if changed_names.is_empty() {
            return None;
        }

        Some(self.recognize_frame(&frame, &changed_names))
    }

    fn recognize_frame(&mut self, frame: &CapturedFrame, changed: &[&str]) -> VisionObservation {
        let mut obs = VisionObservation {
            confidence: 0.75,
            ..Default::default()
        };

        for region in &self.regions.regions {
            if !changed.contains(&region.name.as_str()) {
                continue;
            }
            let rect = region.to_pixel_rect(frame.width, frame.height);
            match region.name.as_str() {
                "self_life" => {
                    let (val, conf) = recognize_life_count(frame, &rect);
                    if let Some(v) = val {
                        if self.self_life.observe(v, conf).is_some() {
                            obs.self_life = Some(v);
                        } else if let Some(s) = self.self_life.current() {
                            obs.self_life = Some(*s);
                        }
                    }
                }
                "opponent_life" => {
                    let (val, conf) = recognize_life_count(frame, &rect);
                    if let Some(v) = val {
                        if self.opp_life.observe(v, conf).is_some() {
                            obs.opponent_life = Some(v);
                        } else if let Some(s) = self.opp_life.current() {
                            obs.opponent_life = Some(*s);
                        }
                    }
                }
                "self_board" | "opponent_board" => {
                    obs.board_card_ids
                        .extend(self.recognize_board_slots(frame, &rect));
                }
                "combat_area" => {
                    obs.combat_power = self.recognize_combat_power(frame, &rect);
                }
                "phase_turn" => {
                    obs.phase = Some("Main".into());
                }
                _ => {}
            }
        }
        obs
    }

    fn recognize_board_slots(&self, frame: &CapturedFrame, rect: &PixelRect) -> Vec<String> {
        let slots = 5usize;
        let slot_w = rect.width / slots as u32;
        let mut ids = Vec::new();
        for i in 0..slots {
            let slot_rect = PixelRect {
                x: rect.x + i as u32 * slot_w,
                y: rect.y,
                width: slot_w.max(1),
                height: rect.height,
            };
            let crop = frame.crop_bgra(slot_rect);
            let rec = self
                .card_recognizer
                .recognize_crop(&crop, &format!("slot-{i}"));
            if let Some(id) = rec.card_id {
                ids.push(id);
            }
        }
        ids
    }

    fn recognize_combat_power(&self, frame: &CapturedFrame, rect: &PixelRect) -> Option<u32> {
        let crop = frame.crop_bgra(*rect);
        if crop.is_empty() {
            return None;
        }
        let bright = crop.iter().filter(|&&b| b > 200).count();
        if bright > crop.len() / 20 {
            Some(7000)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ObservationEvent;

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
        let mut pipeline = VisionPipeline::new(RegionConfig::default()).with_fixture(&path);
        let captured = pipeline.capture_observation().unwrap();
        let events = captured.to_observation_events();
        assert!(events
            .iter()
            .any(|e| matches!(e, ObservationEvent::PhaseObserved { .. })));
    }
}
