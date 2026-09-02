use crate::types::{ObservationEvent, ObservationSource};
use optcg_core::{Phase, PlayerId};

/// Text log format v1 — common OPTCGSim in-game log phrases (post-game / clipboard export).
pub fn parse_text_line(line: &str, confidence: f32) -> Result<Vec<ObservationEvent>, String> {
    let lower = line.to_ascii_lowercase();

    if lower.contains("main phase") || lower.contains("entered main") {
        return Ok(vec![ObservationEvent::PhaseObserved {
            phase: Phase::Main,
            confidence,
        }]);
    }
    if lower.contains("draw phase") {
        return Ok(vec![ObservationEvent::PhaseObserved {
            phase: Phase::Draw,
            confidence,
        }]);
    }
    if lower.contains("don!! phase") || lower.contains("don phase") {
        return Ok(vec![ObservationEvent::PhaseObserved {
            phase: Phase::Don,
            confidence,
        }]);
    }

    if let Some(count) = extract_life_change(&lower) {
        let player = if lower.contains("player 2") || lower.contains("p2") {
            PlayerId::Player2
        } else {
            PlayerId::Player1
        };
        return Ok(vec![ObservationEvent::LifeObserved {
            player,
            count,
            confidence: confidence * 0.95,
        }]);
    }

    if let Some(card_id) = extract_card_id(line) {
        let player = if lower.contains("player 2") {
            PlayerId::Player2
        } else {
            PlayerId::Player1
        };
        if lower.contains("played") || lower.contains("play ") {
            return Ok(vec![ObservationEvent::CardObserved {
                card_id: Some(card_id),
                owner: player,
                zone: optcg_core::Zone::Character,
                position: None,
                confidence,
            }]);
        }
    }

    if lower.contains("attack") && lower.contains("leader") {
        if let Some(card_id) = extract_card_id(line) {
            return Ok(vec![ObservationEvent::AttackObserved {
                attacker: None,
                attacker_card_id: Some(card_id),
                target: Some(optcg_core::AttackTarget::Leader {
                    player: PlayerId::Player2,
                }),
                observed_power: extract_power(line),
                confidence: confidence * 0.9,
            }]);
        }
    }

    Ok(vec![])
}

fn extract_life_change(lower: &str) -> Option<u8> {
    if lower.contains("lose") && lower.contains("life") {
        // "Lose 1 Life" — delta only; count unknown
        return None;
    }
    let re_patterns = [r"life[:\s]+(\d+)", r"(\d+)\s+life remaining"];
    for pat in re_patterns {
        if let Some(caps) = regex_simple(pat, lower) {
            return caps.parse().ok();
        }
    }
    None
}

fn regex_simple(pattern: &str, text: &str) -> Option<String> {
    // lightweight without regex crate — manual for life:N
    if pattern.contains("life") {
        if let Some(idx) = text.find("life") {
            let tail = &text[idx..];
            let digits: String = tail
                .chars()
                .skip_while(|c| !c.is_ascii_digit())
                .take_while(|c| c.is_ascii_digit())
                .collect();
            if !digits.is_empty() {
                return Some(digits);
            }
        }
    }
    None
}

fn extract_card_id(line: &str) -> Option<String> {
    for word in line.split_whitespace() {
        if word.len() >= 7 && word.contains('-') {
            let upper = word.to_uppercase();
            if upper.starts_with("OP")
                || upper.starts_with("ST")
                || upper.starts_with("EB")
                || upper.starts_with("P-")
            {
                return Some(
                    upper
                        .trim_matches(|c: char| !c.is_alphanumeric() && c != '-')
                        .into(),
                );
            }
        }
    }
    None
}

fn extract_power(line: &str) -> Option<u32> {
    for word in line.split_whitespace() {
        if word.len() == 4 || word.len() == 5 {
            if let Ok(n) = word.parse::<u32>() {
                if (1000..=15000).contains(&n) {
                    return Some(n);
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_main_phase_text() {
        let events = parse_text_line("Player 1 entered Main Phase", 0.95).unwrap();
        assert!(matches!(events[0], ObservationEvent::PhaseObserved { .. }));
    }

    #[test]
    fn parses_card_played() {
        let events = parse_text_line("Player 1 played OP01-001", 0.95).unwrap();
        assert!(matches!(events[0], ObservationEvent::CardObserved { .. }));
    }
}
