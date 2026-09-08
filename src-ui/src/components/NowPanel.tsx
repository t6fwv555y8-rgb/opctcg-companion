import { useRef } from "react";
import { battleDoThis } from "../battleDoThis";
import { nextCoachPin, playByPlayLine, type CoachPin } from "../playByPlay";
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
  gameId?: string;
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

/// Rayleigh's broadcast. Updates as the board does.
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
  gameId,
}: Props) {
  const waiting =
    pageState === "queue"
      ? "In queue — the next call lands when the match starts."
      : pageState === "lobby"
        ? "In the lobby — queue a match and Rayleigh will call it."
        : pageState === "ended"
          ? "Final. The recap lands as soon as the result is readable."
          : "Waiting for a readable position.";
  const table = combatCoach ?? battleDoThis(combat, analysis);
  const fighting = Boolean(combat?.active || analysis);
  const line =
    table?.line?.trim() ||
    (!fighting && strategy?.action.description?.trim()) ||
    phaseCoach?.trim() ||
    waiting;
  // Keep Rayleigh's last read. Falling back the instant a turn ends was
  // snapping Play-by-play to generic phase copy.
  const pin = useRef<CoachPin | null>(null);
  const game = gameId || pageState || "";
  const incoming = coachLine?.trim() ?? "";
  pin.current = nextCoachPin(pin.current, game, incoming);
  const broadcast = playByPlayLine(pin.current.text, line);
  const steps = (
    table?.steps?.length ? table.steps : (deckStrategy?.this_turn ?? [])
  ).slice(0, 3);
  const alts = options
    .filter((opt) => opt.action.description?.trim() && opt.action.description.trim() !== line)
    .slice(0, 3);
  const blockerOpen = Boolean(combat?.blocker_offered);
  const cover = deckStrategy?.vs_opponent?.trim() || "";

  return (
    <div className="flex flex-col gap-3">
      <section className="broadcast">
        <div className="flex items-center justify-between gap-2">
          <div className="broadcast-tag">Rayleigh · Play-by-play</div>
          {blockerOpen && (
            <span className="rounded-full bg-amber-500/15 px-2 py-0.5 text-[11px] text-amber-100">
              Blocker window
            </span>
          )}
        </div>
        {analysis && <CombatStrip analysis={analysis} />}
        {coachBusy && !incoming && (
          <p className="mt-2 animate-pulse text-[13px] text-amber-200/60">
            Reading the new position…
          </p>
        )}
        {coachError && (
          <p className="mt-2 text-[13px] text-hud-danger">{coachError}</p>
        )}
        <p className="broadcast-copy">
          {broadcast}
          {coachBusy && incoming && (
            <span className="ml-1 animate-pulse text-amber-300">▌</span>
          )}
        </p>
        {paused && (
          <p className="mt-2 text-[12px] text-hud-warn">
            The read is shaky — treat this as provisional.
          </p>
        )}
      </section>

      <section className="hud-panel px-3.5 py-3">
        <div className="best-line">
          <div className="text-[10px] font-semibold uppercase tracking-[0.16em] text-amber-200">
            The call
          </div>
          <p className="mt-1 text-[16px] font-semibold leading-snug text-amber-50">{line}</p>
        </div>
        {steps.length > 0 && (
          <ul className="mt-3 space-y-2">
            {steps.map((step) => (
              <li
                key={step}
                className="border-l border-amber-400/35 pl-3 text-[13px] leading-snug text-slate-300"
              >
                {step}
              </li>
            ))}
          </ul>
        )}
        {alts.length > 0 && (
          <ul className="mt-3 space-y-1.5 border-t border-white/5 pt-3 text-[13px]">
            {alts.map((opt, i) => (
              <li
                key={opt.action.description}
                className={i === 0 ? "alt-line-1" : "alt-line-2"}
              >
                <span className="mr-1.5 text-[10px] font-semibold uppercase tracking-[0.12em] text-slate-500">
                  {i === 0 ? "Next" : "Hold"}
                </span>
                {opt.action.description}
              </li>
            ))}
          </ul>
        )}
      </section>

      {cover && (
        <section className="cover-them">
          <div className="text-[10px] font-semibold uppercase tracking-[0.16em] text-rose-200/80">
            Cover them
          </div>
          <p className="mt-1.5 text-[13px] leading-relaxed text-rose-50/90">{cover}</p>
        </section>
      )}

    </div>
  );
}
