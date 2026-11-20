import type { ObservationStatusDto, SourceSelectionKind } from "../types/game";

interface Props {
  observation: ObservationStatusDto | null;
  onSelect: (selection: SourceSelectionKind) => void;
}

const OPTIONS: { value: SourceSelectionKind; label: string }[] = [
  { value: "auto", label: "Auto Detect" },
  { value: "one_simulator", label: "OneSimulator" },
  { value: "optcgsim", label: "OPTCGSim" },
  { value: "mock", label: "Mock" },
  { value: "replay", label: "Replay" },
];

const DEV = Boolean((import.meta as ImportMeta & { env?: { DEV?: boolean } }).env?.DEV);

// Mock and Replay are test feeds; regular players only need real games.
const TEST_SOURCES: SourceSelectionKind[] = ["mock", "replay"];

export function SourceSelector({ observation, onSelect }: Props) {
  const current = observation?.selection ?? "auto";
  const options = OPTIONS.filter((o) => DEV || !TEST_SOURCES.includes(o.value));

  return (
    <div className="hud-panel p-3">
      <div className="hud-title">Game Source</div>
      <div className="mt-2 grid grid-cols-2 gap-1">
        {options.map((opt) => (
          <button
            key={opt.value}
            onClick={() => onSelect(opt.value)}
            className={`rounded px-2 py-2 text-sm transition ${
              current === opt.value
                ? "bg-hud-accent/30 text-hud-accent ring-1 ring-hud-accent/50"
                : "bg-slate-800/60 text-slate-300 hover:bg-slate-700/60"
            }`}
          >
            {opt.label}
          </button>
        ))}
      </div>
    </div>
  );
}
