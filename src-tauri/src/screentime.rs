// Screen-time limit: daily active-time allowance + optional bedtime. When the
// limit is reached the session is locked; if the child unlocks it again while
// still over, a parent-password prompt is shown and, unless time is granted,
// the session is re-locked. User-level and bypassable (see CLAUDE.md).

use crate::{auth, db, rules, winmon};
use chrono::{Datelike, Local, Weekday};
use rusqlite::Connection;
use serde::Serialize;
use std::os::windows::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};
use winreg::RegKey;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
/// How long the child has to answer the parent-password prompt after unlocking.
const PROMPT_GRACE: Duration = Duration::from_secs(10);
/// Max seconds credited per poll (sleep/hibernate gaps must not count as use).
const MAX_TICK_SECS: i64 = 10;

pub fn today() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}

/// Reset the per-day counters when the date changes.
pub fn roll_day(conn: &Connection) {
    let t = today();
    if db::get_setting(conn, "limit_day").as_deref() != Some(t.as_str()) {
        for (k, v) in [
            ("limit_used_secs", "0"),
            ("limit_bonus_secs", "0"),
            ("limit_pass_until", "0"),
            ("limit_warned5_day", ""),
            ("limit_warned1_day", ""),
        ] {
            let _ = db::set_setting(conn, k, v);
        }
        let _ = db::set_setting(conn, "limit_day", &t);
    }
}

#[derive(Serialize, Clone)]
pub struct Status {
    pub enabled: bool,
    pub used_secs: i64,
    /// Daily allowance incl. today's bonus; None = no daily limit.
    pub allowance_secs: Option<i64>,
    pub remaining_secs: Option<i64>,
    pub bonus_secs: i64,
    pub in_bedtime: bool,
    pub over: bool,
}

pub fn compute(conn: &Connection, now: i64) -> Status {
    let enabled = db::get_setting_i64(conn, "limit_enabled", 0) == 1;
    let used = db::get_setting_i64(conn, "limit_used_secs", 0);
    let bonus = db::get_setting_i64(conn, "limit_bonus_secs", 0);
    let pass_until = db::get_setting_i64(conn, "limit_pass_until", 0);
    let weekend = matches!(Local::now().weekday(), Weekday::Sat | Weekday::Sun);
    let mins = db::get_setting_i64(
        conn,
        if weekend { "limit_weekend_mins" } else { "limit_weekday_mins" },
        0,
    );
    let allowance = if mins > 0 { Some(mins * 60 + bonus) } else { None };
    let bedtime = db::get_setting(conn, "bedtime_range").unwrap_or_default();
    let in_bedtime = !bedtime.trim().is_empty()
        && rules::schedule_active(Some(&bedtime), Local::now().time())
        && now >= pass_until;
    let over_daily = matches!(allowance, Some(a) if used >= a);
    Status {
        enabled,
        used_secs: used,
        allowance_secs: allowance,
        remaining_secs: allowance.map(|a| (a - used).max(0)),
        bonus_secs: bonus,
        in_bedtime,
        over: enabled && (over_daily || in_bedtime),
    }
}

/// Parent grants extra time: raises today's allowance and passes bedtime for
/// the same span.
pub fn grant(conn: &Connection, minutes: i64, now: i64) {
    roll_day(conn);
    let bonus = db::get_setting_i64(conn, "limit_bonus_secs", 0) + minutes * 60;
    let _ = db::set_setting(conn, "limit_bonus_secs", &bonus.to_string());
    let _ = db::set_setting(conn, "limit_pass_until", &(now + minutes * 60).to_string());
    log_event(conn, now, &format!("screen time: parent granted +{} min", minutes));
}

pub fn log_event(conn: &Connection, now: i64, msg: &str) {
    let _ = conn.execute(
        "INSERT INTO rule_events(rule_id, app_name, window_title, occurred_at, message)
         VALUES (0, 'ScreenTime', '', ?1, ?2)",
        (now, msg),
    );
}

// ---- PowerShell dialogs --------------------------------------------------

fn ps_file(name: &str, script: &str) -> std::path::PathBuf {
    let p = db::data_dir().join(name);
    let _ = std::fs::write(&p, script);
    p
}

fn ps_command(file: &std::path::Path, args: &[&str]) -> Command {
    let mut c = Command::new("powershell");
    c.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-WindowStyle", "Hidden", "-File"])
        .arg(file)
        .args(args)
        .creation_flags(CREATE_NO_WINDOW);
    c
}

