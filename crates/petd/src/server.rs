//! Loopback HTTP + WebSocket API.
//!
//!   POST /v1/event             unified event from an emitter
//!   POST /v1/usage             {rate_limits}: Claude Code usage limits from its status line
//!   POST /v1/approve           {source, session, tool, detail, note}: held until the user answers on the card
//!   POST /v1/approve/answer    {id, behavior: allow | deny | terminal} (from the card)
//!   GET  /v1/approve/pending   requests waiting for the card
//!   POST /v1/cmd               /pet command: show | hide | stats | focus | report …
//!   GET  /v1/settings          what the settings page shows; POST {key, value} changes one
//!   POST /v1/pace              {chatter}: how much the pets say and do on their own
//!   POST /v1/demo              act out the agent states on the pets, nothing recorded
//!   GET  /v1/pets              current pets
//!   GET  /v1/lang              the UI language in use
//!   GET  /v1/ws?token=…        pushes pet.state / pet.* / world.state messages
//!   POST /v1/pets/{id}/hide    hide one pet until "show"
//!   GET  /v1/world             bugs, frozen, focus, sounds
//!   POST /v1/world/squash      {id}: one click on a bug
//!   GET  /v1/stats             fish collection + today's report
//!   GET  /v1/report?day=       daily report (today by default)
//!   GET  /v1/bond              familiarity, trust, vitality, recent moments
//!   GET  /v1/today             your time at the desk, the basket, what the pet noticed today
//!   POST /v1/earn              {why: "kegel", pet}: the set the pet led was done to the end; a fish, if one is due
//!   GET  /v1/memories          the diary across days
//!   POST /v1/stat              {kind: "radio", n}: windows report usage for the daily report
//!
//! Every request needs the token: `X-RillBuddy-Token` header or `?token=`.

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
        .allow_methods([Method::GET, Method::POST])
        .allow_headers([
            header::CONTENT_TYPE,
            header::HeaderName::from_static("x-rillbuddy-token"),
        ]);

    Router::new()
        .route("/v1/event", post(event))
        .route("/v1/cmd", post(cmd))
        .route("/v1/demo", post(demo))
        .route("/v1/usage", post(usage))
        .route("/v1/approve", post(approve))
        .route("/v1/approve/answer", post(approve_answer))
        .route("/v1/approve/pending", get(approve_pending))
        .route("/v1/pace", post(pace))
        .route("/v1/settings", get(get_settings).post(set_setting))
        .route("/v1/pets", get(pets))
        .route("/v1/lang", get(lang))
        .route("/v1/pets/{id}/hide", post(hide_pet))
        .route("/v1/world", get(world_state))
        .route("/v1/world/squash", post(squash))
        .route("/v1/stats", get(stats))
        .route("/v1/report", get(report))
        .route("/v1/song", get(song))
        .route("/v1/bond", get(bond))
        .route("/v1/today", get(today))
        .route("/v1/earn", post(earn))
        .route("/v1/memories", get(memories))
        .route("/v1/stat", post(stat))
        .route("/v1/storeroom", get(storeroom))
        .route("/v1/storeroom/pack", post(storeroom_pack))
        .route("/v1/storeroom/unpack", post(storeroom_unpack))
        .route("/v1/storeroom/park", post(storeroom_park))
        .route("/v1/drawer", get(drawer))
        .route("/v1/night", get(night))
        .route("/v1/todos", get(todos).post(todo_add))
        .route("/v1/todos/done", post(todo_done))
        .route("/v1/night/run", post(night_run))
        .route("/v1/drawer/prepare", post(drawer_prepare))
        .route("/v1/drawer/stow", post(drawer_stow))
        .route("/v1/drawer/unstow", post(drawer_unstow))
        .route("/v1/drawer/recycle", post(drawer_recycle))
        .route("/v1/drawer/reveal", post(drawer_reveal))
        .route("/v1/ws", get(ws))
        .layer(middleware::from_fn_with_state(sh.clone(), auth))
        .layer(cors)
        .with_state(sh)
}

async fn auth(Ax(sh): Ax<Shared>, req: Request, next: Next) -> Response {
    if req.method() == Method::OPTIONS {
        return next.run(req).await;
    }
    // Emitters from before the rename send x-familiar-token.
    let header_ok = ["x-rillbuddy-token", "x-familiar-token"].iter().any(|h| {
        req.headers().get(*h).and_then(|v| v.to_str().ok()) == Some(sh.token.as_str())
    });
    let query_ok = req
        .uri()
        .query()
        .map(|q| {
            q.split('&')
                .any(|kv| kv.strip_prefix("token=") == Some(sh.token.as_str()))
        })
        .unwrap_or(false);
    if header_ok || query_ok {
        next.run(req).await
    } else {
        StatusCode::UNAUTHORIZED.into_response()
    }
}

