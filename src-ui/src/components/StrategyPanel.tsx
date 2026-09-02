import type { RecommendationsPayload } from "../types/companion";

interface Props {
  recommendations: RecommendationsPayload | null;
}

export function StrategyPanel({ recommendations }: Props) {
  const topBeam = recommendations?.beam?.[0];
  const mcts = recommendations?.mcts;

  return (
    <div className="hud-panel p-3">
      <div className="hud-title mb-2">Optimal Strategy</div>

      {topBeam ? (
        <div className="mb-3">
          <div className="text-xs text-slate-400">Beam Search Top Pick</div>
          <div className="mt-1 font-mono text-sm text-white">
            {topBeam.action.description}
          </div>
          <div className="mt-1 flex items-center gap-2">
            <span className="text-[10px] text-slate-400">Score</span>
            <span className="stat-value text-hud-accent">
              {topBeam.score.toFixed(1)}
            </span>
          </div>
          {topBeam.sequence.length > 1 && (
            <div className="mt-2 space-y-0.5">
              <div className="text-[10px] uppercase text-slate-500">
                Sequence
              </div>
              {topBeam.sequence.slice(0, 3).map((step, i) => (
                <div key={i} className="text-[10px] text-slate-400">
                  {i + 1}. {step}
                </div>
              ))}
            </div>
          )}
        </div>
      ) : (
        <p className="mb-3 text-xs text-slate-400">Awaiting game state...</p>
      )}

      {mcts && (
        <div className="border-t border-slate-600/50 pt-2">
          <div className="text-xs text-slate-400">MCTS Best Line</div>
          <div className="mt-1 font-mono text-sm text-hud-success">
            {mcts.best_action.description}
          </div>
          <div className="mt-1 flex items-center gap-3">
            <div>
              <span className="text-[10px] text-slate-400">Win Rate </span>
              <span className="stat-value">
                {(mcts.win_rate * 100).toFixed(1)}%
              </span>
            </div>
            <div>
              <span className="text-[10px] text-slate-400">Visits </span>
              <span className="stat-value">{mcts.visits}</span>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
