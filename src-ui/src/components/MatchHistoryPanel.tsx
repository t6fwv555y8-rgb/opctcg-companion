import { useState } from "react";
import { ImportButton } from "./ImportButton";
import type { MatchHistoryDto, MatchRecordDto, TallyDto } from "../types/game";

interface Props {
  history: MatchHistoryDto | null;
  error: string | null;
  onSetResult: (id: string, result: "win" | "loss" | null) => void;
  onDelete: (id: string) => void;
  onImported?: () => void;
}

/** Below this many decided games a win rate says almost nothing. */
const ENOUGH_GAMES = 5;

function rate(wins: number, losses: number): string {
  const decided = wins + losses;
  if (decided === 0) return "no result yet";
  const pct = Math.round((wins / decided) * 100);
  return decided < ENOUGH_GAMES ? `${wins}–${losses} (too few to judge)` : `${pct}% (${wins}–${losses})`;
}

function TallyList({
  title,
  rows,
  names,
}: {
  title: string;
  rows: TallyDto[];
  names: Record<string, string>;
}) {
  if (rows.length === 0) return null;
  return (
    <div>
      <div className="text-[10px] uppercase tracking-wide text-slate-400">{title}</div>
      <ul className="mt-1 space-y-0.5">
        {rows.slice(0, 6).map((t) => (
          <li key={t.leader} className="text-[11px] leading-snug text-slate-200" title={t.leader}>
            <span className="font-semibold">{names[t.leader] ?? t.leader}</span>
            <span className="text-slate-400">
              {" "}
              — {t.games} game{t.games === 1 ? "" : "s"}, {rate(t.wins, t.losses)}
            </span>
          </li>
        ))}
      </ul>
    </div>
  );
}

function when(ms: number): string {
  try {
    return new Date(ms).toLocaleString(undefined, {
      month: "short",
      day: "numeric",
      hour: "numeric",
      minute: "2-digit",
    });
  } catch {
    return "";
  }
}

function Row({
  match,
  names,
  onSetResult,
  onDelete,
}: {
  match: MatchRecordDto;
  names: Record<string, string>;
  onSetResult: Props["onSetResult"];
  onDelete: Props["onDelete"];
}) {
  const [confirming, setConfirming] = useState(false);
  const you = names[match.your_leader] ?? match.your_leader;
  const opp = names[match.opponent_leader] ?? match.opponent_leader;
  const btn =
    "rounded border px-1.5 py-0.5 text-[10px] hover:bg-slate-800/60 border-slate-600/60 text-slate-300";
  return (
    <li className="rounded bg-slate-900/60 px-2 py-1">
      <div className="flex items-start justify-between gap-2">
        <div className="min-w-0 text-[11px] leading-snug text-slate-200">
          <span className="font-semibold">{you}</span> vs <span className="font-semibold">{opp}</span>
          <div className="text-[10px] text-slate-400">
            {when(match.ended_ms)} · {match.turns} turn{match.turns === 1 ? "" : "s"}
            {match.went_first === true && " · you went first"}
            {match.went_first === false && " · you went second"}
          </div>
        </div>
        <div className="flex shrink-0 items-center gap-1">
          {match.result ? (
            <>
              <span
                className={`rounded px-1.5 py-0.5 text-[10px] font-semibold ${
                  match.result === "win" ? "bg-hud-success/20 text-hud-success" : "bg-hud-danger/20 text-hud-danger"
                }`}
              >
                {match.result === "win" ? "Won" : "Lost"}
              </span>
              <button type="button" className={btn} onClick={() => onSetResult(match.id, null)} title="Clear this result">
                Undo
              </button>
            </>
          ) : (
            <>
              <button type="button" className={btn} onClick={() => onSetResult(match.id, "win")}>
                I won
              </button>
              <button type="button" className={btn} onClick={() => onSetResult(match.id, "loss")}>
                I lost
              </button>
            </>
          )}
          {confirming ? (
            <>
              <button type="button" className={`${btn} text-hud-danger`} onClick={() => onDelete(match.id)}>
                Delete
              </button>
              <button type="button" className={btn} onClick={() => setConfirming(false)}>
                No
              </button>
            </>
          ) : (
            <button type="button" className={btn} onClick={() => setConfirming(true)} aria-label="Delete this game">
              ×
            </button>
          )}
        </div>
      </div>
    </li>
  );
}