/// Takes the emitter's event. When it is a permission request the card may
/// answer, the reply says how long the card waits (`approve`, ms), and the
/// emitter comes back with the details.
async fn event(Ax(sh): Ax<Shared>, Json(ev): Json<Event>) -> Response {
    use crate::protocol::EventKind;
    let now = crate::now_ms();
    let (urgent, approve) = {
        let mut hub = sh.hub.lock().unwrap();
        let urgent = hub.apply(&ev, now);
        let approve = (ev.event == EventKind::Attention
            && ev.detail.kind.as_deref() == Some("permission"))
        .then(|| hub.approve_wait(&ev.source))
        .flatten();
        (urgent, approve)
    };
    if urgent {
        sh.urgent.notify_one();
    }
    match approve {
        Some(ms) => Json(serde_json::json!({ "approve": ms })).into_response(),
        None => StatusCode::NO_CONTENT.into_response(),
    }
}

#[derive(Deserialize)]
struct UsageReq {
    #[serde(default)]
    rate_limits: serde_json::Value,
}

/// Claude Code's usage limits, from its status line (the emitter's `--statusline`).
async fn usage(Ax(sh): Ax<Shared>, Json(u): Json<UsageReq>) -> StatusCode {
    let pet = {
        let views = sh.hub.lock().unwrap().pets();
        crate::world::World::camp_pet(&views, "claude-code").map(|v| v.id.clone())
    };
    for m in crate::usage::update(&u.rate_limits, pet.as_deref(), crate::now_ms()) {
        let _ = sh.ws_tx.send(m);
    }
    StatusCode::NO_CONTENT
}

#[derive(Deserialize)]
struct AskReq {
    source: String,
    session: String,
    #[serde(default)]
    tool: String,
    #[serde(default)]
    detail: String,
    #[serde(default)]
    note: String,
}

/// Takes the request off the card's list when the hook stops waiting, however that happens.
struct AskGuard {
    sh: Shared,
    id: u64,
}

impl Drop for AskGuard {
    fn drop(&mut self) {
        self.sh.hub.lock().unwrap().end_ask(self.id);
        let _ = self.sh.ws_tx.send(crate::msg(
            "approve.done",
            &serde_json::json!({ "id": self.id }),
        ));
    }
}

/// A permission request from the emitter, held until the user answers on the
/// pet's card or the wait runs out. `{"behavior": "allow" | "deny"}`, or 204
/// when the terminal should ask as usual.
async fn approve(Ax(sh): Ax<Shared>, Json(a): Json<AskReq>) -> Response {
    let held = {
        let mut hub = sh.hub.lock().unwrap();
        hub.ask(
            &a.source,
            &a.session,
            &a.tool,
            &a.detail,
            &a.note,
            crate::now_ms(),
        )
        .map(|(id, rx, wait)| (id, rx, wait, hub.ask_views(Some(id))))
    };
    let Some((id, rx, wait, views)) = held else {
        return StatusCode::NO_CONTENT.into_response();
    };
    let _guard = AskGuard { sh: sh.clone(), id };
    for v in views {
        let _ = sh.ws_tx.send(crate::msg("approve.ask", &v));
    }
    match tokio::time::timeout(std::time::Duration::from_millis(wait), rx).await {
        Ok(Ok(b @ ("allow" | "deny"))) => {
            Json(serde_json::json!({ "behavior": b })).into_response()
        }
        _ => StatusCode::NO_CONTENT.into_response(),
    }
}

#[derive(Deserialize)]
struct AskAnswer {
    id: u64,
    behavior: String,
}

async fn approve_answer(Ax(sh): Ax<Shared>, Json(a): Json<AskAnswer>) -> Response {
    let ok = sh.hub.lock().unwrap().answer_ask(a.id, &a.behavior);
    reply(if ok {
        Ok(String::new())
    } else {
        Err(crate::tr!(
            "这个请求已经不在等了",
            "That request is no longer waiting"
        ))
    })
}

