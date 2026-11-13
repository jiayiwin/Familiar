//! 手上的事: the few things you asked the pet to hold for you.
//!
//! A thing comes in three ways: you write a line (the menu's 「记一件事」, the
//! card, or a hotkey on selected text); an agent leaves its "next step" when
//! its session ends; or you write "等 Claude 做完…", and it waits on that agent.
//! It is brought up again only at natural moments: when the agent it waits on
//! stops and you are at the desk, and the next morning for what is left.
//!
//! Kept in the daemon's kv as one small list; nothing leaves the machine.

use crate::db::Db;
use serde::{Deserialize, Serialize};

const KEY: &str = "todos";
const DAY_MS: u64 = 86_400_000;
/// An agent leaves at most this many things open at once; past that its new ones are dropped.
const AGENT_OPEN_MAX: usize = 4;
/// Crossed-off things stay this long (for "put it back"), then go.
const KEEP_DONE_MS: u64 = 3 * DAY_MS;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Todo {
    pub id: u64,
    pub text: String,
    pub at: u64,
    /// "you", or the agent that left it ("claude-code", "codex").
    pub from: String,
    /// The project the agent was in (agent-left things only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    /// The agent it waits on ("claude-code", "codex"): brought up when that agent stops.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wait: Option<String>,
    /// The agent it waits on has stopped since.
    #[serde(default)]
    pub ready: bool,
    /// Brought up already (for `wait`: once, when it got ready).
    #[serde(default)]
    pub told: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub done_at: Option<u64>,
    /// The pet that holds it (the one it was handed to, or the agent's own).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pet: Option<String>,
}

impl Todo {
    pub fn open(&self) -> bool {
        self.done_at.is_none()
    }
}

pub fn all(db: &Db) -> Vec<Todo> {
    db.kv_get(KEY).and_then(|v| serde_json::from_str(&v).ok()).unwrap_or_default()
}

fn save(db: &Db, list: &[Todo]) {
    db.kv_set(KEY, &serde_json::to_string(list).unwrap_or_default());
}

/// Open things, oldest first; then what was crossed off lately, newest first.
pub fn listed(db: &Db, now: u64) -> Vec<Todo> {
    let mut list = all(db);
    let before = list.len();
    list.retain(|t| t.done_at.is_none_or(|d| now.saturating_sub(d) < KEEP_DONE_MS));
    if list.len() != before {
        save(db, &list);
    }
    let (mut open, mut done): (Vec<Todo>, Vec<Todo>) = list.into_iter().partition(Todo::open);
    open.sort_by_key(|t| t.at);
    done.sort_by_key(|t| std::cmp::Reverse(t.done_at));
    open.extend(done);
    open
}

pub fn open_count(db: &Db) -> usize {
    all(db).iter().filter(|t| t.open()).count()
}

/// The agent a line waits on, if it says so: 「等 Claude 做完…」, 「Codex 跑完提醒我…」,
/// "when Claude is done…". 「它」/"it" is the agent of the pet you wrote it to (`mine`).
pub fn wait_of(text: &str, mine: Option<&str>) -> Option<String> {
    let low = text.to_lowercase();
    let zh_done = ["做完", "跑完", "弄完", "写完", "改完", "结束", "停下", "忙完", "完了"];
    let en_done = ["is done", "'s done", "finishes", "finished", "is finished", "stops", "is through"];
    let waits = (text.contains('等') || low.contains("when ") || low.contains("after ") || text.contains("提醒"))
        && (zh_done.iter().any(|w| text.contains(w)) || en_done.iter().any(|w| low.contains(w)));
    if !waits {
        return None;
    }
    if low.contains("claude") {
        Some("claude-code".into())
    } else if low.contains("codex") {
        Some("codex".into())
    } else if text.contains('它') || low.contains("it ") || low.contains("it's") {
        mine.map(String::from)
    } else {
        None
    }
}

