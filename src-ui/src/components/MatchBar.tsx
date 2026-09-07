import type { GameStateDto, HudOperatingStateKind } from "../types/game";

interface Props {
  gameState: GameStateDto | null;
  yourName: string;
  theirName: string;
  hudState: HudOperatingStateKind | null;
  sourceLabel: string | null;
}

function whoseTurn(gs: GameStateDto): string {
  return gs.active_player === 0 ? "Your turn" : "Their turn";
}

export function MatchBar({
  gameState,
  yourName,
  theirName,
  hudState,
  sourceLabel,
}: Props) {
  const live = hudState === "live";
  const you = gameState?.player_one.life ?? "–";
  const them = gameState?.player_two.life ?? "–";

  return (
    <header className="shrink-0 border-b border-slate-700/60 px-3 py-2">
      <div className="flex items-center justify-between gap-2 text-xs text-slate-400">
        <span className="flex items-center gap-1.5">
          <span className={`pulse-dot ${live ? "connected" : "disconnected"}`} />
          {sourceLabel ?? "Searching"}
          {hudState && hudState !== "live" ? ` · ${hudState}` : ""}
        </span>
        {gameState && (
          <span className="font-medium text-slate-200">
            {whoseTurn(gameState)} · {gameState.phase}
          </span>
        )}
      </div>
      <div className="mt-2 grid grid-cols-[1fr_auto_1fr] items-end gap-2">
        <div className="min-w-0">
          <div className="text-3xl font-semibold tabular-nums leading-none text-white">
            {you}
          </div>
          <div className="mt-1 truncate text-sm text-slate-300">
            {yourName || "You"}
          </div>
        </div>
        <div className="pb-1 text-xs uppercase tracking-wide text-slate-500">
          vs
        </div>
        <div className="min-w-0 text-right">
          <div className="text-3xl font-semibold tabular-nums leading-none text-white">
            {them}
          </div>
          <div className="mt-1 truncate text-sm text-slate-300">
            {theirName || "Opponent"}
          </div>
        </div>
      </div>
    </header>
  );
}
