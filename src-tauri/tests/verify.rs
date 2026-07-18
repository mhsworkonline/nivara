// End-to-end verification of the password gate, app-termination enforcement,
// and DNS website blocking. Runs the real shipping binaries against a throwaway
// LOCALAPPDATA so it never touches real user data.
//
// Run single-threaded (the DNS proxy binds 127.0.0.1:53):
//   cargo test --test verify -- --test-threads=1 --nocapture

use std::process::Command;
use std::time::Duration;

fn temp_localappdata(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("nivara_verify_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("Nivara")).unwrap();
    dir
}

fn bin(name: &str) -> std::path::PathBuf {
    // target/debug/<name>.exe relative to the test binary location.
    let mut p = std::env::current_exe().unwrap();
    p.pop(); // deps
    if p.ends_with("deps") {
        p.pop();
    }
    p.join(format!("{}.exe", name))
}

#[test]
fn password_gate_cli() {
    let lad = temp_localappdata("pw");
    std::env::set_var("LOCALAPPDATA", &lad);

    let conn = nivara_lib::db::open().unwrap();
    assert!(!nivara_lib::auth::is_set(&conn), "should start unset");

    let recovery = nivara_lib::auth::setup(&conn, "correct-horse").unwrap();
    println!("recovery code issued: {}", recovery);
    assert!(recovery.starts_with("NIVARA-"));
    assert!(nivara_lib::auth::is_set(&conn));
    assert!(nivara_lib::auth::verify(&conn, "correct-horse"));
    assert!(!nivara_lib::auth::verify(&conn, "wrong"));

    // Recovery resets the password.
    nivara_lib::auth::recover(&conn, &recovery, "new-pass").unwrap();
    assert!(nivara_lib::auth::verify(&conn, "new-pass"));
    assert!(!nivara_lib::auth::verify(&conn, "correct-horse"));
    assert!(nivara_lib::auth::recover(&conn, "NIVARA-XXXX-XXXX-XXXX", "x").is_err());
    drop(conn);

    // The shipping CLI used by the uninstaller must agree on exit codes.
    let exe = bin("nivara");
    let ok = Command::new(&exe)
        .args(["--check-password", "new-pass"])
        .env("LOCALAPPDATA", &lad)
        .status()
        .unwrap();
    let bad = Command::new(&exe)
        .args(["--check-password", "nope"])
        .env("LOCALAPPDATA", &lad)
        .status()
        .unwrap();
    println!("--check-password correct -> {:?}", ok.code());
    println!("--check-password wrong   -> {:?}", bad.code());
    assert_eq!(ok.code(), Some(0));
    assert_eq!(bad.code(), Some(1));
}

// Regression: website DNS blocking must work with ONLY dns_filter_enabled=1
// (global app enforcement OFF), against a freshly created rule + domain.
#[test]
fn dns_block_without_global_enforcement() {
    let lad = temp_localappdata("dnsonly");
    std::env::set_var("LOCALAPPDATA", &lad);

    let conn = nivara_lib::db::open().unwrap();
    nivara_lib::db::set_setting(&conn, "enforcement_enabled", "0").unwrap(); // app enforce OFF
    nivara_lib::db::set_setting(&conn, "dns_filter_enabled", "1").unwrap(); // DNS filter ON
    nivara_lib::db::set_setting(&conn, "screenshots_enabled", "0").unwrap();
    nivara_lib::db::set_setting(&conn, "monitor_should_run", "1").unwrap();
    seed_rule(&conn, "instagram.com", "website"); // new rule, not example.com
    drop(conn);

    let mut monitor = Command::new(bin("nivara-monitor"))
        .env("LOCALAPPDATA", &lad)
        .spawn()
        .unwrap();
    std::thread::sleep(Duration::from_secs(4)); // start + first blocklist refresh

    let exact = Command::new("nslookup")
        .args(["-timeout=3", "instagram.com", "127.0.0.1"])
        .output()
        .unwrap();
    let exact = String::from_utf8_lossy(&exact.stdout).into_owned();
    let www = Command::new("nslookup")
        .args(["-timeout=3", "www.instagram.com", "127.0.0.1"])
        .output()
        .unwrap();
    let www = String::from_utf8_lossy(&www.stdout).into_owned();
    println!("nslookup instagram.com:\n{}", exact);
    println!("nslookup www.instagram.com:\n{}", www);

    let conn = nivara_lib::db::open().unwrap();
    nivara_lib::db::set_setting(&conn, "monitor_should_run", "0").unwrap();
    drop(conn);
    std::thread::sleep(Duration::from_secs(2));
    let _ = monitor.kill();

    assert!(exact.contains("0.0.0.0"), "exact domain must be sinkholed");
    assert!(www.contains("0.0.0.0"), "www. subdomain must be sinkholed");
}

fn seed_rule(conn: &rusqlite::Connection, target: &str, ttype: &str) {
    conn.execute(
        "INSERT INTO rules(target, target_type, action, schedule, enabled, enforcement_mode)
         VALUES (?1, ?2, 'block', NULL, 1, 'enforce')",
        (target, ttype),
    )
    .unwrap();
}

fn is_running(name: &str) -> bool {
    let out = Command::new("tasklist")
        .args(["/FI", &format!("IMAGENAME eq {}", name), "/NH"])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout)
        .to_lowercase()
        .contains(&name.to_lowercase())
}

