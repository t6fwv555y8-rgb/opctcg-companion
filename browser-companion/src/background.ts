/** Background service worker — keeps bridge connection lifecycle isolated from tabs. */
import { connectBridge, pingBridge } from "./bridge.js";

connectBridge();
setInterval(pingBridge, 15000);

chrome.runtime.onInstalled.addListener(() => {
  console.info("[optcg-companion] extension installed");
});
