/// A top-level window of the simulator.
#[derive(Debug, Clone, PartialEq)]
pub struct SimWindow {
    /// The system's id for the window (what `screencapture -l` takes).
    pub window_number: u32,
    pub owner_pid: i32,
    pub title: String,
    /// Position and size in points (not pixels: a Retina screen has 2 pixels per point).
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// What to tell the player when macOS will not let the app see the game window.
pub const SCREEN_ACCESS_HELP: &str = "macOS has not allowed screen capture yet. Open System Settings > \
Privacy & Security > Screen & System Audio Recording, turn on the app that runs this HUD, then restart it.";

#[cfg(target_os = "macos")]
mod mac {
    use super::SimWindow;
    use core_foundation::base::{CFType, TCFType};
    use core_foundation::boolean::CFBoolean;
    use core_foundation::dictionary::CFDictionary;
    use core_foundation::number::CFNumber;
    use core_foundation::string::CFString;
    use core_graphics::window::{
        copy_window_info, kCGNullWindowID, kCGWindowListExcludeDesktopElements,
        kCGWindowListOptionOnScreenOnly,
    };

    extern "C" {
        fn CGPreflightScreenCaptureAccess() -> bool;
        fn CGRequestScreenCaptureAccess() -> bool;
    }

    pub fn screen_access_granted() -> bool {
        // SAFETY: a plain system call with no arguments.
        unsafe { CGPreflightScreenCaptureAccess() }
    }

    /// Ask macOS to show its permission prompt (it only ever shows once).
    pub fn request_screen_access() -> bool {
        // SAFETY: a plain system call with no arguments.
        unsafe { CGRequestScreenCaptureAccess() }
    }

    fn number(dict: &CFDictionary<CFString, CFType>, key: &str) -> Option<f64> {
        dict.find(CFString::new(key))
            .and_then(|v| v.downcast::<CFNumber>())
            .and_then(|n| n.to_f64())
    }

    fn string(dict: &CFDictionary<CFString, CFType>, key: &str) -> Option<String> {
        dict.find(CFString::new(key))
            .and_then(|v| v.downcast::<CFString>())
            .map(|s| s.to_string())
    }

    /// Every on-screen window owned by OPTCGSim, biggest first.
    pub fn sim_windows() -> Vec<SimWindow> {
        let options = kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements;
        let Some(list) = copy_window_info(options, kCGNullWindowID) else {
            return Vec::new();
        };
        let mut found = Vec::new();
        for item in list.iter() {
            // SAFETY: each element of the list is a CFDictionary of window properties.
            let dict: CFDictionary<CFString, CFType> =
                unsafe { CFDictionary::wrap_under_get_rule(*item as _) };
            let owner = string(&dict, "kCGWindowOwnerName").unwrap_or_default();
            if !owner.to_lowercase().contains("optcgsim") {
                continue;
            }
            // Layer 0 is a normal window; menus and overlays sit on other layers.
            if number(&dict, "kCGWindowLayer").unwrap_or(0.0) != 0.0 {
                continue;
            }
            let _ = dict
                .find(CFString::new("kCGWindowIsOnscreen"))
                .and_then(|v| v.downcast::<CFBoolean>());
            let Some(bounds) = dict
                .find(CFString::new("kCGWindowBounds"))
                .and_then(|v| v.downcast::<CFDictionary>())
            else {
                continue;
            };
            let bounds: CFDictionary<CFString, CFType> =
                unsafe { CFDictionary::wrap_under_get_rule(bounds.as_concrete_TypeRef()) };
            let (Some(x), Some(y), Some(width), Some(height)) = (
                number(&bounds, "X"),
                number(&bounds, "Y"),
                number(&bounds, "Width"),
                number(&bounds, "Height"),
            ) else {
                continue;
            };
            let Some(window_number) = number(&dict, "kCGWindowNumber") else {
                continue;
            };
            found.push(SimWindow {
                window_number: window_number as u32,
                owner_pid: number(&dict, "kCGWindowOwnerPID").unwrap_or(0.0) as i32,
                title: string(&dict, "kCGWindowName").unwrap_or_default(),
                x,
                y,
                width,
                height,
            });
        }
        found.sort_by(|a, b| (b.width * b.height).partial_cmp(&(a.width * a.height)).unwrap_or(std::cmp::Ordering::Equal));
        found
    }
}

/// The simulator's main window, if it is open and on screen.
pub fn find_sim_window() -> Option<SimWindow> {
    #[cfg(target_os = "macos")]
    {
        mac::sim_windows().into_iter().find(|w| w.width >= 400.0 && w.height >= 300.0)
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

/// Whether macOS currently lets this app capture other windows.
pub fn screen_access_granted() -> bool {
    #[cfg(target_os = "macos")]
    {
        mac::screen_access_granted()
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

/// Show macOS's permission prompt (it appears at most once per app).
pub fn request_screen_access() -> bool {
    #[cfg(target_os = "macos")]
    {
        mac::request_screen_access()
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}