async fn approve_pending(Ax(sh): Ax<Shared>) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "asks": sh.hub.lock().unwrap().ask_views(None) }))
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
        "tour" => {
            crate::ops::demo(&sh);
            crate::tr!("演一遍给你看：思考、调工具、等你、完成、出错。", "Acting it out for you: thinking, tool, waiting on you, done, error.")
        }
        "focus" => {
            let on = match c.args.first().map(String::as_str) {
                Some("on") => true,
                Some("off") => false,
                _ => !sh.hub.lock().unwrap().cfg.features.focus_mode,
            };
            crate::ops::set_focus(&sh, on).unwrap_or_else(|e| e)
        }
        // Debug: trigger a pet.act directly (act <act> <pet>, e.g. "act dance claude-code").
        "act" => {
            let kind = c.args.first().map(String::as_str).unwrap_or("dance");
            let pet = c.args.get(1).cloned().unwrap_or_else(|| "claude-code".into());
            crate::ops::perform(&sh, &pet, kind)
        }
        // Debug aid: replay a right-click effect (feed | card) on a pet.
        "demo" => {
            let kind = c.args.first().cloned().unwrap_or_else(|| "feed".into());
            let pet = c.args.get(1).cloned().unwrap_or_else(|| "claude-code".into());
            let payload = match kind.as_str() {
                "card" => serde_json::json!({ "kind": "card", "from": 96.0, "used": 0.0 }),
                _ => serde_json::json!({ "kind": "feed", "ok": true, "text": crate::i18n::t("……嗯。", "…mm.") }),
            };
            let _ = sh.ws_tx.send(crate::msg("fx", &serde_json::json!({ "pet": pet, "payload": payload })));
            format!("demo {kind} on {pet}")
        }
        "lang" | "language" => match c.args.first() {
            Some(l) => crate::ops::set_language(&sh, l).unwrap_or_else(|e| e),
            None => crate::tr!("界面语言：{}（可选 auto、zh、en）", "UI language: {} (auto, zh or en)", crate::i18n::code()),
        },
        "size" => match c.args.first() {
            Some(s) => crate::ops::set_size(&sh, s).unwrap_or_else(|e| e),
            None => crate::tr!("当前大小：{}。可选 s、m、l", "Current size: {}. Options: s, m, l", sh.hub.lock().unwrap().cfg.features.size.to_uppercase()),
        },
        // /pet kegel: a set now. (It used to take daily times; the pet now leads a set when you have sat long.)
        "kegel" => match c.args.first() {
            None => crate::ops::kegel_now(&sh, None).unwrap_or_else(|e| e),
            Some(_) => crate::tr!("提肛不再定时：坐久了它会带你做。现在做一组用 /pet kegel。", "Kegels are no longer timed: the pet leads a set when you have sat long. For one now: /pet kegel."),
        },
        "menu" => {
            let pet = c.args.first().cloned().unwrap_or_else(|| "claude-code".into());
            let _ = sh.control.send(crate::Control::Menu { pet: pet.clone() });
            crate::tr!("在 {pet} 上打开了菜单", "Opened the menu on {pet}")
        }
        "radio" => {
            let _ = sh.control.send(crate::Control::Radio);
            crate::tr!("已打开电台。", "Opened the radio.")
        }
        // /pet moment share|bump|chat: play one of the moments between pets now.
        "moment" => {
            static TALKS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            let now = crate::now_ms();
            let obs = crate::think::look(&sh, now);
            let which = c.args.first().map(String::as_str).unwrap_or("chat");
            match crate::think::shown(&obs, which, TALKS.fetch_add(1, std::sync::atomic::Ordering::Relaxed)) {
                Ok(mut plan) => {
                    if crate::think::take_sweet(&sh, &mut plan) {
                        // An offer has no diary line (yet): it is only a question.
                        if !plan.note.is_empty() {
                            sh.db.log_event(now, "noticed", &plan.note, 1);
                        }
                        crate::think::stage(&sh, &plan, now, obs.kegel_reps);
                        crate::tr!("开始了，看桌面。", "Watch the desktop.")
                    } else {
                        crate::tr!("糖罐是空的，没糖可分。", "The jar is empty: nothing to share.")
                    }
                }
                Err(e) => e,
            }
        }
        "report" => {
            let _ = sh.control.send(crate::Control::Report);
            crate::tr!("已打开今日报告。", "Opened today's report.")
        }
        "fling" => {
            let pet = c.args.first().cloned().unwrap_or_else(|| "claude-code".into());
            let vx = c.args.get(1).and_then(|v| v.parse().ok()).unwrap_or(1800.0);
            let vy = c.args.get(2).and_then(|v| v.parse().ok()).unwrap_or(-900.0);
            let _ = sh.control.send(crate::Control::Fling { pet: pet.clone(), vx, vy });
            crate::tr!("把 {pet} 甩了出去。", "Flung {pet}.")
        }
        other => crate::tr!("未知命令 {other}。可用：show、hide、stats、focus、size、lang、kegel、radio、moment、report、tour", "Unknown command {other}. Available: show, hide, stats, focus, size, lang, kegel, radio, moment, report, tour"),
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

