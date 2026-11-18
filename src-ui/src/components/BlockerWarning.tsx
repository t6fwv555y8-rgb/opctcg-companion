import { GlossaryText } from "./Glossary";
import type { CombatAnalysis, CombatState } from "../types/game";

interface Props {
  combat: CombatState | null;
  analysis: CombatAnalysis | null;
}

export function BlockerWarning({ combat, analysis }: Props) {
  const show =
    combat?.blocker_offered ||
    analysis?.blocker_available ||
    analysis?.recommended_block;

  if (!show) return null;

  const urgent = analysis?.recommended_block || combat?.blocker_offered;

  return (
    <div className={`hud-panel p-3 ${urgent ? "border-hud-warn animate-pulse" : ""}`}>
      <div className="hud-title text-hud-warn">
        <GlossaryText text="Blocker warning" />
      </div>
      <p className="text-xs text-slate-300">
        {combat?.blocker_offered
          ? "A Blocker can step in on this attack"
          : "A Blocker is ready on the field"}
      </p>
      {combat?.blocker_id && (
        <div className="mt-1 font-mono text-xs text-hud-accent">
          {combat.blocker_id}
        </div>
      )}
    </div>
  );
}
