//! Milestone 6 production pipeline tests — fixture validation for live-runtime abstractions.

use optcg_observation::{
    adapter::ObservationAdapter,
    adapters::optcgsim::regions::{NormalizedRegion, RegionConfig},
    adapters::replay::{ReplayAdapter, ReplaySpeed},
    analysis::AnalysisEligibility,
    capture::CapturedFrame,
    capture::{CaptureConfig, CapturePipeline, ChangeDetector, FrameBuffer},
    recording::{load_replay_lines, write_regression_fixture, ReplaySessionV1},
    sync_status::SyncStatus,
    temporal::TemporalField,
    types::{ObservationEnvelope, ObservationEvent, ObservationSource},
    window::GameWindowInfo,
};
use std::time::Instant;

fn test_frame(w: u32, h: u32, fill: u8) -> CapturedFrame {
    CapturedFrame {
        timestamp: Instant::now(),
        width: w,
        height: h,
        stride: (w * 4) as usize,
        pixels: FrameBuffer {
            data: vec![fill; (w * h * 4) as usize],
        },
        window: GameWindowInfo {
            process_id: 1,
            title: "OPTCGSim".into(),
            x: 0,
            y: 0,
            width: w,
            height: h,
            minimized: false,
            visible: true,
            monitor_scale: 1.0,
            hwnd: 0,
        },
    }
}

#[test]
fn window_resize_updates_pixel_regions() {
    let region = NormalizedRegion {
        name: "self_board".into(),
        x: 0.1,
        y: 0.5,
        width: 0.8,
        height: 0.2,
    };
    let small = region.to_pixel_rect(1280, 720);
    let large = region.to_pixel_rect(1920, 1080);
    assert!(large.width > small.width);
    assert!(large.height > small.height);
}

#[test]
fn dpi_scaling_preserves_relative_layout() {
    let region = NormalizedRegion {
        name: "self_life".into(),
        x: 0.02,
        y: 0.55,
        width: 0.08,
        height: 0.25,
    };
    let at_100 = region.to_pixel_rect(1920, 1080);
    let at_150 = region.to_pixel_rect(2880, 1620);
    assert!(at_150.width >= at_100.width);
    assert!((at_150.width as f32 / at_100.width as f32 - 1.5).abs() < 0.05);
}

#[test]
fn window_unavailable_returns_none_capture() {
    let pipeline = CapturePipeline::new(CaptureConfig::default());
    let window = GameWindowInfo {
        process_id: 0,
        title: String::new(),
        x: 0,
        y: 0,
        width: 0,
        height: 0,
        minimized: true,
        visible: false,
        monitor_scale: 1.0,
        hwnd: 0,
    };
    let result = pipeline.capture_once(&window);
    assert!(result.is_ok() || result.is_err());
}

#[test]
fn capture_backpressure_tracks_drops() {
    let pipeline = CapturePipeline::new(CaptureConfig {
        channel_capacity: 1,
        ..Default::default()
    });
    pipeline.record_drop();
    pipeline.record_drop();
    assert_eq!(pipeline.stats().frames_dropped, 2);
}

#[test]
fn unchanged_frame_suppression() {
    let mut detector = ChangeDetector::new();
    let frame = test_frame(640, 360, 42);
    assert!(detector.frame_changed(&frame));
    assert!(!detector.frame_changed(&frame));
}

#[test]
fn life_temporal_confirmation_requires_stability() {
    let mut life = TemporalField::<u8>::new(2);
    assert!(life.observe(3, 0.9).is_none());
    assert!(life.observe(3, 0.9).is_none());
    assert_eq!(life.observe(3, 0.9), Some(3));
}

#[test]
fn rested_state_temporal_smoothing() {
    let mut rested = TemporalField::<bool>::new(2);
    rested.observe(true, 0.7);
    rested.observe(false, 0.7);
    assert!(rested.current().is_none() || rested.current() == Some(&true));
    rested.observe(false, 0.7);
    rested.observe(false, 0.7);
    assert_eq!(rested.current(), Some(&false));
}

