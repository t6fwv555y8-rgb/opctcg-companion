use super::info::GameWindowInfo;
use crate::process_detect::detect_optcgsim_processes;

pub fn is_available() -> bool {
    std::env::var("OPTCG_VISION_FIXTURE").is_ok()
}

pub fn discover_optcgsim_window() -> Option<GameWindowInfo> {
    if !is_available() {
        return None;
    }
    Some(fixture_window())
}

pub fn list_candidate_windows() -> Vec<GameWindowInfo> {
    discover_optcgsim_window().into_iter().collect()
}

fn fixture_window() -> GameWindowInfo {
    GameWindowInfo {
        process_id: detect_optcgsim_processes()
            .first()
            .and_then(|p| p.process_id)
            .unwrap_or(0),
        title: "OPTCGSim (fixture)".into(),
        x: 0,
        y: 0,
        width: 1920,
        height: 1080,
        minimized: false,
        visible: true,
        monitor_scale: 1.0,
        hwnd: 0,
    }
}
