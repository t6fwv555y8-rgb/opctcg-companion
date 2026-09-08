import type { GameStateDto, HudOperatingStateKind } from "../types/game";

interface Props {
  gameState: GameStateDto | null;
  yourName: string;
  theirName: string;
  hudState: HudOperatingStateKind | null;
  sourceLabel: string | null;
}

function Seat({
  you,
  seat,
  player,
  life,
  swings,
  hand,
  don,
}: {
  you?: boolean;
  seat: string;
  player: string;
  life: number | string;
  swings: number;
  hand?: number;
  don?: number;
}) {
  const meta = [
    player && player !== "You" && player !== "Opponent" ? player : null,
    `${swings} ${swings === 1 ? "swing" : "swings"}`,
    hand != null ? `${hand} cards` : null,
    don != null ? `${don} DON` : null,
  ]
    .filter(Boolean)
    .join(" · ");

  return (
    <div className={`scoreboard-seat ${you ? "you" : "them"}`}>
      <div className="scoreboard-row">
        <div className="min-w-0">
          <div className="scoreboard-leader">{seat}</div>
          <div className="scoreboard-meta">{meta}</div>
        </div>
        <div className="scoreboard-life" aria-label={`${life} life`}>
          {life}
        </div>
      </div>
    </div>
  );
}

export function MatchBar({
  gameState,
  yourName,
  theirName,
  hudState,
  sourceLabel,
}: Props) {
  const page = gameState?.page_state ?? "";
  const queued = page === "queue";
  const live = hudState === "live" || queued;
  const youLife = queued ? "–" : (gameState?.player_one.life ?? "–");
  const themLife = queued ? "–" : (gameState?.player_two.life ?? "–");
  const inMatch = (page === "match" || page === "ended") && !queued;
  const yourTurn = gameState?.active_player === 0;
  const clock = queued
    ? "QUEUE"
    : page === "lobby"
      ? "LOBBY"
      : page === "ended"
        ? "END"
        : gameState
          ? `TURN ${gameState.turn_number} · ${gameState.phase}`
          : "—";
  const whose = !inMatch ? "" : yourTurn ? "Your turn" : "Their turn";

  return (
    <header className="scoreboard">
      <div className="scoreboard-ticker">
        <span className="flex items-center gap-1.5">
          <span className={`pulse-dot ${live ? "connected" : "disconnected"}`} />
          {sourceLabel ?? "Searching"}
        </span>
        <span className="scoreboard-brand">
          {clock}
          {whose ? ` · ${whose}` : ""}
        </span>
        <span>Life</span>
      </div>
      <div className="scoreboard-stack">
        <Seat
          seat="Them"
          player={theirName || "Opponent"}
          life={themLife}
          swings={gameState?.player_two.swings ?? 0}
          hand={inMatch ? gameState?.player_two.hand_count : undefined}
          don={inMatch ? gameState?.player_two.active_don : undefined}
        />
        <Seat
          you
          seat="You"
          player={yourName || "You"}
          life={youLife}
          swings={gameState?.player_one.swings ?? 0}
          hand={inMatch ? gameState?.player_one.hand_count : undefined}
          don={inMatch ? gameState?.player_one.active_don : undefined}
        />
      </div>
    </header>
  );
}
