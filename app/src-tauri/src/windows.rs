//! Window management: one window per pet (created / closed as petd's layout
//! changes), one overlay per monitor, remembered positions, click-through
//! outside clickable things, the hover card, the gallery, the first-run
//! chooser and the report card. Also the Win32 probes the physics and the
//! office guards need (cursor, title bars, fullscreen apps).

use crate::physics::{Rect as Surface, FOOT_H};
use crate::App;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, WebviewUrl, WebviewWindow, WebviewWindowBuilder, Window};

const PET_W: f64 = 200.0;
const PET_H: f64 = 168.0;
const MARGIN: f64 = 24.0;
// Room for minis: a camp pet grows to ~450 px wide with 4 minis + "+N".
const SPACING: f64 = 100.0;

/// Autoplay so the attention chime plays without a click first.
const BASE_ARGS: &str =
    "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --autoplay-policy=no-user-gesture-required";

/// WebView2 flags; debug builds open DevTools Protocol on `FAMILIAR_CDP_PORT`
/// so tests can drive the pages.
pub fn browser_args() -> String {
    match std::env::var("FAMILIAR_CDP_PORT") {
        Ok(port) if cfg!(debug_assertions) && !port.is_empty() => format!("{BASE_ARGS} --remote-debugging-port={port}"),
        _ => BASE_ARGS.to_string(),
    }
}

// ---------------------------------------------------------------- positions

/// Where each pet stands, as the physical screen point under its feet
/// (bottom-centre of the window). Saved to ~/.familiar/windows.json.
pub struct Positions {
    file: PathBuf,
    map: Mutex<HashMap<String, (i32, i32)>>,
    dirty: AtomicBool,
}

impl Positions {
    pub fn load(file: PathBuf) -> Self {
        let map = std::fs::read_to_string(&file)
            .ok()
            .and_then(|t| serde_json::from_str::<HashMap<String, (i32, i32)>>(&t).ok())
            .unwrap_or_default();
        Positions { file, map: Mutex::new(map), dirty: AtomicBool::new(false) }
    }

    pub fn remember(&self, w: &Window) {
        let (Ok(pos), Ok(size)) = (w.outer_position(), w.outer_size()) else { return };
        let id = w.label().trim_start_matches("pet-").to_string();
        let anchor = (pos.x + size.width as i32 / 2, pos.y + size.height as i32);
        let mut map = self.map.lock().unwrap();
        if map.get(&id) != Some(&anchor) {
            map.insert(id, anchor);
            self.dirty.store(true, Ordering::SeqCst);
        }
    }

    fn get(&self, id: &str) -> Option<(i32, i32)> {
        self.map.lock().unwrap().get(id).copied()
    }

    pub fn save_now(&self) {
        if !self.dirty.swap(false, Ordering::SeqCst) {
            return;
        }
        let text = serde_json::to_string_pretty(&*self.map.lock().unwrap()).unwrap_or_default();
        let tmp = self.file.with_extension("json.tmp");
        if std::fs::write(&tmp, text).is_ok() {
            let _ = std::fs::rename(&tmp, &self.file);
        }
    }
}

/// Writes positions at most every 2 s, so dragging does not hammer the disk.
pub fn spawn_position_saver(app: Arc<App>) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(2));
        app.positions.save_now();
    });
}

// ---------------------------------------------------------------- visibility

/// Pets show unless the user hid them, the boss key is on, or a fullscreen
/// app is in front.
pub fn visible(app: &App) -> bool {
    !app.hidden.load(Ordering::SeqCst) && !app.boss.load(Ordering::SeqCst) && !app.fullscreen.load(Ordering::SeqCst)
}

pub fn apply_visibility(h: &AppHandle, app: &App) {
    let show = visible(app);
    for (label, w) in h.webview_windows() {
        if label.starts_with("pet-") {
            let _ = if show { w.show() } else { w.hide() };
        } else if (label == "card" || label.starts_with("overlay-")) && !show {
            let _ = w.hide();
        }
    }
    // Overlays decide for themselves when they have something to draw.
    for (label, _) in h.webview_windows() {
        if label.starts_with("overlay-") {
            let _ = h.emit_to(label.as_str(), "overlay:visible", show);
        }
    }
}

// ---------------------------------------------------------------- pet windows

