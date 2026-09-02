import type { CombatAnalysis, CombatState } from "../types/companion";

interface Props {
  combat: CombatState | null;
  analysis: CombatAnalysis | null;
}

function formatPower(n: number): string {
  return n >= 1000 ? `${(n / 1000).toFixed(0)}k` : String(n);
}

export function CombatMathPanel({ combat, analysis }: Props) {
  if (!combat?.active && !analysis) {
    return (
      <div className="hud-panel p-3">
        <div className="hud-title mb-2">Combat Math</div>
        <p className="text-xs text-slate-400">No active combat</p>
      </div>
    );
  }

  return (
    <div className="hud-panel p-3">
      <div className="hud-title mb-2">Combat Math</div>
      {analysis ? (
        <div className="space-y-2">
          <div className="grid grid-cols-2 gap-2">
            <div>
              <div className="text-[10px] uppercase text-slate-400">ATK</div>
              <div className="stat-value text-hud-accent">
                {formatPower(analysis.attacker_power)}
              </div>
            </div>
            <div>
              <div className="text-[10px] uppercase text-slate-400">DEF</div>
              <div className="stat-value">
                {formatPower(analysis.defender_power)}
              </div>
            </div>
          </div>
          <div className="rounded border border-slate-600/50 bg-slate-800/50 p-2">
            <div className="text-[10px] uppercase text-slate-400">
              Differential
            </div>
            <div
              className={`stat-value text-lg ${analysis.power_differential > 0 ? "text-hud-danger" : "text-hud-success"}`}
            >
              {analysis.power_differential > 0 ? "+" : ""}
              {formatPower(analysis.power_differential)}
            </div>
          </div>
          <div className="flex items-center justify-between text-xs">
            <span className="text-slate-300">Counter Needed</span>
            <span className="stat-value text-hud-warn">
              {formatPower(analysis.required_counter)}
            </span>
          </div>
          <div className="flex items-center justify-between text-xs">
            <span className="text-slate-300">Shield Required</span>
            <span className="stat-value">{analysis.shield_needed}</span>
          </div>
          <div className="flex flex-wrap gap-1">
            {analysis.lethal_to_leader && (
              <span className="rounded bg-hud-danger/20 px-2 py-0.5 text-[10px] text-hud-danger">
                LETHAL
              </span>
            )}
            {analysis.survives_without_counter && (
              <span className="rounded bg-hud-success/20 px-2 py-0.5 text-[10px] text-hud-success">
                SURVIVES
              </span>
            )}
            {!analysis.survives_without_counter &&
              analysis.survives_with_base_counter && (
                <span className="rounded bg-hud-warn/20 px-2 py-0.5 text-[10px] text-hud-warn">
                  COUNTER OK
                </span>
              )}
          </div>
        </div>
      ) : (
        <p className="text-xs text-slate-400">Calculating...</p>
      )}
    </div>
  );
}
