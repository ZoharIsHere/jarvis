// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, WindowEvent};
use tauri_plugin_sql::{Builder as SqlBuilder, Migration, MigrationKind};

#[derive(serde::Deserialize)]
struct AnthropicBlock {
    #[serde(default)]
    text: String,
}
#[derive(serde::Deserialize)]
struct AnthropicResponse {
    #[serde(default)]
    content: Vec<AnthropicBlock>,
}

// Reused across calls so the TLS handshake and connection pool survive between
// questions instead of being rebuilt every time.
static HTTP: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();

/// Rolling conversation history so JARVIS can follow a thread across turns
/// instead of treating every utterance as a cold start.
static HISTORY: std::sync::Mutex<Vec<serde_json::Value>> = std::sync::Mutex::new(Vec::new());
/// Keeps the prompt bounded — spoken exchanges are short, so this is generous.
const MAX_TURNS: usize = 20;

const PERSONA: &str = "You are JARVIS, Zohar's personal AI assistant — calm, precise, \
lightly witty, a British butler who is also a close friend. Your replies are spoken \
aloud, so keep them short and conversational: 1-3 sentences, no markdown, no lists, \
no emoji. Never read out raw numbers robotically; speak naturally. If you genuinely \
don't know something, say so plainly.";

// ---- secrets ---------------------------------------------------------------
// Env vars die with the shell, which meant re-exporting the key on every
// launch. These read the login Keychain instead, so a secret is stored once.
// Env still wins when set, so a one-off override is still possible.

const KEYCHAIN_ACCOUNT: &str = "jarvis";

