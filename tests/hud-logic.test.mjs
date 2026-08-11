// Tests for the pure logic inside hud/index.html.
//
// The HUD has to stay a single self-contained file (it is also the public
// GitHub Pages demo), so rather than restructuring it these tests pull the
// script out, stub the browser/Tauri surface, and exercise the functions that
// are pure enough to assert on.
//
//   node --test tests/
//
// No dependencies, no build step.

import { test } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

/** Load the HUD script with a minimal fake DOM, and return its internals. */
function loadHud() {
  const html = fs.readFileSync(path.join(ROOT, "hud/index.html"), "utf8");
  const src = html.match(/<script>([\s\S]*)<\/script>\s*<\/body>/)[1];

  // Deliberately permissive: any method the HUD reaches for is a no-op, so a
  // new DOM call in the HUD doesn't break these tests. We're asserting on
  // pure logic, not rendering.
  const el = () =>
    new Proxy(
      {
        textContent: "",
        innerHTML: "",
        className: "",
        value: "",
        style: {},
        dataset: {},
        classList: { add() {}, remove() {}, toggle() {}, contains: () => false },
      },
      {
        get(target, prop) {
          if (prop in target) return target[prop];
          if (prop === "children" || prop === "childNodes") return [];
          if (prop === "parentNode" || prop === "nextElementSibling") return null;
          // Everything else: a chainable no-op function.
          return () => el();
        },
        set(target, prop, value) {
          target[prop] = value;
          return true;
        },
      }
    );

  const doc = {
    body: { classList: { add() {}, remove() {}, toggle() {} }, dataset: {} },
    hidden: false,
    getElementById: () => el(),
    querySelector: () => el(),
    querySelectorAll: () => [],
    createElement: () => el(),
    createElementNS: () => el(),
    addEventListener() {},
  };

  const win = {
    document: doc,
    addEventListener() {},
    speechSynthesis: null,
    // No __TAURI__ — this is the browser/Pages path, which must stay mock-only.
    matchMedia: () => ({ matches: false, addEventListener() {} }),
  };

  const captured = {};
  const harness = `
    ${src}
    ;globalThis.__t = {
      habitKeywords, computeStreak, jobIsDue, fmtDate, mondayOf, esc, venueFor, hhmm, num, NATIVE,
      classifyDifficulty, navTarget, looksLikeQuestion
    };
  `;

  const fn = new Function(
    "window", "document", "setInterval", "setTimeout", "clearTimeout",
    "console", "navigator", "SpeechSynthesisUtterance", "Date", "globalThis",
    harness
  );

  fn(
    win, doc,
    () => 0, () => 0, () => {},
    { log() {}, error() {}, warn() {} },
    { language: "en-US" },
    function () {},
    Date,
    globalThis
  );
  return globalThis.__t;
}

const H = loadHud();

// --- the public demo must never light up native paths ----------------------

test("NATIVE is false without window.__TAURI__", () => {
  assert.equal(H.NATIVE, false);
});

// --- escaping: model and DB text reaches innerHTML ---------------------------

test("esc neutralizes HTML metacharacters", () => {
  assert.equal(H.esc('<img src=x onerror="alert(1)">'),
    "&lt;img src=x onerror=&quot;alert(1)&quot;&gt;");
  assert.equal(H.esc("a & b"), "a &amp; b");
  assert.equal(H.esc("it's"), "it&#39;s");
});

test("esc handles null and undefined without throwing", () => {
  assert.equal(H.esc(null), "");
  assert.equal(H.esc(undefined), "");
});

// --- habit matching ---------------------------------------------------------

test("habitKeywords drops noise words and short tokens", () => {
  assert.deepEqual(H.habitKeywords("No phone in bed"), ["phone", "bed"]);
  // Symbols and digits are stripped, so "Study ≥2h" is reachable by "study".
  assert.deepEqual(H.habitKeywords("Study ≥2h"), ["study"]);
  assert.deepEqual(H.habitKeywords("Novel · 1 scene"), ["novel", "scene"]);
});

// --- streaks ----------------------------------------------------------------

test("computeStreak counts consecutive days back from today", () => {
  const today = new Date(); today.setHours(0, 0, 0, 0);
  const day = (n) => {
    const d = new Date(today); d.setDate(d.getDate() - n);
    return H.fmtDate(d);
  };
  const log = { 7: { [day(0)]: true, [day(1)]: true, [day(2)]: true } };
  assert.equal(H.computeStreak(7, log), 3);
});