/// Write one down. `pet_camp`: the agent of the pet it was written to (for 「等它做完」).
pub fn add(db: &Db, text: &str, from: &str, project: Option<&str>, pet: Option<&str>, pet_camp: Option<&str>, now: u64) -> Option<Todo> {
    let text: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let text: String = text.chars().take(120).collect();
    if text.is_empty() {
        return None;
    }
    let mut list = all(db);
    // The same line still open: it is already in hand.
    if list.iter().any(|t| t.open() && t.text == text) {
        return None;
    }
    if from != "you" && list.iter().filter(|t| t.open() && t.from == from).count() >= AGENT_OPEN_MAX {
        return None;
    }
    let id = list.iter().map(|t| t.id).max().unwrap_or(0).max(now) + 1;
    let wait = if from == "you" { wait_of(&text, pet_camp) } else { None };
    let t = Todo { id, text, at: now, from: from.into(), project: project.map(String::from), wait, ready: false, told: false, done_at: None, pet: pet.map(String::from) };
    list.push(t.clone());
    save(db, &list);
    Some(t)
}

/// Cross one off (or, with `back`, put it back in hand). The thing, as it is now.
pub fn done(db: &Db, id: u64, back: bool, now: u64) -> Option<Todo> {
    let mut list = all(db);
    let t = list.iter_mut().find(|t| t.id == id)?;
    t.done_at = if back { None } else { Some(now) };
    let out = t.clone();
    save(db, &list);
    Some(out)
}

/// An agent stopped (a turn ended): what waited on it is ready to be brought up.
pub fn agent_stopped(db: &Db, source: &str) -> bool {
    let mut list = all(db);
    let mut any = false;
    for t in list.iter_mut().filter(|t| t.open() && !t.told && t.wait.as_deref() == Some(source)) {
        t.ready = true;
        any = true;
    }
    if any {
        save(db, &list);
    }
    any
}

/// The first thing ready to be brought up; marked told.
pub fn take_ready(db: &Db) -> Option<Todo> {
    let mut list = all(db);
    let t = list.iter_mut().find(|t| t.open() && t.ready && !t.told)?;
    t.told = true;
    let out = t.clone();
    save(db, &list);
    Some(out)
}

/// Things still open that were written before this morning (5:00).
pub fn carried(db: &Db, now: u64) -> usize {
    let start = morning_of(now);
    all(db).iter().filter(|t| t.open() && t.at < start).count()
}

fn morning_of(now: u64) -> u64 {
    use chrono::{Local, TimeZone, Timelike};
    let Some(t) = Local.timestamp_millis_opt(now as i64).single() else { return now };
    let day = if t.hour() < 5 { t - chrono::Duration::days(1) } else { t };
    day.with_hour(5).and_then(|d| d.with_minute(0)).and_then(|d| d.with_second(0)).map(|d| d.timestamp_millis() as u64).unwrap_or(now)
}

/// Looked at every think tick, with you at the desk and active (and not in
/// focus mode): a thing that waited on an agent that has now stopped is brought
/// up by that agent's pet; in the morning, what is left from yesterday is
/// brought up once (unless the night's line already said so).
pub fn tick(sh: &crate::server::Shared, hour: u8, active: bool) {
    if !active {
        return;
    }
    let db = &sh.db;
    if let Some(t) = take_ready(db) {
        let agent = t.wait.clone().unwrap_or_default();
        let Some((pet, who)) = pet_of(sh, &agent) else { return };
        let text = crate::voice::todo(who, "wait", agent_name(&agent), &t.text);
        let _ = sh.ws_tx.send(crate::msg("pet.act", &serde_json::json!({ "pet": pet, "act": "offer", "text": text, "why": "todo", "offer": "go" })));
        return;
    }
    let now = crate::now_ms();
    let day = format!("todo:carried:{}", morning_of(now));
    if (6..12).contains(&hour) && db.kv_get(&day).is_none() && carried(db, now) > 0 {
        // The night's line goes first, and carries yesterday's notes in it.
        if crate::night::last(sh).is_some_and(|n| !n.told) {
            return;
        }
        let Some((pet, who)) = crate::drawer::asker(sh) else { return };
        db.kv_set(&day, "1");
        let text = crate::voice::todo(who, "carry", "", "");
        let _ = sh.ws_tx.send(crate::msg("pet.act", &serde_json::json!({ "pet": pet, "act": "offer", "text": text, "why": "todo", "offer": "card" })));
    }
}

