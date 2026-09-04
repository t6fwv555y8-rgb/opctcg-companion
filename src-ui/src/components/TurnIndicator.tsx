import type { DeckInfoDto, GameStateDto } from "../types/game";

interface Props {
  gameState: GameStateDto | null;
  yourDeck?: DeckInfoDto | null;
  opponentDeck?: DeckInfoDto | null;
}

export function TurnIndicator({
  gameState,
  yourDeck = null,
  opponentDeck = null,
}: Props) {
  if (!gameState) return null;

  const isP1 = gameState.active_player === 0;
  const matchup =
    yourDeck || opponentDeck
      ? `${yourDeck?.name ?? "You"} vs ${opponentDeck?.name ?? "Opp"}`
      : null;

  return (
    <div className="flex flex-col gap-0.5 px-1">
      <div className="flex items-center gap-2">
        <span
          className={`h-2 w-2 rounded-full ${isP1 ? "bg-hud-accent" : "bg-slate-500"}`}
        />
        <span className="text-[10px] uppercase tracking-wider text-slate-400">
          {isP1 ? "Player 1 Turn" : "Player 2 Turn"} · {gameState.phase}
        </span>
      </div>
      {matchup && (
        <div className="truncate pl-4 text-[10px] text-slate-300">{matchup}</div>
      )}
    </div>
  );
}