/// Read a secret: environment first, then the login Keychain.
fn secret(name: &str) -> Option<String> {
    if let Ok(v) = std::env::var(name) {
        if !v.trim().is_empty() {
            return Some(v);
        }
    }
    let out = Command::new("security")
        .args([
            "find-generic-password",
            "-a",
            KEYCHAIN_ACCOUNT,
            "-s",
            name,
            "-w",
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let v = String::from_utf8(out.stdout).ok()?.trim().to_string();
    if v.is_empty() {
        None
    } else {
        Some(v)
    }
}

/// Store a secret in the login Keychain. `-U` updates an existing entry.
///
/// The value is passed as a separate argument, never interpolated into a
/// shell string, so it can't be re-interpreted. It will briefly be visible
/// in this process's argv — acceptable for a single-user desktop app, and
/// noted here so it isn't mistaken for a stronger guarantee than it is.
#[tauri::command]
fn set_secret(name: String, value: String) -> Result<(), String> {
    if name.trim().is_empty() || value.trim().is_empty() {
        return Err("name and value are required".into());
    }
    let status = Command::new("security")
        .args([
            "add-generic-password",
            "-a",
            KEYCHAIN_ACCOUNT,
            "-s",
            &name,
            "-w",
            &value,
            "-U",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| format!("keychain write failed: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err("keychain rejected the write".into())
    }
}

/// Which secrets exist, without ever returning their values.
#[tauri::command]
fn secret_status() -> serde_json::Value {
    let names = ["ANTHROPIC_API_KEY", "TUYA_ACCESS_ID", "TUYA_ACCESS_SECRET"];
    let mut out = serde_json::Map::new();
    for n in names {
        let from_env = std::env::var(n).map(|v| !v.trim().is_empty()).unwrap_or(false);
        let present = from_env || secret(n).is_some();
        out.insert(
            n.to_string(),
            serde_json::json!({ "present": present, "source": if from_env { "env" } else if present { "keychain" } else { "none" } }),
        );
    }
    serde_json::Value::Object(out)
}

/// Where JARVIS keeps durable notes. Deliberately the OpenClaw-compatible layout
/// from the project's PRD (USER.md / MEMORY.md / memory/YYYY-MM-DD.md) so that
/// migrating the brain later doesn't mean rewriting memory.
fn memory_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(home).join("Library/Application Support/com.hila.jarvis")
}

fn read_memory_files() -> String {
    let dir = memory_dir();
    let mut out = String::new();
    for name in ["USER.md", "MEMORY.md"] {
        if let Ok(body) = std::fs::read_to_string(dir.join(name)) {
            let body = body.trim();
            if !body.is_empty() {
                out.push_str(&format!("\n\n## {name}\n{body}"));
            }
        }
    }
    out
}

/// Append a durable fact to MEMORY.md.
#[tauri::command]
fn remember(fact: String) -> Result<String, String> {
    let fact = fact.trim();
    if fact.is_empty() {
        return Err("nothing to remember".into());
    }
    let dir = memory_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("could not create memory dir: {e}"))?;
    let path = dir.join("MEMORY.md");

    let mut body = std::fs::read_to_string(&path).unwrap_or_else(|_| "# Memory\n".to_string());
    if !body.ends_with('\n') {
        body.push('\n');
    }
    body.push_str(&format!("- {fact}\n"));
    std::fs::write(&path, body).map_err(|e| format!("could not write memory: {e}"))?;
    Ok(fact.to_string())
}

/// Clear the in-memory conversation thread (durable MEMORY.md is untouched).
#[tauri::command]
fn reset_conversation() {
    if let Ok(mut h) = HISTORY.lock() {
        h.clear();
    }
}

// Calls the Anthropic API from the Rust side so the API key never touches the
// webview or the public hud/index.html — it's read from an env var only.
//
// `context` is a live snapshot of the spine (energy, habits, deadlines) built by
// the frontend, which already holds the DB connection.
/// `tier` comes from the frontend's cheap classifier:
///   1 — simple general question  → small local model, minimal prompt
///   2 — needs some reasoning     → mid local model, minimal prompt
///   3 — needs his actual data, or wants something done → cloud
///
/// Tiers 1-2 deliberately get **no memory and no spine context**. That isn't a
/// shortcut: feeding a sub-2B model his energy readings is precisely what made
/// it invent a study session in benchmarking, and the smaller prompt is also
/// what takes these from ~12s to ~3.5s.
#[tauri::command]
async fn ask_jarvis(
    prompt: String,
    context: Option<String>,
    tier: Option<u8>,
    models: Option<Vec<String>>,
) -> Result<serde_json::Value, String> {
    let tier = tier.unwrap_or(3).clamp(1, 3);
    let local = tier < 3;

    // Prompt length dominates latency on local models — measured at ~2s of the
    // ~5.7s round trip for the full persona. Tier 1 therefore gets a compact
    // brief instead: it keeps the voice and the do-not-fabricate rule, which
    // are the only parts that change the answer, and drops the rest.
    let mut system = if local {
        "You are JARVIS, a calm, precise British butler. Answer in 1-2 short spoken \
         sentences, no markdown or lists. You cannot see his personal data here — never \
         invent details about his day, sleep, habits or schedule; say you need to check."
            .to_string()
    } else {
        PERSONA.to_string()
    };
    if local {
        // Tier 2 can afford a little more character without hurting latency much.
        if tier == 2 {
            system.push_str(" Take a moment to reason before answering.");
        }
    } else {
        let memories = read_memory_files();
        if !memories.is_empty() {
            system.push_str("\n\nWhat you know about Zohar (from your saved notes):");
            system.push_str(&memories);
        }
        if let Some(ctx) = context.as_deref().map(str::trim).filter(|c| !c.is_empty()) {
            system.push_str(&format!(
                "\n\nLive readings from the spine (his local database) right now:\n{ctx}\n\
                 Use these when he asks how he's doing or what to work on. Don't recite \
                 them unless asked."
            ));
        }
    }

    // Only the cloud tier carries conversation history — resending it to a
    // local model is most of its latency, and history is where the personal
    // details a small model would garble live.
    let messages = if local {
        vec![serde_json::json!({"role": "user", "content": prompt})]
    } else {
        let mut hist = HISTORY.lock().map_err(|_| "history lock poisoned".to_string())?;
        hist.push(serde_json::json!({"role": "user", "content": prompt}));
        if hist.len() > MAX_TURNS {
            let excess = hist.len() - MAX_TURNS;
            hist.drain(0..excess);
        }
        hist.clone()
    };

    // On any failure the just-added user turn is rolled back, otherwise the next
    // call would send two consecutive user messages and the API would reject it.
    fn rollback() {
        if let Ok(mut h) = HISTORY.lock() {
            h.pop();
        }
    }

    let started = std::time::Instant::now();
    let models = models.unwrap_or_default();
    let chosen_model = if local {
        models
            .get((tier as usize) - 1)
            .cloned()
            .unwrap_or_else(|| "qwen2.5:0.5b".to_string())
    } else {
        "claude-sonnet-5".to_string()
    };

    let result = if local {
        ask_ollama_model(&chosen_model, &system, &messages).await
    } else {
        ask_anthropic(&system, &messages).await
    };

    let text = match result {
        Ok(t) if !t.trim().is_empty() => t,
        Ok(_) => {
            if !local {
                rollback();
            }
            return Err("empty response".to_string());
        }
        Err(e) => {
            if !local {
                rollback();
            }
            return Err(e);
        }
    };

    if !local {
        if let Ok(mut hist) = HISTORY.lock() {
            hist.push(serde_json::json!({"role": "assistant", "content": text}));
        }
    }

    // Return the routing outcome alongside the answer so the frontend can log
    // what actually happened rather than what it intended.
    Ok(serde_json::json!({
        "text": text,
        "tier": tier,
        "provider": if local { "ollama" } else { "anthropic" },
        "model": chosen_model,
        "latency_ms": started.elapsed().as_millis() as u64,
    }))
}

/// Which brain answers. Defaults to Anthropic; `JARVIS_LLM=ollama` switches to a
/// local model. The PRD's rule is that local stays the privacy-safe path and
/// cloud sits behind a switch — never the other way round.
enum Provider {
    Anthropic,
    Ollama,
}

fn provider() -> Provider {
    match std::env::var("JARVIS_LLM").as_deref().map(str::trim) {
        Ok("ollama") | Ok("local") => Provider::Ollama,
        _ => Provider::Anthropic,
    }
}

/// Reports the active brain so the HUD can show it.
#[tauri::command]
fn llm_status() -> serde_json::Value {
    match provider() {
        Provider::Ollama => serde_json::json!({
            "provider": "ollama",
            "model": ollama_model(),
            "ready": true,
        }),
        Provider::Anthropic => serde_json::json!({
            "provider": "anthropic",
            "model": "claude-sonnet-5",
            "ready": secret("ANTHROPIC_API_KEY").is_some(),
        }),
    }
}

fn ollama_model() -> String {
    std::env::var("JARVIS_OLLAMA_MODEL").unwrap_or_else(|_| "llama3.2:1b".to_string())
}

fn ollama_url() -> String {
    std::env::var("JARVIS_OLLAMA_URL").unwrap_or_else(|_| "http://127.0.0.1:11434".to_string())
}

async fn ask_anthropic(system: &str, messages: &[serde_json::Value]) -> Result<String, String> {
    let key = secret("ANTHROPIC_API_KEY").ok_or_else(|| {
        "No Anthropic API key. Say \"save my API key\" to store it in the Keychain, \
         or export ANTHROPIC_API_KEY before launching."
            .to_string()
    })?;

    let body = serde_json::json!({
        "model": "claude-sonnet-5",
        "max_tokens": 300,
        "system": system,
        "messages": messages,
    });

    let resp = HTTP
        .get_or_init(reqwest::Client::new)
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let detail = resp
            .json::<serde_json::Value>()
            .await
            .ok()
            .and_then(|v| {
                v.get("error")
                    .and_then(|e| e.get("message"))
                    .and_then(|m| m.as_str())
                    .map(str::to_string)
            })
            .unwrap_or_else(|| status.to_string());
        return Err(detail);
    }

    let parsed: AnthropicResponse = resp
        .json()
        .await
        .map_err(|e| format!("parse failed: {e}"))?;
    Ok(parsed.content.into_iter().map(|b| b.text).collect())
}

/// Local model via Ollama's chat API. Ollama takes the system prompt as a
/// message rather than a separate field.
/// Start the local model server if it isn't already up.
///
/// Deliberately started on demand rather than via a LaunchAgent: the app
/// already owns its own scheduling, and a second scheduler is how you end up
/// with two things racing to do the same job.
fn ensure_ollama() -> bool {
    // Cheap liveness check first — nothing to do if it's already serving.
    let up = |()| {
        std::net::TcpStream::connect_timeout(
            &"127.0.0.1:11434".parse().unwrap(),
            std::time::Duration::from_millis(300),
        )
        .is_ok()
    };
    if up(()) {
        return true;
    }

    let candidates = [
        dirs_home().join("Applications/Ollama.app/Contents/Resources/ollama"),
        PathBuf::from("/Applications/Ollama.app/Contents/Resources/ollama"),
        PathBuf::from("/usr/local/bin/ollama"),
        PathBuf::from("/opt/homebrew/bin/ollama"),
    ];
    let Some(bin) = candidates.into_iter().find(|p| p.exists()) else {
        return false;
    };

    if Command::new(bin)
        .arg("serve")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .is_err()
    {
        return false;
    }

    // Model load is lazy, so we only wait for the socket, not for readiness.
    for _ in 0..40 {
        std::thread::sleep(std::time::Duration::from_millis(250));
        if up(()) {
            return true;
        }
    }
    false
}

fn dirs_home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/tmp".into()))
}

