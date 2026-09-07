import { memo, useEffect, useRef, useState } from "react";
import { useCoachStream } from "../hooks/useCoachStream";
import type {
  CoachChatMessage,
  ContextScope,
  FinishReason,
  ToolRun,
} from "../types/coach";

const SUGGESTIONS = [
  "What should I do this turn?",
  "Can I survive this attack?",
  "How does this matchup play out?",
];

const ENDING_NOTE: Record<Exclude<FinishReason, "complete">, string> = {
  cancelled: "Stopped",
  interrupted: "Board changed — answer stopped",
  failed: "Could not finish",
};

/**
 * Memoized so a streaming answer only re-renders its own bubble. The hook
 * preserves the object identity of completed messages, which is what makes
 * this effective.
 */
const Bubble = memo(function Bubble({
  message,
}: {
  message: CoachChatMessage;
}) {
  const isUser = message.role === "user";
  const waiting = message.content.length === 0 && message.streaming;

  return (
    <div className={`flex ${isUser ? "justify-end" : "justify-start"}`}>
      <div
        className={`max-w-[88%] rounded px-3 py-2 text-sm leading-relaxed ${
          isUser
            ? "bg-hud-accent/15 text-slate-100"
            : "bg-slate-800/60 text-slate-200"
        }`}
      >
        <div className="whitespace-pre-wrap">
          {waiting ? (
            <span className="animate-pulse text-slate-400">Thinking…</span>
          ) : (
            <>
              {message.content}
              {message.streaming && (
                <span className="ml-0.5 animate-pulse text-hud-accent">▌</span>
              )}
            </>
          )}
        </div>
        {!isUser &&
          (message.automatic || message.groundedOn || message.endedBecause) && (
          <div className="mt-0.5 flex flex-wrap items-center gap-1 text-[8px] text-slate-500">
            {message.automatic && (
              <span
                title="Read automatically when the board settled"
                className="rounded bg-hud-accent/15 px-1 font-mono text-hud-accent"
              >
                auto
              </span>
            )}
            {message.groundedOn && <span>{message.groundedOn.label}</span>}
            {message.endedBecause && (
              <span
                className={
                  message.endedBecause === "failed"
                    ? "text-hud-danger"
                    : "text-hud-warn"
                }
              >
                {ENDING_NOTE[message.endedBecause]}
              </span>
            )}
          </div>
        )}
      </div>
    </div>
  );
});

function AutoToggle({
  enabled,
  disabled,
  onChange,
}: {
  enabled: boolean;
  disabled: boolean;
  onChange: (enabled: boolean) => void;
}) {
  const title = disabled
    ? "Unavailable while the board is withheld — an automatic read has nothing to read"
    : enabled
      ? "Reading the board on its own after each move settles"
      : "Read the board on its own after each move settles, without being asked";

  return (
    <button
      type="button"
      role="switch"
      aria-checked={enabled}
      disabled={disabled}
      onClick={() => onChange(!enabled)}
      title={title}
      className={`rounded border px-1.5 py-0.5 font-mono text-[8px] disabled:opacity-40 ${
        enabled
          ? "border-hud-accent/50 bg-hud-accent/15 text-hud-accent"
          : "border-slate-600/60 text-slate-500 enabled:hover:text-slate-300"
      }`}
    >
      auto
    </button>
  );
}

/**
 * What the next question will send, and a way to take any of it back.
 *
 * Every turn ships the live board and your deck list to whichever model is
 * configured, which may be a third-party API. This makes that visible at the
 * point of asking and lets it be withheld per question.
 */
function ContextPills({
  scope,
  onChange,
}: {
  scope: ContextScope;
  onChange: (scope: ContextScope) => void;
}) {
  const items = [
    {
      key: "board" as const,
      label: "Board",
      title: "Live position, counter estimate, phase, combat math, ranked options",
    },
    {
      key: "deck" as const,
      label: "Deck",
      title: "Your saved deck list, leader, and matchup plan",
    },
  ];

  return (
    <div className="flex flex-wrap items-center gap-1 text-[9px]">
      <span className="text-slate-500">
        {scope.board || scope.deck ? "Sharing" : "Sharing nothing"}
      </span>
      {items.map(({ key, label, title }) => {
        const on = scope[key];
        return (
          <button
            key={key}
            type="button"
            role="switch"
            aria-checked={on}
            title={on ? `${title} — click to withhold` : `${title} — click to share`}
            onClick={() => onChange({ ...scope, [key]: !on })}
            className={`rounded border px-1 py-px ${
              on
                ? "border-hud-accent/40 bg-hud-accent/10 text-hud-accent"
                : "border-slate-700/60 text-slate-600 line-through hover:text-slate-400"
            }`}
          >
            {label}
          </button>
        );
      })}
      {!scope.board && !scope.deck && (
        <span className="text-slate-600">— rules answers only</span>
      )}
    </div>
  );
}

function ToolChips({ tools }: { tools: ToolRun[] }) {
  if (tools.length === 0) return null;
  return (
    <div className="flex flex-wrap gap-1">
      {tools.map((tool, i) => (
        <span
          key={`${tool.tool}-${i}`}
          title={tool.summary}
          className="rounded bg-slate-800/70 px-1 py-0.5 font-mono text-[8px] text-slate-400"
        >
          {tool.tool}
        </span>
      ))}
    </div>
  );
}

