import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef, useState } from "react";
import type {
  CoachChat,
  CoachChatMessage,
  CoachHistory,
  CoachStatus,
  CoachStreamEvent,
  ToolRun,
} from "../types/coach";

const COACH_EVENT = "coach-chat-event";

function errorText(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

export function useCoachChat(): CoachChat {
  const [messages, setMessages] = useState<CoachChatMessage[]>([]);
  const [status, setStatus] = useState<CoachStatus | null>(null);
  const [activity, setActivity] = useState<string | null>(null);
  const [tools, setTools] = useState<ToolRun[]>([]);
  const [streaming, setStreaming] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const mounted = useRef(true);
  const currentTurn = useRef<number | null>(null);
  // Grounding frames are emitted while the send command is still running, so
  // they can arrive before we know the turn id. Hold them until we do.
  const pending = useRef<CoachStreamEvent[]>([]);

  const appendToLastAssistant = useCallback((chunk: string) => {
    setMessages((prev) => {
      const last = prev[prev.length - 1];
      if (!last || last.role !== "assistant") return prev;
      return [...prev.slice(0, -1), { ...last, content: last.content + chunk }];
    });
  }, []);

  const finishLastAssistant = useCallback((text?: string | null) => {
    setMessages((prev) => {
      const last = prev[prev.length - 1];
      if (!last || last.role !== "assistant") return prev;
      return [
        ...prev.slice(0, -1),
        {
          ...last,
          content: typeof text === "string" ? text : last.content,
          streaming: false,
        },
      ];
    });
  }, []);

  const apply = useCallback(
    (event: CoachStreamEvent) => {
      switch (event.type) {
        case "status":
          setActivity(event.data);
          break;
        case "tool_run":
          setTools((prev) => [...prev, event.data]);
          break;
        case "text_delta":
          setActivity(null);
          appendToLastAssistant(event.data);
          break;
        case "done": {
          finishLastAssistant(event.data.text);
          setStreaming(false);
          setActivity(null);
          currentTurn.current = null;
          if (event.data.reason === "failed") {
            setError(event.data.error ?? "The coach could not answer");
          }
          break;
        }
      }
    },
    [appendToLastAssistant, finishLastAssistant],
  );

  useEffect(() => {
    mounted.current = true;

    invoke<CoachHistory>("coach_history")
      .then((history) => {
        if (!mounted.current) return;
        setStatus(history.status);
        setMessages(
          history.messages
            .filter((m) => m.role !== "system")
            .map((m) => ({
              role: m.role as CoachChatMessage["role"],
              content: m.content,
            })),
        );
      })
      .catch((e) => {
        if (mounted.current) setError(errorText(e));
      });

    const unlisten = listen<CoachStreamEvent>(COACH_EVENT, (event) => {
      if (!mounted.current) return;
      const frame = event.payload;
      if (currentTurn.current === null) {
        pending.current.push(frame);
        return;
      }
      if (frame.turn_id !== currentTurn.current) return;
      apply(frame);
    });

    return () => {
      mounted.current = false;
      unlisten.then((off) => off());
    };
  }, [apply]);

  const send = useCallback(
    async (message: string) => {
      const question = message.trim();
      if (!question || streaming) return;

      setError(null);
      setTools([]);
      setActivity("Sending");
      setStreaming(true);
      pending.current = [];
      currentTurn.current = null;
      setMessages((prev) => [
        ...prev,
        { role: "user", content: question },
        { role: "assistant", content: "", streaming: true },
      ]);

      try {
        const { turn_id } = await invoke<{ turn_id: number }>(
          "coach_send_message",
          { message: question },
        );
        if (!mounted.current) return;

        currentTurn.current = turn_id;
        const queued = pending.current.filter((e) => e.turn_id === turn_id);
        pending.current = [];
        queued.forEach(apply);
      } catch (e) {
        if (!mounted.current) return;
        setError(errorText(e));
        setStreaming(false);
        setActivity(null);
        // Roll back the optimistic pair; the question was never registered.
        setMessages((prev) => prev.slice(0, -2));
      }
    },
    [apply, streaming],
  );

  const cancel = useCallback(async () => {
    try {
      await invoke<number | null>("coach_cancel");
    } catch (e) {
      if (mounted.current) setError(errorText(e));
    }
    if (!mounted.current) return;
    // Settle the UI here rather than waiting for the terminal frame, so Cancel
    // is responsive even if the turn completed in the same instant.
    setStreaming(false);
    setActivity(null);
    finishLastAssistant();
    currentTurn.current = null;
  }, [finishLastAssistant]);

  const reset = useCallback(async () => {
    try {
      const history = await invoke<CoachHistory>("coach_reset");
      if (!mounted.current) return;
      setStatus(history.status);
      setMessages([]);
      setTools([]);
      setActivity(null);
      setStreaming(false);
      setError(null);
      currentTurn.current = null;
      pending.current = [];
    } catch (e) {
      if (mounted.current) setError(errorText(e));
    }
  }, []);

  return {
    messages,
    status,
    activity,
    tools,
    streaming,
    error,
    send,
    cancel,
    reset,
  };
}
