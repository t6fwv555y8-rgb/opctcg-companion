import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useState } from "react";
import type { MatchHistoryDto } from "../types/game";

/** Your recorded matches, refreshed whenever the app saves or changes one. */
export function useMatchHistory() {
  const [history, setHistory] = useState<MatchHistoryDto | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      setHistory(await invoke<MatchHistoryDto>("get_match_history"));
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    void refresh();
    let off: (() => void) | undefined;
    listen("matches-updated", () => void refresh())
      .then((fn) => {
        off = fn;
      })
      .catch(() => {});
    return () => off?.();
  }, [refresh]);

  const setResult = useCallback(
    async (id: string, result: "win" | "loss" | null) => {
      try {
        await invoke("set_match_result", { id, result });
      } catch (e) {
        setError(String(e));
      }
      await refresh();
    },
    [refresh],
  );

  const remove = useCallback(
    async (id: string) => {
      try {
        await invoke("delete_match", { id });
      } catch (e) {
        setError(String(e));
      }
      await refresh();
    },
    [refresh],
  );

  return { history, error, setResult, remove, refresh };
}
