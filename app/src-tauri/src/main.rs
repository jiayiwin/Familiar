// No console window in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod desktop;
mod physics;
mod radial;
mod selection;
mod setup;
mod tray;
mod windows;

use petd::{Control, Options, StartError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

static MODEL_DL_DONE: AtomicU64 = AtomicU64::new(0);
static MODEL_DL_TOTAL: AtomicU64 = AtomicU64::new(0);

// Letter pending delivery: generated before window is ready to receive events.
use std::time::{Duration, Instant};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

#[derive(Clone, Serialize)]
struct Info {
    port: u16,
    token: String,
}

/// Clickable rectangle inside a pet or overlay window (logical px).
#[derive(Clone, Copy, Debug, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// Shared app state.
pub struct App {
    pub petd: petd::Petd,
    /// Per window: clickable rects, and whether clicks currently pass through.
    pub hits: Mutex<HashMap<String, (Vec<Rect>, bool)>>,
    /// Hidden from the tray or /pet hide.
    pub hidden: AtomicBool,
    /// Boss key pressed.
    pub boss: AtomicBool,
    /// A fullscreen app is in front.
    pub fullscreen: AtomicBool,
    pub positions: windows::Positions,
    pub motion: physics::Motion,
    /// Boss key, focus toggle.
    hotkeys: Mutex<(Option<Shortcut>, Option<Shortcut>)>,
    /// 记一件事 (with what is selected, if anything).
    note_key: Mutex<Option<Shortcut>>,
}

/// The pet windows ask for this to reach petd's HTTP/WS API.
#[tauri::command]
fn petd_info(info: tauri::State<'_, Info>) -> Info {
    info.inner().clone()
}

/// The letter waiting in the box (kept by petd, so a restart does not lose it).
#[tauri::command]
fn take_letter(app: tauri::State<'_, Arc<App>>) -> Option<serde_json::Value> {
    petd::letters::take(&app.petd.shared.db)
}

#[tauri::command]
fn model_state(app: tauri::State<'_, Arc<App>>) -> serde_json::Value {
    let ready = petd::local_llm::is_ready(&app.petd.paths.home);
    serde_json::json!({
        "ready": ready,
        "done": MODEL_DL_DONE.load(Ordering::Relaxed),
        "total": MODEL_DL_TOTAL.load(Ordering::Relaxed),
    })
}

#[tauri::command]
async fn start_model_download(app: tauri::State<'_, Arc<App>>) -> Result<(), String> {
    if petd::local_llm::is_ready(&app.petd.paths.home) {
        return Ok(());
    }
    if MODEL_DL_TOTAL.load(Ordering::Relaxed) > 0 {
        return Ok(()); // already downloading
    }
    let home = app.petd.paths.home.clone();
    tauri::async_runtime::spawn(async move {
        MODEL_DL_TOTAL.store(petd::local_llm::MODEL_BYTES, Ordering::Relaxed);
        let _ = petd::local_llm::download(&home, |done, total| {
            MODEL_DL_DONE.store(done, Ordering::Relaxed);
            MODEL_DL_TOTAL.store(total, Ordering::Relaxed);
        }).await;
        // Reset so is_ready() is the source of truth
        MODEL_DL_DONE.store(0, Ordering::Relaxed);
        MODEL_DL_TOTAL.store(0, Ordering::Relaxed);
    });
    Ok(())
}

#[tauri::command]
fn set_hit_rects(window: WebviewWindow, app: tauri::State<'_, Arc<App>>, rects: Vec<Rect>) {
    let mut hits = app.hits.lock().unwrap();
    let e = hits.entry(window.label().to_string()).or_insert((vec![], false));
    e.0 = rects;
}

#[tauri::command]
async fn pet_menu(window: WebviewWindow, app: tauri::State<'_, Arc<App>>, pet: String) -> Result<(), String> {
    let has_letter = petd::letters::waiting(&app.petd.shared.db);
    let model = radial::build(&app, &pet, has_letter).ok_or("no such pet")?;
    let r = radial::open(window.app_handle(), &window, model);
    if let Err(e) = &r {
        app.petd.paths.log(&format!("menu: {e}"));
    }
    r
}

/// A pick in the right-click menu: same ids as the tray.
/// Async: picks may open windows (gallery, report), and a sync command runs
/// on the main thread, where creating a window deadlocks.
#[tauri::command]
async fn menu_pick(h: AppHandle, id: String) {
    radial::close(&h);
    tray::close(&h);
    let h2 = h.clone();
    let _ = h.run_on_main_thread(move || on_menu(&h2, &id));
}

#[tauri::command]
fn menu_close(h: AppHandle) {
    radial::close(&h);
    tray::close(&h);
}

/// A toggle in the tray panel: run it, keep the panel open, send fresh state.
#[tauri::command]
async fn tray_pick(h: AppHandle, id: String) {
    let h2 = h.clone();
    let _ = h.run_on_main_thread(move || {
        on_menu(&h2, &id);
        let app = h2.state::<Arc<App>>().inner().clone();
        refresh_tray(&h2, &app);
    });
}

/// The tray panel asks for its state (also pushed on open).
#[tauri::command]
fn tray_info(app: tauri::State<'_, Arc<App>>) -> serde_json::Value {
    tray_state(app.inner())
}

/// Mouse went down on the pet and moved: the host follows the cursor and
/// throws the pet on release.
#[tauri::command]
fn drag_start(window: WebviewWindow, app: tauri::State<'_, Arc<App>>) {
    physics::drag(app.inner().clone(), window);
}

/// The overlay sends a pet after a bug: run along the floor to `x` (logical screen px), or home when `x` is None.
#[tauri::command]
fn pet_chase(h: AppHandle, app: tauri::State<'_, Arc<App>>, pet: String, x: Option<f64>) {
    physics::chase(app.inner().clone(), &h, &pet, x);
}

/// Walk a pet over to stand beside another (onto that one's floor, on whatever screen it is), or home with None.
#[tauri::command]
fn pet_visit(h: AppHandle, app: tauri::State<'_, Arc<App>>, pet: String, target: Option<String>) {
    physics::walk(app.inner().clone(), &h, &pet, target.as_deref());
}

/// The sweep: a pet rides its broom until its window's top-left is at (x, y), physical px.
#[tauri::command]
fn pet_fly(h: AppHandle, app: tauri::State<'_, Arc<App>>, pet: String, x: f64, y: f64, ms: f64) {
    physics::fly_to(app.inner().clone(), &h, &pet, x, y, ms);
}

/// The desktop's icons and where each goes (None: nothing to tidy, icons hidden or auto-arranged).
#[tauri::command]
async fn desk_plan() -> Option<desktop::Plan> {
    desktop::plan()
}

/// The put-away sweep: the old files' icons fly into the folder `drawer`
/// (already made, its icon on the desktop); everything else is packed around it.
#[tauri::command]
async fn desk_stow_plan(paths: Vec<String>, drawer: String) -> Option<desktop::Plan> {
    desktop::stow_plan(paths, drawer)
}

/// The sweep starts: every icon's place is kept for "put them back".
#[tauri::command]
async fn desk_begin() -> bool {
    desktop::begin()
}

/// Icons on their way, a frame at a time: [key, x, y] in physical px.
#[tauri::command]
async fn desk_move(moves: Vec<(String, i32, i32)>) {
    desktop::moves(moves)
}

/// The sweep is over: the grid as it was, and a line in the diary (`who`: which of the five swept, `n`: icons put away).
#[tauri::command]
async fn desk_end(h: AppHandle, who: String, n: u32) {
    desktop::end();
    if n > 0 {
        petd::tidy::noted(&h.state::<Arc<App>>().petd.shared, &who, n);
    }
}

/// Every icon back where it was before the last sweep; how many went back.
#[tauri::command]
async fn desk_undo() -> u32 {
    desktop::undo()
}

/// A pet clinging to a screen edge lets go (it was clicked, or its agent needs you).
#[tauri::command]
fn pet_let_go(window: WebviewWindow, app: tauri::State<'_, Arc<App>>, why: String) {
    physics::let_go(&app, window.label(), &why);
}

/// Double-click: bring the terminal / IDE of a session to the front
/// (`project`: picks among an app's windows when it has several).
#[tauri::command]
fn focus_session(pid: u32, project: Option<String>) -> bool {
    pid != 0 && windows::focus_process_window(pid, project.as_deref().unwrap_or(""))
}

/// Annoyed pets hop away (logical px/s).
#[tauri::command]
fn pet_hop(window: WebviewWindow, app: tauri::State<'_, Arc<App>>, vx: f64, vy: f64) {
    physics::throw(app.inner().clone(), window, vx, vy, false);
}

#[tauri::command]
fn open_config(app: tauri::State<'_, Arc<App>>) {
    open_path(&app.petd.paths.config());
}

/// The welcome window's "hook up" buttons: same confirm-and-install as the tray.
#[tauri::command]
async fn connect_agent(h: AppHandle, which: String) {
    connect(&h, &which);
}

#[tauri::command]
async fn open_report_window(app: AppHandle) {
    windows::open_report(&app);
}

/// Boss key / fullscreen state for pages that want to stay quiet.
#[tauri::command]
fn quiet(app: tauri::State<'_, Arc<App>>) -> bool {
    !windows::visible(&app)
}

fn open_path(p: &std::path::Path) {
    #[cfg(windows)]
    let _ = std::process::Command::new("explorer").arg(p).spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(p).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let _ = std::process::Command::new("xdg-open").arg(p).spawn();
}

/// Language choices for the tray and the right-click menu: (id, Chinese, English).
const LANGS: [(&str, &str, &str); 3] = [("auto", "跟随系统", "Auto"), ("zh", "中文", "中文"), ("en", "English", "English")];

fn current_language(app: &App) -> String {
    app.petd.hub.lock().unwrap().cfg.language.to_ascii_lowercase()
}

/// Everything the tray panel shows (tray.html renders it in the app's style).
fn tray_state(app: &App) -> serde_json::Value {
    let (focus, size, sounds, chatter, hidden, views) = {
        let hub = app.petd.hub.lock().unwrap();
        let f = &hub.cfg.features;
        (f.focus_mode, f.size.to_lowercase(), f.sounds, f.chatter.clone(), hub.hidden, hub.pets())
    };
    // Who is here: each pet, whose agent it stands for, and what that agent is doing.
    let now = petd::now_ms();
    let pets: Vec<_> = views
        .iter()
        .map(|v| {
            let waiting = v.state == petd::session::State::Attention;
            let state = if waiting {
                let m = now.saturating_sub(v.since) / 60_000;
                if m >= 1 { petd::tr!("等你 {m} 分钟", "waiting {m} min") } else { petd::i18n::t("等你", "waiting for you").to_string() }
            } else {
                v.state.label().to_string()
            };
            let name = petd::cast::by_id(&v.skin).map(|f| f.name()).unwrap_or("");
            serde_json::json!({ "id": v.id, "camp": v.camp, "agent": v.marker, "state": state, "waiting": waiting, "skin": v.skin, "name": name })
        })
        .collect();
    let (boss, focus_key) = hotkey_names(app);
    let langs: Vec<_> = LANGS.iter().map(|(id, zh, en)| serde_json::json!({ "id": id, "label": petd::i18n::t(zh, en) })).collect();
    // The panel belongs to no one pet: it wears the main pet's skin.
    let skin = petd::cast::main(&app.petd.hub.lock().unwrap().cfg).id;
    serde_json::json!({
        "skin": skin,
        "version": env!("CARGO_PKG_VERSION"),
        "en": petd::i18n::is_en(),
        "lang": current_language(app),
        "langs": langs,
        "focus": focus,
        "size": size,
        "sounds": sounds,
        "initiative": chatter != "quiet",
        "hidden": hidden,
        "pets": pets,
        "autostart": setup::autostart_on(),
        "boss": boss,
        "focusKey": focus_key,
        "modelReady": petd::local_llm::is_ready(&app.petd.paths.home),
        "modelDlDone": MODEL_DL_DONE.load(Ordering::Relaxed),
        "modelDlTotal": MODEL_DL_TOTAL.load(Ordering::Relaxed),
    })
}

/// Push fresh state to the tray panel if it is open.
fn refresh_tray(h: &AppHandle, app: &App) {
    tray::refresh(h, tray_state(app));
}

fn toggle_focus(app: &App) {
    let on = !app.petd.hub.lock().unwrap().cfg.features.focus_mode;
    let _ = petd::ops::set_focus(&app.petd.shared, on);
}

/// Boss key: everything disappears in one frame and goes quiet; pressing it
/// again brings the pets back with a peek.
fn toggle_boss(h: &AppHandle, app: &App) {
    let on = !app.boss.load(Ordering::SeqCst);
    app.boss.store(on, Ordering::SeqCst);
    windows::apply_visibility(h, app);
    for (label, _) in h.webview_windows() {
        if label.starts_with("pet-") || label.starts_with("overlay-") {
            let _ = h.emit_to(label.as_str(), "boss", on);
            if !on && label.starts_with("pet-") {
                let _ = h.emit_to(label.as_str(), "phys", serde_json::json!({ "kind": "peek" }));
            }
        }
    }
}

/// Hotkeys from config.yaml (`hotkeys.boss_key`, `hotkeys.focus_toggle`).
fn read_hotkeys(home: &std::path::Path) -> (String, String) {
    let v: serde_yaml::Value = std::fs::read_to_string(home.join("config.yaml"))
        .ok()
        .and_then(|t| serde_yaml::from_str(&t).ok())
        .unwrap_or(serde_yaml::Value::Null);
    let get = |k: &str, d: &str| v.get("hotkeys").and_then(|h| h.get(k)).and_then(|x| x.as_str()).unwrap_or(d).to_string();
    (get("boss_key", "Ctrl+Alt+H"), get("focus_toggle", "Ctrl+Alt+F"))
}

fn hotkey_names(app: &App) -> (String, String) {
    let k = *app.hotkeys.lock().unwrap();
    let name = |s: &Option<Shortcut>| s.map(|s| s.into_string()).unwrap_or_else(|| petd::i18n::t("未设置", "not set").into());
    (name(&k.0).replace("control", "Ctrl").replace("alt", "Alt").replace("shift", "Shift").replace("Key", ""), name(&k.1).replace("control", "Ctrl").replace("alt", "Alt").replace("shift", "Shift").replace("Key", ""))
}

fn register_hotkeys(h: &AppHandle, app: &App) {
    let (boss, focus) = read_hotkeys(&app.petd.paths.home);
    let gs = h.global_shortcut();
    let parse = |s: &str| s.parse::<Shortcut>().map_err(|e| app.petd.paths.log(&format!("hotkey {s}: {e}"))).ok();
    let (b, f) = (parse(&boss), parse(&focus));
    for s in [b, f].into_iter().flatten() {
        if let Err(e) = gs.register(s) {
            app.petd.paths.log(&format!("hotkey {}: {e}", s.into_string()));
        }
    }
    *app.hotkeys.lock().unwrap() = (b, f);
    let v: serde_yaml::Value = std::fs::read_to_string(app.petd.paths.home.join("config.yaml"))
        .ok()
        .and_then(|t| serde_yaml::from_str(&t).ok())
        .unwrap_or(serde_yaml::Value::Null);
    let note = v.get("hotkeys").and_then(|h| h.get("note")).and_then(|x| x.as_str()).unwrap_or("Ctrl+Alt+N").to_string();
    let n = parse(&note);
    if let Some(s) = n {
        if let Err(e) = gs.register(s) {
            app.petd.paths.log(&format!("hotkey {}: {e}", s.into_string()));
        }
    }
    *app.note_key.lock().unwrap() = n;
}

/// 记一件事 over a pet's head (the first pet when none is named), starting from `text`.
fn open_note(h: &AppHandle, app: &App, pet: Option<String>, text: Option<String>) -> Result<(), String> {
    let pet = pet.or_else(|| app.petd.shared.hub.lock().unwrap().pets().first().map(|p| p.id.clone())).ok_or("no pet")?;
    let win = h.get_webview_window(&format!("pet-{pet}")).ok_or("no pet window")?;
    let model = radial::build(app, &pet, false).ok_or("no such pet")?;
    radial::open_note(h, &win, model, text)
}

/// The card's 「记一件事…」: the line to write in, over that pet's head.
#[tauri::command]
async fn note_open(h: AppHandle, app: tauri::State<'_, Arc<App>>, pet: Option<String>, text: Option<String>) -> Result<(), String> {
    open_note(&h, app.inner(), pet, text)
}

/// Share-toolbar windows of common meeting apps (title or class contains
/// one of these). Extend with `meeting.share_windows` in config.yaml.
const SHARE_WINDOWS: &[&str] = &[
    "共享屏幕工具栏", "正在共享屏幕", "您正在共享", "你正在共享", "共享控制栏", "屏幕共享中",
    "ZPToolBarParentWnd", "zoom share toolbar", "Sharing control bar", "is sharing your screen",
];

fn share_patterns(home: &std::path::Path) -> Vec<String> {
    let v: serde_yaml::Value = std::fs::read_to_string(home.join("config.yaml"))
        .ok()
        .and_then(|t| serde_yaml::from_str(&t).ok())
        .unwrap_or(serde_yaml::Value::Null);
    let mut out: Vec<String> = SHARE_WINDOWS.iter().map(|s| s.to_string()).collect();
    if let Some(list) = v.get("meeting").and_then(|m| m.get("share_windows")).and_then(|l| l.as_sequence()) {
        out.extend(list.iter().filter_map(|x| x.as_str().map(String::from)));
    }
    out
}

/// Hide everything while a fullscreen app (video, slides, game) is in front
/// or a meeting app shares the screen; come back 3 s after it ends.
fn spawn_fullscreen_guard(h: AppHandle, app: Arc<App>) {
    std::thread::spawn(move || {
        let mut clear_since: Option<Instant> = None;
        let mut patterns = share_patterns(&app.petd.paths.home);
        let mut n = 0u64;
        loop {
            std::thread::sleep(Duration::from_millis(700));
            n += 1;
            if n % 40 == 0 {
                patterns = share_patterns(&app.petd.paths.home);
            }
            let fs = windows::fullscreen_in_front() || windows::sharing_screen(&patterns);
            let was = app.fullscreen.load(Ordering::SeqCst);
            if fs {
                clear_since = None;
                if !was {
                    app.fullscreen.store(true, Ordering::SeqCst);
                    let (h2, a2) = (h.clone(), app.clone());
                    let _ = h.run_on_main_thread(move || windows::apply_visibility(&h2, &a2));
                }
            } else if was {
                let since = *clear_since.get_or_insert_with(Instant::now);
                if since.elapsed() >= Duration::from_secs(3) {
                    clear_since = None;
                    app.fullscreen.store(false, Ordering::SeqCst);
                    let (h2, a2) = (h.clone(), app.clone());
                    let _ = h.run_on_main_thread(move || windows::apply_visibility(&h2, &a2));
                }
            }
        }
    });
}

/// A line from the pet that was right-clicked (optionally with an action).
fn pet_say(h: &AppHandle, pet: &str, text: &str, action: Option<&str>) {
    let _ = h.emit_to(format!("pet-{pet}").as_str(), "say", serde_json::json!({ "text": text, "action": action }));
}

/// Every menu (tray and right-click) lands here. Entries about one pet carry its id: kind|pet|…
fn on_menu(h: &AppHandle, id: &str) {
    let app = h.state::<Arc<App>>().inner().clone();
    let sh = &app.petd.shared;
    let g: Vec<&str> = id.split('|').collect();
    match g.as_slice() {
        // A "yes" on something a pet offered: offer|pet|radio:shelf, or offer|pet|storeroom.
        ["offer", pet, what] => {
            match what.split_once(':') {
                Some(("radio", shelf)) => windows::open_radio_playing(h, pet, shelf),
                // The storeroom on one of its views: desktop, downloads, or "disk" (where most could be freed).
                Some(("storeroom", view)) => windows::open_tab(h, "storeroom", None, Some(view)),
                _ if *what == "storeroom" => windows::open_storeroom(h),
                // 收工曲: today's, played in the window's own tab.
                _ if *what == "song" => {
                    let today = petd::db::day_of(petd::now_ms());
                    windows::open_tab(h, "song", Some(pet), Some(&format!("play:{today}")));
                }
                // After the rename: Claude Code (and the rest) move over to the new names.
                _ if *what == "rehook" => {
                    let (h, pet) = (h.clone(), pet.to_string());
                    std::thread::spawn(move || {
                        let Some(root) = setup::plugin_root(h.path().resource_dir().ok()) else { return };
                        pet_say(&h, &pet, &setup::move_hookups(&root), None);
                    });
                }
                // The pet sweeps the desktop: its own window runs the sweep.
                _ if *what == "tidy" => {
                    let _ = h.emit_to(format!("pet-{pet}"), "sweep", serde_json::json!({ "stow": false }));
                }
                // ...and puts the files nobody touched in weeks into 桌面收纳 as it goes.
                _ if *what == "stow" => {
                    let _ = h.emit_to(format!("pet-{pet}"), "sweep", serde_json::json!({ "stow": true }));
                }
                _ => {}
            }
            return;
        }
        ["codexcard", pet] => {
            open_codex_usage();
            pet_say(
                h,
                pet,
                &petd::tr!(
                    "在 Codex 里输入 /usage 选「使用重置卡」（桌面版：设置 → 用量）。用掉了我会知道的。",
                    "In Codex, type /usage and pick \"Use reset card\" (desktop app: Settings → Usage). I'll know when you've used it."
                ),
                None,
            );
            return;
        }
        ["codexdemo", pet] => {
            let _ = h.emit_to(format!("pet-{pet}").as_str(), "fx", serde_json::json!({ "kind": "card", "from": 96.0, "used": 0.0 }));
            return;
        }
        // 喂它: `feed|pet` one fish, `feed|pet|n` up to n. A secret ends the batch.
        ["feed", pet] | ["feed", pet, _] => {
            use petd::bond::BondReward;
            let n: usize = g.get(2).and_then(|s| s.parse().ok()).unwrap_or(1).min(50);
            let (mut text, mut bond, mut count, mut fed) = (String::new(), "none", 0i64, 0i64);
            // Whoever is fed answers as itself.
            let fellow = { let hub = sh.hub.lock().unwrap(); petd::cast::of(&hub.cfg, pet) };
            for _ in 0..n {
                let (said, reward, feeds) = sh.world.lock().unwrap().feed(pet, fellow.id, petd::now_ms(), &sh.db);
                text = said;
                if matches!(reward, BondReward::NoFish) {
                    break;
                }
                (count, fed) = (feeds, fed + 1);
                bond = match reward {
                    BondReward::NoFish => "none",
                    BondReward::Whisper(_) => "whisper",
                    BondReward::Secret(_) => "secret",
                };
                if bond != "whisper" {
                    break;
                }
            }
            // What it ate, so the window can show that very one flying in.
            let treat = {
                let w = sh.world.lock().unwrap();
                w.last_fed.as_ref().and_then(|id| w.t.fish.iter().find(|f| &f.id == id)).map(|f| serde_json::json!({ "shape": f.shape, "colors": f.colors }))
            };
            let _ = h.emit_to(format!("pet-{pet}").as_str(), "fx", serde_json::json!({ "kind": "feed", "ok": fed > 0, "text": text, "bond": bond, "count": count, "batch": fed, "treat": treat }));
            // Fed while it clings to a screen edge: heavier now, it slips off.
            if fed > 0 {
                physics::let_go(&app, &format!("pet-{pet}"), "fed");
            }
            return;
        }
        _ => {}
    }
    let mut parts = id.splitn(3, '|');
    match (parts.next(), parts.next(), parts.next()) {
        (Some("act"), Some(pet), Some(kind)) => {
            petd::ops::perform(sh, pet, kind);
        }
        (Some("kegel"), pet, _) => {
            if let Err(e) = petd::ops::kegel_now(sh, pet) {
                app.petd.paths.log(&format!("kegel: {e}"));
            }
        }
        (Some("lang"), Some(lang), _) => {
            if let Err(e) = petd::ops::set_language(sh, lang) {
                app.petd.paths.log(&format!("language: {e}"));
            }
            refresh_tray(h, &app);
        }
        (Some("focus"), _, _) => toggle_focus(&app),
        (Some("sounds"), _, _) => {
            let on = !app.petd.hub.lock().unwrap().cfg.features.sounds;
            let _ = petd::ops::set_setting(sh, "sounds", &serde_json::Value::Bool(on));
        }
        (Some("size"), Some(s), _) => {
            let _ = petd::ops::set_size(sh, s);
        }
        (Some("chatter"), Some(s), _) => {
            let _ = petd::ops::set_chatter(sh, s);
        }
        (Some("hide"), Some(pet), _) => petd::ops::hide_pet(sh, pet),
        (Some("show"), _, _) => {
            app.boss.store(false, Ordering::SeqCst);
            petd::ops::show_all(sh)
        }
        (Some("hideall"), _, _) => petd::ops::hide_all(sh),
        (Some("room"), pet, _) => windows::open_room(h, pet),
        (Some("gallery"), _, _) => windows::open_gallery(h),
        // The kegel reps live with the other settings now.
        (Some("settings" | "reminders"), _, _) => windows::open_settings(h),
        (Some("report"), _, _) => windows::open_report(h),
        (Some("storeroom"), _, _) => windows::open_storeroom(h),
        (Some("radio"), pet, _) => windows::open_radio(h, pet),
        (Some("config"), _, _) => open_path(&app.petd.paths.config()),
        (Some("folder"), _, _) => open_path(&app.petd.paths.home),
        (Some("autostart"), _, _) => {
            let want = !setup::autostart_on();
            if let Err(e) = setup::set_autostart(want) {
                app.petd.paths.log(&format!("autostart: {e}"));
            }
            refresh_tray(h, &app);
        }
        (Some("connect"), Some(which), _) => connect(h, which),
        (Some("read_letter"), _, _) => {
            if petd::letters::waiting(&app.petd.shared.db) {
                crate::windows::open_letter(h);
            }
        }
        (Some("quit"), _, _) => {
            app.positions.save_now();
            h.exit(0);
        }
        _ => {}
    }
}

/// A terminal running `codex`, where `/usage` spends a banked reset.
fn open_codex_usage() {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // `start` reads its first argument as the new window's title: give it an empty one, so `cmd` is the program.
        let home = std::env::var_os("USERPROFILE").map(std::path::PathBuf::from);
        let mut cmd = std::process::Command::new("cmd");
        cmd.args(["/C", "start", "", "cmd", "/K", "echo Type /usage in Codex to use a reset card && codex"]).creation_flags(0x0800_0000);
        if let Some(h) = home.filter(|h| h.is_dir()) {
            cmd.current_dir(h);
        }
        let _ = cmd.spawn();
    }
}

