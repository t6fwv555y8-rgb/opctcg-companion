mod discover;
mod info;

#[cfg(target_os = "windows")]
mod windows_impl;

#[cfg(not(target_os = "windows"))]
mod stub_impl;

pub use discover::{discover_optcgsim_window, list_candidate_windows};
pub use info::GameWindowInfo;

use crate::error::ObsResult;

/// Refresh window metadata (position, size, DPI, visibility).
pub fn refresh_window_info(info: &mut GameWindowInfo) -> ObsResult<bool> {
    #[cfg(target_os = "windows")]
    {
        return windows_impl::refresh_window_info(info);
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = info;
        Ok(stub_impl::is_available())
    }
}
