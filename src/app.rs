//! The egui window, the tray glue, and the background worker that performs the
//! status checks over HTTP using the stored `.ROBLOSECURITY` cookie.

use crate::http::Session;
use crate::model::Presence;
use crate::popup::Popup;
use crate::tray::{self, Dot, Tray};
use crate::config::{Config, MAX_USERNAMES};
use crate::{autostart, browser, history, install, session};
use eframe::egui;
use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender};
use std::time::{Duration, Instant};

/// One watched user as shown in the window.
#[derive(Clone)]
pub struct Entry {
    pub username: String,
    pub display: String,
    pub presence: Presence,
}

/// Commands sent from the UI to the worker.
pub enum Cmd {
    Refresh,
    /// Open the login window and capture the cookie once signed in.
    OpenLogin,
    /// Use a cookie the user pasted in directly.
    SetCookie(String),
    SetUsernames(Vec<String>),
}

/// Events sent from the worker back to the UI.
pub enum Evt {
    Status(String),
    LoggedIn(bool),
    Entries(Vec<Entry>),
    /// Someone came online or went offline.
    Popup(String),
    /// A line that was just appended to the history log.
    History(String),
}

pub struct App {
    cfg: Config,
    usernames_text: String,
    autostart_on: bool,

    entries: Vec<Entry>,
    status: String,
    logged_in: bool,

    cookie_input: String,

    /// Online/offline log, oldest first (also in history.log).
    history: Vec<String>,

    /// Running from the installed location: show Uninstall instead of Install.
    installed: bool,
    install_autostart: bool,
    confirm_uninstall: bool,
    uninstall_data: bool,

    tray: Tray,
    popup: Popup,
    to_worker: Sender<Cmd>,
    from_worker: Receiver<Evt>,

    start_hidden: bool,
    applied_hidden: bool,
}

impl App {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        start_hidden: bool,
        preview_popup: bool,
    ) -> anyhow::Result<Self> {
        let cfg = Config::load();
        let usernames_text = cfg.usernames.join("\n");
        let tray = Tray::new()?;

        let (to_worker, worker_rx) = std::sync::mpsc::channel();
        let (worker_tx, from_worker) = std::sync::mpsc::channel();
        spawn_worker(cc.egui_ctx.clone(), worker_rx, worker_tx, cfg.clone());

        // Prime the worker with the saved list and kick an immediate check.
        let _ = to_worker.send(Cmd::SetUsernames(cfg.usernames.clone()));
        let _ = to_worker.send(Cmd::Refresh);

        let mut popup = Popup::default();
        if preview_popup {
            let (date, time) = history::now();
            popup.push(format!("{date} {time}  user NAME is online"));
            popup.push(format!("{date} {time}  user NAME is offline"));
        }

        Ok(Self {
            autostart_on: autostart::is_enabled(),
            usernames_text,
            cfg,
            entries: Vec::new(),
            status: "Starting…".into(),
            logged_in: false,
            cookie_input: String::new(),
            history: history::recent(200),
            installed: install::is_installed(),
            install_autostart: true,
            confirm_uninstall: false,
            uninstall_data: false,
            tray,
            popup,
            to_worker,
            from_worker,
            start_hidden,
            applied_hidden: false,
        })
    }

    fn save_usernames(&mut self) {
        let list: Vec<String> = self
            .usernames_text
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();
        if list.len() > MAX_USERNAMES {
            self.status = format!("Only the first {MAX_USERNAMES} usernames are watched.");
        }
        let list: Vec<String> = list.into_iter().take(MAX_USERNAMES).collect();
        self.usernames_text = list.join("\n");
        self.cfg.usernames = list.clone();
        let _ = self.cfg.save();
        let _ = self.to_worker.send(Cmd::SetUsernames(list));
        let _ = self.to_worker.send(Cmd::Refresh);
    }

    fn drain_worker(&mut self) {
        while let Ok(evt) = self.from_worker.try_recv() {
            match evt {
                Evt::Status(s) => self.status = s,
                Evt::LoggedIn(b) => self.logged_in = b,
                Evt::Entries(e) => self.entries = e,
                Evt::Popup(line) => self.popup.push(line),
                Evt::History(line) => self.history.push(line),
            }
        }
    }

    fn update_tray(&mut self) {
        let any_online = self.entries.iter().any(|e| {
            matches!(e.presence, Presence::Online | Presence::InGame { .. })
        });
        let dot = if !self.logged_in {
            Dot::Unknown
        } else if any_online {
            Dot::Online
        } else {
            Dot::Idle
        };
        let tip = if self.entries.is_empty() {
            "Roblox Online Tray — no users watched".to_string()
        } else {
            let mut lines = vec![];
            for e in &self.entries {
                lines.push(format!("{}: {}", e.display, presence_label(&e.presence)));
            }
            lines.join("\n")
        };
        // Tooltips are limited to 128 chars on Windows.
        let tip: String = tip.chars().take(120).collect();
        self.tray.set(dot, &tip);
    }

    fn poll_tray_menu(&mut self, ctx: &egui::Context) {
        while let Ok(ev) = tray_icon::menu::MenuEvent::receiver().try_recv() {
            let id = ev.id.0.as_str();
            if id == tray::ID_OPEN {
                show_window(ctx);
            } else if id == tray::ID_CHECK {
                let _ = self.to_worker.send(Cmd::Refresh);
            } else if id == tray::ID_QUIT {
                // `exit` skips Drop, so remove the icon first or Windows leaves a
                // ghost icon in the tray until the mouse passes over it.
                self.tray.remove();
                std::process::exit(0);
            }
        }
        // Left-click on the icon opens the window; other clicks are just drained.
        while let Ok(ev) = tray_icon::TrayIconEvent::receiver().try_recv() {
            if let tray_icon::TrayIconEvent::Click {
                button: tray_icon::MouseButton::Left,
                button_state: tray_icon::MouseButtonState::Up,
                ..
            } = ev
            {
                show_window(ctx);
            }
        }
    }
}

