//! The "user is online / offline" popup: a small borderless, always-on-top egui
//! viewport in the bottom-right corner, just above the taskbar. It stays until closed.

use eframe::egui;

const WIDTH: f32 = 340.0;
const MARGIN: f32 = 12.0;
/// Older messages are dropped so the popup never grows off-screen.
const MAX_LINES: usize = 6;

#[derive(Default)]
pub struct Popup {
    lines: Vec<String>,
}

impl Popup {
    pub fn push(&mut self, line: String) {
        self.lines.push(line);
        let extra = self.lines.len().saturating_sub(MAX_LINES);
        self.lines.drain(..extra);
    }

    /// Call every frame; shows the popup while there is something to say.
    pub fn show(&mut self, ctx: &egui::Context) {
        if self.lines.is_empty() {
            return;
        }
        let height = 44.0 + 22.0 * self.lines.len() as f32;
        let (right, bottom) = work_area_bottom_right(ctx);

        let builder = egui::ViewportBuilder::default()
            .with_title("Roblox Online Tray")
            .with_inner_size([WIDTH, height])
            .with_position([right - WIDTH - MARGIN, bottom - height - MARGIN])
            .with_decorations(false)
            .with_transparent(true)
            .with_resizable(false)
            .with_taskbar(false)
            .with_active(false)
            .with_always_on_top();

        let lines = &self.lines;
        let closed = ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of("online-popup"),
            builder,
            |ui, _class| {
                let style = ui.style().clone();
                egui::Frame::window(&style)
                    .shadow(egui::Shadow::NONE)
                    .show(ui, |ui| {
                        ui.set_min_size(ui.available_size());
                        let close = ui
                            .horizontal(|ui| {
                                ui.strong("Roblox Online Tray");
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| ui.button("✕").clicked(),
                                )
                                .inner
                            })
                            .inner;
                        ui.separator();
                        for line in lines {
                            ui.label(line);
                        }
                        close || ui.input(|i| i.viewport().close_requested())
                    })
                    .inner
            },
        );
        if closed {
            self.lines.clear();
        }
    }
}

/// Bottom-right corner of the primary monitor's work area (excludes the taskbar),
/// in egui points.
#[cfg(windows)]
fn work_area_bottom_right(_ctx: &egui::Context) -> (f32, f32) {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::HiDpi::GetDpiForSystem;
    use windows_sys::Win32::UI::WindowsAndMessaging::{SPI_GETWORKAREA, SystemParametersInfoW};

    let mut r = RECT { left: 0, top: 0, right: 1920, bottom: 1040 };
    let scale = unsafe {
        SystemParametersInfoW(SPI_GETWORKAREA, 0, (&mut r as *mut RECT).cast(), 0);
        GetDpiForSystem() as f32 / 96.0
    };
    let scale = if scale > 0.0 { scale } else { 1.0 };
    (r.right as f32 / scale, r.bottom as f32 / scale)
}

/// No portable work-area query: use the monitor size and leave room for a dock/panel.
#[cfg(not(windows))]
fn work_area_bottom_right(ctx: &egui::Context) -> (f32, f32) {
    let size = ctx
        .input(|i| i.viewport().monitor_size)
        .unwrap_or(egui::vec2(1920.0, 1080.0));
    (size.x, size.y - 70.0)
}
