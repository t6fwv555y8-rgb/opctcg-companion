use optcg_core::Phase;
use optcg_observation::{
    adapters::optcgsim::{
        detector::{discover_combat_logs, discover_installation},
        log_reader::IncrementalLogReader,
        parser::OptcgSimLogParser,
        vision::{RegionConfig, VisionPipeline},
        OptcgSimConfig,
    },
    bridge_protocol::BrowserGameSnapshot,
    confidence::ConfidenceConfig,
    diff::SnapshotDiffer,
    reconciler::ObservationReconciler,
    session::GameSession,
    types::{ObservationEvent, ObservationSource},
};
use std::fs;
use std::path::PathBuf;

fn fixture(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(path)
}

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

fn semantically_equivalent(a: &optcg_core::GameState, b: &optcg_core::GameState) -> bool {
    a.phase == b.phase
        && a.turn_number == b.turn_number
        && a.player_one().life == b.player_one().life
        && a.player_two().life == b.player_two().life
}

#[test]
fn browser_snapshot_pipeline_reaches_main_phase() {
    let mut differ = SnapshotDiffer::new(ConfidenceConfig::default());
    let snap: BrowserGameSnapshot = serde_json::from_str(
        &fs::read_to_string(fixture("onesimulator/equivalent_position.json")).unwrap(),
    )
    .unwrap();
    let events = differ.diff(&snap);
    let state = reconcile_events(ObservationSource::BrowserSimulator, events);
    assert_eq!(state.phase, Phase::Main);
    assert_eq!(state.player_one().life, 5);
}

#[test]
fn cross_adapter_semantic_equivalence() {
    let onesim_snap: BrowserGameSnapshot = serde_json::from_str(
        &fs::read_to_string(fixture("onesimulator/equivalent_position.json")).unwrap(),
    )
    .unwrap();
    let mut differ = SnapshotDiffer::new(ConfidenceConfig::default());
    let browser_state = reconcile_events(
        ObservationSource::BrowserSimulator,
        differ.diff(&onesim_snap),
    );

    let parser = OptcgSimLogParser::new();
    let log = fs::read_to_string(fixture("optcgsim/combat_log.json")).unwrap();
    let mut optcgsim_events = Vec::new();
    for line in log.lines() {
        if let Ok(events) = parser.parse_line(line) {
            optcgsim_events.extend(events);
        }
    }
    let optcgsim_state = reconcile_events(ObservationSource::DesktopSimulator, optcgsim_events);

    assert!(semantically_equivalent(&browser_state, &optcgsim_state));
    assert_eq!(browser_state.phase, Phase::Main);
}

#[test]
fn optcgsim_log_incremental_read_and_parse() {
    let path = fixture("optcgsim/combat_log.txt");
    let mut reader = IncrementalLogReader::open(&path).unwrap();
    let lines = reader.read_new_lines().unwrap();
    assert!(!lines.is_empty());

    let parser = OptcgSimLogParser::new();
    let mut phases = 0;
    for line in lines {
        if let Ok(events) = parser.parse_line(&line) {
            phases += events
                .iter()
                .filter(|e| matches!(e, ObservationEvent::PhaseObserved { .. }))
                .count();
        }
    }
    assert!(phases >= 1);
}

#[test]
fn optcgsim_combat_logs_post_game_only() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("OPTCGSim_Data/StreamingAssets")).unwrap();
    let config = OptcgSimConfig {
        custom_install_paths: vec![dir.path().to_path_buf()],
        ..Default::default()
    };
    let install = discover_installation(&config).unwrap();
    let discovery = discover_combat_logs(&Some(install));
    assert!(!discovery.live_capable);
}

#[test]
fn optcgsim_vision_fixture_pipeline() {
    let fixture_path = fixture("optcgsim/vision_observation.json");
    let pipeline = VisionPipeline::new(RegionConfig::default()).with_fixture(&fixture_path);
    let obs = pipeline.capture_observation().expect("fixture observation");
    let events = obs.to_observation_events();
    let state = reconcile_events(ObservationSource::DesktopSimulator, events);
    assert_eq!(state.phase, Phase::Main);
}

#[test]
fn optcgsim_installation_discovery() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("OPTCGSim_Data/StreamingAssets/Decks")).unwrap();
    let config = OptcgSimConfig {
        custom_install_paths: vec![dir.path().to_path_buf()],
        ..Default::default()
    };
    let install = discover_installation(&config).expect("install");
    assert!(install.streaming_assets.is_some());
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

#[test]
fn latency_tracker_records_stages() {
    use optcg_observation::latency::LatencyTracker;
    let tracker = LatencyTracker::new();
    let mut timer = tracker.begin_observation();
    timer.mark_analysis_start();
    timer.finish();
    let snap = tracker.snapshot();
    assert!(snap.total_latency_ms >= 0);
}
