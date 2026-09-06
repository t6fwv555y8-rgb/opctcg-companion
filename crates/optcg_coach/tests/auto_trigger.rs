//! The automatic-read policy driven by real `GameState` transitions.
//!
//! The unit tests in `auto` cover the state machine against synthetic digests.
//! These check the pieces agree once wired together: that `fingerprint` moves
//! when a play happens and holds still when it does not, and that
//! `is_decision_point` gates the moments the player has no say in.

use optcg_coach::{fingerprint, is_decision_point, AutoDecision, AutoTrigger, AutoTriggerConfig};
use optcg_core::{GameState, Phase};
use std::time::{Duration, Instant};

const SETTLE: Duration = Duration::from_millis(1_000);
const FLOOR: Duration = Duration::from_millis(5_000);

fn trigger() -> AutoTrigger {
    AutoTrigger::new(AutoTriggerConfig {
        enabled: true,
        settle: SETTLE,
        min_interval: FLOOR,
    })
}

/// Feed a position to the trigger the way the poll ticker does: every 500ms
/// until the settle window closes. Returns whether it fired within `window`.
fn poll_until_settled(
    trigger: &mut AutoTrigger,
    state: &GameState,
    from: Instant,
    window: Duration,
) -> bool {
    let mut now = from;
    while now <= from + window {
        let position = fingerprint(state);
        if trigger.observe(&position, is_decision_point(state), now) == AutoDecision::Fire {
            return true;
        }
        now += Duration::from_millis(500);
    }
    false
}

fn your_main_phase() -> GameState {
    let mut state = GameState::new();
    state.turn_number = 3;
    state.active_player = 0;
    state.phase = Phase::Main;
    state
}

#[test]
fn a_quiet_board_on_your_turn_is_read_once() {
    let mut trigger = trigger();
    let state = your_main_phase();
    let start = Instant::now();

    assert!(
        poll_until_settled(&mut trigger, &state, start, Duration::from_secs(3)),
        "your own main phase should be read"
    );
    assert!(
        !poll_until_settled(
            &mut trigger,
            &state,
            start + Duration::from_secs(30),
            Duration::from_secs(3)
        ),
        "an unchanged board must not be read again"
    );
}

#[test]
fn the_opponents_turn_stays_quiet_until_they_attack_you() {
    let mut trigger = trigger();
    let mut state = your_main_phase();
    state.active_player = 1;
    let start = Instant::now();

    assert!(
        !poll_until_settled(&mut trigger, &state, start, Duration::from_secs(10)),
        "nothing to decide on their turn"
    );

    // Their attack resolves against you: block and counter are your call.
    state.phase = Phase::Combat;
    state.combat.active = true;
    state.combat.attacker_id = Some("OP01-001".into());
    state.combat.target_player = Some(0);
    state.combat.target_is_leader = true;

    assert!(
        poll_until_settled(
            &mut trigger,
            &state,
            start + Duration::from_secs(10),
            Duration::from_secs(3)
        ),
        "being attacked is the moment advice matters most"
    );
}

#[test]
fn a_burst_of_changes_produces_one_read_of_the_finished_position() {
    let mut trigger = trigger();
    let mut state = your_main_phase();
    let start = Instant::now();
    let mut reads = 0;

    // Life ticks down three times in quick succession, 300ms apart, then the
    // board holds still. Each change moves the fingerprint.
    let mut now = start;
    for life in [4u32, 3, 2] {
        state.players[0].life = life;
        let position = fingerprint(&state);
        if trigger.observe(&position, is_decision_point(&state), now) == AutoDecision::Fire {
            reads += 1;
        }
        now += Duration::from_millis(300);
    }
    assert_eq!(reads, 0, "mid-burst positions must not be read");

    assert!(
        poll_until_settled(&mut trigger, &state, now, Duration::from_secs(3)),
        "the position the burst ended on should be read once it settles"
    );
}

#[test]
fn churn_that_does_not_change_the_position_never_triggers_a_read() {
    let mut trigger = trigger();
    let mut state = your_main_phase();
    let start = Instant::now();

    assert!(poll_until_settled(
        &mut trigger,
        &state,
        start,
        Duration::from_secs(3)
    ));

    // Observation bookkeeping churns constantly and says nothing about the
    // board, so it must not look like a new position.
    let mut now = start + Duration::from_secs(30);
    for i in 0..40 {
        state.event_sequence += 1;
        state.event_log.push(format!("log line {i}"));
        state.timestamp = chrono::Utc::now();

        let position = fingerprint(&state);
        assert_eq!(
            trigger.observe(&position, is_decision_point(&state), now),
            AutoDecision::Idle,
            "event churn must not trigger a read"
        );
        now += Duration::from_millis(500);
    }
}

#[test]
fn the_floor_between_reads_holds_across_a_run_of_real_plays() {
    let mut trigger = trigger();
    let mut state = your_main_phase();
    let start = Instant::now();

    assert!(poll_until_settled(
        &mut trigger,
        &state,
        start,
        Duration::from_secs(3)
    ));

    // Twelve distinct positions, each held for 1.5s across 18s of play. Every
    // one of them clears the 1s settle window on its own, so without the floor
    // all twelve would be read.
    let mut now = start + Duration::from_secs(3);
    let mut reads = 0;
    for life in 0..12u32 {
        state.players[0].life = life;
        for _ in 0..3 {
            let position = fingerprint(&state);
            if trigger.observe(&position, is_decision_point(&state), now) == AutoDecision::Fire {
                reads += 1;
            }
            now += Duration::from_millis(500);
        }
    }

    // The bound is a literal rather than a figure derived from FLOOR, so that
    // weakening the floor fails the test instead of relaxing its expectation.
    assert!(
        (2..=4).contains(&reads),
        "expected the 5s floor to cut 12 readable positions down to a handful, got {reads}"
    );
}
