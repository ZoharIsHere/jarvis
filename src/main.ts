import { invoke } from "@tauri-apps/api/core";
import Database from "@tauri-apps/plugin-sql";

// Force the spine DB to open + run migrations on startup.
async function initSpine() {
  try {
    const db = await Database.load("sqlite:jarvis.db");
    console.log("[jarvis] spine DB loaded:", db);
  } catch (err) {
    console.error("[jarvis] spine DB failed to load:", err);
  }
}

let greetInputEl: HTMLInputElement | null;
let greetMsgEl: HTMLElement | null;

async function greet() {
  if (greetMsgEl && greetInputEl) {
    greetMsgEl.textContent = await invoke("greet", {
      name: greetInputEl.value,
    });
  }
}

window.addEventListener("DOMContentLoaded", () => {
  initSpine();

  greetInputEl = document.querySelector("#greet-input");
  greetMsgEl = document.querySelector("#greet-msg");
  document.querySelector("#greet-form")?.addEventListener("submit", (e) => {
    e.preventDefault();
    greet();
  });
});