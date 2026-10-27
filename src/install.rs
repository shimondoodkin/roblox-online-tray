//! Self-install / self-uninstall, per user and without admin rights.
//!
//! - Windows: %LOCALAPPDATA%\Programs\RobloxOnlineTray, a Start Menu shortcut, and an
//!   Apps & Features entry whose Uninstall button runs `--uninstall`.
//! - macOS:   a minimal `Roblox Online Tray.app` bundle in ~/Applications.
//! - Linux:   ~/.local/bin, plus an app-menu entry and icon under ~/.local/share.
//!
//! Start-at-login is handled by `autostart`, pointed at the installed copy.

use std::path::Path;

const DISPLAY_NAME: &str = "Roblox Online Tray";

fn same_path(a: &Path, b: &Path) -> bool {
    let norm = |p: &Path| {
        let s = p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
        // Windows paths are case-insensitive.
        if cfg!(windows) {
            s.to_string_lossy().to_ascii_lowercase()
        } else {
            s.to_string_lossy().into_owned()
        }
    };
    norm(a) == norm(b)
}

/// Whether the running exe is the installed copy.
pub fn is_installed() -> bool {
    std::env::current_exe().is_ok_and(|cur| same_path(&cur, &imp::installed_exe()))
}

/// Install (or reinstall over) the per-user copy and start it. The caller then exits.
pub fn install_and_launch(autostart: bool) -> anyhow::Result<()> {
    let exe = imp::installed_exe();
    let current = std::env::current_exe()?;
    if !same_path(&current, &exe) {
        if let Some(dir) = exe.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::copy(&current, &exe).map_err(|e| {
            anyhow::anyhow!("{e} (if Roblox Online Tray is already running, quit it first)")
        })?;
    }
    imp::register(&exe)?;
    crate::autostart::set_enabled_for(&exe, autostart)?;
    imp::launch(&exe)
}

/// Remove the installed copy, its shortcuts and start-at-login; with `remove_data`
/// also the settings, history and saved sign-in. The caller then exits (on Windows
/// the files are deleted by a helper once this process is gone).
pub fn uninstall(remove_data: bool) {
    let _ = crate::autostart::set_enabled(false);
    imp::remove(remove_data);
}

#[cfg(windows)]
mod imp {
    use super::DISPLAY_NAME;
    use std::os::windows::process::CommandExt;
    use std::path::{Path, PathBuf};
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;

    const EXE_NAME: &str = "roblox-online-tray.exe";
    const UNINSTALL_KEY: &str =
        r"Software\Microsoft\Windows\CurrentVersion\Uninstall\RobloxOnlineTray";
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    fn env_dir(var: &str) -> PathBuf {
        PathBuf::from(std::env::var_os(var).unwrap_or_default())
    }

    fn install_dir() -> PathBuf {
        env_dir("LOCALAPPDATA").join(r"Programs\RobloxOnlineTray")
    }

    pub fn installed_exe() -> PathBuf {
        install_dir().join(EXE_NAME)
    }

    fn shortcut_path() -> PathBuf {
        env_dir("APPDATA")
            .join(r"Microsoft\Windows\Start Menu\Programs")
            .join(format!("{DISPLAY_NAME}.lnk"))
    }

    pub fn register(exe: &Path) -> anyhow::Result<()> {
        let (key, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(UNINSTALL_KEY)?;
        key.set_value("DisplayName", &DISPLAY_NAME)?;
        key.set_value("DisplayVersion", &env!("CARGO_PKG_VERSION"))?;
        key.set_value("Publisher", &"Shimon Doodkin")?;
        key.set_value("InstallLocation", &install_dir().display().to_string())?;
        key.set_value("DisplayIcon", &format!("{},0", exe.display()))?;
        key.set_value("UninstallString", &format!("\"{}\" --uninstall", exe.display()))?;
        key.set_value("NoModify", &1u32)?;
        key.set_value("NoRepair", &1u32)?;
        create_shortcut(exe, &shortcut_path());
        Ok(())
    }

    /// A `.lnk` via PowerShell's `WScript.Shell`, best-effort: a missing shortcut is not fatal.
    fn create_shortcut(target: &Path, lnk: &Path) {
        if let Some(parent) = lnk.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let esc = |p: &Path| p.display().to_string().replace('\'', "''");
        let script = format!(
            "$s = (New-Object -ComObject WScript.Shell).CreateShortcut('{}'); \
             $s.TargetPath = '{}'; $s.WorkingDirectory = '{}'; $s.Save()",
            esc(lnk),
            esc(target),
            esc(target.parent().unwrap_or(Path::new("."))),
        );
        let _ = std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
    }

    pub fn launch(exe: &Path) -> anyhow::Result<()> {
        std::process::Command::new(exe).spawn()?;
        Ok(())
    }

    /// A running exe can't delete itself, so a detached `cmd /c` line waits for this
    /// process to exit and then deletes everything. The delay is `ping`, not `timeout`:
    /// `timeout` needs a console and fails instantly in a no-window cmd. The line goes
    /// through `raw_arg` because argv-style quoting mangles the embedded quotes.
    pub fn remove(remove_data: bool) {
        let mut parts = vec![
            format!("taskkill /f /im {EXE_NAME} >NUL 2>&1"),
            "ping -n 4 -w 1000 127.0.0.1 >NUL".to_string(),
            format!("del \"{}\" >NUL 2>&1", shortcut_path().display()),
            format!("reg delete \"HKCU\\{UNINSTALL_KEY}\" /f >NUL 2>&1"),
            format!("rmdir /s /q \"{}\" >NUL 2>&1", install_dir().display()),
        ];
        if remove_data {
            parts.push(format!(
                "rmdir /s /q \"{}\" >NUL 2>&1",
                crate::config::app_dir().display()
            ));
        }
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        let _ = std::process::Command::new("cmd")
            .raw_arg(format!("/c {}", parts.join(" & ")))
            .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW)
            .spawn();
    }