async fn ask_ollama_model(
    model: &str,
    system: &str,
    messages: &[serde_json::Value],
) -> Result<String, String> {
    // Starting the server is blocking, so keep it off the async runtime.
    let started = tauri::async_runtime::spawn_blocking(ensure_ollama)
        .await
        .unwrap_or(false);
    if !started {
        return Err(
            "local model server unavailable — install Ollama or set llm_local_enabled=0".into(),
        );
    }

    let mut msgs = vec![serde_json::json!({"role": "system", "content": system})];
    msgs.extend_from_slice(messages);

    let body = serde_json::json!({
        "model": model,
        "messages": msgs,
        "stream": false,
        "options": { "num_predict": 200 },
    });

    let resp = HTTP
        .get_or_init(reqwest::Client::new)
        .post(format!("{}/api/chat", ollama_url()))
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("ollama unreachable ({e}) — is `ollama serve` running?"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("ollama error {status}: {body}"));
    }

    let v: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("ollama parse failed: {e}"))?;
    Ok(v.get("message")
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .unwrap_or("")
        .to_string())
}

// The speech helper ships next to the app binary (Tauri strips the target-triple
// suffix when bundling). Falls back to the un-stripped name and the source bin/
// dir so `tauri dev` works too.
fn speech_helper_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let arch = if cfg!(target_arch = "aarch64") {
        "aarch64-apple-darwin"
    } else {
        "x86_64-apple-darwin"
    };
    let candidates = [
        dir.join("jarvis-listen"),
        dir.join(format!("jarvis-listen-{arch}")),
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("bin/jarvis-listen-{arch}")),
    ];
    candidates.into_iter().find(|p| p.exists())
}

