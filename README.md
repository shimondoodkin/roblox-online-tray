# Roblox Online Tray

A small tray app (Rust + egui) that watches up to 6 Roblox users and tells you when
they come online or go offline. It was made for checking whether your kid is online
so you can play together.

## Download

**[Download the latest release](https://github.com/shimondoodkin/roblox-online-tray/releases/latest)**

| System  | File                                     | Status       |
|---------|------------------------------------------|--------------|
| Windows | `roblox-online-tray-windows-x64.zip`     | tested       |
| macOS   | `roblox-online-tray-macos-universal.tar.gz` | untested  |
| Linux   | `roblox-online-tray-linux-x64.tar.gz`    | untested     |

All builds are on the [releases page](https://github.com/shimondoodkin/roblox-online-tray/releases).
The macOS and Linux builds compile in CI but have not been tried on a real machine yet.

- **Windows:** unzip and run `roblox-online-tray.exe`. SmartScreen may warn because the
  exe isn't signed: **More info → Run anyway**.
- **macOS:** extract, then run `./roblox-online-tray` from Terminal. The binary isn't
  signed, so the first time you may need to allow it in **System Settings → Privacy &
  Security**.
- **Linux:** extract and run `./roblox-online-tray`. It needs GTK 3 and an
  AppIndicator-capable tray (`libayatana-appindicator3`).

## Usage

1. **Log in with Chrome** and sign in (an account that's **friends** with the person).
2. Enter their Roblox username(s), one per line (up to 6), then **Save & check**.
3. The tray icon turns **green** when someone you watch is online or in a game; hover it
   for details, or left-click it to open the window. When someone comes online or goes
   offline, a small popup appears in the bottom-right corner with the date and time. It
   stays until you close it with ✕.

Closing the window keeps the app running in the tray. Right-click the tray icon for
**Open window / Check now / Quit**.

### Options
- **Start at login** — per-user, no admin; launches hidden in the tray.
- **Pop up when someone comes online / goes offline** — **Preview** shows a sample
  (`--preview-popup` on the command line does the same at startup).
- **Check every N seconds** — polling interval (15–600 s).
- **Keep a history of online / offline times** — off by default. When on, each change
  is saved with its date and time to `history.log`, listed in the window newest first;
  **Open log file** opens the whole log.

## How it works

Status checks are plain HTTPS requests to Roblox's web API, authenticated with your
`.ROBLOSECURITY` session cookie (the same approach the Python Roblox libraries use):

- `GET  users.roblox.com/v1/users/authenticated`   — is the session valid
- `POST users.roblox.com/v1/usernames/users`        — username → user id
- `POST presence.roblox.com/v1/presence/users`      — online / in-game status

No browser runs during checks. A browser is used **only** to obtain the cookie the
first time (see below).

### Why a login is needed
Roblox shows a younger account's online activity only to their **friends**. A
logged-out request just reports them offline, so you must sign in with an account that
is friends with the person you're watching.

### Getting the cookie
Two ways, both store the cookie in the app's own folder:

1. **Log in with Chrome** (default) — opens a dedicated Chrome profile at the Roblox
   login page. You sign in; the app then reads `.ROBLOSECURITY` back over Chrome's
   DevTools protocol (`Storage.getCookies`) and stores it. You can close that window.
   The app never decrypts your everyday Chrome's cookie store.
2. **Paste a cookie** — expand "or paste a .ROBLOSECURITY cookie" and paste it yourself
   (e.g. from your browser's DevTools → Application → Cookies).

Roblox rotates the token periodically; the app picks up the new value automatically.

## Requirements
- Windows, macOS or Linux
- Google Chrome, Microsoft Edge or Chromium — only for the one-time login/cookie
  capture. Set `CHROME_PATH` if it's installed somewhere unusual.

## Building from source

```bash
cargo run --release
```

On Linux, first install `libgtk-3-dev libxdo-dev libayatana-appindicator3-dev`.

Releases are built by GitHub Actions: pushing a tag that starts with `v` (for example
`v0.1.0`) builds all three systems and publishes them as a release.

## Files & security
Everything lives in the app folder: `%APPDATA%\roblox-online-tray\` on Windows,
`~/Library/Application Support/roblox-online-tray/` on macOS,
`~/.config/roblox-online-tray/` on Linux.

- `config.json`     — watched usernames and options
- `history.log`     — online / offline times (only if history is turned on)
- `session.dat`     — your `.ROBLOSECURITY` cookie **(sensitive!)**
- `chrome-profile/` — the dedicated login profile (also contains the cookie)

Anyone who can read `session.dat` (or the profile) can act as your Roblox account until
you sign out of it. Use **Sign out** in the app to delete the stored cookie.

Never paste your `.ROBLOSECURITY` cookie into a program you don't trust. The cookie
stays on your computer and is sent only to roblox.com.
