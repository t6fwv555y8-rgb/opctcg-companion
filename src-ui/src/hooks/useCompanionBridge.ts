import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef, useState } from "react";
import type {
  CompanionBridge,
  ObservationStatusDto,
  OverlaySettings,
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

  const refreshDeckStrategy = useCallback(async () => {
    setRefreshingStrategy(true);
    try {
      const payload = await invoke<StateUpdatePayload>("refresh_deck_strategy");
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
  }, []);

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
  };
}
