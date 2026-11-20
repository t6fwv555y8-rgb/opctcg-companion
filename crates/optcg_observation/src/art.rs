//! Recognising a card from its artwork.
//!
//! OPTCGSim draws cards from the image files it ships. This index hashes the
//! artwork of each of those images, so a picture of a card on screen (smaller,
//! softer, a little darker, maybe turned on its side) can be matched back to
//! the card it came from.
//!
//! The hash looks only at the illustration, not the frame, cost badge or text
//! box. Those look alike on every card and would make all cards seem similar.

use image::{imageops::FilterType, DynamicImage, GenericImageView};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// 16x16 difference hash: 256 bits.
pub type Hash = [u64; 4];

/// The illustration area of a card, as fractions of the card's width and height.
const ART_X: (f32, f32) = (0.05, 0.95);
const ART_Y: (f32, f32) = (0.04, 0.58);

/// The left part of the artwork, which stays visible when the next card in a
/// hand overlaps this one.
const LEFT_X: (f32, f32) = (0.05, 0.46);

/// Hash the illustration part of a card picture (any size, upright).
pub fn art_hash(card: &DynamicImage) -> Hash {
    hash_columns(card, ART_X)
}

/// Hash only the left part of the artwork (for a card partly covered by its neighbour).
pub fn left_hash(card: &DynamicImage) -> Hash {
    hash_columns(card, LEFT_X)
}

fn hash_columns(card: &DynamicImage, columns: (f32, f32)) -> Hash {
    let (w, h) = card.dimensions();
    let x0 = (columns.0 * w as f32) as u32;
    let x1 = (columns.1 * w as f32) as u32;
    let y0 = (ART_Y.0 * h as f32) as u32;
    let y1 = (ART_Y.1 * h as f32) as u32;
    let art = card.crop_imm(x0, y0, (x1 - x0).max(1), (y1 - y0).max(1));
    // 17 columns so each of the 16 columns has a right-hand neighbour to compare with.
    let small = art.resize_exact(17, 16, FilterType::Triangle).to_luma8();
    let mut hash = [0u64; 4];
    for y in 0..16u32 {
        for x in 0..16u32 {
            let left = small.get_pixel(x, y)[0];
            let right = small.get_pixel(x + 1, y)[0];
            if left > right {
                let bit = (y * 16 + x) as usize;
                hash[bit / 64] |= 1u64 << (bit % 64);
            }
        }
    }
    hash
}

/// Open a picture by what is inside the file, not its extension: some of the
/// simulator's ".png" files are really JPEGs.
pub fn load_image(path: &Path) -> Option<DynamicImage> {
    image::ImageReader::open(path)
        .ok()?
        .with_guessed_format()
        .ok()?
        .decode()
        .ok()
}

/// How much the artwork area varies (standard deviation of brightness). A flat
/// patch of table or mat has none, so it can never be a card.
pub fn art_contrast(card: &DynamicImage) -> f32 {
    let (w, h) = card.dimensions();
    let art = card.crop_imm(
        (ART_X.0 * w as f32) as u32,
        (ART_Y.0 * h as f32) as u32,
        (((ART_X.1 - ART_X.0) * w as f32) as u32).max(1),
        (((ART_Y.1 - ART_Y.0) * h as f32) as u32).max(1),
    );
    let gray = art.resize_exact(24, 24, FilterType::Triangle).to_luma8();
    let values: Vec<f32> = gray.pixels().map(|p| p[0] as f32).collect();
    let mean = values.iter().sum::<f32>() / values.len() as f32;
    (values.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / values.len() as f32).sqrt()
}

/// Below this a picture is too flat to be a card's artwork.
pub const MIN_CONTRAST: f32 = 14.0;

pub fn hamming(a: &Hash, b: &Hash) -> u32 {
    a.iter().zip(b).map(|(x, y)| (x ^ y).count_ones()).sum()
}

/// A best guess for what a picture shows, with how far off it was.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    pub card_id: String,
    /// Bits that differ out of 256. Lower is closer.
    pub distance: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ArtIndex {
    ids: Vec<String>,
    hashes: Vec<Hash>,
    /// Hash of the left part of the artwork, for cards seen partly covered.
    #[serde(default)]
    left: Vec<Hash>,
}

