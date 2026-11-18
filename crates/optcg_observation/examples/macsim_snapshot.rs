//! Saves one picture of the OPTCGSim window, so the board layout can be studied.
//!
//!     cargo run -p optcg_observation --example macsim_snapshot -- out.png

use optcg_observation::macsim::{capture_window, find_sim_window, screen_access_granted};

fn main() {
    let out = std::env::args().nth(1).unwrap_or_else(|| "optcgsim.png".into());
    println!("screen capture allowed: {}", screen_access_granted());
    let Some(window) = find_sim_window() else {
        println!("no OPTCGSim window is on screen");
        std::process::exit(2);
    };
    println!(
        "window {} \"{}\" at ({}, {}) {}x{} points",
        window.window_number, window.title, window.x, window.y, window.width, window.height
    );
    match capture_window(&window) {
        Ok(frame) => {
            frame.image.save(&out).expect("could not save the picture");
            println!("saved {out}: {}x{} pixels (scale {})", frame.image.width(), frame.image.height(), frame.scale);
        }
        Err(e) => {
            println!("capture failed: {e}");
            std::process::exit(3);
        }
    }
}
