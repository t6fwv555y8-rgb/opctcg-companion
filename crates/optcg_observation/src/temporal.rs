use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Observed scalar with confidence and staleness metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservedValue<T> {
    pub value: Option<T>,
    pub confidence: f32,
    pub observed_at_ms: i64,
}

impl<T: Clone> ObservedValue<T> {
    pub fn unknown() -> Self {
        Self {
            value: None,
            confidence: 0.0,
            observed_at_ms: chrono::Utc::now().timestamp_millis(),
        }
    }

    pub fn with_value(value: T, confidence: f32) -> Self {
        Self {
            value: Some(value),
            confidence,
            observed_at_ms: chrono::Utc::now().timestamp_millis(),
        }
    }

    pub fn age_ms(&self) -> u64 {
        let now = chrono::Utc::now().timestamp_millis();
        (now - self.observed_at_ms).max(0) as u64
    }

    pub fn is_stale(&self, max_age: Duration) -> bool {
        self.age_ms() > max_age.as_millis() as u64
    }
}

/// Bounded temporal confirmation — avoids single-frame noise.
#[derive(Debug, Clone)]
pub struct TemporalField<T: Clone + PartialEq> {
    pub pending: Option<T>,
    pub stable: Option<T>,
    pub pending_count: u32,
    pub confirm_frames: u32,
    pub confidence: f32,
}

impl<T: Clone + PartialEq> TemporalField<T> {
    pub fn new(confirm_frames: u32) -> Self {
        Self {
            pending: None,
            stable: None,
            pending_count: 0,
            confirm_frames,
            confidence: 0.0,
        }
    }

    pub fn observe(&mut self, value: T, confidence: f32) -> Option<T> {
        if self.stable.as_ref() == Some(&value) {
            self.pending = None;
            self.pending_count = 0;
            self.confidence = confidence;
            return None;
        }
        if self.pending.as_ref() == Some(&value) {
            self.pending_count += 1;
            let required = self.confirm_frames.saturating_add(1);
            if self.pending_count >= required {
                self.stable = Some(value.clone());
                self.confidence = confidence;
                self.pending = None;
                self.pending_count = 0;
                return Some(value);
            }
            return None;
        }
        self.pending = Some(value);
        self.pending_count = 1;
        None
    }

    /// Do not drop stable value on brief unknown observation.
    pub fn observe_optional(&mut self, value: Option<T>, confidence: f32) -> Option<T> {
        match value {
            Some(v) => self.observe(v, confidence),
            None => None,
        }
    }

    pub fn current(&self) -> Option<&T> {
        self.stable.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn life_temporal_confirmation() {
        let mut life = TemporalField::<u8>::new(2);
        assert!(life.observe(4, 0.9).is_none());
        assert!(life.observe(4, 0.9).is_none());
        let accepted = life.observe(4, 0.9);
        assert_eq!(accepted, Some(4));
    }

    #[test]
    fn unknown_does_not_zero_stable() {
        let mut life = TemporalField::<u8>::new(1);
        life.observe(5, 0.9);
        life.observe(5, 0.9);
        life.observe_optional(None, 0.0);
        assert_eq!(life.current(), Some(&5));
    }
}
