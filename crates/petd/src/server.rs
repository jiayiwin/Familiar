//! Loopback HTTP + WebSocket API (design §3, §4).
//!
//!   POST /v1/event             unified event from an emitter
//!   POST /v1/cmd               /pet command: show | hide | stats | switch
//!   GET  /v1/pets              current pets
//!   GET  /v1/ws?token=…        pushes pet.state / app.* messages
//!   POST /v1/pets/{id}/input   window interaction (M3)
//!   POST /v1/pets/{id}/hide    hide one pet until "show"
//!   GET  /v1/config            selection, favorites, mode (gallery)
//!   POST /v1/select            {target, pack}: switch a camp / project / rule
//!   POST /v1/unselect          {target: "project:<p>"}: drop a project override
//!   POST /v1/favorite          {pack, on}
//!   POST /v1/mode              {mode}
//!   GET  /v1/world             bugs, tug / shadow race, wallet, frozen
//!   POST /v1/world/squash      {id}: one click on a bug
//!   GET  /v1/stats             wallet + fish collection
//!   GET  /v1/report?day=       daily report (today by default)
//!   POST /v1/stat              {kind: "radio", n}: windows report usage for the daily report
//!   GET  /v1/progress          achievements, totals, seasons
//!   GET  /v1/shop              catalog, owned, equipped, wallet
//!   POST /v1/shop/buy          {item}
//!   POST /v1/shop/equip        {pack, slot, item | null}
//!   GET  /v1/games             offers (race / bookmaker), affinity, records
//!   POST /v1/bet               {game: race | book, pick, stake}
//!   GET  /v1/packs             installed packs (+ broken ones with reasons)
//!   GET  /v1/packs/{id}        resolved pack manifest
//!   GET  /v1/packs/{id}/files/{*path}  sprite / sound bytes
//!
//! Every request needs the token: `X-Familiar-Token` header or `?token=`.

