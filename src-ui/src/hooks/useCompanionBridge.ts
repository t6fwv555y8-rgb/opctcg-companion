import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useRef, useState } from "react";
import type {
  CombatAnalysis,
  CompanionBridge,
  ConnectionStatus,
  GameState,
  LegalAction,
  RecommendationsPayload,
} from "../types/companion";

const POLL_INTERVAL_MS = 100;

async function safeInvoke<T>(cmd: string): Promise<T | null> {
  try {
    return await invoke<T>(cmd);
  } catch {
    return null;
  }
}

export function useCompanionBridge(): CompanionBridge {
  const [gameState, setGameState] = useState<GameState | null>(null);
  const [recommendations, setRecommendations] =
    useState<RecommendationsPayload | null>(null);
  const [combatAnalysis, setCombatAnalysis] = useState<CombatAnalysis | null>(
    null
  );
  const [connectionStatus, setConnectionStatus] =
    useState<ConnectionStatus | null>(null);
  const [legalActions, setLegalActions] = useState<LegalAction[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [clickThrough, setClickThroughState] = useState(false);
  const mounted = useRef(true);

  const poll = useCallback(async () => {
    if (!mounted.current) return;

    try {
      const [gs, recs, combat, conn, actions] = await Promise.all([
        invoke<GameState>("get_game_state"),
        invoke<RecommendationsPayload>("get_recommendations"),
        invoke<CombatAnalysis | null>("get_combat_analysis"),
        invoke<ConnectionStatus>("get_connection_status"),
        invoke<LegalAction[]>("get_legal_actions"),
      ]);

      if (!mounted.current) return;

      setGameState(gs);
      setRecommendations(recs);
      setCombatAnalysis(combat);
      setConnectionStatus(conn);
      setLegalActions(actions);
      setError(null);
      setLoading(false);
    } catch (e) {
      if (!mounted.current) return;
      setError(e instanceof Error ? e.message : String(e));
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    mounted.current = true;
    poll();
    const id = setInterval(poll, POLL_INTERVAL_MS);
    return () => {
      mounted.current = false;
      clearInterval(id);
    };
  }, [poll]);

  const setClickThrough = useCallback(async (enabled: boolean) => {
    try {
      await invoke("set_click_through", { enabled });
      setClickThroughState(enabled);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, []);

  return {
    gameState,
    recommendations,
    combatAnalysis,
    connectionStatus,
    legalActions,
    loading,
    error,
    clickThrough,
    setClickThrough,
  };
}

export { safeInvoke };
