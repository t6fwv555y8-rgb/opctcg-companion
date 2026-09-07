import type { MatchupReportDto } from "../types/game";

interface Props {
  report: MatchupReportDto | null;
}

function standingTone(standing: string): string {
  if (standing === "favourable") return "text-hud-accent";
  if (standing === "rough") return "text-hud-warn";
  return "text-slate-300";
}

/// Your deck's record against the leader across the table.
export function MatchupPanel({ report }: Props) {
  if (!report) {
    return (
      <div className="hud-panel p-3">
        <div className="hud-title">Matchup</div>
        <p className="mt-2 text-sm leading-relaxed text-slate-400">
          No finished games against this leader yet. Wins and losses are
          recorded as they happen — nothing is guessed.
        </p>
      </div>
    );
  }

  const finished = report.wins + report.losses;
  const percent =
    report.win_rate != null ? Math.round(report.win_rate * 100) : null;

  return (
    <div className="hud-panel p-3">
      <div className="flex items-center justify-between gap-2">
        <div className="hud-title">Matchup</div>
        <span className="text-xs text-slate-400">
          {finished} finished
          {report.unfinished > 0 ? ` · ${report.unfinished} unfinished` : ""}
        </span>
      </div>

      <div className="mt-1 flex items-baseline gap-2">
        <span className="font-mono text-2xl text-slate-100">
          {report.wins}-{report.losses}
        </span>
        <span className={`text-sm ${standingTone(report.standing)}`}>
          {report.standing}
        </span>
        {percent != null && (
          <span className="text-sm text-slate-400">{percent}%</span>
        )}
      </div>

      {report.notes.length > 0 && (
        <ul className="mt-1.5 space-y-0.5">
          {report.notes.map((note) => (
            <li key={note} className="text-sm leading-snug text-slate-300">
              {note}
            </li>
          ))}
        </ul>
      )}

      <p className="mt-2 text-xs leading-snug text-slate-500">
        Real results against this leader, never a prediction of this game.
      </p>
    </div>
  );
}
