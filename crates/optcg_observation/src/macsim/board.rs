//! Reading the cards on the simulator's board from a picture of its window.
//!
//! Positions are fixed fractions of the window picture, measured on the
//! 2048x1015 window. Each card is recognised from its artwork (see
//! [`crate::art`]), trying a few small position shifts because a hash is
//! sensitive to a card being a pixel or two off.

use crate::art::{art_hash, left_hash, ArtIndex, Match};
use image::DynamicImage;

/// Measured on a 2000x991 view of the window; everything is converted to fractions.
const VIEW_W: f32 = 2000.0;
const VIEW_H: f32 = 991.0;

/// Card size on screen (view units).
const CARD_W: f32 = 89.0;
const CARD_H: f32 = 122.0;

/// How far a card may sit from where we expect it, in view units.
const JITTER_X: [f32; 9] = [-6.0, -4.0, -2.0, 0.0, 2.0, 4.0, 6.0, 8.0, 10.0];
const JITTER_Y: [f32; 3] = [-3.0, 0.0, 3.0];
/// A quick pass uses only these few shifts.
const COARSE_X: [f32; 5] = [-4.0, -2.0, 0.0, 2.0, 4.0];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    You,
    Opponent,
}

/// What was recognised in one place on the board.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotRead {
    pub card_id: String,
    /// Turned on its side (used this turn).
    pub rested: bool,
    pub distance: u32,
}

struct View<'a> {
    image: &'a DynamicImage,
}

impl View<'_> {
    /// A piece of the picture, in view units.
    fn crop(&self, x: f32, y: f32, w: f32, h: f32) -> DynamicImage {
        let (fw, fh) = (self.image.width() as f32, self.image.height() as f32);
        let x0 = (x / VIEW_W * fw).max(0.0) as u32;
        let y0 = (y / VIEW_H * fh).max(0.0) as u32;
        let w = ((w / VIEW_W * fw) as u32).min(self.image.width().saturating_sub(x0)).max(1);
        let h = ((h / VIEW_H * fh) as u32).min(self.image.height().saturating_sub(y0)).max(1);
        self.image.crop_imm(x0, y0, w, h)
    }

    /// Best match for a card whose expected top-left corner is (x, y).
    fn best_card(&self, index: &ArtIndex, x: f32, y: f32, partly_covered: bool) -> Option<Match> {
        self.best_card_within(index, x, y, partly_covered, crate::art::MAX_DISTANCE)
    }

    /// As [`best_card`](Self::best_card), accepting only matches within `limit` bits.
    fn best_card_within(&self, index: &ArtIndex, x: f32, y: f32, partly_covered: bool, limit: u32) -> Option<Match> {
        self.search(index, x, y, partly_covered, limit, &JITTER_X, &JITTER_Y)
    }

    /// Search a few positions around (x, y) and keep the closest match.
    #[allow(clippy::too_many_arguments)]
    fn search(&self, index: &ArtIndex, x: f32, y: f32, partly_covered: bool, limit: u32, xs: &[f32], ys: &[f32]) -> Option<Match> {
        let mut best: Option<Match> = None;
        for &dy in ys {
            for &dx in xs {
                let card = self.crop(x + dx, y + dy, CARD_W, CARD_H);
                if crate::art::art_contrast(&card) < crate::art::MIN_CONTRAST {
                    continue; // bare table or mat, not a card
                }
                let hash = if partly_covered { left_hash(&card) } else { art_hash(&card) };
                let found = if partly_covered { index.nearest_left(&hash, 2) } else { index.nearest(&hash, 2) };
                let Some(top) = found.first() else { continue };
                let margin_ok = found.get(1).map_or(true, |second| second.distance >= top.distance + crate::art::MIN_MARGIN);
                if top.distance <= limit && margin_ok && best.as_ref().map_or(true, |b| top.distance < b.distance) {
                    best = Some(top.clone());
                }
            }
        }
        best
    }
}

