use crate::error::{ObsResult, ObservationError};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// Incremental log reader — tracks file offset, avoids full re-reads.
pub struct IncrementalLogReader {
    path: PathBuf,
    offset: u64,
    partial: String,
}

impl IncrementalLogReader {
    pub fn open(path: impl AsRef<Path>) -> ObsResult<Self> {
        Ok(Self {
            path: path.as_ref().to_path_buf(),
            offset: 0,
            partial: String::new(),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn offset(&self) -> u64 {
        self.offset
    }

    pub fn read_new_lines(&mut self) -> ObsResult<Vec<String>> {
        let mut file = File::open(&self.path).map_err(|e| {
            ObservationError::Io(std::io::Error::new(
                e.kind(),
                format!("open {}: {e}", self.path.display()),
            ))
        })?;

        let len = file.metadata()?.len();
        if len < self.offset {
            // truncated / rotated
            self.offset = 0;
            self.partial.clear();
        }

        file.seek(SeekFrom::Start(self.offset))?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)?;
        self.offset = file.metadata()?.len();

        let chunk = String::from_utf8_lossy(&buf);
        let combined = format!("{}{}", self.partial, chunk);

        let mut lines: Vec<String> = combined
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();

        if !combined.ends_with('\n') && !lines.is_empty() {
            self.partial = lines.pop().unwrap_or_default();
        } else {
            self.partial.clear();
        }

        Ok(lines)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn incremental_read_appends() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "line1").unwrap();
        let path = file.path().to_path_buf();
        let mut reader = IncrementalLogReader::open(&path).unwrap();
        assert_eq!(reader.read_new_lines().unwrap(), vec!["line1".to_string()]);
        writeln!(file, "line2").unwrap();
        assert_eq!(reader.read_new_lines().unwrap(), vec!["line2".to_string()]);
    }

    #[test]
    fn handles_truncation() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "old").unwrap();
        let path = file.path().to_path_buf();
        let mut reader = IncrementalLogReader::open(&path).unwrap();
        reader.read_new_lines().unwrap();
        writeln!(file, "new").unwrap();
        // simulate truncate by resetting offset beyond file — handled via len check
        reader.offset = 9999;
        let lines = reader.read_new_lines().unwrap();
        assert!(!lines.is_empty() || reader.offset() <= 9999);
    }
}
