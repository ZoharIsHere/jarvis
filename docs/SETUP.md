# Setup — step by step

Everything you need to do yourself, in order, with the exact commands.
Each step says what it unlocks, so you can stop at any point and still have a
working JARVIS.

**Nothing here is required to run him.** He already works: voice commands,
habit tracking, the planner, Julie's nudges, scheduled jobs. These steps add
capability on top.

---

## Step 0 — Run him (30 seconds)

```bash
cd ~/Projects/jarvis
./src-tauri/target/debug/bundle/macos/jarvis.app/Contents/MacOS/jarvis
```

macOS will ask for **Microphone** and **Speech Recognition** — click Allow for
both. If it doesn't ask, see [Troubleshooting](#troubleshooting).

You get a menu-bar icon. **⌥Space** summons him from any app; **Esc** hides
him; closing the window hides rather than quits.

> ⚠️ Always run the built `.app` above, never `npm run tauri dev` — the dev
> binary is unbundled and macOS refuses it microphone access.

**Works immediately, no setup:** "open focus" · "I ran today" · "I'm fried" ·
"plan my day" · "how was my week" · "JARVIS, status" · "remember that…" ·
"what was I doing yesterday" · "back up the spine"

---

## Step 1 — Rotate two credentials 🔴

The only genuinely urgent items. Both leaked outside this repo during
development (see the Security note in the README); neither was ever committed.

