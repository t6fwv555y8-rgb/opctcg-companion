import type { ConnectionStatusDto } from "../types/game";

interface Props {
  connection: ConnectionStatusDto | null;
}

export function ConnectionStatus({ connection }: Props) {
  if (!connection) {
    return (
      <div className="hud-panel p-3">
        <div className="hud-title">System</div>
        <p className="text-xs text-slate-400">Initializing...</p>
      </div>
    );
  }

  const statusColor =
    connection.status === "connected"
      ? "text-hud-success"
      : connection.status === "error"
        ? "text-hud-danger"
        : connection.status === "connecting"
          ? "text-hud-warn"
          : "text-slate-400";

  const dotClass =
    connection.status === "connected"
      ? "pulse-dot connected"
      : connection.status === "error"
        ? "pulse-dot disconnected"
        : "pulse-dot disconnected";

  return (
    <div className="hud-panel p-3">
      <div className="flex items-center justify-between">
        <div className="hud-title">System</div>
        <span className={`text-[10px] font-semibold ${statusColor}`}>
          {connection.status === "error" ? "⚠ " : "● "}
          {connection.label}
        </span>
      </div>
      <div className="mt-2 space-y-1 text-xs">
        <div className="flex justify-between">
          <span className="text-slate-400">WebSocket</span>
          <span className="flex items-center gap-1">
            <span className={dotClass} />
            {connection.websocket_connected ? "up" : "down"}
          </span>
        </div>
        <div className="flex justify-between">
          <span className="text-slate-400">Event #</span>
          <span className="font-mono">{connection.event_sequence}</span>
        </div>
        <div className="flex justify-between">
          <span className="text-slate-400">Latency</span>
          <span
            className={`font-mono ${connection.latency_ms < 100 ? "text-hud-success" : "text-hud-warn"}`}
          >
            {connection.latency_ms}ms
          </span>
        </div>
        {connection.last_error && (
          <p className="text-[10px] text-hud-danger">{connection.last_error}</p>
        )}
      </div>
    </div>
  );
}
