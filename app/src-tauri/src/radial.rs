//! The right-click menu: a ring of buttons around the pet (menu.html), with
//! a card for each group. This file builds the menu as data; ids are the
//! same strings the tray uses, so picks go through `on_menu`.

use crate::{current_language, target_name, App, LANGS, MODES};
use petd::i18n::t;
use petd::tr;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

#[derive(Serialize, Default)]
pub struct Node {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checked: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<Node>,
}

fn item(id: impl Into<String>, icon: &str, label: impl Into<String>) -> Node {
    Node { id: Some(id.into()), icon: (!icon.is_empty()).then(|| icon.to_string()), label: label.into(), enabled: true, ..Default::default() }
}

fn check(id: impl Into<String>, label: impl Into<String>, on: bool) -> Node {
    Node { checked: Some(on), ..item(id, "", label) }
}

fn info(label: impl Into<String>) -> Node {
    Node { label: label.into(), enabled: false, ..Default::default() }
}

fn group(icon: &str, label: &str, children: Vec<Node>) -> Node {
    Node { icon: Some(icon.into()), label: label.into(), enabled: true, children, ..Default::default() }
}

fn with_hint(mut n: Node, hint: impl Into<String>) -> Node {
    n.hint = Some(hint.into());
    n
}

#[derive(Serialize)]
pub struct Model {
    pub pet: String,
    pub title: String,
    pub camp: Option<String>,
    pub items: Vec<Node>,
}