### 1a. Garmin password
1. [connect.garmin.com](https://connect.garmin.com) → Account Settings → change password
2. Then run the collector once so it caches a login token:
   ```bash
   cd ~/Projects/jarvis
   export GARMIN_EMAIL=your@email.com
   export GARMIN_PASSWORD='your-new-password'
   python3 garmin_collector.py
   ```
3. Expect `body_battery`, `sleep_quality` etc. to be **numbers, not None**.
   If you see a rate-limit error, wait a few hours and retry — see
   [Troubleshooting](#troubleshooting).

**Unlocks:** real vitals. Julie currently runs on self-reported check-ins
because `energy_state` is all NULL.

### 1b. Notion token
[notion.so/my-integrations](https://www.notion.so/my-integrations) → your
integration → **Refresh** the secret. Nothing in the app uses it yet; this is
purely closing the leak.

### 1c. Resolve the duplicate Garmin scheduler ⚠️
You have a `launchd` job that runs the collector at login, and JARVIS now runs
it every 4 hours. Both firing doubles your hit rate against the API that's
rate-limiting you.

```bash
launchctl unload ~/Library/LaunchAgents/com.hila.jarvis.garmin.plist
```

That leaves JARVIS as the only scheduler — it logs every run to `job_runs`,
backs off on failure, and reports status via *"JARVIS, status"*.

---

## Step 2 — Local brain ✅ already installed

Done for you. `~/Applications/Ollama.app` + 4.4GB of models in `~/.ollama`.
JARVIS starts the server on demand, so there's nothing to launch.

**Verify:** ask him *"what is a binary search tree"* — expect an answer in
~3.5s with a `LOCAL · qwen2.5:0.5b` tag in the transmission panel.

Tunable without a rebuild:
```sql
-- sqlite3 "~/Library/Application Support/com.hila.jarvis/jarvis.db"
UPDATE settings SET value='0'             WHERE key='llm_local_enabled';  -- disable local
UPDATE settings SET value='llama3.2:3b'   WHERE key='llm_tier2_model';    -- slower, better
```

---

## Step 3 — Anthropic API key (optional, ~$0.50/month) 🟠

Only needed for the things a small local model genuinely can't do: questions
about **your own data**, tool calling, and brain-dump parsing.

1. [console.anthropic.com](https://console.anthropic.com) → **API Keys** → **Create Key**
2. Add a payment method (pay-as-you-go, billed **separately** from any
   Claude.ai subscription — there's no way to route one through the other)
3. Say to JARVIS: **"save my API key"**, paste it into the prompt

It goes into the macOS Keychain. No exporting, ever. Don't paste it into a
chat — that puts it in a transcript.

**Cost control is already in place:** commands are free, tiers 1–2 are free
and local, and only tier 3 bills. `llm_daily_budget_usd` in `settings` is the
ceiling.

**Unlocks:** "what should I work on next" · tool calling · brain-dump → tasks ·
screen awareness · Sumie/James sub-workers.

---

## Step 4 — Smart lights (Tuya) 🔗

1. Sign up at [iot.tuya.com](https://iot.tuya.com) — free developer account
2. **Cloud** → **Create Cloud Project**. Pick the data centre matching your
   Smart Life app region (Europe / US / China / India)
3. In the project: **Devices** → **Link App Account** → scan the QR from
   Smart Life → *Me* → top-right scan icon
4. **Devices** → **All Devices** — note the **Device ID** of each light
5. **Overview** — note **Access ID** and **Access Secret**
6. Give me the Access ID, region, and device IDs. For the **secret**, run:
   ```bash
   security add-generic-password -a jarvis -s TUYA_ACCESS_SECRET -w 'your-secret' -U
   ```
   (Same Keychain JARVIS already uses — don't paste it in chat.)

**Unlocks:** voice light control, and the room reacting to your state — amber
on *smoke*, red on *fire*, dimmed when a focus block starts.

---

## Step 5 — Wake-up sequence 🎵

Put a copy of *Highway to Hell* you own at:
```
~/Projects/jarvis/assets/wake.mp3
```
(`assets/` is gitignored — keep it local, don't stream it.)

**Unlocks:** double-clap → music → spoken briefing (`JARVIS_MASTER.md` §3.5).

---

## Step 6 — Better wake word (optional) 🔗

Current detection string-matches the transcript and will false-positive on
"travis". [console.picovoice.ai](https://console.picovoice.ai) → free tier →
copy the AccessKey → tell me.

---

## Step 7 — Phone + alarm 📱

Needs an Android device. A web app **cannot** wake a sleeping phone, so the
alarm requires a native app, which requires the spine server first
(`ROADMAP.md` Phase 4 → 5).

Tell me: your Android version, and whether you can enable USB debugging.

---

## Troubleshooting

**Mic permission never prompts**
Every rebuild changes the ad-hoc signature and invalidates the grant — that's
expected. Reset and relaunch:
```bash
tccutil reset Microphone com.hila.jarvis
tccutil reset SpeechRecognition com.hila.jarvis
```

**Garmin says "rate limit" or "too many requests"**
Garmin throttles aggressively after failed logins. Wait several hours — don't
retry in a loop, that extends the block. Do Step 1c first so only one
scheduler is hitting it.

**"local model server unavailable"**
```bash
~/Applications/Ollama.app/Contents/Resources/ollama serve
```
If that works, JARVIS's auto-start didn't find the binary — tell me where it
lives.

**Voice sounds robotic again**
Piper failed and it fell back to `say`. Check:
```bash
cd ~/Projects/jarvis/vendor
echo "test" | python3 -m piper --model en_GB-alan-medium.onnx --output_file /tmp/t.wav
```

**Rebuilding**
```bash
cd ~/Projects/jarvis
export SDKROOT=$(xcrun --show-sdk-path)     # required: Intel Mac, CLT only
npm run tauri build -- --debug --bundles app
```
`--bundles app` skips DMG creation, which is most of the build time.

**Tests**
```bash
npm test
```

---

## What each step unlocks

| Step | Unlocks | Required? |
|---|---|---|
| 0 | Everything already built | ✅ yes |
| 1 | Real vitals; closes a credential leak | 🔴 do it |
| 2 | Free offline answers | ✅ done |
| 3 | Questions about your own life, tool calling | optional |
| 4 | Lights that react to your state | optional |
| 5 | Clap-to-wake with music | optional |
| 6 | Reliable wake word | optional |
| 7 | Phone alarm, usage tracking | optional |
