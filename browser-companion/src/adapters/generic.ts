import type { BrowserGameSnapshot, ObservableGameSnapshot, SimulatorSiteAdapter } from "../types.js";

/** Generic fallback adapter — reads coarse visible text only, never hidden DOM. */
export class GenericSiteAdapter implements SimulatorSiteAdapter {
  id = "generic";

  matches(_location: Location): boolean {
    return true;
  }

  detectGame(): boolean {
    const body = document.body?.innerText?.toLowerCase() ?? "";
    return (
      body.includes("don!!") ||
      body.includes("life") ||
      body.includes("main phase") ||
      body.includes("one piece")
    );
  }

  observe(): ObservableGameSnapshot {
    if (!this.detectGame()) {
      return { detected: false, snapshot: null };
    }

    const text = document.body?.innerText ?? "";
    const snapshot: BrowserGameSnapshot = {
      timestamp: Date.now(),
      phase: inferPhase(text),
      self: {
        life: inferLife(text, "life"),
        hand_count: inferCount(text, "hand"),
        active_don: inferCount(text, "active don"),
        rested_don: inferCount(text, "rested don"),
        board: [],
      },
      opponent: {
        life: inferOpponentLife(text),
        hand_count: null,
        board: [],
      },
    };

    return { detected: true, snapshot };
  }
}

function inferPhase(text: string): string | null {
  const lower = text.toLowerCase();
  if (lower.includes("main phase")) return "Main";
  if (lower.includes("draw phase")) return "Draw";
  if (lower.includes("don!! phase")) return "Don";
  if (lower.includes("end phase")) return "End";
  return null;
}

function inferLife(text: string, label: string): number | null {
  const re = new RegExp(`${label}\\s*[:\\-]?\\s*(\\d+)`, "i");
  const m = text.match(re);
  return m ? Number(m[1]) : null;
}

function inferOpponentLife(text: string): number | null {
  const m = text.match(/opponent.*?life\s*[:\\-]?\s*(\d+)/i);
  return m ? Number(m[1]) : null;
}

function inferCount(text: string, label: string): number | null {
  const re = new RegExp(`${label}\\s*[:\\-]?\\s*(\\d+)`, "i");
  const m = text.match(re);
  return m ? Number(m[1]) : null;
}