export function CoachChatPanel() {
  const coach = useCoachStream();
  const [draft, setDraft] = useState("");
  const [open, setOpen] = useState(true);
  const scroller = useRef<HTMLDivElement | null>(null);

  // Follow the stream as text arrives.
  useEffect(() => {
    const el = scroller.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [coach.messages, coach.activity]);

  // Asking supersedes whatever is streaming, so this stays available during an
  // automatic read rather than making the user stop it first.
  const submit = (text: string) => {
    if (!text.trim()) return;
    setDraft("");
    void coach.send(text);
  };

  const provider = coach.status?.provider ?? "…";

  return (
    <div className="hud-panel p-3">
      <div className="flex items-center justify-between gap-2">
        <div className="hud-title">Ask the Coach</div>
        <div className="flex items-center gap-1">
          <span
            title={
              coach.status?.live
                ? `Answers from ${provider}`
                : "No model API key set — answering from the rules engine. Set OPTCG_LLM_API_KEY for conversational answers."
            }
            className={`rounded px-1.5 py-0.5 font-mono text-[8px] ${
              coach.status?.live
                ? "bg-hud-success/20 text-hud-success"
                : "bg-slate-700/60 text-slate-400"
            }`}
          >
            {provider}
          </span>
          <AutoToggle
            enabled={coach.status?.auto_enabled ?? true}
            disabled={coach.status ? !coach.status.context.board : true}
            onChange={(enabled) => {
              if (enabled) setOpen(true);
              void coach.setAuto(enabled);
            }}
          />
          <button
            type="button"
            onClick={() => setOpen((v) => !v)}
            className="rounded border border-slate-600/60 px-2 py-0.5 text-[10px] text-slate-300 hover:bg-slate-800/60"
          >
            {open ? "Hide" : "Open"}
          </button>
        </div>
      </div>

      {open && (
        <div className="mt-2 space-y-1.5">
          <div
            ref={scroller}
            className="max-h-64 space-y-2 overflow-y-auto pr-0.5"
          >
            {coach.messages.length === 0 ? (
              <div className="space-y-1">
                <p className="text-sm text-slate-400">
                  Ask about the live board. The coach reads your game state,
                  deck list, and the rules engine before answering.
                </p>
                <div className="flex flex-wrap gap-1">
                  {SUGGESTIONS.map((suggestion) => (
                    <button
                      key={suggestion}
                      type="button"
                      onClick={() => submit(suggestion)}
                      className="rounded border border-slate-700/60 px-2 py-1 text-left text-sm text-slate-300 hover:border-hud-accent/40 hover:text-hud-accent"
                    >
                      {suggestion}
                    </button>
                  ))}
                </div>
              </div>
            ) : (
              coach.messages.map((message, i) => (
                <Bubble key={i} message={message} />
              ))
            )}
          </div>

          {(coach.activity || coach.tools.length > 0) && (
            <div className="space-y-1 border-t border-slate-700/40 pt-1">
              {coach.activity && (
                <div className="flex items-center gap-1 text-[9px] text-slate-400">
                  <span className="h-1 w-1 animate-pulse rounded-full bg-hud-accent" />
                  {coach.activity}…
                </div>
              )}
              <ToolChips tools={coach.tools} />
            </div>
          )}

          {coach.error && (
            <p className="text-[9px] text-hud-danger">{coach.error}</p>
          )}

          {coach.status && (
            <ContextPills
              scope={coach.status.context}
              onChange={(scope) => void coach.setContext(scope)}
            />
          )}

          <div className="flex items-end gap-1">
            <textarea
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
              onKeyDown={(e) => {
                // Enter sends; Shift+Enter makes a new line.
                if (e.key === "Enter" && !e.shiftKey) {
                  e.preventDefault();
                  submit(draft);
                }
              }}
              placeholder="Ask about this board…"
              rows={2}
              className="min-w-0 flex-1 resize-none rounded border border-slate-700 bg-slate-950/80 px-3 py-2 text-sm leading-snug text-slate-200 placeholder:text-slate-600 focus:border-hud-accent/50 focus:outline-none"
            />
            <div className="flex shrink-0 flex-col gap-1">
              {coach.streaming && (
                <button
                  type="button"
                  onClick={() => void coach.interrupt()}
                  className="rounded border border-hud-warn/50 bg-hud-warn/10 px-2 py-0.5 text-[10px] font-semibold text-hud-warn hover:bg-hud-warn/20"
                >
                  Stop
                </button>
              )}
              {(!coach.streaming || draft.trim()) && (
                <button
                  type="button"
                  disabled={!draft.trim()}
                  onClick={() => submit(draft)}
                  className="rounded border border-hud-accent/40 bg-hud-accent/10 px-3 py-1.5 text-sm font-semibold text-hud-accent hover:bg-hud-accent/20 disabled:opacity-50"
                >
                  Ask
                </button>
              )}
              {coach.messages.length > 0 && (
                <button
                  type="button"
                  onClick={() => void coach.reset()}
                  className="rounded border border-slate-600/60 px-2 py-0.5 text-[9px] text-slate-400 hover:bg-slate-800/60"
                >
                  Clear
                </button>
              )}
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