use crate::hub::Hub;
use crate::paths::Paths;
use crate::protocol::Event;
use crate::Control;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path as UrlPath, Query, Request, State as Ax};
use axum::http::{header, HeaderValue, Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

#[derive(Clone)]
pub struct Shared {
    pub hub: Arc<Mutex<Hub>>,
    pub token: String,
    pub paths: Paths,
    /// JSON messages for windows.
    pub ws_tx: broadcast::Sender<String>,
    /// Commands for the host app (show/hide windows…).
    pub control: broadcast::Sender<Control>,
    /// Wakes the flusher right away (attention must reach the screen fast).
    pub urgent: Arc<tokio::sync::Notify>,
    /// Pack folders (path, builtin), rescanned on change.
    pub roots: Vec<(std::path::PathBuf, bool)>,
    pub world: Arc<Mutex<crate::world::World>>,
    pub db: Arc<crate::db::Db>,
}

pub fn router(sh: Shared) -> Router {
    let cors = tower_http::cors::CorsLayer::new()
        .allow_origin([
            HeaderValue::from_static("tauri://localhost"),
            HeaderValue::from_static("http://tauri.localhost"),
            HeaderValue::from_static("https://tauri.localhost"),
            HeaderValue::from_static("http://localhost:1420"),
        ])
        .allow_methods([Method::GET, Method::POST, Method::DELETE])
        .allow_headers([header::CONTENT_TYPE, header::HeaderName::from_static("x-familiar-token")]);

    Router::new()
        .route("/v1/event", post(event))
        .route("/v1/cmd", post(cmd))
        .route("/v1/pets", get(pets))
        .route("/v1/lang", get(lang))
        .route("/v1/pets/{id}/input", post(input))
        .route("/v1/pets/{id}/hide", post(hide_pet))
        .route("/v1/config", get(get_config))
        .route("/v1/select", post(select))
        .route("/v1/unselect", post(unselect))
        .route("/v1/favorite", post(favorite))
        .route("/v1/mode", post(mode))
        .route("/v1/reminders", get(get_reminders).post(set_reminders))
        .route("/v1/world", get(world_state))
        .route("/v1/world/squash", post(squash))
        .route("/v1/stats", get(stats))
        .route("/v1/report", get(report))
        .route("/v1/stat", post(stat))
        .route("/v1/progress", get(progress_info))
        .route("/v1/shop", get(shop))
        .route("/v1/shop/buy", post(shop_buy))
        .route("/v1/shop/equip", post(shop_equip))
        .route("/v1/games", get(games_info))
        .route("/v1/bet", post(bet))
        .route("/v1/world/toss", post(toss))
        .route("/v1/world/catch", post(catch_ball))
        .route("/v1/spring", get(spring_info))
        .route("/v1/spring/hongbao", post(hongbao))
        .route("/v1/spring/lyrics", get(spring_lyrics))
        .route("/v1/spring/music", get(spring_music).post(save_spring_music).delete(remove_spring_music).layer(axum::extract::DefaultBodyLimit::max(60 << 20)))
        .route("/v1/ws", get(ws))
        .route("/v1/packs", get(packs))
        .route("/v1/packs/{id}", get(pack))
        .route("/v1/packs/{id}/files/{*path}", get(pack_file))
        .layer(middleware::from_fn_with_state(sh.clone(), auth))
        .layer(cors)
        .with_state(sh)
}

async fn auth(Ax(sh): Ax<Shared>, req: Request, next: Next) -> Response {
    if req.method() == Method::OPTIONS {
        return next.run(req).await;
    }
    let header_ok = req.headers().get("x-familiar-token").and_then(|v| v.to_str().ok()) == Some(sh.token.as_str());
    let query_ok = req
        .uri()
        .query()
        .map(|q| q.split('&').any(|kv| kv.strip_prefix("token=") == Some(sh.token.as_str())))
        .unwrap_or(false);
    if header_ok || query_ok {
        next.run(req).await
    } else {
        StatusCode::UNAUTHORIZED.into_response()
    }
}

/// Takes the emitter's event. When a whip is waiting for this session and the
/// hook behind the event can carry context, the reply holds the note for the agent.
async fn event(Ax(sh): Ax<Shared>, Json(ev): Json<Event>) -> Response {
    use crate::protocol::EventKind;
    let now = crate::now_ms();
    let (urgent, nudge) = {
        let mut hub = sh.hub.lock().unwrap();
        let urgent = hub.apply(&ev, now);
        let carries = matches!(ev.event, EventKind::TurnStart | EventKind::ToolStart | EventKind::ToolEnd);
        (urgent, if carries { hub.take_nudge(&ev.source, &ev.session, now) } else { None })
    };
    if urgent {
        sh.urgent.notify_one();
    }
    match nudge {
        Some(text) => Json(serde_json::json!({ "nudge": text })).into_response(),
        None => StatusCode::NO_CONTENT.into_response(),
    }
}

#[derive(Deserialize)]
struct Cmd {
    cmd: String,
    #[serde(default)]
    args: Vec<String>,
}

async fn cmd(Ax(sh): Ax<Shared>, Json(c): Json<Cmd>) -> String {
    match c.cmd.as_str() {
        "show" => {
            crate::ops::show_all(&sh);
            crate::tr!("宠物已显示。", "Pets shown.")
        }
        "hide" => {
            crate::ops::hide_all(&sh);
            crate::tr!("宠物已隐藏，/pet show 可恢复。", "Pets hidden. /pet show brings them back.")
        }
        "stats" | "status" => sh.hub.lock().unwrap().stats_text(),
        "switch" => crate::ops::switch_cmd(&sh, &c.args),
        "mode" => match c.args.first() {
            Some(m) => crate::ops::set_mode(&sh, m).unwrap_or_else(|e| e),
            None => crate::tr!(
                "当前路由模式：{}。可选：per-camp、single、per-rule、per-session",
                "Routing mode: {}. Options: per-camp, single, per-rule, per-session",
                sh.hub.lock().unwrap().cfg.mode.as_str()
            ),
        },
        "duel" => {
            let r = {
                let hub = sh.hub.lock().unwrap();
                sh.world.lock().unwrap().request_duel(&hub, crate::now_ms())
            };
            match r {
                Ok((text, outs)) => {
                    for o in outs {
                        if let crate::world::Out::Ws(m) = o {
                            let _ = sh.ws_tx.send(m);
                        }
                    }
                    text
                }
                Err(e) => e,
            }
        }
        "focus" => {
            let on = match c.args.first().map(String::as_str) {
                Some("on") => true,
                Some("off") => false,
                _ => !sh.hub.lock().unwrap().cfg.features.focus_mode,
            };
            crate::ops::set_focus(&sh, on).unwrap_or_else(|e| e)
        }
        "next" => crate::ops::cycle_favorite(&sh).unwrap_or_else(|e| e),
        // Debug aid: replay a right-click effect (rps | feed | bet) on a pet.
        "demo" => {
            let kind = c.args.first().cloned().unwrap_or_else(|| "rps".into());
            let pet = c.args.get(1).cloned().unwrap_or_else(|| "claude-code".into());
            let payload = match kind.as_str() {
                "feed" => serde_json::json!({ "kind": "feed", "ok": true, "text": crate::i18n::t("……嗯。", "…mm.") }),
                "bet" => serde_json::json!({ "kind": "bet", "ok": true, "text": crate::i18n::t("押了 50 金币", "Bet 50 coins"), "game": "race", "stake": 50, "odds": 1.8 }),
                "card" => serde_json::json!({ "kind": "card", "from": 96.0, "used": 0.0 }),
                _ => serde_json::json!({ "kind": "rps", "user": 2, "pet": 0, "outcome": "win", "coins": 2 }),
            };
            let _ = sh.ws_tx.send(crate::msg("fx", &serde_json::json!({ "pet": pet, "payload": payload })));
            format!("demo {kind} on {pet}")
        }
        "badminton" | "pingpong" | "armwrestle" | "game" => {
            let pet = c.args.first().cloned().unwrap_or_else(|| "claude-code".into());
            let game = (c.cmd != "game").then_some(c.cmd.as_str());
            let views = sh.hub.lock().unwrap().pets();
            let r = sh.world.lock().unwrap().request_duo(game, &pet, crate::now_ms(), &views, &sh.db);
            match r {
                Ok(outs) => {
                    for o in outs {
                        match o {
                            crate::world::Out::Ws(m) => {
                                let _ = sh.ws_tx.send(m);
                            }
                            crate::world::Out::Control(ctl) => {
                                let _ = sh.control.send(ctl);
                            }
                        }
                    }
                    crate::tr!("开打！", "Game on!")
                }
                Err(e) => e,
            }
        }
        "tug" => {
            let r = {
                let hub = sh.hub.lock().unwrap();
                sh.world.lock().unwrap().request_tug(&hub)
            };
            sh.urgent.notify_one();
            r.unwrap_or_else(|e| e)
        }
        "lang" | "language" => match c.args.first() {
            Some(l) => crate::ops::set_language(&sh, l).unwrap_or_else(|e| e),
            None => crate::tr!("界面语言：{}（可选 auto、zh、en）", "UI language: {} (auto, zh or en)", crate::i18n::code()),
        },
        "size" => match c.args.first() {
            Some(s) => crate::ops::set_size(&sh, s).unwrap_or_else(|e| e),
            None => crate::tr!("当前大小：{}。可选 s、m、l", "Current size: {}. Options: s, m, l", sh.hub.lock().unwrap().cfg.features.size.to_uppercase()),
        },
        // /pet kegel: a set now; /pet kegel 10:30 15:30: daily times; /pet kegel off.
        "kegel" => match c.args.first().map(String::as_str) {
            None => crate::ops::kegel_now(&sh, None).unwrap_or_else(|e| e),
            Some(a) => {
                let mut r = crate::ops::reminders(&sh);
                r.kegel = if a == "off" { vec![] } else { c.args.clone() };
                crate::ops::set_reminders(&sh, &r).unwrap_or_else(|e| e)
            }
        },
        "menu" => {
            let pet = c.args.first().cloned().unwrap_or_else(|| "claude-code".into());
            let _ = sh.control.send(crate::Control::Menu { pet: pet.clone() });
            crate::tr!("在 {pet} 上打开了菜单", "Opened the menu on {pet}")
        }
        "spring" => {
            let _ = sh.control.send(crate::Control::Spring);
            crate::tr!("已打开春节倒计时。", "Opened the Spring Festival countdown.")
        }
        "radio" => {
            let _ = sh.control.send(crate::Control::Radio);
            crate::tr!("已打开电台。", "Opened the radio.")
        }
        // /pet xiangsheng [piece id or title]
        "xiangsheng" | "xs" | "crosstalk" | "comedy" | "相声" => {
            let piece = c.args.first().cloned();
            let views = sh.hub.lock().unwrap().pets();
            let pet = crate::world::World::camp_pet(&views, "claude-code").or_else(|| views.first()).map(|v| v.id.clone()).unwrap_or_default();
            let r = {
                let mut w = sh.world.lock().unwrap();
                w.reload_xs(&sh.paths.home);
                w.request_xs(piece.as_deref(), &pet, crate::now_ms(), &views, &sh.db)
            };
            match r {
                Ok(outs) => {
                    for o in outs {
                        match o {
                            crate::world::Out::Ws(m) => {
                                let _ = sh.ws_tx.send(m);
                            }
                            crate::world::Out::Control(ctl) => {
                                let _ = sh.control.send(ctl);
                            }
                        }
                    }
                    crate::tr!("开演！", "Showtime!")
                }
                Err(e) => e,
            }
        }
        "report" => {
            let _ = sh.control.send(crate::Control::Report);
            crate::tr!("已打开今日报告。", "Opened today's report.")
        }
        "wallet" | "coins" => {
            let now = crate::now_ms();
            crate::tr!(
                "金币 {}，今日收入 {}/{}。",
                "{} coins, earned today {}/{}.",
                sh.db.balance(),
                sh.db.income_on(&crate::db::day_of(now)),
                sh.world.lock().unwrap().t.balance.economy.daily_cap
            )
        }
        "fling" => {
            let pet = c.args.first().cloned().unwrap_or_else(|| "claude-code".into());
            let vx = c.args.get(1).and_then(|v| v.parse().ok()).unwrap_or(1800.0);
            let vy = c.args.get(2).and_then(|v| v.parse().ok()).unwrap_or(-900.0);
            let _ = sh.control.send(crate::Control::Fling { pet: pet.clone(), vx, vy });
            crate::tr!("把 {pet} 甩了出去。", "Flung {pet}.")
        }
        other => crate::tr!("未知命令 {other}。可用：show、hide、stats、switch、mode、focus、report、wallet", "Unknown command {other}. Available: show, hide, stats, switch, mode, focus, report, wallet"),
    }
}

async fn pets(Ax(sh): Ax<Shared>) -> Json<serde_json::Value> {
    let hub = sh.hub.lock().unwrap();
    Json(serde_json::json!({ "pets": hub.pets(), "hidden": hub.hidden }))
}

async fn hide_pet(Ax(sh): Ax<Shared>, UrlPath(id): UrlPath<String>) -> StatusCode {
    crate::ops::hide_pet(&sh, &id);
    StatusCode::NO_CONTENT
}

async fn get_config(Ax(sh): Ax<Shared>) -> Json<serde_json::Value> {
    let hub = sh.hub.lock().unwrap();
    let c = &hub.cfg;
    // What each camp actually shows, for the gallery's "in use" hints.
    let in_use = serde_json::json!({
        "claude-code": c.pack_for_source("claude-code"),
        "codex": c.pack_for_source("codex"),
    });
    Json(serde_json::json!({
        "mode": c.mode.as_str(),
        "selection": c.selection,
        "in_use": in_use,
        "rules": c.pets,
        "projects_seen": hub.projects_seen(),
        "error": hub.cfg_error,
        "current_target": hub.current_target(),
    }))
}

#[derive(Deserialize)]
struct Select {
    target: String,
    #[serde(default)]
    pack: String,
}

fn reply(r: Result<String, String>) -> Response {
    match r {
        Ok(m) => Json(serde_json::json!({ "ok": true, "message": m })).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "ok": false, "message": e }))).into_response(),
    }
}