fn reply(r: Result<String, String>) -> Response {
    match r {
        Ok(m) => Json(serde_json::json!({ "ok": true, "message": m })).into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "ok": false, "message": e })),
        )
            .into_response(),
    }
}

async fn world_state(Ax(sh): Ax<Shared>) -> Response {
    let m = sh.world.lock().unwrap().state_msg();
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
        Err(e) => (
            StatusCode::CONFLICT,
            Json(serde_json::json!({ "ok": false, "message": e })),
        )
            .into_response(),
    }
}

async fn stats(Ax(sh): Ax<Shared>) -> Json<serde_json::Value> {
    let now = crate::now_ms();
    let counts = sh.db.fish_counts();
    let fish = sh.world.lock().unwrap().t.fish.clone();
    let fish: Vec<serde_json::Value> = fish
        .iter()
        .map(|f| {
            let (n, first) = counts.get(&f.id).copied().unwrap_or((0, 0));
            serde_json::json!({ "id": f.id, "name": f.name, "rarity": f.rarity, "note": f.note, "count": n, "first": first, "shape": f.shape, "colors": f.colors, "from": f.from, "hours": f.hours, "months": f.months })
        })
        .collect();
    Json(serde_json::json!({
        "fish": fish,
        "today": sh.db.report(&crate::db::day_of(now)),
    }))
}

async fn bond(Ax(sh): Ax<Shared>) -> Json<serde_json::Value> {
    let (familiarity, trust, vitality) = sh.db.bond_scores();
    let moments = sh.db.recent_bond_moments(10);
    Json(serde_json::json!({
        "familiarity": familiarity,
        "trust": trust,
        "vitality": vitality,
        "moments": moments,
    }))
}

/// A moment the bond keeps, as a line of the diary.
fn moment_line(m: &crate::db::BondMoment) -> Option<(u64, String)> {
    let text = match m.kind.as_str() {
        "late_night" => crate::tr!("夜里你还在。", "You were still here late at night."),
        "error_burst" => crate::tr!("连着出了几次错。", "Several errors in a row."),
        "stone" => crate::tr!("有一件卡住了很久。", "One job sat stuck for a long time."),
        "radio_on" => match serde_json::from_str::<serde_json::Value>(&m.detail)
            .ok()
            .and_then(|d| d["station"].as_str().map(str::to_string))
        {
            Some(s) => crate::tr!("你开了电台：{s}。", "You put the radio on: {s}."),
            None => crate::tr!("你开了电台。", "You put the radio on."),
        },
        _ => return None,
    };
    Some((m.ts, text))
}

/// The gallery's "what it remembers": the diary across days, and since when this pet has been here.
async fn memories(Ax(sh): Ax<Shared>) -> Json<serde_json::Value> {
    let now = crate::now_ms();
    let arrived = match sh
        .db
        .kv_get("arrived:zimeng")
        .and_then(|v| v.parse::<u64>().ok())
    {
        Some(t) => t,
        None => {
            sh.db.kv_set("arrived:zimeng", &now.to_string());
            now
        }
    };
    let mut items = sh.db.noticed_recent(120);
    items.extend(sh.db.recent_bond_moments(60).iter().filter_map(moment_line));
    items.sort_by(|a, b| b.0.cmp(&a.0));
    let items: Vec<_> = items
        .iter()
        .take(150)
        .map(|(ts, text)| serde_json::json!({ "ts": ts, "text": text }))
        .collect();
    Json(serde_json::json!({ "arrived": arrived, "items": items }))
}

/// The room's "today": your time at the desk, and what the pet noticed.
#[derive(Deserialize)]
struct EarnBody {
    why: String,
    #[serde(default)]
    pet: String,
    /// "catch": how many sweets were caught in the room's game.
    #[serde(default)]
    n: u32,
}

/// A pet window reports that the kegel set it led was done to the end. If a
/// fish is due (a set really began, and today's are not all earned), it goes
/// into the basket and `text` is what that pet says about it.
async fn earn(Ax(sh): Ax<Shared>, Json(b): Json<EarnBody>) -> Json<serde_json::Value> {
    let who = {
        let hub = sh.hub.lock().unwrap();
        crate::cast::of(&hub.cfg, &b.pet).id
    };
    let text = match b.why.as_str() {
        "kegel" => crate::ops::earn_fish(
            &sh,
            "kegel",
            who,
            crate::tr!(
                "你跟着做完了一组提肛，挣到一颗话梅。",
                "You finished a kegel set and earned a salted plum."
            ),
        ),
        // The room's game: enough caught in one round puts one in the jar (a couple a day at most).
        "catch" if b.n >= crate::ops::CATCH_GOAL => {
            let name = crate::cast::by_id(who).map(|f| f.name()).unwrap_or("");
            crate::ops::earn_fish(
                &sh,
                "catch",
                who,
                crate::tr!(
                    "你在小屋里陪{name}接糖，罐里多了一颗。",
                    "You caught sweets with {name} in the room; one went into the jar."
                ),
            )
        }
        _ => None,
    };
    Json(serde_json::json!({ "ok": text.is_some(), "text": text, "fish_left": sh.db.fish_left() }))
}

