//! "Start at login", per user (no admin rights needed). It starts the app with
//! `--tray`, so it launches hidden in the tray.
//!
//! - Windows: the per-user Run registry key.
//! - macOS:   a LaunchAgent plist in ~/Library/LaunchAgents.
//! - Linux:   an XDG autostart .desktop file in ~/.config/autostart.

#[cfg(windows)]
mod imp {
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
}

#[cfg(not(windows))]
mod imp {
    use std::path::PathBuf;

    #[cfg(target_os = "macos")]
    fn entry_path() -> Option<PathBuf> {
        Some(
            dirs::home_dir()?
                .join("Library/LaunchAgents/io.github.shimondoodkin.roblox-online-tray.plist"),
        )
    }

    #[cfg(target_os = "macos")]
    fn entry_contents(exe: &str) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>io.github.shimondoodkin.roblox-online-tray</string>
    <key>ProgramArguments</key>
    <array>
        <string>{exe}</string>
        <string>--tray</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
</dict>
</plist>
"#
        )
    }

    #[cfg(not(target_os = "macos"))]
    fn entry_path() -> Option<PathBuf> {
        Some(dirs::config_dir()?.join("autostart/roblox-online-tray.desktop"))
    }

    #[cfg(not(target_os = "macos"))]
    fn entry_contents(exe: &str) -> String {
        format!(
            "[Desktop Entry]\nType=Application\nName=Roblox Online Tray\n\
             Exec=\"{exe}\" --tray\nX-GNOME-Autostart-enabled=true\n"
        )
    }

    pub fn is_enabled() -> bool {
        entry_path().is_some_and(|p| p.exists())
    }

    pub fn set_enabled(enabled: bool) -> anyhow::Result<()> {
        let path = entry_path().ok_or_else(|| anyhow::anyhow!("no home directory"))?;
        if enabled {
            let exe = std::env::current_exe()?;
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::write(&path, entry_contents(&exe.display().to_string()))?;
        } else {
            match std::fs::remove_file(&path) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
                _ => {}
            }
        }
        Ok(())
    }
}

pub use imp::{is_enabled, set_enabled};
