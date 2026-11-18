import { useState } from "react";
import type {
  DeckCollectionDto,
  DeckInfoDto,
  PastedDeckDto,
  SavedDeckDto,
} from "../types/game";
import { ImportButton } from "./ImportButton";
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
  }) => void | Promise<void>;
  onActivateDeck?: (id: string) => void | Promise<void>;
  onDeleteDeck?: (id: string) => void | Promise<void>;
  onRenameDeck?: (id: string, name: string) => void | Promise<void>;
  onClearPaste?: () => void | Promise<void>;
  /** Called after decks are imported, so the list is re-read. */
  onImported?: () => void;
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
        <div className="text-[10px] uppercase tracking-wide text-slate-500">
          {label}
        </div>
        <div className="mt-0.5 text-[10px] text-slate-500">Not detected yet</div>
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
      <div className="flex items-center gap-1 text-[10px] uppercase tracking-wide text-slate-500">
        <span>{label}</span>
        {deck.from_paste && (
          <span className="rounded bg-hud-accent/20 px-1 text-[10px] text-hud-accent">
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
      </div>
      {list || known.length > 0 ? (
        <details className="mt-1">
          <summary className="cursor-pointer select-none text-[10px] text-slate-400">
            Cards ({list ? deck.list_total_cards || list.length : deck.known_cards.length}
            {list ? "" : " seen"})
          </summary>
          <ul className="mt-1 max-h-28 space-y-0.5 overflow-y-auto">
            {(list
              ? list.slice(0, 16).map((c) => ({ ...c, qty: c.quantity as number | null }))
              : known.map((c) => ({ ...c, qty: null as number | null }))
            ).map((c) => (
              <li
                key={c.card_id}
                className="truncate text-[10px] text-slate-400"
                title={c.card_id}
              >
                {c.qty !== null && (
                  <span className="font-mono text-hud-accent">{c.qty}× </span>
                )}
                <span className="text-slate-200">{c.name}</span>
              </li>
            ))}
            {!list && extra > 0 && (
              <li className="text-[10px] text-slate-500">+{extra} more seen</li>
            )}
          </ul>
        </details>
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
  onImported,
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
    <details className="hud-panel">
      <summary className="flex cursor-pointer select-none items-center justify-between gap-2 px-3 py-2">
        <span className="hud-title">Your deck</span>
        <span className="min-w-0 truncate text-[10px] text-slate-400">
          {active ? active.name : yourDeck?.name ?? "Read from your game"}
        </span>
      </summary>
      <div className="p-3 pt-0">
      <div className="flex items-center justify-end gap-2">
        {manageable && (
          <button
            type="button"
            onClick={() => setOpen((v) => !v)}
            className="rounded border border-slate-600/60 px-2 py-0.5 text-[10px] text-slate-300 hover:bg-slate-800/60"
          >
            {open ? "Hide" : savedCount > 0 ? `My deck lists (${savedCount})` : "Add a deck list"}
          </button>
        )}
      </div>

      {active && !open ? (
        <div className="mt-1 truncate text-[10px] text-slate-400">
          Using <span className="text-slate-100">{active.name}</span>
        </div>
      ) : (
        !active &&
        !open && (
          <p className="mt-1 text-[10px] leading-snug text-slate-400">
            The coach reads your Leader and cards from the game. Adding your deck list is
            optional: it lets the coach and the winning-deck comparison know your exact 50 cards.
          </p>
        )
      )}

      <div className="mt-2 grid grid-cols-2 gap-3">
        <Side label="You" deck={yourDeck} />
        <Side label="Opponent" deck={opponentDeck} />
      </div>

      <div className="mt-2">
        <ImportButton command="import_optcgsim_decks" label="Import my decks from OPTCGSim" onDone={onImported} />
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
    </details>
  );
}
