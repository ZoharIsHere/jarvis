# Things only you can do

Everything in [`ROADMAP.md`](ROADMAP.md) that I can't do myself. Kept current —
items move to **Done** rather than disappearing, so nothing gets silently
dropped.

Ordered by what unblocks the most.

---

## 🔴 Do first — security

### 1. Rotate the Garmin password
Still outstanding from the very first task. The HUD's own demo data records it
as *"Exposed in terminal + chat"*. Nothing in this repo contains it, but it
leaked somewhere.
→ Garmin Connect → Account Settings → change password.
**Unblocks:** the Garmin collector, which unblocks Julie's real vitals
(currently NULL, so she runs on self-reported check-ins only).

### 2. Rotate the Notion integration token
Same story — an `ntn_...` token was exposed outside the repo.
→ notion.so/my-integrations → your integration → **Refresh** the secret.

---

## 🟠 Unblocks the most capability

### 3. Anthropic API key
→ [console.anthropic.com](https://console.anthropic.com) → API Keys → Create
Key. Needs a payment method; it's pay-as-you-go and billed **separately** from
any Claude.ai subscription — there is no way to route one through the other.

**When you have it**, just say *"save my API key"* to JARVIS and paste it into
the prompt. It goes into the macOS Keychain — no exporting on every launch, and
it never touches the repo. (Don't paste it into chat here.)

**Cost, measured on your actual usage shape** (~400 tokens in, ~85 out per
exchange): roughly **$0.50–1.00/month** at 20 exchanges/day on Haiku or Sonnet.
See [`BRAINS.md`](BRAINS.md) for the full table.

**Unblocks:** tool calling (the single biggest capability jump), brain-dump →
tasks, screen awareness, Sumie/James, semantic memory.

Once you have it, tell me and I'll move it into Keychain so you never export it
again.

---

## 🟡 Per-feature — do these when you want that feature

### 4. Tuya — smart lights
1. Sign up at [iot.tuya.com](https://iot.tuya.com) (free developer account)
2. Create a **Cloud Project** (data centre = whichever region your app uses)
3. Link your **Smart Life / Tuya Smart** app account to the project
4. Send me: **Access ID**, **Access Secret**, **data centre region**, and the
   **device IDs** of the lights you want controlled

⚠️ Don't paste the secret into chat — put it in a file I can read, or set it as
an env var and tell me the name. I'll wire it to Keychain like the API key.

### 5. Android phone — alarm + phone layer
- An Android device (the alarm needs a native app; a web app **cannot**
  reliably wake a sleeping phone)
- USB debugging enabled, or a way to sideload
- Tell me the Android version

### 6. *Highway to Hell* — a local copy
For the wake-up sequence. A file you own, kept local — don't stream it. Drop it
at `~/Projects/jarvis/assets/wake.mp3` (gitignored) and tell me.

### 7. Picovoice key — proper wake word
Free tier at [console.picovoice.ai](https://console.picovoice.ai). Fixes
"travis" false-positives.

### 8. Integrations — one account each, only if you want them
Google Calendar · Gmail · Notion (post-rotation) · Spotify · Home Assistant ·
weather. Each needs its own OAuth or key. Tell me which you actually want; I'd
skip the ones you won't use.

### 9. Apple Developer account — only for distribution
$99/yr. Needed **only** if someone other than you runs this without hitting
Gatekeeper. Not needed for your own machine.

---

## ⚪ Decisions I need from you (no accounts, just a call)

- **Native look** — perf forced flattening the glow and cut-corner style, and
  you said it didn't look right. Do you want me to try again, or is smooth-but-
  flat acceptable?
- **Autostart** — plugin is wired but off. Launch JARVIS at login?
- **Public demo behaviour** — pressing the core now opens the mic instead of
  showing a Julie quote. On GitHub Pages that means your uncle gets a "needs
  the desktop app" toast. Keep, or restore the quote for the browser demo?
- **Local model** — keep the Ollama path warm, or drop it until you're on
  Apple Silicon? On this Mac it's 12.5s/reply and it fabricates details.
- **Which integrations** from #8 you actually care about.

---

## ✅ Done

- ✅ Install `gh` and authenticate
- ✅ Grant microphone + speech-recognition permission
- ✅ Confirm the Tauri window renders the HUD
- ✅ Approve the public repo + first push

---

*Last updated: 2026-08-11*
