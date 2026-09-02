pub mod adapter;
pub mod adapters;
pub mod bridge_protocol;
pub mod confidence;
pub mod diff;
pub mod error;
pub mod latency;
pub mod manager;
pub mod pipeline;
pub mod process_detect;
pub mod reconciler;
pub mod recording;
pub mod session;
pub mod types;
pub mod window_source;

pub use adapter::{AdapterStatus, ObservationAdapter};
pub use bridge_protocol::BrowserGameSnapshot;
pub use confidence::ConfidenceConfig;
pub use diff::SnapshotDiffer;
pub use error::ObservationError;
pub use latency::{LatencySnapshot, LatencyTracker};
pub use manager::{AdapterInfo, AdapterManager, SourceSelection};
pub use pipeline::{ObservationPipeline, ObservationPipelineConfig, PipelineResult};
pub use reconciler::{ObservationReconciler, ReconcileOutcome};
pub use recording::ObservationRecorder;
pub use session::{GameSession, GameSessionId};
pub use types::*;

#[cfg(test)]
mod extra_tests {
    use super::*;
    use bridge_protocol::{
        parse_snapshot_payload, validate_bridge_payload, BridgeMessage, MAX_BRIDGE_PAYLOAD,
    };
    use optcg_core::Phase;

    #[test]
    fn rejects_oversized_payload() {
        let huge = vec![b'a'; MAX_BRIDGE_PAYLOAD + 1];
        assert!(validate_bridge_payload(&huge).is_err());
    }

    #[test]
    fn parses_ping_message() {
        let msg = validate_bridge_payload(br#"{"type":"ping"}"#).unwrap();
        assert!(matches!(msg, BridgeMessage::Ping));
    }

    #[test]
    fn parses_raw_snapshot_post() {
        let snap =
            parse_snapshot_payload(br#"{"timestamp":1,"phase":"Main","self":{"life":5}}"#).unwrap();
        assert_eq!(snap.phase, Some("Main".into()));
    }

    #[test]
    fn duplicate_phase_observation_idempotent() {
        let mut reconciler = ObservationReconciler::default();
        let mut session = GameSession::new(ObservationSource::BrowserSimulator);
        let obs = ObservationEvent::PhaseObserved {
            phase: Phase::Main,
            confidence: 0.99,
        };
        reconciler.reconcile(&mut session, &obs).unwrap();
        reconciler.reconcile(&mut session, &obs).unwrap();
        assert_eq!(session.state.phase, Phase::Main);
    }
}
