# J.A.R.V.I.S. — Master Architecture & Context Doc

> **Purpose of this file:** Send this to Claude at the start of any new chat to restore full context on the JARVIS project. It contains the vision, architecture, per-feature plans, sub-projects, tech decisions, and a live progress tracker. Claude updates the **Progress Tracker** and **Changelog** sections as work gets done.

> **Owner:** Zohar. **Tone:** Jarvis — formal, precise, strategic, dry wit.
> **Last updated:** 2026-08-10 (v0.2 — Phase 0 done, voice loop live)

---

## 0. What this is

Not a dashboard. LifeOS *shows* state; JARVIS *changes* state. This is an always-on personal assistant that runs Zohar's life admin, protects his energy, and acts on his behalf — controlled by a Jarvis-style AI controller with specialized sub-workers.

### Locked decisions (from spec)
- **Runs on:** Desktop app for **PC + Mac** (always-on background process). Plus a **phone layer** ("phone OS") that blocks scrolling, tracks app time, and exposes JARVIS on mobile.
- **Voice:** Jarvis movie-style voice (realistic, British, calm). Needs a real TTS engine, not browser default.
- **Wake:** Always-on desktop process listening for **two claps** OR an alarm-clock time trigger → plays intro of *Highway to Hell* → reads today's stats + tasks aloud.
- **AI controller:** **Jarvis** = system controller / router. Sub-workers = **Julie** (anti-burnout), **James**, **Sumie** (decision synthesis). Use the AI brain ONLY where reasoning is needed — most plumbing is plain code to save tokens/cost.
- **Aesthetic:** Iron Man 1 Stark workshop HUD. Deep black, cyan wireframes, arc-reactor blue glow, translucent floating panels, thin linework. Same theme across desktop + phone.

### The four systems requested (all interconnected)
1. **Anti-Burnout System** — guided by Julie, behaves like the Mark I fire-extinguisher: watches for the "fire" (overload) and auto-suppresses it.
2. **Adaptive Energy Model** — learns Zohar's real rhythm and feeds every other system.
3. **Deadline Radar** — pulls every deadline, warns before they bite.
4. **Day Planner** — turns raw ideas + tasks into hour-by-hour day plans.

Plus the cross-cutting: **Wake-Up System**, **Jarvis Controller + board sub-workers**, **Custom HUD UI**, **Phone layer**.

---

## 1. ARCHITECTURE OVERVIEW

```
                    ┌─────────────────────────────┐
                    │   JARVIS CONTROLLER (AI)     │  ← routes, reasons, speaks
                    │   Jarvis = router/controller │
                    │   Julie / James / Sumie subs │
                    └──────────────┬──────────────┘
                                   │ (event bus / shared state)
        ┌──────────────┬──────────┼──────────┬──────────────┐
        ▼              ▼          ▼          ▼              ▼
  ┌──────────┐  ┌──────────┐ ┌────────┐ ┌──────────┐ ┌──────────┐
  │  Energy  │  │ Deadline │ │  Day   │ │  Anti-   │ │  Wake-Up │
  │  Model   │→ │  Radar   │→│Planner │←│ Burnout  │ │  System  │
  └────┬─────┘  └────┬─────┘ └───┬────┘ └────┬─────┘ └────┬─────┘
       │             │           │           │            │
       └─────────────┴─────┬─────┴───────────┴────────────┘
                           ▼
                 ┌───────────────────┐
                 │   SHARED STATE     │  ← single source of truth (local DB)
                 │  (the data spine)  │     every system reads/writes here
                 └─────────┬─────────┘
                           │
       ┌───────────────────┼────────────────────┐
       ▼                   ▼                    ▼
  ┌─────────┐        ┌──────────┐         ┌──────────┐
  │ Desktop │        │  Phone   │         │Integrations│
  │ HUD +   │        │  layer   │         │ GCal/Notion│
  │ daemon  │        │          │         │ Garmin/... │
  └─────────┘        └──────────┘         └──────────┘
```

### Key architectural principle: the Shared State spine
The four systems don't call each other directly. They all read/write **one local database** ("the spine"). This is how they "transfer data to each other" cleanly:
- Energy Model writes `energy_forecast` → Day Planner reads it.
- Deadline Radar writes `deadlines[]` → Day Planner + Anti-Burnout read them.
- Day Planner writes `today_plan` → Wake-Up reads it aloud, Anti-Burnout monitors load.
- Anti-Burnout writes `load_level` + `interventions[]` → Day Planner re-plans, Energy Model logs.

