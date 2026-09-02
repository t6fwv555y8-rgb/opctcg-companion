use crate::error::{EventsError, EventsResult};
use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use parking_lot::Mutex;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

/// Configuration for file system event monitoring.
#[derive(Debug, Clone)]
pub struct FileMonitorConfig {
    pub watch_path: PathBuf,
    pub debounce_ms: u64,
}

impl Default for FileMonitorConfig {
    fn default() -> Self {
        Self {
            watch_path: PathBuf::from("."),
            debounce_ms: 100,
        }
    }
}

/// Watches local simulation log files for delta adjustments.
pub struct FileMonitor {
    config: FileMonitorConfig,
}

impl FileMonitor {
    pub fn new(config: FileMonitorConfig) -> Self {
        Self { config }
    }

    pub async fn start(&self, tx: mpsc::Sender<String>) -> EventsResult<()> {
        let path = self.config.watch_path.clone();
        if !path.exists() {
            std::fs::create_dir_all(&path)?;
        }

        let (notify_tx, notify_rx) = std::sync::mpsc::channel();
        let debounce = self.config.debounce_ms;
        let mut watcher = RecommendedWatcher::new(
            move |res: Result<Event, notify::Error>| {
                if let Err(e) = notify_tx.send(res) {
                    error!(error = %e, "notify channel send failed");
                }
            },
            Config::default().with_poll_interval(Duration::from_millis(debounce)),
        )
        .map_err(|e| EventsError::Notify(e.to_string()))?;

        watcher
            .watch(&path, RecursiveMode::Recursive)
            .map_err(|e| EventsError::Notify(e.to_string()))?;

        info!(path = %path.display(), "file monitor started");

        let last_content: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));

        std::thread::spawn(move || {
            for res in notify_rx {
                match res {
                    Ok(event) => {
                        for p in event.paths {
                            if is_log_file(&p) {
                                if let Ok(content) = std::fs::read_to_string(&p) {
                                    let delta = extract_delta(&last_content, &content);
                                    *last_content.lock() = content;
                                    for line in delta.lines().filter(|l| !l.trim().is_empty()) {
                                        if tx.blocking_send(line.to_string()).is_err() {
                                            return;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => warn!(error = %e, "file watch error"),
                }
            }
        });

        std::mem::forget(watcher);
        Ok(())
    }
}

fn is_log_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| matches!(e, "log" | "jsonl" | "txt"))
        .unwrap_or(false)
}

fn extract_delta(last: &Arc<Mutex<String>>, current: &str) -> String {
    let prev = last.lock().clone();
    if current.starts_with(&prev) {
        current[prev.len()..].to_string()
    } else {
        current.lines().last().unwrap_or("").to_string()
    }
}