fn create_pet_window(h: &AppHandle, app: &App, id: &str, index: usize) -> tauri::Result<()> {
    let label = format!("pet-{id}");
    if h.get_webview_window(&label).is_some() {
        return Ok(());
    }
    let win = WebviewWindowBuilder::new(h, &label, WebviewUrl::App(format!("index.html?pet={id}").into()))
        .title("Familiar")
        .transparent(true)
        .decorations(false)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .focused(false)
        // Never take focus from the user's terminal or IDE, even when clicked.
        .focusable(false)
        .visible(false)
        .inner_size(PET_W, PET_H)
        .additional_browser_args(&browser_args())
        .build()?;

    let monitors = win.available_monitors().unwrap_or_default();
    let saved = app.positions.get(id).and_then(|(ax, ay)| {
        // Only if that point is still on a connected monitor.
        monitors.iter().find_map(|m| {
            let (p, s) = (m.position(), m.size());
            let inside = ax >= p.x && ax < p.x + s.width as i32 && ay > p.y && ay <= p.y + s.height as i32;
            inside.then(|| {
                let sc = m.scale_factor();
                // Never below the work area: the taskbar would hide the camp tag.
                let wa = m.work_area();
                let ay = ay.min(wa.position.y + wa.size.height as i32);
                PhysicalPosition::new(ax - (PET_W * sc / 2.0) as i32, ay - (PET_H * sc) as i32)
            })
        })
    });
    match saved {
        Some(p) => {
            let _ = win.set_position(p);
        }
        None => {
            // New pets line up leftwards from the bottom-right of the primary
            // work area, feet on the taskbar.
            if let Ok(Some(m)) = win.primary_monitor() {
                let sc = m.scale_factor();
                let area = m.work_area();
                let right = (area.position.x as f64 + area.size.width as f64) / sc;
                let bottom = (area.position.y as f64 + area.size.height as f64) / sc;
                let x = right - MARGIN - PET_W - index as f64 * (PET_W + SPACING);
                let _ = win.set_position(tauri::LogicalPosition::new(x.max(0.0), bottom - PET_H + FOOT_H));
            }
        }
    }
    if visible(app) {
        win.show()?;
    }
    Ok(())
}

/// Make the pet windows match petd's pet list.
pub fn sync_pets(h: &AppHandle, app: &Arc<App>, ids: &[String]) {
    for (label, w) in h.webview_windows() {
        if let Some(id) = label.strip_prefix("pet-") {
            if !ids.iter().any(|i| i == id) {
                let _ = w.destroy();
                app.hits.lock().unwrap().remove(&label);
            }
        }
    }
    for (i, id) in ids.iter().enumerate() {
        if let Err(e) = create_pet_window(h, app, id, i) {
            app.petd.paths.log(&format!("pet window {id}: {e}"));
        }
    }
}

// ---------------------------------------------------------------- other windows

pub fn create_card(h: &AppHandle) -> tauri::Result<()> {
    let w = WebviewWindowBuilder::new(h, "card", WebviewUrl::App("card.html".into()))
        .title("Familiar")
        .transparent(true)
        .decorations(false)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .focused(false)
        .focusable(false)
        .visible(false)
        .inner_size(288.0, 120.0)
        .additional_browser_args(&browser_args())
        .build()?;
    // Never in the way of a click.
    let _ = w.set_ignore_cursor_events(true);
    Ok(())
}

