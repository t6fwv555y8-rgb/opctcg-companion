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
      <div className="hud-title text-hud-warn">Blocker Warning</div>
      <p className="text-xs text-slate-300">
        {combat?.blocker_offered
          ? "Blocker window open — decide now"
          : "Blocker available on field"}
      </p>
      {combat?.blocker_id && (
        <div className="mt-1 font-mono text-xs text-hud-accent">
          {combat.blocker_id}
        </div>
      )}
    </div>
  );
}