/// Capture one utterance from the microphone and return the transcript.
///
/// Uses a native AVFoundation/SFSpeechRecognizer helper rather than the
/// webview's Web Speech API, which never reaches macOS's permission system
/// (tauri-apps/wry#1195). Partial transcripts stream to the UI as
/// `jarvis:partial` events while the user is still speaking.
/// The in-flight speech helper, so a listen can be cancelled.
static LISTENER: std::sync::Mutex<Option<std::process::Child>> = std::sync::Mutex::new(None);

/// Stop an in-progress listen. Pressing the core while it's already listening
/// used to be a silent no-op, which read as the app being dead.
#[tauri::command]
fn cancel_listening() {
    if let Ok(mut guard) = LISTENER.lock() {
        if let Some(mut child) = guard.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[tauri::command]
async fn listen_once(app: AppHandle) -> Result<String, String> {
    let helper = speech_helper_path()
        .ok_or_else(|| "speech helper not found — rebuild the app".to_string())?;

    // Never run two listeners at once — they'd fight over the microphone.
    cancel_listening();

    tauri::async_runtime::spawn_blocking(move || {
        let mut child = Command::new(&helper)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("could not start speech helper: {e}"))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "speech helper produced no output".to_string())?;

        if let Ok(mut guard) = LISTENER.lock() {
            *guard = Some(child);
        }

        let mut final_text = String::new();
        let mut error: Option<String> = None;

        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            let Ok(msg) = serde_json::from_str::<serde_json::Value>(&line) else {
                continue;
            };
            let text = msg.get("text").and_then(|t| t.as_str()).unwrap_or("");
            match msg.get("type").and_then(|t| t.as_str()) {
                Some("ready") => {
                    let _ = app.emit("jarvis:ready", ());
                }
                Some("partial") => {
                    let _ = app.emit("jarvis:partial", text);
                }
                Some("final") => final_text = text.to_string(),
                Some("error") => {
                    error = Some(
                        msg.get("message")
                            .and_then(|m| m.as_str())
                            .unwrap_or("unknown speech error")
                            .to_string(),
                    )
                }
                _ => {}
            }
        }

        // The child is owned by LISTENER now; reap and clear it there so a
        // later cancel_listening() can't kill an unrelated process.
        if let Ok(mut guard) = LISTENER.lock() {
            if let Some(mut c) = guard.take() {
                let _ = c.wait();
            }
        }

        if let Some(e) = error {
            return Err(e);
        }
        let final_text = final_text.trim().to_string();
        if final_text.is_empty() {
            return Err("nothing heard".to_string());
        }
        Ok(final_text)
    })
    .await
    .map_err(|e| format!("speech task failed: {e}"))?
}

