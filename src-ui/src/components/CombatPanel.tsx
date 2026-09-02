import type { CombatAnalysis, CombatState } from "../types/game";

interface Props {
  combat: CombatState | null;
  analysis: CombatAnalysis | null;
}

function fmt(n: number): string {
  return n >= 1000 ? `${(n / 1000).toFixed(0)}k` : String(n);
}

export function CombatPanel({ combat, analysis }: Props) {
  const isCritical = analysis?.lethal_to_leader || analysis?.recommended_block;

  if (!combat?.active && !analysis) {
    return (
      <div className="hud-panel p-3">
        <div className="hud-title">Combat</div>
        <p className="text-xs text-slate-400">No active combat</p>
      </div>
    );
  }

  return (
    <div
      className={`hud-panel p-3 ${isCritical ? "border-hud-warn" : ""}`}
    >
      <div className="hud-title mb-2">Combat Math</div>
      {analysis && (
        <div className="space-y-2">
          <div className="grid grid-cols-2 gap-2">
            <div>
              <div className="text-[10px] uppercase text-slate-400">ATK</div>
              <div className="stat-value text-hud-accent">
                {fmt(analysis.attacker_power)}
              </div>
            </div>
            <div>
              <div className="text-[10px] uppercase text-slate-400">DEF</div>
              <div className="stat-value">{fmt(analysis.defender_power)}</div>
            </div>
          </div>
          <div className="rounded border border-slate-600/50 bg-slate-800/50 p-2">
            <div className="text-[10px] text-slate-400">Delta</div>
            <div
              className={`stat-value ${analysis.power_differential > 0 ? "text-hud-danger" : "text-hud-success"}`}
            >
              {analysis.power_differential > 0 ? "+" : ""}
              {fmt(analysis.power_differential)}
            </div>
          </div>
          <div className="flex justify-between text-xs">
            <span className="text-slate-300">Required Counter</span>
            <span className="stat-value text-hud-warn">
              {fmt(analysis.required_counter)}
            </span>
          </div>
          <div className="flex justify-between text-xs">
            <span className="text-slate-300">Available Counter</span>
            <span className="stat-value">
              {fmt(analysis.calculation.available_counter)}
            </span>
          </div>
          <div className="flex flex-wrap gap-1">
            {analysis.survival_status === "LETHAL" && (
              <span className="rounded bg-hud-danger/20 px-2 py-0.5 text-[10px] font-bold text-hud-danger">
                LETHAL
              </span>
            )}
            {analysis.calculation.survives && (
              <span className="rounded bg-hud-success/20 px-2 py-0.5 text-[10px] text-hud-success">
                SURVIVES
              </span>
            )}
            {analysis.survival_status === "COUNTER_REQUIRED" && (
              <span className="rounded bg-hud-warn/20 px-2 py-0.5 text-[10px] text-hud-warn">
                COUNTER REQUIRED
              </span>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
