import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef, useState } from "react";
import type {
  CompanionBridge,
  ObservationStatusDto,
  OverlaySettings,
  Side,
  SourceSelectionKind,
  StateUpdatePayload,
} from "../types/game";

const HEALTH_POLL_MS = 5000;

export function useCompanionBridge(): CompanionBridge {
  const [snapshot, setSnapshot] = useState<StateUpdatePayload | null>(null);
  const [observation, setObservation] = useState<ObservationStatusDto | null>(
    null,
  );
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [refreshingStrategy, setRefreshingStrategy] = useState(false);
  const [overlay, setOverlay] = useState<OverlaySettings>({
    click_through: false,
    opacity: 0.92,
  });
  const mounted = useRef(true);

  const refreshSnapshot = useCallback(async () => {
    try {
      const payload = await invoke<StateUpdatePayload>("get_state_snapshot");
      if (!mounted.current) return;
      setSnapshot(payload);
      setObservation(payload.observation ?? null);
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

    refreshSnapshot();

    const unlistenPromise = listen<StateUpdatePayload>(
      "game-state-updated",
      (event) => {
        if (!mounted.current) return;
        setSnapshot(event.payload);
        setObservation(event.payload.observation ?? null);
        setLoading(false);
        setError(null);
      },
    );

    const healthId = setInterval(refreshSnapshot, HEALTH_POLL_MS);

    return () => {
      mounted.current = false;
      clearInterval(healthId);
      unlistenPromise.then((unlisten) => unlisten());
    };
  }, [refreshSnapshot]);

  const toggleOverlay = useCallback(async (enabled?: boolean) => {
    try {
      const settings = await invoke<OverlaySettings>("toggle_overlay", {
        enabled,
      });
      setOverlay(settings);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, []);

  const setOpacity = useCallback(async (opacity: number) => {
    try {
      const settings = await invoke<OverlaySettings>("set_overlay_opacity", {
        opacity,
      });
      setOverlay(settings);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, []);

  const setObservationSource = useCallback(
    async (selection: SourceSelectionKind) => {
      try {
        const status = await invoke<ObservationStatusDto>(
          "set_observation_source",
          { selection },
        );
        setObservation(status);
        await refreshSnapshot();
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      }
    },
    [refreshSnapshot],
  );

  const runStateCommand = useCallback(
    async (command: string, args?: Record<string, unknown>) => {
      setRefreshingStrategy(true);
      try {
        const payload = await invoke<StateUpdatePayload>(command, args);
        if (!mounted.current) return;
        setSnapshot(payload);
        setObservation(payload.observation ?? null);
        setError(null);
      } catch (e) {
        if (!mounted.current) return;
        setError(e instanceof Error ? e.message : String(e));
      } finally {
        if (mounted.current) setRefreshingStrategy(false);
      }
    },
    [],
  );

  const refreshDeckStrategy = useCallback(
    () => runStateCommand("refresh_deck_strategy"),
    [runStateCommand],
  );

  const setPastedDeck = useCallback(
    (raw: string) => runStateCommand("set_pasted_deck", { raw }),
    [runStateCommand],
  );

  const clearPastedDeck = useCallback(
    () => runStateCommand("clear_pasted_deck"),
    [runStateCommand],
  );

  const saveDeck = useCallback(
    ({
      raw,
      name,
      id,
      side,
    }: {
      raw: string;
      name?: string;
      id?: string;
      side?: Side;
    }) =>
      runStateCommand("save_deck", {
        raw,
        name: name ?? null,
        id: id ?? null,
        side: side ?? "you",
      }),
    [runStateCommand],
  );

  /// Point one side at a saved list, or pass null to read it from play.
  const setDeckSource = useCallback(
    (side: Side, deckId: string | null) =>
      runStateCommand("set_deck_source", { side, deckId }),
    [runStateCommand],
  );

  const activateDeck = useCallback(
    (id: string) => runStateCommand("activate_deck", { id }),
    [runStateCommand],
  );

  const deleteDeck = useCallback(
    (id: string) => runStateCommand("delete_deck", { id }),
    [runStateCommand],
  );

  const renameDeck = useCallback(
    (id: string, name: string) => runStateCommand("rename_deck", { id, name }),
    [runStateCommand],
  );

  return {
    snapshot,
    observation,
    loading,
    error,
    overlay,
    refreshingStrategy,
    toggleOverlay,
    setOpacity,
    setObservationSource,
    refreshDeckStrategy,
    setPastedDeck,
    clearPastedDeck,
    saveDeck,
    setDeckSource,
    activateDeck,
    deleteDeck,
    renameDeck,
  };
}
