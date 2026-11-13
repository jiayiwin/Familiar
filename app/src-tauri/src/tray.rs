//! The tray panel: a small styled window (tray.html) that opens next to the
//! tray icon instead of the plain Windows menu. Picks go through `on_menu`
//! with the same ids the old menu used.

use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, WebviewUrl, WebviewWindowBuilder};

/// Logical size of the panel window; the card sits at its taskbar-side edge.
const W: f64 = 380.0;
const H: f64 = 900.0;

pub fn create(h: &AppHandle) -> tauri::Result<()> {
    WebviewWindowBuilder::new(h, "tray", WebviewUrl::App("tray.html".into()))
        .title("Rill")
        .transparent(true)
        .decorations(false)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .visible(false)
        .inner_size(W, H)
        .additional_browser_args(&crate::windows::browser_args())
        .build()?;
    Ok(())
}

/// Show the panel by the click at `at` (physical px), kept inside the work area.
pub fn open(h: &AppHandle, at: PhysicalPosition<f64>, state: serde_json::Value) {
    let Some(win) = h.get_webview_window("tray") else { return };
    let monitor = win
        .available_monitors()
        .unwrap_or_default()
        .into_iter()
        .find(|m| {
            let (p, s) = (m.position(), m.size());
            at.x >= p.x as f64 && at.x < (p.x + s.width as i32) as f64 && at.y >= p.y as f64 && at.y < (p.y + s.height as i32) as f64
        })
        .or_else(|| win.primary_monitor().ok().flatten());
    let Some(m) = monitor else { return };
    let sc = m.scale_factor();
    let area = m.work_area();
    let (ax, ay) = (area.position.x as f64, area.position.y as f64);
    let (aw, ah) = (area.size.width as f64, area.size.height as f64);
    // Short screens: the card scrolls inside a shorter window.
    let (w, hh) = ((W * sc).round(), (H * sc).round().min(ah - 16.0));
    // Taskbar at the top: hang below it; otherwise sit above the click.
    let top = at.y < ay + ah / 2.0;
    let x = (at.x - w / 2.0).clamp(ax + 8.0, ax + aw - w - 8.0);
    let y = if top { ay + 8.0 } else { ay + ah - hh - 8.0 };
    let _ = win.set_size(tauri::PhysicalSize::new(w as u32, hh as u32));
    let _ = win.set_position(PhysicalPosition::new(x as i32, y as i32));
    let mut st = state;
    st["anchor"] = serde_json::json!(if top { "top" } else { "bottom" });
    let _ = h.emit_to("tray", "tray-open", st);
    let _ = win.show();
    let _ = win.set_focus();
}

/// Fresh state for an open panel (a toggle changed, a pet's state moved on).
pub fn refresh(h: &AppHandle, state: serde_json::Value) {
    if let Some(win) = h.get_webview_window("tray") {
        if win.is_visible().unwrap_or(false) {
            let _ = h.emit_to("tray", "tray-state", state);
        }
    }
}

pub fn close(h: &AppHandle) {
    if let Some(w) = h.get_webview_window("tray") {
        let _ = w.hide();
    }
}
