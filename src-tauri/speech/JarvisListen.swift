// JARVIS native speech listener.
//
// Exists because WKWebView's Web Speech API never reaches macOS's permission
// system (tauri-apps/wry#1195 — unresolved upstream), so the browser-side
// SpeechRecognition always fails with "not-allowed". Capturing audio natively
// via AVAudioEngine + SFSpeechRecognizer goes through AVFoundation instead,
// which prompts and works correctly.
//
// Prefers on-device recognition when available, so audio stays local.
//
// Protocol: emits one JSON object per line on stdout.
//   {"type":"ready"}
//   {"type":"partial","text":"..."}
//   {"type":"final","text":"..."}
//   {"type":"error","message":"..."}

import AVFoundation
import Foundation
import Speech

// --wake keeps listening and only reports once it hears the wake word, so the
// app can run hands-free. Without it, one utterance is captured and we exit.
let WAKE_MODE = CommandLine.arguments.contains("--wake")
let WAKE_WORDS = ["jarvis", "jervis", "travis"]  // common mishears of the wake word

let MAX_SECONDS = WAKE_MODE ? 55.0 : 20.0  // SFSpeechRecognizer caps a task ~1min
let SILENCE_TIMEOUT = 1.7

func emit(_ dict: [String: String]) {
    guard let data = try? JSONSerialization.data(withJSONObject: dict),
        let line = String(data: data, encoding: .utf8)
    else { return }
    print(line)
    fflush(stdout)
}

func fail(_ message: String) -> Never {
    emit(["type": "error", "message": message])
    exit(1)
}

// ---- permissions -----------------------------------------------------------
// Both are required: speech recognition AND microphone input.

var speechAuthorized = false
let speechSem = DispatchSemaphore(value: 0)
SFSpeechRecognizer.requestAuthorization { status in
    speechAuthorized = (status == .authorized)
    speechSem.signal()
}
speechSem.wait()
guard speechAuthorized else {
    fail("speech recognition permission denied — enable JARVIS under System Settings > Privacy & Security > Speech Recognition")
}

var micAuthorized = false
let micSem = DispatchSemaphore(value: 0)
AVCaptureDevice.requestAccess(for: .audio) { granted in
    micAuthorized = granted
    micSem.signal()
}
micSem.wait()
guard micAuthorized else {
    fail("microphone permission denied — enable JARVIS under System Settings > Privacy & Security > Microphone")
}

// ---- recognizer ------------------------------------------------------------

guard let recognizer = SFSpeechRecognizer(locale: Locale(identifier: "en-US")) else {
    fail("no speech recognizer for en-US")
}
guard recognizer.isAvailable else {
    fail("speech recognizer unavailable")
}

let request = SFSpeechAudioBufferRecognitionRequest()
request.shouldReportPartialResults = true
// Keep audio on the machine when the OS can do it locally.
if recognizer.supportsOnDeviceRecognition {
    request.requiresOnDeviceRecognition = true
}

let engine = AVAudioEngine()
let lock = NSLock()
var transcript = ""
var lastActivity = Date()
var finished = false

func finish(_ reason: String) {
    lock.lock()
    if finished {
        lock.unlock()
        return
    }
    finished = true
    let text = transcript
    lock.unlock()

    engine.inputNode.removeTap(onBus: 0)
    engine.stop()
    request.endAudio()

    if reason == "error" {
        exit(1)
    }
    if WAKE_MODE {
        // Only speak up if the wake word was actually said. Otherwise exit
        // quietly and let the parent restart us for another window.
        if let command = commandAfterWakeWord(text), !command.isEmpty {
            emit(["type": "wake", "text": command])
        } else {
            emit(["type": "idle", "text": ""])
        }
        exit(0)
    }
    emit(["type": "final", "text": text])
    exit(0)
}

// In wake mode, strip everything up to and including the wake word and report
// only the command that follows it.
func commandAfterWakeWord(_ text: String) -> String? {
    let lower = text.lowercased()
    for word in WAKE_WORDS {
        guard let range = lower.range(of: word) else { continue }
        let after = String(text[range.upperBound...])
            .trimmingCharacters(in: CharacterSet(charactersIn: " ,.!?—-"))
        return after
    }
    return nil
}

let task = recognizer.recognitionTask(with: request) { result, error in
    if let result = result {
        lock.lock()
        transcript = result.bestTranscription.formattedString
        lastActivity = Date()
        let snapshot = transcript
        lock.unlock()
        // In wake mode nothing is surfaced until the wake word lands, so idle
        // room chatter never shows up on screen.
        if WAKE_MODE {
            if let command = commandAfterWakeWord(snapshot) {
                emit(["type": "partial", "text": command])
            }
        } else {
            emit(["type": "partial", "text": snapshot])
        }
        if result.isFinal {
            finish("final")
        }
    }
    if let error = error {
        lock.lock()
        let hasText = !transcript.isEmpty
        lock.unlock()
        // A timeout/cancel after we already captured speech is a normal stop,
        // not a failure — hand back what we heard.
        if hasText {
            finish("final")
        } else {
            emit(["type": "error", "message": error.localizedDescription])
            finish("error")
        }
    }
}
_ = task

let input = engine.inputNode
let format = input.outputFormat(forBus: 0)
guard format.channelCount > 0 else {
    fail("no audio input device available")
}
input.installTap(onBus: 0, bufferSize: 1024, format: format) { buffer, _ in
    request.append(buffer)
}

engine.prepare()
do {
    try engine.start()
} catch {
    fail("could not start audio engine: \(error.localizedDescription)")
}

emit(["type": "ready"])

// ---- stop conditions -------------------------------------------------------
// Stop once the speaker goes quiet for SILENCE_TIMEOUT, or at MAX_SECONDS.

let started = Date()
DispatchQueue.global().async {
    while true {
        Thread.sleep(forTimeInterval: 0.15)
        lock.lock()
        let idle = Date().timeIntervalSince(lastActivity)
        let snapshot = transcript
        let done = finished
        lock.unlock()
        if done { return }
        // Push-to-talk stops as soon as the speaker pauses. Wake mode waits for
        // the wake word first, so a quiet room doesn't end the window early.
        let readyToStop =
            WAKE_MODE
            ? (commandAfterWakeWord(snapshot).map { !$0.isEmpty } ?? false)
            : !snapshot.isEmpty
        if readyToStop && idle > SILENCE_TIMEOUT {
            finish("silence")
            return
        }
        if Date().timeIntervalSince(started) > MAX_SECONDS {
            finish("timeout")
            return
        }
    }
}

RunLoop.main.run()
