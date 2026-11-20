//! Watching OPTCGSim on a Mac.
//!
//! OPTCGSim has no web page and no live log, so the only way to follow a game
//! as it happens is to look at its window. This module finds that window,
//! captures it, and (in later stages) reads the cards and numbers on the
//! board. Nothing here touches the game itself: it only looks at pixels the
//! player can already see.

pub mod board;
mod capture;
pub mod livelog;
pub mod ocr;
mod reader;
mod window;

pub use capture::{capture_window, CaptureError, Frame};
pub use reader::{Poll, ReaderStatus, Regions, SimReader};
pub use window::{
    find_sim_window, request_screen_access, screen_access_granted, SimWindow, SCREEN_ACCESS_HELP,
};