/// Your hand, left to right, and how many cards it holds.
///
/// The row is a fixed width, so cards overlap when there are many. The number
/// of cards is found by trying each count and keeping the one whose cards all
/// look most like real cards.
pub fn read_hand(frame: &DynamicImage, index: &ArtIndex) -> (usize, Vec<Option<String>>) {
    const LEFT: f32 = 175.0;
    const ROW: f32 = 445.0;
    const TOP: f32 = 831.0;
    let view = View { image: frame };

    let mut best: Option<(f32, usize, Vec<Option<String>>)> = None;
    for n in 1..=12usize {
        let pitch = if n == 1 { 0.0 } else { CARD_W.min((ROW - CARD_W) / (n as f32 - 1.0)) };
        // A card needs about half its width showing to be recognised.
        if n > 1 && pitch < CARD_W * 0.45 {
            continue;
        }
        let mut total = 0.0;
        let mut ids = Vec::with_capacity(n);
        let mut recognised = 0;
        for i in 0..n {
            let covered = i + 1 < n && pitch < CARD_W * 0.98;
            // A quick look first: the exact position matters little for picking the count.
            let found = view.search(index, LEFT + pitch * i as f32, TOP, covered, crate::art::MAX_DISTANCE, &COARSE_X, &[0.0]);
            total += found.as_ref().map_or(110.0, |m| m.distance as f32);
            if found.is_some() {
                recognised += 1;
            }
            ids.push(found.map(|m| m.card_id));
        }
        let mut score = total / n as f32;
        // Prefer a count where most cards were recognised, then the lowest distance.
        score -= recognised as f32 * 0.5;
        // If a card is still visible just past the last one, the hand is bigger
        // than this guess.
        if n < 12 {
            let probe_pitch = if n == 1 { CARD_W } else { pitch };
            if view.search(index, LEFT + probe_pitch * n as f32, TOP, false, 52, &COARSE_X, &[0.0]).is_some() {
                score += 60.0;
            }
        }
        if best.as_ref().map_or(true, |b| score < b.0) {
            best = Some((score, n, ids));
        }
    }
    let Some((_, n, coarse)) = best.filter(|b| b.2.iter().any(Option::is_some)) else {
        return (0, Vec::new());
    };
    // Now that the count is known, look properly at each card.
    let pitch = if n == 1 { 0.0 } else { CARD_W.min((ROW - CARD_W) / (n as f32 - 1.0)) };
    let ids = (0..n)
        .map(|i| {
            let covered = i + 1 < n && pitch < CARD_W * 0.98;
            view.best_card(index, LEFT + pitch * i as f32, TOP, covered)
                .map(|m| m.card_id)
                .or_else(|| coarse[i].clone())
        })
        .collect();
    (n, ids)
}

/// Where a side's five Character slots are (view units): the left edge of the
/// first slot, the distance between slots, and the top of the cards.
fn character_layout(side: Side) -> (f32, f32, f32, f32) {
    // (first slot's card left edge, step to the next slot, card top, direction)
    match side {
        // You fill from the left.
        Side::You => (777.0, 107.0, 528.0, 1.0),
        // The opponent's side is turned around: they fill from the right.
        Side::Opponent => (1139.0, 107.0, 368.0, -1.0),
    }
}

