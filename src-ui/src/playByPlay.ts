/** Rayleigh's broadcast: keep the last coach line, never snap back to phase copy. */
export function playByPlayLine(
  coachLine: string | null | undefined,
  fallback: string,
): string {
  const rayleigh = coachLine?.trim() ?? "";
  return rayleigh || fallback;
}

export type CoachPin = {
  game: string;
  text: string;
  /** Coach text left over from the previous game, ignored until it changes. */
  stale: string;
};

/** Pin Rayleigh's line for this game; drop it when the match id changes. */
export function nextCoachPin(
  pin: CoachPin | null,
  game: string,
  incoming: string,
): CoachPin {
  const line = incoming.trim();
  if (pin === null) {
    return { game, text: line, stale: "" };
  }
  if (pin.game !== game) {
    if (line && line !== pin.text) {
      return { game, text: line, stale: "" };
    }
    return { game, text: "", stale: line };
  }
  if (line && line !== pin.stale) {
    return { game, text: line, stale: "" };
  }
  return pin;
}
