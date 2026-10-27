//! Chrome is used only to obtain the `.ROBLOSECURITY` cookie: the user signs in to a
//! dedicated Chrome profile, and we read the cookie back over the DevTools protocol
//! (`Storage.getCookies`). After that, all status checks are plain HTTP (see `http.rs`)
//! and Chrome is not involved. We never decrypt Chrome's on-disk cookie store.

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

/// 9222 and 9333 are taken by other tools on this machine.
pub const PORT: u16 = 9334;
const LOGIN_URL: &str = "https://www.roblox.com/login";

pub fn profile_dir() -> PathBuf {
    crate::config::app_dir().join("chrome-profile")
}

/// Locate chrome.exe (Edge as a fallback). `CHROME_PATH` overrides.
pub fn find_chrome() -> Result<PathBuf> {
    if let Ok(p) = std::env::var("CHROME_PATH") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Ok(p);
        }
        bail!("CHROME_PATH={} is not a file", p.display());
    }
    let mut candidates = Vec::new();
    for var in ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"] {
        if let Ok(base) = std::env::var(var) {
            let base = PathBuf::from(base);
            candidates.push(base.join(r"Google\Chrome\Application\chrome.exe"));
            candidates.push(base.join(r"Microsoft\Edge\Application\msedge.exe"));
        }
    }
    candidates
        .into_iter()
        .find(|p| p.is_file())
        .context("Chrome not found; set CHROME_PATH")
}

fn chrome_args(profile: &Path, headless: bool, start_url: &str) -> Vec<String> {
    let mut a = vec![
        format!("--remote-debugging-port={PORT}"),
        format!("--user-data-dir={}", profile.display()),
        "--no-first-run".into(),
        "--no-default-browser-check".into(),
        "--disable-sync".into(),
        "--hide-crash-restore-bubble".into(),
        "--disable-session-crashed-bubble".into(),
    ];
    if headless {
        a.push("--headless=new".into());
        a.push("--window-size=640,480".into());
    } else {
        a.push("--window-size=1000,800".into());
    }
    a.push(start_url.into());
    a
}

/// The browser websocket debugger URL, or `None` when nothing answers on the port.
fn ws_url() -> Option<String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_millis(1500)))
        .build()
        .into();
    let mut resp = agent
        .get(&format!("http://127.0.0.1:{PORT}/json/version"))
        .call()
        .ok()?;
    let v: Value = resp.body_mut().read_json().ok()?;
    v["webSocketDebuggerUrl"].as_str().map(str::to_owned)
}

/// Spawn Chrome if nothing is answering on our port. Returns whether we spawned it
/// (so a headless probe can be closed again) and the websocket URL.
fn ensure_chrome(headless: bool, start_url: &str) -> Result<(String, bool)> {
    if let Some(url) = ws_url() {
        return Ok((url, false));
    }
    let profile = profile_dir();
    std::fs::create_dir_all(&profile)?;
    let exe = find_chrome()?;
    let mut cmd = std::process::Command::new(&exe);
    cmd.args(chrome_args(&profile, headless, start_url))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        if headless {
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
    }
    cmd.spawn()
        .with_context(|| format!("spawning {}", exe.display()))?;

    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(url) = ws_url() {
            return Ok((url, true));
        }
        if Instant::now() > deadline {
            bail!("Chrome did not open the debug port {PORT} within 20s");
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

/// Open the visible sign-in window so the user can log in; it stays open.
pub fn open_login() -> Result<()> {
    ensure_chrome(false, LOGIN_URL)?;
    Ok(())
}

/// Read `.ROBLOSECURITY` from the dedicated profile over CDP. Uses a running browser
/// if one is up (e.g. the login window); otherwise launches a brief headless instance
/// just to read the cookie, then closes it. Returns `None` if no cookie is stored yet.
pub fn capture_cookie() -> Result<Option<String>> {
    let (url, spawned) = ensure_chrome(true, "about:blank")?;
    let mut cdp = Cdp::connect(&url)?;
    let result = (|| {
        let v = cdp.call("Storage.getCookies", json!({}), Duration::from_secs(10))?;
        let cookies = v["cookies"].as_array().cloned().unwrap_or_default();
        for c in cookies {
            if c["name"] == ".ROBLOSECURITY" {
                if let Some(val) = c["value"].as_str() {
                    if !val.is_empty() {
                        return Ok(Some(val.to_string()));
                    }
                }
            }
        }
        Ok(None)
    })();
    // Only close a browser we spawned headless ourselves; never the user's login window.
    if spawned {
        let _ = cdp.call("Browser.close", json!({}), Duration::from_secs(5));
    }
    result
}

type Ws = WebSocket<MaybeTlsStream<TcpStream>>;

struct Cdp {
    ws: Ws,
    next_id: u64,
}

impl Cdp {
    fn connect(url: &str) -> Result<Self> {
        let (ws, _) = tungstenite::connect(url).context("connecting to CDP")?;
        // Without this, `read()` blocks forever if Chrome stops answering, and the
        // per-call deadline is never checked.
        if let MaybeTlsStream::Plain(s) = ws.get_ref() {
            s.set_read_timeout(Some(Duration::from_secs(10)))?;
        }
        Ok(Self { ws, next_id: 1 })
    }

    /// Send a browser-level CDP command and wait for its matching reply.
    fn call(&mut self, method: &str, params: Value, timeout: Duration) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        self.ws.send(Message::Text(
            json!({ "id": id, "method": method, "params": params })
                .to_string()
                .into(),
        ))?;

        let deadline = Instant::now() + timeout;
        loop {
            if Instant::now() > deadline {
                bail!("CDP timeout waiting for {method}");
            }
            let Message::Text(text) = self.ws.read()? else {
                continue;
            };
            let v: Value = serde_json::from_str(&text)?;
            if v["id"].as_u64() == Some(id) {
                if let Some(err) = v.get("error").filter(|e| !e.is_null()) {
                    bail!("CDP error on {method}: {err}");
                }
                return Ok(v["result"].clone());
            }
        }
    }
}
