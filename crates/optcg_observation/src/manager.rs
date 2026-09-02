use crate::adapter::{AdapterStatus, ObservationAdapter};
use crate::adapters::{
    optcgsim::{OptcgSimAdapter, OptcgSimConfig, OptcgSimStatus},
    BrowserSimulatorAdapter, MockAdapter, ReplayAdapter, ScreenVisionAdapter,
};
use crate::error::{ObsResult, ObservationError};
use crate::types::{ObservationEnvelope, ObservationSource};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{info, warn};

/// User-facing observation source selection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SourceSelection {
    #[default]
    Auto,
    /// OneSimulator browser bridge
    #[serde(alias = "browser_simulator")]
    OneSimulator,
    /// OPTCGSim desktop adapter
    #[serde(alias = "desktop_simulator")]
    OptcgSim,
    Mock,
    Replay,
    ScreenVision,
}

impl SourceSelection {
    pub fn to_source(&self) -> Option<ObservationSource> {
        match self {
            Self::Auto => None,
            Self::OneSimulator => Some(ObservationSource::BrowserSimulator),
            Self::OptcgSim => Some(ObservationSource::DesktopSimulator),
            Self::Mock => Some(ObservationSource::Mock),
            Self::Replay => Some(ObservationSource::Replay),
            Self::ScreenVision => Some(ObservationSource::ScreenVision),
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Auto => "Auto Detect",
            Self::OneSimulator => "OneSimulator",
            Self::OptcgSim => "OPTCGSim",
            Self::Mock => "Mock",
            Self::Replay => "Replay",
            Self::ScreenVision => "Screen Vision",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterInfo {
    pub source: ObservationSource,
    pub status: AdapterStatus,
    pub detected: bool,
    pub label: String,
}

/// Central manager for all observation adapters — one authoritative source at a time.
pub struct AdapterManager {
    mock: Arc<MockAdapter>,
    optcgsim: Arc<OptcgSimAdapter>,
    browser: Arc<BrowserSimulatorAdapter>,
    screen_vision: Arc<ScreenVisionAdapter>,
    replay: Arc<ReplayAdapter>,
    active: Arc<Mutex<Option<ObservationSource>>>,
    selection: Arc<Mutex<SourceSelection>>,
    replay_path: Arc<Mutex<Option<PathBuf>>>,
}

impl AdapterManager {
    pub fn new(desktop_log_path: PathBuf) -> Self {
        let mut optcgsim_config = OptcgSimConfig::default();
        optcgsim_config.custom_log_paths.push(desktop_log_path);
        Self {
            mock: Arc::new(MockAdapter::default_port()),
            optcgsim: Arc::new(OptcgSimAdapter::new(optcgsim_config)),
            browser: Arc::new(BrowserSimulatorAdapter::default_port()),
            screen_vision: Arc::new(ScreenVisionAdapter::new()),
            replay: Arc::new(ReplayAdapter::new()),
            active: Arc::new(Mutex::new(None)),
            selection: Arc::new(Mutex::new(SourceSelection::Auto)),
            replay_path: Arc::new(Mutex::new(None)),
        }
    }

    pub fn with_mock_port(mut self, port: u16) -> Self {
        self.mock = Arc::new(MockAdapter::new(port));
        self
    }

    pub fn with_browser_port(mut self, port: u16) -> Self {
        self.browser = Arc::new(BrowserSimulatorAdapter::new(port));
        self
    }

    pub fn optcgsim_status(&self) -> OptcgSimStatus {
        self.optcgsim.status_info()
    }

    pub fn selection(&self) -> SourceSelection {
        self.selection.lock().clone()
    }

    pub fn set_selection(&self, selection: SourceSelection) {
        *self.selection.lock() = selection;
    }

    pub fn set_replay_path(&self, path: PathBuf) {
        *self.replay_path.lock() = Some(path);
    }

    pub fn active_source(&self) -> Option<ObservationSource> {
        *self.active.lock()
    }

    pub async fn auto_detect(&self) -> Vec<AdapterInfo> {
        let mut infos = Vec::new();

        let optcgsim_detected = self.optcgsim.detect().await.unwrap_or(false);
        let optcgsim_status = self.optcgsim.status_info();
        infos.push(AdapterInfo {
            source: ObservationSource::DesktopSimulator,
            status: self.optcgsim.status(),
            detected: optcgsim_detected,
            label: optcgsim_status.label,
        });

        let browser_detected = self.browser.detect().await.unwrap_or(false);
        infos.push(AdapterInfo {
            source: ObservationSource::BrowserSimulator,
            status: self.browser.status(),
            detected: browser_detected,
            label: "OneSimulator · LIVE".into(),
        });

        infos.push(AdapterInfo {
            source: ObservationSource::Mock,
            status: self.mock.status(),
            detected: self.mock.detect().await.unwrap_or(true),
            label: ObservationSource::Mock.label().into(),
        });

        infos.push(AdapterInfo {
            source: ObservationSource::Replay,
            status: self.replay.status(),
            detected: self.replay_path.lock().is_some(),
            label: ObservationSource::Replay.label().into(),
        });

        infos
    }

    pub async fn resolve_auto_source(&self) -> ObservationSource {
        let infos = self.auto_detect().await;
        // Browser bridge takes priority when actively observing
        if infos
            .iter()
            .any(|i| i.source == ObservationSource::BrowserSimulator && i.detected)
        {
            return ObservationSource::BrowserSimulator;
        }
        if infos
            .iter()
            .any(|i| i.source == ObservationSource::DesktopSimulator && i.detected)
        {
            return ObservationSource::DesktopSimulator;
        }
        ObservationSource::Mock
    }

    pub fn all_statuses(&self) -> Vec<AdapterInfo> {
        let optcgsim_status = self.optcgsim.status_info();
        vec![
            AdapterInfo {
                source: ObservationSource::Mock,
                status: self.mock.status(),
                detected: true,
                label: ObservationSource::Mock.label().into(),
            },
            AdapterInfo {
                source: ObservationSource::DesktopSimulator,
                status: self.optcgsim.status(),
                detected: optcgsim_status.process_detected
                    || optcgsim_status.installation.is_some(),
                label: optcgsim_status.label,
            },
            AdapterInfo {
                source: ObservationSource::BrowserSimulator,
                status: self.browser.status(),
                detected: self.browser.status().is_live(),
                label: "OneSimulator".into(),
            },
            AdapterInfo {
                source: ObservationSource::ScreenVision,
                status: self.screen_vision.status(),
                detected: false,
                label: ObservationSource::ScreenVision.label().into(),
            },
            AdapterInfo {
                source: ObservationSource::Replay,
                status: self.replay.status(),
                detected: self.replay_path.lock().is_some(),
                label: ObservationSource::Replay.label().into(),
            },
        ]
    }

    pub async fn start(&self, sender: mpsc::Sender<ObservationEnvelope>) -> ObsResult<()> {
        self.stop().await?;

        let selection = self.selection.lock().clone();
        let source = match selection {
            SourceSelection::Auto => self.resolve_auto_source().await,
            other => other
                .to_source()
                .ok_or_else(|| ObservationError::Adapter("invalid selection".into()))?,
        };

        info!(?source, "starting authoritative adapter");
        self.start_source(source, sender).await?;
        *self.active.lock() = Some(source);
        Ok(())
    }

    async fn start_source(
        &self,
        source: ObservationSource,
        sender: mpsc::Sender<ObservationEnvelope>,
    ) -> ObsResult<()> {
        match source {
            ObservationSource::Mock => self.mock.start(sender).await,
            ObservationSource::DesktopSimulator => self.optcgsim.start(sender).await,
            ObservationSource::BrowserSimulator => self.browser.start(sender).await,
            ObservationSource::ScreenVision => self.screen_vision.start(sender).await,
            ObservationSource::Replay => {
                let path = self
                    .replay_path
                    .lock()
                    .clone()
                    .ok_or_else(|| ObservationError::Unavailable("no replay file".into()))?;
                self.replay.load(&path)?;
                self.replay.start(sender).await
            }
        }
    }

    pub async fn stop(&self) -> ObsResult<()> {
        let active = self.active.lock().take();
        if let Some(source) = active {
            match source {
                ObservationSource::Mock => self.mock.stop().await?,
                ObservationSource::DesktopSimulator => self.optcgsim.stop().await?,
                ObservationSource::BrowserSimulator => self.browser.stop().await?,
                ObservationSource::ScreenVision => self.screen_vision.stop().await?,
                ObservationSource::Replay => self.replay.stop().await?,
            }
        }
        Ok(())
    }

    pub async fn switch_source(
        &self,
        selection: SourceSelection,
        sender: mpsc::Sender<ObservationEnvelope>,
    ) -> ObsResult<()> {
        self.set_selection(selection);
        self.start(sender).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn manager_starts_mock_adapter() {
        let dir = tempdir().unwrap();
        let manager = AdapterManager::new(dir.path().to_path_buf()).with_mock_port(19004);
        manager.set_selection(SourceSelection::Mock);
        let (tx, mut rx) = mpsc::channel(8);
        manager.start(tx).await.unwrap();
        assert_eq!(manager.active_source(), Some(ObservationSource::Mock));
        assert!(manager
            .all_statuses()
            .iter()
            .any(|i| i.source == ObservationSource::Mock && i.status == AdapterStatus::Observing));
        manager.stop().await.unwrap();
        let _ = rx.try_recv();
    }

    #[tokio::test]
    async fn only_one_active_at_a_time() {
        let dir = tempdir().unwrap();
        let manager = AdapterManager::new(dir.path().to_path_buf()).with_mock_port(19005);
        let (tx, _rx) = mpsc::channel(8);
        manager.set_selection(SourceSelection::Mock);
        manager.start(tx).await.unwrap();
        manager.set_selection(SourceSelection::OneSimulator);
        let (tx2, _rx2) = mpsc::channel(8);
        manager.start(tx2).await.unwrap();
        assert_eq!(
            manager.active_source(),
            Some(ObservationSource::BrowserSimulator)
        );
        assert!(manager.all_statuses().iter().any(
            |i| i.source == ObservationSource::Mock && i.status == AdapterStatus::Disconnected
        ));
        manager.stop().await.unwrap();
    }

    #[test]
    fn source_selection_labels() {
        assert_eq!(SourceSelection::OneSimulator.label(), "OneSimulator");
        assert_eq!(SourceSelection::OptcgSim.label(), "OPTCGSim");
    }
}
