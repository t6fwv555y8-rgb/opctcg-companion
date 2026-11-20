import type { ReactNode } from "react";
import { GlossaryText } from "./Glossary";
import type {
  DeckStrategyBrief,
  GameStateDto,
  StrategyRecommendation,
} from "../types/game";

interface Props {
  strategy: StrategyRecommendation | null;
  options?: StrategyRecommendation[];
  phaseCoach?: string | null;
  deckStrategy?: DeckStrategyBrief | null;
  gameState?: GameStateDto | null;
  paused?: boolean;
  refreshing?: boolean;
  onRefresh?: () => void;
}

/** Display-only cleanup: drop "[ST01-001]" style ids and a trailing "(X vs Y)". */
function tidyCoachLine(text: string): string {
  return text
    .replace(/\[[A-Za-z]{1,4}\d{1,3}-\d{2,3}\]\s*/g, "")
    .replace(/\s*\([^()]*\svs\s[^()]*\)\s*$/i, "")
    .trim();
}

function confidenceLabel(c: number): string {
  return c >= 0.75 ? "High" : c >= 0.45 ? "Medium" : "Low";
}

function Section({
  title,
  tone = "text-slate-400",
  children,
}: {
  title: string;
  tone?: string;
  children: ReactNode;
}) {
  return (
    <div>
      <div className={`mb-0.5 text-[10px] uppercase tracking-wide ${tone}`}>{title}</div>
      {children}
    </div>
  );
}

export function StrategyPanel({
  strategy,
  options = [],
  phaseCoach,
  deckStrategy = null,
  paused = false,
  refreshing = false,
  onRefresh,
}: Props) {
  const coachLine = phaseCoach ? tidyCoachLine(phaseCoach) : "";
  const hasPlan = Boolean(deckStrategy);

  return (
    <div className="hud-panel space-y-2 p-3">
      <div className="flex items-center justify-between gap-2">
        <div className="hud-title">Suggested play</div>
        {strategy && (
          <span className="text-[10px] text-slate-400">
            Confidence{" "}
            <span className="font-semibold text-slate-100">
              {confidenceLabel(strategy.confidence)}
            </span>
          </span>
        )}
        {!hasPlan && onRefresh && (
          <button
            type="button"
            onClick={onRefresh}
            disabled={refreshing}
            className="rounded border border-slate-600/60 px-2 py-0.5 text-[10px] text-slate-300 hover:bg-slate-800/60 disabled:opacity-50"
            title="Build a detailed strategy for these decks"
          >
            {refreshing ? "Building…" : "Build plan"}
          </button>
        )}
      </div>

      {paused && (
        <p className="text-[10px] text-hud-warn">
          Not sure what’s on the board — treat this as a guess.
        </p>
      )}

      {coachLine || strategy ? (
        <div className="space-y-1">
          {coachLine && (
            <p className="text-[13px] font-semibold leading-snug text-white">
              <GlossaryText text={coachLine} />
            </p>
          )}
          {strategy && (
            <p
              className={`leading-snug ${
                coachLine ? "text-[11px] text-slate-300" : "text-[13px] font-semibold text-white"
              }`}
            >
              {coachLine ? "Best line: " : ""}
              <GlossaryText text={tidyCoachLine(strategy.action.description)} />
            </p>
          )}
          {strategy?.reasoning && (
            <details>
              <summary className="cursor-pointer select-none text-[10px] text-slate-400">
                Why
              </summary>
              <p className="mt-1 text-[11px] leading-snug text-slate-300">
                <GlossaryText text={strategy.reasoning} />
              </p>
            </details>
          )}
        </div>
      ) : (
        <p className="text-[11px] text-slate-400">Waiting for a game to read…</p>
      )}

      {options.length > 1 && (
        <details>
          <summary className="cursor-pointer select-none text-[10px] text-slate-400">
            Other options ({options.length - 1})
          </summary>
          <ol className="mt-1 max-h-40 space-y-1 overflow-y-auto">
            {options.slice(1).map((opt, i) => (
              <li
                key={`${opt.action.description}-${i}`}
                className="flex items-start justify-between gap-2 rounded bg-slate-800/60 px-2 py-1 text-[11px] text-slate-200"
              >
                <span>{tidyCoachLine(opt.action.description)}</span>
                <span className="shrink-0 font-mono text-slate-400">
                  {opt.score.toFixed(1)}
                </span>
              </li>
            ))}
          </ol>
        </details>
      )}

      {hasPlan && deckStrategy && (
        <details className="border-t border-slate-700/60 pt-2">
          <summary className="flex cursor-pointer select-none items-center justify-between gap-2 text-[11px] font-semibold text-slate-300">
            <span>Game plan</span>
            <span className="min-w-0 truncate text-[10px] font-normal text-slate-400">
              {deckStrategy.matchup}
            </span>
          </summary>
          <div className="mt-2 space-y-2">
            <Section title="Plan">
              <p className="text-[11px] leading-snug text-slate-200"><GlossaryText text={deckStrategy.your_plan} /></p>
            </Section>
            <Section title="Vs opponent">
              <p className="text-[11px] leading-snug text-slate-300"><GlossaryText text={deckStrategy.vs_opponent} /></p>
            </Section>
            {deckStrategy.this_turn.length > 0 && (
              <Section title="This turn" tone="text-hud-accent">
                <ol className="space-y-0.5">
                  {deckStrategy.this_turn.map((step, i) => (
                    <li key={i} className="text-[11px] leading-snug text-slate-300">
                      <span className="mr-1 text-slate-500">{i + 1}.</span>
                      <GlossaryText text={tidyCoachLine(step)} />
                    </li>
                  ))}
                </ol>
              </Section>
            )}
            {deckStrategy.priorities.length > 0 && (
              <Section title="Priorities">
                <ul className="space-y-0.5">
                  {deckStrategy.priorities.slice(0, 4).map((p, i) => (
                    <li key={i} className="text-[11px] leading-snug text-slate-300">
                      · <GlossaryText text={p} />
                    </li>
                  ))}
                </ul>
              </Section>
            )}
            {deckStrategy.threats.length > 0 && (
              <Section title="Threats" tone="text-hud-warn">
                <ul className="max-h-20 space-y-0.5 overflow-y-auto">
                  {deckStrategy.threats.slice(0, 5).map((t, i) => (
                    <li key={i} className="text-[11px] leading-snug text-slate-300">
                      · <GlossaryText text={t} />
                    </li>
                  ))}
                </ul>
              </Section>
            )}
            {deckStrategy.list_notes && deckStrategy.list_notes.length > 0 && (
              <Section title="From your list">
                <ul className="max-h-28 space-y-0.5 overflow-y-auto">
                  {deckStrategy.list_notes.slice(0, 8).map((n, i) => (
                    <li key={i} className="text-[11px] leading-snug text-slate-300">
                      · <GlossaryText text={n} />
                    </li>
                  ))}
                </ul>
              </Section>
            )}
            {onRefresh && (
              <button
                type="button"
                onClick={onRefresh}
                disabled={refreshing}
                className="rounded border border-slate-600/60 px-2 py-0.5 text-[10px] text-slate-300 hover:bg-slate-800/60 disabled:opacity-50"
                title="Rebuild detailed strategy for these decks"
              >
                {refreshing ? "Refreshing\u2026" : "Refresh plan"}
              </button>
            )}
          </div>
        </details>
      )}
    </div>
  );
}
