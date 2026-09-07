# Open the app

GitHub is only the source. The app is a desktop window named **OPTCG Companion**.

## First time (once)

1. Install Xcode tools, Rust, and Node — paste these, one at a time:

```bash
xcode-select --install
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Close Terminal and open a new one, then:

```bash
brew install node
```

Skip `brew` if `node -v` already prints 18 or higher.

2. Get the code (skip if you already cloned it):

```bash
cd ~/Desktop
git clone https://github.com/t6fwv555y8-rgb/opctcg-companion.git
```

## Every time you want to use it

```bash
cd ~/Desktop/opctcg-companion
./start
```

Wait for a window titled **OPTCG Companion**. The first launch compiles Rust and can take several minutes. Leave that Terminal open. Demo cards will move on their own.

`Ctrl-C` in that Terminal stops the app.

## What you should see

- A small dark window, not a browser tab
- **Mock Game · LIVE** at the top
- The **Play** tab updates as the board does: what to do now, then a line after each play settles
- Decks, Scouting, Matchup, and Coach panels filling in as the demo plays

A tab at `localhost:1420` means the window never opened — quit that and run `./start` again.

## Play a OneSimulator match

```bash
cd ~/Desktop/opctcg-companion
./start onesimulator
```

Leave that Terminal open. Then **once**, in **Chrome** (not Safari):

1. Go to `chrome://extensions`
2. Turn **Developer mode** on
3. Remove any old **OPTCG Companion Bridge**
4. **Load unpacked** → `Desktop/opctcg-companion/browser-companion`
5. Open https://onesimulator.slidingcodes.com and enter a match

A label in the bottom-right of the game tells you what is happening:

- **Companion is reading this match** — working
- **Start the app first** — the HUD window is not running
- **enter a match** — you are still on the lobby

If the extension was already loaded, click reload, then refresh the OneSimulator tab.

Keep the HUD on **Play**. Life, phase, and the next move update as the extension reads the board. After a play settles, a short strategy line lands under **As you go**.

## No window, only a terminal

```bash
./start --terminal
```

Same panels, printed in the shell. In a second Terminal: `python3 scripts/mock_stream.py`