async fn select(Ax(sh): Ax<Shared>, Json(b): Json<Select>) -> Response {
    reply(crate::ops::select(&sh, &b.target, &b.pack))
}

async fn unselect(Ax(sh): Ax<Shared>, Json(b): Json<Select>) -> Response {
    match b.target.strip_prefix("project:") {
        Some(p) => reply(crate::ops::clear_project(&sh, p)),
        None => reply(Err(crate::tr!("只能取消项目覆盖", "Only project overrides can be removed"))),
    }
}

#[derive(Deserialize)]
struct Fav {
    pack: String,
    on: bool,
}

async fn favorite(Ax(sh): Ax<Shared>, Json(b): Json<Fav>) -> Response {
    reply(crate::ops::set_favorite(&sh, &b.pack, b.on).map(|_| String::new()))
}

#[derive(Deserialize)]
struct ModeBody {
    mode: String,
}

async fn get_reminders(Ax(sh): Ax<Shared>) -> Json<crate::ops::Reminders> {
    Json(crate::ops::reminders(&sh))
}

async fn set_reminders(Ax(sh): Ax<Shared>, Json(b): Json<crate::ops::Reminders>) -> Response {
    reply(crate::ops::set_reminders(&sh, &b))
}

async fn mode(Ax(sh): Ax<Shared>, Json(b): Json<ModeBody>) -> Response {
    reply(crate::ops::set_mode(&sh, &b.mode))
}