/// Hard mute for the always-on wake listener. The project's own architecture doc
/// calls this out as a requirement for running the mic continuously.
static WAKE_ON: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Start/stop hands-free wake-word listening ("Jarvis, ...").
///
/// SFSpeechRecognizer caps a single task at about a minute, so the helper runs
/// in bounded windows and is relaunched until muted.
#[tauri::command]
fn set_wake_listening(app: AppHandle, enabled: bool) -> Result<bool, String> {
    use std::sync::atomic::Ordering;

    if !enabled {
        WAKE_ON.store(false, Ordering::SeqCst);
        return Ok(false);
    }
    if WAKE_ON.swap(true, Ordering::SeqCst) {
        return Ok(true); // already running
    }
    let helper = speech_helper_path()
        .ok_or_else(|| "speech helper not found — rebuild the app".to_string())?;

    std::thread::spawn(move || {
        while WAKE_ON.load(Ordering::SeqCst) {
            let Ok(mut child) = Command::new(&helper)
                .arg("--wake")
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
            else {
                let _ = app.emit("jarvis:wake-error", "could not start speech helper");
                break;
            };

            let Some(stdout) = child.stdout.take() else {
                let _ = child.kill();
                break;
            };

            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if !WAKE_ON.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(msg) = serde_json::from_str::<serde_json::Value>(&line) else {
                    continue;
                };
                let text = msg.get("text").and_then(|t| t.as_str()).unwrap_or("");
                match msg.get("type").and_then(|t| t.as_str()) {
                    Some("partial") => {
                        let _ = app.emit("jarvis:partial", text);
                    }
                    Some("wake") => {
                        let _ = app.emit("jarvis:wake", text);
                    }
                    Some("error") => {
                        let m = msg.get("message").and_then(|m| m.as_str()).unwrap_or("");
                        let _ = app.emit("jarvis:wake-error", m);
                        WAKE_ON.store(false, Ordering::SeqCst);
                    }
                    _ => {}
                }
            }
            let _ = child.wait();
        }
        let _ = app.emit("jarvis:wake-stopped", ());
    });

    Ok(true)
}

/// The currently-speaking `say` process, so it can be interrupted.
static SPEAKING: std::sync::Mutex<Option<std::process::Child>> = std::sync::Mutex::new(None);

/// Stop JARVIS mid-sentence. Barge-in: the user shouldn't have to wait for
/// him to finish before talking back.
#[tauri::command]
fn stop_speaking() {
    if let Ok(mut guard) = SPEAKING.lock() {
        if let Some(mut child) = guard.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

// ---- Piper: local neural TTS ----------------------------------------------
// `say -v Daniel` is instant but audibly synthetic. Piper sounds far better,
// but a cold process costs ~2.8s (model load) against ~0.3s per sentence once
// warm — so it's kept alive between utterances rather than spawned per reply.
//
// Falls back to `say` whenever Piper isn't installed or misbehaves; the voice
// should degrade, never break.

struct Piper {
    child: std::process::Child,
    out_dir: PathBuf,
}
static PIPER: std::sync::Mutex<Option<Piper>> = std::sync::Mutex::new(None);

fn piper_model() -> Option<PathBuf> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()?
        .join("vendor/en_GB-alan-medium.onnx");
    p.exists().then_some(p)
}

/// Start (or reuse) the warm Piper process. Returns the directory it writes into.
fn piper_ensure() -> Option<PathBuf> {
    let mut guard = PIPER.lock().ok()?;

    // Reap a process that died since last time.
    if let Some(p) = guard.as_mut() {
        match p.child.try_wait() {
            Ok(None) => return Some(p.out_dir.clone()),
            _ => {
                *guard = None;
            }
        }
    }

    let model = piper_model()?;
    let out_dir = std::env::temp_dir().join("jarvis-piper");
    std::fs::create_dir_all(&out_dir).ok()?;

    let child = Command::new("python3")
        .arg("-m")
        .arg("piper")
        .arg("--model")
        .arg(&model)
        .arg("--output-dir")
        .arg(&out_dir)
        .arg("--output-dir-naming")
        .arg("timestamp")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    *guard = Some(Piper {
        child,
        out_dir: out_dir.clone(),
    });
    Some(out_dir)
}

fn wav_files(dir: &PathBuf) -> std::collections::HashSet<PathBuf> {
    std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().map(|x| x == "wav").unwrap_or(false))
                .collect()
        })
        .unwrap_or_default()
}

