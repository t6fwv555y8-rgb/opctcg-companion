//! Measures how well the artwork matcher recognises cards, using the card
//! images that ship inside OPTCGSim. Skipped when the simulator is not installed.

use image::{imageops::FilterType, DynamicImage};
use optcg_observation::art::{art_hash, ArtIndex};
use std::path::PathBuf;

fn cards_dir() -> Option<PathBuf> {
    let candidates = [
        std::env::var("OPTCG_SIM_APP").ok().map(PathBuf::from),
        dirs_home().map(|h| h.join("Downloads/1.42c_Mac/OPTCGSim.app")),
        Some(PathBuf::from("/Applications/OPTCGSim.app")),
    ];
    candidates
        .into_iter()
        .flatten()
        .map(|app| app.join("Contents/Resources/Data/StreamingAssets/Cards"))
        .find(|p| p.is_dir())
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(PathBuf::from)
}

#[test]
fn real_card_art_is_recognised_after_the_distortions_seen_on_screen() {
    let Some(dir) = cards_dir() else {
        eprintln!("OPTCGSim not installed here; skipping");
        return;
    };
    let started = std::time::Instant::now();
    let index = ArtIndex::build(&dir);
    eprintln!("indexed {} cards in {:.1}s", index.len(), started.elapsed().as_secs_f32());
    assert!(index.len() > 2000, "expected the full card set, got {}", index.len());

    // Re-open a spread of the cards and try to recognise each after distortion.
    let mut files: Vec<PathBuf> = Vec::new();
    let mut stack = vec![dir.clone()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "jpg" || x == "png")
                && !p.file_stem().unwrap().to_string_lossy().ends_with("_small")
                && optcg_observation::art::card_id_from_stem(&p.file_stem().unwrap().to_string_lossy()).is_some()
            {
                files.push(p);
            }
        }
    }
    files.sort();
    let sample: Vec<&PathBuf> = files.iter().step_by((files.len() / 300).max(1)).collect();

    struct Tally { name: &'static str, right: usize, wrong: usize, declined: usize }
    let tests: Vec<(&'static str, Box<dyn Fn(&DynamicImage) -> DynamicImage>)> = vec![
        ("shrunk to 90x126", Box::new(|i| i.resize_exact(90, 126, FilterType::Triangle))),
        ("shrunk + blurred", Box::new(|i| i.resize_exact(100, 140, FilterType::Triangle).blur(1.2))),
        ("shrunk + dimmed 15%", Box::new(|i| i.resize_exact(100, 140, FilterType::Triangle).brighten(-38))),
        ("tiny 60x84", Box::new(|i| i.resize_exact(60, 84, FilterType::Triangle))),
        ("crop off by 3%", Box::new(|i| {
            let (w, h) = (i.width(), i.height());
            i.crop_imm(w * 3 / 100, h * 3 / 100, w * 94 / 100, h * 94 / 100).resize_exact(100, 140, FilterType::Triangle)
        })),
    ];
    for (name, f) in &tests {
        let mut t = Tally { name, right: 0, wrong: 0, declined: 0 };
        for path in &sample {
            let truth = optcg_observation::art::card_id_from_stem(&path.file_stem().unwrap().to_string_lossy()).unwrap();
            let Some(img) = optcg_observation::art::load_image(path) else { continue };
            match index.identify(&art_hash(&f(&img))) {
                Some(m) if m.card_id == truth => t.right += 1,
                Some(_) => t.wrong += 1,
                None => t.declined += 1,
            }
        }
        let n = sample.len() as f32;
        eprintln!("{:<22} right {:>5.1}%  wrong {:>4.1}%  declined {:>4.1}%", t.name, t.right as f32 / n * 100.0, t.wrong as f32 / n * 100.0, t.declined as f32 / n * 100.0);
        assert!(t.wrong as f32 / n < 0.05, "{}: too many confident mistakes", t.name);
    }
}