test("computeStreak is zero when today is unmarked", () => {
  const today = new Date(); today.setHours(0, 0, 0, 0);
  const yest = new Date(today); yest.setDate(yest.getDate() - 1);
  const log = { 1: { [H.fmtDate(yest)]: true } };
  assert.equal(H.computeStreak(1, log), 0);
});

test("computeStreak stops at the first gap", () => {
  const today = new Date(); today.setHours(0, 0, 0, 0);
  const day = (n) => {
    const d = new Date(today); d.setDate(d.getDate() - n);
    return H.fmtDate(d);
  };
  // Day 2 missing — the run before it must not be counted.
  const log = { 3: { [day(0)]: true, [day(1)]: true, [day(3)]: true, [day(4)]: true } };
  assert.equal(H.computeStreak(3, log), 2);
});

// --- date helpers -----------------------------------------------------------

test("fmtDate is zero-padded ISO, local time", () => {
  assert.equal(H.fmtDate(new Date(2026, 0, 5)), "2026-01-05");
  assert.equal(H.fmtDate(new Date(2026, 11, 31)), "2026-12-31");
});

test("mondayOf returns Monday for every day of the week", () => {
  // 2026-08-10 is a Monday.
  const monday = new Date(2026, 7, 10);
  for (let i = 0; i < 7; i++) {
    const d = new Date(2026, 7, 10 + i);
    assert.equal(H.fmtDate(H.mondayOf(d)), H.fmtDate(monday), `offset ${i}`);
  }
  // Sunday belongs to the week that started six days earlier, not the next one.
  assert.equal(H.fmtDate(H.mondayOf(new Date(2026, 7, 16))), "2026-08-10");
});

// --- scheduler --------------------------------------------------------------

test("interval jobs fire only after the interval elapses", () => {
  const now = new Date(2026, 7, 10, 12, 0, 0);
  const job = { enabled: 1, schedule: "interval", interval_minutes: 60 };
  assert.equal(H.jobIsDue({ ...job, last_run: null }, now), true, "never run");

  const recent = new Date(now.getTime() - 30 * 60000)
    .toISOString().replace("T", " ").slice(0, 19);
  assert.equal(H.jobIsDue({ ...job, last_run: recent }, now), false, "30m ago");

  const old = new Date(now.getTime() - 120 * 60000)
    .toISOString().replace("T", " ").slice(0, 19);
  assert.equal(H.jobIsDue({ ...job, last_run: old }, now), true, "2h ago");
});

test("daily jobs do not fire before their time", () => {
  const job = { enabled: 1, schedule: "daily", time_of_day: "09:30", last_run: null };
  assert.equal(H.jobIsDue(job, new Date(2026, 7, 10, 8, 0)), false);
  assert.equal(H.jobIsDue(job, new Date(2026, 7, 10, 10, 0)), true);
});

test("weekly jobs only fire on their weekday", () => {
  // weekday 6 = Sunday. 2026-08-16 is a Sunday, 2026-08-10 a Monday.
  const job = { enabled: 1, schedule: "weekly", time_of_day: "20:00", weekday: 6, last_run: null };
  assert.equal(H.jobIsDue(job, new Date(2026, 7, 10, 21, 0)), false, "Monday");
  assert.equal(H.jobIsDue(job, new Date(2026, 7, 16, 21, 0)), true, "Sunday");
});

test("disabled jobs never fire", () => {
  const job = { enabled: 0, schedule: "interval", interval_minutes: 1, last_run: null };
  assert.equal(H.jobIsDue(job, new Date()), false);
});

// --- planner helpers --------------------------------------------------------

test("hhmm zero-pads", () => {
  assert.equal(H.hhmm(9, 0), "09:00");
  assert.equal(H.hhmm(21, 50), "21:50");
});

test("venueFor escalates with energy, peak wins", () => {
  assert.match(H.venueFor(95, true), /deep work/);
  assert.equal(H.venueFor(70, false), "library");
  assert.match(H.venueFor(20, false), /light only/);
});

test("num falls back when a setting is missing or unparseable", () => {
  assert.equal(H.num({ a: "75" }, "a", 1), 75);
  assert.equal(H.num({}, "missing", 42), 42);
  assert.equal(H.num({ b: "abc" }, "b", 7), 7);
});