/// Window interactions the world cares about: `{"kind": "petted"}`.
async fn input(Ax(sh): Ax<Shared>, UrlPath(id): UrlPath<String>, Json(body): Json<serde_json::Value>) -> Json<serde_json::Value> {
    let kind = body.get("kind").and_then(|v| v.as_str()).unwrap_or("");
    if kind == "dizzy" {
        crate::progress::bump(&sh.db, "dizzy");
        return Json(serde_json::json!({ "ok": true }));
    }
    if kind == "petted" {
        let pack = sh.hub.lock().unwrap().pets().into_iter().find(|v| v.id == id).map(|v| v.pack);
        if let Some(pack) = pack {
            let counted = sh.world.lock().unwrap().petted(&pack, crate::now_ms(), &sh.db);
            return Json(serde_json::json!({ "ok": true, "counted": counted, "mood": sh.db.mood(&pack) }));
        }
    }
    Json(serde_json::json!({ "ok": true }))
}

async fn world_state(Ax(sh): Ax<Shared>) -> Response {
    let m = sh.world.lock().unwrap().state_msg(crate::now_ms(), &sh.db);
    let v: serde_json::Value = serde_json::from_str(&m).unwrap_or_default();
    Json(v["data"].clone()).into_response()
}

#[derive(Deserialize)]
struct SquashBody {
    id: u64,
}

