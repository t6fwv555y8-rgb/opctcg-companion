import { invoke } from "@tauri-apps/api/core";
import { useState } from "react";

export function CalibrationPanel() {
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const loadDefaults = async () => {
    setError(null);
    try {
      await invoke("get_calibration_profile");
      setSaved(false);
    } catch (e) {
      setError(String(e));
    }
  };

  const save = async () => {
    setError(null);
    try {
      const profile = await invoke<{ id: string; regions: { calibrated: boolean } }>(
        "get_calibration_profile",
      );
      const updated = {
        ...profile,
        id: "optcgsim-user-custom",
        regions: { ...profile.regions, calibrated: true },
      };
      await invoke("save_calibration_profile", { profile: updated });
      setSaved(true);
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <div className="hud-panel p-2 text-[10px]">
      <div className="hud-title">OPTCGSim Calibration</div>
      <p className="mt-1 text-slate-400">
        Load defaults or save custom normalized regions after adjusting in a future
        frame overlay. Non-drag numeric editing can use profile JSON on disk.
      </p>
      <div className="mt-2 flex gap-1">
        <button
          onClick={() => void loadDefaults()}
          className="rounded bg-slate-800 px-2 py-1 text-slate-300 hover:bg-slate-700"
        >
          Reset Defaults
        </button>
        <button
          onClick={() => void save()}
          className="rounded bg-hud-accent/20 px-2 py-1 text-hud-accent hover:bg-hud-accent/30"
        >
          Save Calibration
        </button>
      </div>
      {saved && <div className="mt-1 text-green-400/80">Calibration saved locally.</div>}
      {error && <div className="mt-1 text-hud-danger">{error}</div>}
    </div>
  );
}
