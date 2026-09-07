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

function Side({
  life,
  name,
  leader,
  don,
  you,
}: {
  life: string | number;
  name: string;
  leader: string;
  don: number | null;
  you?: boolean;
}) {
  return (
    <div className={`flex items-baseline gap-2 ${you ? "" : "opacity-80"}`}>
      <span
        className={`shrink-0 font-semibold tabular-nums leading-none ${
          you ? "text-[26px] text-white" : "text-[22px] text-slate-200"
        }`}
      >
        {life}
      </span>
      <div className="min-w-0 flex-1">
        <div className={`truncate text-[13px] ${you ? "text-slate-200" : "text-slate-400"}`}>
          {name}
        </div>
        {leader && (
          <div className="truncate text-[11px] text-slate-500">{leader}</div>
        )}
      </div>
      {don != null && (
        <span className="shrink-0 text-[11px] tabular-nums text-slate-500">{don} DON</span>
      )}
    </div>
  );
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
  const inMatch = (page === "match" || page === "ended") && !queued;

  const status = queued
    ? "In queue"
    : page === "lobby"
      ? "In lobby"
      : page === "ended"
        ? "Game over"
        : page === "match" && hudState === "live"
          ? "Live"
          : hudState && hudState !== "live"
            ? hudState
            : null;

  return (
    <header className="shrink-0 border-b border-white/[0.06] px-3 py-2">
      <div className="flex items-center justify-between gap-2 text-[11px] text-slate-500">
        <span className="flex items-center gap-1.5">
          <span className={`pulse-dot ${live ? "connected" : "disconnected"}`} />
          {sourceLabel ?? "Searching"}
          {status ? ` · ${status}` : ""}
        </span>
        {gameState && page === "match" && (
          <span className="font-medium text-slate-300">
            {whoseTurn(gameState)} · {gameState.phase}
          </span>
        )}
        {page === "ended" && (
          <span className="font-medium text-slate-300">Match finished</span>
        )}
        {queued && (
          <span className="font-medium text-hud-accent">Waiting for a match</span>
        )}
      </div>
      <div className="mt-1.5 flex flex-col gap-1.5">
        <Side
          life={them}
          name={theirName || "Opponent"}
          leader={themLeader}
          don={inMatch ? (gameState?.player_two.active_don ?? 0) : null}
        />
        <Side
          life={you}
          name={yourName || "You"}
          leader={youLeader}
          don={inMatch ? (gameState?.player_one.active_don ?? 0) : null}
          you
        />
      </div>
    </header>
  );
}
