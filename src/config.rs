//! Settings persisted as JSON under %APPDATA%\roblox-online-tray.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Roblox usernames to watch.
    pub usernames: Vec<String>,
    pub interval_secs: u64,
    pub notify: bool,
    /// Keep history.log of online / offline times. Off unless the user turns it on.
    pub log_history: bool,
}

/// Most usernames that can be watched at once.
pub const MAX_USERNAMES: usize = 6;

impl Default for Config {
    fn default() -> Self {
        Self {
            usernames: Vec::new(),
            interval_secs: 60,
            notify: true,
            log_history: false,
        }
    }
}

/// %APPDATA% on Windows, ~/Library/Application Support on macOS, ~/.config on Linux.
pub fn app_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("roblox-online-tray")
}

fn config_path() -> PathBuf {
    app_dir().join("config.json")
}

impl Config {
    pub fn load() -> Self {
        let mut cfg: Self = std::fs::read_to_string(config_path())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        cfg.usernames.truncate(MAX_USERNAMES);
        cfg
    }

    pub fn save(&self) -> anyhow::Result<()> {
        std::fs::create_dir_all(app_dir())?;
        std::fs::write(config_path(), serde_json::to_string_pretty(self)?)?;
        Ok(())
    }
}
