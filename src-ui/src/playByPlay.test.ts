import { nextCoachPin, playByPlayLine } from "./playByPlay";

function assert(cond: unknown, message: string): asserts cond {
  if (!cond) throw new Error(message);
}

assert(
  playByPlayLine("Rayleigh's swinging at their leader.", "Main phase — develop.") ===
    "Rayleigh's swinging at their leader.",
  "a finished coach line must stay on screen",
);
assert(
  playByPlayLine(null, "Main phase — develop.") === "Main phase — develop.",
  "with no coach line, fall back to the table",
);
assert(playByPlayLine("   ", "waiting") === "waiting", "whitespace is not a line");

const first = nextCoachPin(null, "game-1", "Attach DON to Rayleigh.");
assert(first.text === "Attach DON to Rayleigh.", "first mount keeps the live read");

const held = nextCoachPin(first, "game-1", "Attach DON to Rayleigh.");
assert(held.text === first.text, "the same game keeps the pin after the turn ends");

const nextGame = nextCoachPin(held, "game-2", "Attach DON to Rayleigh.");
assert(nextGame.text === "", "a new match must not reuse the last game's line");

const fresh = nextCoachPin(nextGame, "game-2", "Their Luffy is open. Swing.");
assert(fresh.text === "Their Luffy is open. Swing.", "a new read replaces the pin");

console.log("playByPlay ok");
