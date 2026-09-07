import type { GameStateDto, HudOperatingStateKind } from "../types/game";

interface Props {
  gameState: GameStateDto | null;
  yourName: string;
  theirName: string;
  yourLeader: string;
  theirLeader: string;
  hudState: HudOperatingStateKind | null;
  sourceLabel: string | null;
}

function whoseTurn(gs: GameStateDto): string {
  return gs.active_player === 0 ? "Your turn" : "Their turn";
}

function cleanLeader(name: string | null | undefined): string {
  const n = name?.trim() ?? "";
  if (!n || n === "Unknown leader") return "";
  return n;
}

export function MatchBar({
  gameState,
  yourName,
  theirName,
  yourLeader,
  theirLeader,
  hudState,
  sourceLabel,
}: Props) {
  const page = gameState?.page_state ?? "";
  const queued = page === "queue";
  const live = hudState === "live" || queued;
  const you = queued ? "–" : (gameState?.player_one.life ?? "–");
  const them = queued ? "–" : (gameState?.player_two.life ?? "–");
  const youLeader = cleanLeader(yourLeader);
  const themLeader = cleanLeader(theirLeader);

  const status = queued
    ? "In queue"
    : page === "lobby"
      ? "In lobby"
      : page === "match" && hudState === "live"
        ? "Live"
        : hudState && hudState !== "live"
          ? hudState
          : null;

  return (
    <header className="shrink-0 border-b border-slate-700/60 px-3 py-2">
      <div className="flex items-center justify-between gap-2 text-xs text-slate-400">
        <span className="flex items-center gap-1.5">
          <span className={`pulse-dot ${live ? "connected" : "disconnected"}`} />
          {sourceLabel ?? "Searching"}
          {status ? ` · ${status}` : ""}
        </span>
        {gameState && page === "match" && (
          <span className="font-medium text-slate-200">
            {whoseTurn(gameState)} · {gameState.phase}
          </span>
        )}
        {queued && (
          <span className="font-medium text-hud-accent">Waiting for a match</span>
        )}
      </div>
      <div className="mt-2 grid grid-cols-[1fr_auto_1fr] items-end gap-2">
        <div className="min-w-0">
          <div className="text-3xl font-semibold tabular-nums leading-none text-white">
            {you}
          </div>
          <div className="mt-1 truncate text-sm text-slate-200">
            {yourName || "You"}
          </div>
          {youLeader && (
            <div className="truncate text-xs text-slate-400">{youLeader}</div>
          )}
        </div>
        <div className="pb-1 text-xs uppercase tracking-wide text-slate-500">
          vs
        </div>
        <div className="min-w-0 text-right">
          <div className="text-3xl font-semibold tabular-nums leading-none text-white">
            {them}
          </div>
          <div className="mt-1 truncate text-sm text-slate-200">
            {theirName || "Opponent"}
          </div>
          {themLeader && (
            <div className="truncate text-xs text-slate-400">{themLeader}</div>
          )}
        </div>
      </div>
    </header>
  );
}