/// One transparent, click-through overlay per monitor (bugs, rope, race lane,
/// coin pop-ups). Each page is told its monitor's work area in logical px.
pub fn create_overlays(h: &AppHandle) {
    let Some(probe) = h.get_webview_window("card") else { return };
    let monitors = probe.available_monitors().unwrap_or_default();
    let primary = probe.primary_monitor().ok().flatten().map(|m| *m.position());
    for (i, m) in monitors.iter().enumerate() {
        let sc = m.scale_factor();
        let a = m.work_area();
        let (x, y) = (a.position.x as f64 / sc, a.position.y as f64 / sc);
        let (w, hgt) = (a.size.width as f64 / sc, a.size.height as f64 / sc);
        let is_primary = primary == Some(*m.position());
        let url = format!("overlay.html?m={i}&x={x}&y={y}&w={w}&h={hgt}&primary={}", is_primary as u8);
        let res = WebviewWindowBuilder::new(h, format!("overlay-{i}"), WebviewUrl::App(url.into()))
            .title("Familiar overlay")
            .transparent(true)
            .decorations(false)
            .shadow(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .resizable(false)
            .focused(false)
            .focusable(false)
            .visible(false)
            .position(x, y)
            .inner_size(w, hgt)
            .additional_browser_args(&browser_args())
            .build();
        match res {
            Ok(win) => {
                let _ = win.set_ignore_cursor_events(true);
            }
            Err(e) => eprintln!("overlay {i}: {e}"),
        }
    }
}

fn open_page(h: &AppHandle, label: &str, page: &str, title: &str, w: f64, hgt: f64) {
    if let Some(win) = h.get_webview_window(label) {
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_focus();
        return;
    }
    let res = WebviewWindowBuilder::new(h, label, WebviewUrl::App(page.into()))
        .title(title)
        .inner_size(w, hgt)
        .min_inner_size(420.0, 420.0)
        .center()
        .additional_browser_args(&browser_args())
        .build();
    if let Err(e) = res {
        eprintln!("{label}: {e}");
    }
}

pub fn open_gallery(h: &AppHandle) {
    open_page(h, "gallery", "gallery.html", petd::i18n::t("宠物图鉴 · Familiar", "Gallery · Familiar"), 880.0, 680.0);
}

/// The gallery on its shop tab (focuses the gallery if it is already open).
pub fn open_shop(h: &AppHandle) {
    open_page(h, "gallery", "gallery.html?tab=shop", petd::i18n::t("宠物图鉴 · Familiar", "Gallery · Familiar"), 880.0, 680.0);
}

/// The gallery on its reminders tab.
pub fn open_reminders(h: &AppHandle) {
    open_page(h, "gallery", "gallery.html?tab=health", petd::i18n::t("宠物图鉴 · Familiar", "Gallery · Familiar"), 880.0, 680.0);
}

pub fn open_welcome(h: &AppHandle) {
    open_page(h, "welcome", "welcome.html", petd::i18n::t("欢迎使用 Familiar", "Welcome to Familiar"), 760.0, 560.0);
}

pub fn open_spring(h: &AppHandle) {
    open_page(h, "spring", "spring.html", petd::i18n::t("春节倒计时 · Familiar", "Spring Festival · Familiar"), 760.0, 620.0);
}

pub fn open_report(h: &AppHandle) {
    open_page(h, "report", "report.html", petd::i18n::t("今日报告 · Familiar", "Today's report · Familiar"), 460.0, 640.0);
}

// ---------------------------------------------------------------- click-through

/// Clicks on transparent parts of pet and overlay windows fall through to
/// whatever is behind them. Windows has no per-pixel hit testing for these
/// windows, so poll the cursor and switch `ignore_cursor_events` as it enters
/// / leaves the rectangles each page reported.
pub fn spawn_click_through(h: AppHandle, app: Arc<App>) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(40));
        // No cursor (locked screen, secure desktop): make everything click-through.
        let (cx, cy) = cursor().unwrap_or((f64::NAN, f64::NAN));
        // Mid-click or mid-drag nothing turns click-through, but a window may
        // still start taking input (a file dragged over a pet).
        let held = any_down();
        for (label, w) in h.webview_windows() {
            if !label.starts_with("pet-") && !label.starts_with("overlay-") {
                continue;
            }
            let (Ok(pos), Ok(size), Ok(sc)) = (w.outer_position(), w.inner_size(), w.scale_factor()) else { continue };
            let lx = (cx - pos.x as f64) / sc;
            let ly = (cy - pos.y as f64) / sc;
            let inside = lx >= 0.0 && ly >= 0.0 && lx < size.width as f64 / sc && ly < size.height as f64 / sc;
            let mut hits = app.hits.lock().unwrap();
            let entry = hits.entry(label.clone()).or_insert((vec![], label.starts_with("overlay-")));
            let hit = inside
                && entry.0.iter().any(|r| lx >= r.x - 2.0 && lx < r.x + r.w + 2.0 && ly >= r.y - 2.0 && ly < r.y + r.h + 2.0);
            let ignore = !hit;
            if held && ignore {
                continue;
            }
            if entry.1 != ignore {
                entry.1 = ignore;
                drop(hits);
                let _ = w.set_ignore_cursor_events(ignore);
                if ignore {
                    let _ = h.emit_to(label.as_str(), "cursor-left", ());
                }
            }
        }
    });
}

// ---------------------------------------------------------------- Win32 probes

#[cfg(windows)]
pub fn cursor() -> Option<(f64, f64)> {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;
    let mut p = POINT { x: 0, y: 0 };
    (unsafe { GetCursorPos(&mut p) } != 0).then_some((p.x as f64, p.y as f64))
}

#[cfg(windows)]
fn key_down(vk: u16) -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
    unsafe { (GetAsyncKeyState(vk as i32) as u16 & 0x8000) != 0 }
}

