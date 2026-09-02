import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";

interface Props {
  enabled: boolean;
}

interface DebugStatus {
  observation_sequence: number;
  event_sequence: number;
  sync_status: string;
  validation: Array<{
    adapter: string;
    implementation: string;
    fixture_tests: string;
    live_validation: string;
  }>;
}

export function DebugPanel({ enabled }: Props) {
  const [debug, setDebug] = useState<DebugStatus | null>(null);

  useEffect(() => {
    if (!enabled) return;
    const load = async () => {
      try {
        const status = await invoke<DebugStatus>("get_debug_status");
        setDebug(status);
      } catch {
        setDebug(null);
      }
    };
    void load();
    const id = window.setInterval(load, 2000);
    return () => window.clearInterval(id);
  }, [enabled]);

  if (!enabled || !debug) return null;

  return (
    <div className="hud-panel mt-2 space-y-1 p-2 font-mono text-[9px] text-slate-400">
      <div className="hud-title text-[10px]">Debug</div>
      <div>OBS SEQ · {debug.observation_sequence}</div>
      <div>EVENT SEQ · {debug.event_sequence}</div>
      <div>SYNC · {debug.sync_status}</div>
      {debug.validation.map((v) => (
        <div key={v.adapter}>
          {v.adapter}: {v.implementation} / fixtures {v.fixture_tests} / live{" "}
          {v.live_validation}
        </div>
      ))}
    </div>
  );
}
