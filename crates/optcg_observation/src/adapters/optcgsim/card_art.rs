use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

/// Simple 64-bit perceptual hash for card art matching.
pub fn dhash64(image_bytes: &[u8]) -> u64 {
    // Placeholder hash from file bytes — sufficient for fixture/index tests.
    // Production path would decode PNG and compare resized grayscale pixels.
    let mut hash: u64 = 0;
    for (i, b) in image_bytes.iter().take(64).enumerate() {
        if *b as u64 > hash.wrapping_shr(8) {
            hash = hash.wrapping_shl(1) | 1;
        } else {
            hash = hash.wrapping_shl(1);
        }
        let _ = i;
    }
    hash
}

/// Read-only index of OPTCGSim StreamingAssets card images.
#[derive(Debug, Clone, Default)]
pub struct CardArtIndex {
    by_card_id: HashMap<String, PathBuf>,
    by_fingerprint: HashMap<u64, String>,
}

impl CardArtIndex {
    pub fn build_from_streaming_assets(streaming_assets: &Path) -> Self {
        let mut index = Self::default();
        let cards_dir = streaming_assets.join("Cards");
        let search_roots = if cards_dir.is_dir() {
            vec![cards_dir]
        } else {
            vec![streaming_assets.to_path_buf()]
        };

        for root in search_roots {
            index.scan_dir(&root, 4);
        }

        debug!(
            cards = index.by_card_id.len(),
            fingerprints = index.by_fingerprint.len(),
            "card art index built"
        );
        index
    }

    fn scan_dir(&mut self, dir: &Path, depth: u32) {
        if depth == 0 || !dir.is_dir() {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten().take(512) {
            let path = entry.path();
            if path.is_dir() {
                self.scan_dir(&path, depth - 1);
                continue;
            }
            let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
                continue;
            };
            if !matches!(ext.to_ascii_lowercase().as_str(), "png" | "jpg" | "webp") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let card_id = normalize_card_filename(stem);
            if card_id.is_empty() {
                continue;
            }
            self.by_card_id.insert(card_id.clone(), path.clone());
            if let Ok(bytes) = std::fs::read(&path) {
                let fp = dhash64(&bytes);
                self.by_fingerprint.insert(fp, card_id);
            }
        }
    }

    pub fn card_path(&self, card_id: &str) -> Option<&Path> {
        self.by_card_id.get(card_id).map(|p| p.as_path())
    }

    pub fn match_fingerprint(&self, fingerprint: u64) -> Option<&str> {
        self.by_fingerprint.get(&fingerprint).map(|s| s.as_str())
    }

    pub fn len(&self) -> usize {
        self.by_card_id.len()
    }
}

fn normalize_card_filename(stem: &str) -> String {
    let upper = stem.replace('_', "-").to_uppercase();
    if upper.starts_with("OP")
        || upper.starts_with("ST")
        || upper.starts_with("EB")
        || upper.starts_with("P-")
    {
        upper
    } else {
        String::new()
    }
}

/// Optional helper to build cache dir for persisted fingerprints (read-only source files).
pub fn build_cache_index(
    streaming_assets: &Path,
    cache_dir: &Path,
) -> Result<CardArtIndex, String> {
    let index = CardArtIndex::build_from_streaming_assets(streaming_assets);
    if index.len() == 0 {
        return Err("no card art found in StreamingAssets".into());
    }
    if let Err(e) = std::fs::create_dir_all(cache_dir) {
        warn!(error = %e, "could not create card art cache dir");
    } else {
        let manifest = cache_dir.join("card_art_index.json");
        let map: HashMap<String, String> = index
            .by_card_id
            .iter()
            .map(|(id, path)| (id.clone(), path.display().to_string()))
            .collect();
        if let Ok(json) = serde_json::to_string_pretty(&map) {
            let _ = std::fs::write(manifest, json);
        }
    }
    Ok(index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn indexes_card_png_by_filename() {
        let dir = tempdir().unwrap();
        let cards = dir.path().join("Cards");
        std::fs::create_dir_all(&cards).unwrap();
        let mut f = std::fs::File::create(cards.join("OP01-001.png")).unwrap();
        f.write_all(b"fake png bytes").unwrap();
        let index = CardArtIndex::build_from_streaming_assets(dir.path());
        assert!(index.card_path("OP01-001").is_some());
    }

    #[test]
    fn normalizes_underscore_filenames() {
        assert_eq!(normalize_card_filename("OP01_001"), "OP01-001");
    }
}
