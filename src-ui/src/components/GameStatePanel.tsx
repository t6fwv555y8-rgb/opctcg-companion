import type { GameStateDto } from "../types/game";

interface Props {
  gameState: GameStateDto | null;
}

export function GameStatePanel({ gameState }: Props) {
  if (!gameState) {
    return (
      <div className="hud-panel p-3">
        <div className="hud-title">Game State</div>
        <p className="text-xs text-slate-400">Waiting for events...</p>
      </div>
    );
  }

  const active =
    gameState.active_player === 0
      ? gameState.player_one
      : gameState.player_two;
  const opponent =
    gameState.active_player === 0
      ? gameState.player_two
      : gameState.player_one;

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
          <span className="text-slate-400">You (P{gameState.active_player + 1})</span>
          <div className="stat-value">Life {active.life}</div>
          <div className="text-[10px] text-slate-500">
            DON {active.active_don}/{active.active_don + active.rested_don}
          </div>
        </div>
        <div>
          <span className="text-slate-400">Opponent</span>
          <div className="stat-value">Life {opponent.life}</div>
          <div className="text-[10px] text-slate-500">
            Board {opponent.board_count}
          </div>
        </div>
        <div>
          <span className="text-slate-400">Hand</span>
          <div className="stat-value">{active.hand_count}</div>
        </div>
        <div>
          <span className="text-slate-400">Event Seq</span>
          <div className="stat-value">{gameState.event_sequence}</div>
        </div>
      </div>
      {gameState.last_event && (
        <div className="mt-2 rounded bg-slate-800/50 px-2 py-1 text-[10px] text-slate-400">
          #{gameState.last_event.sequence} {gameState.last_event.summary}
        </div>
      )}
    </div>
  );
}
