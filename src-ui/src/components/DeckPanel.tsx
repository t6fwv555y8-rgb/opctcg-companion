import { useEffect, useState } from "react";
import type { DeckInfoDto, PastedDeckDto } from "../types/game";

interface Props {
  yourDeck: DeckInfoDto | null;
  opponentDeck: DeckInfoDto | null;
  pastedDeck?: PastedDeckDto | null;
  applying?: boolean;
  onApplyPaste?: (raw: string) => void | Promise<void>;
  onClearPaste?: () => void | Promise<void>;
}

function Side({
  label,
  deck,
}: {
  label: string;
  deck: DeckInfoDto | null;
}) {
  if (!deck) {
    return (
      <div>
        <div className="text-[9px] uppercase tracking-wide text-slate-500">
          {label}
        </div>
        <div className="mt-0.5 text-[10px] text-slate-500">Waiting…</div>
      </div>
    );
  }

  const list = deck.from_paste && deck.list_entries?.length
    ? deck.list_entries
    : null;
  const known = deck.known_cards.slice(0, 12);
  const extra = Math.max(0, deck.known_cards.length - known.length);

  return (
    <div>
      <div className="flex items-center gap-1 text-[9px] uppercase tracking-wide text-slate-500">
        <span>{label}</span>
        {deck.from_paste && (
          <span className="rounded bg-hud-accent/20 px-1 text-[8px] text-hud-accent">
            pasted
          </span>
        )}
      </div>
      <div className="mt-0.5 text-[12px] font-semibold leading-tight text-white">
        {deck.name}
      </div>
      <div className="mt-0.5 font-mono text-[10px] text-slate-300">
        {deck.leader_color ? `${deck.leader_color} · ` : ""}
        {deck.leader_name || deck.leader_id || "Leader ?"}
        {deck.leader_id ? ` · ${deck.leader_id}` : ""}
      </div>
      {list ? (
        <ul className="mt-1 max-h-28 space-y-0.5 overflow-y-auto">
          {list.slice(0, 16).map((c) => (
            <li
              key={c.card_id}
              className="truncate text-[10px] text-slate-400"
              title={`${c.card_id} · ${c.card_type}${c.rush ? " · rush" : ""}${c.blocker ? " · blocker" : ""}`}
            >
              <span className="font-mono text-hud-accent">{c.quantity}×</span>{" "}
              <span className="text-slate-200">{c.name}</span>
              <span className="ml-1 font-mono text-slate-500">{c.card_id}</span>
            </li>
          ))}
          {(deck.list_total_cards ?? 0) > 0 && (
            <li className="text-[9px] text-slate-500">
              {deck.list_total_cards} cards in list
            </li>
          )}
        </ul>
      ) : known.length > 0 ? (
        <ul className="mt-1 max-h-24 space-y-0.5 overflow-y-auto">
          {known.map((c) => (
            <li
              key={c.card_id}
              className="truncate text-[10px] text-slate-400"
              title={`${c.card_id} · ${c.card_type}`}
            >
              <span className="text-slate-200">{c.name}</span>
              <span className="ml-1 font-mono text-slate-500">{c.card_id}</span>
            </li>
          ))}
          {extra > 0 && (
            <li className="text-[10px] text-slate-500">+{extra} more seen</li>
          )}
        </ul>
      ) : (
        <div className="mt-1 text-[10px] text-slate-500">
          No cards identified yet
        </div>
      )}
    </div>
  );
}

const PLACEHOLDER = `Deck: Red Luffy Aggro
Leader: ST01-001
4x ST01-002
4x ST01-003
2x ST01-010
4x ST01-012`;

export function DeckPanel({
  yourDeck,
  opponentDeck,
  pastedDeck = null,
  applying = false,
  onApplyPaste,
  onClearPaste,
}: Props) {
  const [draft, setDraft] = useState(pastedDeck?.raw ?? "");
  const [open, setOpen] = useState(false);

  useEffect(() => {
    if (pastedDeck?.raw != null) {
      setDraft(pastedDeck.raw);
    }
  }, [pastedDeck?.raw]);

  return (
    <div className="hud-panel p-3">
      <div className="flex items-center justify-between gap-2">
        <div className="hud-title">Decks</div>
        {onApplyPaste && (
          <button
            type="button"
            onClick={() => setOpen((v) => !v)}
            className="rounded border border-slate-600/60 px-2 py-0.5 text-[10px] text-slate-300 hover:bg-slate-800/60"
          >
            {open ? "Hide paste" : "Paste list"}
          </button>
        )}
      </div>

      <div className="mt-2 grid grid-cols-2 gap-3">
        <Side label="You" deck={yourDeck} />
        <Side label="Opponent" deck={opponentDeck} />
      </div>

      {open && onApplyPaste && (
        <div className="mt-2 space-y-1.5 border-t border-slate-700/50 pt-2">
          <div className="text-[9px] uppercase tracking-wide text-slate-500">
            Paste your exact deck
          </div>
          <textarea
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            placeholder={PLACEHOLDER}
            rows={7}
            className="w-full resize-y rounded border border-slate-700 bg-slate-950/80 px-2 py-1 font-mono text-[10px] leading-snug text-slate-200 placeholder:text-slate-600 focus:border-hud-accent/50 focus:outline-none"
          />
          <p className="text-[9px] leading-snug text-slate-500">
            Formats: <span className="font-mono">4x ST01-002</span>,{" "}
            <span className="font-mono">ST01-012 x4</span>,{" "}
            <span className="font-mono">Leader: ST01-001</span>,{" "}
            <span className="font-mono">Deck: Name</span>
          </p>
          {pastedDeck?.warnings && pastedDeck.warnings.length > 0 && (
            <p className="text-[9px] text-amber-400/90">
              {pastedDeck.warnings.slice(0, 3).join(" · ")}
            </p>
          )}
          <div className="flex gap-1">
            <button
              type="button"
              disabled={applying || !draft.trim()}
              onClick={() => onApplyPaste(draft)}
              className="rounded border border-hud-accent/40 bg-hud-accent/10 px-2 py-0.5 text-[10px] font-semibold text-hud-accent hover:bg-hud-accent/20 disabled:opacity-50"
            >
              {applying ? "Applying…" : "Apply list"}
            </button>
            {onClearPaste && pastedDeck && (
              <button
                type="button"
                disabled={applying}
                onClick={() => onClearPaste()}
                className="rounded border border-slate-600/60 px-2 py-0.5 text-[10px] text-slate-300 hover:bg-slate-800/60 disabled:opacity-50"
              >
                Clear
              </button>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
