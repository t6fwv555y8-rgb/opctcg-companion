use crate::auto::{
    AutoTriggerConfig, DEFAULT_MIN_INTERVAL_MS, DEFAULT_SETTLE_MS, URGENT_MIN_INTERVAL_MS,
    URGENT_SETTLE_MS,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

const SETTLE_MIN: u64 = 800;
const SETTLE_MAX: u64 = 2_200;
const INTERVAL_MIN: u64 = 2_000;
const INTERVAL_MAX: u64 = 5_000;
const URGENT_SETTLE_MIN: u64 = 250;
const URGENT_SETTLE_MAX: u64 = 800;
const URGENT_INTERVAL_MIN: u64 = 800;
const URGENT_INTERVAL_MAX: u64 = 2_000;

/// Learned auto-read timing. Survives a restart so the HUD gets quieter or
/// faster from how you actually play, not from the compile-time defaults.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoachPrefs {
    pub settle_ms: u64,
    pub min_interval_ms: u64,
    pub urgent_settle_ms: u64,
    pub urgent_min_interval_ms: u64,
}

impl Default for CoachPrefs {
    fn default() -> Self {
        Self {
            settle_ms: DEFAULT_SETTLE_MS,
            min_interval_ms: DEFAULT_MIN_INTERVAL_MS,
            urgent_settle_ms: URGENT_SETTLE_MS,
            urgent_min_interval_ms: URGENT_MIN_INTERVAL_MS,
        }
    }
}

impl CoachPrefs {
    pub fn path(data_dir: impl AsRef<Path>) -> PathBuf {
        data_dir.as_ref().join("coach_prefs.json")
    }

    pub fn load(data_dir: impl AsRef<Path>) -> Self {
        let Ok(raw) = std::fs::read_to_string(Self::path(data_dir)) else {
            return Self::default();
        };
        serde_json::from_str(&raw).unwrap_or_default()
    }

    pub fn save(&self, data_dir: impl AsRef<Path>) -> Result<(), String> {
        let dir = data_dir.as_ref();
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let raw = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        let path = Self::path(dir);
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, raw).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, path).map_err(|e| e.to_string())
    }

    pub fn to_config(&self, enabled: bool) -> AutoTriggerConfig {
        AutoTriggerConfig {
            enabled,
            settle: Duration::from_millis(self.settle_ms),
            min_interval: Duration::from_millis(self.min_interval_ms),
            urgent_settle: Duration::from_millis(self.urgent_settle_ms),
            urgent_min_interval: Duration::from_millis(self.urgent_min_interval_ms),
        }
    }

    /// The board moved before the last automatic read landed — wait longer.
    pub fn fired_early(&mut self) {
        self.settle_ms = (self.settle_ms + 100).min(SETTLE_MAX);
        self.urgent_settle_ms = (self.urgent_settle_ms + 50).min(URGENT_SETTLE_MAX);
    }

    /// You had to ask "what now" after a quiet stretch — read sooner.
    pub fn asked_late(&mut self) {
        self.min_interval_ms = self.min_interval_ms.saturating_sub(200).max(INTERVAL_MIN);
        self.urgent_min_interval_ms = self
            .urgent_min_interval_ms
            .saturating_sub(100)
            .max(URGENT_INTERVAL_MIN);
        self.settle_ms = self.settle_ms.saturating_sub(50).max(SETTLE_MIN);
        self.urgent_settle_ms = self
            .urgent_settle_ms
            .saturating_sub(25)
            .max(URGENT_SETTLE_MIN);
    }

    /// You asked again right after a read — that one did not land. Slow down.
    pub fn ignored_call(&mut self) {
        self.min_interval_ms = (self.min_interval_ms + 150).min(INTERVAL_MAX);
        self.urgent_min_interval_ms =
            (self.urgent_min_interval_ms + 80).min(URGENT_INTERVAL_MAX);
    }
}

/// True when the player is asking for a live call, not a rules question.
pub fn looks_like_what_now(question: &str) -> bool {
    let q = question.trim().to_ascii_lowercase();
    if q.is_empty() {
        return false;
    }
    [
        "what now",
        "what do i",
        "what should i",
        "coach this",
        "what play",
        "the line",
        "do this",
    ]
    .iter()
    .any(|needle| q.contains(needle))
        || q == "now"
        || q == "?"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn early_fires_lengthen_the_settle() {
        let mut prefs = CoachPrefs::default();
        let before = prefs.settle_ms;
        prefs.fired_early();
        assert!(prefs.settle_ms > before);
        for _ in 0..80 {
            prefs.fired_early();
        }
        assert_eq!(prefs.settle_ms, SETTLE_MAX);
    }

    #[test]
    fn late_asks_shorten_the_gap() {
        let mut prefs = CoachPrefs::default();
        let before = prefs.min_interval_ms;
        prefs.asked_late();
        assert!(prefs.min_interval_ms < before);
    }

    #[test]
    fn what_now_is_recognised() {
        assert!(looks_like_what_now("what now"));
        assert!(looks_like_what_now("What should I do?"));
        assert!(!looks_like_what_now("Does blocker stop Rush?"));
    }

    #[test]
    fn round_trips_through_disk() {
        let dir = std::env::temp_dir().join(format!(
            "optcg-prefs-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let mut prefs = CoachPrefs::default();
        prefs.fired_early();
        prefs.save(&dir).unwrap();
        assert_eq!(CoachPrefs::load(&dir), prefs);
        std::fs::remove_dir_all(&dir).ok();
    }
}
