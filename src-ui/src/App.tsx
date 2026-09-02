import { useCompanionBridge } from "./hooks/useCompanionBridge";
import { BlockerWarning } from "./components/BlockerWarning";
import { CombatPanel } from "./components/CombatPanel";
import { ConnectionStatus } from "./components/ConnectionStatus";
import { GameStatePanel } from "./components/GameStatePanel";
import { StrategyPanel } from "./components/StrategyPanel";
import { TurnIndicator } from "./components/TurnIndicator";

export default function App() {
  const bridge = useCompanionBridge();
  const gs = bridge.snapshot?.game_state ?? null;
  const combat = gs?.combat ?? null;

  return (
    <div
      className="flex h-full w-full flex-col gap-2 p-2 text-white"
      style={{ opacity: bridge.overlay.opacity }}
    >
      <header className="flex items-center justify-between px-1">
        <h1 className="text-sm font-bold tracking-tight text-hud-accent">
          OPTCG Companion
        </h1>
        <div className="flex gap-1">
          <button
            onClick={() => bridge.toggleOverlay()}
            className={`rounded px-2 py-0.5 text-[10px] ${
              bridge.overlay.click_through
                ? "bg-hud-accent/30 text-hud-accent"
                : "bg-slate-700/50 text-slate-300"
            }`}
          >
            {bridge.overlay.click_through ? "Click-Through" : "Interactive"}
          </button>
        </div>
      </header>

      <TurnIndicator gameState={gs} />

      {bridge.error && (
        <div className="rounded border border-hud-danger/50 bg-hud-danger/10 px-2 py-1 text-[10px] text-hud-danger">
          {bridge.error}
        </div>
      )}

      {bridge.loading ? (
        <div className="hud-panel flex flex-1 items-center justify-center p-4">
          <span className="animate-pulse text-xs text-slate-400">
            Connecting to companion bridge...
          </span>
        </div>
      ) : (
        <>
          {/* LEVEL 1 — CRITICAL */}
          <BlockerWarning
            combat={combat}
            analysis={bridge.snapshot?.combat_analysis ?? null}
          />
          <CombatPanel
            combat={combat}
            analysis={bridge.snapshot?.combat_analysis ?? null}
          />

          {/* LEVEL 2 — STRATEGY */}
          <StrategyPanel strategy={bridge.snapshot?.strategy ?? null} />

          {/* LEVEL 3 — STATE */}
          <GameStatePanel gameState={gs} />

          {/* LEVEL 4 — SYSTEM */}
          <ConnectionStatus connection={bridge.snapshot?.connection ?? null} />
        </>
      )}

      <footer className="mt-auto px-1 text-center text-[9px] text-slate-500">
        Event-driven · ws://127.0.0.1:9002
      </footer>
    </div>
  );
}
