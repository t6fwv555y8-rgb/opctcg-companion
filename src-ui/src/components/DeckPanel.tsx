import { useState } from "react";
import type {
  DeckCollectionDto,
  DeckInfoDto,
  DeckOrigin,
  PastedDeckDto,
  SavedDeckDto,
  Side as DeckSide,
} from "../types/game";
import { DeckCollectionPanel } from "./DeckCollectionPanel";

interface Props {
  yourDeck: DeckInfoDto | null;
  opponentDeck: DeckInfoDto | null;
  pastedDeck?: PastedDeckDto | null;
  collection?: DeckCollectionDto | null;
  applying?: boolean;
  onSaveDeck?: (args: {
    raw: string;
    name?: string;
    id?: string;
    side?: DeckSide;
  }) => void | Promise<void>;
  onActivateDeck?: (id: string) => void | Promise<void>;
  onDeleteDeck?: (id: string) => void | Promise<void>;
  onRenameDeck?: (id: string, name: string) => void | Promise<void>;
  onClearPaste?: () => void | Promise<void>;
  onSetDeckSource?: (
    side: DeckSide,
    deckId: string | null,
  ) => void | Promise<void>;
}

/// How a reading is described in the HUD, in the same words the coach is given.
const ORIGIN_BADGE: Record<DeckOrigin, { text: string; hint: string } | null> = {
  observed: null,
  presumed: {
    text: "read",
    hint: "Matched to a saved list by leader. A guess, not confirmed.",
  },
  attached: {
    text: "list",
    hint: "An exact list you supplied for this side.",
  },
};

/// Pick where one side's deck comes from: read from play, or a saved list.
function SourcePicker({
  side,
  deck,
  collection,
  busy,
  onSetDeckSource,
}: {
  side: DeckSide;
  deck: DeckInfoDto | null;
  collection: DeckCollectionDto | null;
  busy: boolean;
  onSetDeckSource: (side: DeckSide, deckId: string | null) => void | Promise<void>;
}) {
  const decks = collection?.decks ?? [];
  const attached = deck?.origin === "attached" ? deck.deck_id ?? "" : "";

  return (
    <select
      value={attached}
      disabled={busy}
      onChange={(e) => onSetDeckSource(side, e.target.value || null)}
      title={
        attached
          ? "Using the list you picked for this side"
          : "Reading this side from play: leader plus revealed cards"
      }
      className="mt-1 w-full rounded border border-slate-700/60 bg-slate-900/60 px-1 py-0.5 text-[9px] text-slate-300 disabled:opacity-50"
    >
      <option value="">Read from play</option>
      {decks.map((saved) => (
        <option key={saved.id} value={saved.id}>
          {saved.name}
        </option>
      ))}
    </select>
  );
}

function Side({
  label,
  deck,
  side,
  collection = null,
  busy = false,
  onSetDeckSource,
}: {
  label: string;
  deck: DeckInfoDto | null;
  side: DeckSide;
  collection?: DeckCollectionDto | null;
  busy?: boolean;
  onSetDeckSource?: (side: DeckSide, deckId: string | null) => void | Promise<void>;
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

  const list = deck.list_entries?.length ? deck.list_entries : null;
  const known = deck.known_cards.slice(0, 12);
  const extra = Math.max(0, deck.known_cards.length - known.length);
  const badge = ORIGIN_BADGE[deck.origin];

  return (
    <div>
      <div className="flex items-center gap-1 text-[9px] uppercase tracking-wide text-slate-500">
        <span>{label}</span>
        {badge && (
          <span
            title={badge.hint}
            className={
              deck.origin === "presumed"
                ? "rounded bg-amber-400/20 px-1 text-[8px] text-amber-300"
                : "rounded bg-hud-accent/20 px-1 text-[8px] text-hud-accent"
            }
          >
            {badge.text}
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
      {onSetDeckSource && (
        <SourcePicker
          side={side}
          deck={deck}
          collection={collection}
          busy={busy}
          onSetDeckSource={onSetDeckSource}
        />
      )}
      {deck.origin === "presumed" && (
        <div className="mt-1 text-[9px] leading-tight text-amber-300/80">
          Likely list, matched on leader — not confirmed.
        </div>
      )}
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

function activeDeckLabel(collection: DeckCollectionDto | null): SavedDeckDto | null {
  if (!collection?.active_id) return null;
  return collection.decks.find((deck) => deck.is_active) ?? null;
}

export function DeckPanel({
  yourDeck,
  opponentDeck,
  pastedDeck = null,
  collection = null,
  applying = false,
  onSaveDeck,
  onActivateDeck,
  onDeleteDeck,
  onRenameDeck,
  onClearPaste,
  onSetDeckSource,
}: Props) {
  const [open, setOpen] = useState(false);
  const manageable =
    Boolean(onSaveDeck) &&
    Boolean(onActivateDeck) &&
    Boolean(onDeleteDeck) &&
    Boolean(onRenameDeck);
  const active = activeDeckLabel(collection);
  const savedCount = collection?.decks.length ?? 0;

  return (
    <div className="hud-panel p-3">
      <div className="flex items-center justify-between gap-2">
        <div className="hud-title">Decks</div>
        {manageable && (
          <button
            type="button"
            onClick={() => setOpen((v) => !v)}
            className="rounded border border-slate-600/60 px-2 py-0.5 text-[10px] text-slate-300 hover:bg-slate-800/60"
          >
            {open ? "Hide decks" : savedCount > 0 ? `My decks (${savedCount})` : "Add deck"}
          </button>
        )}
      </div>

      {active && !open && (
        <div className="mt-1 truncate text-[9px] text-slate-500">
          Using <span className="text-hud-accent">{active.name}</span>
        </div>
      )}

      <div className="mt-2 grid grid-cols-2 gap-3">
        <Side
          label="You"
          side="you"
          deck={yourDeck}
          collection={collection}
          busy={applying}
          onSetDeckSource={onSetDeckSource}
        />
        <Side
          label="Opponent"
          side="opponent"
          deck={opponentDeck}
          collection={collection}
          busy={applying}
          onSetDeckSource={onSetDeckSource}
        />
      </div>

      {open && onSaveDeck && onActivateDeck && onDeleteDeck && onRenameDeck && (
        <div className="mt-2">
          <DeckCollectionPanel
            collection={collection}
            warnings={pastedDeck?.warnings ?? []}
            busy={applying}
            onSave={onSaveDeck}
            onActivate={onActivateDeck}
            onDelete={onDeleteDeck}
            onRename={onRenameDeck}
            onClearActive={onClearPaste}
          />
        </div>
      )}
    </div>
  );
}