const GRANT_PS: &str = r##"param([string]$Msg = '', [string]$Title = "Time's up", [int]$Seconds = 40)
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
[Windows.Forms.Application]::EnableVisualStyles()
function RGB($r, $g, $b) { [Drawing.Color]::FromArgb($r, $g, $b) }
function Round($c, $r) {
  $gp = New-Object Drawing.Drawing2D.GraphicsPath
  $w = $c.Width; $h = $c.Height
  $gp.AddArc(0, 0, $r, $r, 180, 90); $gp.AddArc($w - $r, 0, $r, $r, 270, 90)
  $gp.AddArc($w - $r, $h - $r, $r, $r, 0, 90); $gp.AddArc(0, $h - $r, $r, $r, 90, 90)
  $gp.CloseFigure(); $c.Region = New-Object Drawing.Region($gp)
}
function Lbl($t, $x, $y, $w, $h, $size, $style, $col, $align) {
  $l = New-Object Windows.Forms.Label
  $l.Text = $t; $l.SetBounds($x, $y, $w, $h); $l.ForeColor = $col; $l.BackColor = [Drawing.Color]::Transparent
  $l.Font = New-Object Drawing.Font('Segoe UI', $size, $style); $l.TextAlign = $align
  return $l
}
$vs = [Windows.Forms.SystemInformation]::VirtualScreen
$pb = [Windows.Forms.Screen]::PrimaryScreen.Bounds
$f = New-Object Windows.Forms.Form
$f.FormBorderStyle = 'None'; $f.StartPosition = 'Manual'; $f.Bounds = $vs
$f.TopMost = $true; $f.ShowInTaskbar = $false; $f.Opacity = 0.97
$f.BackColor = RGB 9 13 26
$script:closing = $false; $script:out = ''; $script:mins = 15; $script:left = $Seconds

$card = New-Object Windows.Forms.Panel
$card.Size = New-Object Drawing.Size(440, 440)
$card.Location = New-Object Drawing.Point(($pb.X - $vs.X + [int](($pb.Width - 440) / 2)), ($pb.Y - $vs.Y + [int](($pb.Height - 440) / 2)))
$card.BackColor = RGB 22 30 54
$f.Controls.Add($card)

$ico = New-Object Windows.Forms.Label
$ico.Text = [string][char]0xE72E; $ico.Font = New-Object Drawing.Font('Segoe MDL2 Assets', 30)
$ico.ForeColor = RGB 96 165 250; $ico.TextAlign = 'MiddleCenter'; $ico.SetBounds(0, 22, 440, 54)
$card.Controls.Add($ico)
$card.Controls.Add((Lbl $Title 0 80 440 36 20 'Bold' (RGB 255 255 255) 'MiddleCenter'))
$sub = if ($Title -eq 'Bedtime') { 'It is bedtime. A parent can allow more time.' } else { 'The screen-time limit has been reached. A parent can allow more time.' }
$card.Controls.Add((Lbl $sub 32 118 376 40 10 'Regular' (RGB 148 163 184) 'TopCenter'))
$card.Controls.Add((Lbl 'ADD TIME' 32 168 376 18 8 'Bold' (RGB 100 116 139) 'MiddleLeft'))

$btns = @()
$styleBtns = {
  foreach ($b in $btns) {
    if ([int]$b.Tag -eq $script:mins) { $b.BackColor = RGB 59 130 246; $b.ForeColor = [Drawing.Color]::White }
    else { $b.BackColor = RGB 36 48 82; $b.ForeColor = RGB 203 213 225 }
  }
}
$x = 32
foreach ($m in 15, 30, 60) {
  $b = New-Object Windows.Forms.Button
  $b.Text = "$m min"; $b.Tag = $m; $b.SetBounds($x, 190, 116, 40)
  $b.FlatStyle = 'Flat'; $b.FlatAppearance.BorderSize = 0; $b.Cursor = 'Hand'
  $b.Font = New-Object Drawing.Font('Segoe UI', 10, 'Bold')
  $b.Add_Click({ $script:mins = [int]$this.Tag; & $styleBtns })
  $card.Controls.Add($b); $btns += $b; $x += 130
}
& $styleBtns

$card.Controls.Add((Lbl 'PARENT PASSWORD' 32 244 376 18 8 'Bold' (RGB 100 116 139) 'MiddleLeft'))
$pw = New-Object Windows.Forms.TextBox
$pw.UseSystemPasswordChar = $true; $pw.BorderStyle = 'FixedSingle'
$pw.BackColor = RGB 12 18 36; $pw.ForeColor = [Drawing.Color]::White
$pw.Font = New-Object Drawing.Font('Segoe UI', 13); $pw.SetBounds(32, 266, 376, 32)
$card.Controls.Add($pw)
$err = Lbl $Msg 32 302 376 20 9 'Regular' (RGB 248 113 113) 'MiddleLeft'
$card.Controls.Add($err)

