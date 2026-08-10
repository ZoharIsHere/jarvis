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
#[tauri::command]
async fn ask_jarvis(prompt: String, context: Option<String>) -> Result<String, String> {
    let mut system = PERSONA.to_string();
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

    // History + this turn. Lock is released before the await.
    let messages = {
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

    let result = match provider() {
        Provider::Ollama => ask_ollama(&system, &messages).await,
        Provider::Anthropic => ask_anthropic(&system, &messages).await,
    };

    let text = match result {
        Ok(t) if !t.trim().is_empty() => t,
        Ok(_) => {
            rollback();
            return Err("empty response".to_string());
        }
        Err(e) => {
            rollback();
            return Err(e);
        }
    };

    if let Ok(mut hist) = HISTORY.lock() {
        hist.push(serde_json::json!({"role": "assistant", "content": text}));
    }
    Ok(text)
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
            "ready": std::env::var("ANTHROPIC_API_KEY").is_ok(),
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
    let key = std::env::var("ANTHROPIC_API_KEY")
        .map_err(|_| "ANTHROPIC_API_KEY not set — export it before launching the app".to_string())?;

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
async fn ask_ollama(system: &str, messages: &[serde_json::Value]) -> Result<String, String> {
    let mut msgs = vec![serde_json::json!({"role": "system", "content": system})];
    msgs.extend_from_slice(messages);

    let body = serde_json::json!({
        "model": ollama_model(),
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
#[tauri::command]
async fn listen_once(app: AppHandle) -> Result<String, String> {
    let helper = speech_helper_path()
        .ok_or_else(|| "speech helper not found — rebuild the app".to_string())?;

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

        let _ = child.wait();

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
        // Passed as separate args (never through a shell), so text is not interpreted.
        let child = Command::new("say")
            .args(["-v", &voice, "-r", "185", "--", &text])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("say failed: {e}"))?;

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
            hide_window
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}