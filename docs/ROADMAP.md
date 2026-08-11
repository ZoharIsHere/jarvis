# JARVIS — road to done

The execution plan. Phases are ordered so that **each one is useful on its
own** and nothing waits on something that hasn't shipped.

- **What you need to do** lives in [`YOUR_TODO.md`](YOUR_TODO.md) — a single
  compact list, kept current. Nothing else in this doc asks anything of you.
- Architecture and progress tracker: [`../JARVIS_MASTER.md`](../JARVIS_MASTER.md)
- Brain benchmarks: [`BRAINS.md`](BRAINS.md) · Prior art: [`PRIOR_ART.md`](PRIOR_ART.md)

**Legend** — 🟢 buildable now, nothing needed from you · 🔑 needs an API key ·
🔗 needs an account/OAuth · 📱 needs a phone · ⚠️ has a real risk, see
[Problems](#problems-were-going-to-hit)

---

## ✅ Shipped so far

| Item | Phase |
|---|---|
| Tests — 16, run against the real HUD, CI on push | 0.1 |
| Secrets in the macOS Keychain (`save my API key`) | 0.3 |
| Self-diagnostics (`JARVIS, status`) | 0.5 |
| Job scheduler + `scheduled_jobs` / `job_runs` tables | 1.1 |
| Garmin on a 4-hour cadence | 1.2 |
| Weekly review, Sundays 20:00 | 1.3 |
| Morning briefing, 09:30 | 1.4 |
| Native macOS notifications (nudges reach you when the HUD is hidden) | — |
| Spine backup, nightly, 14 retained | — |
| Continuous conversation (follow-up without pressing again) | 8.3 |
| Learning-loop sampling into `energy_log` + curve-error reporting | 6.2 (partial) |
| Context handoff (`what was I doing yesterday`) | — |

**Still unblocked and not yet built:** HUD file split (0.4), migration
auto-run (0.2), wake-up sequence (1.5 — needs the song file), spine server
(Phase 4), OCR via Apple Vision (7.2), Piper TTS (8.1), media panels (8.5),
multi-day planning, "explain yourself", dry-run mode.

---

## Phase 0 — Foundations (🟢)

Boring, and everything later is safer for it. No new capability.

| # | Item | Why it's first |
|---|---|---|
| 0.1 | **Tests** — energy curve, streak computation, planner slot allocation, nudge rules | ~2,700 lines of untested logic. Every later phase edits it. |
| 0.2 | **Fix migration auto-run** | Migrations only fire when the frontend calls `Database.load()`. A fresh clone gets no schema. Move the call into Rust startup. |
| 0.3 | **Keychain for secrets** ⚠️ | `ANTHROPIC_API_KEY` as an env var means re-exporting on every launch and it dies with the shell. Store in macOS Keychain, read at startup. Makes the key a one-time action instead of a ritual. |
| 0.4 | **Split `hud/index.html`** ⚠️ | It's one `<script>` block, now ~90KB and growing. Split into modules + a tiny concat step that still emits the single file GitHub Pages needs. |
| 0.5 | **Self-diagnostics** — *"JARVIS, status"* | Reports what's actually wired: key present? Garmin fresh? mic granted? spine writable? Turns "it's broken" into a specific answer. |

## Phase 1 — Scheduled mode (🟢)

Your brief's #2. The missing third of on-demand / scheduled / continuous.

| # | Item | Notes |
|---|---|---|
| 1.1 | **Job scheduler in Rust** | Background thread; jobs defined in a new `scheduled_jobs` table (cron-ish spec, last_run, enabled). Survives the HUD being hidden. |
| 1.2 | **Garmin collector on a cadence** | Currently manual, which is why `energy_state` is NULL. Run it every few hours; log failures rather than dying. |
| 1.3 | **Weekly review, Sundays** | `weeklyReview()` exists; just needs a trigger. |
| 1.4 | **Morning briefing** | Reads today's plan, top deadlines, and last night's sleep aloud at a configured time. |
| 1.5 | **Wake-up sequence** (§3.5) | Alarm time or double-clap → *Highway to Hell* → briefing. Clap detection is amplitude double-peak in the existing Swift helper — no new dependency. ⚠️ needs a local copy of the song. |

## Phase 2 — Tool calling (🔑)

**The unlock.** `JARVIS_MASTER.md` opens with *"LifeOS shows state; JARVIS
changes state."* This is what makes that true.

| # | Item | Notes |
|---|---|---|
| 2.1 | **Tool-calling loop** | Anthropic tool use in `ask_anthropic`. The tool *implementations* already exist as `tryActionCommand` internals — this exposes them to the model. |
| 2.2 | **Tool set v1** | `mark_habit_done`, `add_task`, `complete_task`, `plan_day`, `add_deadline`, `snooze_deadline`, `start_focus_block`, `log_checkin`, `search_memory`, `remember` |
| 2.3 | **Confirmation gate** ⚠️ | Destructive/irreversible tools ask first. Non-negotiable before `run_shell` exists. |
| 2.4 | **Action log + undo** | Every tool call writes to an `action_log`. *"undo that"* reverses the last reversible one. |
| 2.5 | **Brain-dump → tasks** (§3.3a) | Free text → structured task rows. Feeds the planner, which currently has an empty `tasks` table. |
| 2.6 | **Cost meter** | Per-request tokens + latency into the spine; live spend in the HUD. Also the data the Phase 6 router needs. ⚠️ pair with a **daily budget cap** that hard-stops calls. |

## Phase 3 — Home & devices (🔗)

| # | Item | Notes |
|---|---|---|
| 3.1 | **Tuya smart lights** 🔗 | Tuya Cloud OpenAPI from Rust (HMAC-SHA256 signed). Commands: on/off, brightness, colour. |
| 3.2 | **Lights as JARVIS state** | The arc-reactor idea made physical: room turns amber on *smoke*, red on *fire*, dims when a focus block starts, warms at sleep-protocol time. This is the feature that makes him feel like he's *in the room*. |
| 3.3 | **Voice control** | *"lights off"*, *"dim to 20"*, *"focus lighting"* — via Phase 2 tools. |
| 3.4 | **Local fallback** ⚠️ | Tuya Cloud dies if the internet does. `tinytuya`-style LAN control as a fallback needs per-device local keys. |

## Phase 4 — Spine server (🟢) ⚠️

**Prerequisite for the phone, and for anything multi-device.** Right now the
spine is a file only this Mac can open.

| # | Item | Notes |
|---|---|---|
| 4.1 | **Local HTTP API over the spine** | Read/write endpoints, bound to LAN. |
| 4.2 | **Token auth + loopback-first** ⚠️ | Never expose an unauthenticated DB to the network. Shared secret minimum. |
| 4.3 | **Conflict handling** ⚠️ | Two writers (Mac + phone) will collide. Last-write-wins per row plus an `updated_at`, or a small op log. Decide before the phone ships. |
| 4.4 | **Offline queue** | Phone commands issued with no LAN replay on reconnect. |

## Phase 5 — Phone layer (📱)

Your alarm clock lives here.

| # | Item | Notes |
|---|---|---|
| 5.1 | **Minimal Android app** 📱 | Kotlin. Talks to the Phase 4 API. ⚠️ A PWA **cannot** do a reliable alarm — the browser can't wake a sleeping device. This has to be native. |
| 5.2 | **Alarm clock** 📱 | `AlarmManager.setAlarmClock()` — fires through Doze and silent mode. JARVIS sets it by voice; wake sequence runs on the phone. |
| 5.3 | **Mobile HUD** | Same theme, single column. |
| 5.4 | **Usage tracking + scroll blocking** 📱 | `UsageStatsManager` + Accessibility Service (§3.8). Feeds the same `app_usage` table the Mac writes. |
| 5.5 | **Push** | `ntfy.sh` is the cheap path — free, self-hostable, has an Android app. Good for nudges; **not** a substitute for a real alarm. |

## Phase 6 — Smarter brain (🔑 / 🟢)

| # | Item | Notes |
|---|---|---|
| 6.1 | **Hybrid router seam** | Extend `provider()` from a static env choice to per-query. ⚠️ On *this* Mac local is 12.5s, so routing to local is a **loss** today — build the seam, leave the policy trivial until Apple Silicon. |
| 6.2 | **Learning loop** (§3.1) | `energy_log` exists and is empty. Log predicted vs. actual, adjust the curve weekly. |
| 6.3 | **Sub-workers** (§3.6) | Julie is done and is pure code. Sumie (decision synthesis) and James need the model — thin persona prompts over the same tool set. |
| 6.4 | **MCP client** | Speak MCP instead of inventing a plugin format; inherit calendar/Slack/filesystem servers. This *is* the PRD's plugin bus. |
| 6.5 | **Semantic memory search** 🔑 | Embeddings over `MEMORY.md` + daily notes. |

## Phase 7 — Perception (🔑)

| # | Item | Notes |
|---|---|---|
| 7.1 | **Screen awareness** 🔑 | Screenshot → vision model. *"what am I looking at"*, *"help with this error"*. |
| 7.2 | **OCR** | Text out of screenshots. Local via Apple Vision framework (Swift, no key) — cheaper than the vision model for pure text. |
| 7.3 | **Focus enforcement** | Block distracting apps during a focus block (macOS DND + app watching). |
| 7.4 | **Document generation** | Reports/decks from spine data. |

## Phase 8 — Voice & polish

| # | Item | Notes |
|---|---|---|
| 8.1 | **Piper TTS** | Free local neural voice; `say -v Daniel` is audibly synthetic. Prebuilt macOS binaries exist — no `cmake`, so unlike whisper.cpp it isn't blocked. |
| 8.2 | **Porcupine wake word** 🔗 | Real detection instead of string-matching "travis". Free tier needs a Picovoice key. |
| 8.3 | **Continuous conversation** | Stay listening after a reply instead of re-pressing. |
| 8.4 | **STT/TTS engine abstraction** | Generalize the seam so Piper/Whisper swap in cleanly (S.A.T.U.R.D.A.Y's toolbox framing). |
| 8.5 | **Media panels** | Agent-summoned panels that swoop in from Z-depth. Pure HUD. |
| 8.6 | **Fix the native look** ⚠️ | Unresolved: perf forced flattening glow/`clip-path`, and you said it didn't look right. Needs a version that's both smooth and good. |

## Phase 9 — Distribution (optional)

Only if anyone but you runs this. **Proper signing needs a paid Apple Developer
account** 🔗 — without it every user hits Gatekeeper. Cross-platform means
replacing the macOS-specific Swift STT and `say`.

## Phase 10 — Multi-room (big)

WebRTC audio transport (S.A.T.U.R.D.A.Y's genuinely novel idea) — JARVIS hears
you from any device, not just this laptop's mic. Depends on Phase 4. Closest
thing to the movie behavior; also the largest single item here.

---

## Problems we're going to hit

Ordered by how much damage they do if ignored.

| # | Problem | Plan |
|---|---|---|
| P1 | **Runaway API cost** — an agent loop with tools can spend fast | Hard daily cap in `settings`, enforced in `ask_anthropic` before the call, not after. Cost meter (2.6) makes it visible. |
| P2 | **Destructive tool calls** — `run_shell`/CodeAct are unsandboxed | Confirmation gate (2.3) before any such tool exists. Allowlist, never blocklist. |
| P3 | **Concurrent spine writers** — Mac + phone collide | Decide the conflict model in Phase 4 (4.3), before the phone can create the problem. |
| P4 | **Unauthenticated network DB** | Loopback + token from the first commit of Phase 4. Never "add auth later". |
| P5 | **`hud/index.html` collapsing under its own weight** | Split now (0.4), while it's still ~90KB. |
| P6 | **No tests + accelerating change** | Phase 0 first. The planner and nudge rules are exactly where a silent regression hides. |
| P7 | **Mic permission dies every rebuild** | Cosmetic but murders the dev loop. Investigate a stable ad-hoc identity; otherwise document and move on. |
| P8 | **Public demo vs native divergence** | The `NATIVE` gate is load-bearing and growing. A smoke test that loads the HUD with `__TAURI__` absent and asserts no errors. |
| P9 | **Tuya cloud outage kills the lights** | Local fallback (3.4). |
| P10 | **2-minute builds on a 2-core laptop** | Prefer `cargo check` while iterating; `--bundles app` (skip DMG) saved real time already. |
| P11 | **Wake-word false positives** | Porcupine (8.2), or require a short confirm window before acting. |
| P12 | **Garmin rate-limits again** | Backoff + cache the token properly; never retry in a tight loop. |

---

## Ideas worth adding (mine, not from the brief)

- **Backup/restore the spine.** It's one SQLite file holding everything, with
  no backup. A nightly copy is cheap insurance.
- **Multi-day planning** — the planner only knows today. "Plan my week" is a
  small extension and much more useful before an exam.
- **Context handoff** — *"what was I working on yesterday?"* from `plan_blocks`
  + `app_usage`. Nearly free once both have history.
- **Native macOS notifications** — nudges currently only exist inside the HUD.
  If it's hidden, Julie is talking to an empty room.
- **"Explain yourself"** — *"why did you say that?"* prints the rule and the
  spine values that fired it. Makes the system debuggable and trustworthy.
- **Dry-run mode** — let the planner or a tool call show what it *would* do.
  The cheapest way to build trust before granting write access.
- **Energy model honesty** — the curve is currently a fixed night-owl shape,
  not learned. Until 6.2 lands, the HUD should not imply it's personalized.

---

## Suggested order

**Now:** Phase 0 → Phase 1. Both fully unblocked, and Phase 1 is where JARVIS
starts acting on his own schedule.

**The moment a key exists:** Phase 2. Biggest single jump in capability.

**Then, by what you want most:** Tuya lights (3) are the most *fun* per hour of
work; the phone/alarm (4→5) is the most *useful* but needs the spine server
first; multi-room (10) is the most impressive and the most expensive.
