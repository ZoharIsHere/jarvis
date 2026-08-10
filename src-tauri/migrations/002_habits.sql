-- ============================================================
-- J.A.R.V.I.S. SPINE — migration 002
-- Habit tracking. Raw daily check-ins live here; streaks and the
-- weekly grid the HUD shows are derived from habit_log at read time,
-- same as energy_state/ceiling_remaining are derived elsewhere.
-- ============================================================

-- ---- habits: habit definitions ----
CREATE TABLE IF NOT EXISTS habits (
  id         INTEGER PRIMARY KEY AUTOINCREMENT,
  name       TEXT NOT NULL UNIQUE,
  sort_order INTEGER DEFAULT 0,
  active     INTEGER DEFAULT 1,
  created_at TEXT DEFAULT (datetime('now'))
);

-- ---- habit_log: one row per habit per day it was marked done/not ----
CREATE TABLE IF NOT EXISTS habit_log (
  id       INTEGER PRIMARY KEY AUTOINCREMENT,
  habit_id INTEGER NOT NULL REFERENCES habits(id),
  date     TEXT NOT NULL,
  done     INTEGER NOT NULL DEFAULT 0,
  UNIQUE(habit_id, date)
);
CREATE INDEX IF NOT EXISTS idx_habit_log_date ON habit_log(date);
CREATE INDEX IF NOT EXISTS idx_habit_log_habit ON habit_log(habit_id);

-- seed the six habits currently shown as mock data in hud/index.html
INSERT OR IGNORE INTO habits (name, sort_order) VALUES
  ('Run', 0),
  ('Study ≥2h', 1),
  ('Guitar', 2),
  ('Sleep by 2am', 3),
  ('Novel · 1 scene', 4),
  ('No phone in bed', 5);
