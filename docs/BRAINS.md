# JARVIS "Minds" — which brain should answer?

Everything below is either **measured on this machine** or **quoted from a
published price list**. Nothing is estimated. Where a number is unmeasured,
it says so.

Swapping brains is one environment variable — see [Switching](#switching)
at the bottom. The adapter lives in `src-tauri/src/lib.rs` (`provider()`,
`ask_anthropic()`, `ask_ollama()`).

---

## The machine these numbers came from

This matters more than anything else in this document.

| | |
|---|---|
| CPU | Intel Core i5-7360U @ 2.30GHz — **2 physical cores**, 4 threads, 15W |
| Released | 2017 (Kaby Lake, ultra-low-power laptop part) |
| RAM | 16 GB |
| GPU acceleration for LLMs | **None.** Intel Macs have no Metal path for this. |
| macOS | 13.7.8 (Ventura) |

Two physical cores at 15W with no GPU offload is the low end of what Ollama
runs on at all. **Do not generalize these numbers** — an Apple Silicon
machine does the same work roughly an order of magnitude faster, and most
published "Intel Mac" figures come from 6-core desktop i7/i9 chips.

---

## Local models — measured

Benchmark: one realistic JARVIS turn — the full system prompt (persona +
`MEMORY.md` + live spine readings, ~375 tokens) plus the question *"How am I
doing today, and what should I work on next?"*. Reproduce with
`scratchpad/bench.py` (kept out of the repo; see [Methodology](#methodology)).

| Model | Size | Prompt (~375 tok) | Reply | **Total wall time** |
|---|---:|---:|---:|---:|
| `qwen2.5:0.5b` | 397 MB | 3.1s (121 tok/s) | 200 tok @ 25.7 tok/s | **12.5s** |
| `llama3.2:1b` | 1.3 GB | 7.5s (50 tok/s) | 85 tok @ 10.3 tok/s | **18.3s** |
| `qwen2.5:1.5b` | 986 MB | 8.7s (43 tok/s) | 44 tok @ 14.0 tok/s | **14.6s** |
| `llama3.2:3b` | 2.0 GB | 16.6s (23 tok/s) | 83 tok @ 6.4 tok/s | **34.0s** |

### Reading the table

**Prompt processing, not generation, is the bottleneck.** Before the model
writes a single word it has to read ~375 tokens of system prompt — that
alone costs 3–17 seconds depending on model size. This is the number that
kills local inference here, and it is the one people forget to measure.

**It gets worse as the conversation grows.** JARVIS keeps a 20-turn rolling
history plus memory files, so a real prompt reaches 1000+ tokens quickly. At
`llama3.2:3b`'s measured 23 tok/s, a 1500-token prompt is **~65 seconds of
silence** before the reply starts. The memory features make local inference
worse, not better.

**Bigger is slower *and* not obviously better.** `llama3.2:3b` has the best
persona ("Good day, sir") and is nearly 3× slower than the 0.5B.

### Quality — what they actually said

All four were given identical context stating **body battery 64, stress 28,
sleep quality 78, 6.4h slept, Monday 5:30 PM**.

| Model | Verbatim | Problem |
|---|---|---|
| `qwen2.5:0.5b` | *"Good morning! On Monday at 5:30 PM, Zohar is working late…"* | Says "good morning" at 5:30 PM; refers to Zohar in the third person while talking **to** him |
| `llama3.2:1b` | *"You're feeling a bit exhausted after that long study session last night…"* | **Invented** the study session. Nothing in the context mentions one |
| `qwen2.5:1.5b` | *"Your energy is high right now, so you might feel focused…"* | Body battery 64/100 is mediocre. It read the number and drew the wrong conclusion |
| `llama3.2:3b` | *"Good day, sir… I'd recommend diving into that Yahadut textbook…"* | Best voice by far. But **invented** the textbook — the context says "Yahadut exam", not a textbook |

**Every single one hallucinated or misread the data.** That is the
disqualifying result, not the latency. The entire point of feeding JARVIS the
spine is that it answers from real numbers; a model that invents a study
session you didn't have is worse than no answer.

---

## Cloud models — published prices, not measured

Not benchmarked here (no API key configured at time of writing). Prices are
per **million tokens**, from Anthropic's published list.

| Model | Input | Output | Context | Notes |
|---|---:|---:|---:|---|
| Claude Opus 5 | $5.00 | $25.00 | 1M | Most capable; overkill for 1–3 sentence spoken replies |
| **Claude Sonnet 5** | **$3.00** | **$15.00** | 1M | *Currently wired.* Intro pricing $2.00/$10.00 through 2026-08-31 |
| Claude Haiku 4.5 | $1.00 | $5.00 | 200K | Cheapest; fine for short conversational turns |

### What a JARVIS conversation actually costs

Using the measured shape of a real turn — ~400 tokens in, ~85 tokens out:

| Model | Per exchange | Exchanges per $1 | 20 exchanges/day for a month |
|---|---:|---:|---:|
| Claude Haiku 4.5 | $0.00083 | ~1,200 | **~$0.50** |
| Claude Sonnet 5 (intro) | $0.00165 | ~600 | **~$1.00** |
| Claude Sonnet 5 (standard) | $0.00248 | ~400 | **~$1.50** |
| Claude Opus 5 | $0.00413 | ~240 | **~$2.50** |

Latency is unmeasured here, but a short cloud turn is normally a couple of
seconds — versus the 12.5s **floor** measured locally.

**Caveat:** these assume a ~400-token prompt. As `MEMORY.md` grows and
conversation history fills the 20-turn window, input tokens rise and so does
cost. Prompt caching would help, but the minimum cacheable prefix is 512
tokens (Opus 5) / 1024 (Sonnet 5) — the current system prompt is **below
both**, so caching does nothing yet. Revisit once memory files get bigger.

---

## Pros and cons

### Local (Ollama)

**For**
- Free. No key, no account, no per-token cost.
- Private — audio-derived text never leaves the machine. Matches the
  local-first principle in the project's PRD.
- Works offline.
- No rate limits.

**Against**
- **12.5s best case on this hardware**, 34s for the model with acceptable
  voice. A voice assistant that pauses 15+ seconds is not a voice assistant.
- **All four tested models fabricated details** not present in the context.
- Degrades as conversation history grows — the opposite of what you want.
- Pins both CPU cores at 100%, which makes the HUD itself lag (this machine
  already struggles to render the HUD's CSS) plus fan noise and battery drain.
- ~1–2 GB disk per model.

### Cloud (Anthropic API)

**For**
- Fast enough to feel conversational.
- Actually reads the spine data correctly.
- Zero local CPU cost — the HUD stays smooth.
- Costs roughly a coffee per month at personal-use volume.

**Against**
- Costs money. Pay-as-you-go, billed separately from any Claude.ai
  subscription — **there is no way to route API usage through a subscription**.
- Requires a network connection.
- Text derived from your microphone leaves the machine. (The *audio* never
  does — speech-to-text is local either way, see `JARVIS_MASTER.md` §9.)
- Needs `ANTHROPIC_API_KEY` set before launch.

---

## Verdict

**Use the cloud brain on this machine.** Local inference is not viable here —
not because of setup, but because a 2017 dual-core 15W chip with no GPU path
has a ~12.5s floor and the small models that fit under it hallucinate the
data they're given.

**Revisit local on Apple Silicon.** With GPU acceleration an M-series machine
runs a 7–8B model at conversational speed, which is both fast enough *and*
large enough to stop inventing study sessions. The provider switch already
exists, so that day is a one-line change.

**If cost is the blocker,** note that nav commands (`"open focus"`) and
`"remember that…"` cost nothing — they never reach a model. Only open-ended
conversation bills. And `claude-haiku-4-5` at ~$0.50/month for daily use is a
reasonable floor if Sonnet feels like too much.

---

## Switching

Defaults to Anthropic. No code changes needed:

```bash
# Cloud (default)
export ANTHROPIC_API_KEY=sk-ant-...

# Local
export JARVIS_LLM=ollama
export JARVIS_OLLAMA_MODEL=llama3.2:1b     # default if unset
export JARVIS_OLLAMA_URL=http://127.0.0.1:11434  # default if unset
```

`llm_status` reports which brain is active and whether it is actually usable
(i.e. whether the key is set), so the HUD can show it.

---

## Methodology

- Ollama 0.32.7, universal binary (x86_64 slice), default quantization for
  each tag as pulled from the Ollama registry.
- One run per model, `stream: false`, `num_predict: 200`, nothing else
  running. Timings from Ollama's own `prompt_eval_duration` /
  `eval_duration` fields, not wall-clock guesses.
- Identical system prompt and question for every model — the real JARVIS
  persona and a real spine snapshot, not a toy prompt.
- Single run per model, so treat these as **order-of-magnitude, not
  benchmark-grade**. The gap between 12.5s and ~2s is large enough that run
  variance doesn't change the conclusion.
- The models and the Ollama install lived in a temp scratchpad and were not
  committed; nothing in this repo depends on them.
