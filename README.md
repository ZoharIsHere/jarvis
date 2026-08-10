# J.A.R.V.I.S.

A personal life-ops HUD, built as a Tauri desktop app (Rust + TypeScript)
around a single local SQLite database. It tracks energy/recovery data,
deadlines, a daily plan, and habits, and renders them into a heads-up
dashboard styled after a fictional AI assistant.

**Live HUD demo:** https://zoharishere.github.io/jarvis/

![JARVIS dashboard screenshot](docs/screenshot.png)

The demo above is `hud/index.html` running standalone against mock data —
it's the same file the Tauri app is meant to load, but the app itself
doesn't point at it yet (see [Known limitations](#known-limitations--in-progress)).

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

- The schema lives in [`src-tauri/migrations/001_spine.sql`](src-tauri/migrations/001_spine.sql)
  and is applied automatically by the `tauri-plugin-sql` migration runner
  in [`src-tauri/src/lib.rs`](src-tauri/src/lib.rs).
- Tables include `energy_state` / `energy_forecast` / `energy_log` (body
  battery, sleep, stress, and a rule-based hourly energy curve),
  `deadlines` and `tasks` (the planner's input pool), `plan_blocks`
  (today's schedule), `interventions` (anti-burnout history),
  `checkins`, `app_usage`, and `settings` (tunable thresholds instead of
  hardcoded constants).
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
hud/index.html          Standalone HUD demo (six pages, mock data) — deployed to GitHub Pages
garmin_collector.py      Garmin → spine collector (run separately, see below)
src/                     Tauri app frontend (currently default Vite/Tauri boilerplate)
src-tauri/               Tauri app backend (Rust)
  src/lib.rs             App entrypoint, SQL migration wiring
  migrations/            SQLite schema migrations (only 001_spine.sql exists so far)
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

On Intel Macs with only the Xcode Command Line Tools installed (no full
Xcode), the SDK path needs to be set explicitly before `tauri dev`:

```bash
npm install
export SDKROOT=$(xcrun --show-sdk-path)
npm run tauri dev
```

## Known limitations / in progress

This is an active work-in-progress personal project, not a finished
product. Specifically, right now:

- **The Tauri app doesn't load the HUD yet.** `hud/index.html` is a
  standalone file; the actual app window still renders the default
  Tauri/Vite starter page (`index.html` / `src/main.ts`). Wiring the
  app's `frontendDist` to the HUD is the next step.
- **The Garmin collector is ~90% done.** It authenticates, pulls body
  battery / stress / sleep, computes an energy forecast, and writes to
  the spine — but it's currently paused on saving the auth token after
  hitting Garmin's rate limit, so it hasn't been run end-to-end yet.
- **Habits has no backing table.** The HUD's "Habits" page is UI/mock
  data only. A `002_habits.sql` migration doesn't exist yet and isn't
  registered in `src-tauri/src/lib.rs` — habit tracking isn't wired
  into the spine at all so far.
- **Only one migration exists.** `energy_*`, `deadlines`, `tasks`,
  `plan_blocks`, `interventions`, `checkins`, `app_usage`, and
  `settings` tables are defined; everything else the HUD displays
  (e.g. habits, some Comms/Projects content) is still mock data with no
  live source.
- **No Notion integration yet**, despite the HUD referencing it —
  that's mocked for now too.

## Security note

This repository (working tree and git history) was scanned before its
first commit and contains no credentials. A Garmin account password and
a Notion integration token were exposed in a terminal/chat session
during development, outside of this repo — those are being rotated
separately and were never committed here.
