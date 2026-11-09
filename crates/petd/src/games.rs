//! M5 games (design §7.3–§7.5) on top of the world layer: race betting, the
//! bookmaker, duels, face slaps, job envy, fish stealing, rock-paper-scissors,
//! feeding, affinity, evolution, and the opponent's absence / return.
//!
//! The same hard rules apply as everywhere: nothing starts while a session
//! waits on the user or in focus mode, and pet-initiated scenes count toward
//! the hourly cap. Bets and duels only settle real results.

use crate::db::{day_of, Db};
use crate::games_db as gdb;
use crate::hub::{Feed, Hub, Key, PetView};
use crate::protocol::{EventKind, TurnResult};
use crate::session::State;
use crate::world::{Out, Script, World};
use crate::Control;
use serde::Serialize;
use serde_json::json;
use std::collections::{HashMap, HashSet};

const RACE_WINDOW: u64 = 120_000;
const BET_WINDOW: u64 = 20_000;
const GAME_TTL: u64 = 30 * 60_000;
const DUEL_TIE_MS: u64 = 5_000;
const STEAL_GRACE: u64 = 15_000;
const DAY: u64 = 86_400_000;

/// Bookmaker picks and their odds (design §7.4).
pub const BOOK_PICKS: &[(&str, &str, f64)] = &[
    ("lt1", "不到 1 分钟", 2.5),
    ("1to5", "1–5 分钟", 2.0),
    ("gt5", "超过 5 分钟", 3.0),
    ("error", "会出错", 2.5),
    ("confirm", "要你确认", 2.0),
];

