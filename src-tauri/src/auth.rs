// Password gate. Deterrent only — salted PBKDF2 hashes stored in SQLite.
// Guards stopping the monitor, changing settings, and uninstalling. This is
// NOT tamper-resistance: any user with account access can bypass it (see CLAUDE.md).

use pbkdf2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use pbkdf2::Pbkdf2;
use rand::rngs::OsRng;
use rusqlite::Connection;

/// Hash a secret with a fresh random salt (PBKDF2-HMAC-SHA256, PHC string).
fn hash_secret(secret: &str) -> Result<String, String> {
    let salt = SaltString::generate(&mut OsRng);
    Pbkdf2
        .hash_password(secret.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| e.to_string())
}

fn verify_secret(secret: &str, phc: &str) -> bool {
    match PasswordHash::new(phc) {
        Ok(parsed) => Pbkdf2.verify_password(secret.as_bytes(), &parsed).is_ok(),
        Err(_) => false,
    }
}

pub fn is_set(conn: &Connection) -> bool {
    conn.query_row("SELECT COUNT(*) FROM auth WHERE id = 1", [], |r| {
        r.get::<_, i64>(0)
    })
    .map(|n| n > 0)
    .unwrap_or(false)
}

/// Human-friendly one-time recovery code, e.g. NIVARA-8F3K-2QP7-XM9D.
fn gen_recovery_code() -> String {
    use rand::Rng;
    const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789"; // no ambiguous chars
    let mut rng = rand::thread_rng();
    let mut groups = Vec::new();
    for _ in 0..3 {
        let g: String = (0..4)
            .map(|_| ALPHABET[rng.gen_range(0..ALPHABET.len())] as char)
            .collect();
        groups.push(g);
    }
    format!("NIVARA-{}", groups.join("-"))
}

/// First-run setup. Fails if a password is already set. Returns the plaintext
/// recovery code exactly once (only its hash is persisted).
pub fn setup(conn: &Connection, password: &str) -> Result<String, String> {
    if password.trim().len() < 4 {
        return Err("password must be at least 4 characters".into());
    }
    if is_set(conn) {
        return Err("password already set".into());
    }
    let recovery = gen_recovery_code();
    let pw_hash = hash_secret(password)?;
    let rec_hash = hash_secret(&recovery)?;
    conn.execute(
        "INSERT INTO auth(id, pw_hash, recovery_hash, created_at) VALUES (1, ?1, ?2, ?3)",
        (&pw_hash, &rec_hash, super::db::now_epoch()),
    )
    .map_err(|e| e.to_string())?;
    Ok(recovery)
}

pub fn verify(conn: &Connection, password: &str) -> bool {
    let phc: Option<String> = conn
        .query_row("SELECT pw_hash FROM auth WHERE id = 1", [], |r| r.get(0))
        .ok();
    match phc {
        Some(h) => verify_secret(password, &h),
        None => false,
    }
}

/// Reset the password using the one-time recovery code.
pub fn recover(conn: &Connection, recovery_code: &str, new_password: &str) -> Result<(), String> {
    if new_password.trim().len() < 4 {
        return Err("password must be at least 4 characters".into());
    }
    let rec_hash: String = conn
        .query_row("SELECT recovery_hash FROM auth WHERE id = 1", [], |r| {
            r.get(0)
        })
        .map_err(|_| "no password configured".to_string())?;
    if !verify_secret(recovery_code.trim(), &rec_hash) {
        return Err("invalid recovery code".into());
    }
    let pw_hash = hash_secret(new_password)?;
    conn.execute(
        "UPDATE auth SET pw_hash = ?1 WHERE id = 1",
        [&pw_hash],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