pub fn build(app: &App, pet: &str) -> Option<Model> {
    let (view, packs, favs, mode, focus, size, kegel) = {
        let hub = app.petd.hub.lock().unwrap();
        let view = hub.pets().into_iter().find(|v| v.id == pet)?;
        let mut packs: Vec<(String, String)> = hub.lib.packs.values().map(|p| (p.id.clone(), p.name.clone())).collect();
        packs.sort();
        (view, packs, hub.cfg.selection.favorites.clone(), hub.cfg.mode, hub.cfg.features.focus_mode, hub.cfg.features.size.to_lowercase(), hub.cfg.features.kegel.clone())
    };
    let lang = current_language(app);
    let pack_name = |id: &str| packs.iter().find(|p| p.0 == id).map(|p| p.1.clone()).unwrap_or_else(|| id.into());
    let offers = {
        let w = app.petd.shared.world.lock().unwrap();
        w.offers(petd::now_ms(), &app.petd.shared.db)
    };

    // ---- games
    let mut games = vec![
        with_hint(item("spring", "🧧", t("春节倒计时", "Spring Festival")), t("抢红包小游戏", "Red-envelope game")),
        with_hint(item("tug", "🤼", t("拔河", "Tug-of-war")), t("比今天的效率分", "Today's productivity score")),
        item(format!("duo|{pet}|badminton"), "🏸", t("羽毛球", "Badminton")),
        item(format!("duo|{pet}|pingpong"), "🏓", t("乒乓球", "Ping-pong")),
        item(format!("duo|{pet}|armwrestle"), "💪", t("掰手腕", "Arm wrestling")),
        group(
            "✊",
            t("猜拳", "Rock-paper-scissors"),
            [t("✊ 石头", "✊ Rock"), t("✌️ 剪刀", "✌️ Scissors"), t("✋ 布", "✋ Paper")]
                .iter()
                .enumerate()
                .map(|(i, n)| item(format!("rps|{pet}|{i}"), "", *n))
                .collect(),
        ),
    ];
    games.push(match &offers.duel_status {
        Some(status) => group(
            "⚔️",
            t("擂台进行中", "Duel on"),
            vec![
                info(status.clone()),
                info(t("下一轮不出错、先做完的赢，赢家 +15", "First to finish the next turn without errors wins, +15 for the winner")),
                item(format!("duelcancel|{pet}"), "✖️", t("取消擂台", "Cancel duel")),
            ],
        ),
        None => with_hint(item(format!("duel|{pet}"), "⚔️", t("发起擂台", "Start a duel")), t("比下一轮谁先做完", "Who finishes the next turn first")),
    });
    let stakes: Vec<i64> = [10, 50, 100].into_iter().filter(|s| s * 2 <= offers.wallet).collect();
    if let Some((occ, ocx, left)) = offers.race {
        let mut sides = vec![];
        for (camp, name, odds) in [("claude-code", "Claude Code", occ), ("codex", "Codex", ocx)] {
            let kids = stakes.iter().map(|s| item(format!("bet|{pet}|race|{camp}|{s}"), "🪙", tr!("{s} 金币", "{s} coins"))).collect();
            sides.push(with_hint(group("🏁", &tr!("押 {name}", "Bet on {name}"), kids), format!("×{odds:.1}")));
        }
        if stakes.is_empty() {
            sides = vec![info(t("金币不够下注", "Not enough coins to bet"))];
        }
        games.push(with_hint(group("🏁", t("赛跑下注", "Race bets"), sides), tr!("剩 {left} 秒", "{left}s left")));
    }
    if let Some(left) = offers.book {
        let mut picks = vec![];
        for (id, _, odds) in petd::games::BOOK_PICKS {
            let kids = stakes.iter().take(2).map(|s| item(format!("bet|{pet}|book|{id}|{s}"), "🪙", tr!("{s} 金币", "{s} coins"))).collect();
            picks.push(with_hint(group("🎫", petd::games::book_pick_name(id), kids), format!("×{odds}")));
        }
        if stakes.is_empty() {
            picks = vec![info(t("金币不够下注", "Not enough coins to bet"))];
        }
        games.push(with_hint(group("🎲", t("庄家局", "Bookmaker"), picks), tr!("剩 {left} 秒", "{left}s left")));
    }

    // ---- switch
    let mut ordered: Vec<&(String, String)> = packs.iter().filter(|p| favs.contains(&p.0)).collect();
    let fav_count = ordered.len();
    ordered.extend(packs.iter().filter(|p| !favs.contains(&p.0)));
    let switch: Vec<Node> = ordered
        .iter()
        .enumerate()
        .map(|(i, (id, name))| {
            let label = if i < fav_count { format!("★ {name}") } else { name.clone() };
            check(format!("switch|{}|{id}", view.target), label, *id == view.pack)
        })
        .collect();

    // ---- settings
    let mut settings = vec![
        group(
            "🧭",
            t("路由模式", "Routing mode"),
            MODES.iter().map(|(id, zh, en)| check(format!("mode|{id}"), t(zh, en), mode.as_str() == *id)).collect(),
        ),
        group(
            "📏",
            t("大小", "Size"),
            [("s", t("小 S", "Small S")), ("m", t("中 M", "Medium M")), ("l", t("大 L", "Large L"))]
                .iter()
                .map(|(id, n)| check(format!("size|{id}"), *n, size == *id))
                .collect(),
        ),
        Node { checked: Some(focus), ..item("focus", "🎯", t("专注模式", "Focus mode")) },
        group("🌐", t("语言", "Language"), LANGS.iter().map(|(id, zh, en)| check(format!("lang|{id}"), t(zh, en), lang == *id)).collect()),
    ];
    if let Some(pat) = view.target.strip_prefix("project:") {
        settings.push(item(format!("unproj|{pat}"), "↩️", t("恢复用阵营宠物", "Back to the team pet")));
    }
    settings.push(item("config", "📝", t("编辑配置…", "Edit config…")));

    // ---- health
    let health = vec![
        with_hint(item(format!("kegel|{pet}"), "🍑", t("提肛一组", "Kegel set")), t("收紧 3 秒、放松 3 秒", "Squeeze 3s, relax 3s")),
        with_hint(
            item("reminders", "⏰", t("提醒设置…", "Reminder settings…")),
            if kegel.is_empty() { t("还没设提肛时间", "No kegel times yet").to_string() } else { kegel.join(" ") },
        ),
    ];

    let mut items = vec![group("🎮", t("游戏", "Games"), games), item(format!("feed|{pet}"), "🐟", t("投喂", "Feed")), group("🎨", t("换装", "Outfits"), switch), group("🍑", t("健康", "Health"), health)];
    if view.camp.as_deref() == Some("codex") {
        items.push(group("🎫", t("重置卡", "Reset card"), reset_card(pet)));
    }
    items.extend([
        group("⚙️", t("设置", "Settings"), settings),
        item("gallery", "📖", t("图鉴", "Gallery")),
        item("report", "📊", t("报告", "Report")),
        item(format!("hide|{pet}"), "🙈", t("隐藏", "Hide")),
    ]);
    Some(Model { pet: pet.into(), title: format!("{} · {}", target_name(&view.target), pack_name(&view.pack)), camp: view.camp.clone(), items })
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
    out.push(with_hint(item(format!("codexcard|{pet}"), "🎫", t("使用重置卡…", "Use reset card…")), t("打开 Codex", "Opens Codex")));
    out.push(item(format!("codexdemo|{pet}"), "✨", t("看看重置动画", "Preview the reset animation")));
    out
}

// ---------------------------------------------------------------- window

pub fn create(h: &AppHandle) -> tauri::Result<()> {
    WebviewWindowBuilder::new(h, "menu", WebviewUrl::App("menu.html".into()))
        .title("Familiar")
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

/// Cover the pet's monitor and show the ring centred on the pet.
pub fn open(h: &AppHandle, pet_win: &WebviewWindow, model: Model) -> Result<(), String> {
    let menu = h.get_webview_window("menu").ok_or("no menu window")?;
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
    let payload = serde_json::json!({
        "model": model,
        "cx": (pcx - area.position.x as f64) / msc,
        "cy": (pcy - area.position.y as f64) / msc,
        "petH": size.height as f64 / sc,
        "w": area.size.width as f64 / msc,
        "h": area.size.height as f64 / msc,
    });
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
