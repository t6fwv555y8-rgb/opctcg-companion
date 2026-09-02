import type { GameStateDto } from "../types/game";

interface Props {
  gameState: GameStateDto | null;
}

export function TurnIndicator({ gameState }: Props) {
  if (!gameState) return null;

  const isP1 = gameState.active_player === 0;

  return (
    <div className="flex items-center gap-2 px-1">
      <span
        className={`h-2 w-2 rounded-full ${isP1 ? "bg-hud-accent" : "bg-slate-500"}`}
      />
      <span className="text-[10px] uppercase tracking-wider text-slate-400">
        {isP1 ? "Player 1 Turn" : "Player 2 Turn"} · {gameState.phase}
      </span>
    </div>
  );
}