/// The Characters on one side, by slot (empty slots are `None`).
pub fn read_characters(frame: &DynamicImage, index: &ArtIndex, side: Side) -> Vec<Option<SlotRead>> {
    let (first, step, top, dir) = character_layout(side);
    let view = View { image: frame };
    (0..5)
        .map(|slot| {
            let x = first + dir * step * slot as f32;
            if let Some(m) = view.best_card(index, x, top, false) {
                return Some(SlotRead { card_id: m.card_id, rested: false, distance: m.distance });
            }
            // A card turned on its side occupies a landscape box around the slot centre.
            let (cx, cy) = (x + CARD_W / 2.0, top + CARD_H / 2.0);
            let landscape = view.crop(cx - CARD_H / 2.0, cy - CARD_W / 2.0, CARD_H, CARD_W);
            [landscape.rotate90(), landscape.rotate270()]
                .iter()
                .filter_map(|upright| {
                    let found = index.nearest(&art_hash(upright), 2);
                    let top = found.first()?;
                    let margin_ok = found.get(1).map_or(true, |s| s.distance >= top.distance + crate::art::MIN_MARGIN);
                    (top.distance <= crate::art::MAX_DISTANCE && margin_ok).then(|| top.clone())
                })
                .min_by_key(|m| m.distance)
                .map(|m| SlotRead { card_id: m.card_id, rested: true, distance: m.distance })
        })
        .collect()
}

/// A tiny fingerprint of a region of the picture, to tell cheaply whether it changed.
pub fn region_signature(frame: &DynamicImage, x: f32, y: f32, w: f32, h: f32) -> Vec<u8> {
    let (fw, fh) = (frame.width() as f32, frame.height() as f32);
    let piece = frame.crop_imm(
        (x / VIEW_W * fw) as u32,
        (y / VIEW_H * fh) as u32,
        ((w / VIEW_W * fw) as u32).max(1),
        ((h / VIEW_H * fh) as u32).max(1),
    );
    piece.resize_exact(24, 8, image::imageops::FilterType::Triangle).to_luma8().into_raw()
}

/// How different two signatures are (average brightness change per point).
pub fn signature_change(a: &[u8], b: &[u8]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return f32::MAX;
    }
    a.iter().zip(b).map(|(x, y)| (*x as f32 - *y as f32).abs()).sum::<f32>() / a.len() as f32
}

/// Where each part of the board is, for change detection (x, y, w, h in view units).
pub const HAND_AREA: (f32, f32, f32, f32) = (170.0, 825.0, 460.0, 135.0);
pub const YOUR_CHARACTERS_AREA: (f32, f32, f32, f32) = (765.0, 520.0, 545.0, 140.0);
pub const OPPONENT_CHARACTERS_AREA: (f32, f32, f32, f32) = (690.0, 360.0, 550.0, 140.0);

#[cfg(test)]
mod tests {
    use super::*;
    use image::{imageops, Rgba, RgbaImage};
    use std::path::PathBuf;

    fn cards_dir() -> Option<PathBuf> {
        let home = PathBuf::from(std::env::var("HOME").ok()?);
        ["Downloads/1.43a_Mac", "Downloads/1.42c_Mac"]
            .iter()
            .map(|d| home.join(d).join("OPTCGSim.app/Contents/Resources/Data/StreamingAssets/Cards"))
            .find(|p| p.is_dir())
    }

    /// A fake window picture with real cards pasted where the simulator draws them.
    fn paint(cards: &[(&str, f32, f32)], dir: &std::path::Path) -> DynamicImage {
        let mut canvas = RgbaImage::from_pixel(2000, 991, Rgba([150, 90, 40, 255]));
        for (id, x, y) in cards {
            let path = ["jpg", "png"]
                .iter()
                .flat_map(|ext| {
                    std::fs::read_dir(dir).unwrap().flatten().map(move |e| e.path().join(format!("{id}_small.{ext}")))
                })
                .find(|p| p.exists())
                .or_else(|| {
                    std::fs::read_dir(dir).unwrap().flatten().map(|e| e.path().join(format!("{id}.jpg"))).find(|p| p.exists())
                })
                .expect("card art present");
            let card = crate::art::load_image(&path).unwrap().resize_exact(CARD_W as u32, CARD_H as u32, image::imageops::FilterType::Triangle);
            imageops::overlay(&mut canvas, &card.to_rgba8(), *x as i64, *y as i64);
        }
        DynamicImage::ImageRgba8(canvas)
    }

