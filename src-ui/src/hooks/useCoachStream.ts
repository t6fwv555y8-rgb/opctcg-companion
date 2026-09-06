import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef, useState } from "react";
import type {
  CoachChatMessage,
  CoachHistory,
  CoachStatus,
  CoachStream,
  CoachStreamEvent,
  ToolRun,
} from "../types/coach";

const COACH_EVENT = "coach://event";

function errorText(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

/**
 * Subscribes to the coach's stream and exposes it as chat state.
 *
 * Text arrives already batched by the backend, so each frame is one render
 * rather than one per token. Completed messages keep their object identity
 * across updates, which lets a memoized bubble skip re-rendering while a later
 * answer streams.
 */
export function useCoachStream(): CoachStream {
  const [messages, setMessages] = useState<CoachChatMessage[]>([]);
  const [status, setStatus] = useState<CoachStatus | null>(null);
  const [activity, setActivity] = useState<string | null>(null);
  const [tools, setTools] = useState<ToolRun[]>([]);
  const [streaming, setStreaming] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const mounted = useRef(true);
  const currentTurn = useRef<number | null>(null);
  // The send command returns the turn id, but the backend may emit grounding
  // frames before that response reaches us. Hold them until the id is known.
  const pending = useRef<CoachStreamEvent[]>([]);

  /** Replace the trailing assistant message, preserving earlier identities. */
  const updateStreamingMessage = useCallback(
    (change: (message: CoachChatMessage) => CoachChatMessage) => {
      setMessages((prev) => {
        const last = prev[prev.length - 1];
        if (!last || last.role !== "assistant") return prev;
        return [...prev.slice(0, -1), change(last)];
      });
    },
    [],
  );

  const apply = useCallback(
    (event: CoachStreamEvent) => {
      switch (event.type) {
        case "state_sync":
          updateStreamingMessage((message) => ({
            ...message,
            groundedOn: event.data,
          }));
          break;
        case "status":
          setActivity(event.data);
          break;
        case "tool_run":
          setTools((prev) => [...prev, event.data]);
          break;
        case "text_delta":
          setActivity(null);
          updateStreamingMessage((message) => ({
            ...message,
            content: message.content + event.data,
          }));
          break;
        case "done": {
          const { reason, text } = event.data;
          updateStreamingMessage((message) => ({
            ...message,
            content: typeof text === "string" ? text : message.content,
            streaming: false,
            endedBecause: reason === "complete" ? undefined : reason,
          }));
          setStreaming(false);
          setActivity(null);
          currentTurn.current = null;
          if (reason === "failed") {
            setError(event.data.error ?? "The coach could not answer");
          }
          break;
        }
      }
    },
    [updateStreamingMessage],
  );

  useEffect(() => {
    mounted.current = true;
    let unlisten: UnlistenFn | undefined;
    let cancelled = false;

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
      .catch((e: unknown) => {
        if (mounted.current) setError(errorText(e));
      });

    listen<CoachStreamEvent>(COACH_EVENT, (event) => {
      if (!mounted.current) return;
      const frame = event.payload;
      if (currentTurn.current === null) {
        pending.current.push(frame);
        return;
      }
      if (frame.turn_id !== currentTurn.current) return;
      apply(frame);
    })
      .then((off) => {
        // Unsubscribe immediately if the effect was torn down while listening.
        if (cancelled) {
          off();
          return;
        }
        unlisten = off;
      })
      .catch((e: unknown) => {
        if (mounted.current) setError(errorText(e));
      });

    return () => {
      mounted.current = false;
      cancelled = true;
      unlisten?.();
    };
  }, [apply]);

  const send = useCallback(
    async (message: string) => {
      const question = message.trim();
      if (!question || streaming) return;

      setError(null);
      setTools([]);
      setActivity("Reading the board");
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
      } catch (e: unknown) {
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

  const interrupt = useCallback(async () => {
    try {
      await invoke<number | null>("coach_cancel");
    } catch (e: unknown) {
      if (mounted.current) setError(errorText(e));
    }
    if (!mounted.current) return;
    // Settle the UI here rather than waiting for the terminal frame, so Stop
    // is responsive even if the turn completed in the same instant.
    setStreaming(false);
    setActivity(null);
    updateStreamingMessage((message) =>
      message.streaming
        ? { ...message, streaming: false, endedBecause: "cancelled" }
        : message,
    );
    currentTurn.current = null;
  }, [updateStreamingMessage]);

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
    } catch (e: unknown) {
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
    interrupt,
    reset,
  };
}
