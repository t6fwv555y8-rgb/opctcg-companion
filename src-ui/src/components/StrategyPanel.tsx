import type {
  DeckStrategyBrief,
  GameStateDto,
  StrategyRecommendation,
} from "../types/game";

interface Props {
  strategy: StrategyRecommendation | null;
  options?: StrategyRecommendation[];
  phaseCoach?: string | null;
  deckStrategy?: DeckStrategyBrief | null;
  gameState?: GameStateDto | null;
  paused?: boolean;
  refreshing?: boolean;
  onRefresh?: () => void;
}

export function StrategyPanel({
  strategy,
  options = [],
  phaseCoach,
  deckStrategy = null,
  gameState,
  paused = false,
  refreshing = false,
  onRefresh,
}: Props) {
  return (
    <div className="hud-panel space-y-2 p-3">
      <div className="flex items-center justify-between gap-2">
        <div className="hud-title">Coach · Next Steps</div>
        <div className="flex items-center gap-1">
          {gameState && (
            <span className="rounded bg-hud-accent/20 px-2 py-0.5 text-[10px] font-semibold text-hud-accent">
              {gameState.phase}
            </span>
          )}
          {onRefresh && (
            <button
              type="button"
              onClick={onRefresh}
              disabled={refreshing}
              className="rounded border border-hud-accent/40 bg-hud-accent/10 px-2 py-0.5 text-[10px] font-semibold text-hud-accent hover:bg-hud-accent/20 disabled:opacity-50"
              title="Rebuild detailed strategy for these decks"
            >
              {refreshing ? "Refreshing…" : "Refresh"}
            </button>
          )}
        </div>
      </div>

      {phaseCoach && (
        <p className="rounded border border-hud-accent/20 bg-hud-accent/10 px-2 py-1 text-[11px] leading-snug text-hud-accent">
          {phaseCoach}
        </p>
      )}

      {deckStrategy && (
        <div className="space-y-1.5 rounded border border-slate-700/60 bg-slate-900/40 px-2 py-1.5">
          <div className="flex items-center justify-between gap-2">
            <div className="text-[9px] uppercase tracking-wide text-slate-500">
              Deck strategy
            </div>
            <div className="truncate text-[9px] text-slate-500">
              {deckStrategy.matchup}
            </div>
          </div>
          <p className="text-[10px] leading-relaxed text-slate-200">
            {deckStrategy.your_plan}
          </p>
          <p className="text-[10px] leading-relaxed text-slate-300">
            {deckStrategy.vs_opponent}
          </p>
          {deckStrategy.this_turn.length > 0 && (
            <div>
              <div className="mb-0.5 text-[9px] uppercase tracking-wide text-hud-accent/80">
                This turn
              </div>
              <ul className="space-y-0.5">
                {deckStrategy.this_turn.map((step, i) => (
                  <li key={i} className="text-[10px] leading-snug text-slate-300">
                    <span className="mr-1 text-slate-500">{i + 1}.</span>
                    {step}
                  </li>
                ))}
              </ul>
            </div>
          )}
          {deckStrategy.priorities.length > 0 && (
            <div>
              <div className="mb-0.5 text-[9px] uppercase tracking-wide text-slate-500">
                Priorities
              </div>
              <ul className="space-y-0.5">
                {deckStrategy.priorities.slice(0, 4).map((p, i) => (
                  <li key={i} className="text-[10px] leading-snug text-slate-400">
                    · {p}
                  </li>
                ))}
              </ul>
            </div>
          )}
          {deckStrategy.threats.length > 0 && (
            <div>
              <div className="mb-0.5 text-[9px] uppercase tracking-wide text-amber-400/80">
                Threats
              </div>
              <ul className="max-h-16 space-y-0.5 overflow-y-auto">
                {deckStrategy.threats.slice(0, 5).map((t, i) => (
                  <li key={i} className="text-[10px] leading-snug text-amber-100/80">
                    · {t}
                  </li>
                ))}
              </ul>
            </div>
          )}
        </div>
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
