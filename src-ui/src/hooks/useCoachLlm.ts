import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useState } from "react";

export type LlmKeySource = "env" | "saved" | "none";

export interface CoachLlmSettings {
  configured: boolean;
  live: boolean;
  source: LlmKeySource;
  provider: string;
  model: string;
  base_url: string;
  key_hint: string | null;
}

function errorText(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

export function useCoachLlm() {
  const [settings, setSettings] = useState<CoachLlmSettings | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  const refresh = useCallback(async () => {
    try {
      const next = await invoke<CoachLlmSettings>("coach_llm_settings");
      setSettings(next);
      setError(null);
    } catch (e: unknown) {
      setError(errorText(e));
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const save = useCallback(
    async (apiKey: string, model: string, baseUrl: string) => {
      setSaving(true);
      try {
        const next = await invoke<CoachLlmSettings>("coach_set_llm", {
          apiKey: apiKey.trim() || null,
          model,
          baseUrl,
        });
        setSettings(next);
        setError(null);
        return next;
      } catch (e: unknown) {
        setError(errorText(e));
        return null;
      } finally {
        setSaving(false);
      }
    },
    [],
  );

  const clear = useCallback(async () => {
    setSaving(true);
    try {
      const next = await invoke<CoachLlmSettings>("coach_clear_llm");
      setSettings(next);
      setError(null);
      return next;
    } catch (e: unknown) {
      setError(errorText(e));
      return null;
    } finally {
      setSaving(false);
    }
  }, []);

  return { settings, error, saving, refresh, save, clear };
}
