// Tauri IPC commands backing the parent dashboard. Read real data from SQLite.

use serde::Serialize;
use std::process::Command;

use crate::auth;
use crate::autostart;
use crate::db;
use crate::monitor;

fn conn() -> Result<rusqlite::Connection, String> {
    db::open().map_err(|e| e.to_string())
}

#[derive(Serialize)]
pub struct ActivityRow {
    id: i64,
    app_name: String,
    window_title: String,
    started_at: i64,
    ended_at: i64,
    duration_secs: i64,
    is_idle: bool,
    category: Option<String>,
}

#[tauri::command]
pub fn get_activity_timeline(from: i64, to: i64, limit: i64) -> Result<Vec<ActivityRow>, String> {
    let c = conn()?;
    let mut stmt = c
        .prepare(
            "SELECT a.id, a.app_name, a.window_title, a.started_at, a.ended_at,
                    a.duration_secs, a.is_idle, cat.category
             FROM activity_log a
             LEFT JOIN app_categories cat ON cat.app_name = a.app_name
             WHERE a.started_at >= ?1 AND a.started_at < ?2
             ORDER BY a.started_at DESC LIMIT ?3",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map((from, to, limit), |r| {
            Ok(ActivityRow {
                id: r.get(0)?,
                app_name: r.get(1)?,
                window_title: r.get(2)?,
                started_at: r.get(3)?,
                ended_at: r.get(4)?,
                duration_secs: r.get(5)?,
                is_idle: r.get::<_, i64>(6)? != 0,
                category: r.get(7)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
}

#[derive(Serialize)]
pub struct UsageRow {
    app_name: String,
    category: Option<String>,
    total_secs: i64,
}

#[tauri::command]
pub fn get_usage_summary(from: i64, to: i64) -> Result<Vec<UsageRow>, String> {
    let c = conn()?;
    let mut stmt = c
        .prepare(
            "SELECT a.app_name, cat.category, SUM(a.duration_secs) AS total
             FROM activity_log a
             LEFT JOIN app_categories cat ON cat.app_name = a.app_name
             WHERE a.started_at >= ?1 AND a.started_at < ?2 AND a.is_idle = 0
             GROUP BY a.app_name ORDER BY total DESC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map((from, to), |r| {
            Ok(UsageRow {
                app_name: r.get(0)?,
                category: r.get(1)?,
                total_secs: r.get(2)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
}

#[derive(Serialize)]
pub struct CategoryTotal {
    category: String,
    total_secs: i64,
}

#[derive(Serialize)]
pub struct Report {
    from: i64,
    to: i64,
    total_active_secs: i64,
    idle_secs: i64,
    by_category: Vec<CategoryTotal>,
    top_apps: Vec<UsageRow>,
}

fn build_report(from: i64, to: i64) -> Result<Report, String> {
    let c = conn()?;
    let total_active: i64 = c
        .query_row(
            "SELECT COALESCE(SUM(duration_secs),0) FROM activity_log
             WHERE started_at >= ?1 AND started_at < ?2 AND is_idle = 0",
            (from, to),
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    let idle: i64 = c
        .query_row(
            "SELECT COALESCE(SUM(duration_secs),0) FROM activity_log
             WHERE started_at >= ?1 AND started_at < ?2 AND is_idle = 1",
            (from, to),
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;

    let mut stmt = c
        .prepare(
            "SELECT COALESCE(cat.category,'Other') AS category, SUM(a.duration_secs)
             FROM activity_log a
             LEFT JOIN app_categories cat ON cat.app_name = a.app_name
             WHERE a.started_at >= ?1 AND a.started_at < ?2 AND a.is_idle = 0
             GROUP BY category ORDER BY 2 DESC",
        )
        .map_err(|e| e.to_string())?;
    let by_category = stmt
        .query_map((from, to), |r| {
            Ok(CategoryTotal {
                category: r.get(0)?,
                total_secs: r.get(1)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    let top_apps = get_usage_summary(from, to)?
        .into_iter()
        .take(10)
        .collect();

    Ok(Report {
        from,
        to,
        total_active_secs: total_active,
        idle_secs: idle,
        by_category,
        top_apps,
    })
}

#[tauri::command]
pub fn get_daily_report(day_start: i64) -> Result<Report, String> {
    build_report(day_start, day_start + 86400)
}

#[tauri::command]
pub fn get_weekly_report(week_start: i64) -> Result<Report, String> {
    build_report(week_start, week_start + 7 * 86400)
}

#[derive(Serialize)]
pub struct ScreenshotRow {
    id: i64,
    file_name: String,
    captured_at: i64,
    app_name: Option<String>,
    window_title: Option<String>,
}

#[tauri::command]
pub fn get_screenshots(from: i64, to: i64, limit: i64) -> Result<Vec<ScreenshotRow>, String> {
    let c = conn()?;
    let mut stmt = c
        .prepare(
            "SELECT id, file_name, captured_at, app_name, window_title
             FROM screenshots WHERE captured_at >= ?1 AND captured_at < ?2
             ORDER BY captured_at DESC LIMIT ?3",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map((from, to, limit), |r| {
            Ok(ScreenshotRow {
                id: r.get(0)?,
                file_name: r.get(1)?,
                captured_at: r.get(2)?,
                app_name: r.get(3)?,
                window_title: r.get(4)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
}

/// Return a screenshot as a base64 data URI for display in the webview.
#[tauri::command]
pub fn read_screenshot(file_name: String) -> Result<String, String> {
    // Guard against path traversal — only a bare file name is allowed.
    if file_name.contains('/') || file_name.contains('\\') || file_name.contains("..") {
        return Err("invalid file name".into());
    }
    let path = db::screenshots_dir().join(&file_name);
    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    Ok(format!("data:image/jpeg;base64,{}", base64_encode(&bytes)))
}

fn base64_encode(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | (b[2] as u32);
        out.push(T[((n >> 18) & 63) as usize] as char);
        out.push(T[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            T[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            T[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

#[derive(Serialize)]
pub struct CategoryRow {
    app_name: String,
    category: String,
}

#[tauri::command]
pub fn get_categories() -> Result<Vec<CategoryRow>, String> {
    let c = conn()?;
    let mut stmt = c
        .prepare("SELECT app_name, category FROM app_categories ORDER BY app_name")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok(CategoryRow {
                app_name: r.get(0)?,
                category: r.get(1)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_category(app_name: String, category: String) -> Result<(), String> {
    let valid = ["Study", "Social", "Entertainment", "Other"];
    if !valid.contains(&category.as_str()) {
        return Err("invalid category".into());
    }
    let c = conn()?;
    c.execute(
        "INSERT INTO app_categories(app_name, category) VALUES (?1,?2)
         ON CONFLICT(app_name) DO UPDATE SET category = excluded.category",
        (&app_name, &category),
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Distinct apps seen in activity, with any assigned category.
#[tauri::command]
pub fn list_apps() -> Result<Vec<CategoryRow>, String> {
    let c = conn()?;
    let mut stmt = c
        .prepare(
            "SELECT DISTINCT a.app_name, COALESCE(cat.category,'')
             FROM activity_log a
             LEFT JOIN app_categories cat ON cat.app_name = a.app_name
             WHERE a.app_name != 'Idle'
             ORDER BY a.app_name",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok(CategoryRow {
                app_name: r.get(0)?,
                category: r.get(1)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
}

#[derive(Serialize)]
pub struct RuleRow {
    id: i64,
    target: String,
    #[serde(rename = "targetType")]
    target_type: String,
    action: String,
    schedule: Option<String>,
    enabled: bool,
    #[serde(rename = "enforcementMode")]
    enforcement_mode: String,
}

#[tauri::command]
pub fn get_rules() -> Result<Vec<RuleRow>, String> {
    let c = conn()?;
    let mut stmt = c
        .prepare(
            "SELECT id, target, target_type, action, schedule, enabled, enforcement_mode
             FROM rules ORDER BY id",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok(RuleRow {
                id: r.get(0)?,
                target: r.get(1)?,
                target_type: r.get(2)?,
                action: r.get(3)?,
                schedule: r.get(4)?,
                enabled: r.get::<_, i64>(5)? != 0,
                enforcement_mode: r.get(6)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub fn save_rule(
    id: Option<i64>,
    target: String,
    target_type: String,
    action: String,
    schedule: Option<String>,
    enabled: bool,
    enforcement_mode: String,
) -> Result<i64, String> {
    let c = conn()?;
    match id {
        Some(rid) => {
            c.execute(
                "UPDATE rules SET target=?1, target_type=?2, action=?3, schedule=?4,
                    enabled=?5, enforcement_mode=?6 WHERE id=?7",
                (
                    &target,
                    &target_type,
                    &action,
                    &schedule,
                    enabled as i64,
                    &enforcement_mode,
                    rid,
                ),
            )
            .map_err(|e| e.to_string())?;
            Ok(rid)
        }
        None => {
            c.execute(
                "INSERT INTO rules(target, target_type, action, schedule, enabled, enforcement_mode)
                 VALUES (?1,?2,?3,?4,?5,?6)",
                (
                    &target,
                    &target_type,
                    &action,
                    &schedule,
                    enabled as i64,
                    &enforcement_mode,
                ),
            )
            .map_err(|e| e.to_string())?;
            Ok(c.last_insert_rowid())
        }
    }
}

#[tauri::command]
pub fn delete_rule(id: i64) -> Result<(), String> {
    let c = conn()?;
    c.execute("DELETE FROM rules WHERE id = ?1", [id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(Serialize)]
pub struct RuleEventRow {
    id: i64,
    rule_id: i64,
    app_name: Option<String>,
    window_title: Option<String>,
    occurred_at: i64,
    message: Option<String>,
}

#[tauri::command]
pub fn get_rule_events(limit: i64) -> Result<Vec<RuleEventRow>, String> {
    let c = conn()?;
    let mut stmt = c
        .prepare(
            "SELECT id, rule_id, app_name, window_title, occurred_at, message
             FROM rule_events ORDER BY occurred_at DESC LIMIT ?1",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([limit], |r| {
            Ok(RuleEventRow {
                id: r.get(0)?,
                rule_id: r.get(1)?,
                app_name: r.get(2)?,
                window_title: r.get(3)?,
                occurred_at: r.get(4)?,
                message: r.get(5)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
}

#[derive(Serialize)]
pub struct SettingRow {
    key: String,
    value: String,
}

#[tauri::command]
pub fn get_settings() -> Result<Vec<SettingRow>, String> {
    let c = conn()?;
    let mut stmt = c
        .prepare("SELECT key, value FROM settings ORDER BY key")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok(SettingRow {
                key: r.get(0)?,
                value: r.get(1)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_setting(key: String, value: String) -> Result<(), String> {
    let c = conn()?;
    db::set_setting(&c, &key, &value).map_err(|e| e.to_string())
}

#[derive(Serialize)]
pub struct MonitorStatus {
    running: bool,
    heartbeat: i64,
    seconds_since_heartbeat: i64,
    should_run: bool,
}

#[tauri::command]
pub fn get_monitor_status() -> Result<MonitorStatus, String> {
    let c = conn()?;
    let hb = db::get_setting_i64(&c, "monitor_heartbeat", 0);
    let should = db::get_setting_i64(&c, "monitor_should_run", 1) == 1;
    let now = db::now_epoch();
    let since = now - hb;
    Ok(MonitorStatus {
        running: hb > 0 && since <= 20,
        heartbeat: hb,
        seconds_since_heartbeat: since,
        should_run: should,
    })
}

fn monitor_exe() -> Result<std::path::PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let dir = exe.parent().ok_or("no parent dir")?;
    Ok(dir.join("nivara-monitor.exe"))
}

#[tauri::command]
pub fn start_monitor() -> Result<(), String> {
    let c = conn()?;
    db::set_setting(&c, "monitor_should_run", "1").map_err(|e| e.to_string())?;
    // Already alive?
    if get_monitor_status()?.running {
        return Ok(());
    }
    let path = monitor_exe()?;
    if !path.exists() {
        return Err(format!("monitor binary not found: {}", path.display()));
    }
    Command::new(path)
        .spawn()
        .map_err(|e| e.to_string())?;
    monitor::log("dashboard launched monitor process");
    Ok(())
}

#[tauri::command]
pub fn stop_monitor(password: String) -> Result<(), String> {
    let c = conn()?;
    // Password gate: stopping the monitor requires the parent password (once set).
    if auth::is_set(&c) && !auth::verify(&c, &password) {
        return Err("incorrect password".into());
    }
    db::set_setting(&c, "monitor_should_run", "0").map_err(|e| e.to_string())
}

// ---- Password gate ------------------------------------------------------

#[tauri::command]
pub fn auth_status() -> Result<bool, String> {
    let c = conn()?;
    Ok(auth::is_set(&c))
}

/// First-run setup. Returns the one-time recovery code (shown once).
#[tauri::command]
pub fn auth_setup(password: String) -> Result<String, String> {
    let c = conn()?;
    auth::setup(&c, &password)
}

#[tauri::command]
pub fn auth_verify(password: String) -> Result<bool, String> {
    let c = conn()?;
    Ok(auth::verify(&c, &password))
}

#[tauri::command]
pub fn auth_recover(recovery_code: String, new_password: String) -> Result<(), String> {
    let c = conn()?;
    auth::recover(&c, &recovery_code, &new_password)
}

// ---- DNS website filtering (elevated apply/remove) ----------------------

/// Point the active adapters' DNS at Nivara's local proxy — and ONLY the proxy
/// (127.0.0.1). No secondary resolver: a secondary would let clients fetch the
/// real IP of a blocked site, and (worse) browsers auto-upgrade to encrypted
/// DoH when they see a known public resolver in the system config, bypassing
/// the proxy entirely. Airtight instead: the proxy is the sole resolver.
///
/// To avoid breaking DNS, this WAITS for the proxy to actually answer on
/// 127.0.0.1:53 before repointing, and aborts (leaving DNS unchanged) if it
/// never comes up. Requires elevation (UAC). Explicit, reversible, bypassable.
#[tauri::command]
pub fn dns_filter_apply() -> Result<(), String> {
    let c = conn()?;
    db::set_setting(&c, "dns_filter_enabled", "1").map_err(|e| e.to_string())?;
    let _ = start_monitor();
    if !wait_for_proxy(8) {
        db::set_setting(&c, "dns_filter_enabled", "0").ok();
        return Err(
            "DNS proxy did not start (port 53 in use, or monitor blocked?). DNS was left unchanged."
                .into(),
        );
    }
    run_elevated_ps(
        r#"Get-DnsClientServerAddress -AddressFamily IPv4 | Where-Object {$_.ServerAddresses.Count -gt 0 -and $_.InterfaceAlias -notlike '*Loopback*'} | ForEach-Object { Set-DnsClientServerAddress -InterfaceIndex $_.InterfaceIndex -ServerAddresses '127.0.0.1' }; Clear-DnsClientCache"#,
    )
}

/// Send a DNS query to 127.0.0.1:53 and return true if the proxy answers within
/// the given number of seconds (polled). Used to gate the DNS repoint.
fn wait_for_proxy(secs: u64) -> bool {
    use std::net::UdpSocket;
    // Minimal DNS query for "a.root-servers.net" A record.
    let query: [u8; 33] = [
        0x12, 0x34, 0x01, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, b'a', 0x0c,
        b'r', b'o', b'o', b't', b'-', b's', b'e', b'r', b'v', b'e', b'r', b's', 0x03, b'n', b'e',
        b't', 0x00, 0x00,
    ];
    let query_full: Vec<u8> = query.iter().copied().chain([0x01u8, 0x00, 0x01]).collect();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(secs);
    while std::time::Instant::now() < deadline {
        if let Ok(sock) = UdpSocket::bind("0.0.0.0:0") {
            let _ = sock.set_read_timeout(Some(std::time::Duration::from_millis(700)));
            if sock.send_to(&query_full, "127.0.0.1:53").is_ok() {
                let mut buf = [0u8; 512];
                if sock.recv_from(&mut buf).is_ok() {
                    return true;
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(400));
    }
    false
}

/// Reset adapters to automatic (DHCP) DNS and stop filtering.
#[tauri::command]
pub fn dns_filter_remove() -> Result<(), String> {
    let c = conn()?;
    db::set_setting(&c, "dns_filter_enabled", "0").map_err(|e| e.to_string())?;
    run_elevated_ps(
        r#"Get-DnsClientServerAddress -AddressFamily IPv4 | ForEach-Object { Set-DnsClientServerAddress -InterfaceIndex $_.InterfaceIndex -ResetServerAddresses }; Clear-DnsClientCache"#,
    )
}

/// Launch an elevated PowerShell command (UAC prompt) and wait for it.
fn run_elevated_ps(inner: &str) -> Result<(), String> {
    let encoded = inner.replace('"', "\\\"");
    let arg = format!(
        "Start-Process powershell -Verb RunAs -Wait -WindowStyle Hidden -ArgumentList '-NoProfile','-Command','{}'",
        encoded.replace('\'', "''")
    );
    let status = Command::new("powershell")
        .args(["-NoProfile", "-Command", &arg])
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err("elevated DNS change failed or was cancelled".into())
    }
}

#[tauri::command]
pub fn get_autostart() -> Result<bool, String> {
    Ok(autostart::is_enabled())
}

#[tauri::command]
pub fn set_autostart(enabled: bool) -> Result<(), String> {
    if enabled {
        autostart::enable().map_err(|e| e.to_string())
    } else {
        autostart::disable().map_err(|e| e.to_string())
    }
}
