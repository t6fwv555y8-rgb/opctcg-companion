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
- Decks, Scouting, Matchup, and Coach panels filling in as the demo plays

A tab at `localhost:1420` means the window never opened — quit that and run `./start` again.

## Play a real match

1. Keep `./start` running.
2. In the window, scroll to **Game Source** and click **OneSimulator**.
3. Chrome → `chrome://extensions` → Developer mode → **Load unpacked** → pick the `browser-companion` folder inside this repo.
4. Open a match on OneSimulator. The HUD follows the game.

## No window, only a terminal

```bash
./start --terminal
```

Same panels, printed in the shell. In a second Terminal: `python3 scripts/mock_stream.py`
