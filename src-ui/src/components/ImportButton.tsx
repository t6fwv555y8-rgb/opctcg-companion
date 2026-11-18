import { invoke } from "@tauri-apps/api/core";
import { useState } from "react";

interface Summary {
  found: number;
  imported: number;
  already_saved: number;
  skipped: number;
  message: string;
}

/** Pulls decks or games in from the OPTCGSim folder on this computer. */
export function ImportButton({
  command,
  label,
  onDone,
}: {
  command: "import_optcgsim_decks" | "import_optcgsim_games";
  label: string;
  onDone?: () => void;
}) {
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);

  const run = async () => {
    setBusy(true);
    setMessage(null);
    try {
      const summary = await invoke<Summary>(command);
      setMessage(summary.message);
      onDone?.();
    } catch (e) {
      setMessage(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="space-y-1">
      <button
        type="button"
        onClick={() => void run()}
        disabled={busy}
        className="rounded border border-slate-600/60 px-2 py-0.5 text-[10px] text-slate-200 hover:bg-slate-800/60 disabled:opacity-50"
      >
        {busy ? "Importing…" : label}
      </button>
      {message && <p className="text-[10px] leading-snug text-slate-300">{message}</p>}
    </div>
  );
}
