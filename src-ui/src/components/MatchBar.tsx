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
  if (n && n !== "Unknown leader" && card && n !== card) return `${n}  ${card}`;
  if (n && n !== "Unknown leader") return n;
  return card;
}

function teamShort(leader: string, fallback: string): string {
  const n = leader.trim();
  if (!n) return fallback;
  const name = n.split("  ")[0]?.split("·")[0]?.trim() ?? n;
  const parts = name.split(/[\s.]+/).filter(Boolean);
  if (parts.length >= 2) return parts[parts.length - 1].slice(0, 8).toUpperCase();
  return name.slice(0, 8).toUpperCase();
}

function Team({
  label,
  player,
  leader,
  color,
  home,
}: {
  label: string;
  player: string;
  leader: string;
  color?: string;
  home?: boolean;
}) {
  return (
    <div className={`scoreboard-team ${home ? "home" : "away"}`}>
      <div className="scoreboard-role">{label} crew</div>
      <div className="scoreboard-abbr" style={color ? { color } : undefined}>
        {teamShort(leader, home ? "HOME" : "AWAY")}
      </div>
      <div className="scoreboard-leader">
        {leader || (home ? "Your leader" : "Their leader")}
      </div>
      <div className="scoreboard-player">{player}</div>
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
  const you = queued ? "–" : (gameState?.player_one.life ?? "–");
  const them = queued ? "–" : (gameState?.player_two.life ?? "–");
  const youLeader = formatLeader(yourLeader, yourLeaderId);
  const themLeader = formatLeader(theirLeader, theirLeaderId);
  const inMatch = (page === "match" || page === "ended") && !queued;
  const yourTurn = gameState?.active_player === 0;
  const clock = queued
    ? "QUEUE"
    : page === "lobby"
      ? "LOCKER"
      : page === "ended"
        ? "FINAL"
        : gameState
          ? `T${gameState.turn_number} · ${gameState.phase}`
          : "—";
  const poss = !inMatch
    ? ""
    : yourTurn
      ? "HOME POSSESSION"
      : "AWAY POSSESSION";

  return (
    <header className="scoreboard">
      <div className="scoreboard-ticker">
        <span className="flex items-center gap-1.5">
          <span className={`pulse-dot ${live ? "connected" : "disconnected"}`} />
          {sourceLabel ?? "Searching"}
        </span>
        <span className="scoreboard-brand">OPTCG · GRAND LINE</span>
        {inMatch && (
          <span>
            AWAY {gameState?.player_two.hand_count ?? 0}H/{gameState?.player_two.active_don ?? 0}D
            <span className="mx-1.5 opacity-40">·</span>
            HOME {gameState?.player_one.hand_count ?? 0}H/{gameState?.player_one.active_don ?? 0}D
          </span>
        )}
      </div>
      <div className="scoreboard-face">
        <Team
          label="Away"
          player={theirName || "Opponent"}
          leader={themLeader}
          color={theirColor}
        />
        <div className="scoreboard-mid">
          <div className="scoreboard-scores">
            <span className="scoreboard-points away-score">{them}</span>
            <span className="scoreboard-vs">
              VS
              <span className="scoreboard-life">LIFE</span>
            </span>
            <span className="scoreboard-points home-score">{you}</span>
          </div>
          <div className="scoreboard-clock">{clock}</div>
          {poss && <div className="scoreboard-poss">{poss}</div>}
        </div>
        <Team
          label="Home"
          player={yourName || "You"}
          leader={youLeader}
          color={yourColor}
          home
        />
      </div>
    </header>
  );
}
