import type { ObservableGameSnapshot } from "../../types.js";
import type { SimulatorSiteAdapter } from "../../types.js";
import { diagnoseGameUi } from "./diagnostics.js";
import { extractOneSimulatorSnapshot } from "./extract.js";
import { ONE_SIMULATOR_HOST, SELECTORS } from "./selectors.js";

export class OneSimulatorSiteAdapter implements SimulatorSiteAdapter {
  id = "onesimulator";

  matches(location: Location): boolean {
    return location.hostname === ONE_SIMULATOR_HOST;
  }

  detectGame(): boolean {
    if (!this.matches(window.location)) return false;
    const diag = diagnoseGameUi();
    return diag.game_detected;
  }

  observe(): ObservableGameSnapshot {
    if (!this.matches(window.location)) {
      return { detected: false, snapshot: null };
    }

    const diag = diagnoseGameUi();
    if (!diag.site_detected) {
      return { detected: false, snapshot: null, diagnostics: diag };
    }

    if (!diag.game_detected) {
      return {
        detected: false,
        snapshot: {
          timestamp: Date.now(),
          source: "onesimulator",
          diagnostics: diag,
        },
        diagnostics: diag,
      };
    }

    if (!diag.ui_recognized) {
      return {
        detected: false,
        snapshot: {
          timestamp: Date.now(),
          source: "onesimulator",
          diagnostics: diag,
        },
        diagnostics: diag,
      };
    }

    const snapshot = extractOneSimulatorSnapshot();
    return { detected: true, snapshot, diagnostics: diag };
  }

  /** For tests / diagnostics */
  static requiredSelectors(): string[] {
    return Object.values(SELECTORS);
  }
}

export { ONE_SIMULATOR_HOST };
