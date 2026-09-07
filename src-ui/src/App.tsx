import { useState } from "react";
import { CalibrationPanel } from "./components/CalibrationPanel";
import { CoachChatPanel } from "./components/CoachChatPanel";
import { ConnectionStatus } from "./components/ConnectionStatus";
import { DebugPanel } from "./components/DebugPanel";
import { DeckPanel } from "./components/DeckPanel";
import { MatchBar } from "./components/MatchBar";
import { MatchupPanel } from "./components/MatchupPanel";
import { NowPanel } from "./components/NowPanel";
import { ScoutingPanel } from "./components/ScoutingPanel";
import { SourceSelector } from "./components/SourceSelector";
import { useCompanionBridge } from "./hooks/useCompanionBridge";

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
  const gs = bridge.snapshot?.game_state ?? null;
  const combat = gs?.combat ?? null;
  const [tab, setTab] = useState<Tab>("play");

  return (
    <div
      className="flex h-full min-h-0 w-full flex-col bg-slate-950 text-base text-white"
      style={{ opacity: bridge.overlay.opacity }}
    >
      <MatchBar
        gameState={gs}
        yourName={bridge.snapshot?.your_deck?.name ?? "You"}
        theirName={bridge.snapshot?.opponent_deck?.name ?? "Opponent"}
        hudState={bridge.observation?.hud_state ?? null}
        sourceLabel={bridge.observation?.active_source ?? null}
      />

      <nav className="flex shrink-0 border-b border-slate-800">
        {TABS.map((item) => (
          <button
            key={item.id}
            type="button"
            onClick={() => setTab(item.id)}
            className={`flex-1 py-2.5 text-sm font-medium ${
              tab === item.id
                ? "border-b-2 border-hud-accent text-white"
                : "text-slate-400 hover:text-slate-200"
            }`}
          >
            {item.label}
          </button>
        ))}
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
            {tab === "play" && (
              <NowPanel
                phaseCoach={bridge.snapshot?.phase_coach ?? null}
                strategy={bridge.snapshot?.strategy ?? null}
                deckStrategy={bridge.snapshot?.deck_strategy ?? null}
                combat={combat}
                analysis={bridge.snapshot?.combat_analysis ?? null}
                paused={
                  bridge.observation?.analysis?.mode === "paused" ||
                  bridge.observation?.hud_state === "lost"
                }
              />
            )}

            {tab === "opp" && (
              <div className="flex flex-col gap-3">
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
                />
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
    </div>
  );
}
