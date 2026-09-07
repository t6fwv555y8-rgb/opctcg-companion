import { useEffect, useState } from "react";
import { useCoachLlm } from "../hooks/useCoachLlm";

const DEFAULT_MODEL = "gpt-4o-mini";
const DEFAULT_BASE = "https://api.openai.com/v1";

export function CoachLlmPanel() {
  const llm = useCoachLlm();
  const [apiKey, setApiKey] = useState("");
  const [model, setModel] = useState(DEFAULT_MODEL);
  const [baseUrl, setBaseUrl] = useState(DEFAULT_BASE);

  useEffect(() => {
    if (!llm.settings) return;
    setModel(llm.settings.model || DEFAULT_MODEL);
    setBaseUrl(llm.settings.base_url || DEFAULT_BASE);
  }, [llm.settings]);

  const live = Boolean(llm.settings?.live);
  const source = llm.settings?.source ?? "none";

  const onSave = async () => {
    const next = await llm.save(apiKey, model, baseUrl);
    if (next?.live) setApiKey("");
  };

  return (
    <section className="hud-panel p-3">
      <div className="hud-title">Coach model</div>
      <p className="mt-2 text-xs leading-relaxed text-slate-300">
        Paste an OpenAI API key so Ask can talk like a coach. It stays on this
        machine. Without a key, Ask still answers from the rules engine.
      </p>

      <div className="mt-2 flex items-center gap-2 text-xs">
        <span
          className={`rounded px-1.5 py-0.5 font-mono ${
            live
              ? "bg-hud-success/20 text-hud-success"
              : "bg-slate-700/60 text-slate-400"
          }`}
        >
          {llm.settings?.provider ?? "…"}
        </span>
        <span className="text-slate-400">
          {live
            ? source === "env"
              ? `Using ${llm.settings?.key_hint ?? "a key"} from the environment`
              : `Key saved ${llm.settings?.key_hint ?? ""}`.trim()
            : "No key yet — Ask is offline"}
        </span>
      </div>

      <label className="mt-3 block text-[10px] uppercase tracking-wide text-slate-400">
        API key
        <input
          type="password"
          autoComplete="off"
          value={apiKey}
          onChange={(e) => setApiKey(e.target.value)}
          placeholder={
            llm.settings?.key_hint
              ? `Saved ${llm.settings.key_hint} — paste to replace`
              : "sk-…"
          }
          className="mt-1 w-full rounded border border-slate-700 bg-slate-900/80 px-2 py-1.5 font-mono text-sm text-slate-100 outline-none focus:border-hud-accent"
        />
      </label>

      <label className="mt-2 block text-[10px] uppercase tracking-wide text-slate-400">
        Model
        <input
          value={model}
          onChange={(e) => setModel(e.target.value)}
          placeholder={DEFAULT_MODEL}
          className="mt-1 w-full rounded border border-slate-700 bg-slate-900/80 px-2 py-1.5 font-mono text-sm text-slate-100 outline-none focus:border-hud-accent"
        />
      </label>

      <label className="mt-2 block text-[10px] uppercase tracking-wide text-slate-400">
        Endpoint
        <input
          value={baseUrl}
          onChange={(e) => setBaseUrl(e.target.value)}
          placeholder={DEFAULT_BASE}
          className="mt-1 w-full rounded border border-slate-700 bg-slate-900/80 px-2 py-1.5 font-mono text-sm text-slate-100 outline-none focus:border-hud-accent"
        />
      </label>

      {llm.error && (
        <p className="mt-2 text-sm text-hud-danger">{llm.error}</p>
      )}

      <div className="mt-3 flex gap-2">
        <button
          type="button"
          disabled={llm.saving}
          onClick={() => void onSave()}
          className="rounded bg-hud-accent/20 px-3 py-1.5 text-sm text-hud-accent disabled:opacity-50"
        >
          {llm.saving ? "Saving…" : "Save and use"}
        </button>
        {source === "saved" && (
          <button
            type="button"
            disabled={llm.saving}
            onClick={() => void llm.clear()}
            className="rounded border border-slate-700 px-3 py-1.5 text-sm text-slate-300 disabled:opacity-50"
          >
            Clear saved key
          </button>
        )}
      </div>

      <p className="mt-2 text-[11px] leading-relaxed text-slate-500">
        Get a key at platform.openai.com → API keys. Local runners (Ollama, LM
        Studio) can use any key and set the endpoint to their server.
      </p>
    </section>
  );
}
