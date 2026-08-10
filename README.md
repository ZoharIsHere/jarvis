# J.A.R.V.I.S.

A personal life-ops HUD, built as a Tauri desktop app (Rust + TypeScript)
around a single local SQLite database. It tracks energy/recovery data,
deadlines, a daily plan, and habits, and renders them into a heads-up
dashboard styled after a fictional AI assistant.

**Live HUD demo:** https://zoharishere.github.io/jarvis/

![JARVIS dashboard screenshot](docs/screenshot.png)

The demo above is `hud/index.html` running standalone against mock data.
The Tauri desktop app now loads this same file directly (see [Known
limitations](#known-limitations--in-progress) for what's still mock vs.
live).

## Architecture: the spine

Everything in this project is organized around one idea: **subsystems
never talk to each other directly.** Instead, every collector, planner,
and UI panel reads and writes a single SQLite database — the "spine"
(`jarvis.db`) — and nothing else.

```
Garmin collector ─┐
(external script)  │
                    ├──▶  jarvis.db (spine)  ◀──▶  Tauri app / HUD
Future collectors ─┘         (SQLite)
(calendar, phone usage, …)
```

Concretely:

- The schema lives in [`src-tauri/migrations/`](src-tauri/migrations)
  and is applied by the `tauri-plugin-sql` migration runner in
  [`src-tauri/src/lib.rs`](src-tauri/src/lib.rs) — but only once
  something on the frontend actually opens the database via the plugin's
  JS API. `hud/index.html` doesn't do that yet (it's still mock data
  throughout), so in practice migrations only run when a frontend build
  that calls `Database.load()` is in place.
