import { useEffect, useState } from "react";
import type { DeckCollectionDto, SavedDeckDto } from "../types/game";

interface Props {
  collection: DeckCollectionDto | null;
  warnings?: string[];
  busy?: boolean;
  onSave: (args: { raw: string; name?: string; id?: string }) => void | Promise<void>;
  onActivate: (id: string) => void | Promise<void>;
  onDelete: (id: string) => void | Promise<void>;
  onRename: (id: string, name: string) => void | Promise<void>;
  onClearActive?: () => void | Promise<void>;
}

const PLACEHOLDER = `Deck: Red Luffy Aggro
Leader: ST01-001
4x ST01-002
4x ST01-003
2x ST01-010
4x ST01-012`;

function leaderLine(deck: SavedDeckDto): string {
  const parts = [deck.leader_color, deck.leader_name ?? deck.leader_id].filter(
    (part): part is string => Boolean(part),
  );
  return parts.join(" · ");
}

function DeckRow({
  deck,
  editing,
  busy,
  onActivate,
  onEdit,
  onDelete,
  onRename,
}: {
  deck: SavedDeckDto;
  editing: boolean;
  busy: boolean;
  onActivate: () => void;
  onEdit: () => void;
  onDelete: () => void;
  onRename: (name: string) => void;
}) {
  const [renaming, setRenaming] = useState(false);
  const [renameDraft, setRenameDraft] = useState(deck.name);
  const [confirmingDelete, setConfirmingDelete] = useState(false);

  // Reset transient row UI when the deck underneath changes identity or name.
  useEffect(() => {
    setRenaming(false);
    setConfirmingDelete(false);
    setRenameDraft(deck.name);
  }, [deck.id, deck.name]);

  const commitRename = () => {
    const next = renameDraft.trim();
    setRenaming(false);
    if (next && next !== deck.name) onRename(next);
  };

  return (
    <li
      className={`rounded border px-2 py-1 ${
        deck.is_active
          ? "border-hud-accent/50 bg-hud-accent/10"
          : "border-slate-700/60 bg-slate-900/40"
      }`}
    >
      {renaming ? (
        <div className="flex items-center gap-1">
          <input
            autoFocus
            value={renameDraft}
            onChange={(e) => setRenameDraft(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") commitRename();
              if (e.key === "Escape") {
                setRenameDraft(deck.name);
                setRenaming(false);
              }
            }}
            className="min-w-0 flex-1 rounded border border-slate-700 bg-slate-950/80 px-1 py-0.5 text-[10px] text-slate-100 focus:border-hud-accent/50 focus:outline-none"
          />
          <button
            type="button"
            onClick={commitRename}
            className="rounded border border-hud-accent/40 px-1.5 py-0.5 text-[9px] text-hud-accent hover:bg-hud-accent/20"
          >
            Save
          </button>
        </div>
      ) : (
        <div className="flex items-start justify-between gap-1">
          <button
            type="button"
            onClick={onActivate}
            disabled={busy || deck.is_active}
            title={deck.is_active ? "Active deck" : "Use this deck"}
            className="min-w-0 flex-1 text-left disabled:cursor-default"
          >
            <div className="flex items-center gap-1">
              {deck.is_active && (
                <span className="h-1.5 w-1.5 shrink-0 rounded-full bg-hud-accent" />
              )}
              <span
                className={`truncate text-[11px] font-semibold ${
                  deck.is_active ? "text-hud-accent" : "text-slate-200"
                }`}
              >
                {deck.name}
              </span>
            </div>
            <div className="truncate font-mono text-[9px] text-slate-500">
              {leaderLine(deck) || "Leader ?"}
              {deck.total_cards > 0 ? ` · ${deck.total_cards} cards` : ""}
            </div>
          </button>

          <div className="flex shrink-0 items-center gap-0.5">
            {!deck.is_active && (
              <button
                type="button"
                onClick={onActivate}
                disabled={busy}
                className="rounded border border-slate-600/60 px-1.5 py-0.5 text-[9px] text-slate-300 hover:bg-slate-800/60 disabled:opacity-50"
              >
                Use
              </button>
            )}
            <button
              type="button"
              onClick={onEdit}
              className={`rounded border px-1.5 py-0.5 text-[9px] ${
                editing
                  ? "border-hud-accent/50 text-hud-accent"
                  : "border-slate-600/60 text-slate-300 hover:bg-slate-800/60"
              }`}
              title="Load this list into the editor"
            >
              Edit
            </button>
            {confirmingDelete ? (
              <>
                <button
                  type="button"
                  onClick={onDelete}
                  disabled={busy}
                  className="rounded border border-hud-danger/50 bg-hud-danger/10 px-1.5 py-0.5 text-[9px] text-hud-danger hover:bg-hud-danger/20 disabled:opacity-50"
                >
                  Confirm
                </button>
                <button
                  type="button"
                  onClick={() => setConfirmingDelete(false)}
                  className="rounded border border-slate-600/60 px-1.5 py-0.5 text-[9px] text-slate-400 hover:bg-slate-800/60"
                >
                  No
                </button>
              </>
            ) : (
              <>
                <button
                  type="button"
                  onClick={() => setRenaming(true)}
                  className="rounded border border-slate-600/60 px-1.5 py-0.5 text-[9px] text-slate-300 hover:bg-slate-800/60"
                  title="Rename deck"
                >
                  Name
                </button>
                <button
                  type="button"
                  onClick={() => setConfirmingDelete(true)}
                  className="rounded border border-slate-600/60 px-1.5 py-0.5 text-[9px] text-slate-400 hover:bg-hud-danger/20 hover:text-hud-danger"
                  title="Delete deck"
                >
                  ×
                </button>
              </>
            )}
          </div>
        </div>
      )}
    </li>
  );
}

