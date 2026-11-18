import { DeckBuilderPanel } from "./components/DeckBuilderPanel";
import { MatchHistoryPanel, PendingResultBanner } from "./components/MatchHistoryPanel";
import { useMatchHistory } from "./hooks/useMatchHistory";
import { MetaComparePanel } from "./components/MetaComparePanel";
import { GlossaryPanel, TermSheet, useGlossary } from "./components/Glossary";
import { BlockerWarning } from "./components/BlockerWarning";
import { useCompanionBridge } from "./hooks/useCompanionBridge";
import { CalibrationPanel } from "./components/CalibrationPanel";
import { DebugPanel } from "./components/DebugPanel";
import { CoachChatPanel } from "./components/CoachChatPanel";
import { CombatPanel } from "./components/CombatPanel";
import { ConnectionStatus } from "./components/ConnectionStatus";
import { DeckPanel } from "./components/DeckPanel";
import { GameStatePanel } from "./components/GameStatePanel";
import { SourceSelector } from "./components/SourceSelector";
import { SourceStatusHud } from "./components/SourceStatusHud";
import { StrategyPanel } from "./components/StrategyPanel";
import { useDebugFlag } from "./hooks/useDebugFlag";

export default function App() {
  const bridge = useCompanionBridge();
  const glossary = useGlossary();
  const matches = useMatchHistory();
  const [debug, setDebug] = useDebugFlag();
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
            title="Choose whether clicks land on this window or on the game underneath"
            className={`rounded px-2 py-0.5 text-[10px] ${
              bridge.overlay.click_through
                ? "bg-hud-accent/30 text-hud-accent"
                : "bg-slate-700/50 text-slate-300"
            }`}
          >
            {bridge.overlay.click_through ? "Clicks pass through" : "Clickable"}
          </button>
        </div>
      </header>

      <div className="min-h-0 flex-1 overflow-y-auto overflow-x-hidden overscroll-contain px-2 pb-2">
        <div className="flex flex-col gap-2 pb-2">
          <SourceStatusHud
            observation={bridge.observation}
            debug={debug}
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
              <PendingResultBanner history={matches.history} onSetResult={matches.setResult} />
              {/* Order: what is urgent (blocker), what to do now, then optional depth. */}
              <BlockerWarning
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
              <DeckPanel
                yourDeck={bridge.snapshot?.your_deck ?? null}
                opponentDeck={bridge.snapshot?.opponent_deck ?? null}
                pastedDeck={bridge.snapshot?.pasted_deck ?? null}
                collection={bridge.snapshot?.deck_collection ?? null}
                applying={bridge.refreshingStrategy}
                onSaveDeck={bridge.saveDeck}
                onActivateDeck={bridge.activateDeck}
                onDeleteDeck={bridge.deleteDeck}
                onRenameDeck={bridge.renameDeck}
                onClearPaste={bridge.clearPastedDeck}
                onImported={bridge.refresh}
              />
              <details className="hud-panel group">
                <summary className="flex cursor-pointer select-none items-center justify-between gap-2 px-3 py-2 text-[11px] font-semibold text-slate-200">
                  <span>My games</span>
                  {matches.history && matches.history.stats.games > 0 && (
                    <span className="text-[10px] font-normal text-slate-400">
                      {matches.history.stats.games} saved
                    </span>
                  )}
                </summary>
                <div className="p-3 pt-0">
                  <MatchHistoryPanel
                    history={matches.history}
                    error={matches.error}
                    onSetResult={matches.setResult}
                    onDelete={matches.remove}
                    onImported={matches.refresh}
                  />
                </div>
              </details>
              <details className="hud-panel group">
                <summary className="cursor-pointer select-none px-3 py-2 text-[11px] font-semibold text-slate-200">
                  Build a deck from winners
                </summary>
                <div className="p-3 pt-0">
                  <DeckBuilderPanel
                    yourLeaderId={bridge.snapshot?.your_deck?.leader_id ?? ""}
                    onSave={bridge.saveDeck}
                  />
                </div>
              </details>
              <details className="hud-panel group">
                <summary className="cursor-pointer select-none px-3 py-2 text-[11px] font-semibold text-slate-200">
                  Compare to winning decks
                </summary>
                <div className="p-3 pt-0">
                  <MetaComparePanel
                    deckKey={`${bridge.snapshot?.your_deck?.name ?? ""}|${bridge.snapshot?.your_deck?.leader_id ?? ""}`}
                  />
                </div>
              </details>
              <details className="hud-panel group" open={Boolean(combat?.active)}>
                <summary className="cursor-pointer select-none px-3 py-2 text-[11px] font-semibold text-slate-200">
                  Board &amp; combat details
                </summary>
                <div className="flex flex-col gap-2 p-3 pt-0">
                  <CombatPanel combat={combat} analysis={bridge.snapshot?.combat_analysis ?? null} />
                  <GameStatePanel gameState={gs} />
                </div>
              </details>
              <details className="hud-panel group">
                <summary className="cursor-pointer select-none px-3 py-2 text-[11px] font-semibold text-slate-200">
                  Learn the game
                </summary>
                <div className="p-3 pt-0">
                  <GlossaryPanel />
                </div>
              </details>
              <details className="hud-panel group">
                <summary className="cursor-pointer select-none px-3 py-2 text-[11px] font-semibold text-slate-200">
                  Settings &amp; connection
                </summary>
                <div className="flex flex-col gap-2 p-3 pt-0">
                  <SourceSelector
                    observation={bridge.observation}
                    onSelect={bridge.setObservationSource}
                  />
                  <ConnectionStatus connection={bridge.snapshot?.connection ?? null} />
                  <label className="flex cursor-pointer items-center gap-2 text-[11px] text-slate-200">
                    <input
                      type="checkbox"
                      checked={glossary.enabled}
                      onChange={(e) => glossary.setEnabled(e.target.checked)}
                    />
                    Explain game terms (tap underlined words)
                  </label>
                  <label className="flex cursor-pointer items-center gap-2 text-[10px] text-slate-300">
                    <input
                      type="checkbox"
                      checked={debug}
                      onChange={(e) => setDebug(e.target.checked)}
                    />
                    Debug
                  </label>
                  {debug && <CalibrationPanel />}
                  <DebugPanel enabled={debug} />
                </div>
              </details>
            </>
          )}
        </div>
      </div>
      <TermSheet />
    </div>
  );
}
