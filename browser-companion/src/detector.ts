import { GenericSiteAdapter } from "./adapters/generic.js";
import { OneSimulatorSiteAdapter } from "./adapters/onesimulator/index.js";
import type { SimulatorSiteAdapter } from "./types.js";

const ADAPTERS: SimulatorSiteAdapter[] = [
  new OneSimulatorSiteAdapter(),
  new GenericSiteAdapter(),
];

export function resolveAdapter(location: Location): SimulatorSiteAdapter | null {
  for (const adapter of ADAPTERS) {
    if (adapter.matches(location)) {
      return adapter;
    }
  }
  return null;
}

export function detectGame(location: Location): boolean {
  const adapter = resolveAdapter(location);
  return adapter?.detectGame() ?? false;
}

export function getActiveAdapterId(location: Location): string | null {
  return resolveAdapter(location)?.id ?? null;
}
