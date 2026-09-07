import { BlockerWarning } from "./components/BlockerWarning";
import { useCompanionBridge } from "./hooks/useCompanionBridge";
import { CalibrationPanel } from "./components/CalibrationPanel";
import { DebugPanel } from "./components/DebugPanel";
import { CoachChatPanel } from "./components/CoachChatPanel";
import { CombatPanel } from "./components/CombatPanel";
import { ConnectionStatus } from "./components/ConnectionStatus";
import { DeckPanel } from "./components/DeckPanel";
import { MatchupPanel } from "./components/MatchupPanel";
import { ScoutingPanel } from "./components/ScoutingPanel";
import { GameStatePanel } from "./components/GameStatePanel";
import { SourceSelector } from "./components/SourceSelector";
import { SourceStatusHud } from "./components/SourceStatusHud";
import { StrategyPanel } from "./components/StrategyPanel";
import { TurnIndicator } from "./components/TurnIndicator";

const DEBUG = Boolean((import.meta as ImportMeta & { env?: { DEV?: boolean } }).env?.DEV);

export default function App() {
  const bridge = useCompanionBridge();
  const gs = bridge.snapshot?.game_state ?? null;
  const combat = gs?.combat ?? null;

  return (
    <div
      className="flex h-full min-h-0 w-full flex-col text-white"
      style={{ opacity: bridge.overlay.opacity }}
    >
      <header className="flex shrink-0 items-center justify-between px-3 pb-1 pt-2">
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

      <div className="min-h-0 flex-1 overflow-y-auto overflow-x-hidden overscroll-contain px-2 pb-2">
        <div className="flex flex-col gap-2">
          <SourceStatusHud observation={bridge.observation} debug={DEBUG} />
          {bridge.observation?.selection === "mock" && (
            <p className="px-1 text-[10px] leading-snug text-slate-500">
              Demo is running — cards move on their own. For a real match,
              quit and run ./start onesimulator.
            </p>
          )}
          {bridge.observation?.selection === "one_simulator" &&
            bridge.observation.hud_state !== "live" && (
              <p className="px-1 text-[10px] leading-snug text-slate-500">
                Waiting on OneSimulator. In Chrome: chrome://extensions →
                Load unpacked → browser-companion, then open a match.
              </p>
            )}

          <TurnIndicator
            gameState={gs}
            yourDeck={bridge.snapshot?.your_deck ?? null}
            opponentDeck={bridge.snapshot?.opponent_deck ?? null}
          />

          {bridge.error && (
            <div className="rounded border border-hud-danger/50 bg-hud-danger/10 px-2 py-1 text-[10px] text-hud-danger">
              {bridge.error}
            </div>
          )}

          {bridge.loading ? (
            <div className="hud-panel flex items-center justify-center p-4">
              <span className="animate-pulse text-xs text-slate-400">
                Connecting to companion bridge...
              </span>
            </div>
          ) : (
            <>
              <DeckPanel
                yourDeck={bridge.snapshot?.your_deck ?? null}
                opponentDeck={bridge.snapshot?.opponent_deck ?? null}
                pastedDeck={bridge.snapshot?.pasted_deck ?? null}
                collection={bridge.snapshot?.deck_collection ?? null}
                applying={bridge.refreshingStrategy}
                onSaveDeck={bridge.saveDeck}
                onSetDeckSource={bridge.setDeckSource}
                onActivateDeck={bridge.activateDeck}
                onDeleteDeck={bridge.deleteDeck}
                onRenameDeck={bridge.renameDeck}
                onClearPaste={bridge.clearPastedDeck}
              />
              <ScoutingPanel
                report={bridge.snapshot?.scouting ?? null}
                listAttached={
                  bridge.snapshot?.opponent_deck?.origin === "attached"
                }
              />
              <MatchupPanel report={bridge.snapshot?.matchup ?? null} />
              <BlockerWarning
                combat={combat}
                analysis={bridge.snapshot?.combat_analysis ?? null}
              />
              <CombatPanel
                combat={combat}
                analysis={bridge.snapshot?.combat_analysis ?? null}
              />
              <StrategyPanel
                strategy={bridge.snapshot?.strategy ?? null}
                options={bridge.snapshot?.options ?? []}
                phaseCoach={bridge.snapshot?.phase_coach ?? null}
                deckStrategy={bridge.snapshot?.deck_strategy ?? null}
                gameState={gs}
                paused={
                  bridge.observation?.analysis?.mode === "paused" ||
                  bridge.observation?.hud_state === "lost"
                }
                refreshing={bridge.refreshingStrategy}
                onRefresh={bridge.refreshDeckStrategy}
              />
              <CoachChatPanel />
              <GameStatePanel gameState={gs} />
              <SourceSelector
                observation={bridge.observation}
                onSelect={bridge.setObservationSource}
              />
              {DEBUG && <CalibrationPanel />}
              <DebugPanel enabled={DEBUG} />
              <ConnectionStatus
                connection={bridge.snapshot?.connection ?? null}
              />
            </>
          )}
        </div>
      </div>

      <footer className="shrink-0 border-t border-slate-800/80 px-2 py-1 text-center text-[9px] text-slate-500">
        Scroll for more · mock :9002 · browser :9003
      </footer>
    </div>
  );
}