**Why this matters:** you can build/test each system alone, they stay decoupled, and adding a 5th system later is trivial.

### Where the "AI brain" is actually needed (token discipline)
| Needs AI (reasoning/language) | Plain code (no AI) |
|---|---|
| Turning messy ideas → structured day plan | Detecting two claps |
| Julie deciding *which* intervention fits the moment | Counting app-time minutes |
| Reading stats aloud in natural Jarvis phrasing | Pulling GCal events via API |
| Sumie synthesizing a board decision | Playing Highway to Hell |
| Parsing a free-text "brain dump" into tasks | Computing "hours until deadline" |
| Rebalancing a plan after a burnout flag | Storing/retrieving from the spine DB |

Rule: **AI only touches language and judgment. Everything mechanical is code.**

---

## 2. TECH STACK (proposed)

- **Desktop app:** Electron or Tauri. → **Tauri** recommended (Rust core, tiny footprint, real always-on daemon, native audio, runs PC + Mac from one codebase). Electron is the fallback if we want faster JS-only dev.
- **Shared State spine:** SQLite (local, fast, one file, works offline). Synced to phone.
- **UI:** React + custom HUD component library (the cyan/glass Stark theme). Reused across desktop + phone webview.
- **Voice (Jarvis):** ElevenLabs API (best match for movie-Jarvis British voice) → TTS. STT (if we want voice commands later) via Whisper.
- **Clap detection:** Rust/Python audio process, amplitude-spike + double-peak detection. No AI needed.
- **AI controller:** Claude API (Sonnet for routing/most tasks, escalate when needed). Sub-workers = system prompts + routing, not separate models.
- **Phone layer:** Android first (allows real app-blocking + usage stats via Accessibility + UsageStatsManager APIs). iOS is heavily sandboxed — can do Screen Time API + Shortcuts, but true scroll-blocking is limited. **Flag:** iOS will be a weaker version of the phone layer than Android.
- **Integrations:** GCal, Notion, Garmin (python-garminconnect), Val.town relay — all already partly wired in your ecosystem.

---

## 3. PER-FEATURE PLANS

Each feature is analyzed as requested:
**(1)** what it needs to work in real life · **(2)** how it integrates · **(3)** sub-projects.

---

### 3.1 ADAPTIVE ENERGY MODEL
*This is the foundation — build first. Every other system reads its output.*

**(1) What it needs to work in real life**
- Learn Zohar's real rhythm, not an ideal one: sleep drifts to ~3am, target ~2am/9:30am, night-owl peak, sustainable ceiling ~5h study/day.
- Model energy as a **curve across the day**, not a single number. E.g. low morning → rising afternoon → peak evening/night.
- Inputs it must ingest: sleep (from Garmin/phone), time already spent studying today, training load (runs), time-of-day, and self-reported check-ins ("how's your fuel?" 1 tap).
- Must degrade gracefully: if no Garmin data one day, fall back to defaults + yesterday.
- **Annoying if:** it nags for check-ins constantly, or overrides how he actually feels. Fix: max 2 check-ins/day, and manual override always wins.
- Must output something the Day Planner can consume: an hourly `energy_forecast` (0–100) for today.

**(2) How it integrates**
- Writes `energy_forecast[]` (hourly) + `ceiling_remaining` (study hours left today) to the spine.
- Day Planner reads the forecast → schedules hard work at peak, admin at troughs.
- Anti-Burnout reads `ceiling_remaining` → if near 0 and more work scheduled, that's a fire.
- Learns over time: logs predicted-vs-actual energy, adjusts the personal curve weekly.

**(3) Sub-projects**
- **3.1a — Data ingest adapters:** Garmin pull (sleep, HR, training load), phone sleep/wake, run logs. *(plain code)*
- **3.1b — Energy curve engine:** the model that turns inputs → hourly forecast. Start rule-based (no AI), upgrade to learned weights later. *(plain code + light stats)*
- **3.1c — Check-in mechanism:** minimal 1-tap "fuel level" prompt, rate-limited. *(UI + code)*

---

### 3.2 DEADLINE RADAR