export function DeckCollectionPanel({
  collection,
  warnings = [],
  busy = false,
  onSave,
  onActivate,
  onDelete,
  onRename,
  onClearActive,
}: Props) {
  const decks = collection?.decks ?? [];
  const maxDecks = collection?.max_decks ?? 0;
  const [draft, setDraft] = useState("");
  const [draftName, setDraftName] = useState("");
  const [editingId, setEditingId] = useState<string | null>(null);

  // Drop the editor target once that deck is gone from the collection.
  useEffect(() => {
    if (editingId && !decks.some((deck) => deck.id === editingId)) {
      setEditingId(null);
    }
  }, [decks, editingId]);

  const startEdit = (deck: SavedDeckDto) => {
    setEditingId(deck.id);
    setDraft(deck.raw);
    setDraftName(deck.name);
  };

  const resetEditor = () => {
    setEditingId(null);
    setDraft("");
    setDraftName("");
  };

  const editingDeck = editingId
    ? decks.find((deck) => deck.id === editingId) ?? null
    : null;
  const nameChanged = Boolean(
    editingDeck && draftName.trim() && draftName.trim() !== editingDeck.name,
  );
  const atCapacity = maxDecks > 0 && decks.length >= maxDecks && !editingDeck;

  const uniqueName = (base: string) => {
    const taken = (candidate: string) =>
      decks.some((deck) => deck.name.toLowerCase() === candidate.toLowerCase());
    if (!taken(base)) return base;
    for (let n = 2; n <= maxDecks + 2; n += 1) {
      const candidate = `${base} ${n}`;
      if (!taken(candidate)) return candidate;
    }
    return base;
  };

  const save = async (mode: "update" | "new") => {
    const raw = draft.trim();
    if (!raw) return;
    let name = draftName.trim();
    // Copying must not collide with the source deck, which the backend would
    // otherwise treat as an update to it.
    if (mode === "new" && editingDeck) {
      name = uniqueName(name || `${editingDeck.name} copy`);
    }
    await onSave({
      raw,
      name: name || undefined,
      id: mode === "update" ? editingId ?? undefined : undefined,
    });
    if (mode === "new") resetEditor();
  };

  return (
    <div className="space-y-2 border-t border-slate-700/50 pt-2">
      <div className="flex items-center justify-between gap-2">
        <div className="text-[9px] uppercase tracking-wide text-slate-500">
          My decks {decks.length > 0 && `(${decks.length}${maxDecks ? `/${maxDecks}` : ""})`}
        </div>
        {onClearActive && collection?.active_id && (
          <button
            type="button"
            onClick={() => onClearActive()}
            disabled={busy}
            className="rounded border border-slate-600/60 px-1.5 py-0.5 text-[9px] text-slate-400 hover:bg-slate-800/60 disabled:opacity-50"
            title="Stop using the active deck (keeps it saved)"
          >
            Deselect
          </button>
        )}
      </div>

      {decks.length > 0 ? (
        <ul className="max-h-40 space-y-1 overflow-y-auto pr-0.5">
          {decks.map((deck) => (
            <DeckRow
              key={deck.id}
              deck={deck}
              editing={deck.id === editingId}
              busy={busy}
              onActivate={() => onActivate(deck.id)}
              onEdit={() => startEdit(deck)}
              onDelete={() => onDelete(deck.id)}
              onRename={(name) => onRename(deck.id, name)}
            />
          ))}
        </ul>
      ) : (
        <p className="text-[10px] text-slate-500">
          No saved decks yet. Paste a list below to build your collection.
        </p>
      )}

      <div className="space-y-1.5 border-t border-slate-700/40 pt-2">
        <div className="flex items-center justify-between gap-2">
          <div className="text-[9px] uppercase tracking-wide text-slate-500">
            {editingDeck ? `Editing ${editingDeck.name}` : "Add a deck"}
          </div>
          {editingDeck && (
            <button
              type="button"
              onClick={resetEditor}
              className="rounded border border-slate-600/60 px-1.5 py-0.5 text-[9px] text-slate-400 hover:bg-slate-800/60"
            >
              New deck
            </button>
          )}
        </div>

        <input
          value={draftName}
          onChange={(e) => setDraftName(e.target.value)}
          placeholder="Deck name (optional — taken from the list otherwise)"
          className="w-full rounded border border-slate-700 bg-slate-950/80 px-2 py-1 text-[10px] text-slate-200 placeholder:text-slate-600 focus:border-hud-accent/50 focus:outline-none"
        />
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

        {warnings.length > 0 && (
          <p className="text-[9px] text-hud-warn">
            {warnings.slice(0, 3).join(" · ")}
          </p>
        )}
        {atCapacity && (
          <p className="text-[9px] text-hud-warn">
            Collection is full ({maxDecks}). Delete a deck to add another.
          </p>
        )}

        <div className="flex flex-wrap gap-1">
          <button
            type="button"
            disabled={busy || !draft.trim() || (atCapacity && !editingDeck)}
            onClick={() => save(editingDeck ? "update" : "new")}
            className="rounded border border-hud-accent/40 bg-hud-accent/10 px-2 py-0.5 text-[10px] font-semibold text-hud-accent hover:bg-hud-accent/20 disabled:opacity-50"
            title={
              editingDeck
                ? "Overwrite this saved deck and use it"
                : "Save to your collection and use it"
            }
          >
            {busy
              ? "Saving…"
              : editingDeck
                ? nameChanged
                  ? "Update & rename"
                  : "Update deck"
                : "Save & use"}
          </button>
          {editingDeck && (
            <button
              type="button"
              disabled={busy || !draft.trim() || (maxDecks > 0 && decks.length >= maxDecks)}
              onClick={() => save("new")}
              className="rounded border border-slate-600/60 px-2 py-0.5 text-[10px] text-slate-300 hover:bg-slate-800/60 disabled:opacity-50"
              title="Save as a separate deck (give it a new name)"
            >
              Save as copy
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
