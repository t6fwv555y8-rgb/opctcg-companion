import type { DeckInfoDto } from "../types/game";

interface Props {
  yourDeck: DeckInfoDto | null;
  opponentDeck: DeckInfoDto | null;
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

  const known = deck.known_cards.slice(0, 12);
  const extra = Math.max(0, deck.known_cards.length - known.length);

  return (
    <div>
      <div className="text-[9px] uppercase tracking-wide text-slate-500">
        {label}
      </div>
      <div className="mt-0.5 text-[12px] font-semibold leading-tight text-white">
        {deck.name}
      </div>
      <div className="mt-0.5 font-mono text-[10px] text-slate-300">
        {deck.leader_color ? `${deck.leader_color} · ` : ""}
        {deck.leader_name || deck.leader_id || "Leader ?"}
        {deck.leader_id ? ` · ${deck.leader_id}` : ""}
      </div>
      {known.length > 0 ? (
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

export function DeckPanel({ yourDeck, opponentDeck }: Props) {
  return (
    <div className="hud-panel p-3">
      <div className="hud-title">Decks</div>
      <div className="mt-2 grid grid-cols-2 gap-3">
        <Side label="You" deck={yourDeck} />
        <Side label="Opponent" deck={opponentDeck} />
      </div>
    </div>
  );
}
