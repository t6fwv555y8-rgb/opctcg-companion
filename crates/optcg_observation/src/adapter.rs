use crate::error::ObsResult;
use crate::types::ObservationSource;
use async_trait::async_trait;
use tokio::sync::mpsc;

/// Lifecycle status for an observation adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AdapterStatus {
    #[default]
    Unavailable,
    Detecting,
    Connected,
    Observing,
    Degraded,
    Disconnected,
    Error,
}

impl AdapterStatus {
    pub fn is_live(&self) -> bool {
        matches!(self, Self::Connected | Self::Observing | Self::Degraded)
    }
}

/// Simulator-independent observation adapter interface.
#[async_trait]
pub trait ObservationAdapter: Send + Sync {
    fn source(&self) -> ObservationSource;
    fn status(&self) -> AdapterStatus;
    async fn detect(&self) -> ObsResult<bool>;
    async fn start(&self, sender: mpsc::Sender<crate::types::ObservationEnvelope>)
        -> ObsResult<()>;
    async fn stop(&self) -> ObsResult<()>;
}
