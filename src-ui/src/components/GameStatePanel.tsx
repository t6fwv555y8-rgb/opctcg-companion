import type { GameStateDto } from "../types/game";

interface Props {
  gameState: GameStateDto | null;
}

function BoardList({
  title,
  leaderId,
  leaderPower,
  cards,
}: {
  title: string;
  leaderId: string;
  leaderPower: number;
  cards: { card_id: string; rested: boolean; attached_don: number; power: number }[];
}) {
  return (
    <div>
      <div className="text-[9px] uppercase tracking-wide text-slate-500">{title}</div>
      <div className="mt-0.5 font-mono text-[10px] text-slate-200">
        Leader {leaderId || "?"} · {leaderPower}
      </div>
      {cards.length === 0 ? (
        <div className="text-[10px] text-slate-500">Empty board</div>
      ) : (
        <ul className="mt-1 space-y-0.5">
          {cards.map((c, i) => (
            <li key={`${c.card_id}-${i}`} className="font-mono text-[10px] text-slate-300">
              {c.card_id}
              {c.rested ? " · rested" : " · active"}
              {c.attached_don > 0 ? ` · +${c.attached_don} DON` : ""}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

export function GameStatePanel({ gameState }: Props) {
  if (!gameState) {
    return (
      <div className="hud-panel p-3">
        <div className="hud-title">Board</div>
        <p className="text-xs text-slate-400">Waiting for events...</p>
      </div>
    );
  }

  const self = gameState.player_one;
  const opp = gameState.player_two;

  return (
    <div className="hud-panel p-3">
      <div className="flex items-center justify-between">
        <div className="hud-title">Board · Turn {gameState.turn_number}</div>
        <span className="rounded bg-hud-accent/20 px-2 py-0.5 text-[10px] font-semibold text-hud-accent">
          {gameState.phase}
        </span>
      </div>

      <div className="mt-2 grid grid-cols-2 gap-2 text-xs">
        <div>
          <span className="text-slate-400">You</span>
          {(self.deck_name || self.leader_id) && (
            <div className="truncate text-[10px] text-hud-accent">
              {self.deck_name || self.leader_id}
            </div>
          )}
          <div className="stat-value">Life {self.life}</div>
          <div className="text-[10px] text-slate-500">
            DON {self.active_don} active / {self.rested_don} rested · Hand{" "}
            {self.hand_count}
          </div>
        </div>
        <div>
          <span className="text-slate-400">Opponent</span>
          {(opp.deck_name || opp.leader_id) && (
            <div className="truncate text-[10px] text-hud-accent">
              {opp.deck_name || opp.leader_id}
            </div>
          )}
          <div className="stat-value">Life {opp.life}</div>
          <div className="text-[10px] text-slate-500">
            DON {opp.active_don}/{opp.rested_don} · Hand {opp.hand_count} · Board{" "}
            {opp.board_count}
          </div>
        </div>
      </div>

      <div className="mt-3 grid grid-cols-2 gap-3 border-t border-slate-700/50 pt-2">
        <BoardList
          title="Your board"
          leaderId={self.leader_id}
          leaderPower={self.leader_power}
          cards={self.board ?? []}
        />
        <BoardList
          title="Opponent board"
          leaderId={opp.leader_id}
          leaderPower={opp.leader_power}
          cards={opp.board ?? []}
        />
      </div>

      {gameState.last_event && (
        <div className="mt-2 rounded bg-slate-800/50 px-2 py-1 text-[10px] text-slate-400">
          #{gameState.last_event.sequence} {gameState.last_event.summary}
        </div>
      )}
    </div>
  );
}
