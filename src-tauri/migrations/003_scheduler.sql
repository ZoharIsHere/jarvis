-- ============================================================
-- J.A.R.V.I.S. SPINE — migration 003
-- Scheduled mode: the missing third of on-demand / scheduled / continuous.
-- Jobs live in the spine like everything else, so they survive restarts and
-- are configurable without a rebuild.
-- ============================================================

CREATE TABLE IF NOT EXISTS scheduled_jobs (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,
  name          TEXT NOT NULL UNIQUE,
  kind          TEXT NOT NULL,            -- garmin | weekly_review | morning_briefing | spine_backup
  -- schedule: 'daily' (at time_of_day), 'weekly' (weekday + time_of_day),
  --           'interval' (every interval_minutes)
  schedule      TEXT NOT NULL DEFAULT 'daily',
  time_of_day   TEXT,                     -- 'HH:MM' for daily/weekly
  weekday       INTEGER,                  -- 0=Mon … 6=Sun, for weekly
  interval_minutes INTEGER,               -- for interval
  enabled       INTEGER NOT NULL DEFAULT 1,
  last_run      TEXT,
  last_status   TEXT,
  created_at    TEXT DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_jobs_enabled ON scheduled_jobs(enabled);

-- Sensible defaults. Times are deliberately late — this is a night-owl profile.
INSERT OR IGNORE INTO scheduled_jobs (name, kind, schedule, time_of_day, weekday, interval_minutes) VALUES
  ('Garmin sync',       'garmin',           'interval', NULL,    NULL, 240),
  ('Morning briefing',  'morning_briefing', 'daily',    '09:30', NULL, NULL),
  ('Weekly review',     'weekly_review',    'weekly',   '20:00', 6,    NULL),
  ('Spine backup',      'spine_backup',     'daily',    '03:00', NULL, NULL);

-- Nothing was ever backing up the one file that holds everything.
CREATE TABLE IF NOT EXISTS job_runs (
  id       INTEGER PRIMARY KEY AUTOINCREMENT,
  job_id   INTEGER REFERENCES scheduled_jobs(id),
  ts       TEXT DEFAULT (datetime('now')),
  status   TEXT,
  detail   TEXT
);
CREATE INDEX IF NOT EXISTS idx_job_runs_job ON job_runs(job_id);
