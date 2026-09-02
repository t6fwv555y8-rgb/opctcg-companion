import type { ConnectionStatus } from "../types/companion";

interface Props {
  status: ConnectionStatus | null;
}

export function ConnectivityBar({ status }: Props) {
  const wsOk = status?.websocket_connected ?? false;
  const fileOk = status?.file_monitor_active ?? false;
  const latency = status?.latency_ms ?? 0;
  const events = status?.events_processed ?? 0;

  return (
    <div className="hud-panel p-3">
      <div className="hud-title mb-2">System Connectivity</div>
      <div className="space-y-2">
        <div className="flex items-center justify-between">
          <span className="text-xs text-slate-300">WebSocket</span>
          <span className="flex items-center gap-2 text-xs">
            <span
              className={`pulse-dot ${wsOk ? "connected" : "disconnected"}`}
            />
            <span className={wsOk ? "text-hud-success" : "text-hud-danger"}>
              {wsOk ? "Connected" : "Disconnected"}
            </span>
          </span>
        </div>
        <div className="flex items-center justify-between">
          <span className="text-xs text-slate-300">File Monitor</span>
          <span className="flex items-center gap-2 text-xs">
            <span
              className={`pulse-dot ${fileOk ? "connected" : "disconnected"}`}
            />
            <span className={fileOk ? "text-hud-success" : "text-slate-400"}>
              {fileOk ? "Active" : "Idle"}
            </span>
          </span>
        </div>
        <div className="flex items-center justify-between">
          <span className="text-xs text-slate-300">Latency</span>
          <span
            className={`stat-value ${latency < 100 ? "text-hud-success" : "text-hud-warn"}`}
          >
            {latency}ms
          </span>
        </div>
        <div className="flex items-center justify-between">
          <span className="text-xs text-slate-300">Events</span>
          <span className="stat-value">{events}</span>
        </div>
        <div className="mt-1 h-1.5 w-full overflow-hidden rounded-full bg-slate-700">
          <div
            className={`h-full transition-all duration-300 ${wsOk ? "bg-hud-success" : "bg-slate-500"}`}
            style={{ width: wsOk ? "100%" : "20%" }}
          />
        </div>
      </div>
    </div>
  );
}
