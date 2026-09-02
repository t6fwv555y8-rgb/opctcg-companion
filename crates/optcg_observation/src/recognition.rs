use crate::adapters::optcgsim::card_art::{dhash64, CardArtIndex};
use crate::capture::{CapturedFrame, PixelRect};
use crate::temporal::TemporalField;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CardCandidate {
    pub card_id: String,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CardRecognition {
    pub card_id: Option<String>,
    pub confidence: f32,
    pub candidates: Vec<CardCandidate>,
    pub slot: String,
}

/// Visual card matcher using local StreamingAssets index.
pub struct CardRecognizer {
    index: Option<CardArtIndex>,
    min_confidence: f32,
}

impl CardRecognizer {
    pub fn new(index: Option<CardArtIndex>) -> Self {
        Self {
            index,
            min_confidence: 0.72,
        }
    }

    pub fn recognize_crop(&self, crop: &[u8], slot: &str) -> CardRecognition {
        if crop.is_empty() {
            return CardRecognition {
                slot: slot.into(),
                ..Default::default()
            };
        }
        let fp = dhash64(crop);
        let mut candidates = Vec::new();
        if let Some(index) = &self.index {
            if let Some(id) = index.match_fingerprint(fp) {
                candidates.push(CardCandidate {
                    card_id: id.to_string(),
                    confidence: 0.85,
                });
            }
        }
        let best = candidates.first().cloned();
        let confidence = best.as_ref().map(|c| c.confidence).unwrap_or(0.0);
        CardRecognition {
            card_id: if confidence >= self.min_confidence {
                best.map(|c| c.card_id)
            } else {
                None
            },
            confidence,
            candidates,
            slot: slot.into(),
        }
    }
}

/// Estimate life count from life region brightness clusters (structural heuristic).
pub fn recognize_life_count(frame: &CapturedFrame, rect: &PixelRect) -> (Option<u8>, f32) {
    let crop = frame.crop_bgra(*rect);
    if crop.is_empty() {
        return (None, 0.0);
    }
    let avg: u32 = crop.iter().map(|&b| b as u32).sum::<u32>() / crop.len() as u32;
    if avg < 20 {
        return (None, 0.0);
    }
    let bright_spots = crop
        .chunks(16)
        .filter(|c| c.iter().map(|&b| b as u32).sum::<u32>() / c.len() as u32 > 180)
        .count();
    let estimate = (bright_spots / 8).min(10) as u8;
    if estimate == 0 {
        (None, 0.3)
    } else {
        (Some(estimate.max(1)), 0.65)
    }
}

/// Detect rested cards via horizontal/vertical edge ratio in slot crop.
pub fn recognize_rested(crop: &[u8], width: u32, height: u32) -> (bool, f32) {
    if crop.is_empty() || width == 0 || height == 0 {
        return (false, 0.0);
    }
    let w = width as usize;
    let h = height as usize;
    let mut h_edge = 0u32;
    let mut v_edge = 0u32;
    for y in 0..h {
        for x in 0..w.saturating_sub(1) {
            let i = (y * w + x) * 4;
            if i + 4 < crop.len() {
                h_edge += (crop[i] as i32 - crop[i + 4] as i32).unsigned_abs();
            }
        }
    }
    for x in 0..w {
        for y in 0..h.saturating_sub(1) {
            let i = (y * w + x) * 4;
            let j = ((y + 1) * w + x) * 4;
            if j + 3 < crop.len() {
                v_edge += (crop[i] as i32 - crop[j] as i32).unsigned_abs();
            }
        }
    }
    let ratio = h_edge as f32 / v_edge.max(1) as f32;
    let rested = ratio > 1.4;
    (rested, if rested { 0.7 } else { 0.6 })
}

/// Board slot grid — layout-aware empty slot support.
#[derive(Debug, Clone)]
pub struct BoardSlotState {
    pub slots: Vec<CardRecognition>,
    pub temporal: TemporalField<String>,
}

impl Default for BoardSlotState {
    fn default() -> Self {
        Self {
            slots: Vec::new(),
            temporal: TemporalField::new(2),
        }
    }
}

impl BoardSlotState {
    pub fn new() -> Self {
        Self {
            slots: Vec::new(),
            temporal: TemporalField::new(2),
        }
    }

    pub fn update_slot(&mut self, idx: usize, recognition: CardRecognition) {
        while self.slots.len() <= idx {
            self.slots.push(CardRecognition::default());
        }
        self.slots[idx] = recognition;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn low_confidence_card_not_certain() {
        let rec = CardRecognizer::new(None);
        let result = rec.recognize_crop(&[0u8; 64], "slot-0");
        assert!(result.card_id.is_none());
        assert!(result.confidence < 0.72);
    }
}