    /// `--uninstall` from Apps & Features: confirm with message boxes, then clean up.
    pub fn run_uninstall_prompt() {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            IDYES, MB_DEFBUTTON2, MB_ICONQUESTION, MB_YESNO, MessageBoxW,
        };
        let ask = |text: &str, flags| {
            let wide = |s: &str| s.encode_utf16().chain([0]).collect::<Vec<u16>>();
            let (text, title) = (wide(text), wide(DISPLAY_NAME));
            unsafe {
                MessageBoxW(std::ptr::null_mut(), text.as_ptr(), title.as_ptr(), flags) == IDYES
            }
        };
        if !ask("Uninstall Roblox Online Tray?", MB_YESNO | MB_ICONQUESTION) {
            return;
        }
        let remove_data = ask(
            "Also delete your settings, history and saved Roblox sign-in?\n\n\
             Choose No to keep them for a later reinstall.",
            MB_YESNO | MB_ICONQUESTION | MB_DEFBUTTON2,
        );
        super::uninstall(remove_data);
    }
}

#[cfg(windows)]
pub use imp::run_uninstall_prompt;

#[cfg(target_os = "macos")]
mod imp {
    use super::DISPLAY_NAME;
    use std::path::{Path, PathBuf};

    fn bundle_dir() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_default()
            .join("Applications")
            .join(format!("{DISPLAY_NAME}.app"))
    }

    pub fn installed_exe() -> PathBuf {
        bundle_dir().join("Contents/MacOS/roblox-online-tray")
    }

    pub fn register(_exe: &Path) -> anyhow::Result<()> {
        let plist = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>{DISPLAY_NAME}</string>
    <key>CFBundleDisplayName</key>
    <string>{DISPLAY_NAME}</string>
    <key>CFBundleIdentifier</key>
    <string>io.github.shimondoodkin.roblox-online-tray</string>
    <key>CFBundleExecutable</key>
    <string>roblox-online-tray</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>{}</string>
    <key>NSHighResolutionCapable</key>
    <true/>
</dict>
</plist>
"#,
            env!("CARGO_PKG_VERSION")
        );
        std::fs::write(bundle_dir().join("Contents/Info.plist"), plist)?;
        // The user already allowed this binary to run; don't make Gatekeeper ask again.
        let _ = std::process::Command::new("xattr")
            .args(["-dr", "com.apple.quarantine"])
            .arg(bundle_dir())
            .output();
        Ok(())
    }

    pub fn launch(_exe: &Path) -> anyhow::Result<()> {
        std::process::Command::new("open").arg(bundle_dir()).spawn()?;
        Ok(())
    }

    pub fn remove(remove_data: bool) {
        let _ = std::fs::remove_dir_all(bundle_dir());
        if remove_data {
            let _ = std::fs::remove_dir_all(crate::config::app_dir());
        }
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
mod imp {
    use super::DISPLAY_NAME;
    use std::path::{Path, PathBuf};

    const ICON_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><rect x="5" y="5" width="54" height="54" rx="16" fill="#4876d6" stroke="#2b478a" stroke-width="3"/><path d="M26 19 L44 32 L26 45 Z" fill="#fff"/></svg>"##;

    fn home() -> PathBuf {
        dirs::home_dir().unwrap_or_default()
    }

    pub fn installed_exe() -> PathBuf {
        home().join(".local/bin/roblox-online-tray")
    }

    fn desktop_file() -> PathBuf {
        home().join(".local/share/applications/roblox-online-tray.desktop")
    }

    fn icon_file() -> PathBuf {
        home().join(".local/share/icons/hicolor/scalable/apps/roblox-online-tray.svg")
    }

    pub fn register(exe: &Path) -> anyhow::Result<()> {
        for path in [desktop_file(), icon_file()] {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
        }
        std::fs::write(icon_file(), ICON_SVG)?;
        std::fs::write(
            desktop_file(),
            format!(
                "[Desktop Entry]\nType=Application\nName={DISPLAY_NAME}\n\
                 Comment=Shows when your Roblox friends come online\n\
                 Exec=\"{}\"\nIcon={}\nCategories=Utility;\nTerminal=false\n",
                exe.display(),
                icon_file().display()
            ),
        )?;
        Ok(())
    }

    pub fn launch(exe: &Path) -> anyhow::Result<()> {
        use std::os::unix::process::CommandExt;
        // Own process group, so closing a terminal this was started from doesn't stop it.
        std::process::Command::new(exe).process_group(0).spawn()?;
        Ok(())
    }

    pub fn remove(remove_data: bool) {
        for path in [installed_exe(), desktop_file(), icon_file()] {
            let _ = std::fs::remove_file(path);
        }
        if remove_data {
            let _ = std::fs::remove_dir_all(crate::config::app_dir());
        }
    }
}
