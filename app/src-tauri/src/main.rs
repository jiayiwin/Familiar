// No console window in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod physics;
mod radial;
mod setup;
mod tray;
mod windows;

use petd::{Control, Options, StartError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

static MODEL_DL_DONE: AtomicU64 = AtomicU64::new(0);
static MODEL_DL_TOTAL: AtomicU64 = AtomicU64::new(0);

// Letter pending delivery: generated before window is ready to receive events.
static PENDING_LETTER: Mutex<Option<serde_json::Value>> = Mutex::new(None);
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
    hotkeys: Mutex<(Option<Shortcut>, Option<Shortcut>, Option<Shortcut>)>,
}

/// The pet windows ask for this to reach petd's HTTP/WS API.
#[tauri::command]
fn petd_info(info: tauri::State<'_, Info>) -> Info {
    info.inner().clone()
}

#[tauri::command]
fn take_letter() -> Option<serde_json::Value> {
    PENDING_LETTER.lock().unwrap().take()
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
    let has_letter = PENDING_LETTER.lock().unwrap().is_some();
    let model = radial::build(&app, &pet, has_letter).ok_or("no such pet")?;
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

/// Everything the tray panel shows (tray.html renders it in the app's style).
fn tray_state(app: &App) -> serde_json::Value {
    let (broken, mode, focus) = {
        let hub = app.petd.hub.lock().unwrap();
        (hub.lib.broken.len(), hub.cfg.mode, hub.cfg.features.focus_mode)
    };
    let (boss, focus_key) = hotkey_names(app);
    let modes: Vec<_> = MODES.iter().map(|(id, zh, en)| serde_json::json!({ "id": id, "label": petd::i18n::t(zh, en) })).collect();
    let langs: Vec<_> = LANGS.iter().map(|(id, zh, en)| serde_json::json!({ "id": id, "label": petd::i18n::t(zh, en) })).collect();
    let model_ready = petd::local_llm::is_ready(&app.petd.paths.home);
    let dl_done = MODEL_DL_DONE.load(Ordering::Relaxed);
    let dl_total = MODEL_DL_TOTAL.load(Ordering::Relaxed);
    let habit_data = |ht: &str| {
        let db = &app.petd.shared.db;
        let streak: i64 = db.kv_get(&format!("habit:{}:streak", ht)).and_then(|v| v.parse().ok()).unwrap_or(0);
        let best: i64 = db.kv_get(&format!("habit:{}:best", ht)).and_then(|v| v.parse().ok()).unwrap_or(0);
        let today_v = db.kv_get(&format!("habit:{}:today", ht)).unwrap_or_default();
        let last_date = db.kv_get(&format!("habit:{}:last_date", ht)).unwrap_or_default();
        let today_s = chrono::Local::now().format("%Y-%m-%d").to_string();
        let today_checked = !last_date.is_empty() && last_date == today_s;
        serde_json::json!({
            "streak": streak, "best": best,
            "today": if today_checked { today_v } else { String::new() },
            "active": !last_date.is_empty(),
        })
    };
    serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "en": petd::i18n::t("zh", "en") == "en",
        "coins": app.petd.shared.db.balance(),
        "broken": broken,
        "mode": mode.as_str(),
        "lang": current_language(app),
        "focus": focus,
        "autostart": setup::autostart_on(),
        "boss": boss,
        "focusKey": focus_key,
        "modes": modes,
        "langs": langs,
        "modelReady": model_ready,
        "modelDlDone": dl_done,
        "modelDlTotal": dl_total,
        "habits": { "smoking": habit_data("smoking"), "sugar": habit_data("sugar") },
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
        // 小皮鞭: the user holds the whip; the pet answers to its state.
        ["whip", pet] => {
            let views = app.petd.hub.lock().unwrap().pets();
            let r = sh.world.lock().unwrap().user_whip(pet, &views, petd::now_ms(), &sh.db);
            match r {
                Ok(outs) => send_outs(sh, outs),
                Err(e) => pet_say(h, pet, &e, None),
            }
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
            // The pet lost to you: after the reveal and its sulk, the whip.
            if outcome == "win" {
                let views = app.petd.hub.lock().unwrap().pets();
                let outs = sh.world.lock().unwrap().lost_to_user(pet, &views, 2600, &sh.db);
                send_outs(sh, outs);
            }
            return;
        }
        // 相声: from a pet's ring menu (xs|pet) or the tray (xs: the Claude Code pet, or any).
        ["xs", rest @ ..] => {
            let views = app.petd.hub.lock().unwrap().pets();
            let pet = match rest.first() {
                Some(p) => p.to_string(),
                None => petd::world::World::camp_pet(&views, "claude-code").or_else(|| views.first()).map(|v| v.id.clone()).unwrap_or_default(),
            };
            let r = {
                let mut w = sh.world.lock().unwrap();
                w.reload_xs(&app.petd.paths.home);
                w.request_xs(rest.get(1).copied(), &pet, petd::now_ms(), &views, &sh.db)
            };
            match r {
                Ok(outs) => send_outs(sh, outs),
                Err(e) => pet_say(h, &pet, &e, None),
            }
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
        ["feed", pet] | ["feed", pet, _] => {
            let n: usize = g.get(2).and_then(|s| s.parse().ok()).unwrap_or(1).min(50);
            let views = app.petd.hub.lock().unwrap().pets();
            if let Some(v) = views.iter().find(|v| v.id == *pet).cloned() {
                let mut last_bond_tier = "none";
                let mut last_text = String::new();
                let mut last_count = 0i64;
                let mut batch_fed = 0i64;
                let mut letter_data: Option<(i64, String)> = None;
                for _ in 0..n {
                    let (text, outs, reward, feed_count) = sh.world.lock().unwrap().feed(&v, &views, petd::now_ms(), &sh.db);
                    for o in outs {
                        if let petd::world::Out::Ws(m) = o {
                            let _ = sh.ws_tx.send(m);
                        }
                    }
                    let no_fish = matches!(reward, petd::bond::BondReward::NoFish);
                    let milestone = matches!(reward, petd::bond::BondReward::Letter { .. } | petd::bond::BondReward::Secret(_));
                    if !no_fish {
                        last_text = text;
                        last_count = feed_count;
                        batch_fed += 1;
                    } else {
                        last_text = text; // "鱼篓是空的" message
                    }
                    let (bond_str, ldata) = match reward {
                        petd::bond::BondReward::NoFish => ("none", None),
                        petd::bond::BondReward::Whisper(_) => ("whisper", None),
                        petd::bond::BondReward::Secret(_) => ("secret", None),
                        petd::bond::BondReward::Letter { feed_count: fc, pet_name, .. } => ("letter", Some((fc, pet_name))),
                    };
                    last_bond_tier = bond_str;
                    if ldata.is_some() { letter_data = ldata; }
                    if milestone || no_fish { break; }
                }
                // If batch partially succeeded then ran out of fish, report what was fed
                let report_ok = batch_fed > 0;
                let report_count = last_count;
                let _ = h.emit_to(format!("pet-{pet}").as_str(), "fx", serde_json::json!({
                    "kind": "feed", "ok": report_ok, "text": last_text, "bond": last_bond_tier,
                    "count": report_count, "batch": batch_fed
                }));
                if let Some((fc, pack_id)) = letter_data {
                    // Resolve display name from pack library (pack_id comes from bond::on_feed)
                    let display_name = {
                        let hub = sh.hub.lock().unwrap();
                        hub.lib.packs.get(&pack_id)
                            .map(|p| p.name.clone())
                            .filter(|n| !n.is_empty())
                            .unwrap_or_else(|| pack_id.clone())
                    };
                    let h2 = h.clone();
                    let en = petd::i18n::is_en();
                    let home = app.petd.paths.home.clone();
                    let pet_label = format!("pet-{pet}");
                    // The pet's picture goes on the envelope as the stamp.
                    let stamp = {
                        let info = h.state::<Info>();
                        format!("http://127.0.0.1:{}/v1/packs/{}/files/preview.png?token={}", info.port, pack_id, info.token)
                    };
                    tauri::async_runtime::spawn(async move {
                        let letter = petd::bond::generate_letter(fc, &display_name, en, Some(home)).await;
                        let payload = serde_json::json!({ "text": letter, "count": fc, "pet_name": display_name, "pack": pack_id, "stamp": stamp });
                        *PENDING_LETTER.lock().unwrap() = Some(payload);
                        let _ = h2.emit_to(pet_label.as_str(), "letter-notify", ());
                    });
                }
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
        (Some("radio"), _, _) => windows::open_radio(h),
        (Some("shop"), _, _) => windows::open_shop(h),
        (Some("config"), _, _) => open_path(&app.petd.paths.config()),
        (Some("folder"), _, _) => open_path(&app.petd.paths.home),
        (Some("xsdir"), _, _) => {
            let dir = petd::xiangsheng::dir(&app.petd.paths.home);
            let _ = std::fs::create_dir_all(&dir);
            open_path(&dir)
        }
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
        (Some("read_letter"), _, _) => {
            if PENDING_LETTER.lock().unwrap().is_some() {
                let _ = crate::windows::open_letter(h);
            }
        }
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

/// The first pet window's id, for lines the tray wants a pet to say.
fn first_pet(h: &AppHandle) -> Option<String> {
    let mut labels: Vec<String> = h.webview_windows().keys().filter(|l| l.starts_with("pet-")).cloned().collect();
    labels.sort();
    labels.first().map(|l| l["pet-".len()..].to_string())
}

#[tauri::command]
// Async on purpose: creating a window from a sync command deadlocks on Windows.
async fn habit_craving(h: tauri::AppHandle, habit_type: String) {
    use chrono::Datelike;
    let _ = habit_type;
    let zh = ["先别动。我放段相声，你听着。", "我这儿有段相声，正好。", "来。听完这段再说。"];
    let en = ["Hold still. I'll put on a xiangsheng bit — just listen.", "I've got a bit queued up. Good timing.", "Come on. Hear this one out first."];
    let i = chrono::Local::now().ordinal() as usize % zh.len();
    if let Some(pet) = first_pet(&h) {
        pet_say(&h, &pet, petd::i18n::t(zh[i], en[i]), None);
    }
    windows::open_radio_tab(&h, "xs");
}

#[tauri::command]
fn habit_checkin(h: tauri::AppHandle, app: tauri::State<'_, Arc<App>>, habit_type: String, status: String) -> serde_json::Value {
    let db = &app.petd.shared.db;
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let yesterday = (chrono::Local::now() - chrono::Duration::days(1)).format("%Y-%m-%d").to_string();
    let streak_key = format!("habit:{}:streak", habit_type);
    let best_key = format!("habit:{}:best", habit_type);
    let last_key = format!("habit:{}:last_date", habit_type);
    let today_key = format!("habit:{}:today", habit_type);
    let last_date = db.kv_get(&last_key).unwrap_or_default();
    let streak: i64 = db.kv_get(&streak_key).and_then(|v| v.parse().ok()).unwrap_or(0);
    let best: i64 = db.kv_get(&best_key).and_then(|v| v.parse().ok()).unwrap_or(0);
    let new_streak: i64 = match status.as_str() {
        "relapsed" => 0,
        "clean" | "resisted" => {
            if last_date == today { streak }
            else if last_date == yesterday || streak == 0 { streak + 1 }
            else { 1 }
        }
        _ => streak,
    };
    let new_best = new_streak.max(best);
    db.kv_set(&streak_key, &new_streak.to_string());
    db.kv_set(&best_key, &new_best.to_string());
    db.kv_set(&last_key, &today);
    db.kv_set(&today_key, &status);
    let n = new_streak;
    let pick = |zh: &[&str], en: &[&str]| -> String {
        let i = (n.max(1) as usize - 1) % zh.len();
        petd::i18n::t(zh[i], en[i.min(en.len() - 1)]).to_string()
    };
    let msg: String = match (status.as_str(), n) {
        ("relapsed", _) => pick(
            &["我知道的。今天晚一点再喂我一次吧。", "嗯。明天我还在这儿。", "没什么好说的。我把今天记下了，没记别的。"],
            &["I know. Feed me a little later tonight.", "Mm. I'll still be here tomorrow.", "Nothing to say. I wrote today down, nothing else."],
        ),
        ("resisted", _) => pick(
            &["过去了。它来的时候我也在。", "我知道。刚才那几分钟键盘很安静。", "这种时候我帮不上什么。但我看见了。"],
            &["It passed. I was here when it came.", "I know. The keyboard went quiet for a few minutes there.", "I can't do much at times like that. But I saw."],
        ),
        ("clean", 3) => petd::i18n::t("第三天最难。你过来了。", "Day three is the hard one. You're through it.").into(),
        ("clean", 7) => petd::i18n::t("七天。我数了七次日落，屏幕上的那种。", "Seven days. I counted seven sunsets — the on-screen kind.").into(),
        ("clean", 14) => petd::i18n::t("两个星期。你最近存文件的节奏变了，我说不清是哪里。", "Two weeks. The rhythm of your saves has changed. I can't say where.").into(),
        ("clean", 30) => petd::i18n::t("一个月。我不打算说什么。就是想让你知道我在数。", "A month. I'm not going to say anything. I just want you to know I'm counting.").into(),
        ("clean", 100) => petd::i18n::t("一百天。这个数我留着，不告诉别人。", "A hundred days. I'm keeping this number. Not telling anyone.").into(),
        ("clean", _) => pick(
            &["记下了。", "嗯。今天的键盘声比昨天稳。", "我把今天也叠上去了。", "今天的光线没抖。"],
            &["Noted.", "Mm. The keyboard sounded steadier than yesterday.", "I stacked today on top of the others.", "The light didn't flicker today."],
        ),
        _ => String::new(),
    };
    if !msg.is_empty() {
        if let Some(pet) = first_pet(&h) {
            pet_say(&h, &pet, &msg, None);
        }
    }
    serde_json::json!({ "streak": new_streak, "best": new_best, "today": status, "msg": msg })
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
            tray_info,
            tray_pick,
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
            quiet,
            model_state,
            start_model_download,
            take_letter,
            habit_checkin,
            habit_craving
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

            // A pixel-exact 32 px icon for the tray; the panel opens on either click.
            let tray_icon = tauri::image::Image::new_owned(include_bytes!("../icons/tray.rgba").to_vec(), 32, 32);
            let st_tray = state.clone();
            TrayIconBuilder::with_id("main")
                .icon(tray_icon)
                .tooltip("Familiar")
                .on_tray_icon_event(move |t, ev| {
                    if let tauri::tray::TrayIconEvent::Click { button_state: tauri::tray::MouseButtonState::Up, position, .. } = ev {
                        let h = t.app_handle();
                        tray::open(h, position, tray_state(&st_tray));
                    }
                })
                .build(app)?;
            tray::create(&handle)?;

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
                            Control::Radio => windows::open_radio(&h2),
                            Control::Menu { pet } => {
                                let has_letter = PENDING_LETTER.lock().unwrap().is_some();
                                if let (Some(w), Some(model)) = (h2.get_webview_window(&format!("pet-{pet}")), radial::build(&st, &pet, has_letter)) {
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
