use super::info::GameWindowInfo;
use crate::process_detect::detect_optcgsim_processes;

#[cfg(target_os = "windows")]
use super::windows_impl;

#[cfg(not(target_os = "windows"))]
use super::stub_impl;

/// Discover the primary OPTCGSim game window.
pub fn discover_optcgsim_window() -> Option<GameWindowInfo> {
    #[cfg(target_os = "windows")]
    {
        windows_impl::discover_optcgsim_window()
    }
    #[cfg(not(target_os = "windows"))]
    {
        stub_impl::discover_optcgsim_window()
    }
}

/// List all candidate OPTCGSim windows for user selection.
pub fn list_candidate_windows() -> Vec<GameWindowInfo> {
    #[cfg(target_os = "windows")]
    {
        windows_impl::list_candidate_windows()
    }
    #[cfg(not(target_os = "windows"))]
    {
        stub_impl::list_candidate_windows()
    }
}

/// Whether any OPTCGSim process appears to be running.
pub fn process_running() -> bool {
    !detect_optcgsim_processes().is_empty()
}
