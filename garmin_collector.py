#!/usr/bin/env python3
"""
J.A.R.V.I.S. — Garmin Collector / Energy Model (3.1)
=====================================================
Pulls today's body battery, stress, and sleep from Garmin Connect,
computes a rule-based hourly energy curve (night-owl profile),
and writes energy_state + energy_forecast into the spine (jarvis.db).

The Tauri app never runs this — it just reads what this writes.
Run on a schedule (launchd) or manually.

First run: set GARMIN_EMAIL / GARMIN_PASSWORD env vars, it logs in once
and saves a token to ~/.garminconnect so later runs need no password.

Requires: Python 3.12+, `pip install garminconnect`
"""

import os
import sys
import sqlite3
import datetime as dt

# ---- CONFIG ---------------------------------------------------------------
# Path to the spine DB created by the Tauri app.
# Default matches macOS app-data location; override with JARVIS_DB env var.
DEFAULT_DB = os.path.expanduser(
    "~/Library/Application Support/com.hila.jarvis/jarvis.db"
)
DB_PATH = os.environ.get("JARVIS_DB", DEFAULT_DB)
TOKEN_DIR = os.path.expanduser("~/.garminconnect")

# Night-owl energy profile: relative energy multiplier by hour (0-23).
# Peaks late evening (21:00-00:30), trough early morning. Scaled by body battery.
# These are the "shape" of Zohar's day; body battery sets the overall height.
NIGHT_OWL_SHAPE = {
    0: 0.75, 1: 0.60, 2: 0.40, 3: 0.20, 4: 0.10, 5: 0.10,
    6: 0.15, 7: 0.25, 8: 0.35, 9: 0.45, 10: 0.55, 11: 0.62,
    12: 0.65, 13: 0.60, 14: 0.62, 15: 0.68, 16: 0.72, 17: 0.75,
    18: 0.80, 19: 0.85, 20: 0.90, 21: 0.98, 22: 1.00, 23: 0.90,
}
PEAK_HOURS = {21, 22, 23, 0}  # flagged is_peak for the planner


# ---- GARMIN PULL ----------------------------------------------------------
def fetch_garmin(today):
    """Return dict with body_battery, stress, sleep_quality, sleep_hours,
    or None values if unavailable. Never crashes the collector."""
    result = {
        "body_battery": None, "stress_level": None,
        "sleep_quality": None, "sleep_hours": None, "training_load": None,
    }
    try:
        from garminconnect import Garmin
    except ImportError:
        print("garminconnect not installed — run: pip install garminconnect")
        return result

    email = os.environ.get("GARMIN_EMAIL")
    password = os.environ.get("GARMIN_PASSWORD")
    try:
        api = Garmin(email, password)
        # Garth token auth: reuse saved token if present, else login fresh
        if os.path.isdir(TOKEN_DIR) and os.listdir(TOKEN_DIR):
            api.login(TOKEN_DIR)
        else:
            api.login()
            api.client.dump(TOKEN_DIR)
    except Exception as e:
        print(f"Garmin login failed: {e}")
        return result

    iso = today.isoformat()

    # Body Battery (0-100). Endpoint returns a daily report; take latest value.
    try:
        bb = api.get_body_battery(iso, iso)
        # structure: list of days, each with 'bodyBatteryValuesArray' [[ts, status, val], ...]
        vals = []
        for day in bb or []:
            for entry in day.get("bodyBatteryValuesArray", []) or []:
                if len(entry) >= 3 and entry[2] is not None:
                    vals.append(entry[2])
        if vals:
            result["body_battery"] = int(vals[-1])  # most recent reading
    except Exception as e:
        print(f"body battery pull failed: {e}")

    # Daily stress (0-100 avg)
    try:
        st = api.get_stress_data(iso)
        avg = st.get("avgStressLevel") if isinstance(st, dict) else None
        if avg is not None and avg >= 0:
            result["stress_level"] = int(avg)
    except Exception as e:
        print(f"stress pull failed: {e}")

    # Sleep: score (0-100) + duration
    try:
        sl = api.get_sleep_data(iso)
        dto = sl.get("dailySleepDTO", {}) if isinstance(sl, dict) else {}
        scores = dto.get("sleepScores", {}) or {}
        overall = scores.get("overall", {}) or {}
        if overall.get("value") is not None:
            result["sleep_quality"] = int(overall["value"])
        secs = dto.get("sleepTimeSeconds")
        if secs:
            result["sleep_hours"] = round(secs / 3600.0, 1)
    except Exception as e:
        print(f"sleep pull failed: {e}")

    return result