$ok = New-Object Windows.Forms.Button
$ok.Text = 'Allow more time'; $ok.SetBounds(32, 332, 250, 46)
$ok.FlatStyle = 'Flat'; $ok.FlatAppearance.BorderSize = 0; $ok.Cursor = 'Hand'
$ok.BackColor = RGB 34 197 94; $ok.ForeColor = RGB 5 46 22; $ok.Font = New-Object Drawing.Font('Segoe UI', 11, 'Bold')
$ok.Add_Click({ $script:out = "$($script:mins)`t$($pw.Text)"; $script:closing = $true; $f.Close() })
$card.Controls.Add($ok)
$no = New-Object Windows.Forms.Button
$no.Text = 'Lock now'; $no.SetBounds(294, 332, 114, 46)
$no.FlatStyle = 'Flat'; $no.FlatAppearance.BorderSize = 0; $no.Cursor = 'Hand'
$no.BackColor = RGB 51 65 85; $no.ForeColor = RGB 226 232 240; $no.Font = New-Object Drawing.Font('Segoe UI', 10)
$no.Add_Click({ $script:closing = $true; $f.Close() })
$card.Controls.Add($no)
$f.AcceptButton = $ok

$cd = Lbl '' 0 392 440 20 9 'Regular' (RGB 148 163 184) 'MiddleCenter'
$card.Controls.Add($cd)
$bar = New-Object Windows.Forms.Panel
$bar.SetBounds(32, 420, 376, 4); $bar.BackColor = RGB 59 130 246
$track = New-Object Windows.Forms.Panel
$track.SetBounds(32, 420, 376, 4); $track.BackColor = RGB 36 48 82
$card.Controls.Add($bar); $card.Controls.Add($track); $bar.BringToFront()
function Tick {
  $cd.Text = "Locking again in $($script:left)s unless time is granted"
  $bar.Width = [int](376 * $script:left / [math]::Max(1, $Seconds))
  if ($script:left -le 10) { $bar.BackColor = RGB 239 68 68 }
}
Tick
$t = New-Object Windows.Forms.Timer
$t.Interval = 1000
$t.Add_Tick({ $script:left--; if ($script:left -le 0) { $script:closing = $true; $f.Close() } else { Tick } })
$f.Add_FormClosing({ param($s, $e) if (-not $script:closing) { $e.Cancel = $true } })
$f.Add_Shown({ Round $card 24; $f.Activate(); $pw.Focus(); $t.Start() })
[void]$f.ShowDialog()
$t.Stop()
if ($script:out) { [Console]::Out.Write($script:out) }
"##;

const WARN_PS: &str = r#"param([string]$Msg = '')
Add-Type -AssemblyName System.Windows.Forms
$o = New-Object Windows.Forms.Form
$o.TopMost = $true
$o.ShowInTaskbar = $false
$o.StartPosition = 'CenterScreen'
$o.Opacity = 0
$o.Show()
[void][Windows.Forms.MessageBox]::Show($o, $Msg, 'Nivara - Screen time', 'OK', 'Warning')
"#;

fn warn(msg: &str) {
    let f = ps_file("st_warn.ps1", WARN_PS);
    let _ = ps_command(&f, &[msg]).spawn();
}

/// Interactive parent grant (used by `nivara.exe --grant-time`). Up to 3
/// attempts. Returns true if time was granted.
pub fn grant_prompt_cli() -> bool {
    let conn = match db::open() {
        Ok(c) => c,
        Err(_) => return false,
    };
    let file = ps_file("st_grant.ps1", GRANT_PS);
    let title = if compute(&conn, db::now_epoch()).in_bedtime { "Bedtime" } else { "Time's up" };
    let mut msg = String::new();
    for _ in 0..3 {
        let mut args: Vec<&str> = vec!["-Title", title, "-Seconds", "10"];
        if !msg.is_empty() {
            args.extend(["-Msg", &msg]);
        }
        let out = match ps_command(&file, &args).stdout(Stdio::piped()).output() {
            Ok(o) => o,
            Err(_) => return false,
        };
        let text = String::from_utf8_lossy(&out.stdout).to_string();
        let Some((mins, pw)) = text.trim_end_matches(['\r', '\n']).split_once('\t') else {
            return false; // cancelled
        };
        let mins: i64 = mins.trim().parse().unwrap_or(15);
        if !auth::is_set(&conn) || auth::verify(&conn, pw) {
            grant(&conn, mins, db::now_epoch());
            return true;
        }
        msg = "Incorrect password. Try again.".into();
    }
    false
}

// ---- Task Manager / cmd policy (HKCU, speed bump only) -------------------

const POLICY_SYSTEM: &str = r"Software\Microsoft\Windows\CurrentVersion\Policies\System";

fn restrictions_active() -> bool {
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(POLICY_SYSTEM, KEY_READ)
        .map(|k| {
            k.get_value::<u32, _>("DisableTaskMgr").unwrap_or(0) == 1
                && k.get_value::<u32, _>("DisableCMD").unwrap_or(0) == 1
        })
        .unwrap_or(false)
}