/// The night's morning line has said yesterday's notes too: not again today.
pub fn carried_said(db: &Db, now: u64) {
    db.kv_set(&format!("todo:carried:{}", morning_of(now)), "1");
}

/// The pet that keeps company to `agent`; else any pet free to speak.
fn pet_of(sh: &crate::server::Shared, agent: &str) -> Option<(String, &'static str)> {
    {
        let hub = sh.hub.lock().unwrap();
        if hub.cfg.features.focus_mode {
            return None;
        }
        let pets = hub.pets();
        if let Some(p) = pets.iter().find(|v| v.camp.as_deref() == Some(agent) && !matches!(v.state, crate::session::State::Offline)) {
            return Some((p.id.clone(), crate::cast::by_id(&p.skin).map(|f| f.id).unwrap_or("zimeng")));
        }
    }
    crate::drawer::asker(sh)
}

/// The name an agent goes by in a line.
pub fn agent_name(source: &str) -> &'static str {
    match source {
        "claude-code" => "Claude",
        "codex" => "Codex",
        "gemini" => "Gemini",
        "cursor" => "Cursor",
        _ => "agent",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> (tempfile::TempDir, Db) {
        let d = tempfile::tempdir().unwrap();
        let db = Db::open(&d.path().join("t.db")).unwrap();
        (d, db)
    }

    #[test]
    fn a_line_that_says_wait_waits_on_that_agent() {
        assert_eq!(wait_of("等 Claude 做完提醒我看 PR", None).as_deref(), Some("claude-code"));
        assert_eq!(wait_of("Codex 跑完提醒我合并", None).as_deref(), Some("codex"));
        assert_eq!(wait_of("等它做完看一下", Some("codex")).as_deref(), Some("codex"));
        assert_eq!(wait_of("remind me when Claude is done", None).as_deref(), Some("claude-code"));
        assert_eq!(wait_of("周五前把发票寄出去", Some("codex")), None);
        assert_eq!(wait_of("看 Claude 写的文档", None), None);
    }

    #[test]
    fn things_are_held_crossed_off_and_brought_up_once() {
        let (_d, db) = db();
        let t0 = 1_700_000_000_000;
        let a = add(&db, "  周五前 把发票寄出去 ", "you", None, None, None, t0).unwrap();
        assert_eq!(a.text, "周五前 把发票寄出去");
        assert!(add(&db, "周五前 把发票寄出去", "you", None, None, None, t0 + 1).is_none(), "the same line is already in hand");
        let w = add(&db, "等 Claude 做完看 PR", "you", None, None, None, t0 + 2).unwrap();
        assert_eq!(w.wait.as_deref(), Some("claude-code"));
        assert_eq!(open_count(&db), 2);
        // Codex stopping does not bring it up; Claude stopping does, once.
        assert!(!agent_stopped(&db, "codex"));
        assert!(take_ready(&db).is_none());
        assert!(agent_stopped(&db, "claude-code"));
        assert_eq!(take_ready(&db).map(|t| t.id), Some(w.id));
        assert!(take_ready(&db).is_none());
        assert!(!agent_stopped(&db, "claude-code"), "told once is enough");
        // Crossed off, then put back.
        done(&db, a.id, false, t0 + 10).unwrap();
        assert_eq!(open_count(&db), 1);
        assert_eq!(listed(&db, t0 + 11).last().map(|t| t.id), Some(a.id));
        done(&db, a.id, true, t0 + 12).unwrap();
        assert_eq!(open_count(&db), 2);
        // Crossed-off things go after a few days.
        done(&db, a.id, false, t0 + 20).unwrap();
        assert_eq!(listed(&db, t0 + 20 + KEEP_DONE_MS + 1).len(), 1);
    }

    #[test]
    fn an_agent_leaves_only_a_few() {
        let (_d, db) = db();
        for i in 0..6 {
            add(&db, &format!("补测试 {i}"), "claude-code", Some("api"), None, None, 1_000 + i);
        }
        assert_eq!(open_count(&db), AGENT_OPEN_MAX);
        assert!(add(&db, "我自己的", "you", None, None, None, 2_000).is_some());
    }
}
