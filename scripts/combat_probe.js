// Paste into the browser console (F12 > Console) on the OneSimulator game tab
// WHILE a battle is on screen (an attack declared, or you are asked to block or
// counter). It copies a compact description of the battle UI to the clipboard
// so it can be pasted back to fix the companion's combat detection. It reads
// the page only: no card ids of hidden cards, no account data.
(() => {
  const board = document.querySelector(".game-board-shell") || document.body;
  const short = (el) => ({
    tag: el.tagName.toLowerCase(),
    zone: el.getAttribute("data-card-zone"),
    player: el.getAttribute("data-card-player-id"),
    slot: el.getAttribute("data-card-slot"),
    img: (el.querySelector("img")?.getAttribute("src") || "").split("/").pop(),
    class: (el.getAttribute("class") || "").slice(0, 200),
  });
  const marked = [...board.querySelectorAll("[data-card-zone]")]
    .filter((el) => /ring-|border-yellow|border-red|animate-|scale-|outline/.test(el.className?.toString() || ""))
    .map(short);
  const banners = [...board.querySelectorAll("*")]
    .filter((el) => el.children.length < 6 && /battle|attack|counter|block|power|vs/i.test(el.textContent || "") && (el.textContent || "").length < 160)
    .slice(0, 25)
    .map((el) => ({ tag: el.tagName.toLowerCase(), class: (el.getAttribute("class") || "").slice(0, 160), text: el.textContent.trim().slice(0, 140) }));
  const out = JSON.stringify({ url: location.pathname, marked, banners, boardText: (board.textContent || "").replace(/\s+/g, " ").slice(0, 600) }, null, 1);
  navigator.clipboard.writeText(out).then(() => console.log("Copied " + out.length + " chars. Paste it into the chat."), () => console.log(out));
})();
