# Prior art — what exists, and what's worth stealing

A survey of projects in this space, with what each one is actually good for.
Star counts and last-updated dates were read from the GitHub API on
**2026-08-10** and will drift.

The honest headline: **most repositories named "JARVIS" are small hobby
projects.** The genuinely useful ideas mostly come from serious
general-purpose assistant projects that don't use the name at all, and from
this project's own architecture docs.

---

## 1. The direct ancestors (this project's own lineage)

Read these before anything below — they are more specific to what JARVIS is
trying to be than any third-party repo.

### `JARVIS_MASTER.md` (in this repo)

The architecture doc this project was built from. Specifies the spine, the
Julie / James / Sumie sub-workers, the anti-burnout "spark → smoke → fire"
extinguisher model, double-clap wake → *Highway to Hell* → spoken briefing,
and the phone layer. **Section 6 is the live progress tracker**; §9 documents
the voice architecture.

Two things in it were already resolved by building: the always-on-mic
question (wake word now has a hard mute) and Tauri-vs-Electron (Tauri,
shipped).

### The PRD plan (`jarvis_prd_plan_*.plan.md`, not in this repo)

Docker-first, local-first, OpenClaw-compatible markdown memory
(`USER.md` / `MEMORY.md` / `memory/YYYY-MM-DD.md`), a privacy gateway that
keeps calendar and email off cloud models, and a plugin bus with
`public` / `local_only` privacy classes. Integration order: voice → weather →
Home Assistant → Calendar → Gmail → Slack → OpenClaw.

**Already adopted:** the markdown memory layout is implemented exactly as
specified, so the eventual OpenClaw migration won't have to rewrite memory.

**Notable disagreement with what got built:** the PRD specifies a
*browser-served* UI (`"UI shell: Browser web app served from Docker (not
native desktop for v1)"`). This project went native Tauri instead. Worth
knowing, because the native choice is what forced the macOS microphone
workaround in `JARVIS_MASTER.md` §9 — the browser path wouldn't have hit it.

---

## 2. Iron-Man-styled assistants

