import type {
  CombatAnalysis,
  CombatDoThis,
  CombatState,
} from "./types/game";

function fmtPower(n: number): string {
  return n >= 1000 && n % 1000 === 0 ? `${n / 1000}k` : String(n);
}

function targetLabel(combat: CombatState | null): string {
  if (combat?.target_is_leader) {
    return combat.target_player === 0 ? "your leader" : "their leader";
  }
  return combat?.target_player === 0 ? "your character" : "their character";
}

function defending(combat: CombatState | null, analysis: CombatAnalysis | null): boolean {
  if (combat?.target_player === 0) return true;
  if (combat?.target_player === 1) return false;
  if (combat?.attacker_player === 1) return true;
  if (combat?.attacker_player === 0) return false;
  return Boolean(
    analysis &&
      combat?.target_is_leader &&
      (analysis.lethal_to_leader ||
        analysis.recommended_block ||
        analysis.required_counter > 0),
  );
}

/** Client fallback when the HUD has combat math but no combat_coach yet. */
export function battleDoThis(
  combat: CombatState | null,
  analysis: CombatAnalysis | null,
): CombatDoThis | null {
  if (!combat?.active && !analysis) return null;

  const youDefend = defending(combat, analysis);
  const target = targetLabel(combat);

  if (analysis) {
    const need = fmtPower(analysis.required_counter);
    if (youDefend) {
      if (analysis.lethal_to_leader) {
        const canBlock = analysis.blocker_available || Boolean(combat?.blocker_offered);
        return {
          line: canBlock
            ? `Block this or counter ${need} — this swing is lethal.`
            : `Counter ${need} or you lose — this swing is lethal.`,
          steps: [
            ...(canBlock ? ["Block this swing with a ready character."] : []),
            ...(analysis.required_counter > 0
              ? [`Or counter ${need} to keep the life.`]
              : []),
            "If you take it, you lose.",
          ],
        };
      }
      if (
        analysis.recommended_block ||
        (combat?.blocker_offered && !analysis.survives_without_counter)
      ) {
        return {
          line: `Block this swing at ${target}.`,
          steps: [
            "Activate a ready blocker.",
            analysis.required_counter > 0
              ? `If you don't block, you need ${need} counter.`
              : "If you don't block, take the hit.",
          ],
        };
      }
      if (analysis.required_counter > 0 && !analysis.survives_without_counter) {
        return {
          line: `Counter ${need} or take the hit on ${target}.`,
          steps: [
            `Play ${need} counter if this body / life is worth the card.`,
            "Otherwise take the hit and keep the cards.",
          ],
        };
      }
      if (analysis.survives_without_counter) {
        return {
          line: `This swing doesn't break ${target} — take it.`,
          steps: [
            "Don't spend a blocker or counter here.",
            "Resolve and get back to the turn.",
          ],
        };
      }
    } else if (analysis.lethal_to_leader) {
      return {
        line: `This swing is lethal on ${target} — go through.`,
        steps: [
          "Confirm the attack.",
          "Watch for a blocker before it resolves.",
        ],
      };
    } else if (analysis.required_counter > 0) {
      return {
        line: `Swing at ${target}. They need ${need} to live.`,
        steps: [
          `Attack ${target}.`,
          `They must counter ${need} or lose the body / life.`,
        ],
      };
    } else {
      return {
        line: `Swing at ${target} — they don't break this.`,
        steps: [`Attack ${target}.`, "Resolve and continue the turn."],
      };
    }
  }

  if (combat?.blocker_offered) {
    return {
      line: "Blocker window — decide now.",
      steps: [
        `They're swinging at ${target}.`,
        "Block, counter, or take the hit.",
      ],
    };
  }
  if (youDefend) {
    return {
      line: `They're swinging at ${target} — block, counter, or take it.`,
      steps: [
        "Compare powers on this attack.",
        "Block or counter only if the save is worth the card.",
      ],
    };
  }
  if (combat?.active) {
    return {
      line: `You're attacking ${target} — resolve this swing.`,
      steps: [
        `Confirm the attack on ${target}.`,
        "Watch for their blocker, then resolve.",
      ],
    };
  }
  return {
    line: "A battle is open — resolve this swing before anything else.",
    steps: [
      "Identify the attacker and the target.",
      "Decide block, counter, or take the hit.",
    ],
  };
}