async fn squash(Ax(sh): Ax<Shared>, Json(b): Json<SquashBody>) -> Response {
    let now = crate::now_ms();
    let res = {
        let mut hub = sh.hub.lock().unwrap();
        sh.world.lock().unwrap().squash(b.id, now, &mut hub, &sh.db)
    };
    sh.urgent.notify_one();
    match res {
        Ok(r) => {
            if r.killed {
                let _ = sh.ws_tx.send(crate::msg("bug.dead", &r));
            }
            Json(serde_json::json!({ "ok": true, "result": r })).into_response()
        }
        Err(e) => (StatusCode::CONFLICT, Json(serde_json::json!({ "ok": false, "message": e }))).into_response(),
    }
}

async fn stats(Ax(sh): Ax<Shared>) -> Json<serde_json::Value> {
    let now = crate::now_ms();
    let counts = sh.db.fish_counts();
    let (fish, cap) = {
        let w = sh.world.lock().unwrap();
        (w.t.fish.clone(), w.t.balance.economy.daily_cap)
    };
    let fish: Vec<serde_json::Value> = fish
        .iter()
        .map(|f| {
            let (n, first) = counts.get(&f.id).copied().unwrap_or((0, 0));
            serde_json::json!({ "id": f.id, "name": f.name, "rarity": f.rarity, "note": f.note, "count": n, "first": first })
        })
        .collect();
    Json(serde_json::json!({
        "wallet": sh.db.balance(),
        "income_today": sh.db.income_on(&crate::db::day_of(now)),
        "daily_cap": cap,
        "fish": fish,
        "today": sh.db.report(&crate::db::day_of(now)),
    }))
}

async fn progress_info(Ax(sh): Ax<Shared>) -> Json<serde_json::Value> {
    let now = crate::now_ms();
    let week = crate::progress::week_of(now);
    let current = {
        let w = sh.world.lock().unwrap();
        crate::progress::week_scores(&sh.db, &week, |d| w.camp_score(d))
    };
    Json(serde_json::json!({
        "achievements": crate::progress::achievements(&sh.db),
        "totals": crate::progress::totals(&sh.db),
        "seasons": crate::progress::seasons(&sh.db),
        "week": week,
        "current": current.map(|(cc, cx)| serde_json::json!({ "claude-code": cc, "codex": cx })),
        "champion": crate::progress::champion(&sh.db, now),
    }))
}