/// Display name of a bookmaker pick in the current language ("" if unknown).
pub fn book_pick_name(id: &str) -> &'static str {
    match id {
        "lt1" => crate::i18n::t("不到 1 分钟", "Under 1 min"),
        "1to5" => crate::i18n::t("1–5 分钟", "1–5 min"),
        "gt5" => crate::i18n::t("超过 5 分钟", "Over 5 min"),
        "error" => crate::i18n::t("会出错", "Hits an error"),
        "confirm" => crate::i18n::t("要你确认", "Needs your OK"),
        _ => BOOK_PICKS.iter().find(|(p, _, _)| *p == id).map(|(_, n, _)| *n).unwrap_or(""),
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Race {
    pub a: Key,
    pub b: Key,
    pub start: u64,
    pub until: u64,
    pub odds: (f64, f64),
}

#[derive(Debug, Clone, Serialize)]
pub struct Book {
    pub key: Key,
    pub start: u64,
    pub until: u64,
}

#[derive(Debug, Clone, Default)]
pub struct Duel {
    pub a: Key,
    pub b: Key,
    pub project: String,
    pub armed: u64,
    pub started: [Option<u64>; 2],
    /// (finished at, clean?)
    pub done: [Option<(u64, bool)>; 2],
    /// The pet showing each side (for the on-screen badges).
    pub pets: [String; 2],
}

#[derive(Debug, Clone)]
struct Taunt {
    target_camp: String,
    taunter: String,
    until: u64,
}

#[derive(Debug, Clone)]
struct Steal {
    row: i64,
    thief: String,
    victim_camp: String,
    until: u64,
}

#[derive(Default)]
pub struct Games {
    last_start: HashMap<String, (Key, u64)>,
    raced: HashSet<Key>,
    pub race: Option<Race>,
    race_steps: [u32; 2],
    pub book: Option<Book>,
    pub duel: Option<Duel>,
    taunts: Vec<Taunt>,
    steal: Option<Steal>,
    last_minute: u64,
    seen: HashMap<String, u64>,
    seen_saved: u64,
    said: HashMap<String, String>,
    book_hint_at: u64,
    slept_together: String,
    cooldown: HashMap<String, u64>,
    /// pack -> evolution thresholds (from its manifest), refreshed every 5 s.
    pub evo: HashMap<String, Vec<i64>>,
}

/// What the right-click menu may offer right now.
#[derive(Debug, Clone, Serialize)]
pub struct Offers {
    /// Race open for bets: (odds CC, odds CX, seconds left).
    pub race: Option<(f64, f64, u64)>,
    /// Bookmaker open: seconds left.
    pub book: Option<u64>,
    pub duel_running: bool,
    /// Running duel, one line for the menu.
    pub duel_status: Option<String>,
    pub wallet: i64,
}

/// 1 if a beats b, -1 if b beats a, 0 tie (0 rock, 1 scissors, 2 paper).
fn rps(a: usize, b: usize) -> i32 {
    match (a, b) {
        _ if a == b => 0,
        (0, 1) | (1, 2) | (2, 0) => 1,
        _ => -1,
    }
}

impl World {
    fn camp_name(c: &str) -> &'static str {
        match c {
            "claude-code" => "Claude Code",
            "codex" => "Codex",
            _ => crate::i18n::t("对手", "the other side"),
        }
    }

    fn cool(&mut self, key: &str, now: u64, ms: u64) -> bool {
        if self.games.cooldown.get(key).is_some_and(|t| now < *t) {
            return false;
        }
        self.games.cooldown.insert(key.to_string(), now + ms);
        true
    }

    /// A bubble (no action) on a pet, optionally after a delay.
    fn say(pet: &str, text: &str, ms: u64, delay: u64, out: &mut Vec<Out>) {
        out.push(Out::Ws(crate::msg("pet.script", &json!({ "pet": pet, "rule": "game", "bubble": text, "ms": ms, "delay": delay }))));
    }

    /// An action + bubble, marking the pet busy for its duration.
    #[allow(clippy::too_many_arguments)]
    fn act(&mut self, pet: &str, rule: &str, action: &str, text: &str, ms: u64, delay: u64, moved: bool, now: u64, out: &mut Vec<Out>) {
        self.scripts.insert(pet.to_string(), Script { rule: rule.into(), until: now + delay + ms, during_attention: false, moved });
        out.push(Out::Ws(crate::msg(
            "pet.script",
            &json!({ "pet": pet, "rule": rule, "action": action, "bubble": text, "ms": ms, "delay": delay }),
        )));
    }

    fn idle_for(v: &PetView, now: u64, ms: u64) -> bool {
        matches!(v.state, State::Idle | State::Done) && now.saturating_sub(v.since) >= ms
    }

    fn working(v: &PetView) -> bool {
        matches!(v.state, State::Thinking | State::Tool | State::Reading)
    }

    /// Raise the affinity of the two camp packs and announce new levels.
    pub(crate) fn bond(&mut self, db: &Db, views: &[PetView], delta: i64, out: &mut Vec<Out>) {
        let (Some(cc), Some(cx)) = (Self::camp_pet(views, "claude-code"), Self::camp_pet(views, "codex")) else { return };
        for at in gdb::add_affinity(db, &cc.pack, &cx.pack, delta) {
            let text = match at {
                20 => crate::i18n::t("我们好像熟了一点。", "I think we're getting used to each other."),
                50 => crate::i18n::t("困了可以靠着了。", "Tired? You can lean on me now."),
                _ => crate::i18n::t("八十了。我们拍了张照，在今天的报告里。", "Eighty. We took a photo — it's in today's report."),
            };
            Self::say(&cc.id, text, 5000, 0, out);
            Self::say(&cx.id, text, 5000, 0, out);
        }
    }

    pub(crate) fn on_taunt(&mut self, target_camp: &str, taunter: &str, now: u64) {
        self.games.taunts.push(Taunt { target_camp: target_camp.into(), taunter: taunter.into(), until: now + 10 * 60_000 });
    }

    /// Called from `tick` with this tick's feed (before it is consumed).
    pub(crate) fn games_tick(&mut self, now: u64, feed: &[Feed], views: &[PetView], hub: &Hub, db: &Db, out: &mut Vec<Out>) {
        let quiet = self.frozen || self.focus;
        for f in feed {
            match f {
                Feed::Event { key, ev } if !ev.sub => self.on_event(key, ev.event, now, views, db, quiet, out),
                Feed::Finished { key, fin, .. } => {
                    self.on_finished(key, fin.result, fin.tool_errors, fin.attentions, fin.ended_at.saturating_sub(fin.started_at), now, views, db, out)
                }
                _ => {}
            }
        }
        self.game_timers(now, views, db, out);
        self.toss_timer(now, views, db, out);
        if now.saturating_sub(self.games.last_minute) >= 60_000 {
            self.games.last_minute = now;
            if !quiet {
                self.try_steal(now, views, db, out);
                self.try_rps(now, views, db, out);
                self.try_duo(now, views, db, out);
                self.try_xs(now, views, db, out);
                self.sleep_together(now, views, db, out);
            }
            self.absence(now, views, db, out);
        }
        let _ = hub;
    }

    #[allow(clippy::too_many_arguments)]
    fn on_event(&mut self, key: &Key, kind: EventKind, now: u64, views: &[PetView], db: &Db, quiet: bool, out: &mut Vec<Out>) {
        let camp = key.0.clone();
        let other = Self::other_camp(&camp);
        // The opponent is back after a long absence.
        let last = self.games.seen.get(&camp).copied().or_else(|| db.kv_get(&format!("seen_{camp}")).and_then(|v| v.parse().ok()));
        self.games.seen.insert(camp.clone(), now);
        if let (Some(last), Some(o)) = (last, other) {
            if now.saturating_sub(last) >= DAY && !quiet {
                if let Some(op) = Self::camp_pet(views, o) {
                    Self::say(&op.id, &crate::tr!("{}，你终于来了！", "{}, finally! There you are!", Self::camp_name(&camp)), 4000, 1500, out);
                }
                if let Some(me) = Self::pet_for(views, key) {
                    out.push(Out::Ws(crate::msg("pet.script", &json!({ "pet": me.id, "rule": "game", "action": "peek", "ms": 0, "delay": 800 }))));
                }
            }
        }
        match kind {
            EventKind::SessionStart if !quiet => {
                // 抢活: a new session for the other camp makes this one jealous.
                if let Some(o) = other.and_then(|o| Self::camp_pet(views, o)).cloned() {
                    if Self::idle_for(&o, now, 0) && !self.scripts.contains_key(&o.id) && self.under_cap(&o.id, now) && self.rand() < 0.5 && self.cool(&format!("envy-{}", o.id), now, 10 * 60_000) {
                        self.recent.entry(o.id.clone()).or_default().push_back(now);
                        self.act(&o.id, "envy", "sulk", crate::i18n::t("我也可以的……", "I can do that too…"), 3000, 0, false, now, out);
                        db.log_interaction(now, &o.id, "envy", false);
                    }
                }
            }
            EventKind::ToolEnd => {
                // Race progress: one step per finished tool call.
                if let Some(r) = self.games.race.clone() {
                    for (i, k) in [&r.a, &r.b].into_iter().enumerate() {
                        if k == key {
                            self.games.race_steps[i] += 1;
                            if let Some(p) = Self::pet_for(views, k) {
                                out.push(Out::Ws(crate::msg("race", &json!({ "state": "step", "pet": p.id, "steps": self.games.race_steps[i] }))));
                            }
                        }
                    }
                }
            }
            EventKind::TurnStart => {
                self.games.last_start.insert(camp.clone(), (key.clone(), now));
                // Race: both camps start a turn within 2 minutes.
                if self.games.race.is_none() {
                    if let Some(o) = other {
                        if let Some((okey, ots)) = self.games.last_start.get(o).cloned() {
                            if now.saturating_sub(ots) <= RACE_WINDOW && !self.games.raced.contains(key) && !self.games.raced.contains(&okey) {
                                let (a, b) = if camp == "claude-code" { (key.clone(), okey) } else { (okey, key.clone()) };
                                self.start_race(a, b, now, views, db, quiet, out);
                            }
                        }
                    }
                }
                // Bookmaker: one camp alone, a new turn opens the book for 20 s.
                let online: HashSet<&str> = views.iter().filter(|v| v.online > 0).filter_map(|v| v.camp.as_deref()).collect();
                if self.games.race.is_none() && self.games.book.is_none() && online.len() == 1 {
                    self.games.book = Some(Book { key: key.clone(), start: now, until: now + BET_WINDOW });
                    if !quiet && now.saturating_sub(self.games.book_hint_at) >= 3_600_000 {
                        self.games.book_hint_at = now;
                        if let Some(p) = Self::pet_for(views, key) {
                            Self::say(&p.id, crate::i18n::t("庄家局开盘：右键押这一轮会怎样", "Bookmaker open: right-click to bet on how this turn goes"), 4000, 0, out);
                        }
                    }
                }
                // Duel: the next turn of each side counts.
                if let Some(d) = self.games.duel.as_mut() {
                    for i in 0..2 {
                        let k = if i == 0 { &d.a } else { &d.b };
                        if k == key && d.started[i].is_none() {
                            d.started[i] = Some(now);
                            out.push(Out::Ws(crate::msg("duel", &json!({ "state": "step", "pet": d.pets[i], "phase": "started" }))));
                        }
                    }
                }
            }
            _ => {}
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn start_race(&mut self, a: Key, b: Key, now: u64, views: &[PetView], db: &Db, quiet: bool, out: &mut Vec<Out>) {
        let odds = gdb::race_odds(db);
        self.games.raced.insert(a.clone());
        self.games.raced.insert(b.clone());
        self.games.race = Some(Race { a: a.clone(), b: b.clone(), start: now, until: now + BET_WINDOW, odds });
        self.games.race_steps = [0, 0];
        let pets: Vec<String> = [&a, &b].iter().filter_map(|k| Self::pet_for(views, k).map(|p| p.id)).collect();
        out.push(Out::Ws(crate::msg("race", &json!({ "state": "open", "odds": odds, "until": now + BET_WINDOW, "pets": pets }))));
        if !quiet {
            for (k, o) in [(&a, odds.0), (&b, odds.1)] {
                if let Some(p) = Self::pet_for(views, k) {
                    Self::say(&p.id, &crate::tr!("赛跑开始！右键押我（赔率 {o:.1}）", "Race on! Right-click to bet on me (odds {o:.1})"), 5000, 0, out);
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn on_finished(&mut self, key: &Key, result: TurnResult, tool_errors: u32, attentions: u32, ms: u64, now: u64, views: &[PetView], db: &Db, out: &mut Vec<Out>) {
        let cap = self.t.balance.economy.daily_cap;
        let ok = result == TurnResult::Ok;
        self.games.raced.remove(key);

        // Race: first clean finish wins; an error or abort loses on the spot.
        if let Some(r) = self.games.race.clone() {
            if &r.a == key || &r.b == key {
                let winner = if ok { key.0.clone() } else { Self::other_camp(&key.0).unwrap_or("tie").to_string() };
                self.games.race = None;
                self.games.raced.remove(&r.a);
                self.games.raced.remove(&r.b);
                gdb::record_race(db, now, &winner);
                let (net, n) = gdb::settle_bets(db, now, "race", cap, |pick| Some(pick == winner));
                let winner_pet = Self::camp_pet(views, &winner).map(|p| p.id.clone());
                let loser_pet = Self::other_camp(&winner).and_then(|c| Self::camp_pet(views, c)).map(|p| p.id.clone());
                out.push(Out::Ws(crate::msg(
                    "race",
                    &json!({ "state": "done", "winner": winner, "net": net, "winner_pet": winner_pet, "loser_pet": loser_pet }),
                )));
                if !self.frozen {
                    let w = Self::camp_pet(views, &winner).cloned();
                    let l = Self::other_camp(&winner).and_then(|c| Self::camp_pet(views, c)).cloned();
                    let mut text = crate::tr!("我先到的。", "I got there first.");
                    if n > 0 {
                        text = if net > 0 { crate::tr!("我先到的。你押对了，+{net}", "I got there first. You called it, +{net}") } else { crate::tr!("我先到的。你这把 {net}", "I got there first. You: {net}") };
                    }
                    let lose_text = crate::i18n::t("跑输了……", "Lost the race…");
                    // Both free: the winner walks over with the whip.
                    let both_free = matches!((&w, &l), (Some(w), Some(l)) if w.id != l.id && !self.scripts.contains_key(&w.id) && !self.scripts.contains_key(&l.id));
                    if both_free {
                        self.whip_walkover(w.as_ref().unwrap(), l.as_ref().unwrap(), &text, lose_text, "race", now, db, out);
                    } else {
                        if let Some(w) = w.map(|p| p.id) {
                            if !self.scripts.contains_key(&w) {
                                self.act(&w, "race", "cheer", &text, 3000, 0, false, now, out);
                            }
                        }
                        if let Some(l) = l.map(|p| p.id) {
                            if !self.scripts.contains_key(&l) {
                                self.act(&l, "race", "sulk", lose_text, 3000, 0, false, now, out);
                            }
                        }
                    }
                }
            }
        }

        // Bookmaker.
        if let Some(b) = self.games.book.clone() {
            if &b.key == key {
                self.games.book = None;
                let mins = ms / 60_000;
                let band = if mins < 1 { "lt1" } else if mins < 5 { "1to5" } else { "gt5" };
                let erred = result == TurnResult::Error || tool_errors > 0;
                let (net, n) = gdb::settle_bets(db, now, "book", cap, |pick| match pick {
                    "error" => Some(erred),
                    "confirm" => Some(attentions > 0),
                    p if result == TurnResult::Aborted => {
                        let _ = p;
                        None
                    }
                    p => Some(p == band),
                });
                if n > 0 {
                    if let Some(p) = Self::pet_for(views, key) {
                        out.push(Out::Ws(crate::msg("book", &json!({ "state": "done", "pet": p.id, "net": net }))));
                        if !self.frozen {
                            let text = if net > 0 { crate::tr!("庄家局：你赢了 +{net}", "Bookmaker: you won +{net}") } else if net == 0 { crate::tr!("庄家局：退注", "Bookmaker: stake refunded") } else { crate::tr!("庄家局：这把输了 {net}", "Bookmaker: you lost {net}") };
                            Self::say(&p.id, &text, 4000, 600, out);
                        }
                    }
                }
            }
        }

        // Duel.
        if let Some(d) = self.games.duel.as_mut() {
            for i in 0..2 {
                let k = if i == 0 { &d.a } else { &d.b };
                if k == key && d.started[i].is_some() && d.done[i].is_none() {
                    let clean = ok && tool_errors == 0;
                    d.done[i] = Some((now, clean));
                    let took = now.saturating_sub(d.started[i].unwrap_or(now));
                    out.push(Out::Ws(crate::msg("duel", &json!({ "state": "step", "pet": d.pets[i], "phase": "done", "clean": clean, "ms": took }))));
                }
            }
        }

        // 打脸: the taunted camp finishes a turn within 10 minutes.
        if ok {
            if let Some(i) = self.games.taunts.iter().position(|t| t.target_camp == key.0 && now < t.until) {
                let t = self.games.taunts.remove(i);
                if !self.frozen && !self.focus {
                    if let Some(me) = Self::pet_for(views, key) {
                        if !self.scripts.contains_key(&me.id) {
                            self.act(&me.id, "slap", "taunt", crate::i18n::t("我就说。", "Told you."), 3000, 0, false, now, out);
                        }
                    }
                    if !self.scripts.contains_key(&t.taunter) {
                        self.act(&t.taunter, "slap", "sulk", "……", 2500, 600, false, now, out);
                    }
                }
            }
        }

        // 偷吃: the victim finishes in time and takes the fish back.
        if let Some(s) = self.games.steal.clone() {
            if s.victim_camp == key.0 && now < s.until {
                self.games.steal = None;
                gdb::restore_fish(db, s.row);
                if let Some(v) = Self::pet_for(views, key) {
                    Self::say(&v.id, crate::i18n::t("还我鱼！", "Give my fish back!"), 3000, 0, out);
                }
                Self::say(&s.thief, crate::i18n::t("好好好，还你……", "Okay okay, here…"), 3000, 800, out);
            }
        }

        // Evolution: cumulative turns of the pack this session shows.
        if result != TurnResult::Aborted {
            if let Some(p) = Self::pet_for(views, key) {
                let thresholds = self.evolution_thresholds(self.games.evo.get(&p.pack).cloned());
                let before = gdb::stage(gdb::pack_turns(db, &p.pack), &thresholds);
                let after = gdb::stage(gdb::add_pack_turn(db, &p.pack), &thresholds);
                if after > before && !self.frozen {
                    Self::say(&p.id, &crate::tr!("我变了。你看出来了吗。", "I've changed. Can you tell?"), 5000, 0, out);
                }
            }
        }
    }

    fn game_timers(&mut self, now: u64, views: &[PetView], db: &Db, out: &mut Vec<Out>) {
        let cap = self.t.balance.economy.daily_cap;
        if self.games.race.as_ref().is_some_and(|r| now.saturating_sub(r.start) > GAME_TTL) {
            let r = self.games.race.take().unwrap();
            self.games.raced.remove(&r.a);
            self.games.raced.remove(&r.b);
            gdb::record_race(db, now, "tie");
            gdb::settle_bets(db, now, "race", cap, |_| None);
            out.push(Out::Ws(crate::msg("race", &json!({ "state": "done", "winner": "tie", "net": 0 }))));
        }
        if self.games.book.as_ref().is_some_and(|b| now.saturating_sub(b.start) > 2 * 3_600_000) {
            self.games.book = None;
            gdb::settle_bets(db, now, "book", cap, |_| None);
        }
        self.games.taunts.retain(|t| now < t.until);
        if self.games.steal.as_ref().is_some_and(|s| now >= s.until) {
            self.games.steal = None; // the fish is gone
        }
        // Duel decisions.
        if let Some(d) = self.games.duel.clone() {
            let verdict: Option<&str> = match (d.done[0], d.done[1]) {
                (Some((ta, true)), Some((tb, true))) => Some(if ta.abs_diff(tb) < DUEL_TIE_MS { "tie" } else if ta < tb { "a" } else { "b" }),
                (Some((ta, true)), other) if now.saturating_sub(ta) >= DUEL_TIE_MS && other.is_none_or(|(_, c)| !c) => Some("a"),
                (other, Some((tb, true))) if now.saturating_sub(tb) >= DUEL_TIE_MS && other.is_none_or(|(_, c)| !c) => Some("b"),
                (Some((_, false)), Some((_, false))) => Some("tie"),
                _ if now.saturating_sub(d.armed) > GAME_TTL => Some("tie"),
                _ => None,
            };
            if let Some(v) = verdict {
                self.games.duel = None;
                let winner = match v {
                    "a" => d.a.0.clone(),
                    "b" => d.b.0.clone(),
                    _ => "tie".into(),
                };
                gdb::record_duel(db, now, &d.project, &winner);
                if winner == "tie" {
                    for k in [&d.a, &d.b] {
                        if let Some(p) = Self::pet_for(views, k) {
                            Self::say(&p.id, crate::i18n::t("擂台：平局！", "Duel: it's a tie!"), 4000, 0, out);
                        }
                    }
                } else {
                    let _ = db.earn(15, "duel", &d.project, now, cap);
                    let w = Self::camp_pet(views, &winner).cloned();
                    let l = Self::other_camp(&winner).and_then(|c| Self::camp_pet(views, c)).cloned();
                    let win_text = crate::i18n::t("这轮我没出错。", "No mistakes this round.");
                    let lose_text = crate::i18n::t("擂台输了……", "Lost the duel…");
                    let both_free = matches!((&w, &l), (Some(w), Some(l)) if w.id != l.id && !self.scripts.contains_key(&w.id) && !self.scripts.contains_key(&l.id));
                    if both_free && !self.frozen {
                        // The loser takes the whip.
                        self.whip_walkover(w.as_ref().unwrap(), l.as_ref().unwrap(), win_text, lose_text, "duel", now, db, out);
                    } else {
                        if let Some(w) = w.map(|p| p.id) {
                            if !self.scripts.contains_key(&w) {
                                self.act(&w, "duel", "cheer", win_text, 3500, 0, false, now, out);
                            }
                        }
                        if let Some(l) = l.map(|p| p.id) {
                            if !self.scripts.contains_key(&l) {
                                self.act(&l, "duel", "sulk", lose_text, 3000, 0, false, now, out);
                            }
                        }
                    }
                }
                let winner_pet = match v {
                    "a" => d.pets[0].clone(),
                    "b" => d.pets[1].clone(),
                    _ => String::new(),
                };
                out.push(Out::Ws(crate::msg("duel", &json!({ "state": "done", "winner": winner, "winner_pet": winner_pet, "pets": d.pets }))));
            }
        }
    }

    /// 偷吃: an idle pet sneaks over and steals one of today's fish.
    fn try_steal(&mut self, now: u64, views: &[PetView], db: &Db, out: &mut Vec<Out>) {
        if self.games.steal.is_some() {
            return;
        }
        for camp in ["claude-code", "codex"] {
            let (Some(a), Some(b)) = (Self::camp_pet(views, camp).cloned(), Self::other_camp(camp).and_then(|o| Self::camp_pet(views, o)).cloned()) else {
                continue;
            };
            let victim_camp = b.camp.clone().unwrap_or_default();
            if !Self::idle_for(&a, now, 120_000) || !Self::working(&b) || self.scripts.contains_key(&a.id) || !self.under_cap(&a.id, now) {
                continue;
            }
            if self.rand() >= 0.2 || !self.cool(&format!("steal-{}", a.id), now, 30 * 60_000) {
                continue;
            }
            let Some(row) = gdb::take_fish(db, now, Some(&victim_camp), "stolen") else { continue };
            self.recent.entry(a.id.clone()).or_default().push_back(now);
            self.games.steal = Some(Steal { row, thief: a.id.clone(), victim_camp, until: now + STEAL_GRACE });
            out.push(Out::Control(Control::Move { pet: a.id.clone(), target: Some(b.id.clone()) }));
            self.act(&a.id, "steal", "cheer", crate::i18n::t("偷吃一条鱼～", "Snagged a fish~"), 3000, 1200, true, now, out);
            Self::say(&b.id, crate::i18n::t("喂！我的鱼！", "Hey! My fish!"), 3000, 1600, out);
            db.log_interaction(now, &a.id, "steal", false);
            return;
        }
    }

    /// Two idle pets play rock-paper-scissors, best of three.
    fn try_rps(&mut self, now: u64, views: &[PetView], db: &Db, out: &mut Vec<Out>) {
        let (Some(a), Some(b)) = (Self::camp_pet(views, "claude-code").cloned(), Self::camp_pet(views, "codex").cloned()) else { return };
        if !Self::idle_for(&a, now, 180_000) || !Self::idle_for(&b, now, 180_000) {
            return;
        }
        if self.scripts.contains_key(&a.id) || self.scripts.contains_key(&b.id) || !self.under_cap(&a.id, now) || !self.under_cap(&b.id, now) {
            return;
        }
        if self.rand() >= 0.15 || !self.cool("rps", now, 30 * 60_000) {
            return;
        }
        self.play_rps(&a, &b, now, db, views, out);
    }

    pub(crate) fn play_rps(&mut self, a: &PetView, b: &PetView, now: u64, db: &Db, views: &[PetView], out: &mut Vec<Out>) {
        let (mut wa, mut wb, mut round) = (0, 0, 0u64);
        while wa < 2 && wb < 2 && round < 7 {
            let (pa, pb) = ((self.rand() * 3.0) as usize, (self.rand() * 3.0) as usize);
            match rps(pa, pb) {
                1 => wa += 1,
                -1 => wb += 1,
                _ => {}
            }
            // Each round: both hands shake, then show their sign.
            for (p, sign) in [(&a.id, pa), (&b.id, pb)] {
                out.push(Out::Ws(crate::msg("pet.fx", &json!({ "pet": p, "kind": "rps_round", "sign": sign, "delay": round * 2200 }))));
            }
            if round == 0 {
                Self::say(&a.id, crate::i18n::t("猜拳！三局两胜", "Rock-paper-scissors! Best of three"), 1500, 0, out);
            }
            round += 1;
        }
        let end = round * 2200;
        let (w, l) = if wa > wb { (a, b) } else { (b, a) };
        let _ = db.earn(2, "rps", "", now, self.t.balance.economy.daily_cap);
        self.recent.entry(a.id.clone()).or_default().push_back(now);
        self.recent.entry(b.id.clone()).or_default().push_back(now);
        self.scripts.insert(a.id.clone(), Script { rule: "rps".into(), until: now + end + 2500, during_attention: false, moved: false });
        self.scripts.insert(b.id.clone(), Script { rule: "rps".into(), until: now + end + 2500, during_attention: false, moved: false });
        out.push(Out::Ws(crate::msg("pet.script", &json!({ "pet": w.id, "rule": "rps", "action": "cheer", "bubble": crate::i18n::t("你出慢了。", "You were late."), "ms": 2500, "delay": end }))));
        out.push(Out::Ws(crate::msg("pet.script", &json!({ "pet": l.id, "rule": "rps", "action": "sulk", "bubble": crate::i18n::t("再来！", "Again!"), "ms": 2500, "delay": end }))));
        db.log_interaction(now, &a.id, "rps", false);
        db.log_event(now, "game", "rps", 1);
        self.bond(db, views, 1, out);
    }

    /// Affinity 50+: when both pets sleep, they sleep side by side (once a day).
    fn sleep_together(&mut self, now: u64, views: &[PetView], db: &Db, out: &mut Vec<Out>) {
        let (Some(a), Some(b)) = (Self::camp_pet(views, "claude-code").cloned(), Self::camp_pet(views, "codex").cloned()) else { return };
        if a.state != State::Sleeping || b.state != State::Sleeping || gdb::affinity(db, &a.pack, &b.pack) < 50 {
            return;
        }
        let today = day_of(now);
        if self.games.slept_together == today || self.scripts.contains_key(&b.id) {
            return;
        }
        self.games.slept_together = today;
        out.push(Out::Control(Control::Tug { mover: b.id.clone(), anchor: a.id.clone() }));
        self.scripts.insert(b.id.clone(), Script { rule: "sleep".into(), until: now + 10 * 60_000, during_attention: false, moved: true });
        Self::say(&b.id, crate::i18n::t("一起睡～", "Nap time together~"), 3000, 1500, out);
    }

    /// The other camp has not shown up for days.
    fn absence(&mut self, now: u64, views: &[PetView], db: &Db, out: &mut Vec<Out>) {
        if now.saturating_sub(self.games.seen_saved) >= 5 * 60_000 {
            self.games.seen_saved = now;
            for (c, t) in &self.games.seen {
                db.kv_set(&format!("seen_{c}"), &t.to_string());
            }
        }
        if self.frozen || self.focus {
            return;
        }
        let today = day_of(now);
        for camp in ["claude-code", "codex"] {
            let Some(me) = Self::camp_pet(views, camp).cloned() else { continue };
            let other = Self::other_camp(camp).unwrap();
            let last = self.games.seen.get(other).copied().or_else(|| db.kv_get(&format!("seen_{other}")).and_then(|v| v.parse().ok()));
            let Some(last) = last else { continue };
            let days = now.saturating_sub(last) / DAY;
            if days < 3 || self.games.said.get(camp) == Some(&today) || !Self::idle_for(&me, now, 0) {
                continue;
            }
            self.games.said.insert(camp.into(), today.clone());
            let text = if days >= 7 {
                crate::tr!("寻人启事：{} 已经 {days} 天没来上班了", "Missing: {} hasn't shown up for {days} days", Self::camp_name(other))
            } else {
                crate::tr!("{} 今天没来上班？", "No {} at work today?", Self::camp_name(other))
            };
            Self::say(&me.id, &text, 5000, 0, out);
        }
    }

    fn evolution_thresholds(&self, custom: Option<Vec<i64>>) -> Vec<i64> {
        custom.unwrap_or_else(|| vec![100, 500, 2000])
    }

    // ------------------------------------------------------------ ball toss

    /// One pet dragged onto another: they toss a ball back and forth five
    /// times (the overlay draws it); clicking the ball in the air steals it.
    pub fn request_toss(&mut self, a: &str, b: &str, now: u64, views: &[PetView]) -> Vec<Out> {
        let mut out = vec![];
        if self.frozen || self.focus || self.toss.is_some() || a == b {
            return out;
        }
        let (Some(pa), Some(pb)) = (views.iter().find(|v| v.id == a).cloned(), views.iter().find(|v| v.id == b).cloned()) else { return out };
        if pb.state == State::Attention || pa.state == State::Attention || self.scripts.contains_key(&pb.id) {
            return out;
        }
        if !self.cool("toss", now, 2 * 60_000) {
            return out;
        }
        let total = TOSS_THROWS * TOSS_EACH;
        self.toss = Some((a.to_string(), b.to_string(), now));
        for p in [&pa, &pb] {
            self.scripts.insert(p.id.clone(), Script { rule: "toss".into(), until: now + total + 800, during_attention: false, moved: false });
        }
        out.push(Out::Ws(crate::msg("toss", &json!({ "state": "start", "a": a, "b": b, "throws": TOSS_THROWS, "each": TOSS_EACH, "start": now }))));
        // Throwers wave, catchers cheer, alternating.
        for i in 0..TOSS_THROWS {
            let (thrower, catcher) = if i % 2 == 0 { (&pa.id, &pb.id) } else { (&pb.id, &pa.id) };
            out.push(Out::Ws(crate::msg("pet.script", &json!({ "pet": thrower, "rule": "toss", "action": "wave", "ms": 600, "delay": i * TOSS_EACH }))));
            out.push(Out::Ws(crate::msg("pet.script", &json!({ "pet": catcher, "rule": "toss", "action": "cheer", "ms": 500, "delay": i * TOSS_EACH + 1100 }))));
        }
        Self::say(&pa.id, crate::i18n::t("接球！", "Catch!"), 1500, 0, &mut out);
        out
    }

    /// The user clicked the ball in the air.
    pub fn catch_ball(&mut self, now: u64, db: &Db) -> Result<(i64, Vec<Out>), String> {
        let (a, b, _) = self.toss.take().ok_or_else(|| crate::tr!("球已经落地了", "The ball already landed"))?;
        let mut out = vec![];
        for p in [&a, &b] {
            self.scripts.remove(p);
            out.push(Out::Ws(crate::msg("pet.cancel", &json!({ "pet": p, "rule": "toss" }))));
        }
        let paid = db.earn(1, "toss", "", now, self.t.balance.economy.daily_cap);
        Self::say(&a, crate::i18n::t("球被截胡了！", "Ball stolen!"), 3000, 0, &mut out);
        Self::say(&b, crate::i18n::t("还给我们！", "Give it back!"), 3000, 300, &mut out);
        out.push(Out::Ws(crate::msg("toss", &json!({ "state": "caught", "coins": paid }))));
        Ok((paid, out))
    }

    fn toss_timer(&mut self, now: u64, views: &[PetView], db: &Db, out: &mut Vec<Out>) {
        if let Some((a, b, start)) = self.toss.clone() {
            if now.saturating_sub(start) >= TOSS_THROWS * TOSS_EACH + 300 {
                self.toss = None;
                out.push(Out::Ws(crate::msg("toss", &json!({ "state": "done" }))));
                let _ = (a, b);
                self.bond(db, views, 1, out);
            }
        }
    }

    // ------------------------------------------------------------ user actions

    /// Right-click: what can be offered now.
    pub fn offers(&self, now: u64, db: &Db) -> Offers {
        Offers {
            race: self.games.race.as_ref().filter(|r| now < r.until).map(|r| (r.odds.0, r.odds.1, (r.until - now) / 1000)),
            book: self.games.book.as_ref().filter(|b| now < b.until).map(|b| (b.until - now) / 1000),
            duel_running: self.games.duel.is_some(),
            duel_status: self.games.duel.as_ref().map(|d| {
                let side = |i: usize, camp: &str| {
                    let st = match (d.started[i], d.done[i]) {
                        (_, Some((_, true))) => crate::i18n::t("已完成", "done"),
                        (_, Some((_, false))) => crate::i18n::t("失误", "slipped"),
                        (Some(_), None) => crate::i18n::t("进行中", "working"),
                        _ => crate::i18n::t("等下一轮", "waiting for next turn"),
                    };
                    format!("{} {st}", Self::camp_name(camp))
                };
                crate::tr!("擂台 · {} · {} / {}", "Duel · {} · {} / {}", d.project, side(0, &d.a.0), side(1, &d.b.0))
            }),
            wallet: db.balance(),
        }
    }

    /// Place a bet: game "race" (pick = camp) or "book" (pick from BOOK_PICKS).
    pub fn bet(&mut self, game: &str, pick: &str, stake: i64, now: u64, db: &Db) -> Result<String, String> {
        match game {
            "race" => {
                let r = self.games.race.as_ref().filter(|r| now < r.until).ok_or_else(|| crate::tr!("下注时间已过", "Betting is closed"))?;
                let odds = match pick {
                    "claude-code" => r.odds.0,
                    "codex" => r.odds.1,
                    _ => return Err(crate::tr!("只能押 Claude Code 或 Codex", "You can only bet on Claude Code or Codex")),
                };
                gdb::place_bet(db, now, "race", pick, stake, odds)?;
                Ok(crate::tr!("押了 {} {stake} 金币，赔率 {odds:.1}", "Bet {stake} coins on {}, odds {odds:.1}", Self::camp_name(pick)))
            }
            "book" => {
                self.games.book.as_ref().filter(|b| now < b.until).ok_or_else(|| crate::tr!("下注时间已过", "Betting is closed"))?;
                let (id, _, odds) = BOOK_PICKS.iter().find(|(id, _, _)| *id == pick).ok_or_else(|| crate::tr!("没有这个选项", "No such option"))?;
                let name = book_pick_name(id);
                gdb::place_bet(db, now, "book", pick, stake, *odds)?;
                Ok(crate::tr!("押「{name}」{stake} 金币，赔率 {odds}", "Bet {stake} coins on \"{name}\", odds {odds}"))
            }
            _ => Err(crate::tr!("没有这个玩法", "No such game")),
        }
    }

    /// Right-click "发起擂台": pair one session from each camp (same project
    /// preferred); their next turns decide.
    pub fn request_duel(&mut self, hub: &Hub, now: u64) -> Result<(String, Vec<Out>), String> {
        if self.games.duel.is_some() {
            return Err(crate::tr!("擂台已经在进行了", "A duel is already on"));
        }
        if self.frozen {
            return Err(crate::tr!("有会话在等你，先处理它", "A session is waiting for you — handle it first"));
        }
        let live: Vec<_> = hub.sessions.values().filter(|s| s.state != State::Offline).collect();
        let cc: Vec<_> = live.iter().filter(|s| s.source == "claude-code").collect();
        let cx: Vec<_> = live.iter().filter(|s| s.source == "codex").collect();
        if cc.is_empty() || cx.is_empty() {
            return Err(crate::tr!("擂台需要 Claude Code 和 Codex 都有会话在线", "A duel needs both Claude Code and Codex sessions online"));
        }
        let pair = cc
            .iter()
            .flat_map(|a| cx.iter().map(move |b| (*a, *b)))
            .max_by_key(|(a, b)| (a.project == b.project, a.last_event.max(b.last_event)))
            .unwrap();
        let (a, b) = pair;
        let project = if a.project == b.project { a.project_name().to_string() } else { format!("{} vs {}", a.project_name(), b.project_name()) };
        let ka: Key = (a.source.clone(), a.session.clone());
        let kb: Key = (b.source.clone(), b.session.clone());
        let views = hub.pets();
        let pet = |k: &Key| Self::pet_for(&views, k).map(|p| p.id.clone()).unwrap_or_else(|| k.0.clone());
        let pets = [pet(&ka), pet(&kb)];
        self.games.duel = Some(Duel { a: ka, b: kb, project: project.clone(), armed: now, pets: pets.clone(), ..Duel::default() });
        let rule = crate::i18n::t("两边各自的下一轮比速度：不出错、先做完的赢，赢家 +15 金币", "Each side's next turn races: finish first with no errors to win, winner gets +15 coins");
        let out = vec![Out::Ws(crate::msg("duel", &json!({ "state": "start", "project": project, "pets": pets, "rule": rule })))];
        Ok((crate::tr!("擂台开打（{project}）！{rule}。给两边各发一个任务就开始", "Duel on ({project})! {rule}. Give each side a task to start"), out))
    }

    /// Right-click "取消擂台".
    pub fn cancel_duel(&mut self) -> Vec<Out> {
        match self.games.duel.take() {
            Some(d) => vec![Out::Ws(crate::msg("duel", &json!({ "state": "cancel", "pets": d.pets })))],
            None => vec![],
        }
    }

    /// Right-click "猜拳": the pet plays with the user. Returns (pet's sign,
    /// outcome for the user: win / lose / tie, coins won).
    pub fn rps_user(&mut self, pick: usize, now: u64, db: &Db) -> (usize, &'static str, i64) {
        let me = (self.rand() * 3.0).min(2.0) as usize;
        db.log_event(now, "game", "rps_user", 1);
        match rps(pick % 3, me) {
            1 => (me, "win", db.earn(2, "rps", "user", now, self.t.balance.economy.daily_cap)),
            -1 => (me, "lose", 0),
            _ => (me, "tie", 0),
        }
    }

    /// Right-click "投喂": feed today's latest fish (any fish if none today).
    pub fn feed(&mut self, pet: &PetView, views: &[PetView], now: u64, db: &Db) -> (String, Vec<Out>, crate::bond::BondReward, i64) {
        let mut out = vec![];
        let row = gdb::take_fish(db, now, None, "eaten");
        if row.is_none() {
            return (crate::tr!("篓里什么都没有。我等着。", "Nothing in the basket. I'll wait."), out, crate::bond::BondReward::NoFish, 0);
        }
        let _ = db.with(|c| {
            c.execute(
                "INSERT INTO pet_stats (pack, mood, petted) VALUES (?1, 3, 0) ON CONFLICT(pack) DO UPDATE SET mood = mood + 3",
                [&pet.pack],
            )
        });
        self.bond(db, views, 1, &mut out);
        let en = crate::i18n::is_en();
        let (bond_text, reward, feed_count) = crate::bond::on_feed(db, &pet.pack, &pet.pack, &pet.id, en);
        if matches!(reward, crate::bond::BondReward::Letter { .. }) {
            db.log_event(now, "letter", &pet.id, 1);
        }
        let text = if bond_text.is_empty() { crate::tr!("……嗯。", "…mm.") } else { bond_text };
        (text, out, reward, feed_count)
    }

    /// Evolution stage and affinity, for views and the report.
    pub fn stage_of(&self, pack: &str, custom: Option<Vec<i64>>, db: &Db) -> u8 {
        gdb::stage(gdb::pack_turns(db, pack), &self.evolution_thresholds(custom))
    }
}

/// Evolution thresholds a pack declares: `evolutions: [{ "turns": 100 }, …]`.
pub fn thresholds_of(ev: Option<&serde_json::Value>) -> Option<Vec<i64>> {
    let list: Vec<i64> = ev?.as_array()?.iter().filter_map(|e| e.get("turns").and_then(|t| t.as_i64())).collect();
    (!list.is_empty()).then_some(list)
}

const TOSS_THROWS: u64 = 5;
const TOSS_EACH: u64 = 1400;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::balance::Tables;
    use crate::config::Config;
    use crate::pack::Library;
    use crate::protocol::{Detail, Event};

    fn ev(source: &str, session: &str, kind: EventKind, ts: u64) -> Event {
        Event { v: 1, source: source.into(), session: session.into(), project: "/w/p".into(), event: kind, sub: false, detail: Detail::default(), pid: 0, ts }
    }

    fn setup() -> (World, Hub, Db) {
        let mut t = Tables::builtin();
        t.balance.quips.enabled = false;
        let mut cfg = Config::default();
        cfg.features.arena = false;
        let mut w = World::new(t, 7);
        // Keep achievements (and their coins) out of these balance checks.
        w.last_stats = u64::MAX / 2;
        (w, Hub::new(cfg, Library::default()), Db::in_memory())
    }

    /// Coins from games only (bets, refunds, winnings, duel prize).
    fn game_coins(db: &Db) -> i64 {
        db.with(|c| c.query_row("SELECT COALESCE(SUM(delta), 0) FROM wallet_log WHERE reason IN ('bet', 'bet_refund', 'bet_win', 'duel')", [], |r| r.get(0)))
            .unwrap()
    }

    fn msgs(out: &[Out], kind: &str) -> Vec<serde_json::Value> {
        out.iter()
            .filter_map(|o| match o {
                Out::Ws(s) => serde_json::from_str::<serde_json::Value>(s).ok().filter(|v| v["type"] == kind),
                _ => None,
            })
            .map(|v| v["data"].clone())
            .collect()
    }

    #[test]
    fn race_opens_takes_bets_and_pays_the_winner() {
        let (mut w, mut h, db) = setup();
        let t0 = 1_793_600_000_000;
        db.earn(200, "test", "", t0, 300);
        h.apply(&ev("claude-code", "a", EventKind::TurnStart, t0), t0);
        w.tick(t0, &mut h, &db);
        h.apply(&ev("codex", "b", EventKind::TurnStart, t0 + 30_000), t0 + 30_000);
        let out = w.tick(t0 + 30_000, &mut h, &db);
        assert_eq!(msgs(&out, "race")[0]["state"], "open");
        let offers = w.offers(t0 + 31_000, &db);
        assert_eq!(offers.race.map(|r| (r.0, r.1)), Some((1.8, 1.8)));
        w.bet("race", "codex", 50, t0 + 32_000, &db).unwrap();
        assert!(w.bet("race", "codex", 10, t0 + 60_000, &db).is_err(), "window closed");
        // Claude Code errors out: Codex wins on the spot.
        let mut done = ev("claude-code", "a", EventKind::TurnDone, t0 + 90_000);
        done.detail.result = Some(TurnResult::Error);
        h.apply(&done, t0 + 90_000);
        let out = w.tick(t0 + 90_000, &mut h, &db);
        let r = msgs(&out, "race");
        assert_eq!(r[0]["winner"], "codex");
        assert_eq!(r[0]["net"], 40, "50 at 1.8 -> +40");
        assert_eq!(game_coins(&db), 40);
    }

    #[test]
    fn bookmaker_when_alone() {
        let (mut w, mut h, db) = setup();
        let t0 = 1_793_600_000_000;
        db.earn(100, "test", "", t0, 300);
        h.apply(&ev("claude-code", "a", EventKind::TurnStart, t0), t0);
        w.tick(t0, &mut h, &db);
        assert!(w.offers(t0 + 1000, &db).book.is_some());
        w.bet("book", "1to5", 20, t0 + 1000, &db).unwrap();
        w.bet("book", "error", 10, t0 + 1000, &db).unwrap();
        let mut done = ev("claude-code", "a", EventKind::TurnDone, t0 + 3 * 60_000);
        done.detail.result = Some(TurnResult::Ok);
        h.apply(&done, t0 + 3 * 60_000);
        w.tick(t0 + 3 * 60_000, &mut h, &db);
        // 20 at 2.0 wins (+20), 10 on "error" lost (-10).
        assert_eq!(game_coins(&db), 20 - 10);
    }

    #[test]
    fn duel_first_clean_finish_wins() {
        let (mut w, mut h, db) = setup();
        let t0 = 1_793_600_000_000;
        h.apply(&ev("claude-code", "a", EventKind::SessionStart, t0), t0);
        h.apply(&ev("codex", "b", EventKind::SessionStart, t0), t0);
        w.tick(t0, &mut h, &db);
        w.request_duel(&h, t0).unwrap();
        h.apply(&ev("claude-code", "a", EventKind::TurnStart, t0 + 1000), t0 + 1000);
        h.apply(&ev("codex", "b", EventKind::TurnStart, t0 + 1000), t0 + 1000);
        w.tick(t0 + 1000, &mut h, &db);
        let mut d = ev("codex", "b", EventKind::TurnDone, t0 + 60_000);
        d.detail.result = Some(TurnResult::Ok);
        h.apply(&d, t0 + 60_000);
        w.tick(t0 + 60_000, &mut h, &db);
        let out = w.tick(t0 + 66_000, &mut h, &db);
        assert_eq!(msgs(&out, "duel")[0]["winner"], "codex");
        assert_eq!(game_coins(&db), 15);
    }

    #[test]
    fn duel_ignores_the_turn_cut_short_by_the_new_prompt() {
        let (mut w, mut h, db) = setup();
        let t0 = 1_793_600_000_000;
        // Both sessions have an unfinished turn when the duel starts.
        h.apply(&ev("claude-code", "a", EventKind::TurnStart, t0), t0);
        h.apply(&ev("codex", "b", EventKind::TurnStart, t0), t0);
        w.tick(t0, &mut h, &db);
        w.request_duel(&h, t0 + 1000).unwrap();
        h.apply(&ev("claude-code", "a", EventKind::TurnStart, t0 + 2000), t0 + 2000);
        h.apply(&ev("codex", "b", EventKind::TurnStart, t0 + 2000), t0 + 2000);
        w.tick(t0 + 2000, &mut h, &db);
        let mut d = ev("claude-code", "a", EventKind::TurnDone, t0 + 60_000);
        d.detail.result = Some(TurnResult::Ok);
        h.apply(&d, t0 + 60_000);
        w.tick(t0 + 60_000, &mut h, &db);
        let out = w.tick(t0 + 66_000, &mut h, &db);
        assert_eq!(msgs(&out, "duel")[0]["winner"], "claude-code");
    }

    #[test]
    fn face_slap_after_a_taunt() {
        let (mut w, mut h, db) = setup();
        for r in &mut w.t.rules {
            r.chance = 1.0;
        }
        let t0 = 1_793_600_000_000;
        h.apply(&ev("claude-code", "a", EventKind::SessionStart, t0), t0);
        h.apply(&ev("codex", "b", EventKind::TurnStart, t0), t0);
        let mut e = ev("codex", "b", EventKind::TurnDone, t0 + 1000);
        e.detail.result = Some(TurnResult::Error);
        h.apply(&e, t0 + 1000);
        let out = w.tick(t0 + 1000, &mut h, &db);
        assert!(msgs(&out, "pet.script").iter().any(|s| s["rule"] == "taunt"));
        w.tick(t0 + 10_000, &mut h, &db);
        h.apply(&ev("codex", "b", EventKind::TurnStart, t0 + 20_000), t0 + 20_000);
        let mut ok = ev("codex", "b", EventKind::TurnDone, t0 + 120_000);
        ok.detail.result = Some(TurnResult::Ok);
        h.apply(&ok, t0 + 120_000);
        let out = w.tick(t0 + 120_000, &mut h, &db);
        assert!(msgs(&out, "pet.script").iter().any(|s| s["rule"] == "slap" && s["bubble"] == "我就说。"));
    }

    #[test]
    fn ball_toss_and_a_catch() {
        let (mut w, mut h, db) = setup();
        let t0 = 1_793_600_000_000;
        h.apply(&ev("claude-code", "a", EventKind::SessionStart, t0), t0);
        h.apply(&ev("codex", "b", EventKind::SessionStart, t0), t0);
        w.tick(t0, &mut h, &db);
        let views = h.pets();
        let out = w.request_toss("claude-code", "codex", t0 + 1000, &views);
        assert_eq!(msgs(&out, "toss")[0]["state"], "start");
        assert!(w.request_toss("claude-code", "codex", t0 + 2000, &views).is_empty(), "one at a time, cooldown");
        let (coins, out) = w.catch_ball(t0 + 2500, &db).unwrap();
        assert_eq!(coins, 1);
        assert_eq!(msgs(&out, "toss")[0]["state"], "caught");
        assert!(w.catch_ball(t0 + 2600, &db).is_err(), "already caught");
    }

    #[test]
    fn evolution_thresholds_from_the_manifest() {
        let v = serde_json::json!([{ "turns": 10 }, { "turns": 50, "dir": "sprites/big" }]);
        assert_eq!(thresholds_of(Some(&v)), Some(vec![10, 50]));
        assert_eq!(thresholds_of(Some(&serde_json::json!({}))), None);
        assert_eq!(gdb::stage(12, &[10, 50]), 1);
    }

    #[test]
    fn same_camp_tug_in_per_session_mode() {
        let (mut w, mut h, db) = setup();
        h.cfg.mode = crate::config::Mode::PerSession;
        h.cfg.features.arena = true;
        w.last_stats = 0; // this one needs the stats refresh
        let t0 = 1_793_600_000_000;
        h.apply(&ev("codex", "x", EventKind::SessionStart, t0), t0);
        h.apply(&ev("codex", "y", EventKind::SessionStart, t0 + 1), t0 + 1);
        let out = w.tick(t0 + 10_000, &mut h, &db);
        let tug: Vec<_> = msgs(&out, "pet.script").into_iter().filter(|s| s["rule"] == "tug").collect();
        assert_eq!(tug.len(), 2, "two codex pets pull against each other");
        assert!(tug.iter().all(|s| s["bubble"].as_str().unwrap().contains("师兄")), "{tug:?}");
    }

    #[test]
    fn rps_with_the_user_pays_on_a_win() {
        let (mut w, _h, db) = setup();
        let t0 = 1_793_600_000_000;
        let mut wins = 0;
        for i in 0..30 {
            let (_, outcome, coins) = w.rps_user(i % 3, t0, &db);
            if outcome == "win" {
                wins += 1;
                assert_eq!(coins, 2);
            }
        }
        assert!(wins > 0);
        assert_eq!(db.balance(), wins * 2);
    }
}
