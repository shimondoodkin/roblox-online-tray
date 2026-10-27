# Roblox Online Tray

A small Windows tray app (Rust + egui) that watches one or more Roblox users and
tells you when they come online — made for checking whether your kid is online to
play together.

## How it works

Status checks are plain HTTPS requests to Roblox's web API, authenticated with your
`.ROBLOSECURITY` session cookie (the same approach the Python Roblox libraries use):

- `GET  users.roblox.com/v1/users/authenticated`   — is the session valid
- `POST users.roblox.com/v1/usernames/users`        — username → user id
- `POST presence.roblox.com/v1/presence/users`      — online / in-game status

No browser runs during checks, so there is no pop-up window. A browser is used **only**
to obtain the cookie the first time (see below).

### Why a login is needed
Roblox hides a younger account's online activity from everyone except their **friends**.
A logged-out request just reports them offline. So you must sign in with an account that
is friends with the person you're watching.

### Getting the cookie
Two ways, both land the cookie in the app's own config folder:

1. **Log in with Chrome** (default) — opens a dedicated Chrome profile at the Roblox
   login page. You sign in; the app then reads `.ROBLOSECURITY` back over Chrome's
   DevTools protocol (`Storage.getCookies`) and stores it. You can close that window.
   The app never decrypts your everyday Chrome's cookie store.
2. **Paste a cookie** — expand "or paste a .ROBLOSECURITY cookie" and paste it yourself
   (e.g. from your browser's DevTools → Application → Cookies).

Roblox rotates the token periodically; the app picks up the new value automatically
from responses.

## Usage

```bash
cargo run --release
```

1. **Log in with Chrome** and sign in (an account that's **friends** with the person).
2. Enter their Roblox username(s), one per line, **Save & check**.
3. The tray icon turns **green** when someone you watch is online or in a game; hover it
   for details (left-click it to open the window). When someone comes online or goes
   offline, a small popup appears above the clock with the date and time; it stays until
   you close it with ✕.

Closing the window keeps the app running in the tray. Right-click the tray icon for
**Open window / Check now / Quit**.

### History
Every online / offline change is logged with its date and time, whether or not the
popup is on. The **History** section of the window lists them newest first, and
**Open log file** opens the full log in Notepad.

### Options
- **Start with Windows** — per-user, no admin; launches hidden in the tray at login.
- **Pop up when someone comes online / goes offline** — the popup above; **Preview**
  shows a sample. (`--preview-popup` on the command line does the same at startup.)
- **Check every N seconds** — polling interval (15–600 s).

## Requirements
- Windows
- Google Chrome (or Microsoft Edge) — only for the one-time login/cookie capture. Set
  `CHROME_PATH` if it's installed somewhere unusual.

## Files & security
Everything lives under `%APPDATA%\roblox-online-tray\`:
- `config.json`   — watched usernames and options
- `history.log`   — online / offline times, one line per change
- `session.dat`   — your `.ROBLOSECURITY` cookie **(sensitive!)**
- `chrome-profile\` — the dedicated login profile (also contains the cookie)

Anyone who can read `session.dat` (or the profile) can act as your Roblox account until
you sign out of it, so keep this machine/account to yourself. Use **Sign out** in the
app to delete the stored cookie.
