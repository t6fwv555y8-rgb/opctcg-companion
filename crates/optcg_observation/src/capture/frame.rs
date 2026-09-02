use crate::window::GameWindowInfo;
use serde::{Deserialize, Serialize};
use std::time::Instant;

/// Owned BGRA pixel buffer (4 bytes per pixel).
#[derive(Debug, Clone)]
pub struct FrameBuffer {
    pub data: Vec<u8>,
}

impl FrameBuffer {
    pub fn new(size: usize) -> Self {
        Self {
            data: vec![0u8; size],
        }
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

/// Pixel rectangle within a captured frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PixelRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl PixelRect {
    pub fn clamp_to_frame(&self, frame_w: u32, frame_h: u32) -> PixelRect {
        let x = self.x.min(frame_w.saturating_sub(1));
        let y = self.y.min(frame_h.saturating_sub(1));
        let width = self.width.min(frame_w.saturating_sub(x));
        let height = self.height.min(frame_h.saturating_sub(y));
        PixelRect {
            x,
            y,
            width,
            height,
        }
    }
}

/// One captured window frame with metadata.
#[derive(Debug, Clone)]
pub struct CapturedFrame {
    pub timestamp: Instant,
    pub width: u32,
    pub height: u32,
    pub stride: usize,
    pub pixels: FrameBuffer,
    pub window: GameWindowInfo,
}

impl CapturedFrame {
    pub fn crop_bgra(&self, rect: PixelRect) -> Vec<u8> {
        let rect = rect.clamp_to_frame(self.width, self.height);
        if rect.width == 0 || rect.height == 0 {
            return Vec::new();
        }
        let bpp = 4usize;
        let row_bytes = self.stride;
        let mut out = Vec::with_capacity((rect.width * rect.height * 4) as usize);
        for row in 0..rect.height {
            let src_y = rect.y + row;
            let src_start = src_y as usize * row_bytes + rect.x as usize * bpp;
            let src_end = src_start + rect.width as usize * bpp;
            if src_end <= self.pixels.data.len() {
                out.extend_from_slice(&self.pixels.data[src_start..src_end]);
            }
        }
        out
    }
}