// --- difficulty router ------------------------------------------------------
//
// Tier 3 is the safety property: benchmarking showed sub-2B models answer
// general questions well but fabricate personal details when handed spine
// data. Anything about his life must never route local, so these are the
// tests that actually matter — a false negative here means JARVIS confidently
// inventing a study session that never happened.

test("anything personal routes to tier 3", () => {
  for (const q of [
    "how am I doing today",
    "what should I work on next",
    "how did I sleep",
    "is my energy good right now",
    "what's my deadline for the exam",
    "am I on track with my habits",
    "how's my streak looking",
    "what's on my schedule tonight",
  ]) {
    assert.equal(H.classifyDifficulty(q).tier, 3, q);
  }
});

test("action requests route to tier 3", () => {
  for (const q of [
    "add a task to review linked lists",
    "remind me to call the office",
    "snooze that deadline",
    "start a focus block",
    "delete the run habit",
  ]) {
    assert.equal(H.classifyDifficulty(q).tier, 3, q);
  }
});

test("simple impersonal questions route to tier 1", () => {
  for (const q of [
    "what is a binary search tree",
    "who wrote the Iliad",
    "what does recursion mean",
    "capital of France",
  ]) {
    assert.equal(H.classifyDifficulty(q).tier, 1, q);
  }
});

// Regression: bare "I"/"me" used to force tier 3, which sent almost every
// natural sentence to the cloud. With no API key that failed silently, so
// JARVIS appeared to transcribe the question and then ignore it.
test('"I" alone does not make a question personal', () => {
  for (const q of [
    "explain how I would implement a stack",
    "what is the best way for me to learn recursion",
    "can you tell me what a hash map is",
    "show me an example of a for loop",
  ]) {
    assert.ok(
      H.classifyDifficulty(q).tier < 3,
      `${q} → tier ${H.classifyDifficulty(q).tier} (should stay local)`
    );
  }
});

test("reasoning questions route to tier 2", () => {
  for (const q of [
    "why is quicksort faster than bubble sort",
    "compare arrays and linked lists",
    "explain the tradeoff between recursion and iteration",
  ]) {
    assert.equal(H.classifyDifficulty(q).tier, 2, q);
  }
});

test("long questions escalate past tier 1", () => {
  const long =
    "what is the difference between a stack and a queue and when would " +
    "you reach for one over the other in a typical program you might write";
  assert.ok(H.classifyDifficulty(long).tier >= 2);
});

test("classifier is case-insensitive", () => {
  assert.equal(H.classifyDifficulty("HOW AM I DOING TODAY").tier, 3);
  assert.equal(H.classifyDifficulty("What Is Recursion").tier, 1);
});

// --- navigation ------------------------------------------------------------
//
// Navigation was a raw substring scan, so ordinary words hijacked it:
// "recommend" contains "comm", "remain" contains "main", "homework" contains
// "home". The question was dropped and the page silently switched, which is
// indistinguishable from the app ignoring you. These are the regression tests.

test("ordinary sentences do not trigger navigation", () => {
  for (const q of [
    "can you recommend a good study method",
    "what is the most common sorting algorithm",
    "how much time does the domain transfer remain",
    "help me with my homework",
    "explain what a habitat is",
    "leave a comment on the pull request",
  ]) {
    assert.equal(H.navTarget(q), null, q);
  }
});

test("explicit navigation still works", () => {
  assert.equal(H.navTarget("open habits"), "habits");
  assert.equal(H.navTarget("go to the planner"), "planner");
  assert.equal(H.navTarget("show me projects"), "projects");
  assert.equal(H.navTarget("switch to comms"), "comms");
  assert.equal(H.navTarget("take me to the dashboard"), "dashboard");
});

test("bare page names still navigate", () => {
  assert.equal(H.navTarget("habits"), "habits");
  assert.equal(H.navTarget("focus"), "focus");
  assert.equal(H.navTarget("dashboard"), "dashboard");
});

test("questions are recognized as questions", () => {
  for (const q of [
    "what is a stack",
    "how do I center a div",
    "can you explain recursion",
    "is this right?",
    "tell me about binary trees",
  ]) {
    assert.ok(H.looksLikeQuestion(q), q);
  }
  for (const s of ["open habits", "mark run done", "I'm fried", "plan my day"]) {
    assert.ok(!H.looksLikeQuestion(s), s);
  }
});
