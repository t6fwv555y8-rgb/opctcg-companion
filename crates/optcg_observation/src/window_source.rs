use serde::{Deserialize, Serialize};

/// Abstraction for observing a single simulator window (future vision fallback).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowSourceConfig {
    pub window_title_hint: Option<String>,
    pub process_name_hint: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WindowSourceStatus {
    Unavailable,
    Ready,
    Capturing,
    Error,
}

/// Captured frame metadata — actual pixel buffer deferred to vision pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameMetadata {
    pub width: u32,
    pub height: u32,
    pub captured_at_ms: i64,
}

pub struct WindowSource {
    config: WindowSourceConfig,
    status: WindowSourceStatus,
}

impl WindowSource {
    pub fn new(config: WindowSourceConfig) -> Self {
        Self {
            config,
            status: WindowSourceStatus::Unavailable,
        }
    }

    pub fn status(&self) -> WindowSourceStatus {
        self.status
    }

    pub fn detect(&mut self) -> bool {
        // Platform-specific window enumeration deferred; do not false-positive.
        self.status = WindowSourceStatus::Unavailable;
        false
    }

    pub fn capture_metadata(&self) -> Option<FrameMetadata> {
        if self.status != WindowSourceStatus::Ready {
            return None;
        }
        Some(FrameMetadata {
            width: 0,
            height: 0,
            captured_at_ms: chrono::Utc::now().timestamp_millis(),
        })
    }
}
