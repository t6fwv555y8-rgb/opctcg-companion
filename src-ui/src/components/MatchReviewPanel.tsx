import type { MatchReviewDto } from "../types/game";

interface Props {
  review: MatchReviewDto;
}

function outcomeTone(outcome: string | null | undefined): string {
  if (outcome === "won") return "text-emerald-300";
  if (outcome === "lost") return "text-rose-300";
  return "text-slate-300";
}

/// How the last game went — shown the moment the table goes quiet.
export function MatchReviewPanel({ review }: Props) {
  return (
    <section className="hud-panel px-3.5 py-3">
      <div className="flex items-center justify-between gap-2">
        <div className="hud-title text-hud-accent">Last game</div>
        {review.outcome && (
          <span
            className={`text-[11px] font-medium uppercase tracking-[0.14em] ${outcomeTone(review.outcome)}`}
          >
            {review.outcome}
          </span>
        )}
      </div>
      {review.outcome === "won" && (
        <div className="win-banner mt-2">
          <div className="text-[10px] font-semibold uppercase tracking-[0.16em] text-emerald-200">
            Rayleigh
          </div>
          <p className="mt-1 text-[16px] font-semibold leading-snug text-emerald-50">
            You won. That is how you get stronger.
          </p>
        </div>
      )}
      <p className="mt-2 text-[15px] leading-snug text-white">{review.headline}</p>
      <p className="mt-1 text-[12px] text-slate-500">
        Turn {review.last_turn || 1} · {review.your_life}–{review.their_life}
        {review.your_leader || review.their_leader
          ? ` · ${review.your_leader || "You"} vs ${review.their_leader || "Them"}`
          : ""}
      </p>
      {review.notes.length > 0 && (
        <ul className="mt-3 space-y-2">
          {review.notes.map((note) => (
            <li
              key={note}
              className="border-l border-sky-400/30 pl-3 text-[13px] leading-snug text-slate-300"
            >
              {note}
            </li>
          ))}
        </ul>
      )}
      {review.you_played.length > 0 && (
        <p className="mt-3 text-[12px] leading-snug text-slate-500">
          You showed {review.you_played.join(", ")}
        </p>
      )}
    </section>
  );
}
