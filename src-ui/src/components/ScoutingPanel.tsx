import { useState } from "react";
import type { ScoutedCardDto, ScoutingReportDto } from "../types/game";

interface Props {
  report: ScoutingReportDto | null;
  /// Their real list is attached, so the report is withheld rather than absent.
  listAttached: boolean;
}

/// How many cards to show before asking.
const COMPACT_CARDS = 6;

/// Cards seen in at least this share of games are the deck's spine. Matches
/// `STAPLE_CONFIDENCE` in the scouting crate.
const STAPLE_CONFIDENCE = 0.6;

/// A thin sample is a warning, not a result.
function reliabilityTone(reliability: string): string {
  if (reliability === "solid") return "text-hud-accent";
  if (reliability === "fair") return "text-slate-300";
  return "text-hud-warn";
}

function CardRow({ card, games }: { card: ScoutedCardDto; games: number }) {
  const percent = Math.round(card.confidence * 100);
  const staple = card.confidence >= STAPLE_CONFIDENCE;

  return (
    <li
      className="flex items-center gap-1.5"
      title={`Seen in ${card.games_seen} of ${games} games · first appeared turn ${card.earliest_turn}`}
    >
      <span className="w-6 shrink-0 font-mono text-[10px] text-hud-accent">
        {card.likely_copies}×
      </span>
      <span className="min-w-0 flex-1 truncate text-[10px] text-slate-200">
        {card.name}
      </span>
      <span className="h-1 w-10 shrink-0 overflow-hidden rounded bg-slate-800">
        <span
          className={`block h-full ${staple ? "bg-hud-accent" : "bg-slate-600"}`}
          style={{ width: `${percent}%` }}
        />
      </span>
      <span className="w-7 shrink-0 text-right font-mono text-[9px] text-slate-500">
        {percent}%
      </span>
    </li>
  );
}

/// What we have worked out about the opponent's deck from watching them play.
export function ScoutingPanel({ report, listAttached }: Props) {
  const [expanded, setExpanded] = useState(false);

  if (!report) {
    return (
      <div className="hud-panel p-3">
        <div className="hud-title">Scouting</div>
        <p className="mt-1 text-[10px] leading-snug text-slate-500">
          {listAttached
            ? "Their list is attached, so there is nothing left to infer. Games are still recorded either way."
            : "Nothing on this leader yet. Every game you play is recorded, and the cards they show build up into a picture of their deck."}
        </p>
      </div>
    );
  }

  const shown = expanded ? report.cards : report.cards.slice(0, COMPACT_CARDS);
  const hidden = report.cards.length - shown.length;

  return (
    <div className="hud-panel p-3">
      <div className="flex items-center justify-between gap-2">
        <div className="hud-title">Scouting</div>
        <span className="text-[9px] text-slate-500">
          {report.games} game{report.games === 1 ? "" : "s"} ·{" "}
          <span className={reliabilityTone(report.reliability)}>
            {report.reliability}
          </span>
        </span>
      </div>

      <div className="mt-1 text-[10px] text-slate-300">
        Plays <span className="text-hud-accent">{report.pace}</span>
      </div>
      <div className="text-[9px] text-slate-500">
        {report.mapped_copies} of their 50 cards mapped
      </div>

      {shown.length > 0 && (
        <ul className="mt-1.5 space-y-1">
          {shown.map((card) => (
            <CardRow key={card.card_id} card={card} games={report.games} />
          ))}
        </ul>
      )}

      {(hidden > 0 || expanded) && (
        <button
          type="button"
          onClick={() => setExpanded((v) => !v)}
          className="mt-1 text-[9px] text-slate-500 hover:text-slate-300"
        >
          {expanded ? "Show less" : `Show ${hidden} more`}
        </button>
      )}

      {report.notes.length > 0 && (
        <ul className="mt-1.5 space-y-0.5 border-t border-slate-700/50 pt-1.5">
          {report.notes.map((note) => (
            <li key={note} className="text-[9px] leading-snug text-slate-400">
              {note}
            </li>
          ))}
        </ul>
      )}

      <p className="mt-1.5 text-[9px] leading-snug text-slate-500">
        Inferred from cards they have played, never a confirmed list.
      </p>
    </div>
  );
}