/// Synthesize one line with Piper and return the WAV it produced.
fn piper_say(text: &str) -> Option<PathBuf> {
    use std::io::Write;
    let out_dir = piper_ensure()?;
    let before = wav_files(&out_dir);

    {
        let mut guard = PIPER.lock().ok()?;
        let p = guard.as_mut()?;
        let stdin = p.child.stdin.as_mut()?;
        // One utterance per line; newlines inside would split it into several.
        let line = text.replace('\n', " ");
        writeln!(stdin, "{line}").ok()?;
        stdin.flush().ok()?;
    }

    // Wait for the new file. Generous ceiling: a long reply on a slow CPU.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(60));
        let after = wav_files(&out_dir);
        if let Some(new) = after.difference(&before).next() {
            // Let the file finish being written before playing it.
            let mut last = 0u64;
            for _ in 0..50 {
                let size = std::fs::metadata(new).map(|m| m.len()).unwrap_or(0);
                if size > 0 && size == last {
                    break;
                }
                last = size;
                std::thread::sleep(std::time::Duration::from_millis(40));
            }
            return Some(new.clone());
        }
    }
    None
}

/// Speak text using the system voice. WKWebView's speechSynthesis is
/// unreliable in a packaged app, and macOS ships better voices anyway.
#[tauri::command]
async fn speak_native(text: String, voice: Option<String>) -> Result<(), String> {
    if text.trim().is_empty() {
        return Ok(());
    }
    // Whatever was being said is now stale — cut it off rather than queueing.
    stop_speaking();

    let voice = voice.unwrap_or_else(|| "Daniel".to_string());
    tauri::async_runtime::spawn_blocking(move || {
        // Prefer Piper; fall back to `say` if it isn't installed or stalls, so
        // a TTS problem degrades the voice rather than silencing him.
        let piper_wav = if std::env::var("JARVIS_TTS").as_deref() == Ok("say") {
            None
        } else {
            piper_say(&text)
        };

        let child = match &piper_wav {
            Some(wav) => Command::new("afplay")
                .arg(wav)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|e| format!("afplay failed: {e}"))?,
            None => Command::new("say")
                // Separate args, never through a shell, so text isn't interpreted.
                .args(["-v", &voice, "-r", "185", "--", &text])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|e| format!("say failed: {e}"))?,
        };

        if let Ok(mut guard) = SPEAKING.lock() {
            *guard = Some(child);
        }
        // Wait outside the lock so stop_speaking() can interrupt mid-sentence.
        loop {
            std::thread::sleep(std::time::Duration::from_millis(80));
            let mut guard = match SPEAKING.lock() {
                Ok(g) => g,
                Err(_) => break,
            };
            match guard.as_mut() {
                Some(c) => match c.try_wait() {
                    Ok(Some(_)) => {
                        *guard = None;
                        break;
                    }
                    Ok(None) => {}
                    Err(_) => {
                        *guard = None;
                        break;
                    }
                },
                None => break, // interrupted by stop_speaking()
            }
        }
        // Don't let synthesized audio pile up in temp.
        if let Some(wav) = piper_wav {
            let _ = std::fs::remove_file(wav);
        }
        Ok(())
    })
    .await
    .map_err(|e| format!("speech task failed: {e}"))?
}

// ---- ambient sensing -------------------------------------------------------
// JARVIS should know what's happening without being asked. Both of these read
// existing macOS surfaces rather than pulling in a dependency.

