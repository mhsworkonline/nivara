// User-level autostart via HKCU Run key. No elevation required.

use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};
use winreg::RegKey;

const RUN_PATH: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE_NAME: &str = "NivaraMonitor";

fn monitor_exe_path() -> std::io::Result<String> {
    let exe = std::env::current_exe()?;
    let dir = exe.parent().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "no parent dir")
    })?;
    let monitor = dir.join("nivara-monitor.exe");
    let target = if monitor.exists() { monitor } else { exe };
    Ok(format!("\"{}\"", target.display()))
}

pub fn is_enabled() -> bool {
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(RUN_PATH, KEY_READ)
        .and_then(|k| k.get_value::<String, _>(VALUE_NAME))
        .is_ok()
}

pub fn enable() -> std::io::Result<()> {
    let cmd = monitor_exe_path()?;
    let (key, _) = RegKey::predef(HKEY_CURRENT_USER)
        .create_subkey_with_flags(RUN_PATH, KEY_WRITE)?;
    key.set_value(VALUE_NAME, &cmd)
}

pub fn disable() -> std::io::Result<()> {
    let key = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(RUN_PATH, KEY_WRITE)?;
    match key.delete_value(VALUE_NAME) {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}
