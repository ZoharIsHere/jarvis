use std::path::Path;
use std::process::Command;

// Builds the native speech helper (macOS only). See speech/JarvisListen.swift
// for why it exists. Kept non-fatal: if swiftc is unavailable the app still
// builds, voice input just reports the helper as missing at runtime.
#[cfg(target_os = "macos")]
fn build_speech_helper() {
    println!("cargo:rerun-if-changed=speech/JarvisListen.swift");

    let src = Path::new("speech/JarvisListen.swift");
    if !src.exists() {
        println!("cargo:warning=speech/JarvisListen.swift missing; voice input disabled");
        return;
    }

    let arch = if cfg!(target_arch = "aarch64") {
        "aarch64-apple-darwin"
    } else {
        "x86_64-apple-darwin"
    };
    let out = format!("bin/jarvis-listen-{arch}");

    if let Err(e) = std::fs::create_dir_all("bin") {
        println!("cargo:warning=could not create bin/: {e}");
        return;
    }

    let sdk = Command::new("xcrun")
        .args(["--show-sdk-path"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string());

    let mut cmd = Command::new("swiftc");
    cmd.arg("-O");
    if let Some(sdk) = &sdk {
        // The SDK's include dir is forced ahead of /usr/local/include, which on
        // some machines holds a stray Block.h (from an old liblzma) that
        // shadows the system one and breaks the Foundation module build.
        cmd.args(["-sdk", sdk]);
        cmd.args(["-Xcc", &format!("-I{sdk}/usr/include")]);
    }
    cmd.args(["-o", &out]).arg(src);

    match cmd.output() {
        Ok(o) if o.status.success() => {}
        Ok(o) => {
            let err = String::from_utf8_lossy(&o.stderr);
            let tail: Vec<&str> = err.lines().filter(|l| l.contains("error")).take(5).collect();
            println!("cargo:warning=speech helper build failed: {}", tail.join(" | "));
        }
        Err(e) => println!("cargo:warning=could not run swiftc: {e}"),
    }
}

#[cfg(not(target_os = "macos"))]
fn build_speech_helper() {}

fn main() {
    build_speech_helper();
    tauri_build::build()
}
