import { useEffect, useRef, useState } from "react";
import { CalibrationPanel } from "./components/CalibrationPanel";
import { CoachLlmPanel } from "./components/CoachLlmPanel";
import { CoachChatPanel } from "./components/CoachChatPanel";
import { ConnectionStatus } from "./components/ConnectionStatus";
import { DebugPanel } from "./components/DebugPanel";
import { DeckBuilderPanel } from "./components/DeckBuilderPanel";
import { DeckPanel } from "./components/DeckPanel";
import { GlossaryPanel, TermSheet, useGlossary } from "./components/Glossary";
import { MatchBar } from "./components/MatchBar";
import { MatchHistoryPanel, PendingResultBanner } from "./components/MatchHistoryPanel";
import { MatchReviewPanel } from "./components/MatchReviewPanel";
import { MatchupPanel } from "./components/MatchupPanel";
import { MetaComparePanel } from "./components/MetaComparePanel";
import { NowPanel } from "./components/NowPanel";
import { ScoutingPanel } from "./components/ScoutingPanel";
import { SourceSelector } from "./components/SourceSelector";
import { useCoachStream } from "./hooks/useCoachStream";
import { useCompanionBridge } from "./hooks/useCompanionBridge";
import { useMatchHistory } from "./hooks/useMatchHistory";

const DEBUG = Boolean((import.meta as ImportMeta & { env?: { DEV?: boolean } }).env?.DEV);

type Tab = "play" | "opp" | "ask" | "setup";

const TABS: { id: Tab; label: string }[] = [
  { id: "play", label: "Play" },
  { id: "opp", label: "Opp" },
  { id: "ask", label: "Ask" },
  { id: "setup", label: "Setup" },
];

export default function App() {
  const bridge = useCompanionBridge();
  const coach = useCoachStream();
  const glossary = useGlossary();
  const matches = useMatchHistory();
  const gs = bridge.snapshot?.game_state ?? null;
  const combat = gs?.combat ?? null;
  const [tab, setTab] = useState<Tab>("play");
  const nudgedAuto = useRef(false);

  // Older sessions defaulted automatic reads off. Turn them on once so Play
  // keeps advising, but do not fight the user if they later switch them off.
  useEffect(() => {
    if (nudgedAuto.current || !coach.status) return;
    nudgedAuto.current = true;
    if (!coach.status.auto_enabled) {
      void coach.setAuto(true);
    }
  }, [coach.status, coach.setAuto]);

  const pageState = gs?.page_state ?? "";
  const recap = bridge.snapshot?.review ?? null;
  const thisGame = Boolean(recap && gs?.game_id && recap.game_id === gs.game_id);
  const showRecap = Boolean(recap && (pageState !== "match" || thisGame));
  const queued = pageState === "queue";
  const yourPlayer = gs?.player_one.player_name?.trim() || "You";
  const theirPlayer =
    gs?.player_two.player_name?.trim() ||
    (queued ? "Waiting for opponent" : "Opponent");

  return (
    <div
      className="app-shell flex h-full min-h-0 w-full flex-col text-base text-white"
      style={{ opacity: bridge.overlay.opacity }}
    >
      <MatchBar
        gameState={gs}
        yourName={yourPlayer}
        theirName={theirPlayer}
        hudState={bridge.observation?.hud_state ?? null}
        sourceLabel={bridge.observation?.active_source ?? null}
      />

      <nav className="shrink-0 px-3 pt-2">
        <div className="tab-bar">
          {TABS.map((item) => (
            <button
              key={item.id}
              type="button"
              onClick={() => setTab(item.id)}
              className={tab === item.id ? "on" : ""}
            >
              {item.label}
            </button>
          ))}
        </div>
      </nav>

      <div className="min-h-0 flex-1 overflow-y-auto overflow-x-hidden px-3 py-3">
        {bridge.error && (
          <div className="mb-3 rounded border border-hud-danger/50 bg-hud-danger/10 px-3 py-2 text-sm text-hud-danger">
            {bridge.error}
          </div>
        )}

        {bridge.loading ? (
          <div className="hud-panel flex items-center justify-center p-6 text-sm text-slate-400">
            Connecting…
          </div>
        ) : (
          <>
            <PendingResultBanner
              history={matches.history}
              onSetResult={matches.setResult}
            />
            {tab === "play" && (
              showRecap && recap ? (
                <MatchReviewPanel review={recap} />
              ) : (
                <NowPanel
                  thisTurn={bridge.snapshot?.deck_strategy?.this_turn ?? []}
                  combat={combat}
                  analysis={bridge.snapshot?.combat_analysis ?? null}
                  combatCoach={bridge.snapshot?.combat_coach ?? null}
                  paused={
                    bridge.observation?.analysis?.mode === "paused" ||
                    bridge.observation?.hud_state === "lost"
                  }
                  pageState={pageState}
                />
              )
            )}

            {tab === "opp" && (
              <div className="flex flex-col gap-3">
                {recap && <MatchReviewPanel review={recap} />}
                <MatchupPanel report={bridge.snapshot?.matchup ?? null} />
                <ScoutingPanel
                  report={bridge.snapshot?.scouting ?? null}
                  listAttached={
                    bridge.snapshot?.opponent_deck?.origin === "attached"
                  }
                />
              </div>
            )}

            {tab === "ask" && <CoachChatPanel />}

            {tab === "setup" && (
              <div className="flex flex-col gap-3">
                <CoachLlmPanel />
                <SourceSelector
                  observation={bridge.observation}
                  onSelect={bridge.setObservationSource}
                />
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
                  onImported={bridge.refresh}
                />
                <details className="hud-panel group">
                  <summary className="flex cursor-pointer select-none items-center justify-between gap-2 px-3 py-2 text-sm font-semibold text-slate-200">
                    <span>My games</span>
                    {matches.history && matches.history.stats.games > 0 && (
                      <span className="text-xs font-normal text-slate-400">
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
                  <summary className="cursor-pointer select-none px-3 py-2 text-sm font-semibold text-slate-200">
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
                  <summary className="cursor-pointer select-none px-3 py-2 text-sm font-semibold text-slate-200">
                    Compare to winning decks
                  </summary>
                  <div className="p-3 pt-0">
                    <MetaComparePanel
                      deckKey={`${bridge.snapshot?.your_deck?.name ?? ""}|${bridge.snapshot?.your_deck?.leader_id ?? ""}`}
                    />
                  </div>
                </details>
                <details className="hud-panel group">
                  <summary className="cursor-pointer select-none px-3 py-2 text-sm font-semibold text-slate-200">
                    Learn the game
                  </summary>
                  <div className="p-3 pt-0">
                    <GlossaryPanel />
                  </div>
                </details>
                <label className="flex cursor-pointer items-center gap-2 text-sm text-slate-200">
                  <input
                    type="checkbox"
                    checked={glossary.enabled}
                    onChange={(e) => glossary.setEnabled(e.target.checked)}
                  />
                  Explain game terms (tap underlined words)
                </label>
                <ConnectionStatus
                  connection={bridge.snapshot?.connection ?? null}
                />
                <button
                  type="button"
                  onClick={() => bridge.toggleOverlay()}
                  className="rounded border border-slate-700 px-3 py-2 text-sm text-slate-300"
                >
                  {bridge.overlay.click_through
                    ? "Click-through on"
                    : "Window is interactive"}
                </button>
                {DEBUG && <CalibrationPanel />}
                <DebugPanel enabled={DEBUG} />
              </div>
            )}
          </>
        )}
      </div>
      <TermSheet />
    </div>
  );
}
