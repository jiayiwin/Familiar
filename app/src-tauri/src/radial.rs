//! The right-click menu: a short card beside the pet (menu.html) whose groups
//! open a second card. This file builds the menu as data; ids are the same
//! strings the tray uses, so picks go through `on_menu`.

use crate::App;
use petd::i18n::t;
use petd::tr;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

#[derive(Serialize, Default)]
pub struct Node {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub label: String,
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checked: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    /// Line icon for a row of the first card: music | sweet | mail | home | moon | more.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub glyph: Option<String>,
    /// A red dot: something is waiting behind this row.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub dot: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<Node>,
}

fn item(id: impl Into<String>, label: impl Into<String>) -> Node {
    Node { id: Some(id.into()), label: label.into(), enabled: true, ..Default::default() }
}

fn check(id: impl Into<String>, label: impl Into<String>, on: bool) -> Node {
    Node { checked: Some(on), ..item(id, label) }
}

fn info(label: impl Into<String>) -> Node {
    Node { label: label.into(), enabled: false, ..Default::default() }
}

fn group(label: &str, children: Vec<Node>) -> Node {
    Node { label: label.into(), enabled: true, children, ..Default::default() }
}

fn with_hint(mut n: Node, hint: impl Into<String>) -> Node {
    n.hint = Some(hint.into());
    n
}

fn with_glyph(mut n: Node, glyph: &str) -> Node {
    n.glyph = Some(glyph.into());
    n
}

/// "2 小时 14 分" / "14 分".
fn span_text(ms: u64) -> String {
    let m = ms / 60_000;
    match (m / 60, m % 60) {
        (0, m) => tr!("{m} 分", "{m}m"),
        (h, 0) => tr!("{h} 小时", "{h}h"),
        (h, m) => tr!("{h} 小时 {m} 分", "{h}h {m}m"),
    }
}

#[derive(Serialize)]
pub struct Model {
    pub pet: String,
    pub title: String,
    /// Which of the five pets this is: the menu wears its skin.
    pub skin: String,
    /// What it is watching and how long the agents worked today.
    pub sub: String,
    pub camp: Option<String>,
    pub items: Vec<Node>,
}

pub fn build(app: &App, pet: &str, has_letter: bool) -> Option<Model> {
    let (view, focus) = {
        let hub = app.petd.hub.lock().unwrap();
        let view = hub.pets().into_iter().find(|v| v.id == pet)?;
        (view, hub.cfg.features.focus_mode)
    };
    let db = &app.petd.shared.db;

    // ---- keep me company: what the pet does for you
    // Its piece, on whatever it plays.
    let played = match petd::cast::by_id(&view.skin).map(|f| f.instrument).unwrap_or("piano") {
        "cello" => t("拉首曲子", "Play the cello"),
        "flute" => t("吹首曲子", "Play the flute"),
        "xylophone" => t("敲首曲子", "Play the xylophone"),
        "harp" => t("拨首曲子", "Play the harp"),
        _ => t("弹首曲子", "Play the piano"),
    };
    let company = vec![
        item(format!("act|{pet}|dance"), t("跳支舞", "Dance")),
        item(format!("act|{pet}|play"), played),
        item(format!("radio|{pet}"), t("放电台", "Radio")),
        item(format!("kegel|{pet}"), t("做一组提肛", "A kegel set")),
        item(format!("note|{pet}"), t("记一件事", "Write something down")),
    ];

    // ---- what else there is about this pet (the pages are in its room; the switches in the tray)
    let mut more = vec![
        group(t("多喂几颗", "Feed more"), vec![item(format!("feed|{pet}|5"), "×5"), item(format!("feed|{pet}|10"), "×10")]),
    ];
    match view.camp.as_deref() {
        Some("codex") => more.push(group(t("重置卡", "Reset card"), reset_card(pet))),
        Some("claude-code") => more.push(group(t("用量", "Usage"), claude_usage())),
        _ => {}
    }
    more.push(item(format!("hide|{pet}"), t("隐藏", "Hide")));

    // ---- the first card
    let fish = db.fish_left();
    let feed = with_hint(item(format!("feed|{pet}|1"), t("喂它", "Feed it")), if fish > 0 { tr!("罐里 {fish} 颗", "{fish} in the jar") } else { t("罐空了 · 问它怎么挣", "Empty · ask it how to earn").to_string() });
    let mut items = vec![with_glyph(group(t("陪我一下", "Keep me company"), company), "music"), with_glyph(feed, "sweet")];
    if has_letter {
        let mut letter = with_glyph(item("read_letter", t("有一封信", "A letter")), "mail");
        letter.dot = true;
        items.push(letter);
    }
    items.extend([
        with_glyph(with_hint(item(format!("room|{pet}"), t("小屋", "Room")), t("今天、图鉴、设置", "Today, gallery, settings")), "home"),
        with_glyph(check("focus", t("安静一会儿", "Quiet for a while"), focus), "moon"),
        with_glyph(group(t("更多", "More"), more), "more"),
    ]);

    // Under the name: the project it watches, and today's agent time.
    let watching = view.sessions.iter().max_by_key(|c| c.updated_at).map(|c| c.project.clone()).filter(|p| !p.is_empty());
    let today = db.report(&petd::db::day_of(petd::now_ms())).work_ms;
    let mut sub = match watching {
        Some(p) => tr!("在看 {p}", "Watching {p}"),
        None => t("现在没活", "Nothing running").to_string(),
    };
    if today >= 60_000 {
        let span = span_text(today);
        sub.push_str(&tr!(" · 今天 {span}", " · {span} today"));
    }
    let skin = view.skin.clone();
    let title = petd::cast::by_id(&skin).map(|f| f.name()).unwrap_or("").to_string();
    Some(Model { pet: pet.into(), title, skin, sub, camp: view.camp.clone(), items })
}

