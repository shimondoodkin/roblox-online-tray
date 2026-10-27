//! A plain-text log of when watched users came online / went offline, kept at
//! %APPDATA%\roblox-online-tray\history.log (one line per change, oldest first).

use std::io::Write;
use std::path::PathBuf;

pub fn path() -> PathBuf {
    crate::config::app_dir().join("history.log")
}

/// Local date and time as ("YYYY-MM-DD", "HH:MM").
pub fn now() -> (String, String) {
    use windows_sys::Win32::System::SystemInformation::GetLocalTime;
    let t = unsafe {
        let mut t = std::mem::zeroed();
        GetLocalTime(&mut t);
        t
    };
    (
        format!("{:04}-{:02}-{:02}", t.wYear, t.wMonth, t.wDay),
        format!("{:02}:{:02}", t.wHour, t.wMinute),
    )
}

pub fn append(line: &str) {
    let _ = std::fs::create_dir_all(crate::config::app_dir());
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path())
    {
        let _ = writeln!(f, "{line}");
    }
}

/// The last `n` lines, oldest first.
pub fn recent(n: usize) -> Vec<String> {
    let text = std::fs::read_to_string(path()).unwrap_or_default();
    let lines: Vec<String> = text.lines().map(str::to_owned).collect();
    lines[lines.len().saturating_sub(n)..].to_vec()
}
