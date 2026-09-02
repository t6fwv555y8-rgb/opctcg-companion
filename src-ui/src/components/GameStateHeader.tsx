import type { GameState, LegalAction } from "../types/companion";

interface Props {
  gameState: GameState | null;
  legalActions: LegalAction[];
}

export function GameStateHeader({ gameState, legalActions }: Props) {
  if (!gameState) {
    return (
      <div className="hud-panel p-3">
        <div className="hud-title">Game State</div>
        <p className="text-xs text-slate-400">Loading...</p>
      </div>
    );
  }

  const active = gameState.players[gameState.active_player];

  return (
    <div className="hud-panel p-3">
      <div className="flex items-center justify-between">
        <div className="hud-title">Turn {gameState.turn_number}</div>
        <span className="rounded bg-hud-accent/20 px-2 py-0.5 text-[10px] font-semibold text-hud-accent">
          {gameState.phase}
        </span>
      </div>
      <div className="mt-2 grid grid-cols-2 gap-2 text-xs">
        <div>
          <span className="text-slate-400">Active P{gameState.active_player + 1}</span>
          <div className="stat-value">Life {active.life}</div>
        </div>
        <div>
          <span className="text-slate-400">DON!!</span>
          <div className="stat-value">
            {active.don_active}/{active.don_active + active.don_rested}
          </div>
        </div>
        <div>
          <span className="text-slate-400">Hand</span>
          <div className="stat-value">{active.hand_count}</div>
        </div>
        <div>
          <span className="text-slate-400">Legal Moves</span>
          <div className="stat-value">{legalActions.length}</div>
        </div>
      </div>
    </div>
  );
}
