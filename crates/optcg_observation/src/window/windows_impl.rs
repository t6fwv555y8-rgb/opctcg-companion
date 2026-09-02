use super::info::GameWindowInfo;
use crate::error::{ObsResult, ObservationError};
use crate::process_detect::detect_optcgsim_processes;
use std::sync::Mutex;
use tracing::debug;

use windows::Win32::Foundation::{BOOL, HWND, LPARAM, RECT};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowRect, GetWindowTextLengthW, GetWindowTextW, IsIconic, IsWindowVisible,
};

static SELECTED_HWND: Mutex<Option<isize>> = Mutex::new(None);

pub fn set_selected_hwnd(hwnd: isize) {
    *SELECTED_HWND.lock().unwrap() = Some(hwnd);
}

pub fn discover_optcgsim_window() -> Option<GameWindowInfo> {
    if let Some(hwnd) = *SELECTED_HWND.lock().unwrap() {
        if let Some(info) = window_info_from_hwnd(HWND(hwnd as _)) {
            return Some(info);
        }
    }
    list_candidate_windows().into_iter().next()
}

pub fn list_candidate_windows() -> Vec<GameWindowInfo> {
    let pids: Vec<u32> = detect_optcgsim_processes()
        .into_iter()
        .filter_map(|p| p.process_id)
        .collect();

    let mut found = Vec::new();
    unsafe {
        let ctx = EnumContext {
            target_pids: pids,
            results: &mut found,
        };
        let _ = EnumWindows(Some(enum_callback), LPARAM(&ctx as *const _ as isize));
    }
    found
}

struct EnumContext<'a> {
    target_pids: Vec<u32>,
    results: &'a mut Vec<GameWindowInfo>,
}

unsafe extern "system" fn enum_callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let ctx = &mut *(lparam.0 as *mut EnumContext);
    if let Some(info) = try_window(hwnd, &ctx.target_pids) {
        ctx.results.push(info);
    }
    BOOL(1)
}

fn try_window(hwnd: HWND, target_pids: &[u32]) -> Option<GameWindowInfo> {
    use windows::Win32::System::Threading::GetProcessId;
    use windows::Win32::System::Threading::{
        GetWindowThreadProcessId, OpenProcess, PROCESS_QUERY_INFORMATION,
    };

    unsafe {
        if !IsWindowVisible(hwnd).as_bool() {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return None;
        }
        if !target_pids.is_empty() && !target_pids.contains(&pid) {
            // Also accept windows whose title mentions OPTCG when process list is empty
            let title = read_title(hwnd)?;
            if !title.to_ascii_lowercase().contains("optcg") {
                return None;
            }
        }
        window_info_from_hwnd(hwnd)
    }
}

fn read_title(hwnd: HWND) -> Option<String> {
    unsafe {
        let len = GetWindowTextLengthW(hwnd);
        if len == 0 {
            return None;
        }
        let mut buf = vec![0u16; (len + 1) as usize];
        GetWindowTextW(hwnd, &mut buf).ok()?;
        Some(
            String::from_utf16_lossy(&buf)
                .trim_matches('\0')
                .to_string(),
        )
    }
}

fn window_info_from_hwnd(hwnd: HWND) -> Option<GameWindowInfo> {
    unsafe {
        let mut rect = RECT::default();
        GetWindowRect(hwnd, &mut rect).ok()?;
        let width = (rect.right - rect.left).max(0) as u32;
        let height = (rect.bottom - rect.top).max(0) as u32;
        if width == 0 || height == 0 {
            return None;
        }
        let dpi = GetDpiForWindow(hwnd);
        let scale = if dpi > 0 { dpi as f32 / 96.0 } else { 1.0 };
        let title = read_title(hwnd).unwrap_or_else(|| "OPTCGSim".into());
        let mut pid = 0u32;
        windows::Win32::System::Threading::GetWindowThreadProcessId(hwnd, Some(&mut pid));
        Some(GameWindowInfo {
            process_id: pid,
            title,
            x: rect.left,
            y: rect.top,
            width,
            height,
            minimized: IsIconic(hwnd).as_bool(),
            visible: IsWindowVisible(hwnd).as_bool(),
            monitor_scale: scale,
            hwnd: hwnd.0 as u64,
        })
    }
}

pub fn refresh_window_info(info: &mut GameWindowInfo) -> ObsResult<bool> {
    if info.hwnd == 0 {
        return Ok(false);
    }
    let hwnd = HWND(info.hwnd as _);
    let updated = window_info_from_hwnd(hwnd)
        .ok_or_else(|| ObservationError::Unavailable("window no longer available".into()))?;
    *info = updated;
    debug!(title = %info.title, w = info.width, h = info.height, "window refreshed");
    Ok(info.is_capturable())
}
