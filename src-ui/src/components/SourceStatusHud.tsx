import type { ObservationStatusDto } from "../types/game";

interface Props {
  observation: ObservationStatusDto | null;
  debug?: boolean;
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

export function SourceStatusHud({ observation, debug = false }: Props) {
  if (!observation) {
    return (
      <div className="rounded border border-slate-700/50 bg-slate-900/40 px-2 py-1 text-[10px] text-slate-400">
        SEARCHING FOR GAME...
      </div>
    );
  }

  const live = observation.adapters.some((a) => a.live);
  const dotClass = live ? "pulse-dot connected" : "pulse-dot disconnected";
  const label = formatSourceLabel(observation);

  return (
    <div className="rounded border border-slate-700/50 bg-slate-900/40 px-2 py-1">
      <div className="flex items-center justify-between text-[10px]">
        <span className="text-slate-400">SOURCE</span>
        <span className="flex items-center gap-1 font-semibold text-hud-accent">
          <span className={dotClass} />
          {label}
        </span>
      </div>
      {observation.analysis?.hud_label && !debug && (
        <div className="mt-0.5 text-[9px] text-amber-400/80">
          {observation.analysis.hud_label}
        </div>
      )}
      {!debug && observation.sync_state !== "synced" && !observation.analysis?.hud_label && (
        <div className="mt-0.5 text-[9px] text-amber-400/80">
          STATE · {observation.sync_state.toUpperCase()}
        </div>
      )}
      {debug && (
        <div className="mt-1 space-y-0.5 font-mono text-[9px] text-slate-500">
          <div>hud {observation.hud_state}</div>
          <div>sync {observation.sync_state}</div>
          <div>analysis {observation.analysis?.mode}</div>
          <div>obs {observation.latency.observation_latency_ms}ms</div>
          <div>total {observation.latency.total_latency_ms}ms</div>
        </div>
      )}
    </div>
  );
}