#[test]
fn enforcement_app_and_dns() {
    let lad = temp_localappdata("enf");
    std::env::set_var("LOCALAPPDATA", &lad);

    let conn = nivara_lib::db::open().unwrap();
    nivara_lib::db::set_setting(&conn, "enforcement_enabled", "1").unwrap();
    nivara_lib::db::set_setting(&conn, "dns_filter_enabled", "1").unwrap();
    nivara_lib::db::set_setting(&conn, "screenshots_enabled", "0").unwrap();
    nivara_lib::db::set_setting(&conn, "monitor_should_run", "1").unwrap();
    seed_rule(&conn, "notepad.exe", "app");
    seed_rule(&conn, "example.com", "website");
    drop(conn);

    // Launch the real monitor against the temp data dir.
    let mut monitor = Command::new(bin("nivara-monitor"))
        .env("LOCALAPPDATA", &lad)
        .spawn()
        .unwrap();
    std::thread::sleep(Duration::from_secs(3)); // let it start + bind DNS

    // --- App termination ---
    let mut np = Command::new("notepad.exe").spawn().unwrap();
    println!("launched notepad pid {}", np.id());
    // Poll up to ~15s for the monitor to terminate it.
    let mut killed = false;
    for _ in 0..15 {
        std::thread::sleep(Duration::from_secs(1));
        if !is_running("notepad.exe") {
            killed = true;
            break;
        }
    }
    let _ = np.kill(); // cleanup if still alive
    println!("notepad terminated by enforcement: {}", killed);

    // --- DNS block ---
    let blocked = Command::new("nslookup")
        .args(["-timeout=3", "example.com", "127.0.0.1"])
        .output()
        .unwrap();
    let btxt = String::from_utf8_lossy(&blocked.stdout);
    println!("nslookup example.com @127.0.0.1:\n{}", btxt);

    // --- DNS passthrough (non-blocked domain still resolves) ---
    let allow = Command::new("nslookup")
        .args(["-timeout=3", "wikipedia.org", "127.0.0.1"])
        .output()
        .unwrap();
    let atxt = String::from_utf8_lossy(&allow.stdout);
    println!("nslookup wikipedia.org @127.0.0.1:\n{}", atxt);

    // Stop the monitor.
    let conn = nivara_lib::db::open().unwrap();
    nivara_lib::db::set_setting(&conn, "monitor_should_run", "0").unwrap();
    // Verify a termination event was logged.
    let events: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM rule_events WHERE message LIKE 'blocked: terminated%'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    println!("logged termination events: {}", events);
    drop(conn);
    std::thread::sleep(Duration::from_secs(2));
    let _ = monitor.kill();

    assert!(killed, "monitor should have terminated notepad.exe");
    assert!(events >= 1, "a termination event should be logged");
    assert!(btxt.contains("0.0.0.0"), "blocked domain should resolve to 0.0.0.0");
    // wikipedia should NOT be 0.0.0.0 (it forwarded upstream).
    assert!(
        !atxt.contains("Addresses:  0.0.0.0") && !atxt.contains("Address:  0.0.0.0"),
        "non-blocked domain must not be sinkholed"
    );
}
