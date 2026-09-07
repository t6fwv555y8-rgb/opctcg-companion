import { battleDoThis } from "../battleDoThis";
import type {
  CombatAnalysis,
  CombatDoThis,
  CombatState,
  DeckStrategyBrief,
  StrategyRecommendation,
} from "../types/game";

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

function fmtPower(n: number): string {
  return n >= 1000 && n % 1000 === 0 ? `${n / 1000}k` : String(n);
}

function CombatStrip({ analysis }: { analysis: CombatAnalysis }) {
  const lethal =
    analysis.lethal_to_leader || analysis.survival_status === "LETHAL";
  const holds =
    analysis.survives_without_counter || analysis.survival_status === "SURVIVES";
  const tone = lethal
    ? "bg-rose-500/12 text-rose-100"
    : holds && analysis.required_counter <= 0
      ? "bg-emerald-500/10 text-slate-200"
      : "bg-amber-500/10 text-amber-50";
  const tag = lethal
    ? "Lethal"
    : analysis.survival_status === "COUNTER_REQUIRED"
      ? "Counter"
      : holds
        ? "Holds"
        : null;

  return (
    <div
      className={`mt-2 flex items-center justify-between gap-3 rounded-lg px-2.5 py-1.5 text-[13px] ${tone}`}
    >
      <p className="min-w-0 tabular-nums">
        <span className="font-semibold text-sky-300">
          {fmtPower(analysis.attacker_power)}
        </span>
        <span className="mx-1.5 text-slate-500">→</span>
        <span className="font-semibold">
          {fmtPower(analysis.defender_power)}
        </span>
        {analysis.required_counter > 0 && (
          <span className="ml-2 text-slate-400">
            need {fmtPower(analysis.required_counter)}
          </span>
        )}
      </p>
      {tag && (
        <span className="shrink-0 text-[11px] font-medium text-current/80">
          {tag}
        </span>
      )}
    </div>
  );
}

function RosterColumn({
  title,
  rows,
}: {
  title: string;
  rows: string[];
}) {
  return (
    <div>
      <div className="text-[10px] font-medium uppercase tracking-[0.16em] text-slate-500">
        {title}
      </div>
      {rows.length === 0 ? (
        <p className="mt-1.5 text-[12px] text-slate-600">Empty</p>
      ) : (
        <ul className="mt-1.5 space-y-1">
          {rows.map((row, i) => {
            const meta = /\bDON\b|\bin hand\b|\brest\b/.test(row) && !/\(ST/i.test(row);
            return (
              <li
                key={`${i}-${row}`}
                className={`text-[12px] leading-snug ${
                  meta
                    ? "text-slate-500"
                    : i === 0
                      ? "font-medium text-slate-200"
                      : "text-slate-400"
                }`}
              >
                {row}
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
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
        : pageState === "ended"
          ? "Game over — the recap lands as soon as the result is readable."
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
  ).slice(0, 3);
  const you = table?.you ?? [];
  const them = table?.them ?? [];
  const alts = table
    ? []
    : options
        .filter((opt) => opt.action.description?.trim() !== line)
        .slice(0, 3);
  const blockerOpen = Boolean(combat?.blocker_offered);
  const notes = Boolean(coachLine || coachBusy || coachError);

  return (
    <div className="flex flex-col gap-4">
      <section className="hud-panel px-3.5 py-3">
        <div className="flex items-center justify-between gap-2">
          <div className="hud-title text-hud-accent">Do this</div>
          {blockerOpen && (
            <span className="rounded-full bg-amber-500/15 px-2 py-0.5 text-[11px] text-amber-100">
              Blocker window
            </span>
          )}
        </div>
        {analysis && <CombatStrip analysis={analysis} />}
        <p className="mt-2 text-[15px] leading-snug text-white">{line}</p>
        {paused && (
          <p className="mt-2 text-[12px] text-hud-warn">
            The read is shaky — treat this as provisional.
          </p>
        )}
        {steps.length > 0 && (
          <ul className="mt-3 space-y-2">
            {steps.map((step) => (
              <li
                key={step}
                className="border-l border-sky-400/30 pl-3 text-[13px] leading-snug text-slate-300"
              >
                {step}
              </li>
            ))}
          </ul>
        )}
        {alts.length > 0 && (
          <ul className="mt-3 space-y-1 border-t border-white/5 pt-3 text-[13px] text-slate-400">
            {alts.map((opt) => (
              <li key={opt.action.description}>{opt.action.description}</li>
            ))}
          </ul>
        )}
      </section>

      {(you.length > 0 || them.length > 0) && (
        <div className="flex flex-col gap-3 px-0.5">
          <RosterColumn title="Them" rows={them} />
          <RosterColumn title="You" rows={you} />
        </div>
      )}

      {notes && (
        <section className="px-0.5">
          <div className="text-[10px] font-medium uppercase tracking-[0.16em] text-slate-500">
            As you go
          </div>
          {coachBusy && !coachLine && (
            <p className="mt-1.5 animate-pulse text-[13px] text-slate-500">
              Reading the new position…
            </p>
          )}
          {coachError && (
            <p className="mt-1.5 text-[13px] text-hud-danger">{coachError}</p>
          )}
          {coachLine && (
            <p className="mt-1.5 text-[13px] leading-relaxed text-slate-400">
              {coachLine}
              {coachBusy && (
                <span className="ml-1 animate-pulse text-hud-accent">▌</span>
              )}
            </p>
          )}
        </section>
      )}
    </div>
  );
}
