import { battleDoThis } from "../battleDoThis";
import type {
  CombatAnalysis,
  CombatDoThis,
  CombatState,
} from "../types/game";

interface Props {
  thisTurn: string[];
  combat: CombatState | null;
  analysis: CombatAnalysis | null;
  combatCoach?: CombatDoThis | null;
  paused: boolean;
  pageState?: string;
}

function fmtPower(n: number): string {
  return n >= 1000 && n % 1000 === 0 ? `${n / 1000}k` : String(n);
}

function defending(
  combat: CombatState | null,
  analysis: CombatAnalysis | null,
): boolean {
  if (combat?.target_player === 0) return true;
  if (combat?.target_player === 1) return false;
  if (combat?.attacker_player === 1) return true;
  if (combat?.attacker_player === 0) return false;
  return Boolean(
    analysis &&
      combat?.target_is_leader &&
      (analysis.lethal_to_leader ||
        analysis.recommended_block ||
        analysis.required_counter > 0),
  );
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
      className={`mb-2 flex items-center justify-between gap-3 rounded-lg px-2.5 py-1.5 text-[13px] ${tone}`}
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

function waitingLine(pageState?: string): string {
  if (pageState === "queue") return "In queue — the next swing will land here.";
  if (pageState === "lobby") return "In the lobby — queue a match.";
  if (pageState === "ended") return "Final. Recap is on this tab when it lands.";
  return "Watching the table.";
}

/// Attack and defense for the swing that is open. Nothing else.
export function NowPanel({
  thisTurn,
  combat,
  analysis,
  combatCoach,
  paused,
  pageState,
}: Props) {
  const fighting = Boolean(combat?.active || analysis);
  const table = fighting
    ? (battleDoThis(combat, analysis) ?? combatCoach)
    : null;
  const defend = fighting && defending(combat, analysis);
  const title = fighting ? (defend ? "Defend" : "Attack") : "Plan";
  const line =
    table?.line?.trim() ||
    thisTurn[0]?.trim() ||
    waitingLine(pageState);
  const steps = table?.steps?.length
    ? table.steps.slice(0, 3)
    : thisTurn.slice(line === thisTurn[0] ? 1 : 0).slice(0, 3);

  return (
    <section className="hud-panel px-3.5 py-3">
      <div className="best-line">
        <div className="text-[10px] font-semibold uppercase tracking-[0.16em] text-amber-200">
          {title}
        </div>
        {analysis && <CombatStrip analysis={analysis} />}
        <p className="mt-1 text-[16px] font-semibold leading-snug text-amber-50">
          {line}
        </p>
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
      {paused && (
        <p className="mt-2 text-[12px] text-hud-warn">
          The read is shaky — treat this as provisional.
        </p>
      )}
    </section>
  );
}