/// Codex pet: current quota and how to spend a banked reset.
fn reset_card(pet: &str) -> Vec<Node> {
    let now_s = petd::now_ms() / 1000;
    let mut out = vec![];
    match petd::codex::current() {
        Some(l) => {
            for w in [&l.primary, &l.secondary].into_iter().flatten() {
                out.push(info(tr!(
                    "{}额度 已用 {:.0}% · {}重置",
                    "{} quota {:.0}% used · resets {}",
                    petd::codex::window_name(w.window_minutes),
                    w.used_percent,
                    petd::codex::until_text(w.resets_at, now_s)
                )));
            }
        }
        None => out.push(info(t(
            "最近 8 天的 Codex 记录里还没有额度信息，用 Codex 跑一轮后就有",
            "No quota info in the last 8 days of Codex logs yet. Run Codex once and it shows up",
        ))),
    }
    out.push(with_hint(item(format!("codexcard|{pet}"), t("使用重置卡…", "Use reset card…")), t("打开 Codex", "Opens Codex")));
    out
}

/// Claude Code's 5-hour and weekly windows, once its status line has reported them.
fn claude_usage() -> Vec<Node> {
    let now_s = petd::now_ms() / 1000;
    match petd::usage::current(now_s) {
        Some(l) => [&l.primary, &l.secondary]
            .into_iter()
            .flatten()
            .map(|w| {
                info(tr!(
                    "{}额度 已用 {:.0}% · {}重置",
                    "{} quota {:.0}% used · resets {}",
                    petd::codex::window_name(w.window_minutes),
                    w.used_percent,
                    petd::codex::until_text(w.resets_at, now_s)
                ))
            })
            .collect(),
        None => vec![
            info(t("还没有用量数据", "No usage numbers yet").to_string()),
            with_hint(item("connect|usage", t("接上用量显示…", "Hook up the usage display…")), t("通过 Claude Code 的状态栏读取", "Read from Claude Code's status line")),
        ],
    }
}

// ---------------------------------------------------------------- window

pub fn create(h: &AppHandle) -> tauri::Result<()> {
    WebviewWindowBuilder::new(h, "menu", WebviewUrl::App("menu.html".into()))
        .title("Rill")
        .transparent(true)
        .decorations(false)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .visible(false)
        .inner_size(400.0, 400.0)
        .additional_browser_args(&crate::windows::browser_args())
        .build()?;
    Ok(())
}

/// Cover the pet's monitor and tell the page where the pet stands.
pub fn open(h: &AppHandle, pet_win: &WebviewWindow, model: Model) -> Result<(), String> {
    let menu = h.get_webview_window("menu").ok_or("no menu window")?;
    open_at(h, &menu, pet_win, serde_json::json!({ "model": model }))
}

/// 记一件事 without the menu first: the line to write in, over `pet_win`'s head (`text`: what to start from).
pub fn open_note(h: &AppHandle, pet_win: &WebviewWindow, model: Model, text: Option<String>) -> Result<(), String> {
    let menu = h.get_webview_window("menu").ok_or("no menu window")?;
    open_at(h, &menu, pet_win, serde_json::json!({ "model": model, "note": { "text": text } }))
}

fn open_at(h: &AppHandle, menu: &WebviewWindow, pet_win: &WebviewWindow, mut payload: serde_json::Value) -> Result<(), String> {
    let sc = pet_win.scale_factor().map_err(|e| e.to_string())?;
    let pos = pet_win.outer_position().map_err(|e| e.to_string())?;
    let size = pet_win.outer_size().map_err(|e| e.to_string())?;
    let (pcx, pcy) = (pos.x as f64 + size.width as f64 / 2.0, pos.y as f64 + size.height as f64 * 0.58);
    let monitor = pet_win
        .available_monitors()
        .unwrap_or_default()
        .into_iter()
        .find(|m| {
            let (p, s) = (m.position(), m.size());
            pcx >= p.x as f64 && pcx < (p.x + s.width as i32) as f64 && pcy >= p.y as f64 && pcy < (p.y + s.height as i32) as f64
        })
        .or_else(|| pet_win.current_monitor().ok().flatten())
        .ok_or("no monitor")?;
    let area = monitor.work_area();
    let msc = monitor.scale_factor();
    let _ = menu.set_position(tauri::PhysicalPosition::new(area.position.x, area.position.y));
    let _ = menu.set_size(tauri::PhysicalSize::new(area.size.width, area.size.height));
    let place = serde_json::json!({
        "top": (pos.y as f64 - area.position.y as f64) / msc,
        "cx": (pcx - area.position.x as f64) / msc,
        "cy": (pcy - area.position.y as f64) / msc,
        "petH": size.height as f64 / sc,
        "w": area.size.width as f64 / msc,
        "h": area.size.height as f64 / msc,
    });
    if let (Some(p), Some(o)) = (payload.as_object_mut(), place.as_object()) {
        p.extend(o.clone());
    }
    let _ = h.emit_to("menu", "menu-open", payload);
    let _ = menu.show();
    let _ = menu.set_focus();
    Ok(())
}

pub fn close(h: &AppHandle) {
    if let Some(m) = h.get_webview_window("menu") {
        let _ = m.hide();
    }
}