/// "OP17-081_small" -> "OP17-081"; anything that is not a card id -> None.
pub fn card_id_from_stem(stem: &str) -> Option<String> {
    let base = stem.strip_suffix("_small").unwrap_or(stem).to_uppercase();
    let (set, number) = base.split_once('-')?;
    let letters = set.chars().take_while(|c| c.is_ascii_alphabetic()).count();
    let ok = (1..=4).contains(&letters)
        && set[letters..].chars().all(|c| c.is_ascii_digit())
        && !number.is_empty()
        && number.chars().all(|c| c.is_ascii_digit());
    ok.then_some(base)
}

/// Every card id written in a piece of text ("Sanji [OP17-082]" -> "OP17-082").
pub fn card_id_candidates(text: &str) -> Vec<String> {
    text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
        .filter_map(|token| {
            let upper = token.to_uppercase();
            let (set, number) = upper.split_once('-')?;
            let letters = set.chars().take_while(|c| c.is_ascii_uppercase()).count();
            let ok = (1..=3).contains(&letters)
                && set[letters..].chars().all(|c| c.is_ascii_digit())
                && set.len() - letters <= 2
                && number.len() == 3
                && number.chars().all(|c| c.is_ascii_digit());
            ok.then_some(upper)
        })
        .collect()
}

impl ArtIndex {
    pub fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    pub fn insert(&mut self, card_id: &str, hash: Hash) {
        self.insert_with_left(card_id, hash, hash);
    }

    pub fn insert_with_left(&mut self, card_id: &str, hash: Hash, left: Hash) {
        self.ids.push(card_id.to_string());
        self.hashes.push(hash);
        self.left.push(left);
    }

