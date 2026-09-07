import type { GameStateDto, HudOperatingStateKind } from "../types/game";

interface Props {
  gameState: GameStateDto | null;
  yourName: string;
  theirName: string;
  yourLeader: string;
  theirLeader: string;
  yourLeaderId?: string;
  theirLeaderId?: string;
  yourColor?: string;
  theirColor?: string;
  hudState: HudOperatingStateKind | null;
  sourceLabel: string | null;
}

function formatLeader(
  name: string | null | undefined,
  id: string | null | undefined,
): string {
  const n = name?.trim() ?? "";
  const card = id?.trim() ?? "";
  if (n && n !== "Unknown leader" && n !== "—" && card && n !== card) {
    return `${n}  ${card}`;
  }
  if (n && n !== "Unknown leader") return n;
  return card;
}

function LifePips({ life }: { life: number | string }) {
  const n = typeof life === "number" ? life : 0;
  const slots = Math.max(5, n);
  return (
    <div className="life-pips" aria-label={`${n} life`}>
      {Array.from({ length: slots }, (_, i) => (
        <span key={i} className={i < n ? "pip on" : "pip"} />
      ))}
      <span className="life-count">{typeof life === "number" ? n : life}</span>
    </div>
  );
}

function Seat({
  you,
  seat,
  player,
  leader,
  color,
  life,
  swings,
  hand,
  don,
}: {
  you?: boolean;
  seat: string;
  player: string;
  leader: string;
  color?: string;
  life: number | string;
  swings: number;
  hand?: number;
  don?: number;
}) {
  return (
    <div className={`scoreboard-seat ${you ? "you" : "them"}`}>
      <div className="scoreboard-role">{seat}</div>
      <div className="scoreboard-leader" style={color ? { color } : undefined}>
        {leader || (you ? "Your leader" : "Their leader")}
      </div>
      <div className="scoreboard-player">{player}</div>
      <LifePips life={life} />
      <div className="scoreboard-meta">
        {swings} {swings === 1 ? "swing" : "swings"}
        {hand != null && don != null && (
          <>
            <span className="mx-1.5 opacity-40">·</span>
            {hand} cards · {don} DON
          </>
        )}
      </div>
    </div>
  );
}

export function MatchBar({
  gameState,
  yourName,
  theirName,
  yourLeader,
  theirLeader,
  yourLeaderId,
  theirLeaderId,
  yourColor,
  theirColor,
  hudState,
  sourceLabel,
}: Props) {
  const page = gameState?.page_state ?? "";
  const queued = page === "queue";
  const live = hudState === "live" || queued;
  const youLife = queued ? "–" : (gameState?.player_one.life ?? "–");
  const themLife = queued ? "–" : (gameState?.player_two.life ?? "–");
  const youLeader = formatLeader(yourLeader, yourLeaderId);
  const themLeader = formatLeader(theirLeader, theirLeaderId);
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
        <span className="scoreboard-brand">OPTCG · LIFE</span>
        <span>{clock}</span>
      </div>
      <div className="scoreboard-stack">
        <Seat
          seat="Them · top of table"
          player={theirName || "Opponent"}
          leader={themLeader}
          color={theirColor}
          life={themLife}
          swings={gameState?.player_two.swings ?? 0}
          hand={inMatch ? gameState?.player_two.hand_count : undefined}
          don={inMatch ? gameState?.player_two.active_don : undefined}
        />
        <div className="scoreboard-mid">
          <div className="scoreboard-clock">{clock}</div>
          {whose && <div className="scoreboard-poss">{whose}</div>}
        </div>
        <Seat
          you
          seat="You · bottom of table"
          player={yourName || "You"}
          leader={youLeader}
          color={yourColor}
          life={youLife}
          swings={gameState?.player_one.swings ?? 0}
          hand={inMatch ? gameState?.player_one.hand_count : undefined}
          don={inMatch ? gameState?.player_one.active_don : undefined}
        />
      </div>
    </header>
  );
}
