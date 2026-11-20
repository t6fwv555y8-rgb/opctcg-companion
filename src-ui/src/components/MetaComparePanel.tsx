import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useState } from "react";
import type { CardAdviceDto, DeckComparisonDto } from "../types/game";
import { GlossaryText } from "./Glossary";

interface Props {
  /** Re-run the comparison when the active deck changes. */
  deckKey: string;
}

function copies(n: number): string {
  return `${Math.round(n * 10) / 10}x`;
}

function List({
  title,
  hint,
  rows,
  render,
}: {
  title: string;
  hint?: string;
  rows: CardAdviceDto[];
  render: (row: CardAdviceDto) => string;
}) {
  if (rows.length === 0) return null;
  return (
    <div>
      <div className="text-[10px] uppercase tracking-wide text-slate-400">{title}</div>
      {hint && <p className="text-[10px] text-slate-500">{hint}</p>}
      <ul className="mt-1 space-y-0.5">
        {rows.map((row) => (
          <li key={row.card_id} className="text-[11px] leading-snug text-slate-200" title={row.card_id}>
            <span className="font-semibold">{row.name}</span>
            <span className="text-slate-400"> — {render(row)}</span>
          </li>
        ))}
      </ul>
    </div>
  );
}

/** Your active deck set against what tournament top finishers actually play. */
export function MetaComparePanel({ deckKey }: Props) {
  const [result, setResult] = useState<DeckComparisonDto | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  // null means "the format with the most lists".
  const [format, setFormat] = useState<string | null>(null);
  // "all", "official" (in-person) or "online".
  const [events, setEvents] = useState<string>("all");

  const run = useCallback(async (reload: boolean, chosen: string | null, kind: string) => {
    setBusy(true);
    setError(null);
    try {
      if (reload) await invoke<number>("reload_meta_data");
      setResult(await invoke<DeckComparisonDto>("compare_deck_to_meta", {
          format: chosen,
          events: kind,
        }));
    } catch (e) {
      setResult(null);
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }, []);

  useEffect(() => {
    void run(false, format, events);
  }, [run, deckKey, format, events]);

  return (
    <div className="space-y-2">
      {busy && !result && <p className="text-[11px] text-slate-400">Comparing…</p>}
      {error && <p className="text-[11px] text-amber-300">{error}</p>}

      {result && (
        <>
          <p className="text-[11px] leading-snug text-slate-300">
            {result.lists > 0 ? (
              <>
                {result.has_list ? "Your " : "Winning "}
                <span className="font-semibold text-slate-100">{result.leader_name}</span>{" "}
                {result.has_list ? "deck versus" : "lists:"} {result.lists} winning list
                {result.lists === 1 ? "" : "s"}{result.has_list ? " with the same Leader" : ""}
                {result.official_lists + result.unofficial_lists > 0 &&
                  ` (${result.official_lists} from in-person events, ${result.unofficial_lists} from online events)`}
                .
              </>
            ) : (
              <>
                No winning lists in the data use{" "}
                <span className="font-semibold text-slate-100">{result.leader_name}</span> yet.
              </>
            )}
          </p>

          {result.formats.length > 1 && (
            <div className="flex flex-wrap items-center gap-1 text-[10px]">
              <span className="text-slate-400">Format:</span>
              {result.formats.map((f) => (
                <button
                  key={f.format}
                  type="button"
                  onClick={() => setFormat(f.format)}
                  className={`rounded border px-1.5 py-0.5 ${
                    f.format === result.format
                      ? "border-hud-accent/60 bg-hud-accent/15 text-hud-accent"
                      : "border-slate-600/60 text-slate-300 hover:bg-slate-800/60"
                  }`}
                  title={`${f.lists} lists in the data`}
                >
                  {f.format}
                </button>
              ))}
            </div>
          )}

          {result.all_official_lists > 0 && result.all_unofficial_lists > 0 && (
            <div className="flex flex-wrap items-center gap-1 text-[10px]">
              <span className="text-slate-400">Events:</span>
              {[
                { key: "all", label: "All" },
                { key: "official", label: `In-person (${result.all_official_lists})` },
                { key: "online", label: `Online (${result.all_unofficial_lists})` },
              ].map((o) => (
                <button
                  key={o.key}
                  type="button"
                  onClick={() => setEvents(o.key)}
                  className={`rounded border px-1.5 py-0.5 ${
                    o.key === result.events
                      ? "border-hud-accent/60 bg-hud-accent/15 text-hud-accent"
                      : "border-slate-600/60 text-slate-300 hover:bg-slate-800/60"
                  }`}
                >
                  {o.label}
                </button>
              ))}
            </div>
          )}

          {result.your_build && (
            <p className="text-[11px] leading-snug text-slate-300">
              Your list looks like the{" "}
              <span className="font-semibold text-slate-100">{result.your_build.label}</span> (
              {result.your_build.lists} of {result.lists} winning lists
              {result.builds.length > 1 && `; ${result.builds.length} different builds exist`}).
            </p>
          )}

          {result.violations.length > 0 && (
            <div className="rounded border border-hud-danger/50 bg-hud-danger/10 p-2">
              <div className="text-[11px] font-semibold text-hud-danger">
                Not legal in Standard
              </div>
              <ul className="mt-1 space-y-0.5">
                {result.violations.map((v, i) => (
                  <li key={`${v.card_id}-${i}`} className="text-[11px] leading-snug text-slate-100" title={v.card_id}>
                    <span className="font-semibold">{v.name}</span>
                    <span className="text-slate-300"> — {v.reason}</span>
                  </li>
                ))}
              </ul>
              <p className="mt-1 text-[10px] text-slate-400">
                This only matters for Standard play. Casual and Extra games on the simulator may
                still allow these cards.
              </p>
            </div>
          )}

          {result.enough_data && result.lists < 10 && (
            <p className="text-[10px] text-amber-300">
              Small sample ({result.lists} lists): treat these numbers as a hint.
            </p>
          )}

          {result.lists > 0 && !result.enough_data && (
            <p className="text-[11px] text-amber-300">
              Only {result.lists} list{result.lists === 1 ? "" : "s"}: too few to judge your deck.
            </p>
          )}

          {result.lists === 0 && result.available_leaders.length > 0 && (
            <p className="text-[10px] text-slate-400">
              Leaders with data: {result.available_leaders.map((l) => `${l.name} (${l.lists})`).join(", ")}
            </p>
          )}

          {result.enough_data && !result.has_list && (
            <div className="space-y-2">
              <List
                title="What winners with this Leader play"
                rows={result.popular}
                render={(r) => `in ${r.in_lists} of ${result.lists} lists, usually ${copies(r.avg_copies)}`}
              />
              <p className="text-[10px] text-slate-500">
                Add your deck list (Your deck, below) to see how yours compares.
              </p>
            </div>
          )}

          {result.enough_data && result.has_list && (
            <div className="space-y-2">
              <List
                title="Most winners also play"
                rows={result.missing}
                render={(r) => `in ${r.in_lists} of ${result.lists} lists, usually ${copies(r.avg_copies)}`}
              />
              <List
                title="You run fewer copies than most"
                rows={result.fewer_copies}
                render={(r) => `you ${r.your_copies}x, winners average ${copies(r.avg_copies)}`}
              />
              <List
                title="Few winners play these"
                hint="Not wrong by itself: it just means they are unusual."
                rows={result.rarely_played}
                render={(r) => `${r.your_copies}x in your deck, in only ${r.in_lists} of ${result.lists} lists`}
              />
              <p className="text-[11px] text-slate-300">
                <GlossaryText text="Counter cards" />: you play {result.your_counter_cards}, winners
                average {Math.round(result.avg_counter_cards * 10) / 10}.
              </p>
              {result.missing.length === 0 &&
                result.fewer_copies.length === 0 &&
                result.rarely_played.length === 0 && (
                  <p className="text-[11px] text-hud-success">
                    Your list is very close to what winners play.
                  </p>
                )}
            </div>
          )}

          {result.unknown_cards > 0 && (
            <p className="text-[10px] text-slate-500">
              {result.unknown_cards} cards in your list aren&apos;t in the card database, so they
              are left out of the counts.
            </p>
          )}
          <p className="text-[10px] text-slate-500">
            Based on top-cut lists from{" "}
            {result.source.replace(/^https?:\/\//, "")}
            {result.generated_at && `, downloaded ${result.generated_at.slice(0, 10)}`}. Winners are
            a guide, not a guarantee.
            {result.banlist_updated
              ? ` Ban list as of ${result.banlist_updated}.`
              : " No ban list downloaded, so legality isn't checked."}
          </p>
        </>
      )}

      <button
        type="button"
        onClick={() => void run(true, format, events)}
        disabled={busy}
        className="rounded border border-slate-600/60 px-2 py-0.5 text-[10px] text-slate-300 hover:bg-slate-800/60 disabled:opacity-50"
      >
        {busy ? "Working…" : "Reload data"}
      </button>
    </div>
  );
}