    /// Hash every card image under `cards_dir` (the simulator's `Cards` folder).
    ///
    /// Each card is hashed from its small picture when there is one: it is far
    /// quicker to read and on screen cards are small anyway. Work is spread
    /// across the computer's cores.
    pub fn build(cards_dir: &Path) -> Self {
        let mut best: std::collections::BTreeMap<String, (u64, PathBuf)> = std::collections::BTreeMap::new();
        let mut stack = vec![cards_dir.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else { continue };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                let is_image = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "jpg" | "jpeg" | "png"));
                let Some(id) = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .and_then(card_id_from_stem)
                    .filter(|_| is_image)
                else {
                    continue;
                };
                // Smaller file wins: prefer the "_small" picture, and never a huge one.
                let size = entry.metadata().map(|m| m.len()).unwrap_or(u64::MAX);
                let is_small = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .is_some_and(|s| s.ends_with("_small"));
                let rank = if is_small { 0 } else { size.saturating_add(1) };
                match best.get(&id) {
                    Some((old, _)) if *old <= rank => {}
                    _ => {
                        best.insert(id, (rank, path));
                    }
                }
            }
        }
        let work: Vec<(String, PathBuf)> = best.into_iter().map(|(id, (_, p))| (id, p)).collect();
        let threads = std::thread::available_parallelism().map_or(2, |n| n.get()).min(8);
        let chunk = work.len().div_ceil(threads).max(1);
        let mut hashed: Vec<(String, Hash, Hash)> = std::thread::scope(|scope| {
            let handles: Vec<_> = work
                .chunks(chunk)
                .map(|part| {
                    scope.spawn(move || {
                        part.iter()
                            .filter_map(|(id, path)| {
                                load_image(path).map(|img| (id.clone(), art_hash(&img), left_hash(&img)))
                            })
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            handles.into_iter().flat_map(|h| h.join().unwrap_or_default()).collect()
        });
        hashed.sort_by(|a, b| a.0.cmp(&b.0));
        let mut index = ArtIndex::default();
        for (id, hash, left) in hashed {
            index.insert_with_left(&id, hash, left);
        }
        index
    }

    /// The closest cards, nearest first.
    pub fn nearest(&self, hash: &Hash, count: usize) -> Vec<Match> {
        let mut all: Vec<(u32, usize)> = self
            .hashes
            .iter()
            .enumerate()
            .map(|(i, h)| (hamming(hash, h), i))
            .collect();
        all.sort_unstable();
        all.into_iter()
            .take(count)
            .map(|(distance, i)| Match { card_id: self.ids[i].clone(), distance })
            .collect()
    }

    /// Like [`nearest`](Self::nearest) but comparing only the left part of the
    /// artwork, for a card whose right side is covered.
    pub fn nearest_left(&self, hash: &Hash, count: usize) -> Vec<Match> {
        let mut all: Vec<(u32, usize)> = self
            .left
            .iter()
            .enumerate()
            .map(|(i, h)| (hamming(hash, h), i))
            .collect();
        all.sort_unstable();
        all.into_iter()
            .take(count)
            .map(|(distance, i)| Match { card_id: self.ids[i].clone(), distance })
            .collect()
    }

    /// The match, only if it is close and clearly better than the runner-up.
    pub fn identify(&self, hash: &Hash) -> Option<Match> {
        let top = self.nearest(hash, 2);
        let best = top.first()?;
        let margin_ok = top.get(1).map_or(true, |second| second.distance >= best.distance + MIN_MARGIN);
        (best.distance <= MAX_DISTANCE && margin_ok).then(|| best.clone())
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, serde_json::to_vec(self)?)
    }

    pub fn load(path: &Path) -> Option<Self> {
        serde_json::from_slice(&std::fs::read(path).ok()?).ok()
    }
}

/// Accept a match only this close (of 256 bits) ...
pub const MAX_DISTANCE: u32 = 70;
/// ... and only when the runner-up is at least this much further away.
pub const MIN_MARGIN: u32 = 8;

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};

    fn synthetic(seed: u32) -> DynamicImage {
        // A "card": coloured frame, textured art area, plain text box.
        let mut img = RgbImage::from_pixel(240, 336, Rgb([20, 20, 20]));
        for y in 0..336u32 {
            for x in 0..240u32 {
                if (10..230).contains(&x) && (14..195).contains(&y) {
                    let v = ((x * (seed % 7 + 3) + y * (seed % 5 + 2) + (x * y) / (seed % 11 + 5)) % 256) as u8;
                    img.put_pixel(x, y, Rgb([v, v.wrapping_mul(3), v.wrapping_add(seed as u8)]));
                }
            }
        }
        DynamicImage::ImageRgb8(img)
    }

    #[test]
    fn ids_are_read_from_file_names_and_other_files_are_ignored() {
        assert_eq!(card_id_from_stem("OP17-081_small").as_deref(), Some("OP17-081"));
        assert_eq!(card_id_from_stem("st01-002").as_deref(), Some("ST01-002"));
        assert_eq!(card_id_from_stem("P-088").as_deref(), Some("P-088"));
        assert_eq!(card_id_from_stem("Don"), None);
        assert_eq!(card_id_from_stem("background"), None);
    }

    #[test]
    fn the_same_picture_matches_itself_at_zero_distance() {
        let img = synthetic(3);
        assert_eq!(hamming(&art_hash(&img), &art_hash(&img)), 0);
    }

    #[test]
    fn a_shrunk_and_dimmed_copy_still_finds_its_card() {
        let mut index = ArtIndex::default();
        for seed in 1..40 {
            index.insert(&format!("T-{seed:03}"), art_hash(&synthetic(seed)));
        }
        let small = synthetic(17)
            .resize_exact(72, 100, FilterType::Triangle)
            .brighten(-12);
        let found = index.identify(&art_hash(&small)).expect("a confident match");
        assert_eq!(found.card_id, "T-017");
    }

    #[test]
    fn an_unrelated_picture_is_not_forced_into_a_match() {
        let mut index = ArtIndex::default();
        for seed in 1..10 {
            index.insert(&format!("T-{seed:03}"), art_hash(&synthetic(seed)));
        }
        let noise = DynamicImage::ImageRgb8(RgbImage::from_fn(120, 160, |x, y| {
            let v = ((x * 97 + y * 193 + x * y * 31) % 251) as u8;
            Rgb([v, v ^ 0x55, v.wrapping_mul(7)])
        }));
        assert!(index.identify(&art_hash(&noise)).is_none());
    }
}
