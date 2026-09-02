import { GenericSiteAdapter, type SimulatorSiteAdapter } from "./adapters/generic";

const ADAPTERS: SimulatorSiteAdapter[] = [new GenericSiteAdapter()];

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
