//! Storage for the Roblox session cookie (`.ROBLOSECURITY`).
//!
//! This is a sensitive value: anyone who can read it can act as your Roblox
//! account until it is invalidated. It is kept in the app's own config folder,
//! which already contains the full signed-in Chrome profile, so this adds no new
//! exposure beyond what logging in already created.

use std::path::PathBuf;

fn cookie_path() -> PathBuf {
    crate::config::app_dir().join("session.dat")
}

pub fn load() -> Option<String> {
    let s = std::fs::read_to_string(cookie_path()).ok()?;
    let s = s.trim().to_string();
    if is_plausible(&s) { Some(s) } else { None }
}

pub fn save(cookie: &str) -> anyhow::Result<()> {
    std::fs::create_dir_all(crate::config::app_dir())?;
    std::fs::write(cookie_path(), cookie.trim())?;
    Ok(())
}

pub fn clear() {
    let _ = std::fs::remove_file(cookie_path());
}

/// A real `.ROBLOSECURITY` starts with the warning banner Roblox prepends and is
/// long. This rejects blanks, "DELETED", and obviously truncated paste errors.
pub fn is_plausible(cookie: &str) -> bool {
    let c = cookie.trim();
    c.len() > 100 && (c.starts_with("_|WARNING") || c.contains("_|WARNING"))
}