/** Your own results, kept on this computer. */
export function MatchHistoryPanel({ history, error, onSetResult, onDelete, onImported }: Props) {
  if (!history) {
    return <p className="text-[11px] text-slate-400">{error ?? "Loading…"}</p>;
  }
  const { stats, recent, names } = history;
  if (stats.games === 0) {
    return (
      <div className="space-y-2">
        <p className="text-[11px] leading-snug text-slate-300">
          No games yet. Play a game with the HUD open and it is saved here when it ends. Everything
          stays on your computer.
        </p>
        <ImportButton command="import_optcgsim_games" label="Import games from OPTCGSim" onDone={onImported} />
      </div>
    );
  }
  return (
    <div className="space-y-2">
      <p className="text-[11px] leading-snug text-slate-200">
        {stats.games} game{stats.games === 1 ? "" : "s"} · {rate(stats.wins, stats.losses)}
        {stats.unknown > 0 && (
          <span className="text-amber-300"> · {stats.unknown} with no result yet</span>
        )}
      </p>
      {(stats.first_games > 0 || stats.second_games > 0) && (
        <p className="text-[11px] text-slate-300">
          Going first: {rate(stats.first_wins, stats.first_games - stats.first_wins)} · Going
          second: {rate(stats.second_wins, stats.second_games - stats.second_wins)}
        </p>
      )}
      <TallyList title="With your Leaders" rows={stats.by_your_leader} names={names} />
      {stats.by_opponent_leader.length > 1 && (
        <TallyList title="Against" rows={stats.by_opponent_leader} names={names} />
      )}
      <div>
        <div className="text-[10px] uppercase tracking-wide text-slate-400">Recent games</div>
        <ul className="mt-1 max-h-64 space-y-1 overflow-y-auto">
          {recent.map((m) => (
            <Row key={m.id} match={m} names={names} onSetResult={onSetResult} onDelete={onDelete} />
          ))}
        </ul>
      </div>
      {error && <p className="text-[10px] text-hud-danger">{error}</p>}
      <ImportButton command="import_optcgsim_games" label="Import games from OPTCGSim" onDone={onImported} />
      <p className="text-[10px] text-slate-500">
        Saved on this computer only. Only Leaders and cards both players could see are recorded,
        never a hand.
      </p>
    </div>
  );
}

/** Appears after a game ends when nobody said who won. */
export function PendingResultBanner({
  history,
  onSetResult,
}: {
  history: MatchHistoryDto | null;
  onSetResult: Props["onSetResult"];
}) {
  const [skipped, setSkipped] = useState<Set<string>>(new Set());
  const latest = history?.recent[0];
  const fresh = latest ? Date.now() - latest.ended_ms < 15 * 60 * 1000 : false;
  if (!history || !latest || latest.result || !fresh || skipped.has(latest.id)) return null;
  const opp = history.names[latest.opponent_leader] ?? latest.opponent_leader;
  const btn = "rounded border px-2 py-0.5 text-[11px] font-semibold";
  return (
    <div className="rounded-lg border border-hud-accent/50 bg-hud-accent/10 px-3 py-2">
      <div className="text-[12px] text-slate-100">How did your game against {opp} end?</div>
      <div className="mt-1 flex gap-1">
        <button type="button" className={`${btn} border-hud-success/50 text-hud-success`} onClick={() => onSetResult(latest.id, "win")}>
          I won
        </button>
        <button type="button" className={`${btn} border-hud-danger/50 text-hud-danger`} onClick={() => onSetResult(latest.id, "loss")}>
          I lost
        </button>
        <button
          type="button"
          className={`${btn} border-slate-600/60 text-slate-300`}
          onClick={() => setSkipped(new Set(skipped).add(latest.id))}
        >
          Skip
        </button>
      </div>
    </div>
  );
}
