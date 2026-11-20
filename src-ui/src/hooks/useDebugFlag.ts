import { useCallback, useState } from "react";

const KEY = "optcg.debugHud";

function read(): boolean {
  try {
    return window.localStorage.getItem(KEY) === "1";
  } catch {
    return false;
  }
}

/** Explicit, remembered opt-in for developer diagnostics (off by default). */
export function useDebugFlag(): [boolean, (on: boolean) => void] {
  const [on, setOn] = useState<boolean>(read);
  const set = useCallback((next: boolean) => {
    setOn(next);
    try {
      window.localStorage.setItem(KEY, next ? "1" : "0");
    } catch {
      /* storage unavailable: keep in-memory only */
    }
  }, []);
  return [on, set];
}
