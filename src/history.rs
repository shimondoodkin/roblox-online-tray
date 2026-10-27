//! A plain-text log of when watched users came online / went offline, kept as
//! history.log in the app folder (one line per change, oldest first). Opt-in.

use std::io::Write;
use std::path::PathBuf;

pub fn path() -> PathBuf {
    crate::config::app_dir().join("history.log")
}

/// Local date and time as ("YYYY-MM-DD", "HH:MM").
pub fn now() -> (String, String) {
    let t = chrono::Local::now();
    (t.format("%Y-%m-%d").to_string(), t.format("%H:%M").to_string())
}

/// Open the log in the system's text editor.
pub fn open_in_editor() {
    let path = path();
    // Create it empty if needed, so the editor has something to open.
    let _ = std::fs::create_dir_all(crate::config::app_dir());
    let _ = std::fs::OpenOptions::new().create(true).append(true).open(&path);
    #[cfg(windows)]
    let mut cmd = std::process::Command::new("notepad");
    #[cfg(target_os = "macos")]
    let mut cmd = {
        let mut c = std::process::Command::new("open");
        c.arg("-t");
        c
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut cmd = std::process::Command::new("xdg-open");
    let _ = cmd.arg(path).spawn();
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
