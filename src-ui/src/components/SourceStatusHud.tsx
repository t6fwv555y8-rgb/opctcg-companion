import type { ObservationStatusDto } from "../types/game";

interface Props {
  observation: ObservationStatusDto | null;
  debug?: boolean;
}

export function SourceStatusHud({ observation, debug = false }: Props) {
  if (!observation) {
    return (
      <div className="rounded border border-slate-700/50 bg-slate-900/40 px-2 py-1 text-[10px] text-slate-400">
        SOURCE · SEARCHING FOR GAME...
      </div>
    );
  }

  const sourceLabel = observation.active_source ?? "Searching...";
  const live = observation.adapters.some((a) => a.live);
  const dotClass = live ? "pulse-dot connected" : "pulse-dot disconnected";

  return (
    <div className="rounded border border-slate-700/50 bg-slate-900/40 px-2 py-1">
      <div className="flex items-center justify-between text-[10px]">
        <span className="text-slate-400">SOURCE</span>
        <span className="flex items-center gap-1 font-semibold text-hud-accent">
          <span className={dotClass} />
          {sourceLabel}
          {live ? " · LIVE" : ""}
        </span>
      </div>
      {debug && (
        <div className="mt-1 space-y-0.5 font-mono text-[9px] text-slate-500">
          <div>obs {observation.latency.observation_latency_ms}ms</div>
          <div>analysis {observation.latency.analysis_latency_ms}ms</div>
          <div>total {observation.latency.total_latency_ms}ms</div>
        </div>
      )}
    </div>
  );
}
