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
  align,
}: {
  life: string | number;
  name: string;
  leader: string;
  don: number | null;
  align: "left" | "right";
}) {
  return (
    <div className={`min-w-0 ${align === "right" ? "text-right" : ""}`}>
      <div className={`flex items-baseline gap-2 ${align === "right" ? "justify-end" : ""}`}>
        <span className="text-[28px] font-semibold tabular-nums leading-none text-white">
          {life}
        </span>
        {don != null && (
          <span className="text-[11px] tabular-nums text-slate-500">{don} DON</span>
        )}
      </div>
      <div className="mt-1 truncate text-[13px] text-slate-200">{name}</div>
      {leader && (
        <div className="truncate text-[11px] text-slate-500">{leader}</div>
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
  const inMatch = page === "match" && !queued;

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
        {queued && (
          <span className="font-medium text-hud-accent">Waiting for a match</span>
        )}
      </div>
      <div className="mt-1.5 grid grid-cols-[1fr_auto_1fr] items-end gap-2">
        <Side
          life={you}
          name={yourName || "You"}
          leader={youLeader}
          don={inMatch ? (gameState?.player_one.active_don ?? 0) : null}
          align="left"
        />
        <div className="pb-1 text-[10px] uppercase tracking-[0.18em] text-slate-600">
          vs
        </div>
        <Side
          life={them}
          name={theirName || "Opponent"}
          leader={themLeader}
          don={inMatch ? (gameState?.player_two.active_don ?? 0) : null}
          align="right"
        />
      </div>
    </header>
  );
}