- Tables include `energy_state` / `energy_forecast` / `energy_log` (body
  battery, sleep, stress, and a rule-based hourly energy curve),
  `deadlines` and `tasks` (the planner's input pool), `plan_blocks`
  (today's schedule), `interventions` (anti-burnout history),
  `checkins`, `app_usage`, `settings` (tunable thresholds instead of
  hardcoded constants), and `habits` / `habit_log` (habit definitions and
  daily check-ins — streaks are computed from the log, not stored).
- [`garmin_collector.py`](garmin_collector.py) is a standalone Python
  script, run on a schedule (e.g. via `launchd`), that pulls body
  battery / stress / sleep from Garmin Connect, computes a rule-based
  24-hour energy forecast, and writes the result straight into the
  spine. **The Tauri app never talks to Garmin, and the collector never
  talks to the app** — the database is the entire interface between them.

The intent is that any future subsystem (calendar sync, phone usage
tracking, a real Notion sync instead of mock data) plugs in the same
way: write to the spine, let the HUD read from it. No subsystem needs
to know any other subsystem exists.

## Project layout

```
hud/index.html          The HUD (six pages, mock data) — deployed to GitHub Pages,
                         and what the Tauri app's window now loads directly
garmin_collector.py      Garmin → spine collector (run separately, see below)
src/                     Unused Vite/Tauri starter frontend, kept for reference
src-tauri/               Tauri app backend (Rust)
  src/lib.rs             App entrypoint, SQL migration wiring
  migrations/            SQLite schema migrations (001_spine.sql, 002_habits.sql)
```

## Running it

### The HUD (what's deployed to GitHub Pages)

`hud/index.html` is fully standalone and runs against hardcoded mock
data — no build step, no backend. Just open it in a browser:

```bash
open hud/index.html
```

### The Garmin collector

```bash
pip install garminconnect
export GARMIN_EMAIL=you@example.com
export GARMIN_PASSWORD=your-password
python3 garmin_collector.py
```

First run logs in and saves a token to `~/.garminconnect` so later runs
don't need the password again. It writes into
`~/Library/Application Support/com.hila.jarvis/jarvis.db` by default
(override with `JARVIS_DB`), and expects that database to already exist
(i.e. the Tauri app has been launched at least once so migrations ran).

### The Tauri app

Opens a native window loading `hud/index.html` directly — no Vite build
in the loop. On Intel Macs with only the Xcode Command Line Tools
installed (no full Xcode), the SDK path needs to be set explicitly
before `tauri dev`:

```bash
export SDKROOT=$(xcrun --show-sdk-path)
npm run tauri dev
```

This is enough for the UI, the spine data, and habit tracking. **It is
not enough for voice** — `tauri dev` runs the raw unbundled binary,
which macOS won't grant microphone access to. For voice, build and run
the actual `.app`:

```bash
export SDKROOT=$(xcrun --show-sdk-path)
npm run tauri build -- --debug
codesign --force --deep --sign - --entitlements src-tauri/Entitlements.plist \
  src-tauri/target/debug/bundle/macos/jarvis.app
export ANTHROPIC_API_KEY=sk-ant-...   # optional — see Voice, below
./src-tauri/target/debug/bundle/macos/jarvis.app/Contents/MacOS/jarvis
```

(Launch the binary inside the bundle directly, not `open` — `open`
won't inherit `ANTHROPIC_API_KEY` from the shell. Ad-hoc signing is
already set in `tauri.conf.json`, so a plain `tauri build` should
sign automatically; the manual `codesign` line above is a fallback if
it doesn't. Every rebuild changes the app's signature, which
invalidates the previous mic permission grant — macOS will re-prompt
after each build. That's expected.)

## Voice

Press the center dial (or the mic button) and talk. Speech-to-text is
fully native and free — a Swift helper
([`src-tauri/speech/JarvisListen.swift`](src-tauri/speech/JarvisListen.swift))
using AVAudioEngine + SFSpeechRecognizer, preferring on-device
recognition when macOS supports it. **This exists because the obvious
approach — the browser's `webkitSpeechRecognition` — does not work
inside a Tauri app on macOS.** WKWebView never forwards the permission
request to macOS's TCC system, so it always fails with `not-allowed`
and the app never even appears in System Settings → Privacy →
Microphone. This is an open upstream bug
([tauri-apps/wry#1195](https://github.com/tauri-apps/wry/issues/1195))
— the wry maintainers have tried private Apple APIs and haven't
resolved it. Capturing audio outside the webview is the actual fix,
not a workaround.

Nav commands ("open focus", "open habits", …) and "remember that
\<fact\>" (writes straight to `MEMORY.md`) work with zero API cost.
Open-ended conversation calls Claude and needs `ANTHROPIC_API_KEY`
(from [console.anthropic.com](https://console.anthropic.com), billed
separately from any Claude.ai subscription — pay-as-you-go). Without
it set, conversational replies fail with a clear on-screen error;
everything else still works.

Replies are spoken with macOS's built-in `say -v Daniel` rather than
the browser's speech synthesis, which is unreliable once packaged.

A **WAKE ON** button enables hands-free "Jarvis, …" listening. It's a
hard mute — off actually kills the listener process, not just ignores
its output.

See [`JARVIS_MASTER.md`](JARVIS_MASTER.md) §9 for the full voice
architecture and everything above in more detail.

## Known limitations / in progress

This is an active work-in-progress personal project, not a finished
product. Specifically, right now:

- **Only Dashboard and Habits are live.** Inside the Tauri app,
  Dashboard's vitals/energy/ceiling and the Habits page read and write
  the real spine DB (`hud/index.html`'s `window.__TAURI__` gate — see
  the bottom of the `<script>` block). Focus, Planner, Projects, and
  Comms are still hardcoded JS data, and stay that way even in the
  native app. The GitHub Pages demo is unaffected either way — it has
  no Tauri backend, so it's 100% mock by design.
- **`tauri-plugin-sql` only runs pending migrations when something
  calls `Database.load()` from the frontend.** `002_habits.sql` was
  applied directly to the local spine DB by hand the first time to
  unblock testing (it's idempotent, so re-running it via the plugin
  later is safe) — but a fresh clone of this repo needs the app
  launched once against a real `Database.load()` call for migrations
  to actually apply.
- **The demo state switcher (Blue/Red/Gray) is browser-only now** —
  it writes mock vitals straight over live DB values, so it's hidden
  whenever the app is running against the real spine.
- **The Garmin collector is ~90% done.** It authenticates, pulls body
  battery / stress / sleep, computes an energy forecast, and writes to
  the spine — but the last real attempt got rate-limited by Garmin
  before a token could be cached, and the Garmin password from that
  attempt is being rotated (see [Security note](#security-note)). It
  needs one successful manual run with the new password, once Garmin's
  rate limit clears, before it can go back on a schedule.
- **Conversation with Jarvis needs `ANTHROPIC_API_KEY`**, deliberately
  not set up yet (separate cost from any Claude.ai subscription). Nav
  commands and "remember that…" work without it. See
  [Voice](#voice), above.
- **No Notion integration**, despite the HUD referencing it — mocked
  for now.
- **No tool-calling yet** — Jarvis can talk about your day but can't
  change anything in it (add a task, mark a habit done by voice
  outside the Habits page, adjust a deadline). This is the single
  biggest thing that would make it feel like an actual assistant
  rather than a chatbot with a nice HUD; see `JARVIS_MASTER.md` §9's
  closing notes.

## Security note

This repository (working tree and git history) was scanned before its
first commit and contains no credentials. A Garmin account password and
a Notion integration token were exposed in a terminal/chat session
during development, outside of this repo — those are being rotated
separately and were never committed here.