**(1) What it needs to work in real life**
- Pull every deadline that can bite: exams, homework due dates, registration windows, admin (token rotation, emails owed).
- Sources: GCal, Notion, Bar-Ilan portal, manual add. Course exam schedule is known (Probability, Infi 1/2, Intro CS, Yahadut).
- Show **time-to-impact**, not just a date. "Intro CS exam in 5 days" beats "Aug 2".
- Escalating urgency: gentle → firm → alarm as it approaches, tuned to task size.
- **Annoying if:** it warns about everything equally, or double-counts across sources. Fix: dedupe by title+date, severity tiers, snooze with reason.
- Must feed the planner so deadlines actually get *scheduled*, not just displayed.

**(2) How it integrates**
- Writes `deadlines[]` (title, due, source, size estimate, severity) to the spine.
- Day Planner reads them → back-plans work sessions before each due date.
- Anti-Burnout reads them → detects "too many deadlines colliding" as a fire condition.
- Wake-Up reads the top 3 → reads them aloud in the morning.

**(3) Sub-projects**
- **3.2a — Source connectors:** GCal, Notion, portal scraper/manual, each normalizing to one deadline schema. *(plain code)*
- **3.2b — Dedupe + severity engine:** merge sources, assign urgency by size × time-left. *(plain code)*
- **3.2c — Back-planning hook:** emits "needs N sessions before date X" for the planner. *(plain code)*

---

### 3.3 DAY PLANNER
*The system that turns ideas + tasks → hour-by-hour plans. This is where AI earns its keep.*

