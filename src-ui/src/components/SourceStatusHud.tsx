import { TermLink } from "./Glossary";
import type { DeckInfoDto, GameStateDto, ObservationStatusDto } from "../types/game";

interface Props {
  observation: ObservationStatusDto | null;
  debug?: boolean;
  gameState?: GameStateDto | null;
  yourDeck?: DeckInfoDto | null;
  opponentDeck?: DeckInfoDto | null;
}

function formatSourceLabel(observation: ObservationStatusDto): string {
  const src = observation.active_source ?? "NO SUPPORTED GAME DETECTED";
  const hud = observation.hud_state?.toUpperCase() ?? "SEARCHING";
  if (hud === "LOST") return `${src} · LOST`;
  if (hud === "SYNCING") return `${src} · SYNCING`;
  if (hud === "PARTIAL") return `${src} · PARTIAL`;
  if (hud === "LIVE") return `${src} · LIVE`;
  if (hud === "CONNECTING") return "INITIALIZING STATE...";
  return `${src} · ${hud}`;
}

/** Plain-language status for the default (non-debug) view. */
function friendlyStatus(observation: ObservationStatusDto): string {
  switch (observation.hud_state?.toUpperCase()) {
    case "LIVE":
      return "Connected — reading your game";
    case "SYNCING":
    case "CONNECTING":
      return "Connected — catching up…";
    case "PARTIAL":
      return "Connected — can only see part of the board";
    case "LOST":
      return "Lost the game — reopen or refocus it";
    default:
      return "Looking for a game…";
  }
}

const PHASE_TERM: Record<string, string> = {
  Refresh: "refresh-phase",
  Draw: "draw-phase",
  Don: "don-phase",
  Main: "main-phase",
  End: "end-phase",
  Combat: "attack",
};

/** The backend says "Don"; players know it as DON!!. */
const PHASE_LABEL: Record<string, string> = { Don: "DON!! phase" };

export function SourceStatusHud({
  observation,
  debug = false,
  gameState = null,
  yourDeck = null,
  opponentDeck = null,
}: Props) {
  const shell = "rounded-lg border border-slate-600/70 bg-slate-900/90 px-3 py-2";

  if (!observation) {
    return (
      <div className={`${shell} text-[11px] uppercase text-slate-200`}>
        Searching for game…
      </div>
    );
  }

  const live = observation.adapters.some((a) => a.live);
  const dotClass = live ? "pulse-dot connected" : "pulse-dot disconnected";
  const label = formatSourceLabel(observation);
  const hud = observation.hud_state?.toUpperCase();
  // Only explain the state when something is off; "Live" speaks for itself.
  const note =
    observation.analysis?.hud_label ??
    (hud === "LIVE" ? null : friendlyStatus(observation));
  const matchup =
    yourDeck || opponentDeck
      ? `${yourDeck?.name ?? "You"} vs ${opponentDeck?.name ?? "Opp"}`
      : null;

  return (
    <div className={shell}>
      <div className="flex items-center gap-2 text-[11px]">
        <span className={dotClass} />
        <span className="font-semibold uppercase tracking-wide text-slate-100">
          {label}
        </span>
        {note && (
          <span className="min-w-0 truncate text-amber-300" title={note}>
            {note}
          </span>
        )}
      </div>
      {gameState && (
        <div className="mt-1 flex items-baseline justify-between gap-2 text-[12px]">
          <span
            className={`font-semibold ${
              gameState.active_player === 0 ? "text-hud-accent" : "text-slate-200"
            }`}
          >
            {gameState.active_player === 0 ? "Your turn" : "Opponent\u2019s turn"} ·{" "}
            <TermLink term={PHASE_TERM[gameState.phase] ?? ""}>
              {PHASE_LABEL[gameState.phase] ?? gameState.phase}
            </TermLink>
          </span>
          {matchup && (
            <span className="min-w-0 truncate text-[10px] text-slate-400">{matchup}</span>
          )}
        </div>
      )}
      {debug && (
        <div className="mt-1 flex flex-wrap gap-x-3 font-mono text-[10px] text-slate-400">
          <span>hud {observation.hud_state}</span>
          <span>sync {observation.sync_state}</span>
          <span>analysis {observation.analysis?.mode}</span>
          <span>obs {observation.latency.observation_latency_ms}ms</span>
          <span>total {observation.latency.total_latency_ms}ms</span>
        </div>
      )}
    </div>
  );
}
