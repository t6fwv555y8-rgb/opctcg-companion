// Forwards game snapshots from the OneSimulator tab to the local HUD.
// Nothing else lives here — no WebSocket and no reconnect loop.
// If Chrome shows dist/bridge.js or ws://127.0.0.1:9003, you loaded the old
// extension. Remove it and Load unpacked → this folder (not dist).

const HUD = "http://127.0.0.1:9003/snapshot";

function badge(text, color) {
  if (!chrome.action) return;
  void chrome.action.setBadgeText({ text });
  void chrome.action.setBadgeBackgroundColor({ color });
}

chrome.runtime.onMessage.addListener((message, _sender, reply) => {
  if (message?.type !== "snapshot") return false;

  fetch(HUD, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(message.snapshot),
  })
    .then((res) => {
      const ok = res.ok;
      badge(ok ? "ON" : "!", ok ? "#22c55e" : "#ef4444");
      reply({ ok });
    })
    .catch(() => {
      badge("!", "#ef4444");
      reply({ ok: false, error: "hud" });
    });

  return true;
});
