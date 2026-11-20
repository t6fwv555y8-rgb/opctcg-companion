use super::window::SimWindow;
use image::DynamicImage;

/// One picture of the simulator window.
#[derive(Debug)]
pub struct Frame {
    pub image: DynamicImage,
    /// Pixels per point: 2 on a Retina display.
    pub scale: f32,
    /// The picture as a file, for the text reader. Removed when the frame is dropped.
    pub path: std::path::PathBuf,
}

impl Drop for Frame {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureError {
    /// macOS did not let us see the window (permission not granted).
    PermissionDenied,
    /// The window is no longer there.
    WindowGone,
    Other(String),
}

impl std::fmt::Display for CaptureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PermissionDenied => write!(f, "{}", super::window::SCREEN_ACCESS_HELP),
            Self::WindowGone => write!(f, "The simulator window is not on screen."),
            Self::Other(m) => write!(f, "{m}"),
        }
    }
}

/// A picture that is one flat colour is what a blocked capture looks like.
fn looks_blank(image: &DynamicImage) -> bool {
    let rgb = image.to_rgb8();
    let (w, h) = rgb.dimensions();
    if w == 0 || h == 0 {
        return true;
    }
    let first = rgb.get_pixel(0, 0).0;
    let mut differing = 0u32;
    let step = (w / 40).max(1);
    let mut sampled = 0u32;
    for y in (0..h).step_by(step as usize) {
        for x in (0..w).step_by(step as usize) {
            sampled += 1;
            let p = rgb.get_pixel(x, y).0;
            let diff: i32 = p.iter().zip(first).map(|(a, b)| (*a as i32 - b as i32).abs()).sum();
            if diff > 24 {
                differing += 1;
            }
        }
    }
    differing * 50 < sampled // fewer than 2% of sampled points differ
}

/// Take a picture of one window. Uses the system's own `screencapture` tool,
/// which keeps working across macOS versions and respects the permission the
/// player gave.
pub fn capture_window(window: &SimWindow) -> Result<Frame, CaptureError> {
    let dir = std::env::temp_dir();
    let path = dir.join(format!("optcg-frame-{}-{}.png", std::process::id(), window.window_number));
    let output = std::process::Command::new("/usr/sbin/screencapture")
        // -x: no sound   -o: no window shadow   -l: this window only
        .args(["-x", "-o", "-t", "png", "-l", &window.window_number.to_string()])
        .arg(&path)
        .output()
        .map_err(|e| CaptureError::Other(format!("could not run screencapture: {e}")))?;
    let stderr = String::from_utf8_lossy(&output.stderr).to_lowercase();
    if !output.status.success() || !path.exists() {
        let _ = std::fs::remove_file(&path);
        return Err(if stderr.contains("permission") || stderr.contains("not authorized") {
            CaptureError::PermissionDenied
        } else if stderr.contains("could not create image") || stderr.contains("no such window") {
            CaptureError::WindowGone
        } else {
            CaptureError::Other(format!("screencapture failed: {}", stderr.trim()))
        });
    }
    let Some(image) = crate::art::load_image(&path) else {
        let _ = std::fs::remove_file(&path);
        return Err(CaptureError::Other("could not read the captured picture".into()));
    };
    if looks_blank(&image) {
        let _ = std::fs::remove_file(&path);
        return Err(CaptureError::PermissionDenied);
    }
    let scale = if window.width > 0.0 { image.width() as f32 / window.width as f32 } else { 1.0 };
    Ok(Frame { image, scale, path })
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};

    #[test]
    fn a_single_colour_picture_counts_as_blank() {
        let flat = DynamicImage::ImageRgb8(RgbImage::from_pixel(400, 300, Rgb([10, 10, 10])));
        assert!(looks_blank(&flat));
    }

    #[test]
    fn a_picture_with_content_does_not() {
        let busy = DynamicImage::ImageRgb8(RgbImage::from_fn(400, 300, |x, y| Rgb([(x % 255) as u8, (y % 255) as u8, 90])));
        assert!(!looks_blank(&busy));
    }
}
