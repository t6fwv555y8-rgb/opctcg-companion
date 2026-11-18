/**
 * Plain-language glossary for the One Piece Card Game.
 *
 * Rules terms follow the official rules (en.onepiece-cardgame.com/rules) and
 * the card text OneSimulator shows. `slang` entries are community usage, not
 * official rules, and the UI labels them that way.
 */
export type GlossaryCategory = "basics" | "turn" | "battle" | "cardtext" | "slang";

export interface GlossaryEntry {
  key: string;
  term: string;
  category: GlossaryCategory;
  /** One plain sentence a new player can read at a glance. */
  short: string;
  /** Optional extra detail. */
  more?: string;
  /** Optional "what to do with this knowledge" line. */
  tip?: string;
  /** Literal spellings to underline in HUD text (matched case-insensitively). */
  aliases?: string[];
  /** Regex sources for spellings that vary, e.g. "DON!! -2". */
  patterns?: string[];
  /** Set false for everyday words that would be noise if underlined. */
  underline?: boolean;
}

export const CATEGORY_LABEL: Record<GlossaryCategory, string> = {
  basics: "The basics",
  turn: "Turn flow",
  battle: "Battle",
  cardtext: "Reading card text",
  slang: "Player slang",
};

export const GLOSSARY: GlossaryEntry[] = [
  // ---- The basics
  {
    key: "leader",
    term: "Leader",
    category: "basics",
    short:
      "Your main card. It sets your deck's colours and starting Life, and it can attack every turn.",
    more: "If your Leader is hit while you have 0 Life, you lose.",
    aliases: ["leader", "leaders"],
  },
  {
    key: "life",
    term: "Life",
    category: "basics",
    short:
      "Face-down cards that act as your health. When your Leader is hit, the top Life card goes to your hand.",
    more: "At 0 Life, the next hit on your Leader loses the game.",
    tip: "Life cards are also free cards: being hit early isn't always bad.",
    aliases: ["life", "life card", "life cards"],
  },
  {
    key: "don",
    term: "DON!!",
    category: "basics",
    short:
      "Your energy. Spend DON!! to play cards, or attach one to a Leader or Character for +1000 power on your turn.",
    more: "You get new DON!! every turn from your DON!! deck, and they refresh at the start of your turn.",
    aliases: ["DON!!", "DON!! card", "DON!! cards"],
  },
  {
    key: "don-deck",
    term: "DON!! deck",
    category: "basics",
    short: "A separate pile of 10 DON!! cards you take from each turn.",
    aliases: ["DON!! deck"],
  },
  {
    key: "cost-area",
    term: "Cost area",
    category: "basics",
    short:
      "Where your DON!! sit. Active DON!! can be spent; rested ones are already used this turn.",
    aliases: ["cost area"],
  },
  {
    key: "attached-don",
    term: "Attached DON!!",
    category: "basics",
    short: "A DON!! placed on a Leader or Character gives it +1000 power during your turn.",
    tip: "Attach DON!! before you attack to win a battle or to beat a Blocker.",
    aliases: ["attached DON!!", "attach DON!!", "attached"],
    underline: false,
  },
  {
    key: "character",
    term: "Character",
    category: "basics",
    short: "A card you play to the board. It can attack, block, and use its effects.",
    more: "You can control up to 5 Characters; playing a 6th means trashing one of yours.",
    aliases: ["character", "characters"],
  },
  {
    key: "event",
    term: "Event",
    category: "basics",
    short: "A one-shot card: play it, use its effect, and it goes to the trash.",
    more: "Some Events have a [Counter] effect you can use while defending.",
    aliases: ["event", "events"],
  },
  {
    key: "stage",
    term: "Stage",
    category: "basics",
    short: "A card that stays on the board and gives a lasting effect. You can have one at a time.",
    aliases: ["stage"],
  },
  {
    key: "trash",
    term: "Trash",
    category: "basics",
    short: "Your discard pile. K.O.'d Characters and used Events go here.",
    aliases: ["trash"],
  },
  {
    key: "ko",
    term: "K.O.",
    category: "basics",
    short:
      "A Character that loses a battle or is destroyed by an effect is K.O.'d and goes to the trash.",
    aliases: ["K.O.'d", "K.O.", "K.O"],
  },
  {
    key: "rested",
    term: "Rested / Active",
    category: "basics",
    short:
      "Active cards are upright and ready. Rested cards are turned sideways: already used this turn.",
    more: "Your rested cards and DON!! become active again at the start of your turn.",
    aliases: ["rested", "rest", "active"],
    underline: false,
  },
  {
    key: "power",
    term: "Power",
    category: "basics",
    short: "How strong a card is in battle. The higher power wins; a tie goes to the attacker.",
    aliases: ["power"],
    underline: false,
  },
  {
    key: "cost",
    term: "Cost",
    category: "basics",
    short: "How many DON!! you must spend to play a card.",
    aliases: ["cost"],
    underline: false,
  },
  {
    key: "board",
    term: "Board",
    category: "basics",
    short: "Everything each player has in play: Leader, Characters and Stage.",
    aliases: ["board"],
  },

  // ---- Turn flow
  {
    key: "refresh-phase",
    term: "Refresh Phase",
    category: "turn",
    short: "Start of your turn: your rested cards and DON!! become active again.",
    aliases: ["Refresh Phase", "Refresh"],
  },
  {
    key: "draw-phase",
    term: "Draw Phase",
    category: "turn",
    short: "Draw 1 card. The player going first skips this on turn 1.",
    aliases: ["Draw Phase"],
  },
  {
    key: "don-phase",
    term: "DON!! Phase",
    category: "turn",
    short: "Add 2 DON!! from your DON!! deck to your Cost area (only 1 on the first player's first turn).",
    aliases: ["DON!! Phase"],
  },
  {
    key: "main-phase",
    term: "Main Phase",
    category: "turn",
    short:
      "Your action phase: play cards, attach DON!!, use effects and attack, in any order.",
    aliases: ["Main Phase"],
  },
  {
    key: "end-phase",
    term: "End Phase",
    category: "turn",
    short: "Your turn ends. Effects that last \"this turn\" wear off.",
    aliases: ["End Phase"],
  },
  {
    key: "mulligan",
    term: "Mulligan",
    category: "turn",
    short:
      "Once per game, before play starts, you may shuffle your opening 5 cards back and draw 5 new ones.",
    tip: "Keep a hand with a couple of cheap Characters you can play on turns 1 and 2.",
    aliases: ["mulligan"],
  },
  {
    key: "first-player",
    term: "Going first",
    category: "turn",
    short:
      "The first player skips their first draw, gets only 1 DON!! on turn 1, and can't attack that turn.",
    aliases: ["going first", "first player"],
  },

  // ---- Battle
  {
    key: "attack",
    term: "Attack",
    category: "battle",
    short:
      "Rest an active Leader or Character to attack the opponent's Leader or one of their rested Characters.",
    aliases: ["attack", "attacks", "attacker"],
  },
  {
    key: "block-step",
    term: "Block Step",
    category: "battle",
    short: "The defender may rest a Blocker to take the hit instead of the original target.",
    aliases: ["Block Step"],
  },
  {
    key: "counter-step",
    term: "Counter Step",
    category: "battle",
    short:
      "The defender may trash Counter cards from hand, or play [Counter] Events, to add power to the card being attacked.",
    aliases: ["Counter Step"],
  },
  {
    key: "counter",
    term: "Counter",
    category: "battle",
    short:
      "A defence boost. Many cards show +1000 or +2000: during an attack you can trash that card from hand to add it to the defender's power.",
    more: "Some Events also have a [Counter] effect you can play instead.",
    tip: "Before you swing, count how many cards your opponent holds: each could add up to +2000.",
    aliases: ["counter", "counters", "[Counter]"],
  },
  {
    key: "damage-step",
    term: "Damage Step",
    category: "battle",
    short:
      "Powers are compared. The attacker wins on equal or higher power. A hit Leader loses a Life card; a hit Character is K.O.'d.",
    aliases: ["Damage Step"],
  },
  {
    key: "blocker",
    term: "Blocker",
    category: "battle",
    short:
      "[Blocker]: when your opponent attacks, you can rest this Character to make it the new target.",
    tip: "A Blocker protects your Life, but the Character may be K.O.'d, so block with cards you can afford to lose.",
    aliases: ["[Blocker]", "blocker", "blockers"],
  },
  {
    key: "rush",
    term: "Rush",
    category: "battle",
    short: "[Rush]: this Character can attack on the turn you play it.",
    aliases: ["[Rush]", "rush"],
  },
  {
    key: "rush-character",
    term: "Rush: Character",
    category: "battle",
    short: "[Rush: Character]: can attack Characters on the turn it is played, but not the Leader.",
    aliases: ["[Rush: Character]"],
  },
  {
    key: "double-attack",
    term: "Double Attack",
    category: "battle",
    short: "[Double Attack]: if this card damages a Leader, the Leader takes 2 Life damage instead of 1.",
    aliases: ["[Double Attack]", "double attack"],
  },
  {
    key: "banish",
    term: "Banish",
    category: "battle",
    short:
      "[Banish]: Life cards this card damages go to the trash instead of the hand, and their [Trigger] can't activate.",
    aliases: ["[Banish]", "banish"],
  },
  {
    key: "trigger",
    term: "Trigger",
    category: "battle",
    short:
      "[Trigger]: if this card is revealed from your Life by damage, you may use its effect for free instead of taking it into your hand.",
    aliases: ["[Trigger]", "trigger", "triggers"],
  },
  {
    key: "lethal",
    term: "Lethal",
    category: "battle",
    short: "Having enough damage to win the game this turn.",
    aliases: ["lethal"],
  },

  // ---- Reading card text
  {
    key: "on-play",
    term: "[On Play]",
    category: "cardtext",
    short: "Happens as soon as you play this card from your hand.",
    aliases: ["[On Play]"],
  },
  {
    key: "when-attacking",
    term: "[When Attacking]",
    category: "cardtext",
    short: "Happens each time this card attacks.",
    aliases: ["[When Attacking]"],
  },
  {
    key: "on-block",
    term: "[On Block]",
    category: "cardtext",
    short: "Happens when you use this card to block.",
    aliases: ["[On Block]"],
  },
  {
    key: "on-ko",
    term: "[On K.O.]",
    category: "cardtext",
    short: "Happens when this Character is K.O.'d.",
    aliases: ["[On K.O.]"],
  },
  {
    key: "activate-main",
    term: "[Activate: Main]",
    category: "cardtext",
    short: "An ability you choose to use during your Main Phase, often by resting this card.",
    aliases: ["[Activate: Main]"],
  },
  {
    key: "main",
    term: "[Main]",
    category: "cardtext",
    short: "An Event you play during your own Main Phase.",
    aliases: ["[Main]"],
  },
  {
    key: "your-turn",
    term: "[Your Turn] / [Opponent's Turn]",
    category: "cardtext",
    short: "The effect only works during that player's turn.",
    aliases: ["[Your Turn]", "[Opponent's Turn]", "[Opponent’s Turn]"],
  },
  {
    key: "end-of-turn",
    term: "[End of Your Turn]",
    category: "cardtext",
    short: "Happens when your turn ends.",
    aliases: ["[End of Your Turn]"],
  },
  {
    key: "once-per-turn",
    term: "[Once Per Turn]",
    category: "cardtext",
    short: "This effect can be used only one time each turn.",
    aliases: ["[Once Per Turn]", "once per turn"],
  },
  {
    key: "don-x",
    term: "[DON!! x2]",
    category: "cardtext",
    short:
      "The effect only works while this card has at least that many DON!! attached (x1, x2 and so on).",
    patterns: ["\\[DON!! x\\d\\]"],
  },
  {
    key: "don-minus",
    term: "DON!! -2",
    category: "cardtext",
    short:
      "A cost: return that many DON!! from your field to your DON!! deck. It lowers your resources, so it is a real price.",
    patterns: ["DON!! [\\-\\u2212]\\d+"],
  },
  {
    key: "up-to",
    term: "up to",
    category: "cardtext",
    short: "You choose any number from 0 up to the amount shown. You can pick fewer, or none.",
    aliases: ["up to"],
    underline: false,
  },

  // ---- Player slang (community usage, not official rules)
  {
    key: "tempo",
    term: "Tempo",
    category: "slang",
    short: "Who has the momentum: usually the player with more on the board putting pressure on the other.",
    aliases: ["tempo"],
  },
  {
    key: "curve",
    term: "Curve",
    category: "slang",
    short: "Using all your DON!! each turn to build the board. \"Playing on curve\" means not wasting any.",
    aliases: ["on curve", "curve"],
  },
  {
    key: "swing",
    term: "Swing",
    category: "slang",
    short: "To attack. \"Swing at the Leader\" means attack the Leader.",
    aliases: ["swing", "swings"],
  },
  {
    key: "going-wide",
    term: "Going wide",
    category: "slang",
    short: "Playing many small Characters so you have lots of attacks and blockers.",
    aliases: ["going wide"],
  },
  {
    key: "bounce",
    term: "Bounce",
    category: "slang",
    short: "Returning a card from the board to its owner's hand. Blue decks do this a lot.",
    aliases: ["bounce"],
  },
  {
    key: "ramp",
    term: "Ramp",
    category: "slang",
    short: "Getting extra DON!! or resources faster than normal to play big cards early.",
    aliases: ["ramp"],
  },
  {
    key: "otk",
    term: "OTK",
    category: "slang",
    short: "\"One-turn kill\": winning the game in a single turn from a position that looked safe.",
    aliases: ["OTK"],
  },
  {
    key: "trade",
    term: "Trade",
    category: "slang",
    short: "Both sides lose a similar card, for example two Characters K.O. each other.",
    aliases: ["trade", "trades"],
  },
  {
    key: "chump",
    term: "Chump block",
    category: "slang",
    short: "Blocking with a card you expect to lose, just to save your Life.",
    aliases: ["chump block", "chump"],
  },
];

