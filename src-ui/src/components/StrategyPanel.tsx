import type { StrategyRecommendation } from "../types/game";

interface Props {
  strategy: StrategyRecommendation | null;
}

export function StrategyPanel({ strategy }: Props) {
  return (
    <div className="hud-panel p-3">
      <div className="hud-title mb-2">Strategy</div>
      {strategy ? (
        <>
          <div className="font-mono text-sm text-hud-success">
            {strategy.action.description}
          </div>
          <div className="mt-1 flex gap-3 text-xs">
            <span>
              Score{" "}
              <span className="stat-value text-hud-accent">
                {strategy.score.toFixed(1)}
              </span>
            </span>
            <span>
              Conf{" "}
              <span className="stat-value">
                {(strategy.confidence * 100).toFixed(0)}%
              </span>
            </span>
          </div>
          <p className="mt-2 text-[10px] leading-relaxed text-slate-400">
            {strategy.reasoning}
          </p>
        </>
      ) : (
        <p className="text-xs text-slate-400">No recommendation yet</p>
      )}
    </div>
  );
}