# ---- ENERGY CURVE ---------------------------------------------------------
def build_forecast(body_battery, sleep_quality):
    """Rule-based hourly curve. Shape = night-owl; height scaled by body
    battery (fallback 60) and dampened if sleep was bad."""
    height = body_battery if body_battery is not None else 60
    # bad sleep drags the whole curve down
    if sleep_quality is not None and sleep_quality < 40:
        height = int(height * 0.8)
    forecast = []
    for hour in range(24):
        energy = int(round(NIGHT_OWL_SHAPE[hour] * height))
        energy = max(0, min(100, energy))
        forecast.append((hour, energy, 1 if hour in PEAK_HOURS else 0))
    return forecast


def compute_ceiling_remaining(conn, today, base_ceiling=5.0):
    """5h/day study ceiling minus hours already spent in completed focus
    blocks today (from plan_blocks). Falls back to full ceiling."""
    try:
        cur = conn.cursor()
        rows = cur.execute(
            "SELECT start, end FROM plan_blocks "
            "WHERE date=? AND kind='focus' AND status='done'",
            (today.isoformat(),),
        ).fetchall()
        used = 0.0
        for start, end in rows:
            try:
                h1, m1 = map(int, start.split(":"))
                h2, m2 = map(int, end.split(":"))
                used += ((h2 * 60 + m2) - (h1 * 60 + m1)) / 60.0
            except Exception:
                pass
        return round(max(0.0, base_ceiling - used), 1)
    except Exception:
        return base_ceiling


# ---- WRITE TO SPINE -------------------------------------------------------
def write_spine(data, forecast, today):
    conn = sqlite3.connect(DB_PATH)
    cur = conn.cursor()
    ds = today.isoformat()

    ceiling = compute_ceiling_remaining(conn, today)

    # energy_state: upsert current rolled-up state
    cur.execute(
        """INSERT INTO energy_state
           (date, body_battery, sleep_quality, sleep_hours, stress_level,
            ceiling_remaining, training_load, updated_at)
           VALUES (?,?,?,?,?,?,?,datetime('now'))
           ON CONFLICT(date) DO UPDATE SET
             body_battery=excluded.body_battery,
             sleep_quality=excluded.sleep_quality,
             sleep_hours=excluded.sleep_hours,
             stress_level=excluded.stress_level,
             ceiling_remaining=excluded.ceiling_remaining,
             training_load=excluded.training_load,
             updated_at=datetime('now')""",
        (ds, data["body_battery"], data["sleep_quality"], data["sleep_hours"],
         data["stress_level"], ceiling, data["training_load"]),
    )

    # energy_forecast: clear today's rows, insert fresh curve
    cur.execute("DELETE FROM energy_forecast WHERE date=?", (ds,))
    cur.executemany(
        "INSERT INTO energy_forecast (date, hour, energy, is_peak) "
        "VALUES (?,?,?,?)",
        [(ds, h, e, p) for (h, e, p) in forecast],
    )

    # update color-state flag based on thresholds in settings
    def setting(key, default):
        r = cur.execute("SELECT value FROM settings WHERE key=?", (key,)).fetchone()
        return int(r[0]) if r else default

    red_stress = setting("red_stress_threshold", 75)
    gray_batt = setting("gray_battery_threshold", 25)
    gray_sleep = setting("gray_sleep_threshold", 30)

    stress = data["stress_level"]
    batt = data["body_battery"]
    sleep = data["sleep_quality"]

    color = "blue"
    if stress is not None and stress >= red_stress:
        color = "red"
    elif (batt is not None and batt <= gray_batt) or \
         (sleep is not None and sleep <= gray_sleep):
        color = "gray"

    cur.execute(
        "UPDATE state_flags SET value=?, updated_at=datetime('now') "
        "WHERE key='ui_color_state'", (color,))

    conn.commit()
    conn.close()
    return ceiling, color


# ---- MAIN -----------------------------------------------------------------
def main():
    if not os.path.exists(DB_PATH):
        print(f"Spine DB not found at:\n  {DB_PATH}")
        print("Set JARVIS_DB env var to the real path, or launch the app once.")
        sys.exit(1)

    today = dt.date.today()
    print(f"Collecting for {today} → {DB_PATH}")

    data = fetch_garmin(today)
    forecast = build_forecast(data["body_battery"], data["sleep_quality"])
    ceiling, color = write_spine(data, forecast, today)

    print("Wrote energy_state:")
    print(f"  body_battery : {data['body_battery']}")
    print(f"  stress_level : {data['stress_level']}")
    print(f"  sleep_quality: {data['sleep_quality']}")
    print(f"  sleep_hours  : {data['sleep_hours']}")
    print(f"  ceiling_rem  : {ceiling} h")
    print(f"  ui_color     : {color}")
    peak = [h for (h, e, p) in forecast if p]
    print(f"  forecast     : 24h curve written, peak hours {peak}")


if __name__ == "__main__":
    main()
