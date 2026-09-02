use optcg_core::Phase;
use optcg_observation::{
    bridge_protocol::{BrowserGameSnapshot, BrowserPlayerSnapshot},
    confidence::ConfidenceConfig,
    diff::SnapshotDiffer,
    reconciler::ObservationReconciler,
    session::GameSession,
    types::{ObservationEvent, ObservationSource},
};

fn reconcile_events(
    source: ObservationSource,
    events: Vec<ObservationEvent>,
) -> optcg_core::GameState {
    let mut reconciler = ObservationReconciler::new(ConfidenceConfig::default());
    let mut session = GameSession::new(source);
    for event in events {
        let _ = reconciler.reconcile(&mut session, &event);
    }
    session.state
}

#[test]
fn browser_snapshot_pipeline_reaches_main_phase() {
    let mut differ = SnapshotDiffer::new(ConfidenceConfig::default());
    let snap = BrowserGameSnapshot {
        timestamp: 1,
        phase: Some("Main".into()),
        self_player: Some(BrowserPlayerSnapshot {
            life: Some(5),
            hand_count: Some(5),
            active_don: Some(2),
            rested_don: Some(0),
            ..Default::default()
        }),
        ..Default::default()
    };
    let events = differ.diff(&snap);
    let state = reconcile_events(ObservationSource::BrowserSimulator, events);
    assert_eq!(state.phase, Phase::Main);
    assert_eq!(state.player_one().life, 5);
}

#[test]
fn desktop_structured_raw_matches_browser_semantics() {
    let desktop_state = reconcile_events(
        ObservationSource::DesktopSimulator,
        vec![ObservationEvent::StructuredRaw {
            raw: "PHASE_CHANGED|MAIN".into(),
            source: ObservationSource::DesktopSimulator,
            confidence: 1.0,
        }],
    );

    let mut differ = SnapshotDiffer::new(ConfidenceConfig::default());
    let browser_state = reconcile_events(
        ObservationSource::BrowserSimulator,
        differ.diff(&BrowserGameSnapshot {
            timestamp: 1,
            phase: Some("Main".into()),
            ..Default::default()
        }),
    );

    assert_eq!(desktop_state.phase, browser_state.phase);
    assert_eq!(desktop_state.phase, Phase::Main);
}

#[test]
fn combat_engine_independent_of_source() {
    let gs = reconcile_events(
        ObservationSource::BrowserSimulator,
        vec![ObservationEvent::StructuredRaw {
            raw: "PHASE_CHANGED|COMBAT".into(),
            source: ObservationSource::BrowserSimulator,
            confidence: 0.99,
        }],
    );
    assert_eq!(gs.phase, Phase::Combat);
}