/// 储物间: the agents' folders counted. Kept for ten minutes; `?fresh=1` counts again.
async fn storeroom(
    Ax(sh): Ax<Shared>,
    axum::extract::Query(q): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Json<serde_json::Value> {
    static LAST: std::sync::Mutex<Option<crate::storeroom::Report>> = std::sync::Mutex::new(None);
    let fresh = q.get("fresh").is_some_and(|v| v == "1");
    if let Some(r) = LAST
        .lock()
        .unwrap()
        .clone()
        .filter(|r| !fresh && crate::now_ms() - r.at < 600_000)
    {
        return Json(serde_json::json!(r));
    }
    let home = sh.paths.home.clone();
    let r = tokio::task::spawn_blocking(move || crate::storeroom::count(&home))
        .await
        .unwrap_or_else(|_| crate::storeroom::Report {
            agents: vec![],
            boxes: vec![],
            parked: vec![],
            at: 0,
        });
    *LAST.lock().unwrap() = Some(r.clone());
    Json(serde_json::json!(r))
}

#[derive(serde::Deserialize)]
struct ParkBody {
    names: Vec<String>,
    #[serde(default)]
    back: bool,
}

/// Set Claude Code skills aside in the storeroom, or put them back.
async fn storeroom_park(Ax(sh): Ax<Shared>, Json(b): Json<ParkBody>) -> Json<serde_json::Value> {
    let home = sh.paths.home.clone();
    let back = b.back;
    let r = tokio::task::spawn_blocking(move || crate::storeroom::park(&home, &b.names, back))
        .await
        .unwrap_or_else(|e| Err(e.to_string()));
    match r {
        Ok(n) => {
            if !back && n > 0 {
                sh.db.log_event(crate::now_ms(), "noticed", &crate::tr!("把 {n} 个一个月没用过的技能收进了储物间，新会话不再先读它们。", "Put {n} skills unused for a month away in the storeroom; new sessions no longer read them first."), 1);
            }
            Json(serde_json::json!({ "ok": true, "moved": n }))
        }
        Err(e) => Json(serde_json::json!({ "ok": false, "message": e })),
    }
}

#[derive(serde::Deserialize)]
struct PackBody {
    #[serde(default)]
    agent: String,
    #[serde(default)]
    file: String,
}

/// Pack an agent's sessions older than a month; progress goes out as `storeroom` messages.
async fn storeroom_pack(Ax(sh): Ax<Shared>, Json(b): Json<PackBody>) -> Json<serde_json::Value> {
    let (home, tx, agent) = (sh.paths.home.clone(), sh.ws_tx.clone(), b.agent.clone());
    let r = tokio::task::spawn_blocking(move || {
        crate::storeroom::pack(&home, &agent, |done, total| {
            let _ = tx.send(crate::msg(
                "storeroom",
                &serde_json::json!({ "agent": agent, "done": done, "total": total }),
            ));
        })
    })
    .await
    .unwrap_or_else(|e| Err(e.to_string()));
    match r {
        Ok(b) => {
            let gone = b.bytes.saturating_sub(b.packed);
            sh.db.log_event(
                crate::now_ms(),
                "noticed",
                &crate::tr!(
                    "收拾了储物间：{} 个旧会话装进了一个箱子，腾出 {} MB。",
                    "Tidied the storeroom: {} old sessions went into one box, {} MB freed.",
                    b.files,
                    gone / 1_048_576
                ),
                1,
            );
            Json(serde_json::json!({ "ok": true, "box": b }))
        }
        Err(e) => Json(serde_json::json!({ "ok": false, "message": e })),
    }
}

/// Put a box's sessions back where they were.
async fn storeroom_unpack(Ax(sh): Ax<Shared>, Json(b): Json<PackBody>) -> Json<serde_json::Value> {
    let home = sh.paths.home.clone();
    let r = tokio::task::spawn_blocking(move || crate::storeroom::unpack(&home, &b.file))
        .await
        .unwrap_or_else(|e| Err(e.to_string()));
    match r {
        Ok(n) => Json(serde_json::json!({ "ok": true, "files": n })),
        Err(e) => Json(serde_json::json!({ "ok": false, "message": e })),
    }
}

#[derive(serde::Deserialize)]
struct DrawerBody {
    #[serde(default)]
    place: String,
    /// stow: only these of the old files; recycle: these of the installers and copies.
    #[serde(default)]
    paths: Option<Vec<String>>,
    #[serde(default)]
    id: String,
    #[serde(default)]
    path: String,
}

fn drawer_place(s: &str) -> Result<(crate::drawer::Place, std::path::PathBuf), String> {
    let place = crate::drawer::Place::parse(s).ok_or("place: desktop | downloads")?;
    let dir = place.dir().filter(|d| d.is_dir()).ok_or("no such folder")?;
    Ok((place, dir))
}

/// 收纳: the desktop or Downloads looked over (`?place=desktop|downloads`).
async fn drawer(
    Ax(sh): Ax<Shared>,
    axum::extract::Query(q): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Json<serde_json::Value> {
    let place = q.get("place").cloned().unwrap_or_else(|| "desktop".into());
    let home = sh.paths.home.clone();
    let r = tokio::task::spawn_blocking(move || {
        let (place, dir) = drawer_place(&place)?;
        Ok::<_, String>(crate::drawer::scan(&home, place, &dir, crate::now_ms()))
    })
    .await
    .unwrap_or_else(|e| Err(e.to_string()));
    match r {
        Ok(r) => Json(serde_json::json!(r)),
        Err(e) => Json(serde_json::json!({ "ok": false, "message": e })),
    }
}

/// The put-away folder, made ahead of the pet's sweep (so its icon is there to fly to).
async fn drawer_prepare(Json(b): Json<DrawerBody>) -> Json<serde_json::Value> {
    match drawer_place(&b.place).and_then(|(p, d)| crate::drawer::prepare(p, &d)) {
        Ok(path) => Json(serde_json::json!({ "ok": true, "path": path.to_string_lossy() })),
        Err(e) => Json(serde_json::json!({ "ok": false, "message": e })),
    }
}

/// Put the old files away (all, or the given ones of them).
async fn drawer_stow(Ax(sh): Ax<Shared>, Json(b): Json<DrawerBody>) -> Json<serde_json::Value> {
    let home = sh.paths.home.clone();
    let r = tokio::task::spawn_blocking(move || {
        let (place, dir) = drawer_place(&b.place)?;
        crate::drawer::stow(&home, place, &dir, b.paths.as_deref(), crate::now_ms()).map(|d| (place, d))
    })
    .await
    .unwrap_or_else(|e| Err(e.to_string()));
    match r {
        Ok((place, d)) => {
            if d.moved > 0 {
                crate::drawer::noted(&sh, place, d.moved);
            }
            Json(serde_json::json!({ "ok": true, "done": d }))
        }
        Err(e) => Json(serde_json::json!({ "ok": false, "message": e })),
    }
}

/// 手上的事: what is open, then what was crossed off lately.
async fn todos(Ax(sh): Ax<Shared>) -> Json<serde_json::Value> {
    let now = crate::now_ms();
    Json(serde_json::json!({ "todos": crate::todo::listed(&sh.db, now), "open": crate::todo::open_count(&sh.db) }))
}

#[derive(Deserialize)]
struct TodoBody {
    #[serde(default)]
    text: String,
    #[serde(default)]
    id: u64,
    /// The pet it was handed to (it carries the slip, and says so).
    #[serde(default)]
    pet: Option<String>,
    #[serde(default)]
    back: bool,
}

/// The pet (id, voice, agent) a thing was handed to; else any pet that is about.
fn todo_pet(sh: &Shared, want: Option<&str>) -> Option<(String, &'static str, Option<String>)> {
    let hub = sh.hub.lock().unwrap();
    let pets = hub.pets();
    let p = want.and_then(|w| pets.iter().find(|v| v.id == w)).or_else(|| pets.first())?;
    Some((p.id.clone(), crate::cast::by_id(&p.skin).map(|f| f.id).unwrap_or("zimeng"), p.camp.clone()))
}

/// Write a line down. It goes to the pet it was handed to, which says so.
async fn todo_add(Ax(sh): Ax<Shared>, Json(b): Json<TodoBody>) -> Json<serde_json::Value> {
    let pet = todo_pet(&sh, b.pet.as_deref());
    let camp = pet.as_ref().and_then(|p| p.2.clone());
    match crate::todo::add(&sh.db, &b.text, "you", None, pet.as_ref().map(|p| p.0.as_str()), camp.as_deref(), crate::now_ms()) {
        Some(t) => {
            let line = pet.as_ref().map(|p| crate::voice::todo(p.1, "noted", "", "")).unwrap_or_default();
            let open = crate::todo::open_count(&sh.db);
            let _ = sh.ws_tx.send(crate::msg("todo.changed", &serde_json::json!({ "added": t, "pet": pet.as_ref().map(|p| &p.0), "line": line, "open": open })));
            Json(serde_json::json!({ "ok": true, "todo": t, "open": open }))
        }
        None => Json(serde_json::json!({ "ok": false, "message": crate::i18n::t("这件已经在手上了。", "That one is already in hand.") })),
    }
}

/// Cross one off (the pet eats the slip), or put it back in hand.
async fn todo_done(Ax(sh): Ax<Shared>, Json(b): Json<TodoBody>) -> Json<serde_json::Value> {
    let Some(t) = crate::todo::done(&sh.db, b.id, b.back, crate::now_ms()) else {
        return Json(serde_json::json!({ "ok": false, "message": crate::i18n::t("找不到这一件了。", "That one is gone.") }));
    };
    let pet = todo_pet(&sh, b.pet.as_deref());
    let line = if b.back { String::new() } else { pet.as_ref().map(|p| crate::voice::todo(p.1, "ate", "", "")).unwrap_or_default() };
    let open = crate::todo::open_count(&sh.db);
    let kind = if b.back { "back" } else { "done" };
    let _ = sh.ws_tx.send(crate::msg("todo.changed", &serde_json::json!({ kind: t, "pet": pet.as_ref().map(|p| &p.0), "line": line, "open": open })));
    Json(serde_json::json!({ "ok": true, "todo": t, "open": open }))
}

/// The last night's work, and the drive: how full, and how long it will last.
async fn night(Ax(sh): Ax<Shared>) -> Json<serde_json::Value> {
    let sh2 = sh.clone();
    let (disk, days_left) = tokio::task::spawn_blocking(move || crate::night::drive(&sh2)).await.unwrap_or((None, None));
    let home = sh.paths.home.clone();
    Json(serde_json::json!({
        "night": crate::night::last(&sh),
        "disk": disk,
        "days_left": days_left,
        "kept": crate::drawer::kept(&home).len(),
        "on": sh.hub.lock().unwrap().cfg.features.night,
    }))
}

/// The night's work right now (as it would run tonight), for trying it out.
async fn night_run(Ax(sh): Ax<Shared>) -> Json<serde_json::Value> {
    let on = sh.hub.lock().unwrap().cfg.features.night;
    let sh2 = sh.clone();
    let n = tokio::task::spawn_blocking(move || crate::night::run(&sh2, on)).await.ok();
    Json(serde_json::json!({ "ok": n.is_some(), "night": n }))
}

/// Put one put-away back where everything was. One the night made: the night keeps its hands off that place for a while.
async fn drawer_unstow(Ax(sh): Ax<Shared>, Json(b): Json<DrawerBody>) -> Json<serde_json::Value> {
    if let Some(s) = crate::night::last(&sh).and_then(|n| n.stowed.into_iter().find(|s| s.id == b.id)) {
        crate::night::hands_off(&sh, &s.place);
    }
    let home = sh.paths.home.clone();
    match tokio::task::spawn_blocking(move || crate::drawer::unstow(&home, &b.id)).await.unwrap_or_else(|e| Err(e.to_string())) {
        Ok((back, stuck)) => Json(serde_json::json!({ "ok": true, "back": back, "stuck": stuck })),
        Err(e) => Json(serde_json::json!({ "ok": false, "message": e })),
    }
}

/// Installers and copies the scan found, to the Recycle Bin.
async fn drawer_recycle(Ax(sh): Ax<Shared>, Json(b): Json<DrawerBody>) -> Json<serde_json::Value> {
    let home = sh.paths.home.clone();
    let r = tokio::task::spawn_blocking(move || {
        let (place, dir) = drawer_place(&b.place)?;
        crate::drawer::recycle(&home, place, &dir, b.paths.as_deref().unwrap_or(&[]), crate::now_ms())
    })
    .await
    .unwrap_or_else(|e| Err(e.to_string()));
    match r {
        Ok(n) => {
            if n > 0 {
                sh.db.log_event(crate::now_ms(), "noticed", &crate::tr!("{n} 个用不上的文件移到了回收站。", "{n} files no longer needed went to the Recycle Bin."), 1);
            }
            Json(serde_json::json!({ "ok": true, "n": n }))
        }
        Err(e) => Json(serde_json::json!({ "ok": false, "message": e })),
    }
}

/// Show one of the files the scan found in Explorer.
async fn drawer_reveal(Ax(sh): Ax<Shared>, Json(b): Json<DrawerBody>) -> Json<serde_json::Value> {
    let home = sh.paths.home.clone();
    let ok = tokio::task::spawn_blocking(move || drawer_place(&b.place).is_ok_and(|(p, d)| crate::drawer::reveal(&home, p, &d, &b.path, crate::now_ms())))
        .await
        .unwrap_or(false);
    Json(serde_json::json!({ "ok": ok }))
}

async fn today(Ax(sh): Ax<Shared>) -> Json<serde_json::Value> {
    let now = crate::now_ms();
    let day = crate::db::day_of(now);
    let (desk_ms, breaks) = sh.db.desk_day(&day);
    let mut items = sh.db.noticed(&day);
    // The moments the bond keeps belong to the same diary.
    items.extend(
        sh.db
            .recent_bond_moments(20)
            .iter()
            .filter(|m| crate::db::day_of(m.ts) == day)
            .filter_map(moment_line),
    );
    items.sort_by(|a, b| b.0.cmp(&a.0));
    let noticed: Vec<_> = items
        .iter()
        .take(40)
        .map(|(ts, text)| serde_json::json!({ "ts": ts, "text": text }))
        .collect();
    Json(serde_json::json!({
        "desk_ms": desk_ms,
        "breaks": breaks,
        "fish_left": sh.db.fish_left(),
        "noticed": noticed,
    }))
}

async fn get_settings(Ax(sh): Ax<Shared>) -> Json<serde_json::Value> {
    Json(crate::ops::settings(&sh))
}

#[derive(Deserialize)]
struct Setting {
    key: String,
    value: serde_json::Value,
}

async fn set_setting(Ax(sh): Ax<Shared>, Json(s): Json<Setting>) -> Response {
    reply(crate::ops::set_setting(&sh, &s.key, &s.value))
}

#[derive(Deserialize)]
struct Pace {
    chatter: String,
}

/// `{chatter}`: how much the pets say and do on their own (the first-run page asks).
async fn pace(Ax(sh): Ax<Shared>, Json(p): Json<Pace>) -> Response {
    reply(crate::ops::set_chatter(&sh, &p.chatter))
}

/// Plays the guided show on the pets (first start, or `/pet tour`).
async fn demo(Ax(sh): Ax<Shared>) -> Json<serde_json::Value> {
    Json(crate::ops::demo(&sh))
}

async fn lang() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "lang": crate::i18n::code() }))
}

