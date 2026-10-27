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
}

impl Default for Config {
    fn default() -> Self {
        Self {
            usernames: Vec::new(),
            interval_secs: 60,
            notify: true,
        }
    }
}

pub fn app_dir() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("roblox-online-tray")
}

fn config_path() -> PathBuf {
    app_dir().join("config.json")
}

impl Config {
    pub fn load() -> Self {
        std::fs::read_to_string(config_path())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> anyhow::Result<()> {
        std::fs::create_dir_all(app_dir())?;
        std::fs::write(config_path(), serde_json::to_string_pretty(self)?)?;
        Ok(())
    }
}
