-- ============================================================
-- J.A.R.V.I.S. SPINE — migration 001
-- The single source of truth. Every system reads/writes here.
-- Idempotent: uses IF NOT EXISTS so re-running is safe.
-- ============================================================

-- ---- state_flags: live key-value toggles the whole app polls ----
CREATE TABLE IF NOT EXISTS state_flags (
  key        TEXT PRIMARY KEY,
  value      TEXT,
  updated_at TEXT DEFAULT (datetime('now'))
);

-- seed the flags every system expects to exist
INSERT OR IGNORE INTO state_flags (key, value) VALUES
  ('listening_enabled', '1'),
  ('ui_color_state', 'blue'),
  ('julie_authority', 'normal'),
  ('julie_authority_trigger', NULL),
  ('load_level', 'none'),
  ('active_focus_block_id', NULL),
  ('voice_engine', 'local');

-- ---- energy_forecast: today's hourly predicted curve ----
CREATE TABLE IF NOT EXISTS energy_forecast (
  id           INTEGER PRIMARY KEY AUTOINCREMENT,
  date         TEXT NOT NULL,
  hour         INTEGER NOT NULL,
  energy       INTEGER NOT NULL,
  is_peak      INTEGER DEFAULT 0,
  generated_at TEXT DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_forecast_date ON energy_forecast(date);

-- ---- energy_state: current rolled-up state (drives color) ----
CREATE TABLE IF NOT EXISTS energy_state (
  date              TEXT PRIMARY KEY,
  body_battery      INTEGER,
  sleep_quality     INTEGER,
  sleep_hours       REAL,
  stress_level      INTEGER,
  ceiling_remaining REAL DEFAULT 5.0,
  training_load     INTEGER,
  updated_at        TEXT DEFAULT (datetime('now'))
);

-- ---- energy_log: predicted vs actual, for learning ----
CREATE TABLE IF NOT EXISTS energy_log (
  id                INTEGER PRIMARY KEY AUTOINCREMENT,
  date              TEXT NOT NULL,
  hour              INTEGER NOT NULL,
  predicted_energy  INTEGER,
  actual_energy     INTEGER,
  note              TEXT
);

-- ---- deadlines ----
CREATE TABLE IF NOT EXISTS deadlines (
  id             INTEGER PRIMARY KEY AUTOINCREMENT,
  title          TEXT NOT NULL,
  due            TEXT NOT NULL,
  source         TEXT,
  source_uid     TEXT,
  size_estimate  REAL,
  severity       TEXT DEFAULT 'med',
  sessions_needed INTEGER DEFAULT 0,
  sessions_done  INTEGER DEFAULT 0,
  snoozed_until  TEXT,
  status         TEXT DEFAULT 'open'
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_deadline_uid ON deadlines(source, source_uid);

-- ---- tasks: the pool the planner draws from ----
CREATE TABLE IF NOT EXISTS tasks (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  title       TEXT NOT NULL,
  type        TEXT DEFAULT 'other',
  deadline_id INTEGER REFERENCES deadlines(id),
  size        REAL,
  first_move  TEXT,
  venue_pref  TEXT,
  status      TEXT DEFAULT 'pending',
  created_at  TEXT DEFAULT (datetime('now'))
);

-- ---- plan_blocks: today's hour-by-hour plan ----
CREATE TABLE IF NOT EXISTS plan_blocks (
  id           INTEGER PRIMARY KEY AUTOINCREMENT,
  date         TEXT NOT NULL,
  start        TEXT NOT NULL,
  end          TEXT NOT NULL,
  task_id      INTEGER REFERENCES tasks(id),
  label        TEXT,
  kind         TEXT DEFAULT 'focus',
  venue        TEXT,
  first_move   TEXT,
  status       TEXT DEFAULT 'scheduled',
  is_protected INTEGER DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_block_date ON plan_blocks(date);

-- ---- interventions: anti-burnout history ----
CREATE TABLE IF NOT EXISTS interventions (
  id             INTEGER PRIMARY KEY AUTOINCREMENT,
  ts             TEXT DEFAULT (datetime('now')),
  level          TEXT,
  trigger        TEXT,
  action         TEXT,
  authority_mode TEXT DEFAULT 'advisory',
  overridden     INTEGER DEFAULT 0
);

-- ---- checkins: self-reported fuel ----
CREATE TABLE IF NOT EXISTS checkins (
  id   INTEGER PRIMARY KEY AUTOINCREMENT,
  ts   TEXT DEFAULT (datetime('now')),
  fuel INTEGER
);

-- ---- app_usage: phone layer feed ----
CREATE TABLE IF NOT EXISTS app_usage (
  id           INTEGER PRIMARY KEY AUTOINCREMENT,
  date         TEXT NOT NULL,
  app          TEXT,
  minutes      INTEGER,
  during_focus INTEGER DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_usage_date ON app_usage(date);

-- ---- settings: misc config key-value ----
CREATE TABLE IF NOT EXISTS settings (
  key   TEXT PRIMARY KEY,
  value TEXT
);

-- seed tunable thresholds (so they're not hardcoded)
INSERT OR IGNORE INTO settings (key, value) VALUES
  ('red_stress_threshold', '75'),
  ('gray_battery_threshold', '25'),
  ('gray_sleep_threshold', '30'),
  ('danger_stress_threshold', '90'),
  ('danger_battery_threshold', '12'),
  ('study_ceiling_hours', '5'),
  ('clap_sensitivity', '0.6'),
  ('wakeword_sensitivity', '0.5'),
  ('song_path', ''),
  ('friday_dinner_start', '19:45'),
  ('friday_dinner_end', '21:15');
