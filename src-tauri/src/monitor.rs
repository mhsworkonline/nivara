// Shared monitoring loop used by the background agent binary.
// Tracks foreground app sessions, idle state, periodic screenshots, and
// evaluates rules (log-only). Purely observational.

use rusqlite::Connection;
use std::time::Duration;

use crate::db;
use crate::dns;
use crate::rules;
use crate::winmon;

const POLL_SECS: u64 = 5;

struct Session {
    app_name: String,
    exe_path: String,
    window_title: String,
    started_at: i64,
    is_idle: bool,
}

pub fn log(msg: &str) {
    let line = format!("[{}] {}\n", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"), msg);
    print!("{}", line);
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(db::log_path())
    {
        let _ = f.write_all(line.as_bytes());
    }
}

fn category_of(conn: &Connection, app: &str) -> Option<String> {
    conn.query_row(
        "SELECT category FROM app_categories WHERE app_name = ?1",
        [app],
        |r| r.get(0),
    )
    .ok()
}

fn flush_session(conn: &Connection, s: &Session, ended_at: i64) {
    let dur = (ended_at - s.started_at).max(0);
    if dur <= 0 {
        return;
    }
    let _ = conn.execute(
        "INSERT INTO activity_log(app_name, exe_path, window_title, started_at, ended_at, duration_secs, is_idle)
         VALUES (?1,?2,?3,?4,?5,?6,?7)",
        (
            &s.app_name,
            &s.exe_path,
            &s.window_title,
            s.started_at,
            ended_at,
            dur,
            s.is_idle as i64,
        ),
    );
}

fn evaluate_rules(conn: &Connection, app: &str, title: &str, now: i64) {
    let cat = category_of(conn, app);
    if let Ok(rules) = rules::load_enabled(conn) {
        for rule in &rules {
            // Only monitor-mode (log-only) matches are handled here; enforced
            // rules act in enforce_rules() against the live process list / DNS.
            if !rules::is_enforced(rule) && rules::matches(rule, app, title, cat.as_deref()) {
                let _ = rules::log_would_block(conn, rule, app, title, now);
            }
        }
    }
}

/// Terminate app processes matching enforced app-block rules. Gated by the
/// global "enforcement_enabled" switch. User-level & bypassable.
fn enforce_app_rules(conn: &Connection, now: i64) {
    if db::get_setting_i64(conn, "enforcement_enabled", 0) != 1 {
        return;
    }
    let loaded = match rules::load_enabled(conn) {
        Ok(r) => r,
        Err(_) => return,
    };
    let app_rules: Vec<&rules::Rule> = loaded
        .iter()
        .filter(|r| {
            rules::is_enforced(r) && r.target_type == "app" && rules::schedule_active_now(r)
        })
        .collect();
    if app_rules.is_empty() {
        return;
    }
    for p in winmon::list_processes() {
        for rule in &app_rules {
            let target = rule.target.to_lowercase();
            let target = target.trim();
            if !target.is_empty() && p.name.contains(target) {
                if winmon::terminate_pid(p.pid) {
                    log(&format!("ENFORCE: terminated {} (pid {}) via rule#{}", p.name, p.pid, rule.id));
                    let msg = format!("blocked: terminated {} (rule#{})", p.name, rule.id);
                    let _ = rules::log_event(conn, rule, &p.name, "", now, &msg);
                }
                break;
            }
        }
    }
}

/// Normalize a website rule target to a bare domain (strip scheme, www., path).
fn normalize_domain(target: &str) -> String {
    target
        .to_lowercase()
        .trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_start_matches("www.")
        .split('/')
        .next()
        .unwrap_or("")
        .trim()
        .to_string()
}

/// Domains to sinkhole via DNS: every enabled website-block rule that is
/// on-schedule. Driven by "dns_filter_enabled" (independent of the app
/// "enforcement_enabled" switch). Read fresh from the rules table each call.
fn website_blocklist(conn: &Connection) -> Vec<String> {
    let mut domains = Vec::new();
    if db::get_setting_i64(conn, "dns_filter_enabled", 0) != 1 {
        return domains;
    }
    let loaded = match rules::load_enabled(conn) {
        Ok(r) => r,
        Err(_) => return domains,
    };
    for rule in &loaded {
        if rule.target_type == "website"
            && rule.action == "block"
            && rules::schedule_active_now(rule)
        {
            let d = normalize_domain(&rule.target);
            if !d.is_empty() {
                domains.push(d);
            }
        }
    }
    domains
}

pub fn run_loop() {
    winmon::set_dpi_aware();
    let conn = match db::open() {
        Ok(c) => c,
        Err(e) => {
            log(&format!("FATAL: cannot open db: {}", e));
            return;
        }
    };
    log("monitor started");
    let _ = db::set_setting(&conn, "monitor_should_run", "1");

    // Live DNS blocklist shared with the proxy thread. The proxy is started
    // lazily the first time dns_filter_enabled=1 is observed (so toggling it
    // while the monitor runs takes effect). Pointing the system resolver at
    // 127.0.0.1 is a separate, elevated step (commands::dns_filter_apply).
    let blocklist = dns::new_blocklist();
    let mut dns_started = false;

    let mut current: Option<Session> = None;
    let mut last_screenshot: i64 = 0;
    let mut screen_time = crate::screentime::Enforcer::new();

    loop {
        let now = db::now_epoch();
        let _ = db::set_setting(&conn, "monitor_heartbeat", &now.to_string());

        if db::get_setting_i64(&conn, "monitor_should_run", 1) == 0 {
            if let Some(s) = current.take() {
                flush_session(&conn, &s, now);
            }
            log("stop requested via settings; exiting");
            break;
        }

        // Lazily start the DNS proxy once filtering is enabled.
        if !dns_started && db::get_setting_i64(&conn, "dns_filter_enabled", 0) == 1 {
            let bl = blocklist.clone();
            let upstream = db::get_setting(&conn, "dns_upstream").unwrap_or_else(|| "1.1.1.1".into());
            std::thread::spawn(move || dns::serve(bl, upstream, |m| log(m)));
            dns_started = true;
        }

        let idle_threshold = db::get_setting_i64(&conn, "idle_threshold_secs", 120) as u64;
        let idle = winmon::idle_seconds() >= idle_threshold;

        let fg = winmon::foreground_info();
        let (app, exe, title) = match &fg {
            Some(f) if !idle => (f.app_name.clone(), f.exe_path.clone(), f.window_title.clone()),
            _ => ("Idle".to_string(), String::new(), String::new()),
        };

        // Session change detection: new app or idle-state flip.
        let changed = match &current {
            Some(s) => s.app_name != app || s.is_idle != idle,
            None => true,
        };
        if changed {
            if let Some(s) = current.take() {
                flush_session(&conn, &s, now);
            }
            current = Some(Session {
                app_name: app.clone(),
                exe_path: exe.clone(),
                window_title: title.clone(),
                started_at: now,
                is_idle: idle,
            });
        } else if let Some(s) = current.as_mut() {
            // Same app; keep latest window title.
            s.window_title = title.clone();
        }

        // Rule evaluation only for real (non-idle) foreground activity.
        if !idle && app != "Idle" {
            evaluate_rules(&conn, &app, &title, now);
        }

        // Enforcement runs every poll regardless of foreground/idle so blocked
        // apps are terminated even in the background. Website DNS blocklist is
        // refreshed from the live rules table (independent of app enforcement).
        enforce_app_rules(&conn, now);
        screen_time.tick(&conn, now, idle, &|m| log(m));
        let domains = website_blocklist(&conn);
        if let Ok(mut bl) = blocklist.lock() {
            if *bl != domains {
                log(&format!("dns blocklist updated: {:?}", domains));
                *bl = domains;
            }
        }

        // Screenshots.
        let shots_on = db::get_setting_i64(&conn, "screenshots_enabled", 1) == 1;
        let interval = db::get_setting_i64(&conn, "screenshot_interval_secs", 300).max(10);
        if shots_on && !idle && now - last_screenshot >= interval {
            capture_and_store(&conn, &app, &title, now);
            last_screenshot = now;
            prune_screenshots(&conn, now);
        }

        std::thread::sleep(Duration::from_secs(POLL_SECS));
    }
}

fn capture_and_store(conn: &Connection, app: &str, title: &str, now: i64) {
    match winmon::capture_screen_jpeg(70) {
        Some(bytes) => {
            let fname = format!("shot_{}.jpg", now);
            let path = db::screenshots_dir().join(&fname);
            if let Err(e) = std::fs::write(&path, &bytes) {
                log(&format!("screenshot write failed: {}", e));
                return;
            }
            let _ = conn.execute(
                "INSERT INTO screenshots(file_name, captured_at, app_name, window_title)
                 VALUES (?1,?2,?3,?4)",
                (&fname, now, app, title),
            );
            log(&format!("screenshot captured: {} ({} bytes)", fname, bytes.len()));
        }
        None => log("screenshot capture failed"),
    }
}

fn prune_screenshots(conn: &Connection, now: i64) {
    let days = db::get_setting_i64(conn, "screenshot_retention_days", 7).max(1);
    let cutoff = now - days * 86400;
    let mut stale: Vec<String> = Vec::new();
    if let Ok(mut stmt) =
        conn.prepare("SELECT file_name FROM screenshots WHERE captured_at < ?1")
    {
        if let Ok(rows) = stmt.query_map([cutoff], |r| r.get::<_, String>(0)) {
            for r in rows.flatten() {
                stale.push(r);
            }
        }
    }
    for f in &stale {
        let _ = std::fs::remove_file(db::screenshots_dir().join(f));
    }
    if !stale.is_empty() {
        let _ = conn.execute("DELETE FROM screenshots WHERE captured_at < ?1", [cutoff]);
        log(&format!("pruned {} old screenshots", stale.len()));
    }
}
