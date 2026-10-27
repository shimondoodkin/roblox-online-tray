// Hide the console window in release builds; keep it in debug for logs.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod autostart;
mod browser;
mod config;
mod history;
mod http;
mod model;
mod popup;
mod session;
mod tray;

use eframe::egui;

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().collect();

    // `--dump-icons <dir>`: write raw RGBA of each state for preview, then exit.
    if let Some(p) = args.iter().position(|a| a == "--dump-icons") {
        let dir = args.get(p + 1).cloned().unwrap_or_else(|| ".".into());
        for (name, dot) in [
            ("online", tray::Dot::Online),
            ("idle", tray::Dot::Idle),
            ("unknown", tray::Dot::Unknown),
        ] {
            let rgba = tray::icon_rgba(dot, 96);
            std::fs::write(format!("{dir}/{name}.rgba"), rgba).unwrap();
        }
        return Ok(());
    }

    // `--tray` (used by the auto-start entry) launches straight into the tray.
    let start_hidden = args.iter().any(|a| a == "--tray");
    // `--preview-popup` shows a sample online/offline popup at startup.
    let preview_popup = args.iter().any(|a| a == "--preview-popup");

    // Window/taskbar icon: the same tile as the tray, in the "blue" idle state.
    let icon = {
        let size = 32u32;
        let rgba = tray::icon_rgba(tray::Dot::Unknown, size);
        egui::IconData {
            rgba,
            width: size,
            height: size,
        }
    };

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Roblox Online Tray")
            .with_inner_size([420.0, 560.0])
            .with_min_inner_size([360.0, 420.0])
            .with_icon(icon)
            .with_visible(!start_hidden),
        ..Default::default()
    };

    eframe::run_native(
        "Roblox Online Tray",
        options,
        Box::new(move |cc| {
            app::App::new(cc, start_hidden, preview_popup)
                .map(|a| Box::new(a) as Box<dyn eframe::App>)
                .map_err(|e| e.into())
        }),
    )
}