#[cfg(windows)]
pub fn left_down() -> bool {
    key_down(windows_sys::Win32::UI::Input::KeyboardAndMouse::VK_LBUTTON)
}

#[cfg(windows)]
fn any_down() -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{VK_LBUTTON, VK_RBUTTON};
    key_down(VK_LBUTTON) || key_down(VK_RBUTTON)
}

#[cfg(windows)]
mod win32 {
    use crate::physics::Rect;
    use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM, POINT, RECT};
    use windows_sys::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
    use windows_sys::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST};
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    pub fn class_of(h: HWND) -> String {
        let mut buf = [0u16; 64];
        let n = unsafe { GetClassNameW(h, buf.as_mut_ptr(), buf.len() as i32) };
        String::from_utf16_lossy(&buf[..n.max(0) as usize])
    }

    fn pid_of(h: HWND) -> u32 {
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(h, &mut pid) };
        pid
    }

    pub fn is_shell(h: HWND) -> bool {
        matches!(class_of(h).as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd")
    }

    fn cloaked(h: HWND) -> bool {
        let mut v: u32 = 0;
        let hr = unsafe { DwmGetWindowAttribute(h, DWMWA_CLOAKED as u32, &mut v as *mut u32 as *mut _, 4) };
        hr == 0 && v != 0
    }

    fn rect(h: HWND) -> Option<RECT> {
        let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        (unsafe { GetWindowRect(h, &mut r) } != 0).then_some(r)
    }

    /// The window under a screen point (transparent / click-through ones are skipped by Windows).
    fn top_at(x: i32, y: i32) -> HWND {
        unsafe { GetAncestor(WindowFromPoint(POINT { x, y }), GA_ROOT) }
    }

    unsafe extern "system" fn collect(h: HWND, l: LPARAM) -> BOOL {
        let v = &mut *(l as *mut Vec<HWND>);
        v.push(h);
        1
    }

    /// Top edges of ordinary visible windows that a pet could stand on.
    pub fn window_tops() -> Vec<Rect> {
        let me = std::process::id();
        let mut all: Vec<HWND> = Vec::new();
        unsafe { EnumWindows(Some(collect), &mut all as *mut Vec<HWND> as LPARAM) };
        let mut out = Vec::new();
        for h in all {
            unsafe {
                if IsWindowVisible(h) == 0 || IsIconic(h) != 0 || IsZoomed(h) != 0 || GetWindowTextLengthW(h) == 0 {
                    continue;
                }
                let ex = GetWindowLongW(h, GWL_EXSTYLE) as u32;
                if ex & WS_EX_TOOLWINDOW != 0 {
                    continue;
                }
            }
            if pid_of(h) == me || is_shell(h) || cloaked(h) {
                continue;
            }
            let Some(r) = rect(h) else { continue };
            if r.right - r.left < 160 || r.bottom - r.top < 80 {
                continue;
            }
            // Only if its title bar is actually visible somewhere along it.
            let y = r.top + 4;
            let w = r.right - r.left;
            let seen = [w / 6, w / 2, w * 5 / 6].iter().any(|dx| top_at(r.left + dx, y) == h);
            if seen {
                out.push(Rect { l: r.left as f64 + 8.0, t: r.top as f64, r: r.right as f64 - 8.0, b: r.bottom as f64 });
            }
        }
        out
    }

    /// Parent pid of every process.
    fn parents() -> std::collections::HashMap<u32, u32> {
        use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
        use windows_sys::Win32::System::Diagnostics::ToolHelp::*;
        let mut out = std::collections::HashMap::new();
        unsafe {
            let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snap == INVALID_HANDLE_VALUE {
                return out;
            }
            let mut e: PROCESSENTRY32W = std::mem::zeroed();
            e.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
            if Process32FirstW(snap, &mut e) != 0 {
                loop {
                    out.insert(e.th32ProcessID, e.th32ParentProcessID);
                    if Process32NextW(snap, &mut e) == 0 {
                        break;
                    }
                }
            }
            CloseHandle(snap);
        }
        out
    }

    /// The main visible window of a process, if it has one.
    fn main_window_of(pid: u32) -> Option<HWND> {
        let mut all: Vec<HWND> = Vec::new();
        unsafe { EnumWindows(Some(collect), &mut all as *mut Vec<HWND> as LPARAM) };
        all.into_iter().find(|&h| unsafe {
            pid_of(h) == pid
                && IsWindowVisible(h) != 0
                && GetWindowTextLengthW(h) > 0
                && GetWindow(h, GW_OWNER).is_null()
                && (GetWindowLongW(h, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW) == 0
                && !cloaked(h)
        })
    }

    /// Bring the terminal / IDE that runs `pid` to the front: walk up the
    /// process tree to the first ancestor that owns a visible window.
    pub fn focus_process_window(pid: u32) -> bool {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::{SendInput, INPUT, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VK_MENU};
        let parents = parents();
        let mut cur = pid;
        for _ in 0..12 {
            if cur == 0 || cur == std::process::id() {
                break;
            }
            if let Some(h) = main_window_of(cur) {
                unsafe {
                    if IsIconic(h) != 0 {
                        ShowWindow(h, SW_RESTORE);
                    }
                    // A tap of Alt lifts Windows' foreground lock for us.
                    let key = |flags| INPUT {
                        r#type: INPUT_KEYBOARD,
                        Anonymous: windows_sys::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
                            ki: KEYBDINPUT { wVk: VK_MENU, wScan: 0, dwFlags: flags, time: 0, dwExtraInfo: 0 },
                        },
                    };
                    let inputs = [key(0), key(KEYEVENTF_KEYUP)];
                    SendInput(2, inputs.as_ptr(), std::mem::size_of::<INPUT>() as i32);
                    SetForegroundWindow(h);
                }
                return true;
            }
            match parents.get(&cur) {
                Some(&p) if p != cur => cur = p,
                _ => break,
            }
        }
        false
    }

    /// Is a meeting app sharing the screen? Matches visible windows whose
    /// title or class contains one of `patterns` (their share toolbars).
    pub fn sharing_screen(patterns: &[String]) -> bool {
        if patterns.is_empty() {
            return false;
        }
        let mut all: Vec<HWND> = Vec::new();
        unsafe { EnumWindows(Some(collect), &mut all as *mut Vec<HWND> as LPARAM) };
        all.into_iter().any(|h| unsafe {
            if IsWindowVisible(h) == 0 {
                return false;
            }
            let mut buf = [0u16; 256];
            let n = GetWindowTextW(h, buf.as_mut_ptr(), buf.len() as i32);
            let title = String::from_utf16_lossy(&buf[..n.max(0) as usize]).to_lowercase();
            let class = class_of(h).to_lowercase();
            patterns.iter().any(|p| {
                let p = p.to_lowercase();
                !p.is_empty() && (title.contains(&p) || class.contains(&p))
            })
        })
    }

    /// Is a fullscreen app (video, game, slideshow) in front?
    pub fn fullscreen_in_front() -> bool {
        unsafe {
            let h = GetForegroundWindow();
            if h.is_null() || is_shell(h) || pid_of(h) == std::process::id() {
                return false;
            }
            let Some(r) = rect(h) else { return false };
            let m = MonitorFromWindow(h, MONITOR_DEFAULTTONEAREST);
            let mut mi: MONITORINFO = std::mem::zeroed();
            mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
            if GetMonitorInfoW(m, &mut mi) == 0 {
                return false;
            }
            let mr = mi.rcMonitor;
            r.left <= mr.left && r.top <= mr.top && r.right >= mr.right && r.bottom >= mr.bottom
        }
    }
}

#[cfg(windows)]
pub fn window_tops(_w: &WebviewWindow) -> Vec<Surface> {
    win32::window_tops()
}

#[cfg(windows)]
pub fn fullscreen_in_front() -> bool {
    win32::fullscreen_in_front()
}

#[cfg(windows)]
pub fn sharing_screen(patterns: &[String]) -> bool {
    win32::sharing_screen(patterns)
}

#[cfg(not(windows))]
pub fn sharing_screen(_patterns: &[String]) -> bool {
    false
}

#[cfg(windows)]
pub fn focus_process_window(pid: u32) -> bool {
    win32::focus_process_window(pid)
}

#[cfg(not(windows))]
pub fn focus_process_window(_pid: u32) -> bool {
    false
}

#[cfg(not(windows))]
pub fn cursor() -> Option<(f64, f64)> {
    None
}
#[cfg(not(windows))]
pub fn left_down() -> bool {
    false
}
#[cfg(not(windows))]
fn any_down() -> bool {
    false
}
#[cfg(not(windows))]
pub fn window_tops(_w: &WebviewWindow) -> Vec<Surface> {
    vec![]
}
#[cfg(not(windows))]
pub fn fullscreen_in_front() -> bool {
    false
}