/** Entries in the order the Learn panel lists them. */
export const CATEGORY_ORDER: GlossaryCategory[] = [
  "basics",
  "turn",
  "battle",
  "cardtext",
  "slang",
];

export function entryByKey(key: string): GlossaryEntry | undefined {
  return GLOSSARY.find((entry) => entry.key === key);
}

function escapeRegex(text: string): string {
  return text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

interface Matcher {
  regex: RegExp;
  lookup: Map<string, GlossaryEntry>;
  patternEntries: { regex: RegExp; entry: GlossaryEntry }[];
}

let cached: Matcher | null = null;

/** One regex over every underlinable spelling, longest spellings first. */
export function matcher(): Matcher {
  if (cached) return cached;
  const lookup = new Map<string, GlossaryEntry>();
  const parts: { source: string; length: number }[] = [];
  const patternEntries: { regex: RegExp; entry: GlossaryEntry }[] = [];

  for (const entry of GLOSSARY) {
    if (entry.underline === false) continue;
    for (const alias of entry.aliases ?? []) {
      lookup.set(alias.toLowerCase(), entry);
      const start = /^\w/.test(alias) ? "\\b" : "";
      const end = /\w$/.test(alias) ? "\\b" : "";
      parts.push({ source: `${start}${escapeRegex(alias)}${end}`, length: alias.length });
    }
    for (const source of entry.patterns ?? []) {
      patternEntries.push({ regex: new RegExp(`^${source}$`, "i"), entry });
      parts.push({ source, length: 20 });
    }
  }

  parts.sort((a, b) => b.length - a.length);
  cached = {
    regex: new RegExp(parts.map((p) => p.source).join("|"), "gi"),
    lookup,
    patternEntries,
  };
  return cached;
}

/** Find the entry for a matched piece of text. */
export function resolveMatch(text: string): GlossaryEntry | undefined {
  const m = matcher();
  return (
    m.lookup.get(text.toLowerCase()) ??
    m.patternEntries.find((p) => p.regex.test(text))?.entry
  );
}

/** Entries matching a search box query, across term, spellings and meaning. */
export function searchGlossary(query: string): GlossaryEntry[] {
  const q = query.trim().toLowerCase();
  if (!q) return GLOSSARY;
  return GLOSSARY.filter((entry) =>
    [entry.term, entry.short, entry.more ?? "", ...(entry.aliases ?? [])]
      .join(" ")
      .toLowerCase()
      .includes(q),
  );
}