async fn report(
    Ax(sh): Ax<Shared>,
    Query(q): Query<HashMap<String, String>>,
) -> Json<crate::db::Report> {
    let day = q
        .get("day")
        .cloned()
        .unwrap_or_else(|| crate::db::day_of(crate::now_ms()));
    Json(sh.db.report(&day))
}

/// 收工曲: a day's turns (`?day=YYYY-MM-DD`, today by default).
async fn song(Ax(sh): Ax<Shared>, Query(q): Query<HashMap<String, String>>) -> Response {
    let day = q.get("day").cloned().unwrap_or_else(|| crate::db::day_of(crate::now_ms()));
    let turns = sh.db.day_turns(&day);
    Json(serde_json::json!({ "day": day, "turns": turns })).into_response()
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
/// n == 0 means "station just started" — not seconds, just a bond moment.
async fn stat(Ax(sh): Ax<Shared>, Json(b): Json<StatBody>) -> Response {
    if b.kind != "radio" || !(0..=3600).contains(&b.n) {
        return reply(Err(crate::tr!("不认识这个统计项", "Unknown stat")));
    }
    let now = crate::now_ms();
    if b.n == 0 && !b.what.is_empty() && b.what != "station" {
        // Station started playing: log as a bond moment so it can surface in letters.
        let detail = serde_json::json!({"station": b.what}).to_string();
        sh.db.log_bond_moment(now, "radio_on", &detail);
        // Also queue a fragment (2–21h delay) so the pet may mention it later.
        let delay_ms: u64 = 7_200_000 + (crate::now_ms() % 68_400_000); // deterministic-ish jitter
        sh.db.queue_fragment(now, now + delay_ms, "radio_on");
    } else if b.n > 0 {
        sh.db.log_event(now, &b.kind, &b.what, b.n);
    }
    reply(Ok(String::new()))
}

async fn ws(Ax(sh): Ax<Shared>, up: WebSocketUpgrade) -> Response {
    up.on_upgrade(move |socket| ws_loop(sh, socket))
}

async fn ws_loop(sh: Shared, mut socket: WebSocket) {
    let mut rx = sh.ws_tx.subscribe();
    let snapshot: Vec<String> = {
        let hub = sh.hub.lock().unwrap();
        let mut v: Vec<String> = hub
            .pets()
            .into_iter()
            .map(|p| crate::msg("pet.state", &p))
            .collect();
        v.push(sh.world.lock().unwrap().state_msg());
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
