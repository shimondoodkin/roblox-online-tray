//! System-tray icon and menu. On Windows and macOS it is created on the main thread
//! inside the eframe app, so winit's event loop pumps its messages; on Linux it needs
//! its own GTK thread. Clicks arrive on the global `MenuEvent` / `TrayIconEvent`
//! channels, which the app polls each frame.

use tray_icon::menu::{Menu, MenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

/// Colour of the tray dot for the overall watch state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dot {
    /// Nobody watched is online.
    Idle,
    /// At least one watched user is online or in a game.
    Online,
    /// Not signed in / cannot tell.
    Unknown,
}

// Menu ids are fixed, so they are known even when the menu is built on another thread.
pub const ID_OPEN: &str = "open";
pub const ID_CHECK: &str = "check";
pub const ID_QUIT: &str = "quit";

/// A change to apply to the icon.
enum Update {
    Set(Dot, String),
    Remove,
}

pub struct Tray {
    #[cfg(not(target_os = "linux"))]
    tray: TrayIcon,
    /// On Linux the icon lives on a GTK thread; updates are sent there.
    #[cfg(target_os = "linux")]
    tx: std::sync::mpsc::Sender<Update>,
    last: Option<(Dot, String)>,
}

fn build() -> anyhow::Result<TrayIcon> {
    let menu = Menu::new();
    menu.append(&MenuItem::with_id(ID_OPEN, "Open window", true, None))?;
    menu.append(&MenuItem::with_id(ID_CHECK, "Check now", true, None))?;
    menu.append(&tray_icon::menu::PredefinedMenuItem::separator())?;
    menu.append(&MenuItem::with_id(ID_QUIT, "Quit", true, None))?;

    Ok(TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("Roblox Online Tray — starting…")
        .with_icon(icon_for(Dot::Unknown))
        .build()?)
}

fn apply(tray: &TrayIcon, update: Update) {
    match update {
        Update::Set(dot, tooltip) => {
            let _ = tray.set_icon(Some(icon_for(dot)));
            let _ = tray.set_tooltip(Some(tooltip));
        }
        Update::Remove => {
            let _ = tray.set_visible(false);
        }
    }
}

impl Tray {
    #[cfg(not(target_os = "linux"))]
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self {
            tray: build()?,
            last: None,
        })
    }

    #[cfg(target_os = "linux")]
    pub fn new() -> anyhow::Result<Self> {
        let (tx, rx) = std::sync::mpsc::channel::<Update>();
        std::thread::spawn(move || {
            if gtk::init().is_err() {
                return;
            }
            let Ok(tray) = build() else { return };
            gtk::glib::timeout_add_local(std::time::Duration::from_millis(250), move || {
                while let Ok(u) = rx.try_recv() {
                    apply(&tray, u);
                }
                gtk::glib::ControlFlow::Continue
            });
            gtk::main();
        });
        Ok(Self { tx, last: None })
    }

    fn send(&self, update: Update) {
        #[cfg(not(target_os = "linux"))]
        apply(&self.tray, update);
        #[cfg(target_os = "linux")]
        let _ = self.tx.send(update);
    }

    pub fn set(&mut self, dot: Dot, tooltip: &str) {
        if self.last.as_ref() == Some(&(dot, tooltip.to_string())) {
            return;
        }
        self.last = Some((dot, tooltip.to_string()));
        self.send(Update::Set(dot, tooltip.to_string()));
    }

    /// Take the icon out of the notification area (before a hard exit).
    pub fn remove(&self) {
        self.send(Update::Remove);
        // Give the Linux GTK thread a moment to act before the process exits.
        #[cfg(target_os = "linux")]
        std::thread::sleep(std::time::Duration::from_millis(300));
    }
}

fn icon_for(dot: Dot) -> Icon {
    const S: u32 = 32;
    let rgba = icon_rgba(dot, S);
    Icon::from_rgba(rgba, S, S).expect("valid icon")
}

/// Status colour as (r, g, b), plus a darker shade for the tile border.
fn colors(dot: Dot) -> ((f32, f32, f32), (f32, f32, f32)) {
    let c = match dot {
        Dot::Idle => (120.0, 124.0, 134.0),   // grey
        Dot::Online => (46.0, 196.0, 113.0),  // green
        Dot::Unknown => (72.0, 118.0, 214.0), // blue
    };
    let border = (c.0 * 0.6, c.1 * 0.6, c.2 * 0.6);
    (c, border)
}

/// A rounded-square "app tile" in the status colour with a white play triangle,
/// rendered with 3×3 supersampling for smooth edges. Returns `size*size` RGBA.
pub fn icon_rgba(dot: Dot, size: u32) -> Vec<u8> {
    let s = size as f32;
    let ((cr, cg, cb), (br, bg, bb)) = colors(dot);
    let margin = s * 0.08;
    let radius = s * 0.26; // corner radius
    let border = s * 0.055;

    // Right-pointing play triangle, roughly centred.
    let tx0 = s * 0.40;
    let tx1 = s * 0.68;
    let ty0 = s * 0.30;
    let ty1 = s * 0.70;
    let tyc = (ty0 + ty1) / 2.0;

    // Signed "inside rounded rect" test for point (px, py) with inset `inset`.
    let in_round = |px: f32, py: f32, inset: f32| -> bool {
        let lo = margin + inset;
        let hi = s - margin - inset;
        if px < lo || px > hi || py < lo || py > hi {
            return false;
        }
        let r = (radius - inset).max(0.0);
        // Nearest corner centre.
        let cx = px.clamp(lo + r, hi - r);
        let cy = py.clamp(lo + r, hi - r);
        let dx = px - cx;
        let dy = py - cy;
        dx * dx + dy * dy <= r * r
    };
    // Inside the play triangle?
    let in_tri = |px: f32, py: f32| -> bool {
        if px < tx0 || px > tx1 {
            return false;
        }
        let t = (px - tx0) / (tx1 - tx0); // 0 at base, 1 at tip
        let half = (ty1 - ty0) / 2.0 * (1.0 - t);
        (py - tyc).abs() <= half
    };

    let mut rgba = vec![0u8; (size * size * 4) as usize];
    for y in 0..size {
        for x in 0..size {
            let (mut r, mut g, mut b, mut a) = (0.0f32, 0.0, 0.0, 0.0);
            for sy in 0..3 {
                for sx in 0..3 {
                    let px = x as f32 + (sx as f32 + 0.5) / 3.0;
                    let py = y as f32 + (sy as f32 + 0.5) / 3.0;
                    if !in_round(px, py, 0.0) {
                        continue;
                    }
                    a += 1.0;
                    // Border ring vs fill vs white glyph.
                    let (pr, pg, pb) = if in_tri(px, py) {
                        (255.0, 255.0, 255.0)
                    } else if in_round(px, py, border) {
                        (cr, cg, cb)
                    } else {
                        (br, bg, bb)
                    };
                    r += pr;
                    g += pg;
                    b += pb;
                }
            }
            let i = ((y * size + x) * 4) as usize;
            if a > 0.0 {
                rgba[i] = (r / a) as u8;
                rgba[i + 1] = (g / a) as u8;
                rgba[i + 2] = (b / a) as u8;
            }
            rgba[i + 3] = (a / 9.0 * 255.0) as u8;
        }
    }
    rgba
}
