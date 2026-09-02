use serde::{Deserialize, Serialize};

/// Reconciliation / observation sync status exposed to HUD and debug panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SyncStatus {
    #[default]
    Synced,
    Partial,
    Recovering,
    Degraded,
    Desynced,
}

impl SyncStatus {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Synced => "SYNCED",
            Self::Partial => "PARTIAL",
            Self::Recovering => "RECOVERING",
            Self::Degraded => "DEGRADED",
            Self::Desynced => "DESYNCED",
        }
    }

    pub fn hud_state(&self) -> &'static str {
        match self {
            Self::Synced => "LIVE",
            Self::Partial => "PARTIAL",
            Self::Recovering => "SYNCING",
            Self::Degraded => "PARTIAL",
            Self::Desynced => "LOST",
        }
    }

    pub fn from_confidence(confidence: f32, source_connected: bool) -> Self {
        if !source_connected {
            return Self::Desynced;
        }
        if confidence >= 0.85 {
            SyncStatus::Synced
        } else if confidence >= 0.65 {
            SyncStatus::Partial
        } else if confidence >= 0.45 {
            SyncStatus::Recovering
        } else if confidence >= 0.25 {
            SyncStatus::Degraded
        } else {
            SyncStatus::Desynced
        }
    }
}

/// Reasons sync may be degraded — for debug panel.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SyncContext {
    pub status: SyncStatus,
    pub reasons: Vec<String>,
    pub source_connected: bool,
    pub capture_available: bool,
    pub bridge_connected: bool,
}