/// Seconds since the last keyboard/mouse input. Drives "you've been at this
/// for hours" and "are you even here" behavior.
#[tauri::command]
fn idle_seconds() -> u64 {
    let out = Command::new("ioreg")
        .args(["-c", "IOHIDSystem"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default();

    for line in out.lines() {
        if !line.contains("HIDIdleTime") {
            continue;
        }
        if let Some(raw) = line.rsplit('=').next() {
            // Value is in nanoseconds.
            if let Ok(ns) = raw.trim().trim_matches(|c: char| !c.is_ascii_digit()).parse::<u64>() {
                return ns / 1_000_000_000;
            }
        }
    }
    0
}

/// Name of the frontmost application, for app-usage tracking.
#[tauri::command]
fn frontmost_app() -> Option<String> {
    let out = Command::new("lsappinfo")
        .arg("front")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())?;
    let asn = out.trim();
    if asn.is_empty() {
        return None;
    }
    let info = Command::new("lsappinfo")
        .args(["info", "-only", "name", asn])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())?;
    // Shaped like: "LSDisplayName"="Safari"
    info.split('=')
        .nth(1)
        .map(|s| s.trim().trim_matches('"').to_string())
        .filter(|s| !s.is_empty())
}

/// Post a real macOS notification.
///
/// The HUD now hides rather than quits, so a nudge shown only inside the HUD
/// is a nudge nobody sees. This puts it in Notification Center instead.
#[tauri::command]
fn notify(title: String, body: String) -> Result<(), String> {
    // Passed as separate args and escaped, so neither string is interpreted
    // as AppleScript source.
    let esc = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    let script = format!(
        "display notification \"{}\" with title \"{}\"",
        esc(&body),
        esc(&title)
    );
    Command::new("osascript")
        .args(["-e", &script])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| format!("notify failed: {e}"))
        .map(|_| ())
}

fn spine_path() -> PathBuf {
    memory_dir().join("jarvis.db")
}

/// Copy the spine to a timestamped backup and prune old ones.
///
/// Everything JARVIS knows lives in one SQLite file that had no backup.
#[tauri::command]
fn backup_spine() -> Result<String, String> {
    let src = spine_path();
    if !src.exists() {
        return Err("spine not found".into());
    }
    let dir = memory_dir().join("backups");
    std::fs::create_dir_all(&dir).map_err(|e| format!("could not create backup dir: {e}"))?;

    // Date comes from the OS rather than being formatted by hand.
    let stamp = Command::new("date")
        .args(["+%Y%m%d-%H%M%S"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "backup".to_string());

    let dest = dir.join(format!("jarvis-{stamp}.db"));
    // sqlite3 .backup would be safer mid-write, but a plain copy is adequate
    // here: writes are short and this runs at 03:00.
    std::fs::copy(&src, &dest).map_err(|e| format!("backup failed: {e}"))?;

    // Keep the 14 most recent.
    if let Ok(entries) = std::fs::read_dir(&dir) {
        let mut files: Vec<_> = entries
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with("jarvis-"))
            .collect();
        files.sort_by_key(|e| e.file_name());
        while files.len() > 14 {
            let old = files.remove(0);
            let _ = std::fs::remove_file(old.path());
        }
    }
    Ok(dest.to_string_lossy().to_string())
}

/// Run the Garmin collector. Blocked on a rate limit and a pending password
/// rotation, so failure here is expected and reported rather than fatal.
#[tauri::command]
async fn run_garmin_collector() -> Result<String, String> {
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(|p| p.join("garmin_collector.py"))
        .ok_or_else(|| "could not locate garmin_collector.py".to_string())?;
    if !script.exists() {
        return Err(format!("not found: {}", script.display()));
    }
    tauri::async_runtime::spawn_blocking(move || {
        let out = Command::new("python3")
            .arg(&script)
            .output()
            .map_err(|e| format!("could not run collector: {e}"))?;
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        if out.status.success() {
            Ok(stdout.lines().last().unwrap_or("done").to_string())
        } else {
            Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
        }
    })
    .await
    .map_err(|e| format!("collector task failed: {e}"))?
}

