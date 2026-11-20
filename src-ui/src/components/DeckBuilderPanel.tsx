import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useState } from "react";
import type { GeneratedCardDto, GeneratedDeckDto, GeneratorLeaderDto } from "../types/game";

interface Props {
  /** Your Leader, when the game has shown one, so it is picked first. */
  yourLeaderId: string;
  onSave: (args: { raw: string; name?: string }) => void | Promise<void>;
}

const GROUPS: { key: string; label: string }[] = [
  { key: "character", label: "Characters" },
  { key: "event", label: "Events" },
  { key: "stage", label: "Stages" },
  { key: "unknown", label: "Not in the card database" },
];

function Group({ label, cards }: { label: string; cards: GeneratedCardDto[] }) {
  if (cards.length === 0) return null;
  const total = cards.reduce((n, c) => n + c.quantity, 0);
  return (
    <div>
      <div className="text-[10px] uppercase tracking-wide text-slate-400">
        {label} ({total})
      </div>
      <ul className="mt-1 space-y-0.5">
        {cards.map((c) => (
          <li key={c.card_id} className="text-[11px] leading-snug text-slate-200" title={c.card_id}>
            <span className="font-mono text-hud-accent">{c.quantity}×</span>{" "}
            <span className="font-semibold">{c.name}</span>
            <span className="text-slate-400">
              {c.card_type !== "unknown" && ` · cost ${c.cost}`}
              {c.tags.length > 0 && ` · ${c.tags.join(", ")}`}
              {` · in ${c.in_lists} lists`}
            </span>
          </li>
        ))}
      </ul>
    </div>
  );
}

/** Builds a list from what winning decks of one Leader actually run. */
export function DeckBuilderPanel({ yourLeaderId, onSave }: Props) {
  const [leaders, setLeaders] = useState<GeneratorLeaderDto[]>([]);
  const [leader, setLeader] = useState<string>("");
  const [variant, setVariant] = useState<number | null>(null);
  const [deck, setDeck] = useState<GeneratedDeckDto | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState<string | null>(null);

  useEffect(() => {
    invoke<GeneratorLeaderDto[]>("get_generator_leaders", { format: null })
      .then((list) => {
        setLeaders(list);
        setLeader((current) => {
          if (current) return current;
          return list.find((l) => l.leader_id === yourLeaderId)?.leader_id ?? list[0]?.leader_id ?? "";
        });
      })
      .catch((e) => setError(String(e)));
  }, [yourLeaderId]);

  const build = useCallback(async (id: string, chosen: number | null) => {
    if (!id) return;
    setBusy(true);
    setError(null);
    setNote(null);
    try {
      setDeck(
        await invoke<GeneratedDeckDto>("generate_deck", {
          leaderId: id,
          format: null,
          variant: chosen,
        }),
      );
    } catch (e) {
      setDeck(null);
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }, []);

  useEffect(() => {
    void build(leader, variant);
  }, [build, leader, variant]);

  const copy = async (text: string, what: string) => {
    try {
      await navigator.clipboard.writeText(text);
      setNote(`${what} copied.`);
    } catch {
      setNote("Could not copy: select the text and copy it by hand.");
    }
  };

  if (leaders.length === 0) {
    return (
      <p className="text-[11px] leading-snug text-slate-300">
        {error ??
          "No Leader has enough winning lists yet. Run the winning-deck download (see the README), then tap Reload data in Compare to winning decks."}
      </p>
    );
  }

  const btn =
    "rounded border border-slate-600/60 px-2 py-0.5 text-[10px] text-slate-200 hover:bg-slate-800/60 disabled:opacity-50";
  return (
    <div className="space-y-2">
      <label className="block text-[10px] text-slate-400">
        Leader
        <select
          value={leader}
          onChange={(e) => {
            setVariant(null);
            setLeader(e.target.value);
          }}
          className="mt-0.5 w-full rounded border border-slate-700 bg-slate-950/80 px-2 py-1 text-[11px] text-slate-100"
        >
          {leaders.map((l) => (
            <option key={l.leader_id} value={l.leader_id}>
              {l.name} ({l.leader_id}) — {l.lists} winning lists
            </option>
          ))}
        </select>
      </label>

      {deck && deck.variants.length > 1 && (
        <div className="flex flex-wrap items-center gap-1 text-[10px]">
          <span className="text-slate-400">Build:</span>
          {deck.variants.map((v) => (
            <button
              key={v.id}
              type="button"
              onClick={() => setVariant(v.id)}
              title={`${v.lists} lists (${v.share_pct}%)`}
              className={`rounded border px-1.5 py-0.5 ${
                deck.variant?.id === v.id
                  ? "border-hud-accent/60 bg-hud-accent/15 text-hud-accent"
                  : "border-slate-600/60 text-slate-300 hover:bg-slate-800/60"
              }`}
            >
              {v.label} · {v.lists}
            </button>
          ))}
        </div>
      )}

      {busy && !deck && <p className="text-[11px] text-slate-400">Building…</p>}
      {error && <p className="text-[11px] text-amber-300">{error}</p>}

      {deck && (
        <>
          <p className="text-[11px] leading-snug text-slate-300">
            {deck.total_cards} cards from {deck.based_on_lists} winning lists ({deck.format})
            {deck.variant ? `, the ${deck.variant.label}` : ""}.
          </p>
          {GROUPS.map((g) => (
            <Group key={g.key} label={g.label} cards={deck.cards.filter((c) => c.card_type === g.key)} />
          ))}
          {deck.notes.map((n, i) => (
            <p key={i} className="text-[10px] leading-snug text-slate-400">
              {n}
            </p>
          ))}
          <div className="flex flex-wrap gap-1">
            <button type="button" className={btn} onClick={() => void copy(deck.deck_text, "Deck list")}>
              Copy list
            </button>
            <button type="button" className={btn} onClick={() => void copy(deck.builder_url, "Link")}>
              Copy Egman builder link
            </button>
            <button
              type="button"
              className={btn}
              onClick={async () => {
                await onSave({ raw: deck.deck_text, name: `${deck.leader_name} (generated)` });
                setNote("Saved to your deck lists.");
              }}
            >
              Save to my deck lists
            </button>
          </div>
          {note && <p className="text-[10px] text-hud-success">{note}</p>}
        </>
      )}
    </div>
  );
}
