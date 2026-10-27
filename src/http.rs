//! Direct Roblox web-API calls authenticated with the `.ROBLOSECURITY` cookie.
//!
//! This is the same approach the Python Roblox libraries use: send the cookie as a
//! `Cookie:` header, and for POST endpoints attach an `X-CSRF-TOKEN`. Roblox returns
//! that token in the `x-csrf-token` response header on the first 403; we cache it and
//! resend. Roblox also rotates `.ROBLOSECURITY` via `Set-Cookie`, so we pick up a new
//! value whenever one comes back.

use crate::model::Presence;
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::time::Duration;

const UA: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
     (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36";

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        // Keep 4xx as normal responses so we can read the x-csrf-token header off a 403.
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(15)))
        .build()
        .into()
}

/// Holds the cookie and the CSRF token across calls. The cookie may be updated when
/// Roblox rotates it; callers should persist `cookie` after use.
pub struct Session {
    pub cookie: String,
    csrf: Option<String>,
    agent: ureq::Agent,
}

impl Session {
    pub fn new(cookie: String) -> Self {
        Self {
            cookie,
            csrf: None,
            agent: agent(),
        }
    }

    fn cookie_header(&self) -> String {
        format!(".ROBLOSECURITY={}", self.cookie)
    }

    /// Note a rotated `.ROBLOSECURITY` from Set-Cookie, if present and plausible.
    fn absorb_set_cookie(&mut self, headers: &ureq::http::HeaderMap) {
        for v in headers.get_all("set-cookie") {
            let Ok(s) = v.to_str() else { continue };
            if let Some(val) = parse_cookie(s, ".ROBLOSECURITY") {
                if crate::session::is_plausible(&val) {
                    self.cookie = val;
                }
            }
        }
    }

    /// GET returning parsed JSON, or the HTTP status on failure.
    fn get(&mut self, url: &str) -> Result<(u16, Value)> {
        let mut resp = self
            .agent
            .get(url)
            .header("User-Agent", UA)
            .header("Cookie", &self.cookie_header())
            .call()
            .with_context(|| format!("GET {url}"))?;
        let status = resp.status().as_u16();
        self.absorb_set_cookie(resp.headers());
        let body: Value = resp.body_mut().read_json().unwrap_or(Value::Null);
        Ok((status, body))
    }

    /// POST JSON, retrying once with a fresh CSRF token on a 403.
    fn post(&mut self, url: &str, body: &Value) -> Result<(u16, Value)> {
        for attempt in 0..2 {
            let mut req = self
                .agent
                .post(url)
                .header("User-Agent", UA)
                .header("Content-Type", "application/json")
                .header("Cookie", &self.cookie_header());
            if let Some(t) = &self.csrf {
                req = req.header("X-CSRF-TOKEN", t);
            }
            let mut resp = req
                .send_json(body)
                .with_context(|| format!("POST {url}"))?;
            let status = resp.status().as_u16();

            if let Some(t) = resp
                .headers()
                .get("x-csrf-token")
                .and_then(|v| v.to_str().ok())
            {
                self.csrf = Some(t.to_string());
            }
            self.absorb_set_cookie(resp.headers());

            if status == 403 && attempt == 0 && self.csrf.is_some() {
                continue; // retry once now that we have the token
            }
            let json: Value = resp.body_mut().read_json().unwrap_or(Value::Null);
            return Ok((status, json));
        }
        bail!("CSRF retry failed for {url}")
    }

    /// Is this cookie currently a valid, signed-in session?
    pub fn authenticated(&mut self) -> Result<bool> {
        let (status, _) = self.get("https://users.roblox.com/v1/users/authenticated")?;
        Ok(status == 200)
    }

    /// Resolve a username to (id, canonical name, display name).
    pub fn resolve_username(&mut self, username: &str) -> Result<Option<(u64, String, String)>> {
        let body = json!({ "usernames": [username], "excludeBannedUsers": true });
        let (status, v) = self.post("https://users.roblox.com/v1/usernames/users", &body)?;
        if status == 401 {
            bail!("unauthorized");
        }
        let Some(first) = v["data"].as_array().and_then(|a| a.first()) else {
            return Ok(None);
        };
        let id = first["id"].as_u64().context("missing user id")?;
        let name = first["name"].as_str().unwrap_or(username).to_owned();
        let display = first["displayName"].as_str().unwrap_or(&name).to_owned();
        Ok(Some((id, name, display)))
    }

    /// Presence for several user ids at once.
    pub fn presence(&mut self, ids: &[u64]) -> Result<Vec<(u64, Presence)>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let body = json!({ "userIds": ids });
        let (status, v) = self.post("https://presence.roblox.com/v1/presence/users", &body)?;
        if status == 401 {
            bail!("unauthorized");
        }
        let arr = v["userPresences"].as_array().cloned().unwrap_or_default();
        let mut out = Vec::new();
        for p in arr {
            let id = p["userId"].as_u64().unwrap_or(0);
            let kind = match p["userPresenceType"].as_u64().unwrap_or(0) {
                0 => Presence::Offline,
                1 => Presence::Online,
                2 => Presence::InGame {
                    place: p["lastLocation"].as_str().unwrap_or("a game").to_owned(),
                },
                3 => Presence::InStudio,
                _ => Presence::Unknown,
            };
            out.push((id, kind));
        }
        Ok(out)
    }
}

/// Extract `name=value` from a Set-Cookie header line.
fn parse_cookie(set_cookie: &str, name: &str) -> Option<String> {
    let first = set_cookie.split(';').next()?.trim();
    let (k, v) = first.split_once('=')?;
    if k.trim() == name && !v.trim().is_empty() {
        Some(v.trim().to_string())
    } else {
        None
    }
}
