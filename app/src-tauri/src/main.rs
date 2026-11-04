// No console window in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod physics;
mod radial;
mod setup;
mod windows;

use petd::{Control, Options, StartError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, WebviewWindow, Wry};
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
    hotkeys: Mutex<(Option<Shortcut>, Option<Shortcut>, Option<Shortcut>)>,
}

/// The pet windows ask for this to reach petd's HTTP/WS API.
#[tauri::command]
fn petd_info(info: tauri::State<'_, Info>) -> Info {
    info.inner().clone()
}

#[tauri::command]
fn set_hit_rects(window: WebviewWindow, app: tauri::State<'_, Arc<App>>, rects: Vec<Rect>) {
    let mut hits = app.hits.lock().unwrap();
    let e = hits.entry(window.label().to_string()).or_insert((vec![], false));
    e.0 = rects;
}

#[tauri::command]
async fn pet_menu(window: WebviewWindow, app: tauri::State<'_, Arc<App>>, pet: String) -> Result<(), String> {
    let model = radial::build(&app, &pet).ok_or("no such pet")?;
    let r = radial::open(window.app_handle(), &window, model);
    if let Err(e) = &r {
        app.petd.paths.log(&format!("menu: {e}"));
    }
    r
}

/// A pick in the ring menu: same ids as the tray.
/// Async: picks may open windows (gallery, report), and a sync command runs
/// on the main thread, where creating a window deadlocks.
#[tauri::command]
async fn menu_pick(h: AppHandle, id: String) {
    radial::close(&h);
    let h2 = h.clone();
    let _ = h.run_on_main_thread(move || on_menu(&h2, &id));
}

#[tauri::command]
fn menu_close(h: AppHandle) {
    radial::close(&h);
}

/// Mouse went down on the pet and moved: the host follows the cursor and
/// throws the pet on release.
#[tauri::command]
fn drag_start(window: WebviewWindow, app: tauri::State<'_, Arc<App>>) {
    physics::drag(app.inner().clone(), window);
}

/// Double-click: bring the terminal / IDE of a session to the front.
#[tauri::command]
fn focus_session(pid: u32) -> bool {
    pid != 0 && windows::focus_process_window(pid)
}

/// Annoyed pets hop away (logical px/s).
#[tauri::command]
fn pet_hop(window: WebviewWindow, app: tauri::State<'_, Arc<App>>, vx: f64, vy: f64) {
    physics::throw(app.inner().clone(), window, vx, vy);
}

#[tauri::command]
fn open_pets_dir(app: tauri::State<'_, Arc<App>>) {
    open_path(&app.petd.paths.pets());
}

#[tauri::command]
fn open_config(app: tauri::State<'_, Arc<App>>) {
    open_path(&app.petd.paths.config());
}

/// Install a pet pack from a folder or .zip (gallery button or drag and drop).
// Commands that create windows or do slow work are async: sync commands run
// on the main thread, and building a webview there deadlocks on Windows.
#[tauri::command]
async fn import_pack(app: tauri::State<'_, Arc<App>>, path: String, overwrite: bool) -> Result<petd::install::Outcome, String> {
    petd::install::import(&app.petd.shared, std::path::Path::new(&path), overwrite)
}

/// Install a pack generated in the gallery from a picture.
#[tauri::command]
async fn create_pack(app: tauri::State<'_, Arc<App>>, files: Vec<(String, Vec<u8>)>, overwrite: bool) -> Result<petd::install::Outcome, String> {
    petd::install::create(&app.petd.shared, files, overwrite)
}

/// Bytes of a picture the user chose (dialog or drag and drop) for the pet maker.
#[tauri::command]
async fn read_image(path: String) -> Result<Vec<u8>, String> {
    let p = std::path::Path::new(&path);
    let ok = p
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| ["png", "jpg", "jpeg", "webp", "bmp", "gif"].contains(&e.to_ascii_lowercase().as_str()));
    if !ok {
        return Err(petd::i18n::t("请选择 PNG、JPG 或 WebP 图片", "Choose a PNG, JPG or WebP picture").into());
    }
    let len = std::fs::metadata(p).map_err(|e| e.to_string())?.len();
    if len > 30 * 1024 * 1024 {
        return Err(petd::i18n::t("图片太大了（超过 30 MB）", "The picture is too big (over 30 MB)").into());
    }
    std::fs::read(p).map_err(|e| e.to_string())
}

