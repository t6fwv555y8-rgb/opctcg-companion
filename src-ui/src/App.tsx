import { useCompanionBridge } from "./hooks/useCompanionBridge";
import { BlockerWarning } from "./components/BlockerWarning";
import { CombatMathPanel } from "./components/CombatMathPanel";
import { ConnectivityBar } from "./components/ConnectivityBar";
import { GameStateHeader } from "./components/GameStateHeader";
import { StrategyPanel } from "./components/StrategyPanel";

export default function App() {
  const bridge = useCompanionBridge();

  return (
    <div className="flex h-full w-full flex-col gap-2 p-2 text-white">
      <header className="flex items-center justify-between px-1">
        <h1 className="text-sm font-bold tracking-tight text-hud-accent">
          OPTCG Companion
        </h1>
        <button
          onClick={() => bridge.setClickThrough(!bridge.clickThrough)}
          className={`rounded px-2 py-0.5 text-[10px] transition-colors ${
            bridge.clickThrough
              ? "bg-hud-accent/30 text-hud-accent"
              : "bg-slate-700/50 text-slate-300 hover:bg-slate-600/50"
          }`}
          title="Toggle mouse click-through"
        >
          {bridge.clickThrough ? "Click-Through ON" : "Click-Through OFF"}
        </button>
      </header>

      {bridge.error && (
        <div className="rounded border border-hud-danger/50 bg-hud-danger/10 px-2 py-1 text-[10px] text-hud-danger">
          {bridge.error}
        </div>
      )}

      {bridge.loading ? (
        <div className="hud-panel flex flex-1 items-center justify-center p-4">
          <span className="animate-pulse text-xs text-slate-400">
            Initializing companion bridge...
          </span>
        </div>
      ) : (
        <>
          <ConnectivityBar status={bridge.connectionStatus} />
          <GameStateHeader
            gameState={bridge.gameState}
            legalActions={bridge.legalActions}
          />
          <StrategyPanel recommendations={bridge.recommendations} />
          <CombatMathPanel
            combat={bridge.gameState?.combat ?? null}
            analysis={bridge.combatAnalysis}
          />
          <BlockerWarning
            combat={bridge.gameState?.combat ?? null}
            analysis={bridge.combatAnalysis}
          />
        </>
      )}

      <footer className="mt-auto px-1 text-center text-[9px] text-slate-500">
        Poll interval: 100ms · ws://127.0.0.1:9002
      </footer>
    </div>
  );
}
