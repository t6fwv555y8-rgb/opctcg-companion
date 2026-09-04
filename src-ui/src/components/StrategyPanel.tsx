import type { GameStateDto, StrategyRecommendation } from "../types/game";

interface Props {
  strategy: StrategyRecommendation | null;
  options?: StrategyRecommendation[];
  phaseCoach?: string | null;
  gameState?: GameStateDto | null;
  paused?: boolean;
}

export function StrategyPanel({
  strategy,
  options = [],
  phaseCoach,
  gameState,
  paused = false,
}: Props) {
  return (
    <div className="hud-panel space-y-2 p-3">
      <div className="flex items-center justify-between">
        <div className="hud-title">Coach · Next Steps</div>
        {gameState && (
          <span className="rounded bg-hud-accent/20 px-2 py-0.5 text-[10px] font-semibold text-hud-accent">
            {gameState.phase}
          </span>
        )}
      </div>

      {phaseCoach && (
        <p className="rounded border border-hud-accent/20 bg-hud-accent/10 px-2 py-1 text-[11px] leading-snug text-hud-accent">
          {phaseCoach}
        </p>
      )}

      {paused && (
        <p className="text-[10px] text-amber-400/90">
          State confidence is low — options shown are provisional.
        </p>
      )}

      {strategy ? (
        <div className="rounded border border-hud-success/30 bg-hud-success/5 px-2 py-1.5">
          <div className="text-[9px] uppercase tracking-wide text-hud-success/80">
            Best line
          </div>
          <div className="font-mono text-sm text-hud-success">
            {strategy.action.description}
          </div>
          <div className="mt-1 flex gap-3 text-[10px] text-slate-400">
            <span>
              Score{" "}
              <span className="text-hud-accent">{strategy.score.toFixed(1)}</span>
            </span>
            <span>
              Conf{" "}
              <span className="text-slate-200">
                {(strategy.confidence * 100).toFixed(0)}%
              </span>
            </span>
          </div>
          <p className="mt-1 text-[10px] leading-relaxed text-slate-400">
            {strategy.reasoning}
          </p>
        </div>
      ) : (
        <p className="text-xs text-slate-400">Waiting for readable board state…</p>
      )}

      {options.length > 0 && (
        <div>
          <div className="mb-1 text-[9px] uppercase tracking-wide text-slate-500">
            All options this step
          </div>
          <ol className="max-h-40 space-y-1 overflow-y-auto">
            {options.map((opt, i) => (
              <li
                key={`${opt.action.description}-${i}`}
                className={`rounded px-2 py-1 text-[10px] ${
                  i === 0
                    ? "bg-slate-800/80 text-slate-100"
                    : "bg-slate-900/40 text-slate-300"
                }`}
              >
                <div className="flex items-start justify-between gap-2">
                  <span>
                    <span className="mr-1 text-slate-500">{i + 1}.</span>
                    {opt.action.description}
                  </span>
                  <span className="shrink-0 font-mono text-hud-accent">
                    {opt.score.toFixed(1)}
                  </span>
                </div>
              </li>
            ))}
          </ol>
        </div>
      )}
    </div>
  );
}
