//! Reads the board in saved pictures of the OPTCGSim window.
//!     cargo run -p optcg_observation --example macsim_board -- frame.png [more.png ...]
use optcg_observation::art::{load_image, ArtIndex};
use optcg_observation::macsim::board::{read_characters, read_hand, Side};
use std::path::PathBuf;

fn main() {
    let home = PathBuf::from(std::env::var("HOME").unwrap());
    let app = ["Downloads/1.43a_Mac", "Downloads/1.42c_Mac"].iter().map(|d| home.join(d).join("OPTCGSim.app")).find(|p| p.is_dir()).expect("OPTCGSim.app");
    let index = ArtIndex::build(&app.join("Contents/Resources/Data/StreamingAssets/Cards"));
    for path in std::env::args().skip(1) {
        let frame = load_image(&PathBuf::from(&path)).expect("picture");
        let started = std::time::Instant::now();
        let (n, hand) = read_hand(&frame, &index);
        let mine = read_characters(&frame, &index, Side::You);
        let theirs = read_characters(&frame, &index, Side::Opponent);
        let show = |row: &[Option<optcg_observation::macsim::board::SlotRead>]| row.iter().map(|s| s.as_ref().map(|r| format!("{}{}({})", r.card_id, if r.rested { "R" } else { "" }, r.distance)).unwrap_or("-".into())).collect::<Vec<_>>().join(" ");
        println!("{}  ({:.2}s)", path.rsplit('/').next().unwrap(), started.elapsed().as_secs_f32());
        println!("  hand {n}: {}", hand.iter().map(|c| c.clone().unwrap_or("?".into())).collect::<Vec<_>>().join(" "));
        println!("  your characters:     {}", show(&mine));
        println!("  opponent characters: {}", show(&theirs));
    }
}
