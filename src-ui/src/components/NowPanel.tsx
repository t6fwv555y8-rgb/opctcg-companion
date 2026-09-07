import type {
  CombatAnalysis,
  CombatState,
  DeckStrategyBrief,
  StrategyRecommendation,
} from "../types/game";
import { BlockerWarning } from "./BlockerWarning";
import { CombatPanel } from "./CombatPanel";

interface Props {
  phaseCoach: string | null;
  strategy: StrategyRecommendation | null;
  deckStrategy: DeckStrategyBrief | null;
  combat: CombatState | null;
  analysis: CombatAnalysis | null;
  paused: boolean;
}

/// What to do this second. Nothing else.
export function NowPanel({
  phaseCoach,
  strategy,
  deckStrategy,
  combat,
  analysis,
  paused,
}: Props) {
  const line =
    phaseCoach?.trim() ||
    strategy?.action.description ||
    "Waiting for a readable position.";
  const steps = deckStrategy?.this_turn.slice(0, 3) ?? [];
  const fighting = Boolean(combat?.active || analysis);

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
      </section>
    </div>
  );
}