    #[test]
    fn an_overlapped_hand_is_counted_and_read() {
        let Some(dir) = cards_dir() else { return };
        let index = ArtIndex::build(&dir);
        let hand = ["OP17-081", "OP17-082", "OP17-091", "OP17-087", "OP17-080", "OP17-080"];
        let pitch = (445.0 - CARD_W) / 5.0;
        let placed: Vec<(&str, f32, f32)> = hand.iter().enumerate().map(|(i, id)| (*id, 173.0 + pitch * i as f32, 832.0)).collect();
        let (n, read) = read_hand(&paint(&placed, &dir), &index);
        assert_eq!(n, 6);
        let ids: Vec<_> = read.iter().map(|r| r.clone().unwrap_or_default()).collect();
        assert_eq!(ids, hand.iter().map(|s| s.to_string()).collect::<Vec<_>>());
    }

    #[test]
    fn a_short_hand_without_overlap_is_read_too() {
        let Some(dir) = cards_dir() else { return };
        let index = ArtIndex::build(&dir);
        let hand = ["OP17-082", "OP17-080", "OP17-087"];
        let placed: Vec<_> = hand.iter().enumerate().map(|(i, id)| (*id, 175.0 + 89.0 * i as f32, 831.0)).collect();
        let (n, read) = read_hand(&paint(&placed, &dir), &index);
        assert_eq!(n, 3);
        assert_eq!(read.iter().flatten().count(), 3);
    }

    #[test]
    fn characters_are_read_by_slot_and_empty_slots_stay_empty() {
        let Some(dir) = cards_dir() else { return };
        let index = ArtIndex::build(&dir);
        let canvas = paint(&[("OP17-082", 778.0, 528.0), ("OP17-080", 777.0 + 214.0, 529.0)], &dir);
        let row = read_characters(&canvas, &index, Side::You);
        assert_eq!(row[0].as_ref().map(|r| r.card_id.as_str()), Some("OP17-082"));
        assert!(row[1].is_none(), "nothing in slot 1");
        assert_eq!(row[2].as_ref().map(|r| r.card_id.as_str()), Some("OP17-080"));
        assert!(row[3].is_none() && row[4].is_none());
    }

    #[test]
    fn signatures_notice_a_card_appearing_but_not_noise() {
        let flat = DynamicImage::ImageRgba8(RgbaImage::from_pixel(2000, 991, Rgba([150, 90, 40, 255])));
        let a = region_signature(&flat, YOUR_CHARACTERS_AREA.0, YOUR_CHARACTERS_AREA.1, YOUR_CHARACTERS_AREA.2, YOUR_CHARACTERS_AREA.3);
        let mut noisy = flat.to_rgba8();
        noisy.put_pixel(900, 560, Rgba([151, 90, 40, 255]));
        let b = region_signature(&DynamicImage::ImageRgba8(noisy), YOUR_CHARACTERS_AREA.0, YOUR_CHARACTERS_AREA.1, YOUR_CHARACTERS_AREA.2, YOUR_CHARACTERS_AREA.3);
        assert!(signature_change(&a, &b) < 1.0);
        let mut with_card = flat.to_rgba8();
        for y in 530..650 { for x in 780..870 { with_card.put_pixel(x, y, Rgba([20, 20, 20, 255])); } }
        let c = region_signature(&DynamicImage::ImageRgba8(with_card), YOUR_CHARACTERS_AREA.0, YOUR_CHARACTERS_AREA.1, YOUR_CHARACTERS_AREA.2, YOUR_CHARACTERS_AREA.3);
        assert!(signature_change(&a, &c) > 3.0);
    }

    #[test]
    fn an_empty_board_reads_as_empty() {
        let canvas = DynamicImage::ImageRgba8(RgbaImage::from_pixel(2000, 991, Rgba([150, 90, 40, 255])));
        let index = ArtIndex::default();
        assert!(read_characters(&canvas, &index, Side::Opponent).iter().all(Option::is_none));
        assert_eq!(read_hand(&canvas, &index), (0, Vec::new()));
    }
}