async fn shop(Ax(sh): Ax<Shared>) -> Json<serde_json::Value> {
    let owned = crate::progress::owned(&sh.db);
    let items: Vec<serde_json::Value> = crate::progress::CATALOG
        .iter()
        .map(|i| serde_json::json!({ "id": i.id, "name": i.label(), "kind": i.kind, "price": i.price, "desc": i.about(), "owned": owned.iter().any(|o| o == i.id) }))
        .collect();
    let equipped: Vec<serde_json::Value> = crate::progress::equipped(&sh.db)
        .into_iter()
        .map(|(pack, slot, item)| serde_json::json!({ "pack": pack, "slot": slot, "item": item }))
        .collect();
    Json(serde_json::json!({ "wallet": sh.db.balance(), "items": items, "equipped": equipped }))
}

#[derive(Deserialize)]
struct BuyBody {
    item: String,
}

async fn shop_buy(Ax(sh): Ax<Shared>, Json(b): Json<BuyBody>) -> Response {
    let r = crate::progress::buy(&sh.db, &b.item, crate::now_ms());
    sh.world.lock().unwrap().refresh_soon();
    sh.urgent.notify_one();
    reply(r)
}

#[derive(Deserialize)]
struct EquipBody {
    #[serde(default)]
    pack: String,
    slot: String,
    item: Option<String>,
}

async fn shop_equip(Ax(sh): Ax<Shared>, Json(b): Json<EquipBody>) -> Response {
    let r = crate::progress::equip(&sh.db, &b.pack, &b.slot, b.item.as_deref()).map(|_| String::new());
    sh.world.lock().unwrap().refresh_soon();
    sh.urgent.notify_one();
    reply(r)
}

async fn games_info(Ax(sh): Ax<Shared>) -> Json<serde_json::Value> {
    let now = crate::now_ms();
    let views = sh.hub.lock().unwrap().pets();
    let pack = |c: &str| views.iter().find(|v| v.id == c).or_else(|| views.iter().find(|v| v.camp.as_deref() == Some(c))).map(|v| v.pack.clone());
    let (cc, cx) = (pack("claude-code"), pack("codex"));
    let affinity = match (&cc, &cx) {
        (Some(a), Some(b)) => crate::games_db::affinity(&sh.db, a, b),
        _ => 0,
    };
    let count = |sql: &str| -> serde_json::Value {
        sh.db
            .with(|c| {
                let mut st = c.prepare(sql)?;
                let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
                Ok(rows.flatten().collect::<std::collections::BTreeMap<_, _>>())
            })
            .map(|m| serde_json::json!(m))
            .unwrap_or_default()
    };
    let offers = sh.world.lock().unwrap().offers(now, &sh.db);
    Json(serde_json::json!({
        "offers": offers,
        "affinity": affinity,
        "packs": { "claude-code": cc, "codex": cx },
        "races": count("SELECT winner, COUNT(*) FROM races GROUP BY winner"),
        "duels": count("SELECT winner, COUNT(*) FROM duels GROUP BY winner"),
        "bets": count("SELECT CASE WHEN payout > stake THEN 'won' WHEN payout = stake THEN 'refund' ELSE 'lost' END, COUNT(*) FROM bets WHERE settled = 1 GROUP BY 1"),
    }))
}

#[derive(Deserialize)]
struct BetBody {
    game: String,
    pick: String,
    stake: i64,
}

async fn bet(Ax(sh): Ax<Shared>, Json(b): Json<BetBody>) -> Response {
    let r = sh.world.lock().unwrap().bet(&b.game, &b.pick, b.stake, crate::now_ms(), &sh.db);
    reply(r)
}

#[derive(Deserialize)]
struct TossBody {
    a: String,
    b: String,
}

/// A pet was dragged onto another one (sent by the host).
async fn toss(Ax(sh): Ax<Shared>, Json(t): Json<TossBody>) -> StatusCode {
    let views = sh.hub.lock().unwrap().pets();
    let outs = sh.world.lock().unwrap().request_toss(&t.a, &t.b, crate::now_ms(), &views);
    for o in outs {
        if let crate::world::Out::Ws(m) = o {
            let _ = sh.ws_tx.send(m);
        }
    }
    StatusCode::NO_CONTENT
}

async fn spring_info(Ax(sh): Ax<Shared>) -> Json<serde_json::Value> {
    Json(crate::spring::info(&sh.db, crate::now_ms(), &sh.paths.home))
}

