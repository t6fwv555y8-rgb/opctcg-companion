import { battleDoThis } from "../battleDoThis";
import type {
  CombatAnalysis,
  CombatDoThis,
  CombatState,
  DeckStrategyBrief,
  StrategyRecommendation,
} from "../types/game";
import { BlockerWarning } from "./BlockerWarning";
import { CombatPanel } from "./CombatPanel";

interface Props {
  phaseCoach: string | null;
  strategy: StrategyRecommendation | null;
  options: StrategyRecommendation[];
  deckStrategy: DeckStrategyBrief | null;
  combat: CombatState | null;
  analysis: CombatAnalysis | null;
  combatCoach?: CombatDoThis | null;
  paused: boolean;
  /// Latest unprompted coach line, if one has landed.
  coachLine: string | null;
  coachBusy: boolean;
  coachError?: string | null;
  pageState?: string;
}

/// What to do this second. Updates as the board does.
export function NowPanel({
  phaseCoach,
  strategy,
  options,
  deckStrategy,
  combat,
  analysis,
  combatCoach,
  paused,
  coachLine,
  coachBusy,
  coachError,
  pageState,
}: Props) {
  const waiting =
    pageState === "queue"
      ? "In queue — the next line lands when the match starts."
      : pageState === "lobby"
        ? "In lobby — queue a match and this panel will follow."
        : "Waiting for a readable position.";
  const table = combatCoach ?? battleDoThis(combat, analysis);
  const fighting = Boolean(combat?.active || analysis);
  const line =
    table?.line?.trim() ||
    (!fighting && strategy?.action.description?.trim()) ||
    phaseCoach?.trim() ||
    waiting;
  const steps = (
    table?.steps?.length ? table.steps : (deckStrategy?.this_turn ?? [])
  ).slice(0, 6);
  const alts = table
    ? []
    : options
        .filter((opt) => opt.action.description?.trim() !== line)
        .slice(0, 3);

  return (
    <div className="flex flex-col gap-3">
      <BlockerWarning combat={combat} analysis={analysis} />
      {fighting && <CombatPanel combat={combat} analysis={analysis} />}

      <section className="hud-panel p-4">
        <div className="text-xs font-semibold uppercase tracking-wide text-hud-accent">
          Do this
        </div>
        <p className="mt-2 text-base leading-relaxed text-white">{line}</p>
        {paused && (
          <p className="mt-2 text-sm text-hud-warn">
            The read is shaky — treat this as provisional.
          </p>
        )}
        {steps.length > 0 && (
          <ol className="mt-3 list-decimal space-y-1.5 pl-5 text-sm leading-snug text-slate-200">
            {steps.map((step) => (
              <li key={step}>{step}</li>
            ))}
          </ol>
        )}
        {alts.length > 0 && (
          <ul className="mt-3 space-y-1 border-t border-slate-700/50 pt-3 text-sm text-slate-300">
            {alts.map((opt) => (
              <li key={opt.action.description}>{opt.action.description}</li>
            ))}
          </ul>
        )}
      </section>

      <section className="hud-panel p-4">
        <div className="text-xs font-semibold uppercase tracking-wide text-hud-accent">
          As you go
        </div>
        {coachBusy && !coachLine && (
          <p className="mt-2 animate-pulse text-sm text-slate-400">
            Reading the new position…
          </p>
        )}
        {coachError && (
          <p className="mt-2 text-sm text-hud-danger">{coachError}</p>
        )}
        {coachLine ? (
          <p className="mt-2 text-base leading-relaxed text-slate-100">
            {coachLine}
            {coachBusy && (
              <span className="ml-1 animate-pulse text-hud-accent">▌</span>
            )}
          </p>
        ) : (
          !coachBusy &&
          !coachError && (
            <p className="mt-2 text-sm text-slate-400">
              After each play settles, the next line lands here.
            </p>
          )
        )}
      </section>
    </div>
  );
}
