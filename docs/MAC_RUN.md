# Run on MacBook (HUD, not browser)

GitHub only stores the code. Opening the repo in a browser is **not** the app.

The real companion is a **native macOS window** titled **OPTCG Companion HUD**.  
If you only see `localhost:1420` in Chrome/Safari, you started the **frontend only**.

## 1. Install tools (once)

```bash
xcode-select --install
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
# restart Terminal, then:
rustc --version
node --version   # need 18+
```

## 2. Clone and install

```bash
cd ~/Desktop
git clone https://github.com/t6fwv555y8-rgb/opctcg-companion.git
cd opctcg-companion
git pull

npm run install:all
cd browser-companion && npm install && npm run build && cd ..
```

## 3. Start the real HUD (required command)

From the **repo root** (`opctcg-companion` folder that contains `Cargo.toml`):

```bash
cd ~/Desktop/opctcg-companion/src-ui
npm run tauri:dev
```

First run compiles Rust and can take **5–15 minutes**. Wait until you see a desktop window.

**Success looks like:** a normal macOS app window named **OPTCG Companion HUD** (Dock icon appears).

**Failure looks like:** only a browser tab at `http://localhost:1420` — that is Vite UI only, not the companion backend.

## 4. Smoke-test with mock data

In a **second** Terminal tab:

```bash
cd ~/Desktop/opctcg-companion
python3 scripts/mock_stream.py
```

In the HUD, select source **Mock** (or Auto). State should update.

## 5. OneSimulator (live)

1. Keep the HUD running (`tauri:dev`).
2. Chrome → `chrome://extensions` → Developer mode → **Load unpacked** → select folder:
   `~/Desktop/opctcg-companion/browser-companion`
3. Open https://onesimulator.slidingcodes.com and enter a match.
4. HUD source → **OneSimulator**.

## Commands that are NOT the full app

| Command | What it does |
|---------|----------------|
| `npm run dev:ui` | Browser-only Vite — **not** the HUD |
| `npm run build` | Builds JS assets only |
| Opening GitHub in Safari/Chrome | Source viewing only |

## If the window still does not appear

Paste the **full Terminal output** of:

```bash
cd ~/Desktop/opctcg-companion/src-ui
npm run tauri:dev
```

Common errors:
- `rustc` / `cargo` not found → install Rust, reopen Terminal
- Xcode / `SDK` errors → `xcode-select --install`
- `port 1420` in use → quit other Vite processes, retry
- Compile errors → send the red error block

## Confirm you are in the repo root

```bash
pwd
ls Cargo.toml src-ui src-tauri browser-companion
```

All four must exist.