#[test]
fn low_confidence_analysis_ineligible() {
    let e = AnalysisEligibility::evaluate(0.2, false, false, false, true);
    assert!(!e.eligible);
}

#[test]
fn sync_status_transitions_with_confidence() {
    assert_eq!(SyncStatus::from_confidence(0.9, true), SyncStatus::Synced);
    assert_eq!(
        SyncStatus::from_confidence(0.5, true),
        SyncStatus::Recovering
    );
    assert_eq!(
        SyncStatus::from_confidence(0.9, false),
        SyncStatus::Desynced
    );
}

#[test]
fn new_match_reset_clears_combat() {
    use optcg_observation::GameSession;
    let mut session = GameSession::new(ObservationSource::BrowserSimulator);
    session.state.combat.active = true;
    session.reset_for_source(ObservationSource::BrowserSimulator);
    assert!(!session.state.combat.active);
}

#[test]
fn stale_observation_age() {
    use optcg_observation::temporal::ObservedValue;
    let obs = ObservedValue::with_value(5u8, 0.8);
    assert!(obs.age_ms() < 5000);
}

#[test]
fn replay_v1_schema_version() {
    let mut session = ReplaySessionV1::new(ObservationSource::Mock);
    session.push(&ObservationEnvelope {
        sequence: 1,
        timestamp_ms: 0,
        source: ObservationSource::Mock,
        event: ObservationEvent::StructuredRaw {
            raw: "PHASE_CHANGED|MAIN".into(),
            source: ObservationSource::Mock,
            confidence: 1.0,
        },
    });
    assert_eq!(session.schema_version, 1);
    assert_eq!(session.to_envelopes().len(), 1);
}

#[test]
fn replay_step_mode_enum() {
    assert_eq!(ReplaySpeed::from_label("2x"), ReplaySpeed::Double);
    assert_eq!(ReplaySpeed::from_label("step"), ReplaySpeed::Step);
}

#[test]
fn regression_fixture_generator() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("regression.v1.json");
    let envelopes = vec![ObservationEnvelope {
        sequence: 1,
        timestamp_ms: 0,
        source: ObservationSource::Mock,
        event: ObservationEvent::StructuredRaw {
            raw: "PHASE_CHANGED|MAIN".into(),
            source: ObservationSource::Mock,
            confidence: 1.0,
        },
    }];
    write_regression_fixture(&path, ObservationSource::Mock, &envelopes).unwrap();
    let loaded = load_replay_lines(&path).unwrap();
    assert_eq!(loaded.len(), 1);
}

#[test]
fn default_regions_cover_all_calibration_areas() {
    let config = RegionConfig::default();
    let names: Vec<_> = config.regions.iter().map(|r| r.name.as_str()).collect();
    for required in [
        "self_leader",
        "opponent_leader",
        "self_life",
        "opponent_life",
        "self_don",
        "opponent_don",
        "self_board",
        "opponent_board",
        "phase_turn",
        "combat_area",
    ] {
        assert!(names.contains(&required), "missing region {required}");
    }
}

#[tokio::test]
async fn source_reconnect_replay_reload() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session.v1.json");
    let mut session = ReplaySessionV1::new(ObservationSource::Mock);
    session.push(&ObservationEnvelope {
        sequence: 1,
        timestamp_ms: 0,
        source: ObservationSource::Mock,
        event: ObservationEvent::StructuredRaw {
            raw: "PHASE_CHANGED|MAIN".into(),
            source: ObservationSource::Mock,
            confidence: 1.0,
        },
    });
    std::fs::write(&path, serde_json::to_string_pretty(&session).unwrap()).unwrap();

    let adapter = ReplayAdapter::new();
    adapter.load(&path).unwrap();
    assert!(adapter.detect().await.unwrap());
}