/// What's actually wired up — turns "it's broken" into a specific answer.
#[tauri::command]
fn diagnostics() -> serde_json::Value {
    let spine = spine_path();
    let helper = speech_helper_path();
    serde_json::json!({
        "spine_path": spine.to_string_lossy(),
        "spine_exists": spine.exists(),
        "spine_bytes": std::fs::metadata(&spine).map(|m| m.len()).unwrap_or(0),
        "memory_user_md": memory_dir().join("USER.md").exists(),
        "memory_memory_md": memory_dir().join("MEMORY.md").exists(),
        "speech_helper": helper.as_ref().map(|p| p.to_string_lossy().to_string()),
        "speech_helper_found": helper.is_some(),
        "llm": llm_status(),
        "wake_listening": WAKE_ON.load(std::sync::atomic::Ordering::SeqCst),
        "locale": std::env::var("JARVIS_LOCALE").unwrap_or_else(|_| "en-US".into()),
    })
}

/// Show and focus the HUD from anywhere.
fn summon(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
        let _ = app.emit("jarvis:summoned", ());
    }
}

fn toggle_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        match w.is_visible() {
            Ok(true) => {
                let _ = w.hide();
            }
            _ => summon(app),
        }
    }
}

#[tauri::command]
fn hide_window(app: AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.hide();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(
            SqlBuilder::default()
                .add_migrations(
                    "sqlite:jarvis.db",
                    vec![
                        Migration {
                            version: 1,
                            description: "spine_001",
                            sql: include_str!("../migrations/001_spine.sql"),
                            kind: MigrationKind::Up,
                        },
                        Migration {
                            version: 2,
                            description: "habits_002",
                            sql: include_str!("../migrations/002_habits.sql"),
                            kind: MigrationKind::Up,
                        },
                        Migration {
                            version: 3,
                            description: "scheduler_003",
                            sql: include_str!("../migrations/003_scheduler.sql"),
                            kind: MigrationKind::Up,
                        },
                        Migration {
                            version: 4,
                            description: "llm_calls_004",
                            sql: include_str!("../migrations/004_llm_calls.sql"),
                            kind: MigrationKind::Up,
                        },
                    ],
                )
                .build(),
        )
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        // JARVIS is meant to be ambient, not an app you keep re-opening:
        // closing the window hides it, and the tray icon + global hotkey
        // summon it back from anywhere.
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .setup(|app| {
            let handle = app.handle().clone();

            // Launch at login. Scheduled jobs (morning briefing, Garmin sync)
            // only fire while the app is running, so autostart is what makes
            // "scheduled mode" actually scheduled.
            {
                use tauri_plugin_autostart::ManagerExt;
                let mgr = app.autolaunch();
                if !mgr.is_enabled().unwrap_or(false) {
                    if let Err(e) = mgr.enable() {
                        eprintln!("could not enable autostart: {e}");
                    }
                }
            }

            let show = MenuItem::with_id(app, "show", "Show JARVIS", true, None::<&str>)?;
            let hide = MenuItem::with_id(app, "hide", "Hide", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit JARVIS", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &hide, &quit])?;

            TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("JARVIS")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => summon(app),
                    "hide" => {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.hide();
                        }
                    }
                    "quit" => {
                        WAKE_ON.store(false, std::sync::atomic::Ordering::SeqCst);
                        stop_speaking();
                        app.exit(0);
                    }
                    _ => {}
                })
                // Left-clicking the menu bar icon toggles the HUD, like Spotlight.
                .on_tray_icon_event(|tray, event| {
                    if let tauri::tray::TrayIconEvent::Click {
                        button: tauri::tray::MouseButton::Left,
                        button_state: tauri::tray::MouseButtonState::Up,
                        ..
                    } = event
                    {
                        toggle_window(tray.app_handle());
                    }
                })
                .build(app)?;

            // Summon from anywhere without touching the mouse.
            {
                use tauri_plugin_global_shortcut::{
                    Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState,
                };
                let hotkey = Shortcut::new(Some(Modifiers::ALT), Code::Space);
                let h = handle.clone();
                if let Err(e) = app.global_shortcut().on_shortcut(hotkey, move |_, _, ev| {
                    if ev.state == ShortcutState::Pressed {
                        toggle_window(&h);
                    }
                }) {
                    // Another app may already own the combination — not fatal.
                    eprintln!("could not register ⌥Space: {e}");
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            ask_jarvis,
            listen_once,
            speak_native,
            remember,
            reset_conversation,
            set_wake_listening,
            llm_status,
            stop_speaking,
            idle_seconds,
            frontmost_app,
            hide_window,
            notify,
            backup_spine,
            run_garmin_collector,
            diagnostics,
            set_secret,
            secret_status,
            cancel_listening
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}