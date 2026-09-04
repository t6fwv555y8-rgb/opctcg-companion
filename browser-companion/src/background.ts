/** Background service worker — keeps bridge connection lifecycle isolated from tabs. */
import { connectBridge, pingBridge } from "./bridge.js";

connectBridge();
setInterval(pingBridge, 15000);

chrome.runtime.onInstalled.addListener(() => {
  console.info("[optcg-companion] extension installed");
});

chrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
  if (message?.type === "badge") {
    const action = chrome.action;
    if (action) {
      void action.setBadgeText({ text: String(message.text ?? "") });
      if (message.color) {
        void action.setBadgeBackgroundColor({ color: message.color });
      }
      if (message.title) {
        void action.setTitle({ title: String(message.title) });
      }
    }
    sendResponse({ ok: true });
    return true;
  }
  return false;
});