fn show_window(ctx: &egui::Context) {
    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
}

impl eframe::App for App {
    // Fully transparent, so the popup's rounded corners show the desktop. The main
    // window paints its own opaque panel and is unaffected.
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0; 4]
    }

    // Runs every frame AND while the window is hidden (when a repaint is queued),
    // so the tray keeps working after the window is closed to tray.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.start_hidden && !self.applied_hidden {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            self.applied_hidden = true;
        }

        // Close button hides to tray instead of quitting.
        if ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }

        self.drain_worker();
        self.poll_tray_menu(ctx);
        self.update_tray();
        self.popup.show(ctx);

        // Heartbeat so this runs again even while hidden.
        ctx.request_repaint_after(Duration::from_millis(250));
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // eframe 0.37 hands `ui` a surface with no background; fill it with the
        // themed panel colour so text isn't drawn on bare black.
        let style = ui.style().clone();
        // The frame only covers the content's height, and the clear colour is
        // transparent (for the popup), so paint the whole window first.
        ui.painter()
            .rect_filled(ui.max_rect(), 0.0, style.visuals.panel_fill);
        egui::Frame::central_panel(&style).show(ui, |ui| {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.heading("Roblox Online Tray");
            ui.add_space(4.0);

            if !self.installed {
                ui.label("Not installed yet — it is running from where you downloaded it.");
                ui.horizontal(|ui| {
                    if ui.button("Install").clicked() {
                        match install::install_and_launch(self.install_autostart) {
                            // The installed copy is starting; this one gets out of its way.
                            Ok(()) => {
                                self.tray.remove();
                                std::process::exit(0);
                            }
                            Err(e) => self.status = format!("Install failed: {e}"),
                        }
                    }
                    ui.checkbox(&mut self.install_autostart, "Start at login");
                });
                ui.separator();
            }

            if self.logged_in {
                ui.horizontal(|ui| {
                    ui.colored_label(
                        egui::Color32::from_rgb(46, 204, 113),
                        "✔ Signed in to Roblox",
                    );
                    if ui.small_button("Sign out").clicked() {
                        let _ = self.to_worker.send(Cmd::SetCookie(String::new()));
                    }
                });
            } else {
                ui.colored_label(egui::Color32::from_rgb(230, 170, 40), "Not signed in");
                ui.label(
                    "A kid's online status is only visible to their friends, so sign in \
                     with an account that is friends with the person you watch.",
                );
                if ui.button("Log in with Chrome").clicked() {
                    let _ = self.to_worker.send(Cmd::OpenLogin);
                    self.status =
                        "Opening Chrome — sign in to Roblox, then this reads your session \
                         automatically. You can close that window afterwards."
                            .into();
                }
                ui.collapsing("or paste a .ROBLOSECURITY cookie", |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut self.cookie_input)
                            .desired_rows(2)
                            .desired_width(f32::INFINITY)
                            .password(true)
                            .hint_text("_|WARNING:-DO-NOT-SHARE-THIS…"),
                    );
                    if ui.button("Use this cookie").clicked() {
                        let c = self.cookie_input.trim().to_string();
                        self.cookie_input.clear();
                        let _ = self.to_worker.send(Cmd::SetCookie(c));
                    }
                });
            }

            ui.separator();
            ui.label(format!(
                "Roblox usernames to watch (one per line, up to {MAX_USERNAMES}):"
            ));
            let resp = ui.add(
                egui::TextEdit::multiline(&mut self.usernames_text)
                    .desired_rows(4)
                    .desired_width(f32::INFINITY)
                    .hint_text("e.g. her_roblox_username"),
            );
            if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                self.save_usernames();
            }
            ui.horizontal(|ui| {
                if ui.button("Save & check").clicked() {
                    self.save_usernames();
                }
                if ui.button("Check now").clicked() {
                    let _ = self.to_worker.send(Cmd::Refresh);
                }
            });

            ui.separator();
            ui.horizontal(|ui| {
                if ui
                    .checkbox(&mut self.autostart_on, "Start at login")
                    .changed()
                {
                    let _ = autostart::set_enabled(self.autostart_on);
                }
            });
            ui.horizontal(|ui| {
                if ui
                    .checkbox(&mut self.cfg.notify, "Pop up when someone comes online / goes offline")
                    .changed()
                {
                    let _ = self.cfg.save();
                }
                if ui.small_button("Preview").clicked() {
                    let (date, time) = history::now();
                    self.popup.push(format!("{date} {time}  user NAME is online"));
                }
            });
            ui.horizontal(|ui| {
                ui.label("Check every");
                let resp = ui
                    .add(egui::Slider::new(&mut self.cfg.interval_secs, 15..=600).suffix(" s"));
                // Save when a drag ends, or on a click/keyboard change (not every drag frame).
                if resp.drag_stopped() || (resp.changed() && !resp.dragged()) {
                    let _ = self.cfg.save();
                }
            });

            ui.separator();
            ui.label(egui::RichText::new("Status").strong());
            for e in &self.entries {
                let (col, txt) = presence_color_label(&e.presence);
                ui.horizontal(|ui| {
                    ui.colored_label(col, "●");
                    ui.label(format!("{} ({})", e.display, e.username));
                    ui.label("—");
                    ui.label(txt);
                });
            }
            if self.entries.is_empty() {
                ui.weak("No users yet. Add a username above.");
            }

            ui.separator();
            ui.horizontal(|ui| {
                if ui
                    .checkbox(&mut self.cfg.log_history, "Keep a history of online / offline times")
                    .changed()
                {
                    let _ = self.cfg.save();
                }
                if self.cfg.log_history && ui.small_button("Open log file").clicked() {
                    history::open_in_editor();
                }
            });
            if self.cfg.log_history {
                egui::ScrollArea::vertical()
                    .id_salt("history")
                    .max_height(160.0)
                    .show(ui, |ui| {
                        // Newest first.
                        for line in self.history.iter().rev() {
                            ui.label(line);
                        }
                        if self.history.is_empty() {
                            ui.weak("Online / offline times will be listed here.");
                        }
                    });
            }

            ui.add_space(6.0);
            ui.weak(&self.status);
            ui.add_space(6.0);
            ui.weak("Closing this window keeps the app running in the tray.");

            if self.installed {
                ui.add_space(6.0);
                if !self.confirm_uninstall {
                    if ui.small_button("Uninstall…").clicked() {
                        self.confirm_uninstall = true;
                    }
                } else {
                    ui.separator();
                    ui.label("Remove Roblox Online Tray from this computer?");
                    ui.checkbox(
                        &mut self.uninstall_data,
                        "Also delete settings, history and saved Roblox sign-in",
                    );
                    ui.horizontal(|ui| {
                        if ui.button("Uninstall").clicked() {
                            self.tray.remove();
                            install::uninstall(self.uninstall_data);
                            std::process::exit(0);
                        }
                        if ui.button("Cancel").clicked() {
                            self.confirm_uninstall = false;
                        }
                    });
                }
            }
        });
        });
    }
}

