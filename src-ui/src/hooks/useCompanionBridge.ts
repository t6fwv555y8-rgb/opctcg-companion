import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef, useState } from "react";
import type {
  CompanionBridge,
  OverlaySettings,
  StateUpdatePayload,
} from "../types/game";

const HEALTH_POLL_MS = 5000;

export function useCompanionBridge(): CompanionBridge {
  const [snapshot, setSnapshot] = useState<StateUpdatePayload | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
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
        setLoading(false);
        setError(null);
      }
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

  return {
    snapshot,
    loading,
    error,
    overlay,
    toggleOverlay,
    setOpacity,
  };
}
