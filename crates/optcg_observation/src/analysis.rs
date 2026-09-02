use serde::{Deserialize, Serialize};

/// Whether the rules engine should emit high-confidence strategy recommendations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisEligibility {
    pub eligible: bool,
    pub confidence: f32,
    pub reasons: Vec<String>,
    pub mode: AnalysisMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisMode {
    Full,
    CombatMathOnly,
    Paused,
}

impl Default for AnalysisEligibility {
    fn default() -> Self {
        Self {
            eligible: true,
            confidence: 1.0,
            reasons: vec![],
            mode: AnalysisMode::Full,
        }
    }
}

impl AnalysisEligibility {
    pub fn evaluate(
        sync_confidence: f32,
        phase_known: bool,
        life_known: bool,
        combat_active: bool,
        source_connected: bool,
    ) -> Self {
        let mut reasons = Vec::new();
        if !source_connected {
            reasons.push("source disconnected".into());
            return Self {
                eligible: false,
                confidence: 0.0,
                reasons,
                mode: AnalysisMode::Paused,
            };
        }
        if !phase_known {
            reasons.push("phase unknown".into());
        }
        if !life_known {
            reasons.push("life unknown".into());
        }

        let confidence = sync_confidence;
        if combat_active && confidence >= 0.5 {
            return Self {
                eligible: true,
                confidence,
                reasons,
                mode: AnalysisMode::CombatMathOnly,
            };
        }
        if confidence >= 0.75 && phase_known && life_known {
            return Self {
                eligible: true,
                confidence,
                reasons,
                mode: AnalysisMode::Full,
            };
        }
        if confidence >= 0.5 && (phase_known || life_known) {
            return Self {
                eligible: false,
                confidence,
                reasons,
                mode: AnalysisMode::CombatMathOnly,
            };
        }
        reasons.push("insufficient state confidence".into());
        Self {
            eligible: false,
            confidence,
            reasons,
            mode: AnalysisMode::Paused,
        }
    }

    pub fn hud_label(&self) -> Option<&'static str> {
        match self.mode {
            AnalysisMode::Full if self.eligible => None,
            AnalysisMode::CombatMathOnly => Some("PARTIAL STATE · COMBAT MATH ONLY"),
            AnalysisMode::Paused => Some("ANALYSIS PAUSED · STATE SYNCING"),
            _ => Some("PARTIAL STATE"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pauses_when_disconnected() {
        let e = AnalysisEligibility::evaluate(0.9, true, true, false, false);
        assert!(!e.eligible);
        assert_eq!(e.mode, AnalysisMode::Paused);
    }

    #[test]
    fn combat_math_only_during_combat_partial() {
        let e = AnalysisEligibility::evaluate(0.6, true, false, true, true);
        assert!(e.eligible);
        assert_eq!(e.mode, AnalysisMode::CombatMathOnly);
    }
}
