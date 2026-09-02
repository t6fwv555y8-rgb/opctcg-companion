use serde::{Deserialize, Serialize};

/// Live metadata for the selected OPTCGSim top-level window.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GameWindowInfo {
    pub process_id: u32,
    pub title: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub minimized: bool,
    pub visible: bool,
    pub monitor_scale: f32,
    #[serde(default)]
    pub hwnd: u64,
}

impl GameWindowInfo {
    pub fn aspect_ratio(&self) -> f32 {
        if self.height == 0 {
            return 16.0 / 9.0;
        }
        self.width as f32 / self.height as f32
    }

    pub fn is_capturable(&self) -> bool {
        self.visible && !self.minimized && self.width > 64 && self.height > 64
    }
}
