use super::frame::{CapturedFrame, FrameBuffer};
use crate::error::{ObsResult, ObservationError};
use crate::window::GameWindowInfo;
use std::time::Instant;
use tracing::debug;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits,
    SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, SRCCOPY,
};
use windows::Win32::UI::WindowsAndMessaging::{GetDC, GetWindowDC, ReleaseDC};

/// Capture OPTCGSim window via GDI BitBlt (external, read-only).
pub fn capture_window_frame(window: &GameWindowInfo) -> ObsResult<Option<CapturedFrame>> {
    if window.hwnd == 0 || !window.is_capturable() {
        return Ok(None);
    }
    let hwnd = HWND(window.hwnd as _);
    unsafe {
        let hdc_window = GetWindowDC(hwnd);
        if hdc_window.0.is_null() {
            return Err(ObservationError::Unavailable("GetWindowDC failed".into()));
        }
        let hdc_mem = CreateCompatibleDC(hdc_window);
        if hdc_mem.0.is_null() {
            ReleaseDC(hwnd, hdc_window);
            return Err(ObservationError::Unavailable(
                "CreateCompatibleDC failed".into(),
            ));
        }

        let w = window.width;
        let h = window.height;
        let hbitmap = CreateCompatibleBitmap(hdc_window, w as i32, h as i32);
        if hbitmap.0.is_null() {
            let _ = DeleteDC(hdc_mem);
            ReleaseDC(hwnd, hdc_window);
            return Err(ObservationError::Unavailable(
                "CreateCompatibleBitmap failed".into(),
            ));
        }
        let _old = SelectObject(hdc_mem, hbitmap);

        let blt_ok = BitBlt(hdc_mem, 0, 0, w as i32, h as i32, hdc_window, 0, 0, SRCCOPY).is_ok();
        if !blt_ok {
            let _ = DeleteObject(hbitmap);
            let _ = DeleteDC(hdc_mem);
            ReleaseDC(hwnd, hdc_window);
            return Err(ObservationError::Unavailable("BitBlt failed".into()));
        }

        let stride = (w * 4) as usize;
        let mut data = vec![0u8; stride * h as usize];
        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w as i32,
                biHeight: -(h as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };

        let lines = GetDIBits(
            hdc_mem,
            hbitmap,
            0,
            h,
            Some(data.as_mut_ptr() as _),
            &mut bmi,
            DIB_RGB_COLORS,
        );
        let _ = DeleteObject(hbitmap);
        let _ = DeleteDC(hdc_mem);
        ReleaseDC(hwnd, hdc_window);

        if lines == 0 {
            return Err(ObservationError::Unavailable("GetDIBits failed".into()));
        }

        debug!(w, h, "window frame captured");
        Ok(Some(CapturedFrame {
            timestamp: Instant::now(),
            width: w,
            height: h,
            stride,
            pixels: FrameBuffer { data },
            window: window.clone(),
        }))
    }
}
