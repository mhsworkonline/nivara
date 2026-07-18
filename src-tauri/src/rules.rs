// Rule schema + evaluation. Two modes per rule:
//   enforcementMode = "monitor" -> matches are LOGGED ONLY ("would be blocked").
//   enforcementMode = "enforce" -> action is actually applied (app termination
//                                  or DNS block) when the global enforcement
//                                  switch is on and the schedule is active.
// User-level enforcement is a deterrent and is bypassable (see CLAUDE.md).

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub id: i64,
    pub target: String,
    #[serde(rename = "targetType")]
    pub target_type: String, // "app" | "website" | "category"
    pub action: String,      // e.g. "block"
    pub schedule: Option<String>,
    pub enabled: bool,
    #[serde(rename = "enforcementMode")]
    pub enforcement_mode: String, // "monitor" | "enforce" (enforce is never honored in MVP)
}

pub fn load_enabled(conn: &Connection) -> rusqlite::Result<Vec<Rule>> {
    let mut stmt = conn.prepare(
        "SELECT id, target, target_type, action, schedule, enabled, enforcement_mode
         FROM rules WHERE enabled = 1",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(Rule {
            id: r.get(0)?,
            target: r.get(1)?,
            target_type: r.get(2)?,
            action: r.get(3)?,
            schedule: r.get(4)?,
            enabled: r.get::<_, i64>(5)? != 0,
            enforcement_mode: r.get(6)?,
        })
    })?;
    rows.collect()
}

/// Returns true if the rule matches the observed activity.
pub fn matches(rule: &Rule, app_name: &str, window_title: &str, category: Option<&str>) -> bool {
    let target = rule.target.to_lowercase();
    match rule.target_type.as_str() {
        "app" => app_name.to_lowercase().contains(&target),
        "category" => category
            .map(|c| c.eq_ignore_ascii_case(&rule.target))
            .unwrap_or(false),
        "website" => window_title.to_lowercase().contains(&target),
        _ => false,
    }
}

/// A rule actively enforces (vs. log-only) when enabled, in "enforce" mode,
/// and its action is "block".
pub fn is_enforced(rule: &Rule) -> bool {
    rule.enabled && rule.enforcement_mode == "enforce" && rule.action == "block"
}

/// Whether an optional "HH:MM-HH:MM" schedule is active at the given local time.
/// None/empty schedule = always active. Handles overnight ranges (e.g. 20:00-07:00).
pub fn schedule_active(schedule: Option<&str>, now: chrono::NaiveTime) -> bool {
    let s = match schedule {
        Some(s) if !s.trim().is_empty() => s.trim(),
        _ => return true,
    };
    let parse = |t: &str| -> Option<chrono::NaiveTime> {
        let (h, m) = t.split_once(':')?;
        chrono::NaiveTime::from_hms_opt(h.trim().parse().ok()?, m.trim().parse().ok()?, 0)
    };
    let (a, b) = match s.split_once('-') {
        Some(v) => v,
        None => return true, // unparseable -> treat as always active
    };
    match (parse(a), parse(b)) {
        (Some(start), Some(end)) => {
            if start <= end {
                now >= start && now < end
            } else {
                now >= start || now < end // overnight wrap
            }
        }
        _ => true,
    }
}

/// Convenience: is this rule's schedule active right now (local time)?
pub fn schedule_active_now(rule: &Rule) -> bool {
    schedule_active(rule.schedule.as_deref(), chrono::Local::now().time())
}

pub fn log_event(
    conn: &Connection,
    rule: &Rule,
    app_name: &str,
    window_title: &str,
    now: i64,
    message: &str,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO rule_events(rule_id, app_name, window_title, occurred_at, message)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        (rule.id, app_name, window_title, now, message),
    )?;
    Ok(())
}

pub fn log_would_block(
    conn: &Connection,
    rule: &Rule,
    app_name: &str,
    window_title: &str,
    now: i64,
) -> rusqlite::Result<()> {
    let msg = format!(
        "would be blocked: rule#{} action={} target={} ({})",
        rule.id, rule.action, rule.target, rule.target_type
    );
    conn.execute(
        "INSERT INTO rule_events(rule_id, app_name, window_title, occurred_at, message)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        (rule.id, app_name, window_title, now, msg),
    )?;
    Ok(())
}