fn presence_label(p: &Presence) -> &'static str {
    match p {
        Presence::Offline => "offline",
        Presence::Online => "online (website/app)",
        Presence::InGame { .. } => "in a game",
        Presence::InStudio => "in Studio",
        Presence::Unknown => "unknown",
    }
}

fn presence_color_label(p: &Presence) -> (egui::Color32, String) {
    match p {
        Presence::Offline => (egui::Color32::GRAY, "offline".into()),
        Presence::Online => (
            egui::Color32::from_rgb(46, 204, 113),
            "online".into(),
        ),
        Presence::InGame { place } => (
            egui::Color32::from_rgb(52, 152, 219),
            format!("playing {place}"),
        ),
        Presence::InStudio => (egui::Color32::from_rgb(155, 89, 182), "in Studio".into()),
        Presence::Unknown => (egui::Color32::DARK_GRAY, "unknown".into()),
    }
}

fn spawn_worker(ctx: egui::Context, rx: Receiver<Cmd>, tx: Sender<Evt>, cfg: Config) {
    std::thread::spawn(move || {
        let mut usernames = cfg.usernames.clone();
        let mut interval = Duration::from_secs(cfg.interval_secs.max(15));
        let mut ids: HashMap<String, (u64, String)> = HashMap::new();
        let mut prev: HashMap<u64, Presence> = HashMap::new();
        let mut cookie: Option<String> = session::load();

        let send = |e: Evt| {
            let _ = tx.send(e);
            ctx.request_repaint();
        };

        // One-time silent capture from the dedicated profile, in case the user already
        // logged in there (no visible window — a brief headless read).
        if cookie.is_none() {
            if let Ok(Some(c)) = browser::capture_cookie() {
                if session::is_plausible(&c) {
                    let _ = session::save(&c);
                    cookie = Some(c);
                }
            }
        }

        loop {
            match rx.recv_timeout(interval) {
                Ok(Cmd::SetUsernames(list)) => {
                    usernames = list;
                    ids.retain(|k, _| usernames.contains(k));
                    continue;
                }
                Ok(Cmd::SetCookie(c)) => {
                    if c.is_empty() {
                        session::clear();
                        cookie = None;
                        send(Evt::LoggedIn(false));
                        send(Evt::Status("Signed out.".into()));
                        continue;
                    }
                    if !session::is_plausible(&c) {
                        send(Evt::Status("That doesn't look like a .ROBLOSECURITY cookie.".into()));
                        continue;
                    }
                    if Session::new(c.clone()).authenticated().unwrap_or(false) {
                        let _ = session::save(&c);
                        cookie = Some(c);
                        send(Evt::LoggedIn(true));
                        send(Evt::Status("Signed in.".into()));
                        // fall through to a check
                    } else {
                        send(Evt::Status("That cookie isn't a valid Roblox session.".into()));
                        continue;
                    }
                }
                Ok(Cmd::OpenLogin) => {
                    if let Err(e) = browser::open_login() {
                        send(Evt::Status(format!("Could not open Chrome: {e}")));
                        continue;
                    }
                    send(Evt::Status("Waiting for you to sign in to Roblox…".into()));
                    cookie = wait_for_login();
                    if cookie.is_none() {
                        send(Evt::Status(
                            "Didn't detect a sign-in. Log in and try again, or paste the cookie."
                                .into(),
                        ));
                        continue;
                    }
                    send(Evt::LoggedIn(true));
                    send(Evt::Status("Signed in — you can close the Chrome window.".into()));
                }
                Ok(Cmd::Refresh) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            }

            // Pick up interval/notify changes saved to disk.
            let latest = Config::load();
            interval = Duration::from_secs(latest.interval_secs.max(15));
            let notify = latest.notify;
            let log_history = latest.log_history;

            let Some(cur) = cookie.clone() else {
                send(Evt::LoggedIn(false));
                send(Evt::Entries(Vec::new()));
                send(Evt::Status("Not signed in — click “Log in with Chrome”.".into()));
                continue;
            };

            let mut sess = Session::new(cur);
            match sess.authenticated() {
                Ok(true) => send(Evt::LoggedIn(true)),
                Ok(false) => {
                    session::clear();
                    cookie = None;
                    send(Evt::LoggedIn(false));
                    send(Evt::Status("Session expired — please sign in again.".into()));
                    continue;
                }
                Err(e) => {
                    send(Evt::Status(format!("Network error: {e}")));
                    continue;
                }
            }

            if usernames.is_empty() {
                send(Evt::Entries(Vec::new()));
                persist_rotated(&sess, &mut cookie);
                continue;
            }

            // Resolve any usernames we do not have an id for yet. Problems are kept and
            // appended to the final status, so "Updated …" doesn't hide them.
            let mut problems = Vec::new();
            for name in &usernames {
                if ids.contains_key(name) {
                    continue;
                }
                match sess.resolve_username(name) {
                    Ok(Some((id, real, display))) => {
                        ids.insert(name.clone(), (id, display.clone()));
                        ids.entry(real).or_insert((id, display));
                    }
                    Ok(None) => problems.push(format!("user not found: {name}")),
                    Err(e) => problems.push(format!("lookup failed for {name}: {e}")),
                }
            }

            let wanted: Vec<u64> = usernames
                .iter()
                .filter_map(|n| ids.get(n).map(|(id, _)| *id))
                .collect();

            match sess.presence(&wanted) {
                Ok(list) => {
                    let by_id: HashMap<u64, Presence> = list.into_iter().collect();
                    let mut entries = Vec::new();
                    for name in &usernames {
                        if let Some((id, display)) = ids.get(name) {
                            let p = by_id.get(id).cloned().unwrap_or(Presence::Unknown);
                            let was_on = matches!(
                                prev.get(id),
                                Some(Presence::Online | Presence::InGame { .. })
                            );
                            let now_on = matches!(p, Presence::Online | Presence::InGame { .. });
                            // Only a definite Offline counts; Unknown may be a glitch.
                            let change = if now_on && !was_on {
                                Some(online_text(display, &p))
                            } else if was_on && p == Presence::Offline {
                                Some(format!("{display} is offline"))
                            } else {
                                None
                            };
                            if let Some(text) = change {
                                let (date, time) = history::now();
                                let line = format!("{date} {time}  {text}");
                                if log_history {
                                    history::append(&line);
                                    send(Evt::History(line.clone()));
                                }
                                if notify {
                                    send(Evt::Popup(line));
                                }
                            }
                            prev.insert(*id, p.clone());
                            entries.push(Entry {
                                username: name.clone(),
                                display: display.clone(),
                                presence: p,
                            });
                        }
                    }
                    send(Evt::Entries(entries));
                    let mut status = format!("Updated {}", history::now().1);
                    if !problems.is_empty() {
                        status = format!("{status} — {}", problems.join("; "));
                    }
                    send(Evt::Status(status));
                }
                Err(e) => send(Evt::Status(format!("Check failed: {e}"))),
            }

            persist_rotated(&sess, &mut cookie);
        }
    });
}

/// After signing in, poll the dedicated Chrome for the cookie for a few minutes.
fn wait_for_login() -> Option<String> {
    let deadline = Instant::now() + Duration::from_secs(240);
    while Instant::now() < deadline {
        if let Ok(Some(c)) = browser::capture_cookie() {
            if session::is_plausible(&c) && Session::new(c.clone()).authenticated().unwrap_or(false)
            {
                let _ = session::save(&c);
                return Some(c);
            }
        }
        std::thread::sleep(Duration::from_secs(3));
    }
    None
}

/// Save the cookie if Roblox rotated it during the calls.
fn persist_rotated(sess: &Session, cookie: &mut Option<String>) {
    if cookie.as_deref() != Some(sess.cookie.as_str()) {
        let _ = session::save(&sess.cookie);
        *cookie = Some(sess.cookie.clone());
    }
}

fn online_text(display: &str, p: &Presence) -> String {
    match p {
        Presence::InGame { place } => format!("{display} is playing {place}"),
        _ => format!("{display} is online"),
    }
}
