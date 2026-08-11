-- ============================================================
-- J.A.R.V.I.S. SPINE — migration 004
-- Per-call latency/cost log.
--
-- Makes routing decisions measurable rather than assumed: which tier handled
-- a query, how long it took, whether it escalated, what it cost. This is the
-- data a router should tune itself on.
-- ============================================================

CREATE TABLE IF NOT EXISTS llm_calls (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,
  ts            TEXT DEFAULT (datetime('now')),
  tier          INTEGER NOT NULL,      -- 1 local-small | 2 local-mid | 3 cloud
  provider      TEXT NOT NULL,         -- ollama | anthropic
  model         TEXT,
  latency_ms    INTEGER,
  input_tokens  INTEGER,
  output_tokens INTEGER,
  cost_usd      REAL DEFAULT 0,        -- 0 for local
  escalated     INTEGER DEFAULT 0,     -- tier was raised after a poor answer
  ok            INTEGER DEFAULT 1,
  note          TEXT
);
CREATE INDEX IF NOT EXISTS idx_llm_calls_ts ON llm_calls(ts);

-- Tunable without a rebuild, like every other threshold in this app.
INSERT OR IGNORE INTO settings (key, value) VALUES
  ('llm_tier1_model',   'qwen2.5:0.5b'),
  ('llm_tier2_model',   'qwen2.5:1.5b'),
  ('llm_local_enabled', '1'),
  -- Hard ceiling so an agent loop can't quietly spend a fortune.
  ('llm_daily_budget_usd', '0.50'),
  -- Off by default: auto-reopening the mic after a reply made the next core
  -- press a silent no-op, which reads as the app having gone deaf.
  ('voice_continuous', '0');
