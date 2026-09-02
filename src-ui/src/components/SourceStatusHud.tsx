import type { ObservationStatusDto } from "../types/game";

interface Props {
  observation: ObservationStatusDto | null;
  debug?: boolean;
}

function formatSourceLabel(observation: ObservationStatusDto): string {
  const src = observation.active_source ?? "NO SUPPORTED GAME DETECTED";
  const live = observation.adapters.some((a) => a.live);
  if (!live && observation.searching) {
    return "INITIALIZING STATE...";
  }
  if (!live && !observation.active_source) {
    return "NO SUPPORTED GAME DETECTED";
  }
  return live ? `${src} · LIVE` : src;
}

export function SourceStatusHud({ observation, debug = false }: Props) {
  if (!observation) {
    return (
      <div className="rounded border border-slate-700/50 bg-slate-900/40 px-2 py-1 text-[10px] text-slate-400">
        GAME FOUND · SEARCHING...
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
      {!debug && observation.sync_state !== "synced" && (
        <div className="mt-0.5 text-[9px] text-amber-400/80">
          STATE · {observation.sync_state.toUpperCase()}
        </div>
      )}
      {debug && (
        <div className="mt-1 space-y-0.5 font-mono text-[9px] text-slate-500">
          <div>sync {observation.sync_state}</div>
          <div>obs {observation.latency.observation_latency_ms}ms</div>
          <div>analysis {observation.latency.analysis_latency_ms}ms</div>
          <div>total {observation.latency.total_latency_ms}ms</div>
        </div>
      )}
    </div>
  );
}
