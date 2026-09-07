//! A live HUD that draws in the terminal.
//!
//! The desktop window needs a display. This path starts the same observation
//! pipeline and scouting ledger, then prints the panels as text so the
//! companion can be watched from a shell.

use crate::dto::{
    DeckInfoDto, DeckOrigin, MatchupReportDto, ScoutedCardDto, ScoutingReportDto,
    StateUpdatePayload,
};
use crate::runtime::default_pipeline_config;
use crate::state::AppState;
use optcg_database::{AssetParser, Database};
use optcg_observation::{ObservationPipeline, SourceSelection};
use parking_lot::RwLock;
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;

const WIDTH: usize = 64;

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("warn".parse().unwrap()),
        )
        .with_writer(io::stderr)
        .init();

    let data_dir = dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("optcg-companion");
    let _ = std::fs::create_dir_all(&data_dir);
    let _ = std::fs::create_dir_all(data_dir.join("sessions"));
    let _ = std::fs::create_dir_all(data_dir.join("logs"));

    let db_path = data_dir.join("optcg_companion.db");
    let database = match Database::open(db_path.to_str().unwrap_or("optcg_companion.db")) {
        Ok(db) => db,
        Err(_) => Database::open_in_memory().expect("in-memory database failed"),
    };
    let _ = AssetParser::seed_defaults(&database);

    let game_state = Arc::new(RwLock::new(optcg_core::GameState::new()));
    let pipeline = Arc::new(ObservationPipeline::new(
        Arc::clone(&game_state),
        default_pipeline_config(data_dir.clone()),
    ));
    let app_state = AppState::new(database, Arc::clone(&game_state), data_dir);

    let source = match std::env::var("OPTCG_SOURCE")
        .unwrap_or_else(|_| "mock".into())
        .to_lowercase()
        .as_str()
    {
        "optcgsim" | "desktop" => SourceSelection::OptcgSim,
        "onesimulator" | "browser" => SourceSelection::OneSimulator,
        "auto" => SourceSelection::Auto,
        _ => SourceSelection::Mock,
    };

    eprintln!(
        "OPTCG Companion · terminal HUD · {} · Ctrl-C to quit",
        match source {
            SourceSelection::Mock => "mock :9002",
            SourceSelection::OneSimulator => "browser :9003",
            SourceSelection::OptcgSim => "desktop log",
            SourceSelection::Auto => "auto",
            SourceSelection::Replay => "replay",
            SourceSelection::ScreenVision => "screen",
        }
    );

    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
    runtime.block_on(async move {
        let (result_tx, mut result_rx) = mpsc::channel(512);
        if let Err(e) = pipeline.start(source, result_tx).await {
            eprintln!("pipeline failed to start: {e}");
            return;
        }

        print_hud(&app_state.build_update_payload(None));

        loop {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    app_state.close_scouting_game();
                    break;
                }
                result = result_rx.recv() => {
                    let Some(result) = result else { break; };
                    if result.error.is_none() || result.applied {
                        app_state.scout_position();
                        print_hud(&app_state.build_update_payload(None));
                    }
                }
            }
        }
    });
}

fn print_hud(payload: &StateUpdatePayload) {
    let mut out = String::new();
    out.push('\n');
    out.push_str(&rule('┌', '─', '┐'));
    out.push_str(&line(&header(payload)));
    out.push_str(&line(&matchup_line(payload)));
    out.push_str(&rule('├', '─', '┤'));
    out.push_str(&section("DECKS"));
    out.push_str(&deck_block("YOU", &payload.your_deck));
    out.push_str(&deck_block("OPP", &payload.opponent_deck));
    out.push_str(&rule('├', '─', '┤'));
    out.push_str(&section("SCOUTING"));
    out.push_str(&scouting_block(payload.scouting.as_ref(), payload));
    out.push_str(&rule('├', '─', '┤'));
    out.push_str(&section("MATCHUP"));
    out.push_str(&matchup_block(payload.matchup.as_ref()));
    out.push_str(&rule('├', '─', '┤'));
    out.push_str(&section("COMBAT"));
    out.push_str(&combat_block(payload));
    out.push_str(&rule('├', '─', '┤'));
    out.push_str(&section("COACH"));
    out.push_str(&coach_block(payload));
    out.push_str(&rule('└', '─', '┘'));

    let mut stdout = io::stdout();
    let _ = write!(stdout, "\x1b[2J\x1b[H{out}");
    let _ = stdout.flush();
}

fn header(payload: &StateUpdatePayload) -> String {
    let gs = &payload.game_state;
    if gs.page_state == "queue" {
        return format!(
            "OPTCG Companion   IN QUEUE   seq {}",
            gs.event_sequence
        );
    }
    let who = if gs.active_player == 0 { "YOU" } else { "OPP" };
    format!(
        "OPTCG Companion   {who} · {} · T{}   seq {}",
        format!("{:?}", gs.phase).to_uppercase(),
        gs.turn_number,
        gs.event_sequence
    )
}