**(1) What it needs to work in real life**
- Input: a messy brain-dump of ideas + tasks (free text) AND the structured tasks/deadlines already in the spine.
- Output: a full **hour-by-hour** day plan (Zohar wants full days, not floating blocks), Pomodoro-friendly, varied/irregular across days (ADHD engagement), ~30% buffer built in.
- Hard constraints it must respect: Friday family dinner ~19:45–21:15 (never scheduled over), the Infi-1-before-Infi-2 sequencing rule, the 5h study ceiling, venue selection by focus/energy (father's office, library, home, Bar-Ilan libraries).
- Must place hard work at energy peaks (reads Energy Model) and back-plan deadlines (reads Radar).
- Must include the **ignition helpers**: each task's first move is "stupidly small," phone-away built into focus blocks.
- **Annoying if:** it makes rigid identical plans, ignores how the day actually went, or over-schedules. Fix: variety enforced, buffer built in, re-plans on the fly when Anti-Burnout or a slipped block fires.

**(2) How it integrates**
- Reads: `energy_forecast`, `deadlines`, `ceiling_remaining`, task list, fixed calendar events.
- Writes: `today_plan` (blocks with time, task, venue, first-move) to the spine.
- Wake-Up reads `today_plan` aloud. Anti-Burnout monitors it for overload. Phone layer enforces phone-away during focus blocks.
- Re-plan trigger: when a block is missed or Julie fires an intervention, planner regenerates the *rest* of the day only.

**(3) Sub-projects**
- **3.3a — Brain-dump parser:** free text → structured tasks (size, type, deadline link). *(AI)*
- **3.3b — Scheduler engine:** constraint solver that places blocks respecting energy, deadlines, buffers, fixed events, venue, sequencing. *(mostly code; AI for the final "make it feel varied/human" pass)*
- **3.3c — Ignition layer:** generates the "stupidly small first move" per task. *(AI, cheap/short)*
- **3.3d — Re-plan engine:** regenerate remaining day on disruption. *(code + light AI)*

---

### 3.4 ANTI-BURNOUT SYSTEM (Julie) — "the fire extinguisher"
*Behaves like the Mark I extinguisher: continuously watches for the fire, auto-suppresses when it ignites.*

**(1) What it needs to work in real life**
- Define the "fire" concretely — overload signals to watch: study hours nearing/exceeding the 5h ceiling; energy forecast crashing while hard work remains; too many deadlines colliding in a short window; several missed blocks in a row (spiral); very late night + early demand tomorrow; self-reported "I'm fried."
- Graduated response, not a kill-switch: **spark → smoke → fire.**
  - *Spark* (early): gentle Julie nudge — "You're 20 min from your ceiling, want to bank the win and stop?"
  - *Smoke* (building): actively suggest cutting/moving a block, forces variety or a break into the plan.
  - *Fire* (overload): auto-suppress — Julie clears the rest of the hard blocks, swaps in recovery, tells the planner to re-plan, and says so out loud.
- **Annoying if:** it triggers on normal hard work (false alarms) or is condescending. Fix: thresholds tuned to *his* real data from the Energy Model, Julie's voice stays warm/dry not preachy, and every intervention is overrideable ("no, I'm fine, keep going").

**(2) How it integrates**
- Reads: `energy_forecast`, `ceiling_remaining`, `deadlines`, `today_plan`, block completion status.
- Writes: `load_level` (spark/smoke/fire), `interventions[]` to the spine.
- Triggers the Day Planner's re-plan engine on smoke/fire.
- Julie (AI sub-worker) only decides *which* intervention fits + phrases it; detection thresholds are plain code.
- Logs interventions back to the Energy Model so it learns what actually causes his burnout.

**(3) Sub-projects**
- **3.4a — Fire-detection monitor:** continuous rule checks on the spine, emits spark/smoke/fire. *(plain code)*
- **3.4b — Julie intervention picker:** given a fire level + context, choose + phrase the response. *(AI, this is Julie)*
- **3.4c — Suppression actions:** the concrete moves (clear blocks, insert recovery, trigger re-plan). *(code)*

---

### 3.5 WAKE-UP SYSTEM

**(1) What it needs to work in real life**
- Two triggers: **double-clap** (always-on listener) OR **alarm time** set the night before.
- Sequence on wake: play intro of *Highway to Hell* → Jarvis voice greets + reads today's stats (sleep, energy forecast, ceiling) and top tasks/deadlines → surfaces the HUD.
- Must not misfire on random claps/TV. Fix: require a clean double-peak pattern + a confirmation window; adjustable sensitivity; "not now" cancel.
- Copyright/licensing note: playing your own local copy of the song for personal use is fine; don't stream/redistribute it. Keep the file local.

**(2) How it integrates**
- Reads `today_plan`, `deadlines[]` (top 3), `energy_forecast`, last night's sleep from the spine.
- Uses the Jarvis voice engine to speak. Triggers the HUD to open on desktop (and can push a phone notification).

**(3) Sub-projects**
- **3.5a — Clap listener:** always-on audio, double-peak detection, sensitivity control. *(plain code)*
- **3.5b — Alarm scheduler:** time-based trigger. *(plain code)*
- **3.5c — Wake sequence orchestrator:** music → voice briefing → HUD open, in order. *(code + AI for the spoken briefing text)*

---

### 3.6 JARVIS CONTROLLER + BOARD SUB-WORKERS

**(1) What it needs to work in real life**
- One entry point (voice + text) that routes any request to the right system or sub-worker.
- Jarvis = router/controller + the voice/personality you talk to. Julie/James/Sumie = specialized sub-workers invoked when relevant (per your board rules: Claude/Jarvis leads by default, suggests summoning a member when helpful, doesn't convene the board unprompted; Sumie synthesizes decisions).
- Must keep the Jarvis persona consistent: formal, precise, strategic, dry wit.
- **Annoying if:** it over-invokes the board (paralysis) or is chatty. Fix: minimal routing, board only when it adds value, Sumie lands decisions fast.

**(2) How it integrates**
- Sits above the spine; reads all state, can trigger any system.
- Routes: "plan my day" → Day Planner; "am I overdoing it" → Julie; wake briefing → Wake-Up; a hard decision → Sumie.
- This is the main **AI brain**. Everything else stays code where possible so this is the only expensive layer.

**(3) Sub-projects**
- **3.6a — Router:** intent → system/sub-worker. *(light AI or even keyword-first, AI fallback)*
- **3.6b — Sub-worker prompts:** Julie/James/Sumie personas + scopes. *(prompt engineering)*
- **3.6c — Voice I/O:** TTS (Jarvis voice) + optional STT for commands. *(code + API)*

---

### 3.7 CUSTOM HUD UI (Stark workshop theme)

**(1) What it needs to work in real life**
- Deep black bg, cyan wireframe lines, arc-reactor blue glow, translucent floating panels, thin HUD linework, subtle animation. Readable, not just pretty.
- Panels for: today's plan, energy curve, deadline radar (radial "sweep" fits the theme), burnout status (the extinguisher gauge), wake briefing.
- Must work on desktop (big, multi-panel) and reflow to phone (single-column, same theme).
- **Annoying if:** style hurts legibility or animations distract during focus. Fix: a "focus/quiet" visual mode that dims the flourish.

**(2) How it integrates**
- Pure front-end over the spine — reads state, renders. No logic lives here.
- Shared component library between desktop + phone.

**(3) Sub-projects**
- **3.7a — HUD component kit:** panels, gauges, wireframe frames, glow tokens. *(front-end)*
- **3.7b — Energy curve + radar viz.** *(front-end)*
- **3.7c — Extinguisher/burnout gauge.** *(front-end)*

---

### 3.8 PHONE LAYER ("phone OS")

**(1) What it needs to work in real life**
- Block/interrupt scrolling (esp. during focus blocks), track time in apps, use that time data in the Energy Model + Anti-Burnout, and give mobile access to Jarvis — all in the same HUD theme.
- Android can do this properly (Accessibility Service for scroll-blocking, UsageStatsManager for app time). **iOS flag:** true scroll-blocking is largely not possible; best case is Screen Time limits + Shortcuts nudges. So Android is the real "phone OS," iOS is a companion.
- **Annoying if:** it blocks the wrong things or feels like a prison. Fix: focus-block-aware (only enforces during scheduled focus), quick "emergency unlock," and it *reports* time rather than only punishing.

**(2) How it integrates**
- Writes app-time + focus-compliance to the spine → Energy Model + Anti-Burnout read it.
- Reads `today_plan` → knows when to enforce phone-away.
- Hosts a mobile HUD to talk to Jarvis.

**(3) Sub-projects**
- **3.8a — Usage tracker.** *(Android API)*
- **3.8b — Scroll/app interrupter, focus-block-aware.** *(Android Accessibility)*
- **3.8c — Mobile HUD + Jarvis access.** *(front-end)*
- **3.8d — iOS companion (reduced scope).** *(Screen Time + Shortcuts)*

---

## 4. BUILD ORDER (recommended)

Dependencies dictate order. Nothing plans well without energy + deadlines + the spine.

**Phase 0 — Spine + skeleton**
- SQLite shared-state schema. Bare Tauri desktop shell. HUD theme tokens.

**Phase 1 — The data systems (feed everything else)**
- Energy Model (3.1) → Deadline Radar (3.2). Both write to the spine; verify with a read-only HUD panel.

**Phase 2 — The planner**
- Day Planner (3.3) consuming energy + deadlines. This is the first system that *feels* like Jarvis.

**Phase 3 — Julie's extinguisher**
- Anti-Burnout (3.4) monitoring the plan + energy, wired to re-plan.

**Phase 4 — Voice + Wake-Up**
- Jarvis voice engine (3.6c), then Wake-Up (3.5): claps → music → spoken briefing.

**Phase 5 — Controller polish**
- Full Jarvis router + board sub-workers (3.6) as one entry point.

**Phase 6 — Phone layer**
- Android first (3.8a–c), iOS companion later (3.8d).

**Phase 7 — HUD full build**
- Flesh out all panels/visualizations (3.7) beyond the read-only test panels.

> Rationale: each phase produces something usable on its own, and later phases only turn on once their data dependencies exist.

---

## 5. OPEN QUESTIONS / FLAGS

- **iOS limits:** real scroll-blocking isn't possible on iOS — accept an Android-primary phone layer? *(pending)*
- **ElevenLabs cost:** realistic Jarvis voice = ongoing API cost. OK, or want a one-time local voice model? *(pending — currently using macOS `say -v Daniel`, free but noticeably synthetic)*
- **Always-on mic:** the clap listener means the desktop mic is always active. Comfortable with that + a hard mute? *(**resolved 2026-08-10** — wake word implemented with a hard-mute toggle that kills the listener process, not just ignores it. Speech stays on-device when macOS supports it.)*
- **Tauri vs Electron:** Tauri recommended; confirm before Phase 0. *(**resolved** — Tauri, shipped)*
- **Song file:** keep a local copy of Highway to Hell for personal playback. *(pending — needed for 3.5c)*
- **API cost:** every non-nav utterance is a Claude call. Consider a local model (Ollama) for cheap/simple turns. *(new, pending)*

---

## 6. PROGRESS TRACKER

Status keys: ⬜ not started · 🟨 in progress · ✅ done

| # | Component | Sub-project | Status | Notes |
|---|---|---|---|---|
| 0 | Spine + skeleton | SQLite schema | ✅ | `001_spine.sql` + `002_habits.sql`, applied & verified |
| 0 | Spine + skeleton | Tauri shell | ✅ | Loads `hud/index.html` directly; no Vite in the loop |
| 0 | Spine + skeleton | HUD theme tokens | ✅ | HUD v10, six pages, deployed to GitHub Pages |
| 3.1 | Energy Model | 3.1a ingest adapters | 🟨 | `garmin_collector.py` written; blocked on rate limit + password rotation |
| 3.1 | Energy Model | 3.1b curve engine | ✅ | Rule-based night-owl curve → `energy_forecast` |
| 3.1 | Energy Model | 3.1c check-in | ⬜ | `checkins` table exists, no UI yet |
| 3.2 | Deadline Radar | 3.2a source connectors | ⬜ | |
| 3.2 | Deadline Radar | 3.2b dedupe+severity | ⬜ | |
| 3.2 | Deadline Radar | 3.2c back-planning hook | ⬜ | |
| 3.3 | Day Planner | 3.3a brain-dump parser | ⬜ | |
| 3.3 | Day Planner | 3.3b scheduler engine | ⬜ | |
| 3.3 | Day Planner | 3.3c ignition layer | ⬜ | |
| 3.3 | Day Planner | 3.3d re-plan engine | ⬜ | |
| 3.4 | Anti-Burnout | 3.4a fire-detection | ⬜ | |
| 3.4 | Anti-Burnout | 3.4b Julie picker | ⬜ | |
| 3.4 | Anti-Burnout | 3.4c suppression actions | ⬜ | |
| 3.5 | Wake-Up | 3.5a clap listener | ⬜ | |
| 3.5 | Wake-Up | 3.5b alarm scheduler | ⬜ | |
| 3.5 | Wake-Up | 3.5c sequence orchestrator | ⬜ | |
| 3.6 | Jarvis Controller | 3.6a router | 🟨 | Keyword-first nav, LLM fallback for everything else |
| 3.6 | Jarvis Controller | 3.6b sub-worker prompts | 🟨 | Jarvis persona live; Julie/James/Sumie not split out yet |
| 3.6 | Jarvis Controller | 3.6c voice I/O | ✅ | Native STT + `say` TTS + wake word — see §9 |
| 3.6 | Jarvis Controller | conversation memory | ✅ | 20-turn rolling thread in Rust |
| 3.6 | Jarvis Controller | markdown memory | ✅ | `USER.md`/`MEMORY.md`, "remember that…" |
| 3.7 | HUD UI | 3.7a component kit | ⬜ | |
| 3.7 | HUD UI | 3.7b energy+radar viz | ⬜ | |
| 3.7 | HUD UI | 3.7c extinguisher gauge | ⬜ | |
| 3.8 | Phone layer | 3.8a usage tracker | ⬜ | |
| 3.8 | Phone layer | 3.8b interrupter | ⬜ | |
| 3.8 | Phone layer | 3.8c mobile HUD | ⬜ | |
| 3.8 | Phone layer | 3.8d iOS companion | ⬜ | |

---

## 7. CHANGELOG

- **2026-08-10 v0.2** — Phase 0 complete; voice loop live. Tauri shell wired to HUD v10, spine schema applied (+ `002_habits.sql`), Dashboard vitals and Habits reading/writing real SQLite. Voice: native STT, system TTS, wake word, conversation memory, markdown memory, spine-aware answers. Public repo + GitHub Pages demo shipped. See §9 for the voice architecture and the macOS bug behind it.
- **2026-07-28 v0.1** — Initial architecture. All four systems + wake-up + controller + HUD + phone layer specced through the 3-step method. Build order and spine design locked. Awaiting answers on the Section 5 flags before Phase 0.

---

## 9. VOICE ARCHITECTURE (added v0.2)

### The macOS constraint that shaped this

The obvious implementation — `webkitSpeechRecognition` in the webview — **does not work in a Tauri app on macOS**. WKWebView swallows the permission request before it reaches macOS's TCC system, so the mic always fails with `not-allowed` and the app never even appears in System Settings → Privacy → Microphone. This is an open upstream bug ([tauri-apps/wry#1195](https://github.com/tauri-apps/wry/issues/1195)); the wry maintainers have tried private Apple APIs and have not resolved it. No amount of `Info.plist` / entitlement configuration fixes it.

**Do not spend time re-testing the webview path.** Audio must be captured natively.

### How it actually works

```
core press ──► Rust listen_once ──► jarvis-listen (Swift sidecar)
   or                                 AVAudioEngine + SFSpeechRecognizer
"Jarvis…" ──► Rust wake loop ──►      (on-device when supported)
                                            │
                        JSON lines on stdout │  partial / final / wake
                                            ▼
                          Rust emits Tauri events ──► HUD
                                            │
                              ask_jarvis ──► Claude API (key from env)
                                            │
                              speak_native ──► macOS `say -v Daniel`
```

- **`src-tauri/speech/JarvisListen.swift`** — the native listener. Built automatically by `build.rs` (needs `swiftc`; ships with Xcode CLT). Two modes: single-utterance, and `--wake` for hands-free.
- **Signing is mandatory.** The mic entitlement only takes effect once embedded in a code signature, so `tauri.conf.json` sets `"signingIdentity": "-"` (ad-hoc). Side effect: **every rebuild changes the signature, which invalidates the TCC grant** — so macOS re-prompts for mic access after a rebuild. That's expected, not a bug.
- **`npm run tauri dev` cannot get mic permission** — it runs the raw unbundled binary. Voice only works from the built `.app`:
  ```
  export ANTHROPIC_API_KEY=sk-ant-...
  ./src-tauri/target/debug/bundle/macos/jarvis.app/Contents/MacOS/jarvis
  ```
  (run the binary directly, not `open` — `open` won't inherit the API key from the shell)

### Memory layout

Deliberately the OpenClaw-compatible layout from the PRD, so migrating the brain later doesn't mean rewriting memory. Lives next to the spine DB in `~/Library/Application Support/com.hila.jarvis/`:

| File | What it holds |
|---|---|
| `USER.md` | Durable facts about Zohar. Hand-edited; read into every prompt. |
| `MEMORY.md` | Things Jarvis was told to remember ("remember that…"). Appended automatically. |
| `jarvis.db` | The spine. |

Conversation history is a 20-turn rolling window held in Rust memory — deliberately **not** persisted, so restarting clears the thread while durable notes survive.

### Known gaps

- Local build environment: `/usr/local/include/Block.h` (stray 2018 liblzma file) shadows the system header and breaks Swift builds. Worked around in `build.rs` by forcing SDK includes first — the stray file was left alone rather than deleted. Same stray-header problem blocks `whisper.cpp` (needs `cmake`, and Homebrew can't build on this machine's Command Line Tools).
- Wake word matches `jarvis`/`jervis`/`travis` (common mishears). False positives are possible; the hard mute is the mitigation. [Porcupine](https://github.com/Picovoice/porcupine) is the proper fix.
- macOS `say -v Daniel` is free but audibly synthetic. [Piper](https://github.com/rhasspy/piper) is the free local upgrade; ElevenLabs is the paid one.

### Which brain answers

`JARVIS_LLM` selects the provider (Anthropic by default, `ollama` for a local
model) — see **[`docs/BRAINS.md`](docs/BRAINS.md)** for measured latency and
quality across four local models on this hardware, published cloud pricing,
and the cost of an actual JARVIS conversation.

Short version: local inference was benchmarked on this machine and **is not
viable** — a 2017 dual-core 15W chip with no GPU path has a ~12.5s floor, and
every small model tested fabricated details it was given. The switch exists so
that moving to Apple Silicon later is a one-line change.

### Prior art

**[`docs/PRIOR_ART.md`](docs/PRIOR_ART.md)** surveys what already exists —
other JARVIS projects, serious open assistants worth learning from, and
drop-in building blocks for STT/TTS/wake-word. It closes with the one
architectural idea that matters most next: **tool calling**, which is what
turns "LifeOS shows state; JARVIS changes state" (§0) from a slogan into
something true.

---

## 8. HOW TO RESUME IN A NEW CHAT

Paste this file and say: *"Jarvis project — here's the master doc. We're on Phase X / component Y. Continue."* Claude should read Section 6 for status, Section 4 for what's next, and update Sections 6 + 7 as work completes.
