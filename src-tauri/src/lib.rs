pub mod auth;
pub mod autostart;
pub mod db;
pub mod dns;
pub mod monitor;
pub mod rules;
pub mod winmon;

mod commands;

/// Headless password check used by the NSIS uninstaller.
/// `nivara.exe --check-password <pw>` exits 0 if correct (or if no password is
/// set), 1 otherwise. No window is shown.
fn handle_cli() {
    let args: Vec<String> = std::env::args().collect();

    // Non-interactive check: `--check-password <pw>`.
    if let Some(pos) = args.iter().position(|a| a == "--check-password") {
        let pw = args.get(pos + 1).cloned().unwrap_or_default();
        std::process::exit(check_password_exit_code(&pw));
    }

    // Interactive uninstall gate: prompt for the password (via a PowerShell
    // input box) and verify. Used by the NSIS PREUNINSTALL hook.
    if args.iter().any(|a| a == "--uninstall-gate") {
        let allow = match db::open() {
            Ok(c) => {
                if !auth::is_set(&c) {
                    true // nothing configured -> allow
                } else {
                    let pw = prompt_password();
                    auth::verify(&c, &pw)
                }
            }
            Err(_) => true,
        };
        std::process::exit(if allow { 0 } else { 1 });
    }
}

fn check_password_exit_code(pw: &str) -> i32 {
    match db::open() {
        Ok(c) => {
            if !auth::is_set(&c) || auth::verify(&c, pw) {
                0
            } else {
                1
            }
        }
        Err(_) => 0,
    }
}

/// Show a native input box (via PowerShell) and return the entered password.
/// Empty string on cancel or failure.
fn prompt_password() -> String {
    let script = "Add-Type -AssemblyName Microsoft.VisualBasic; \
        [Microsoft.VisualBasic.Interaction]::InputBox(\
        'Enter the Nivara parent password to allow uninstall.','Nivara Uninstall','')";
    match std::process::Command::new("powershell")
        .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", script])
        .output()
    {
        Ok(out) => String::from_utf8_lossy(&out.stdout).trim().to_string(),
        Err(_) => String::new(),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    handle_cli();
    winmon::set_dpi_aware();
    // Ensure DB + schema exist before UI queries.
    if let Err(e) = db::open() {
        eprintln!("db init failed: {}", e);
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            commands::get_activity_timeline,
            commands::get_usage_summary,
            commands::get_daily_report,
            commands::get_weekly_report,
            commands::get_screenshots,
            commands::read_screenshot,
            commands::get_categories,
            commands::set_category,
            commands::list_apps,
            commands::get_rules,
            commands::save_rule,
            commands::delete_rule,
            commands::get_rule_events,
            commands::get_settings,
            commands::update_setting,
            commands::get_monitor_status,
            commands::start_monitor,
            commands::stop_monitor,
            commands::get_autostart,
            commands::set_autostart,
            commands::auth_status,
            commands::auth_setup,
            commands::auth_verify,
            commands::auth_recover,
            commands::dns_filter_apply,
            commands::dns_filter_remove,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
