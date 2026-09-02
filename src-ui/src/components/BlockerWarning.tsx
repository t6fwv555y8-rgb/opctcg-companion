import type { CombatAnalysis, CombatState } from "../types/companion";

interface Props {
  combat: CombatState | null;
  analysis: CombatAnalysis | null;
}

export function BlockerWarning({ combat, analysis }: Props) {
  const showWarning =
    combat?.blocker_offered ||
    analysis?.blocker_available ||
    analysis?.recommended_block;

  if (!showWarning) {
    return null;
  }

  const shouldBlock = analysis?.recommended_block ?? combat?.blocker_offered;

  return (
    <div
      className={`hud-panel p-3 ${shouldBlock ? "border-hud-warn" : "border-hud-border"}`}
    >
      <div className="flex items-center gap-2">
        <span className="text-lg">{shouldBlock ? "⚠️" : "🛡️"}</span>
        <div>
          <div className="hud-title">Blocker Status</div>
          <p className="text-xs text-slate-300">
            {combat?.blocker_offered
              ? "Blocker window open — decide now"
              : analysis?.blocker_available
                ? "Blocker available on field"
                : "No blocker available"}
          </p>
        </div>
      </div>
      {combat?.blocker_id && (
        <div className="mt-2 rounded bg-slate-800/60 px-2 py-1">
          <span className="text-[10px] text-slate-400">Candidate: </span>
          <span className="font-mono text-xs text-hud-accent">
            {combat.blocker_id}
          </span>
        </div>
      )}
      {shouldBlock && (
        <div className="mt-2 animate-pulse text-xs font-semibold text-hud-warn">
          RECOMMENDED: Activate Blocker
        </div>
      )}
    </div>
  );
}
