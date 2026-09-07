# Chrome extension

Load **this folder**. There is nothing to compile. Do not load `dist`.

1. Chrome → `chrome://extensions`
2. Developer mode → **Load unpacked**
3. Pick this `browser-companion` folder (the one with `background.js` next to `manifest.json`)
4. Keep `./start onesimulator` running
5. Open a match on https://onesimulator.slidingcodes.com

The extension details should say **OPTCG Companion** version **0.2.1**.
`background.js` only posts to `http://127.0.0.1:9003/snapshot`.

If Chrome shows `dist/bridge.js` or `ws://127.0.0.1:9003`, that is the old worker. Remove the extension and load this folder again.
