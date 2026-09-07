# Chrome extension

Load **this folder**. There is nothing to compile.

1. Chrome → `chrome://extensions`
2. Developer mode → **Load unpacked**
3. Pick this `browser-companion` folder
4. Keep `./start onesimulator` running
5. Open a match on https://onesimulator.slidingcodes.com

`background.js` only posts to `http://127.0.0.1:9003/snapshot`.
`content.js` only reads the board and shows a status label on the page.