/// Tray → 接入: confirm, run the installer step off the main thread, report.
fn connect(h: &AppHandle, which: &str) {
    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
    let Some(root) = setup::plugin_root(h.path().resource_dir().ok()) else {
        h.dialog().message(petd::i18n::t("找不到随安装包附带的插件文件。", "Could not find the plugin files that ship with the installer.")).title("Rill").kind(MessageDialogKind::Error).show(|_| {});
        return;
    };
    // (what to tell the user first, the dialog title, the agent the installer names)
    let which = which.to_string();
    let other = petd::config::OTHER_AGENTS.iter().find(|a| a.0 == which).map(|a| a.1);
    let (ask, title) = match (which.as_str(), other) {
        ("usage", _) => (
            petd::i18n::t(
                "会把 Claude Code 的状态栏命令（settings.json 里的 statusLine）接到睿洱：这是 Claude Code 在本机报告 5 小时和每周额度的唯一位置。你原来的状态栏命令会保留并照常显示；写入前备份为 settings.json.rillbuddy-bak。只有订阅账号才有这些数字。",
                "This points Claude Code's status line command (statusLine in settings.json) at Rill: it is the only place Claude Code reports the 5-hour and weekly limits locally. A status line you already have is kept and still shown; a backup is saved as settings.json.rillbuddy-bak first. Only subscription accounts have these numbers.",
            )
            .to_string(),
            petd::i18n::t("接上用量显示", "Hook up the usage display").to_string(),
        ),
        ("codex", _) => (
            petd::i18n::t(
                "会在 Codex 的 hooks.json 里追加睿洱的 hook（原有 hook 保留，写入前备份为 hooks.json.rillbuddy-bak）。",
                "This adds the Rill hooks to Codex's hooks.json (your existing hooks stay; a backup is saved as hooks.json.rillbuddy-bak first).",
            )
            .to_string(),
            petd::i18n::t("接入 Codex", "Hook up Codex").to_string(),
        ),
        (_, Some(name)) => (
            petd::tr!(
                "会在 {name} 自己的配置里追加睿洱的 hook（原有的保留，写入前备份）。它会有一只自己的宠物，显示它的状态。",
                "This adds the Rill hooks to {name}'s own config (yours stay; a backup is saved first). It gets a pet of its own that shows what it is doing."
            ),
            petd::tr!("接入 {name}", "Hook up {name}"),
        ),
        _ => (
            petd::i18n::t(
                "会运行 claude plugin marketplace add 和 claude plugin install，把睿洱插件装进 Claude Code。",
                "This runs claude plugin marketplace add and claude plugin install to put the Rill plugin into Claude Code.",
            )
            .to_string(),
            petd::i18n::t("接入 Claude Code", "Hook up Claude Code").to_string(),
        ),
    };
    let h2 = h.clone();
    h.dialog()
        .message(ask)
        .title(title)
        .buttons(MessageDialogButtons::OkCancelCustom(petd::i18n::t("接入", "Hook up").into(), petd::i18n::t("取消", "Cancel").into()))
        .show(move |ok| {
            if !ok {
                return;
            }
            std::thread::spawn(move || {
                let r = match which.as_str() {
                    "usage" => setup::connect_usage(&root),
                    "codex" => setup::connect_codex(&root),
                    id if other.is_some() => setup::connect_other(&root, id),
                    _ => setup::connect_claude(&root),
                };
                let (text, kind) = match r {
                    Ok(t) => (t, MessageDialogKind::Info),
                    Err(e) => (petd::tr!("没接上：
{e}", "Could not hook it up:
{e}"), MessageDialogKind::Error),
                };
                h2.dialog().message(text).title("Rill").kind(kind).show(|_| {});
            });
        });
}

fn main() {
    setup::adopt_old_webview_data();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|h, shortcut, ev| {
                    if ev.state() != ShortcutState::Pressed {
                        return;
                    }
                    let app = h.state::<Arc<App>>().inner().clone();
                    let (boss, focus) = *app.hotkeys.lock().unwrap();
                    if Some(*shortcut) == boss {
                        toggle_boss(h, &app);
                    } else if Some(*shortcut) == focus {
                        toggle_focus(&app);
                    } else if Some(*shortcut) == *app.note_key.lock().unwrap() {
                        // What you selected, read off the main thread (a slow window must not freeze the pets).
                        let h = h.clone();
                        std::thread::spawn(move || {
                            let text = selection::selected();
                            let app = h.state::<Arc<App>>().inner().clone();
                            if let Err(e) = open_note(&h, &app, None, text) {
                                app.petd.paths.log(&format!("note: {e}"));
                            }
                        });
                    }
                })
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            petd_info,
            set_hit_rects,
            pet_menu,
            menu_pick,
            menu_close,
            note_open,
            tray_info,
            tray_pick,
            drag_start,
            pet_let_go,
            pet_chase,
            pet_visit,
            pet_fly,
            desk_plan,
            desk_begin,
            desk_stow_plan,
            desk_move,
            desk_end,
            desk_undo,
            pet_hop,
            focus_session,
            open_config,
            open_report_window,
            connect_agent,
            quiet,
            model_state,
            start_model_download,
            take_letter
        ])
        .on_menu_event(|h, ev| on_menu(h, ev.id().as_ref()))
        .on_window_event(|w, ev| {
            if let tauri::WindowEvent::Moved(_) = ev {
                if w.label().starts_with("pet-") {
                    let app = w.app_handle().state::<Arc<App>>();
                    app.positions.remember(w);
                }
            }
        })
        .setup(|app| {
            let handle = app.handle().clone();
            let daemon = match tauri::async_runtime::block_on(petd::start(Options::default())) {
                Ok(d) => d,
                // An emitter raced us or the user double-clicked: the other copy wins.
                Err(StartError::AlreadyRunning) => std::process::exit(0),
                Err(e) => return Err(e.to_string().into()),
            };

            app.manage(Info { port: daemon.port, token: daemon.token.clone() });
            setup::first_install(&daemon.paths.home);
            let positions = windows::Positions::load(daemon.paths.home.join("windows.json"));
            let first_run = daemon.first_run;
            let mut rx = daemon.control.subscribe();
            let ids: Vec<String> = daemon.hub.lock().unwrap().pets().into_iter().map(|v| v.id).collect();
            let state = Arc::new(App {
                petd: daemon,
                hits: Mutex::new(HashMap::new()),
                hidden: AtomicBool::new(false),
                boss: AtomicBool::new(false),
                fullscreen: AtomicBool::new(false),
                positions,
                motion: physics::Motion::default(),
                hotkeys: Mutex::new((None, None)),
                note_key: Mutex::new(None),
            });
            app.manage(state.clone());
            register_hotkeys(&handle, &state);

            // A pixel-exact 32 px icon for the tray; the panel opens on either click.
            let tray_icon = tauri::image::Image::new_owned(include_bytes!("../icons/tray.rgba").to_vec(), 32, 32);
            let st_tray = state.clone();
            TrayIconBuilder::with_id("main")
                .icon(tray_icon)
                .tooltip(petd::i18n::t("睿洱", "Rill"))
                .on_tray_icon_event(move |t, ev| {
                    if let tauri::tray::TrayIconEvent::Click { button_state: tauri::tray::MouseButtonState::Up, position, .. } = ev {
                        let h = t.app_handle();
                        tray::open(h, position, tray_state(&st_tray));
                    }
                })
                .build(app)?;
            tray::create(&handle)?;

            windows::create_card(&handle)?;
            windows::create_approve(&handle)?;
            radial::create(&handle)?;
            windows::create_overlays(&handle);
            windows::sync_pets(&handle, &state, &ids);
            if first_run {
                windows::open_welcome(&handle);
            }
            windows::spawn_click_through(handle.clone(), state.clone());
            // After the rename: old hook-ups keep working; a pet offers to move them over.
            {
                let (h, st) = (handle.clone(), state.clone());
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_secs(40));
                    let home = st.petd.paths.home.clone();
                    let root = setup::plugin_root(h.path().resource_dir().ok());
                    if !setup::after_rename(root.as_deref(), &home) {
                        return;
                    }
                    let sh = &st.petd.shared;
                    let Some((pet, _)) = petd::drawer::asker(sh) else { return };
                    setup::mark_asked(&home);
                    let text = petd::tr!(
                        "我改了名字，Claude Code 那边还叫我旧名字。帮你改过来？",
                        "I go by a new name now, and Claude Code still knows me by the old one. Switch it over?"
                    );
                    let _ = sh.ws_tx.send(petd::msg("pet.act", &serde_json::json!({ "pet": pet, "act": "offer", "text": text, "why": "rename", "offer": "rehook" })));
                });
            }
            windows::spawn_position_saver(state.clone());
            spawn_fullscreen_guard(handle.clone(), state.clone());
            physics::spawn_separator(handle.clone(), state.clone());
            desktop::watch(state.clone());

            let h = handle.clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    let c = match rx.recv().await {
                        Ok(c) => c,
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(_) => break,
                    };
                    let h2 = h.clone();
                    let _ = h.run_on_main_thread(move || {
                        let st = h2.state::<Arc<App>>().inner().clone();
                        match c {
                            Control::Show => {
                                st.hidden.store(false, Ordering::SeqCst);
                                windows::apply_visibility(&h2, &st);
                            }
                            Control::Hide => {
                                st.hidden.store(true, Ordering::SeqCst);
                                windows::apply_visibility(&h2, &st);
                            }
                            Control::Layout { ids } => windows::sync_pets(&h2, &st, &ids),
                            Control::ConfigChanged => refresh_tray(&h2, &st),
                            Control::Move { pet, target } => physics::walk(st.clone(), &h2, &pet, target.as_deref()),
                            Control::Report => windows::open_report(&h2),
                            Control::Radio => windows::open_radio(&h2, None),
                            Control::Menu { pet } => {
                                let has_letter = petd::letters::waiting(&st.petd.shared.db);
                                if let (Some(w), Some(model)) = (h2.get_webview_window(&format!("pet-{pet}")), radial::build(&st, &pet, has_letter)) {
                                    let _ = radial::open(&h2, &w, model);
                                }
                            }
                            Control::Fling { pet, vx, vy } => {
                                if let Some(w) = h2.get_webview_window(&format!("pet-{pet}")) {
                                    physics::throw(st.clone(), w, vx, vy, true);
                                }
                            }
                        }
                    });
                }
            });
            // "Waiting N min" moves on; keep an open tray panel roughly current.
            let (h3, st3) = (handle.clone(), state.clone());
            std::thread::spawn(move || loop {
                std::thread::sleep(Duration::from_secs(30));
                let (h4, st4) = (h3.clone(), st3.clone());
                let _ = h3.run_on_main_thread(move || refresh_tray(&h4, &st4));
            });
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("failed to build Rill")
        // Closing the last pet window must not quit petd; only the tray does.
        .run(|_app, ev| {
            if let tauri::RunEvent::ExitRequested { api, code, .. } = ev {
                if code.is_none() {
                    api.prevent_exit();
                }
            }
        });
}