/// Set or clear DisableTaskMgr + DisableCMD for the current user.
pub fn set_restrictions(on: bool) -> std::io::Result<()> {
    let (key, _) = RegKey::predef(HKEY_CURRENT_USER)
        .create_subkey_with_flags(POLICY_SYSTEM, KEY_WRITE)?;
    for name in ["DisableTaskMgr", "DisableCMD"] {
        if on {
            key.set_value(name, &1u32)?;
        } else if let Err(e) = key.delete_value(name) {
            if e.kind() != std::io::ErrorKind::NotFound {
                return Err(e);
            }
        }
    }
    Ok(())
}

// ---- Monitor-side enforcement --------------------------------------------

pub struct Enforcer {
    last_tick: Option<i64>,
    was_over: bool,
    prompt: Option<(Child, Instant)>,
}

impl Enforcer {
    pub fn new() -> Self {
        Enforcer { last_tick: None, was_over: false, prompt: None }
    }

    fn kill_prompt(&mut self) {
        if let Some((mut ch, _)) = self.prompt.take() {
            let _ = ch.kill();
            let _ = ch.wait();
        }
    }

    fn lock(&self, conn: &Connection, now: i64, why: &str) {
        log_event(conn, now, &format!("screen time: locked ({})", why));
        winmon::lock_workstation();
    }

    /// Called every monitor poll. `idle` = no recent input.
    pub fn tick(&mut self, conn: &Connection, now: i64, idle: bool, log: &dyn Fn(&str)) {
        // Task Manager / cmd policy: (re)apply while enabled.
        let want_restrict = db::get_setting_i64(conn, "restrict_taskmgr_cmd", 0) == 1;
        if want_restrict && !restrictions_active() {
            if let Err(e) = set_restrictions(true) {
                log(&format!("restrictions apply failed: {}", e));
            }
        }

        if db::get_setting_i64(conn, "limit_enabled", 0) != 1 {
            self.kill_prompt();
            self.last_tick = None;
            self.was_over = false;
            return;
        }
        roll_day(conn);
        let locked = winmon::is_session_locked();

        // Count active time while under the limit.
        let dt = self.last_tick.map(|t| (now - t).clamp(0, MAX_TICK_SECS)).unwrap_or(0);
        self.last_tick = Some(now);
        let mut st = compute(conn, now);
        if !st.over && !idle && !locked && dt > 0 {
            let used = st.used_secs + dt;
            let _ = db::set_setting(conn, "limit_used_secs", &used.to_string());
            st = compute(conn, now);
        }

        if !st.over {
            self.was_over = false;
            self.kill_prompt();
            // Heads-up warnings (once per day each).
            if let Some(rem) = st.remaining_secs {
                if !idle && !locked {
                    let t = today();
                    if rem <= 60 && db::get_setting(conn, "limit_warned1_day").as_deref() != Some(&t) {
                        let _ = db::set_setting(conn, "limit_warned1_day", &t);
                        let _ = db::set_setting(conn, "limit_warned5_day", &t);
                        warn("Less than 1 minute of screen time left. The computer will lock.");
                    } else if rem <= 300 && db::get_setting(conn, "limit_warned5_day").as_deref() != Some(&t) {
                        let _ = db::set_setting(conn, "limit_warned5_day", &t);
                        warn("About 5 minutes of screen time left.");
                    }
                }
            }
            return;
        }

        // ---- over the limit (or bedtime) ----
        let why = if st.in_bedtime { "bedtime" } else { "daily limit" };
        if locked {
            self.kill_prompt();
            self.was_over = true;
            return;
        }
        match self.prompt.take() {
            Some((mut ch, started)) => match ch.try_wait() {
                Ok(Some(status)) => {
                    if !status.success() {
                        self.lock(conn, now, why);
                    } // success: time granted, next poll sees it
                }
                Ok(None) => {
                    if started.elapsed() > PROMPT_GRACE {
                        let _ = ch.kill();
                        let _ = ch.wait();
                        self.lock(conn, now, &format!("{}, no password", why));
                    } else {
                        self.prompt = Some((ch, started));
                    }
                }
                Err(_) => self.lock(conn, now, why),
            },
            None => {
                if !self.was_over {
                    self.lock(conn, now, why);
                } else {
                    self.prompt = spawn_grant_prompt().map(|c| (c, Instant::now()));
                    if self.prompt.is_none() {
                        self.lock(conn, now, "prompt unavailable");
                    }
                }
            }
        }
        self.was_over = true;
    }
}

/// Launch `nivara.exe --grant-time` (sits next to the monitor exe).
fn spawn_grant_prompt() -> Option<Child> {
    let dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    let exe = dir.join("nivara.exe");
    if !exe.exists() {
        return None;
    }
    Command::new(exe)
        .arg("--grant-time")
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .ok()
}