#[tauri::command]
async fn remove_pack(app: tauri::State<'_, Arc<App>>, id: String) -> Result<String, String> {
    petd::install::remove(&app.petd.shared, &id)
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

fn builtin_pets(app: &AppHandle) -> Option<PathBuf> {
    if let Ok(dir) = app.path().resource_dir() {
        let p = dir.join("pets");
        if p.is_dir() {
            return Some(p);
        }
    }
    // `cargo run` from the repo.
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../pets");
    dev.is_dir().then_some(dev)
}

fn open_path(p: &std::path::Path) {
    #[cfg(windows)]
    let _ = std::process::Command::new("explorer").arg(p).spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(p).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let _ = std::process::Command::new("xdg-open").arg(p).spawn();
}

/// Routing modes: (id, Chinese, English). Pick the label with `mode_name`.
const MODES: [(&str, &str, &str); 4] = [
    ("per-camp", "按阵营（主宠物 + 分身）", "By team (main pet + minis)"),
    ("single", "单宠物", "Single pet"),
    ("per-rule", "按规则", "By rule"),
    ("per-session", "按会话", "Per session"),
];

/// Language choices for the tray and ring menu: (id, Chinese, English).
const LANGS: [(&str, &str, &str); 3] = [("auto", "跟随系统", "Auto"), ("zh", "中文", "中文"), ("en", "English", "English")];

fn current_language(app: &App) -> String {
    app.petd.hub.lock().unwrap().cfg.language.to_ascii_lowercase()
}

fn target_name(t: &str) -> String {
    petd::ops::target_name(t)
}

fn build_tray_menu(h: &AppHandle, app: &App) -> tauri::Result<Menu<Wry>> {
    let (broken, mode, focus) = {
        let hub = app.petd.hub.lock().unwrap();
        (hub.lib.broken.len(), hub.cfg.mode, hub.cfg.features.focus_mode)
    };
    let coins = app.petd.shared.db.balance();
    let gallery_label = if broken > 0 {
        petd::tr!("宠物图鉴…（{broken} 个包有错误）", "Gallery… ({broken} packs with errors)")
    } else {
        petd::tr!("宠物图鉴…", "Gallery…")
    };
    let modes = Submenu::with_id(h, "modes", petd::i18n::t("路由模式", "Routing mode"), true)?;
    for (id, zh, en) in MODES {
        modes.append(&CheckMenuItem::with_id(h, format!("mode|{id}"), petd::i18n::t(zh, en), true, mode.as_str() == id, None::<&str>)?)?;
    }
    let lang = current_language(app);
    let langs = Submenu::with_id(h, "langs", petd::i18n::t("语言", "Language"), true)?;
    for (id, zh, en) in LANGS {
        langs.append(&CheckMenuItem::with_id(h, format!("lang|{id}"), petd::i18n::t(zh, en), true, lang == id, None::<&str>)?)?;
    }
    let (boss, focus_key) = hotkey_names(app);
    Menu::with_items(
        h,
        &[
            &MenuItem::with_id(h, "show", petd::i18n::t("显示宠物", "Show pets"), true, None::<&str>)?,
            &MenuItem::with_id(h, "hideall", petd::tr!("全部隐藏（老板键 {boss}）", "Hide all (boss key {boss})"), true, None::<&str>)?,
            &CheckMenuItem::with_id(h, "focus", petd::tr!("专注模式（{focus_key}）", "Focus mode ({focus_key})"), true, focus, None::<&str>)?,
            &PredefinedMenuItem::separator(h)?,
            &MenuItem::with_id(h, "gallery", gallery_label, true, None::<&str>)?,
            &MenuItem::with_id(h, "report", petd::tr!("今日报告…（金币 {coins}）", "Today's report… ({coins} coins)"), true, None::<&str>)?,
            &MenuItem::with_id(h, "shop", petd::i18n::t("商店…", "Shop…"), true, None::<&str>)?,
            &modes,
            &langs,
            &PredefinedMenuItem::separator(h)?,
            &MenuItem::with_id(h, "config", petd::i18n::t("编辑配置…", "Edit config…"), true, None::<&str>)?,
            &MenuItem::with_id(h, "folder", petd::i18n::t("打开数据目录", "Open data folder"), true, None::<&str>)?,
            &Submenu::with_items(
                h,
                petd::i18n::t("接入", "Hook up"),
                true,
                &[
                    &MenuItem::with_id(h, "connect|claude", petd::i18n::t("接入 Claude Code…", "Hook up Claude Code…"), true, None::<&str>)?,
                    &MenuItem::with_id(h, "connect|codex", petd::i18n::t("接入 Codex…", "Hook up Codex…"), true, None::<&str>)?,
                ],
            )?,
            &CheckMenuItem::with_id(h, "autostart", petd::i18n::t("开机自启", "Start with Windows"), true, setup::autostart_on(), None::<&str>)?,
            &PredefinedMenuItem::separator(h)?,
            &MenuItem::with_id(h, "quit", petd::i18n::t("退出", "Quit"), true, None::<&str>)?,
        ],
    )
}

fn refresh_tray(h: &AppHandle, app: &App) {
    if let (Some(tray), Ok(menu)) = (h.tray_by_id("main"), build_tray_menu(h, app)) {
        let _ = tray.set_menu(Some(menu));
    }
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

/// Hotkeys from config.yaml (`hotkeys.boss_key`, `hotkeys.focus_toggle`, `hotkeys.cycle_pet`).
fn read_hotkeys(home: &std::path::Path) -> (String, String, String) {
    let v: serde_yaml::Value = std::fs::read_to_string(home.join("config.yaml"))
        .ok()
        .and_then(|t| serde_yaml::from_str(&t).ok())
        .unwrap_or(serde_yaml::Value::Null);
    let get = |k: &str, d: &str| v.get("hotkeys").and_then(|h| h.get(k)).and_then(|x| x.as_str()).unwrap_or(d).to_string();
    (get("boss_key", "Ctrl+Alt+H"), get("focus_toggle", "Ctrl+Alt+F"), get("cycle_pet", "Ctrl+Alt+P"))
}

fn hotkey_names(app: &App) -> (String, String) {
    let k = app.hotkeys.lock().unwrap();
    let name = |s: &Option<Shortcut>| s.map(|s| s.into_string()).unwrap_or_else(|| petd::i18n::t("未设置", "not set").into());
    let k = (k.0, k.1);
    (name(&k.0).replace("control", "Ctrl").replace("alt", "Alt").replace("shift", "Shift").replace("Key", ""), name(&k.1).replace("control", "Ctrl").replace("alt", "Alt").replace("shift", "Shift").replace("Key", ""))
}

fn register_hotkeys(h: &AppHandle, app: &App) {
    let (boss, focus, cycle) = read_hotkeys(&app.petd.paths.home);
    let gs = h.global_shortcut();
    let parse = |s: &str| s.parse::<Shortcut>().map_err(|e| app.petd.paths.log(&format!("hotkey {s}: {e}"))).ok();
    let (b, f, c) = (parse(&boss), parse(&focus), parse(&cycle));
    for s in [b, f, c].into_iter().flatten() {
        if let Err(e) = gs.register(s) {
            app.petd.paths.log(&format!("hotkey {}: {e}", s.into_string()));
        }
    }
    *app.hotkeys.lock().unwrap() = (b, f, c);
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

/// Every menu (tray and right-click) lands here.
/// A line from the pet that was right-clicked (optionally with an action).
fn pet_say(h: &AppHandle, pet: &str, text: &str, action: Option<&str>) {
    let _ = h.emit_to(format!("pet-{pet}").as_str(), "say", serde_json::json!({ "text": text, "action": action }));
}

fn on_menu(h: &AppHandle, id: &str) {
    let app = h.state::<Arc<App>>().inner().clone();
    let sh = &app.petd.shared;
    // Game entries carry the pet id: kind|pet|...
    let g: Vec<&str> = id.split('|').collect();
    match g.as_slice() {
        ["bet", pet, game, pick, stake] => {
            let stake_n: i64 = stake.parse().unwrap_or(0);
            let r = sh.world.lock().unwrap().bet(game, pick, stake_n, petd::now_ms(), &sh.db);
            // The chip hangs on the pet that was bet on (race) or the one clicked (bookmaker).
            let (target, odds, label) = if *game == "race" {
                let views = app.petd.hub.lock().unwrap().pets();
                let t = views.iter().find(|v| v.id == *pick).or_else(|| views.iter().find(|v| v.camp.as_deref() == Some(*pick))).map(|v| v.id.clone());
                let odds = sh.world.lock().unwrap().offers(petd::now_ms(), &sh.db).race.map(|r| if *pick == "claude-code" { r.0 } else { r.1 });
                (t.unwrap_or_else(|| pet.to_string()), odds.unwrap_or(0.0), String::new())
            } else {
                let p = petd::games::BOOK_PICKS.iter().find(|(id, _, _)| id == pick);
                (pet.to_string(), p.map(|x| x.2).unwrap_or(0.0), p.map(|x| petd::games::book_pick_name(x.0).to_string()).unwrap_or_default())
            };
            let ok = r.is_ok();
            let text = r.unwrap_or_else(|e| e);
            let _ = h.emit_to(
                format!("pet-{target}").as_str(),
                "fx",
                serde_json::json!({ "kind": "bet", "ok": ok, "text": text, "game": game, "pick": pick, "stake": stake_n, "odds": odds, "label": label }),
            );
            return;
        }
        ["duel", pet] => {
            let r = {
                let hub = app.petd.hub.lock().unwrap();
                sh.world.lock().unwrap().request_duel(&hub, petd::now_ms())
            };
            match r {
                Ok((_, outs)) => send_outs(sh, outs), // the pets explain the rules themselves
                Err(e) => pet_say(h, pet, &e, None),
            }
            return;
        }
        ["duelcancel", pet] => {
            let outs = sh.world.lock().unwrap().cancel_duel();
            send_outs(sh, outs);
            pet_say(h, pet, &petd::tr!("擂台取消了", "Duel called off"), None);
            return;
        }
        ["rps", pet, pick] => {
            let user: usize = pick.parse().unwrap_or(0);
            let (me, outcome, coins) = sh.world.lock().unwrap().rps_user(user, petd::now_ms(), &sh.db);
            let _ = h.emit_to(
                format!("pet-{pet}").as_str(),
                "fx",
                serde_json::json!({ "kind": "rps", "user": user, "pet": me, "outcome": outcome, "coins": coins }),
            );
            return;
        }
        ["duo", pet, game] => {
            let views = app.petd.hub.lock().unwrap().pets();
            let r = sh.world.lock().unwrap().request_duo(Some(game), pet, petd::now_ms(), &views, &sh.db);
            match r {
                Ok(outs) => send_outs(sh, outs),
                Err(e) => pet_say(h, pet, &e, None),
            }
            return;
        }
        ["codexcard", pet] => {
            open_codex_usage();
            pet_say(
                h,
                pet,
                &petd::tr!(
                    "在 Codex 里输入 /usage 选「使用重置卡」（桌面版：设置 → 用量）。用掉了我会知道的～",
                    "In Codex, type /usage and pick \"Use reset card\" (desktop app: Settings → Usage). I'll know when you've used it~"
                ),
                None,
            );
            return;
        }
        ["codexdemo", pet] => {
            let _ = h.emit_to(format!("pet-{pet}").as_str(), "fx", serde_json::json!({ "kind": "card", "from": 96.0, "used": 0.0 }));
            return;
        }
        ["feed", pet] => {
            let views = app.petd.hub.lock().unwrap().pets();
            if let Some(v) = views.iter().find(|v| v.id == *pet).cloned() {
                let (text, outs) = sh.world.lock().unwrap().feed(&v, &views, petd::now_ms(), &sh.db);
                for o in outs {
                    if let petd::world::Out::Ws(m) = o {
                        let _ = sh.ws_tx.send(m);
                    }
                }
                let ate = text == "好吃！" || text == petd::tr!("好吃！", "Yum!");
                let _ = h.emit_to(format!("pet-{pet}").as_str(), "fx", serde_json::json!({ "kind": "feed", "ok": ate, "text": text }));
            }
            return;
        }
        _ => {}
    }
    let mut parts = id.splitn(3, '|');
    match (parts.next(), parts.next(), parts.next()) {
        (Some("switch"), Some(target), Some(pack)) => {
            if let Err(e) = petd::ops::select(sh, target, pack) {
                app.petd.paths.log(&e);
            }
        }
        (Some("mode"), Some(mode), _) => {
            let _ = petd::ops::set_mode(sh, mode);
        }
        (Some("lang"), Some(lang), _) => {
            if let Err(e) = petd::ops::set_language(sh, lang) {
                app.petd.paths.log(&format!("language: {e}"));
            }
            refresh_tray(h, &app);
        }
        (Some("hide"), Some(pet), _) => petd::ops::hide_pet(sh, pet),
        (Some("unproj"), Some(pat), _) => {
            let _ = petd::ops::clear_project(sh, pat);
        }
        (Some("focus"), _, _) => toggle_focus(&app),
        (Some("tug"), _, _) => {
            let r = {
                let hub = app.petd.hub.lock().unwrap();
                app.petd.shared.world.lock().unwrap().request_tug(&hub)
            };
            if let Err(e) = r {
                // Tell the user through the pet that was right-clicked.
                let _ = h.emit("tug-refused", e);
            }
        }
        (Some("size"), Some(s), _) => {
            let _ = petd::ops::set_size(sh, s);
        }
        (Some("show"), _, _) => {
            app.boss.store(false, Ordering::SeqCst);
            petd::ops::show_all(sh)
        }
        (Some("hideall"), _, _) => petd::ops::hide_all(sh),
        (Some("gallery"), _, _) => windows::open_gallery(h),
        (Some("reminders"), _, _) => windows::open_reminders(h),
        (Some("kegel"), pet, _) => {
            if let Err(e) = petd::ops::kegel_now(sh, pet) {
                app.petd.paths.log(&format!("kegel: {e}"));
            }
        }
        (Some("report"), _, _) => windows::open_report(h),
        (Some("spring"), _, _) => windows::open_spring(h),
        (Some("shop"), _, _) => windows::open_shop(h),
        (Some("config"), _, _) => open_path(&app.petd.paths.config()),
        (Some("folder"), _, _) => open_path(&app.petd.paths.home),
        (Some("musicdir"), _, _) => {
            let dir = petd::spring::music_dir(&app.petd.paths.home);
            let _ = std::fs::create_dir_all(&dir);
            open_path(&dir)
        }
        (Some("autostart"), _, _) => {
            let want = !setup::autostart_on();
            if let Err(e) = setup::set_autostart(want) {
                app.petd.paths.log(&format!("autostart: {e}"));
            }
            refresh_tray(h, &app);
        }
        (Some("connect"), Some(which), _) => connect(h, which),
        (Some("quit"), _, _) => {
            app.positions.save_now();
            h.exit(0);
        }
        _ => {}
    }
}

fn send_outs(sh: &petd::server::Shared, outs: Vec<petd::world::Out>) {
    for o in outs {
        match o {
            petd::world::Out::Ws(m) => {
                let _ = sh.ws_tx.send(m);
            }
            petd::world::Out::Control(c) => {
                let _ = sh.control.send(c);
            }
        }
    }
}

/// A terminal running `codex`, where `/usage` spends a banked reset.
fn open_codex_usage() {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", "Codex", "cmd", "/K", "echo Type /usage in Codex to use a reset card && codex"])
            .creation_flags(0x0800_0000)
            .spawn();
    }
}

/// Tray → 接入: confirm, run the installer step off the main thread, report.
fn connect(h: &AppHandle, which: &str) {
    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
    let Some(root) = setup::plugin_root(h.path().resource_dir().ok()) else {
        h.dialog().message(petd::i18n::t("找不到随安装包附带的插件文件。", "Could not find the plugin files that ship with the installer.")).title("Familiar").kind(MessageDialogKind::Error).show(|_| {});
        return;
    };
    let codex = which == "codex";
    let ask = if codex {
        petd::i18n::t(
            "会在 Codex 的 hooks.json 里追加 Familiar 的 hook（原有 hook 保留，写入前备份为 hooks.json.familiar-bak）。",
            "This adds the Familiar hooks to Codex's hooks.json (your existing hooks stay; a backup is saved as hooks.json.familiar-bak first).",
        )
    } else {
        petd::i18n::t(
            "会运行 claude plugin marketplace add 和 claude plugin install，把 Familiar 插件装进 Claude Code。",
            "This runs claude plugin marketplace add and claude plugin install to put the Familiar plugin into Claude Code.",
        )
    };
    let h2 = h.clone();
    h.dialog()
        .message(ask)
        .title(if codex { petd::i18n::t("接入 Codex", "Hook up Codex") } else { petd::i18n::t("接入 Claude Code", "Hook up Claude Code") })
        .buttons(MessageDialogButtons::OkCancelCustom(petd::i18n::t("接入", "Hook up").into(), petd::i18n::t("取消", "Cancel").into()))
        .show(move |ok| {
            if !ok {
                return;
            }
            std::thread::spawn(move || {
                let r = if codex { setup::connect_codex(&root) } else { setup::connect_claude(&root) };
                let (text, kind) = match r {
                    Ok(t) => (t, MessageDialogKind::Info),
                    Err(e) => (petd::tr!("没接上：\n{e}", "Could not hook it up:\n{e}"), MessageDialogKind::Error),
                };
                h2.dialog().message(text).title("Familiar").kind(kind).show(|_| {});
            });
        });
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|h, shortcut, ev| {
                    if ev.state() != ShortcutState::Pressed {
                        return;
                    }
                    let app = h.state::<Arc<App>>().inner().clone();
                    let (boss, focus, cycle) = *app.hotkeys.lock().unwrap();
                    if Some(*shortcut) == boss {
                        toggle_boss(h, &app);
                    } else if Some(*shortcut) == focus {
                        toggle_focus(&app);
                    } else if Some(*shortcut) == cycle {
                        if let Err(e) = petd::ops::cycle_favorite(&app.petd.shared) {
                            app.petd.paths.log(&e);
                        }
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
            drag_start,
            pet_hop,
            focus_session,
            open_pets_dir,
            open_config,
            open_report_window,
            import_pack,
            create_pack,
            read_image,
            remove_pack,
            quiet
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
            let opts = Options { builtin_pets: builtin_pets(&handle), ..Default::default() };
            let daemon = match tauri::async_runtime::block_on(petd::start(opts)) {
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
                hotkeys: Mutex::new((None, None, None)),
            });
            app.manage(state.clone());
            register_hotkeys(&handle, &state);

            TrayIconBuilder::with_id("main")
                .icon(handle.default_window_icon().cloned().expect("app icon"))
                .tooltip("Familiar")
                .menu(&build_tray_menu(&handle, &state)?)
                .build(app)?;

            windows::create_card(&handle)?;
            radial::create(&handle)?;
            windows::create_overlays(&handle);
            windows::sync_pets(&handle, &state, &ids);
            if first_run {
                windows::open_welcome(&handle);
            }
            windows::spawn_click_through(handle.clone(), state.clone());
            windows::spawn_position_saver(state.clone());
            spawn_fullscreen_guard(handle.clone(), state.clone());
            physics::spawn_separator(handle.clone(), state.clone());

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
                            Control::LibraryChanged | Control::ConfigChanged => refresh_tray(&h2, &st),
                            Control::Move { pet, target } => physics::walk(st.clone(), &h2, &pet, target.as_deref()),
                            Control::Tug { mover, anchor } => physics::walk_tug(st.clone(), &h2, &mover, &anchor),
                            Control::Duo { mover, anchor, gap } => physics::walk_pair(st.clone(), &h2, &mover, &anchor, gap, "duo"),
                            Control::Report => windows::open_report(&h2),
                            Control::Spring => windows::open_spring(&h2),
                            Control::Menu { pet } => {
                                if let (Some(w), Some(model)) = (h2.get_webview_window(&format!("pet-{pet}")), radial::build(&st, &pet)) {
                                    let _ = radial::open(&h2, &w, model);
                                }
                            }
                            Control::Fling { pet, vx, vy } => {
                                if let Some(w) = h2.get_webview_window(&format!("pet-{pet}")) {
                                    physics::throw(st.clone(), w, vx, vy);
                                }
                            }
                        }
                    });
                }
            });
            // Coins change often; keep the tray label roughly current.
            let (h3, st3) = (handle.clone(), state.clone());
            std::thread::spawn(move || loop {
                std::thread::sleep(Duration::from_secs(30));
                let (h4, st4) = (h3.clone(), st3.clone());
                let _ = h3.run_on_main_thread(move || refresh_tray(&h4, &st4));
            });
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("failed to build Familiar")
        // Closing the last pet window must not quit petd; only the tray does.
        .run(|_app, ev| {
            if let tauri::RunEvent::ExitRequested { api, code, .. } = ev {
                if code.is_none() {
                    api.prevent_exit();
                }
            }
        });
}
