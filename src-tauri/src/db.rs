use rusqlite::Connection;
use std::path::PathBuf;
use std::time::Duration;

pub const DEFAULT_SETTINGS: &[(&str, &str)] = &[
    ("screenshot_interval_secs", "300"),
    ("screenshot_retention_days", "7"),
    ("screenshots_enabled", "1"),
    ("idle_threshold_secs", "120"),
    ("monitor_should_run", "1"),
    ("monitor_heartbeat", "0"),
    // Enforcement (user-level, bypassable — see CLAUDE.md).
    ("enforcement_enabled", "0"),
    ("dns_filter_enabled", "0"),
    ("dns_upstream", "1.1.1.1"),
    // Screen-time limit (see screentime.rs). 0 minutes = no daily limit.
    ("limit_enabled", "0"),
    ("limit_weekday_mins", "120"),
    ("limit_weekend_mins", "180"),
    ("bedtime_range", ""),
    ("limit_used_secs", "0"),
    ("limit_day", ""),
    ("limit_bonus_secs", "0"),
    ("limit_pass_until", "0"),
    ("limit_warned5_day", ""),
    ("limit_warned1_day", ""),
    // HKCU policy: disable Task Manager + cmd (speed bump only).
    ("restrict_taskmgr_cmd", "0"),
];

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS activity_log (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  app_name TEXT NOT NULL,
  exe_path TEXT,
  window_title TEXT,
  started_at INTEGER NOT NULL,
  ended_at INTEGER NOT NULL,
  duration_secs INTEGER NOT NULL DEFAULT 0,
  is_idle INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_activity_started ON activity_log(started_at);

CREATE TABLE IF NOT EXISTS screenshots (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  file_name TEXT NOT NULL,
  captured_at INTEGER NOT NULL,
  app_name TEXT,
  window_title TEXT
);
CREATE INDEX IF NOT EXISTS idx_screenshots_captured ON screenshots(captured_at);

CREATE TABLE IF NOT EXISTS app_categories (
  app_name TEXT PRIMARY KEY,
  category TEXT NOT NULL CHECK (category IN ('Study','Social','Entertainment','Other'))
);

CREATE TABLE IF NOT EXISTS rules (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  target TEXT NOT NULL,
  target_type TEXT NOT NULL,
  action TEXT NOT NULL,
  schedule TEXT,
  enabled INTEGER NOT NULL DEFAULT 1,
  enforcement_mode TEXT NOT NULL DEFAULT 'monitor'
);

CREATE TABLE IF NOT EXISTS rule_events (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  rule_id INTEGER NOT NULL,
  app_name TEXT,
  window_title TEXT,
  occurred_at INTEGER NOT NULL,
  message TEXT
);
CREATE INDEX IF NOT EXISTS idx_rule_events_occurred ON rule_events(occurred_at);

CREATE TABLE IF NOT EXISTS settings (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS auth (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  pw_hash TEXT NOT NULL,
  recovery_hash TEXT NOT NULL,
  created_at INTEGER NOT NULL
);
"#;

pub fn data_dir() -> PathBuf {
    std::env::var("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("Nivara")
}

pub fn screenshots_dir() -> PathBuf {
    data_dir().join("screenshots")
}

pub fn log_path() -> PathBuf {
    data_dir().join("monitor.log")
}

pub fn open() -> rusqlite::Result<Connection> {
    std::fs::create_dir_all(screenshots_dir()).ok();
    let conn = Connection::open(data_dir().join("nivara.db"))?;
    conn.busy_timeout(Duration::from_millis(5000))?;
    let _: String = conn.pragma_query_value(None, "journal_mode", |r| r.get(0))?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.execute_batch(SCHEMA)?;
    for (k, v) in DEFAULT_SETTINGS {
        conn.execute(
            "INSERT OR IGNORE INTO settings(key, value) VALUES (?1, ?2)",
            (k, v),
        )?;
    }
    Ok(conn)
}

pub fn get_setting(conn: &Connection, key: &str) -> Option<String> {
    conn.query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
        r.get(0)
    })
    .ok()
}

pub fn get_setting_i64(conn: &Connection, key: &str, default: i64) -> i64 {
    get_setting(conn, key)
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO settings(key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        (key, value),
    )?;
    Ok(())
}

pub fn now_epoch() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
