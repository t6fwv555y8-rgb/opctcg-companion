//! Reading text off the screen with macOS's own text recognition (Vision).
//!
//! A small Swift program does the recognition. It is compiled the first time
//! it is needed and kept in the app's data folder. Everything runs on this
//! computer; no picture or text leaves it.

use serde::Deserialize;
use std::path::PathBuf;

const SOURCE: &str = include_str!("ocr.swift");

/// One line of recognised text and where it was, in the picture's pixels.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct OcrLine {
    pub text: String,
    #[serde(rename = "conf")]
    pub confidence: f32,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

fn fnv(text: &str) -> u64 {
    text.bytes().fold(0xcbf29ce484222325, |h, b| (h ^ u64::from(b)).wrapping_mul(0x100000001b3))
}

fn helper_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("optcg-companion")
        .join("bin")
}

/// Path of the compiled helper, building it if it is missing or out of date.
pub fn ensure_helper() -> Result<PathBuf, String> {
    let dir = helper_dir();
    let binary = dir.join(format!("optcg-ocr-{:x}", fnv(SOURCE)));
    if binary.is_file() {
        return Ok(binary);
    }
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let source = dir.join("optcg-ocr.swift");
    std::fs::write(&source, SOURCE).map_err(|e| e.to_string())?;
    let out = std::process::Command::new("swiftc")
        .args(["-O"])
        .arg(&source)
        .arg("-o")
        .arg(&binary)
        .output()
        .map_err(|_| {
            "Text recognition needs Apple's command line tools. Run `xcode-select --install` once, then try again."
                .to_string()
        })?;
    if !out.status.success() {
        return Err(format!(
            "could not build the text reader: {}",
            String::from_utf8_lossy(&out.stderr).lines().next().unwrap_or("unknown error")
        ));
    }
    Ok(binary)
}

/// A running text reader. Starting it costs about half a second, so one is
/// kept for the whole session and asked for each frame.
pub struct OcrServer {
    child: std::process::Child,
    stdin: std::process::ChildStdin,
    stdout: std::io::BufReader<std::process::ChildStdout>,
}

#[derive(Deserialize)]
struct Tagged {
    #[serde(default)]
    r: usize,
    #[serde(flatten)]
    line: Option<OcrLine>,
    #[serde(default)]
    done: bool,
}

impl OcrServer {
    pub fn start() -> Result<Self, String> {
        let helper = ensure_helper()?;
        let mut child = std::process::Command::new(helper)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| format!("could not start the text reader: {e}"))?;
        let stdin = child.stdin.take().ok_or("text reader has no input")?;
        let stdout = child.stdout.take().ok_or("text reader has no output")?;
        Ok(Self { child, stdin, stdout: std::io::BufReader::new(stdout) })
    }

    /// Recognise text in `crops` (x, y, w, h in pixels) of the picture at
    /// `path`. The result has one entry per crop, in order.
    pub fn read(&mut self, path: &std::path::Path, crops: &[(u32, u32, u32, u32, bool)]) -> Result<Vec<Vec<OcrLine>>, String> {
        use std::io::{BufRead, Write};
        let mut request = path.to_string_lossy().to_string();
        for (x, y, w, h, fast) in crops {
            request.push_str(&format!("\t{x} {y} {w} {h} {}", u8::from(*fast)));
        }
        writeln!(self.stdin, "{request}").map_err(|e| format!("text reader stopped: {e}"))?;
        self.stdin.flush().map_err(|e| e.to_string())?;
        let mut out: Vec<Vec<OcrLine>> = vec![Vec::new(); crops.len().max(1)];
        loop {
            let mut line = String::new();
            let n = self.stdout.read_line(&mut line).map_err(|e| e.to_string())?;
            if n == 0 {
                return Err("text reader stopped".into());
            }
            let Ok(item) = serde_json::from_str::<Tagged>(line.trim()) else { continue };
            if item.done {
                break;
            }
            if let (Some(l), Some(slot)) = (item.line, out.get_mut(item.r)) {
                slot.push(l);
            }
        }
        for lines in &mut out {
            lines.sort_by(|a, b| a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal));
        }
        Ok(out)
    }
}

impl Drop for OcrServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_source_always_gets_the_same_helper_name() {
        assert_eq!(fnv(SOURCE), fnv(SOURCE));
        assert_ne!(fnv("a"), fnv("b"));
    }

    #[test]
    fn helper_output_lines_parse() {
        let line = r#"{"text":"[A#1] End Turn","conf":1,"x":3.5,"y":10,"w":120,"h":18}"#;
        let parsed: OcrLine = serde_json::from_str(line).unwrap();
        assert_eq!(parsed.text, "[A#1] End Turn");
        assert_eq!(parsed.confidence, 1.0);
    }
}
