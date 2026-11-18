import {
  createContext,
  Fragment,
  useCallback,
  useContext,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import {
  CATEGORY_LABEL,
  CATEGORY_ORDER,
  entryByKey,
  matcher,
  resolveMatch,
  searchGlossary,
  type GlossaryEntry,
} from "../data/glossary";

const STORAGE_KEY = "optcg.explainTerms";

function readEnabled(): boolean {
  try {
    return window.localStorage.getItem(STORAGE_KEY) !== "off";
  } catch {
    return true;
  }
}

interface GlossaryContextValue {
  enabled: boolean;
  setEnabled: (enabled: boolean) => void;
  current: GlossaryEntry | null;
  open: (key: string | null) => void;
}

const GlossaryContext = createContext<GlossaryContextValue>({
  enabled: false,
  setEnabled: () => {},
  current: null,
  open: () => {},
});

/** Holds "explain terms" on/off and which term is open in the explainer. */
export function GlossaryProvider({ children }: { children: ReactNode }) {
  const [enabled, setEnabledState] = useState(readEnabled);
  const [currentKey, setCurrentKey] = useState<string | null>(null);

  const setEnabled = useCallback((next: boolean) => {
    setEnabledState(next);
    if (!next) setCurrentKey(null);
    try {
      window.localStorage.setItem(STORAGE_KEY, next ? "on" : "off");
    } catch {
      // Private window or blocked storage: the choice just lasts this session.
    }
  }, []);

  const value = useMemo<GlossaryContextValue>(
    () => ({
      enabled,
      setEnabled,
      current: currentKey ? entryByKey(currentKey) ?? null : null,
      open: setCurrentKey,
    }),
    [enabled, setEnabled, currentKey],
  );

  return <GlossaryContext.Provider value={value}>{children}</GlossaryContext.Provider>;
}

export function useGlossary(): GlossaryContextValue {
  return useContext(GlossaryContext);
}

function TermButton({ entry, children }: { entry: GlossaryEntry; children: ReactNode }) {
  const { open } = useGlossary();
  return (
    <button
      type="button"
      onClick={() => open(entry.key)}
      title={entry.short}
      className="cursor-help border-b border-dotted border-hud-accent/70 text-inherit hover:text-hud-accent"
    >
      {children}
    </button>
  );
}

/**
 * Text in which game terms can be tapped for a plain-language explanation.
 * Only the first use of each term in the block is marked, so it stays readable.
 */
export function GlossaryText({ text }: { text: string }) {
  const { enabled } = useGlossary();
  if (!enabled || !text) return <>{text}</>;

  const { regex } = matcher();
  regex.lastIndex = 0;
  const seen = new Set<string>();
  const out: ReactNode[] = [];
  let cursor = 0;

  for (const match of text.matchAll(regex)) {
    const index = match.index ?? 0;
    const entry = resolveMatch(match[0]);
    if (!entry || seen.has(entry.key)) continue;
    seen.add(entry.key);
    if (index > cursor) out.push(text.slice(cursor, index));
    out.push(
      <TermButton key={`${entry.key}-${index}`} entry={entry}>
        {match[0]}
      </TermButton>,
    );
    cursor = index + match[0].length;
  }
  if (out.length === 0) return <>{text}</>;
  if (cursor < text.length) out.push(text.slice(cursor));
  return (
    <>
      {out.map((node, i) => (
        <Fragment key={i}>{node}</Fragment>
      ))}
    </>
  );
}

/** A specific term as a tappable label, e.g. the current phase name. */
export function TermLink({ term, children }: { term: string; children: ReactNode }) {
  const { enabled } = useGlossary();
  const entry = entryByKey(term);
  if (!enabled || !entry) return <>{children}</>;
  return <TermButton entry={entry}>{children}</TermButton>;
}

/** The explainer card shown when a term is tapped. Sits at the bottom of the HUD. */
export function TermSheet() {
  const { current, open } = useGlossary();
  if (!current) return null;
  return (
    <div
      role="dialog"
      aria-label={`What ${current.term} means`}
      className="shrink-0 border-t border-hud-accent/40 bg-slate-950/95 px-3 py-2"
    >
      <div className="flex items-start justify-between gap-2">
        <div className="text-[12px] font-semibold text-hud-accent">
          {current.term}
          {current.category === "slang" && (
            <span className="ml-2 rounded bg-slate-700/70 px-1 text-[10px] font-normal text-slate-300">
              player slang
            </span>
          )}
        </div>
        <button
          type="button"
          onClick={() => open(null)}
          aria-label="Close explanation"
          className="rounded px-1.5 text-[12px] text-slate-400 hover:bg-slate-800 hover:text-slate-100"
        >
          ✕
        </button>
      </div>
      <p className="mt-1 text-[12px] leading-snug text-slate-100">{current.short}</p>
      {current.more && (
        <p className="mt-1 text-[11px] leading-snug text-slate-300">{current.more}</p>
      )}
      {current.tip && (
        <p className="mt-1 text-[11px] leading-snug text-hud-success">Tip: {current.tip}</p>
      )}
    </div>
  );
}

/** Searchable reference for learning the game. */
export function GlossaryPanel() {
  const { open } = useGlossary();
  const [query, setQuery] = useState("");
  const results = useMemo(() => searchGlossary(query), [query]);

  return (
    <div className="space-y-2">
      <input
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        placeholder="Search a term, e.g. Blocker"
        aria-label="Search the glossary"
        className="w-full rounded border border-slate-700 bg-slate-950/80 px-2 py-1 text-[11px] text-slate-100 placeholder:text-slate-500 focus:border-hud-accent/50 focus:outline-none"
      />
      {results.length === 0 && (
        <p className="text-[11px] text-slate-400">No match. Try a shorter word.</p>
      )}
      {CATEGORY_ORDER.map((category) => {
        const entries = results.filter((entry) => entry.category === category);
        if (entries.length === 0) return null;
        return (
          <div key={category}>
            <div className="mb-1 text-[10px] uppercase tracking-wide text-slate-400">
              {CATEGORY_LABEL[category]}
              {category === "slang" && " · community terms, not official rules"}
            </div>
            <ul className="space-y-1">
              {entries.map((entry) => (
                <li key={entry.key}>
                  <button
                    type="button"
                    onClick={() => open(entry.key)}
                    className="w-full rounded bg-slate-900/60 px-2 py-1 text-left hover:bg-slate-800/70"
                  >
                    <span className="text-[11px] font-semibold text-slate-100">{entry.term}</span>
                    <span className="block text-[11px] leading-snug text-slate-300">
                      {entry.short}
                    </span>
                  </button>
                </li>
              ))}
            </ul>
          </div>
        );
      })}
    </div>
  );
}