| Repo | ★ | Language | Updated |
|---|---:|---|---|
| [eadmin2/jarvis_ai](https://github.com/eadmin2/jarvis_ai) | 124 | Python | 2026-08-10 |
| [DawoodTouseef/J.AR.V.I.S.](https://github.com/DawoodTouseef/J.AR.V.I.S.) | 29 | Python | 2026-08-04 |
| [anujssmishra/Iron-Man-HUD---JARVIS](https://github.com/anujssmishra/Iron-Man-HUD---JARVIS) | 2 | Python | 2026-01-31 |
| [Hasan-Ikbal/Jarvis_AI_GUI](https://github.com/Hasan-Ikbal/Jarvis_AI_GUI) | 0 | Python | 2025-11-12 |
| [hzaid01/Jarvis](https://github.com/hzaid01/Jarvis) | 0 | JavaScript | 2026-07-12 |

**Only the first one is worth studying.** The rest are single-author hobby
projects at 0–29 stars; browse them for aesthetic ideas, not architecture.

### eadmin2/jarvis_ai — the closest real analog

Self-hosted Iron-Man-style voice assistant with a glowing arc-reactor HUD in
the browser, local Whisper STT, ElevenLabs voice, persistent memory, and 80+
skills. Actively developed.

**Already borrowed:**
- **Click the ring and talk** — the core dial is the primary mic trigger.
- **Live transcription while speaking** — words appear on the dial as you talk.

**Worth stealing next:**
- **Agent-summoned media panels** — *"show me a video of X, on screen"* and a
  panel swoops in from Z-depth, traces its frame, materializes through a
  scanline. Pure spectacle, very on-theme, and the HUD already has the
  transmission-overlay machinery to build on.
- **Skills as a growth mechanism** — 80+ capabilities without touching core.
- **Security posture** — allowlist proxy, token-gated endpoints,
  loopback-only binding. Relevant the moment JARVIS is reachable over a LAN.

**Notably: it uses local Whisper, not the browser speech API** — the same
conclusion this project reached the hard way (see `JARVIS_MASTER.md` §9).

### DawoodTouseef/J.AR.V.I.S.

PyQt5 + OpenCV desktop automation with voice activation, system monitoring,
and **proactive suggestions from camera and screenshot analysis**. The
proactive-from-screen idea is the interesting part — everything else here is
better covered elsewhere.

---

## 3. Serious assistants (where the real lessons are)

| Repo | ★ | Language | Why it matters here |
|---|---:|---|---|
| [ollama/ollama](https://github.com/ollama/ollama) | 178k | Go | The local-model runtime. Already wired in — see [BRAINS.md](BRAINS.md) |
| [openinterpreter](https://github.com/OpenInterpreter/open-interpreter) | 68k | Rust | Natural language → executes code locally. The "can actually do things" reference |
| [home-assistant/core](https://github.com/home-assistant/core) | 90k | Python | #3 on the PRD's integration list. Its Assist pipeline is Whisper → Piper → Ollama, fully local |
| [janhq/jan](https://github.com/janhq/jan) | 44k | TypeScript | Offline ChatGPT replacement, OpenAI-compatible local server, MCP support |
| [leon-ai/leon](https://github.com/leon-ai/leon) | 17k | TypeScript | Local-first assistant that **made the exact transition this project faces**: intent-classifier → agentic loop with planning |
| [pipecat-ai/pipecat](https://github.com/pipecat-ai/pipecat) | 14k | Python | Voice-agent framework — barge-in, sub-second latency |
| [livekit/agents](https://github.com/livekit/agents) | 13k | Python | Voice agents at scale, native telephony |
| [MycroftAI/mycroft-core](https://github.com/MycroftAI/mycroft-core) | 6.6k | Python | The original open voice assistant; its **skills system** is the reference design |

**If you read one:** [Leon](https://github.com/leon-ai/leon). It is the
closest philosophical match to the PRD (local, private, memory, tools) and it
already went through the architectural change JARVIS is about to.

---

## 4. Building blocks

Swap-in components, should any current piece prove insufficient.

| Repo | ★ | Replaces / adds |
|---|---:|---|
| [openai/whisper](https://github.com/openai/whisper) | 107k | STT. The reference implementation |
| [ggml-org/whisper.cpp](https://github.com/ggml-org/whisper.cpp) | 53k | Whisper in C++ — runs on CPU. **Blocked here: needs `cmake`, and Homebrew is broken on this machine** (see `JARVIS_MASTER.md` §9) |
| [SYSTRAN/faster-whisper](https://github.com/SYSTRAN/faster-whisper) | 25k | Whisper via CTranslate2, substantially faster |
| [rhasspy/piper](https://github.com/rhasspy/piper) | 11k | **Local neural TTS.** The free path to a better voice than macOS `say -v Daniel` — no ElevenLabs bill |
| [Picovoice/porcupine](https://github.com/Picovoice/porcupine) | 4.9k | **Proper wake-word detection.** Current implementation string-matches `jarvis`/`jervis`/`travis`, which will false-positive |
| [KoljaB/RealtimeSTT](https://github.com/KoljaB/RealtimeSTT) | 10k | Streaming STT with voice-activity detection |
| [huggingface/speech-to-speech](https://github.com/huggingface/speech-to-speech) | 12k | Full local voice-agent pipeline reference |
| [modelcontextprotocol/servers](https://github.com/modelcontextprotocol/servers) | 89k | **MCP servers** — calendar, Slack, filesystem, and more, already built |

Two of these map onto known gaps: **Piper** (voice quality) and **Porcupine**
(wake-word robustness). **whisper.cpp is blocked** on this machine — worth
recording so nobody re-attempts it.

---

## 5. The one architectural idea worth more than the rest

Everything above is a component. This is a direction.

**Tool calling.** JARVIS can currently *talk about* your life but cannot
*change* it — which is precisely the line `JARVIS_MASTER.md` opens with:
*"LifeOS shows state; JARVIS changes state."* It doesn't yet.

Give the model a tool set — `add_task`, `complete_habit`, `plan_day`,
`snooze_deadline` — and *"mark my run done and plan tomorrow around my exam"*
just works. Every future capability becomes additive instead of a code change.
Open Interpreter and Leon are both worth reading for how they structure this.

Two follow-ons once tools exist:

- **MCP as the plugin bus.** Rather than inventing a plugin format, speak
  [MCP](https://modelcontextprotocol.io) and inherit an ecosystem — calendar,
  Slack, filesystem — for free. It maps cleanly onto the plugin bus the PRD
  already describes.
- **Proactive JARVIS.** He should *initiate*, not only answer: notice you're
  20 minutes from your study ceiling, notice a deadline colliding with low
  energy. This is exactly the Julie anti-burnout spec in `JARVIS_MASTER.md`
  §3.4 — and the spine already holds every input it needs.

Two smaller ideas worth noting:

- **Barge-in** (Pipecat, LiveKit) — being able to interrupt mid-sentence is
  the single biggest perceived-intelligence jump in voice UX.
- **Screen awareness** (DawoodTouseef) — *"what am I looking at?"* via
  screenshot plus a vision model.