/// The user's song for the Spring Festival page (`?token=` so <audio> can load it).
async fn spring_music(Ax(sh): Ax<Shared>) -> Response {
    let Some(f) = crate::spring::music_file(&sh.paths.home) else { return StatusCode::NOT_FOUND.into_response() };
    match tokio::fs::read(&f).await {
        Ok(bytes) => ([(header::CONTENT_TYPE, crate::spring::content_type(&f)), (header::CACHE_CONTROL, "no-cache")], bytes).into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

/// Save a song picked on the page: raw bytes, `?name=…&ext=mp3`.
async fn save_spring_music(Ax(sh): Ax<Shared>, Query(q): Query<HashMap<String, String>>, body: axum::body::Bytes) -> Response {
    let name = q.get("name").cloned().unwrap_or_default();
    let ext = q.get("ext").cloned().unwrap_or_default();
    match crate::spring::save_music(&sh.paths.home, &name, &ext, &body) {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "ok": false, "message": e }))).into_response(),
    }
}

async fn spring_lyrics(Ax(sh): Ax<Shared>) -> Response {
    let home = &sh.paths.home;
    let Some(f) = crate::spring::music_file(home).and_then(|s| crate::spring::lyrics_file(home, &s)) else { return StatusCode::NOT_FOUND.into_response() };
    match tokio::fs::read(&f).await {
        Ok(bytes) => ([(header::CONTENT_TYPE, "text/plain; charset=utf-8")], String::from_utf8_lossy(&bytes).into_owned()).into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn remove_spring_music(Ax(sh): Ax<Shared>) -> Json<serde_json::Value> {
    crate::spring::remove_music(&sh.paths.home);
    Json(serde_json::json!({ "ok": true }))
}

#[derive(Deserialize)]
struct HongbaoBody {
    score: u32,
    #[serde(default)]
    grabbed: u32,
}

/// A finished round of 抢红包: pay out, and the main pet cheers on the desktop.
async fn hongbao(Ax(sh): Ax<Shared>, Json(b): Json<HongbaoBody>) -> Response {
    let now = crate::now_ms();
    let cap = sh.world.lock().unwrap().t.balance.economy.daily_cap;
    match crate::spring::payout(&sh.db, now, b.score, cap) {
        Ok(p) => {
            let pet = {
                let hub = sh.hub.lock().unwrap();
                let views = hub.pets();
                crate::world::World::camp_pet(&views, "claude-code").or_else(|| views.first()).map(|v| v.id.clone())
            };
            if let Some(pet) = pet {
                let n = b.grabbed;
                let coins = p.coins;
                let bubble = if coins > 0 {
                    crate::tr!("抢到 {n} 个红包 +{coins} 🧧", "{n} red envelopes! +{coins} 🧧")
                } else {
                    crate::tr!("抢到 {n} 个红包 🧧", "{n} red envelopes! 🧧")
                };
                let _ = sh.ws_tx.send(crate::msg(
                    "pet.script",
                    &serde_json::json!({ "pet": pet, "rule": "spring", "action": "cheer", "bubble": bubble, "ms": 3500, "burst": if coins > 0 { "coins" } else { "confetti" }, "burst_text": if coins > 0 { format!("+{coins}") } else { String::new() } }),
                ));
            }
            Json(serde_json::json!({ "ok": true, "coins": p.coins, "left": p.left })).into_response()
        }
        Err(e) => (StatusCode::CONFLICT, Json(serde_json::json!({ "ok": false, "message": e }))).into_response(),
    }
}

async fn catch_ball(Ax(sh): Ax<Shared>) -> Response {
    let r = sh.world.lock().unwrap().catch_ball(crate::now_ms(), &sh.db);
    match r {
        Ok((coins, outs)) => {
            for o in outs {
                if let crate::world::Out::Ws(m) = o {
                    let _ = sh.ws_tx.send(m);
                }
            }
            Json(serde_json::json!({ "ok": true, "coins": coins })).into_response()
        }
        Err(e) => (StatusCode::CONFLICT, Json(serde_json::json!({ "ok": false, "message": e }))).into_response(),
    }
}

async fn lang() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "lang": crate::i18n::code() }))
}

