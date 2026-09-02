use crate::engine::{ActionType, LegalAction};
use optcg_core::{AttackTarget, GameEvent, GameState, Normalizer, Phase, PlayerId};

/// Apply a legal action during search simulation (no event sequencing).
pub fn simulate_action(state: &mut GameState, action: &LegalAction) {
    let player = PlayerId::from_index(action.target_player.unwrap_or(state.active_player))
        .unwrap_or(PlayerId::Player1);

    let event = match action.action_type {
        ActionType::AttachDon => GameEvent::DonAttached {
            player,
            target: action.card_id.clone().unwrap_or_else(|| "LEADER".into()),
            amount: 1,
        },
        ActionType::PlayCharacter | ActionType::PlayStage | ActionType::PlayEvent => {
            GameEvent::CardPlayed {
                player,
                card_id: action.card_id.clone().unwrap_or_default(),
                zone: Some("character".into()),
            }
        }
        ActionType::AttackLeader => GameEvent::AttackDeclared {
            attacker: action.card_id.clone().unwrap_or_default(),
            attacker_player: player,
            target: AttackTarget::Leader {
                player: PlayerId::from_index(action.target_player.unwrap_or(1 - player.index()))
                    .unwrap_or(PlayerId::Player2),
            },
            power: 0,
        },
        ActionType::AttackCharacter => GameEvent::AttackDeclared {
            attacker: action.card_id.clone().unwrap_or_default(),
            attacker_player: player,
            target: AttackTarget::Character {
                player: PlayerId::from_index(action.target_player.unwrap_or(1 - player.index()))
                    .unwrap_or(PlayerId::Player2),
                card_id: action.target_id.clone().unwrap_or_default(),
            },
            power: 0,
        },
        ActionType::ActivateBlocker => GameEvent::BlockerOffered {
            player,
            blocker_id: action.card_id.clone().unwrap_or_default(),
        },
        ActionType::EndTurn => GameEvent::TurnEnded {
            next_player: PlayerId::from_index(1 - state.active_player).unwrap_or(PlayerId::Player2),
        },
        ActionType::EndPhase | ActionType::Pass => GameEvent::PhaseChanged { phase: Phase::End },
    };

    let _ = Normalizer::apply_event(state, &event);
}
