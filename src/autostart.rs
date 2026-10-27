//! "Start with Windows" via the per-user Run key (no admin rights needed).

use winreg::RegKey;
use winreg::enums::HKEY_CURRENT_USER;

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE_NAME: &str = "RobloxOnlineTray";

pub fn is_enabled() -> bool {
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(RUN_KEY)
        .and_then(|k| k.get_value::<String, _>(VALUE_NAME))
        .is_ok()
}

/// Registers the current exe with `--tray`, so it starts hidden in the tray.
pub fn set_enabled(enabled: bool) -> anyhow::Result<()> {
    let (key, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(RUN_KEY)?;
    if enabled {
        let exe = std::env::current_exe()?;
        key.set_value(VALUE_NAME, &format!("\"{}\" --tray", exe.display()))?;
    } else {
        match key.delete_value(VALUE_NAME) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
            _ => {}
        }
    }
    Ok(())
}