fn side_label(player_name: &str, leader: &str, fallback: &str) -> String {
    let name = if player_name.trim().is_empty() {
        fallback
    } else {
        player_name.trim()
    };
    if leader.is_empty() || leader == "Unknown leader" {
        name.to_string()
    } else {
        format!("{name} · {leader}")
    }
}

fn matchup_line(payload: &StateUpdatePayload) -> String {
    let you = payload.game_state.player_one.life;
    let them = payload.game_state.player_two.life;
    format!(
        "{}  {you}♥   vs   {}  {them}♥",
        side_label(
            &payload.game_state.player_one.player_name,
            &payload.your_deck.leader_name,
            "You"
        ),
        side_label(
            &payload.game_state.player_two.player_name,
            &payload.opponent_deck.leader_name,
            "Opponent"
        )
    )
}

fn deck_block(label: &str, deck: &DeckInfoDto) -> String {
    let origin = match deck.origin {
        DeckOrigin::Observed => "read from play",
        DeckOrigin::Presumed => "likely list",
        DeckOrigin::Attached => "list attached",
    };
    let mut block = line(&format!(
        "{label}   {} · {} · {origin}",
        deck.leader_name, deck.leader_id
    ));
    if deck.known_cards.is_empty() {
        block.push_str(&line("      no cards identified yet"));
    } else {
        let names: Vec<String> = deck
            .known_cards
            .iter()
            .take(6)
            .map(|c| c.name.clone())
            .collect();
        block.push_str(&line(&format!("      {}", names.join(" · "))));
    }
    block
}

fn scouting_block(report: Option<&ScoutingReportDto>, payload: &StateUpdatePayload) -> String {
    let Some(report) = report else {
        let attached = payload.opponent_deck.origin == DeckOrigin::Attached;
        return line(if attached {
            "Their list is attached, so there is nothing left to infer."
        } else {
            "Nothing on this leader yet. Games you play are recorded."
        });
    };
    let mut block = line(&format!(
        "{} games · {} · plays {}",
        report.games, report.reliability, report.pace
    ));
    block.push_str(&line(&format!(
        "{} of their 50 cards mapped",
        report.mapped_copies
    )));
    for card in report.cards.iter().take(6) {
        block.push_str(&line(&card_row(card)));
    }
    if report.cards.len() > 6 {
        block.push_str(&line(&format!("      … {} more", report.cards.len() - 6)));
    }
    block
}

fn card_row(card: &ScoutedCardDto) -> String {
    let filled = ((card.confidence * 10.0).round() as usize).min(10);
    let bar: String = std::iter::repeat('█')
        .take(filled)
        .chain(std::iter::repeat('░').take(10 - filled))
        .collect();
    format!(
        "{:>2}× {:<16} {bar} {:>3}%",
        card.likely_copies,
        truncate(&card.name, 16),
        (card.confidence * 100.0).round() as u32
    )
}

fn matchup_block(report: Option<&MatchupReportDto>) -> String {
    let Some(report) = report else {
        return line("No finished games against this leader yet.");
    };
    let percent = report
        .win_rate
        .map(|rate| format!(" · {}%", (rate * 100.0).round() as u32))
        .unwrap_or_default();
    let mut block = line(&format!(
        "{}-{}  {}{percent}",
        report.wins, report.losses, report.standing
    ));
    for note in report.notes.iter().take(4) {
        block.push_str(&line(note));
    }
    block
}

fn combat_block(payload: &StateUpdatePayload) -> String {
    let Some(analysis) = payload.combat_analysis.as_ref() else {
        return line("No active combat.");
    };
    line(&format!(
        "{:?} · Δ{} · counter {}",
        analysis.survival_status, analysis.power_differential, analysis.required_counter
    ))
}

fn coach_block(payload: &StateUpdatePayload) -> String {
    let hint = if !payload.phase_coach.is_empty() {
        payload.phase_coach.as_str()
    } else {
        payload
            .strategy
            .as_ref()
            .map(|s| s.action.description.as_str())
            .unwrap_or("Waiting on a position.")
    };
    let mut block = line(hint);
    if let Some(brief) = payload.deck_strategy.as_ref() {
        if !brief.your_plan.is_empty() {
            block.push_str(&line(&truncate(&brief.your_plan, WIDTH - 4)));
        }
    }
    block
}

fn section(title: &str) -> String {
    line(title)
}

fn rule(left: char, fill: char, right: char) -> String {
    let inner = fill.to_string().repeat(WIDTH - 2);
    format!("{left}{inner}{right}\n")
}

fn line(text: &str) -> String {
    let trimmed = truncate(text, WIDTH - 4);
    format!("│ {trimmed:<width$} │\n", width = WIDTH - 4)
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}
