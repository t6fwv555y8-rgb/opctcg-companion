use super::frame::{CapturedFrame, FrameBuffer};
use crate::error::ObsResult;
use crate::window::GameWindowInfo;
use std::fs;
use std::time::Instant;

/// Non-Windows capture: load synthetic fixture frame when OPTCG_VISION_FIXTURE is set.
pub fn capture_window_frame(window: &GameWindowInfo) -> ObsResult<Option<CapturedFrame>> {
    let path = match std::env::var("OPTCG_VISION_FIXTURE") {
        Ok(p) => p,
        Err(_) => return Ok(None),
    };
    let text = fs::read_to_string(&path).map_err(|e| {
        crate::error::ObservationError::Io(std::io::Error::new(
            e.kind(),
            format!("fixture read: {e}"),
        ))
    })?;
    let v: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| crate::error::ObservationError::InvalidPayload(e.to_string()))?;

    let w = v
        .get("frame_width")
        .and_then(|x| x.as_u64())
        .unwrap_or(window.width as u64) as u32;
    let h = v
        .get("frame_height")
        .and_then(|x| x.as_u64())
        .unwrap_or(window.height as u64) as u32;
    let fill = v.get("frame_fill").and_then(|x| x.as_u64()).unwrap_or(128) as u8;
    let stride = (w * 4) as usize;
    let data = vec![fill; stride * h as usize];

    Ok(Some(CapturedFrame {
        timestamp: Instant::now(),
        width: w,
        height: h,
        stride,
        pixels: FrameBuffer { data },
        window: window.clone(),
    }))
}