async fn report(Ax(sh): Ax<Shared>, Query(q): Query<HashMap<String, String>>) -> Json<crate::db::Report> {
    let day = q.get("day").cloned().unwrap_or_else(|| crate::db::day_of(crate::now_ms()));
    let mut r = sh.db.report(&day);
    // Pack names for the pets on duty; days before duty was recorded fall
    // back to whoever stands for the camp now.
    let hub = sh.hub.lock().unwrap();
    let views = hub.pets();
    for d in &mut r.duty {
        if d.pack.is_empty() {
            if let Some(v) = views.iter().find(|v| v.camp.as_deref() == Some(d.camp.as_str())) {
                d.pack = v.pack.clone();
            }
        }
        d.name = hub.lib.get(&d.pack).map(|p| p.name.clone()).unwrap_or_else(|| d.pack.clone());
    }
    r.duty.retain(|d| !d.pack.is_empty());
    Json(r)
}

#[derive(Deserialize)]
struct StatBody {
    kind: String,
    #[serde(default)]
    what: String,
    #[serde(default = "one")]
    n: i64,
}

fn one() -> i64 {
    1
}

/// Usage a window knows and the daemon cannot see: seconds of radio played.
async fn stat(Ax(sh): Ax<Shared>, Json(b): Json<StatBody>) -> Response {
    if b.kind != "radio" || !(0..=3600).contains(&b.n) {
        return reply(Err(crate::tr!("不认识这个统计项", "Unknown stat")));
    }
    sh.db.log_event(crate::now_ms(), &b.kind, &b.what, b.n);
    reply(Ok(String::new()))
}

async fn ws(Ax(sh): Ax<Shared>, up: WebSocketUpgrade) -> Response {
    up.on_upgrade(move |socket| ws_loop(sh, socket))
}

async fn ws_loop(sh: Shared, mut socket: WebSocket) {
    let mut rx = sh.ws_tx.subscribe();
    let snapshot: Vec<String> = {
        let hub = sh.hub.lock().unwrap();
        let mut v: Vec<String> = hub.pets().into_iter().map(|p| crate::msg("pet.state", &p)).collect();
        v.push(sh.world.lock().unwrap().state_msg(crate::now_ms(), &sh.db));
        v
    };
    for m in snapshot {
        if socket.send(Message::Text(m.into())).await.is_err() {
            return;
        }
    }
    loop {
        tokio::select! {
            m = rx.recv() => match m {
                Ok(text) => if socket.send(Message::Text(text.into())).await.is_err() { return },
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    // Fell behind: resend full state.
                    let msgs: Vec<String> = sh.hub.lock().unwrap().pets().into_iter().map(|p| crate::msg("pet.state", &p)).collect();
                    for m in msgs { if socket.send(Message::Text(m.into())).await.is_err() { return } }
                }
                Err(_) => return,
            },
            incoming = socket.recv() => match incoming {
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => return,
                _ => {}
            }
        }
    }
}

async fn packs(Ax(sh): Ax<Shared>) -> Json<serde_json::Value> {
    let hub = sh.hub.lock().unwrap();
    let mut list: Vec<serde_json::Value> = hub
        .lib
        .packs
        .values()
        .map(|p| {
            serde_json::json!({
                "id": p.id, "name": p.name, "version": p.version, "author": p.author,
                "tags": p.tags, "builtin": p.builtin, "description": p.description,
            })
        })
        .collect();
    list.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    Json(serde_json::json!({ "packs": list, "broken": hub.lib.broken, "rev": hub.lib_rev }))
}

async fn pack(Ax(sh): Ax<Shared>, UrlPath(id): UrlPath<String>) -> Response {
    let hub = sh.hub.lock().unwrap();
    match hub.lib.get(&id) {
        Some(p) => Json(p.clone()).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn pack_file(
    Ax(sh): Ax<Shared>,
    UrlPath((id, path)): UrlPath<(String, String)>,
    _q: Query<HashMap<String, String>>,
) -> Response {
    let dir = match sh.hub.lock().unwrap().lib.get(&id) {
        Some(p) => p.dir.clone(),
        None => return StatusCode::NOT_FOUND.into_response(),
    };
    // Stay inside the pack directory.
    if path.split(['/', '\\']).any(|seg| seg == ".." || seg.contains(':')) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let full = dir.join(&path);
    match tokio::fs::read(&full).await {
        Ok(bytes) => {
            let ct = match full.extension().and_then(|e| e.to_str()).unwrap_or("") {
                "png" => "image/png",
                "wav" => "audio/wav",
                "mp3" => "audio/mpeg",
                "ogg" => "audio/ogg",
                "json" => "application/json",
                _ => "application/octet-stream",
            };
            ([(header::CONTENT_TYPE, ct), (header::CACHE_CONTROL, "no-cache")], bytes).into_response()
        }
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}
