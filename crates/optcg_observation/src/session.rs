use chrono::{DateTime, Utc};
use optcg_core::GameState;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub type GameSessionId = Uuid;

/// Sync confidence indicator for HUD.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SyncState {
    #[default]
    Synced,
    Partial,
    Degraded,
}

impl SyncState {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Synced => "SYNCED",
            Self::Partial => "PARTIAL",
            Self::Degraded => "DEGRADED",
        }
    }
}

/// Active observation session bound to one authoritative source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameSession {
    pub id: GameSessionId,
    pub source: crate::types::ObservationSource,
    pub started_at: DateTime<Utc>,
    pub state: GameState,
    pub observation_sequence: u64,
    pub event_sequence: u64,
    pub confidence: f32,
}

impl GameSession {
    pub fn new(source: crate::types::ObservationSource) -> Self {
        Self {
            id: Uuid::new_v4(),
            source,
            started_at: Utc::now(),
            state: GameState::new(),
            observation_sequence: 0,
            event_sequence: 0,
            confidence: 1.0,
        }
    }

    pub fn reset_for_source(&mut self, source: crate::types::ObservationSource) {
        *self = Self::new(source);
    }

    /// Snapshot of leaders and swings so a reconnect can put them back.
    pub fn carry_match(&self) -> MatchCarry {
        MatchCarry::capture(&self.state)
    }

    pub fn restore_match(&mut self, carry: &MatchCarry) {
        carry.restore(&mut self.state);
    }

    pub fn sync_state(&self) -> SyncState {
        if self.confidence >= 0.85 {
            SyncState::Synced
        } else if self.confidence >= 0.5 {
            SyncState::Partial
        } else {
            SyncState::Degraded
        }
    }
}

/// Leaders and swings from a live match, restored after a tab refresh
/// rebuilds `GameState` without meaning a new game.
#[derive(Debug, Clone)]
pub struct MatchCarry {
    page_state: String,
    sides: [CarriedSide; 2],
}

#[derive(Debug, Clone)]
struct CarriedSide {
    leader_id: String,
    leader_name: String,
    observed: bool,
    swings: u32,
}

impl MatchCarry {
    pub fn capture(state: &GameState) -> Self {
        Self {
            page_state: state.page_state.clone(),
            sides: [
                CarriedSide::from_player(state.player_one()),
                CarriedSide::from_player(state.player_two()),
            ],
        }
    }

    pub fn restore(&self, state: &mut GameState) {
        let old_match = matches!(self.page_state.as_str(), "match" | "ended");
        let new_match = matches!(state.page_state.as_str(), "match" | "ended" | "");
        if !old_match || !new_match {
            return;
        }
        for (i, side) in self.sides.iter().enumerate() {
            let player = &mut state.players[i];
            if side.observed && !player.leader.observed {
                if !side.leader_id.is_empty() {
                    player.set_leader_id(&side.leader_id);
                }
                if !side.leader_name.is_empty() {
                    player.set_leader_name(&side.leader_name);
                }
            }
            if player.swings < side.swings {
                player.swings = side.swings;
            }
        }
    }
}

impl CarriedSide {
    fn from_player(player: &optcg_core::PlayerState) -> Self {
        Self {
            leader_id: if player.leader.observed {
                player.leader.card_id.clone()
            } else {
                String::new()
            },
            leader_name: player.leader_name.clone(),
            observed: player.leader.observed,
            swings: player.swings,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ObservationSource;

    #[test]
    fn a_refresh_puts_leaders_and_swings_back() {
        let mut session = GameSession::new(ObservationSource::BrowserSimulator);
        session.state.page_state = "match".into();
        session.state.player_one_mut().set_leader_id("OP13-001");
        session.state.player_one_mut().set_leader_name("Silvers Rayleigh");
        session.state.player_one_mut().swings = 5;
        session.state.player_two_mut().set_leader_id("OP17-079");
        session.state.player_two_mut().swings = 3;

        let carry = session.carry_match();
        session.reset_for_source(ObservationSource::BrowserSimulator);
        assert!(!session.state.player_one().leader.observed);
        assert_eq!(session.state.player_one().swings, 0);

        session.restore_match(&carry);
        assert!(session.state.player_one().leader.observed);
        assert_eq!(session.state.player_one().leader.card_id, "OP13-001");
        assert_eq!(session.state.player_one().leader_name, "Silvers Rayleigh");
        assert_eq!(session.state.player_one().swings, 5);
        assert_eq!(session.state.player_two().leader.card_id, "OP17-079");
        assert_eq!(session.state.player_two().swings, 3);
    }

    #[test]
    fn lobby_state_is_not_treated_as_the_same_match() {
        let mut session = GameSession::new(ObservationSource::BrowserSimulator);
        session.state.page_state = "lobby".into();
        session.state.player_one_mut().set_leader_id("OP13-001");
        session.state.player_one_mut().swings = 5;
        let carry = session.carry_match();
        session.reset_for_source(ObservationSource::BrowserSimulator);
        session.restore_match(&carry);
        assert_eq!(session.state.player_one().swings, 0);
        assert!(!session.state.player_one().leader.observed);
    }
}
