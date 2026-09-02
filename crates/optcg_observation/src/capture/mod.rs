mod change;
mod frame;
mod pipeline;

#[cfg(target_os = "windows")]
mod windows_impl;

#[cfg(not(target_os = "windows"))]
mod stub_impl;

pub use change::{ChangeDetector, RegionChange};
pub use frame::{CapturedFrame, FrameBuffer, PixelRect};
pub use pipeline::{CaptureConfig, CapturePipeline, CaptureStats};

use crate::error::ObsResult;
use crate::window::GameWindowInfo;

/// Capture a single frame from the selected window.
pub fn capture_window_frame(window: &GameWindowInfo) -> ObsResult<Option<CapturedFrame>> {
    #[cfg(target_os = "windows")]
    {
        windows_impl::capture_window_frame(window)
    }
    #[cfg(not(target_os = "windows"))]
    {
        stub_impl::capture_window_frame(window)
    }
}
