//! The world layer (design §4, §7): turns real agent events into gameplay.
//!
//!   fishing      a pet whose sessions work for 5 s casts a line; each finished
//!                turn lands a fish (by turn length), a boot (error) or nothing
//!   bugs         every failed tool drops a bug, an error turn drops 3, three
//!                failures in a row drop a Boss that petrifies the pet until it
//!                is squashed
//!   tug of war   both camps online: a rope shows today's efficiency scores
//!   shadow race  one camp online: today's turns against the best day so far
//!   quips        working pets mutter now and then
//!   interactions YAML rules (taunt, comfort, applaud, urge …)
//!
//! Hard rules: while any session waits on the user everything freezes except
//! reminder rules (`during_attention`), and no pet starts more than
//! `per_pet_hour` interactions in an hour.

use crate::balance::{Rule, Tables, RARITIES};
use crate::db::{day_of, local_hm, ms_of_day, CampDay, Db};
use crate::hub::{Feed, Hub, Key, PetView};
use crate::protocol::{EventKind, TurnResult};
use crate::session::State;
use crate::Control;
use serde::Serialize;
use serde_json::json;
use std::collections::{HashMap, VecDeque};

const CAMPS: [&str; 2] = ["claude-code", "codex"];
const CATCH_BATCH_MS: u64 = 1200;
const STATS_EVERY_MS: u64 = 5000;
const MOOD_PER_MIN: usize = 3;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Bug {
    pub id: u64,
    pub boss: bool,
    pub hp: u32,
    pub max_hp: u32,
    /// Pet it fell from (the overlay drops it at that pet's feet).
    pub pet: String,
    pub born: u64,
    #[serde(skip)]
    pub session: Option<Key>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CatchFx {
    pub outcome: String,
    pub fish_id: String,
    pub name: String,
    pub rarity: String,
    pub coins: i64,
}

#[derive(Debug, Clone)]
pub(crate) struct Script {
    pub(crate) rule: String,
    pub(crate) until: u64,
    pub(crate) during_attention: bool,
    pub(crate) moved: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SquashResult {
    pub id: u64,
    pub hp: u32,
    pub killed: bool,
    pub boss: bool,
    pub coins: i64,
}

/// Messages for windows (WS) and the host app.
#[derive(Debug, Clone)]
pub enum Out {
    Ws(String),
    Control(Control),
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub(crate) struct Stats {
    tug: Option<serde_json::Value>,
    shadow: Option<serde_json::Value>,
}

pub struct World {
    pub t: Tables,
    pub bugs: Vec<Bug>,
    pub(crate) next_id: u64,
    pub(crate) working_since: HashMap<String, u64>,
    /// (rule, pet) -> the working stretch (`working_since`) an `on: slow` rule already fired for.
    pub(crate) slow_fired: HashMap<(String, String), u64>,
    pub(crate) pending: HashMap<String, (u64, Vec<CatchFx>)>,
    pub(crate) cooldowns: HashMap<(String, String), u64>,
    pub(crate) recent: HashMap<String, VecDeque<u64>>,
    pub(crate) scripts: HashMap<String, Script>,
    pub(crate) quip_next: HashMap<String, u64>,
    pub(crate) quip_used: HashMap<String, u64>,
    pub(crate) file_edits: HashMap<(Key, String), u32>,
    pub(crate) mood_hits: HashMap<String, VecDeque<u64>>,
    pub(crate) rng: u64,
    pub frozen: bool,
    pub(crate) focus: bool,
    pub(crate) arena: bool,
    /// `features.sounds`: the 相声 is voiced.
    pub(crate) sounds: bool,
    pub(crate) last_eval: u64,
    pub(crate) last_stats: u64,
    pub(crate) stats: Stats,
    /// Tug offset at the last tug scene; a new scene needs the score to move.
    pub(crate) last_scene: f64,
    /// When the last tug scene played, and whether the user asked for one.
    pub(crate) last_scene_at: u64,
    /// A tug the user asked for, kept for 30 s until the pets are free.
    pub(crate) tug_requested: Option<u64>,
    pub(crate) shadow_ahead: bool,
    pub(crate) last_state: String,
    /// Lines from quip packs bought in the shop.
    pub(crate) extra_quips: Vec<String>,
    /// Equipped rope skin, for tug scenes.
    pub(crate) rope: Option<String>,
    /// M5 games (races, duels, bets, small talk between the pets).
    pub(crate) games: crate::games::Games,
    /// M5 games on/off (unit tests of the world layer switch them off).
    pub(crate) games_on: bool,
    pub(crate) last_reminder: u64,
    pub(crate) attention_times: Vec<u64>,
    /// Ball toss between two pets (overlay draws the ball).
    pub(crate) toss: Option<(String, String, u64)>,
}

impl World {
    pub fn new(t: Tables, seed: u64) -> World {
        World {
            t,
            bugs: vec![],
            next_id: 1,
            working_since: HashMap::new(),
            slow_fired: HashMap::new(),
            pending: HashMap::new(),
            cooldowns: HashMap::new(),
            recent: HashMap::new(),
            scripts: HashMap::new(),
            quip_next: HashMap::new(),
            quip_used: HashMap::new(),
            file_edits: HashMap::new(),
            mood_hits: HashMap::new(),
            rng: seed | 1,
            frozen: false,
            focus: false,
            arena: true,
            sounds: true,
            last_eval: 0,
            last_stats: 0,
            stats: Stats::default(),
            last_scene: 0.0,
            last_scene_at: 0,
            tug_requested: None,
            shadow_ahead: false,
            last_state: String::new(),
            extra_quips: vec![],
            rope: None,
            games: crate::games::Games::default(),
            games_on: true,
            last_reminder: 0,
            attention_times: vec![],
            toss: None,
        }
    }

    pub(crate) fn rand(&mut self) -> f64 {
        // xorshift64*
        self.rng ^= self.rng >> 12;
        self.rng ^= self.rng << 25;
        self.rng ^= self.rng >> 27;
        (self.rng.wrapping_mul(0x2545F4914F6CDD1D) >> 11) as f64 / (1u64 << 53) as f64
    }

    pub(crate) fn pick<'a, T>(&mut self, v: &'a [T]) -> Option<&'a T> {
        if v.is_empty() {
            return None;
        }
        let i = ((self.rand() * v.len() as f64) as usize).min(v.len() - 1);
        v.get(i)
    }

    /// Main pet of a camp (the camp pet itself if it exists).
    pub fn camp_pet<'a>(views: &'a [PetView], camp: &str) -> Option<&'a PetView> {
        views.iter().find(|v| v.id == camp).or_else(|| views.iter().find(|v| v.camp.as_deref() == Some(camp)))
    }

    pub(crate) fn pet_for(views: &[PetView], key: &Key) -> Option<PetView> {
        let skey = format!("{}/{}", key.0, key.1);
        views
            .iter()
            .find(|v| v.sessions.iter().any(|c| c.key == skey))
            .or_else(|| Self::camp_pet(views, &key.0))
            .cloned()
    }

    pub fn tick(&mut self, now: u64, hub: &mut Hub, db: &Db) -> Vec<Out> {
        let mut out = Vec::new();
        let feed = std::mem::take(&mut hub.feed);
        let feed_copy = feed.clone();
        let views = hub.pets();
        self.focus = hub.cfg.features.focus_mode;
        self.arena = hub.cfg.features.arena;
        self.sounds = hub.cfg.features.sounds;

        // Freeze while anyone waits on the user.
        let frozen = hub.sessions.values().any(|s| s.state == State::Attention);
        if frozen && !self.frozen {
            let ids: Vec<String> =
                self.scripts.iter().filter(|(_, s)| !s.during_attention).map(|(k, _)| k.clone()).collect();
            for id in ids {
                self.cancel(&id, &mut out);
            }
        }
        self.frozen = frozen;
        // A pet that itself starts waiting drops whatever it was doing.
        let waiting: Vec<String> = self
            .scripts
            .keys()
            .filter(|id| views.iter().any(|v| &v.id == *id && v.state == State::Attention))
            .cloned()
            .collect();
        for id in waiting {
            self.cancel(&id, &mut out);
        }

        for f in feed {
            match f {
                Feed::Finished { key, project, fin } => {
                    db.record_turn(&key.0, &key.1, &project, &fin);
                    if fin.result != TurnResult::Aborted {
                        crate::progress::on_turn(db, fin.result == TurnResult::Ok && fin.tool_errors == 0);
                    }
                    // Night owl: one gentle reminder per night.
                    let h = crate::progress::hour(now);
                    if !(4..23).contains(&h) && !self.focus && db.kv_get("night_reminded").as_deref() != Some(day_of(now).as_str()) {
                        if let Some(pet) = Self::pet_for(&views, &key) {
                            db.kv_set("night_reminded", &day_of(now));
                            out.push(Out::Ws(crate::msg("pet.script", &json!({ "pet": pet.id, "rule": "night", "bubble": crate::i18n::t("外面早黑透了。我陪到你关机。", "It's been dark out for hours. I'll stay till you shut down."), "ms": 5000 }))));
                        }
                    }
                    let ms = fin.ended_at.saturating_sub(fin.started_at);
                    if !self.focus && ms >= self.t.balance.fishing.min_turn_s * 1000 {
                        if let Some(pet) = Self::pet_for(&views, &key) {
                            let fx = self.land(&key, fin.result, fin.tool_errors == 0, ms, now, db);
                            let e = self.pending.entry(pet.id.clone()).or_insert((now, vec![]));
                            e.1.push(fx);
                        }
                    }
                    self.rules_turn_done(&key, fin.result, ms, now, &views, db, &mut out);
                }
                Feed::Event { key, ev } => {
                    if ev.sub {
                        continue;
                    }
                    match ev.event {
                        EventKind::ToolStart => {
                            if let Some(file) = ev.detail.file.clone() {
                                *self.file_edits.entry((key.clone(), file)).or_insert(0) += 1;
                            }
                        }
                        EventKind::ToolEnd if ev.detail.ok == Some(false) && ev.detail.interrupt != Some(true) => {
                            let n = self.t.balance.bugs.per_tool_fail;
                            self.spawn_bugs(&key, n, false, now, &views, &mut out);
                        }
                        EventKind::TurnDone if ev.detail.result == Some(TurnResult::Error) => {
                            let n = self.t.balance.bugs.per_turn_error;
                            self.spawn_bugs(&key, n, false, now, &views, &mut out);
                        }
                        EventKind::SessionEnd => {
                            self.file_edits.retain(|(k, _), _| k != &key);
                        }
                        _ => {}
                    }
                }
                Feed::State { key, to, from } => {
                    if to == State::Attention && from != State::Attention {
                        self.attention_times.push(now);
                    }
                    if to == State::Stone && from != State::Stone {
                        let has_boss = self.bugs.iter().any(|b| b.boss && b.session.as_ref() == Some(&key));
                        if !has_boss {
                            self.spawn_bugs(&key, 1, true, now, &views, &mut out);
                        }
                    }
                    self.rules_state(&key, to, now, &views, db, &mut out);
                }
            }
        }

        // Bugs dig into the floor after their time is up.
        let ttl = self.t.balance.bugs.ttl_min * 60_000;
        self.bugs.retain(|b| now.saturating_sub(b.born) < ttl);

        // Fishing: cast after `start_after_s` of work; the rod stays planted
        // through an attention so fishing resumes right after.
        let start_after = self.t.balance.fishing.start_after_s * 1000;
        hub.activities.clear();
        for v in &views {
            let working = matches!(v.state, State::Thinking | State::Tool | State::Reading);
            if working || (v.state == State::Attention && self.working_since.contains_key(&v.id)) {
                let since = *self.working_since.entry(v.id.clone()).or_insert(now);
                if working && !self.focus && !self.frozen && now.saturating_sub(since) >= start_after {
                    hub.activities.insert(v.id.clone(), "fishing".into());
                }
            } else {
                self.working_since.remove(&v.id);
            }
        }

        // Daily rhythm: pack the bag before going home, dance on Friday evening.
        if !self.focus && !self.frozen {
            if let Some(act) = self.rhythm(now) {
                for v in &views {
                    if matches!(v.state, State::Idle | State::Done) && !hub.activities.contains_key(&v.id) {
                        hub.activities.insert(v.id.clone(), act.to_string());
                    }
                }
            }
        }

        // Catches landed close together reel in as one animation ("+N").
        if !self.frozen {
            let due: Vec<String> =
                self.pending.iter().filter(|(_, (t0, _))| now.saturating_sub(*t0) >= CATCH_BATCH_MS).map(|(k, _)| k.clone()).collect();
            for pet in due {
                if let Some((_, items)) = self.pending.remove(&pet) {
                    let coins: i64 = items.iter().map(|i| i.coins).sum();
                    out.push(Out::Ws(crate::msg("pet.fx", &json!({ "pet": pet, "kind": "catch", "items": items, "coins": coins }))));
                }
            }
        }

        // Finished scripts: walk home if the pet went somewhere.
        let done: Vec<String> = self.scripts.iter().filter(|(_, s)| now >= s.until).map(|(k, _)| k.clone()).collect();
        for id in done {
            if let Some(s) = self.scripts.remove(&id) {
                if s.moved {
                    out.push(Out::Control(Control::Move { pet: id, target: None }));
                }
            }
        }

        if self.games_on {
            self.games_tick(now, &feed_copy, &views, hub, db, &mut out);
        }

        if now.saturating_sub(self.last_eval) >= self.t.balance.interactions.tick_ms {
            self.last_eval = now;
            self.quips(now, &views, hub, &mut out);
            self.rules_slow(now, &views, db, &mut out);
        }
        if now.saturating_sub(self.last_stats) >= STATS_EVERY_MS {
            self.last_stats = now;
            self.refresh_stats(now, &views, db, &mut out);
            self.daily_report(now, db, &mut out);
            self.refresh_progress(now, &views, hub, db, &mut out);
            if now.saturating_sub(self.last_reminder) >= 60_000 {
                self.last_reminder = now;
                self.reminders(now, &views, hub, db, &mut out);
            }
        }

        let state = json!({
            "frozen": self.frozen,
            "focus": self.focus,
            "arena": self.arena,
            "bugs": self.bugs,
            "tug": self.stats.tug,
            "shadow": self.stats.shadow,
            "rope": self.rope,
        })
        .to_string();
        if state != self.last_state {
            self.last_state = state;
            out.push(Out::Ws(self.state_msg(now, db)));
        }
        out
    }

    /// Full world state for a window that just connected.
    pub fn state_msg(&self, now: u64, db: &Db) -> String {
        crate::msg(
            "world.state",
            &json!({
                "frozen": self.frozen,
                "focus": self.focus,
                "arena": self.arena,
                "bugs": self.bugs,
                "tug": self.stats.tug,
                "shadow": self.stats.shadow,
                "rope": self.rope,
                "wallet": db.balance(),
                "income_today": db.income_on(&day_of(now)),
                "daily_cap": self.t.balance.economy.daily_cap,
            }),
        )
    }

    pub(crate) fn cancel(&mut self, pet: &str, out: &mut Vec<Out>) {
        if let Some(s) = self.scripts.remove(pet) {
            out.push(Out::Ws(crate::msg("pet.cancel", &json!({ "pet": pet, "rule": s.rule }))));
            if s.moved {
                // Teleport home: the reminder must not wait for a walk.
                out.push(Out::Control(Control::Move { pet: pet.to_string(), target: None }));
            }
        }
    }

    // ------------------------------------------------------------ fishing

    fn roll_rarity(&mut self, minutes: u64) -> &'static str {
        let odds = &self.t.balance.fishing.odds;
        let row = odds
            .iter()
            .find(|o| o.under_min > 0 && minutes < o.under_min)
            .or_else(|| odds.iter().find(|o| o.under_min == 0))
            .map(|o| o.p)
            .unwrap_or([100, 0, 0, 0]);
        let total: u32 = row.iter().sum();
        let mut x = self.rand() * total.max(1) as f64;
        for (i, w) in row.iter().enumerate() {
            if x < *w as f64 {
                return RARITIES[i];
            }
            x -= *w as f64;
        }
        RARITIES[0]
    }

    /// Settle one finished turn: fish, boot or nothing.
    fn land(&mut self, key: &Key, result: TurnResult, clean: bool, ms: u64, now: u64, db: &Db) -> CatchFx {
        match result {
            TurnResult::Error => {
                db.add_catch(now, "boot", "", "", 0, &key.0, &key.1);
                CatchFx { outcome: "boot".into(), fish_id: String::new(), name: crate::i18n::t("破靴子", "Old boot").into(), rarity: String::new(), coins: 0 }
            }
            TurnResult::Aborted => {
                db.add_catch(now, "escape", "", "", 0, &key.0, &key.1);
                CatchFx { outcome: "escape".into(), fish_id: String::new(), name: String::new(), rarity: String::new(), coins: 0 }
            }
            TurnResult::Ok => {
                let rarity = self.roll_rarity(ms / 60_000);
                let pool: Vec<_> = self.t.fish_of(rarity).into_iter().cloned().collect();
                let fish = self.pick(&pool).cloned().unwrap_or_else(|| crate::balance::Fish {
                    id: "fish".into(),
                    name: crate::i18n::t("小鱼", "Minnow").into(),
                    rarity: rarity.into(),
                    note: String::new(),
                    name_en: None,
                    note_en: None,
                });
                let f = &self.t.balance.fishing;
                let mut value = *f.coins.get(rarity).unwrap_or(&1) as f64;
                if clean {
                    value *= f.clean_bonus;
                }
                if db.fish_last_hour(now) >= f.hourly_soft_cap {
                    value *= 0.5;
                }
                let coins = db.earn(value.round() as i64, "fish", &fish.id, now, self.t.balance.economy.daily_cap);
                db.add_catch(now, "fish", &fish.id, rarity, coins, &key.0, &key.1);
                CatchFx { outcome: "fish".into(), fish_id: fish.id, name: fish.name, rarity: rarity.into(), coins }
            }
        }
    }

    // ------------------------------------------------------------ bugs

    fn spawn_bugs(&mut self, key: &Key, n: u32, boss: bool, now: u64, views: &[PetView], out: &mut Vec<Out>) {
        if self.focus {
            return;
        }
        let Some(pet) = Self::pet_for(views, key) else { return };
        for _ in 0..n {
            let hp = if boss { self.t.balance.bugs.boss_hp } else { 1 };
            let bug = Bug { id: self.next_id, boss, hp, max_hp: hp, pet: pet.id.clone(), born: now, session: Some(key.clone()) };
            self.next_id += 1;
            out.push(Out::Ws(crate::msg("bug.spawn", &bug)));
            self.bugs.push(bug);
        }
        // Over the cap: the oldest ordinary bugs leave first.
        while self.bugs.len() > self.t.balance.bugs.max {
            match self.bugs.iter().position(|b| !b.boss) {
                Some(i) => {
                    self.bugs.remove(i);
                }
                None => break,
            }
        }
    }

    /// One click on a bug.
    pub fn squash(&mut self, id: u64, now: u64, hub: &mut Hub, db: &Db) -> Result<SquashResult, String> {
        if self.frozen {
            return Err(crate::tr!("有会话在等你，先去处理它", "A session is waiting for you — go handle it first"));
        }
        let i = self.bugs.iter().position(|b| b.id == id).ok_or_else(|| crate::tr!("这只虫已经不在了", "That bug is already gone"))?;
        self.bugs[i].hp = self.bugs[i].hp.saturating_sub(1);
        let b = self.bugs[i].clone();
        if b.hp > 0 {
            return Ok(SquashResult { id, hp: b.hp, killed: false, boss: b.boss, coins: 0 });
        }
        self.bugs.remove(i);
        let cfg = &self.t.balance.bugs;
        let cap = self.t.balance.economy.daily_cap;
        let coins = if b.boss {
            if let Some(k) = &b.session {
                hub.clear_stone(k, now);
            }
            db.earn(cfg.boss_coins, "boss", "", now, cap)
        } else {
            let total = db.normal_kills_total() + 1;
            if cfg.coins_per > 0 && total % cfg.coins_per as u64 == 0 {
                db.earn(cfg.coins, "bugs", "", now, cap)
            } else {
                0
            }
        };
        db.record_bug_kill(now, b.boss, coins);
        Ok(SquashResult { id, hp: 0, killed: true, boss: b.boss, coins })
    }

    // ------------------------------------------------------------ pets

    /// Petting raises the pack's mood, at most 3 times a minute.
    pub fn petted(&mut self, pack: &str, now: u64, db: &Db) -> bool {
        let q = self.mood_hits.entry(pack.to_string()).or_default();
        while q.front().is_some_and(|t| now.saturating_sub(*t) >= 60_000) {
            q.pop_front();
        }
        if q.len() >= MOOD_PER_MIN {
            return false;
        }
        q.push_back(now);
        db.add_mood(pack, 1);
        true
    }

    // ------------------------------------------------------------ interactions

    pub(crate) fn other_camp(camp: &str) -> Option<&'static str> {
        match camp {
            "claude-code" => Some("codex"),
            "codex" => Some("claude-code"),
            _ => None,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn rules_turn_done(&mut self, key: &Key, result: TurnResult, ms: u64, now: u64, views: &[PetView], db: &Db, out: &mut Vec<Out>) {
        let res = match result {
            TurnResult::Ok => "ok",
            TurnResult::Error => "error",
            TurnResult::Aborted => "aborted",
        };
        let rules: Vec<Rule> = self
            .t
            .rules
            .iter()
            .filter(|r| r.on == "turn_done" && r.result.as_deref().is_none_or(|x| x == res) && ms >= r.min_turn_s * 1000)
            .cloned()
            .collect();
        for r in rules {
            self.fire(&r, key, now, views, db, out);
        }
    }

    /// `on: slow`: a pet has been working on one stretch for `min_turn_s`.
    /// Fires once per stretch (whether or not the dice let the actor act).
    fn rules_slow(&mut self, now: u64, views: &[PetView], db: &Db, out: &mut Vec<Out>) {
        let rules: Vec<Rule> = self.t.rules.iter().filter(|r| r.on == "slow").cloned().collect();
        if rules.is_empty() {
            return;
        }
        for v in views {
            let working = matches!(v.state, State::Thinking | State::Tool | State::Reading);
            let Some(&since) = self.working_since.get(&v.id).filter(|_| working) else { continue };
            let Some(card) = v.sessions.iter().find(|c| matches!(c.state, State::Thinking | State::Tool | State::Reading)) else { continue };
            let key: Key = (card.source.clone(), card.session.clone());
            for r in &rules {
                if now.saturating_sub(since) < r.min_turn_s * 1000 {
                    continue;
                }
                let mark = (r.id.clone(), v.id.clone());
                if self.slow_fired.get(&mark) == Some(&since) {
                    continue;
                }
                self.slow_fired.insert(mark, since);
                self.fire(r, &key, now, views, db, out);
            }
        }
        self.slow_fired.retain(|(_, pet), _| self.working_since.contains_key(pet));
    }

    fn rules_state(&mut self, key: &Key, to: State, now: u64, views: &[PetView], db: &Db, out: &mut Vec<Out>) {
        let name = serde_json::to_value(to).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_default();
        let rules: Vec<Rule> =
            self.t.rules.iter().filter(|r| r.on == "state" && r.state.as_deref() == Some(name.as_str())).cloned().collect();
        for r in rules {
            self.fire(&r, key, now, views, db, out);
        }
    }

    /// Hourly cap check without recording.
    pub(crate) fn under_cap(&mut self, pet: &str, now: u64) -> bool {
        let q = self.recent.entry(pet.to_string()).or_default();
        while q.front().is_some_and(|t| now.saturating_sub(*t) >= 3_600_000) {
            q.pop_front();
        }
        q.len() < self.t.balance.interactions.per_pet_hour
    }

    pub(crate) fn fire(&mut self, r: &Rule, key: &Key, now: u64, views: &[PetView], db: &Db, out: &mut Vec<Out>) {
        if self.focus || (self.frozen && !r.during_attention) {
            return;
        }
        let target = Self::pet_for(views, key);
        let actor = match r.from.as_str() {
            "self" => target.clone(),
            _ => Self::other_camp(&key.0).and_then(|c| Self::camp_pet(views, c)).cloned(),
        };
        let Some(actor) = actor else { return };
        if self.scripts.contains_key(&actor.id) || actor.state == State::Attention {
            return;
        }
        if target.as_ref().is_some_and(|t| t.id == actor.id) && r.from != "self" {
            return;
        }
        let actor_state = serde_json::to_value(actor.state).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_default();
        if !r.actor_state.is_empty() && !r.actor_state.iter().any(|s| s == &actor_state) {
            return;
        }
        let ck = (r.id.clone(), actor.id.clone());
        if self.cooldowns.get(&ck).is_some_and(|until| now < *until) {
            return;
        }
        if !self.under_cap(&actor.id, now) {
            return;
        }
        if self.rand() >= r.chance {
            return;
        }
        // Commit.
        self.cooldowns.insert(ck, now + r.cooldown_ms());
        self.recent.entry(actor.id.clone()).or_default().push_back(now);
        let working = matches!(actor.state, State::Thinking | State::Tool | State::Reading);
        let action = if working { r.working_action.clone().unwrap_or_else(|| r.action.clone()) } else { r.action.clone() };
        let camp = actor.camp.clone().unwrap_or_default();
        let lines: Vec<String> = r.bubble.as_ref().map(|b| b.for_camp(&camp).to_vec()).unwrap_or_default();
        let bubble = self.pick(&lines).cloned();
        let moved = r.movement.as_deref() == Some("to_target") && target.is_some();
        let walk_ms = if moved { 1200 } else { 0 };
        self.scripts.insert(
            actor.id.clone(),
            Script { rule: r.id.clone(), until: now + r.ms + walk_ms, during_attention: r.during_attention, moved },
        );
        if moved {
            out.push(Out::Control(Control::Move { pet: actor.id.clone(), target: target.as_ref().map(|t| t.id.clone()) }));
        }
        out.push(Out::Ws(crate::msg(
            "pet.script",
            &json!({ "pet": actor.id, "rule": r.id, "action": action, "bubble": bubble, "ms": r.ms, "delay": walk_ms }),
        )));
        db.log_interaction(now, &actor.id, &r.id, self.frozen);
        match r.id.as_str() {
            "taunt" => self.on_taunt(&key.0, &actor.id, now),
            "comfort" => self.bond(db, views, 1, out),
            // The rule's bubble is the whipper's line; the lash and the victim's line come here.
            "whip" => {
                if let Some(t) = &target {
                    self.whip(Some(&actor.id), &t.id, t.state, crate::whip::Reason::Slow, walk_ms, out);
                }
            }
            _ => {}
        }
    }

    // ------------------------------------------------------------ quips

    fn quips(&mut self, now: u64, views: &[PetView], hub: &Hub, out: &mut Vec<Out>) {
        let q = self.t.balance.quips.clone();
        if !q.enabled || self.focus || self.frozen {
            return;
        }
        for v in views {
            let working = matches!(v.state, State::Thinking | State::Tool | State::Reading);
            if !working || self.scripts.contains_key(&v.id) {
                self.quip_next.remove(&v.id);
                continue;
            }
            let gap = q.min_s * 1000 + (self.rand() * ((q.max_s.saturating_sub(q.min_s)) * 1000) as f64) as u64;
            let due = *self.quip_next.entry(v.id.clone()).or_insert(now + gap);
            if now < due {
                continue;
            }
            self.quip_next.insert(v.id.clone(), now + gap);
            // Variables come from the busiest session of this pet.
            let card = v.sessions.iter().find(|c| matches!(c.state, State::Tool | State::Reading | State::Thinking));
            let file = card.and_then(|c| c.file.clone());
            let tool = card.and_then(|c| c.tool.clone());
            let project = card.map(|c| c.project.clone()).filter(|p| !p.is_empty());
            let n = card.and_then(|c| {
                let key = (c.source.clone(), c.session.clone());
                file.as_ref().and_then(|f| self.file_edits.get(&(key, f.clone())).copied())
            });
            let fill = |line: &str| -> Option<String> {
                let mut s = line.to_string();
                for (k, v) in [("{file}", file.clone()), ("{tool}", tool.clone()), ("{project}", project.clone()), ("{n}", n.map(|n| n.to_string()))] {
                    if s.contains(k) {
                        s = s.replace(k, &v?);
                    }
                }
                Some(s)
            };
            let fresh = q.no_repeat_min * 60_000;
            let all_lines: Vec<String> = q.lines.iter().cloned().chain(self.extra_quips.iter().cloned()).collect();
            let options: Vec<String> = all_lines
                .iter()
                .filter(|l| self.quip_used.get(*l).is_none_or(|t| now.saturating_sub(*t) >= fresh))
                .filter(|l| !l.contains("{n}") || n.is_some_and(|n| n >= 3))
                .filter_map(|l| fill(l).map(|text| (l.clone(), text)))
                .map(|(l, t)| format!("{l}\u{1}{t}"))
                .collect();
            if let Some(choice) = self.pick(&options).cloned() {
                let (line, text) = choice.split_once('\u{1}').unwrap();
                self.quip_used.insert(line.to_string(), now);
                out.push(Out::Ws(crate::msg("pet.script", &json!({ "pet": v.id, "rule": "quip", "bubble": text, "ms": 4000 }))));
            }
            let _ = hub;
        }
    }

    // ------------------------------------------------------------ tug / shadow / report

    /// Daily tug score of one camp (efficiency or workload).
    pub fn camp_score(&self, d: Option<&CampDay>) -> f64 {
        self.score(d)
    }

    fn score(&self, d: Option<&CampDay>) -> f64 {
        let d = d.cloned().unwrap_or_default();
        if self.t.balance.tug.scoring == "workload" {
            return (d.turns + d.clean) as f64 - d.errors as f64;
        }
        if d.turns < self.t.balance.tug.min_turns || d.turns == 0 {
            return 0.0;
        }
        (d.clean as f64 - d.errors as f64) / d.turns as f64
    }

    fn refresh_stats(&mut self, now: u64, views: &[PetView], db: &Db, out: &mut Vec<Out>) {
        let today = day_of(now);
        let online: Vec<&str> = CAMPS
            .iter()
            .copied()
            .filter(|c| views.iter().any(|v| v.camp.as_deref() == Some(*c) && v.online > 0))
            .collect();
        let days = db.camp_day(&today);
        if online.len() == 2 {
            let (a, b) = (self.score(days.get("claude-code")), self.score(days.get("codex")));
            let offset = if self.t.balance.tug.scoring == "workload" { (a - b) / (a + b + 5.0) } else { (a - b) / 2.0 }
                .clamp(-1.0, 1.0);
            // The two main pets act out a short tug of war whenever the score
            // has moved since the last one (and nobody is waiting on the user).
            // A scene when the score moved, at least every 20 minutes, or on request.
            let due = (offset - self.last_scene).abs() >= 0.1 || now.saturating_sub(self.last_scene_at) >= 20 * 60_000;
            if self.tug_requested.is_some_and(|t| now.saturating_sub(t) > 30_000) {
                self.tug_requested = None;
            }
            let forced = self.tug_requested.is_some();
            if (forced || (self.arena && due)) && self.tug_scene(offset, (a, b), forced, now, views, db, out) {
                self.last_scene = offset;
                self.last_scene_at = now;
                self.tug_requested = None;
            }
            self.stats.tug = Some(json!({
                "offset": offset,
                "scoring": self.t.balance.tug.scoring,
                "claude-code": { "score": a, "day": days.get("claude-code").cloned().unwrap_or_default() },
                "codex": { "score": b, "day": days.get("codex").cloned().unwrap_or_default() },
            }));
            self.stats.shadow = None;
        } else {
            self.stats.tug = None;
            self.tug_requested = None;
            // 同门内卷: two pets of the same camp (per-session mode) pull against each other.
            if online.len() == 1 && self.arena && now.saturating_sub(self.last_scene_at) >= 20 * 60_000 {
                let mine: Vec<&PetView> = views.iter().filter(|v| v.camp.as_deref() == Some(online[0]) && v.online > 0).collect();
                if let [a, b, ..] = mine.as_slice() {
                    let score = |v: &PetView| {
                        let d = v.sessions.first().map(|c| db.session_day(&today, &c.session)).unwrap_or_default();
                        self.score(Some(&d))
                    };
                    let (sa, sb) = (score(a), score(b));
                    let lead = if sa - sb > 0.1 { Some(true) } else if sb - sa > 0.1 { Some(false) } else { None };
                    let (a, b) = ((*a).clone(), (*b).clone());
                    if self.tug_pair(&a, &b, lead, (sa, sb), true, false, now, db, out) {
                        self.last_scene_at = now;
                    }
                }
            }
            // Shadow race against the best day so far.
            let times = db.turn_times(&today);
            let tod = ms_of_day(now);
            let best = db.best_day(&today);
            let (best_now, best_total, best_day) = match &best {
                Some((d, n)) => (db.turn_times(d).iter().filter(|t| **t <= tod).count() as u32, *n, d.clone()),
                None => (0, 0, String::new()),
            };
            let today_n = times.len() as u32;
            let ahead = best.is_some() && today_n > best_now;
            if ahead && !self.shadow_ahead {
                if let Some(c) = online.first() {
                    self.react(c, "shadow_pass", "cheer", crate::i18n::t("这个点。你以前从没走到这儿过。", "By this hour? You've never been this far before."), now, views, db, out);
                }
            }
            self.shadow_ahead = ahead;
            self.stats.shadow = (!online.is_empty() || today_n > 0).then(|| {
                json!({ "today": today_n, "best_now": best_now, "best_total": best_total, "best_day": best_day, "ahead": ahead })
            });
        }
    }

    /// Tug of war with the real pets: the Codex pet walks over to the Claude
    /// Code pet, both lean back on a rope for a few seconds, then it walks home.
    /// Counts as one interaction for each pet.
    #[allow(clippy::too_many_arguments)]
    fn tug_scene(&mut self, offset: f64, scores: (f64, f64), forced: bool, now: u64, views: &[PetView], db: &Db, out: &mut Vec<Out>) -> bool {
        let (Some(cc), Some(cx)) = (Self::camp_pet(views, "claude-code").cloned(), Self::camp_pet(views, "codex").cloned())
        else {
            return false;
        };
        let lead = if offset > 0.05 {
            Some(true)
        } else if offset < -0.05 {
            Some(false)
        } else {
            None
        };
        if !self.tug_pair(&cc, &cx, lead, scores, false, forced, now, db, out) {
            return false;
        }
        // Affinity 20+: a high five when the bout ends.
        if crate::games_db::affinity(db, &cc.pack, &cx.pack) >= 20 {
            for p in [&cc, &cx] {
                out.push(Out::Ws(crate::msg("pet.script", &json!({ "pet": p.id, "rule": "game", "bubble": crate::i18n::t("击掌！✋", "High five! ✋"), "ms": 2000, "delay": 5500 }))));
            }
        }
        self.bond(db, views, 1, out);
        true
    }

    /// A tug bout between any two pets: `right` walks up to `left`, both lean
    /// back on a rope. `lead`: Some(true) the left pet leads. `brothers`: two
    /// pets of the same camp (per-session mode), with their own lines.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn tug_pair(&mut self, left: &PetView, right: &PetView, lead: Option<bool>, scores: (f64, f64), brothers: bool, forced: bool, now: u64, db: &Db, out: &mut Vec<Out>) -> bool {
        const WALK: u64 = 1200;
        const PULL: u64 = 4500;
        if self.focus || self.frozen || left.id == right.id {
            return false;
        }
        for p in [left, right] {
            if self.scripts.contains_key(&p.id) || p.state == State::Attention {
                return false;
            }
        }
        let ck = ("tug".to_string(), "pair".to_string());
        // A bout the user asked for skips the cooldown and the hourly cap.
        if !forced {
            if self.cooldowns.get(&ck).is_some_and(|until| now < *until) {
                return false;
            }
            if !self.under_cap(&left.id, now) || !self.under_cap(&right.id, now) {
                return false;
            }
        }
        self.cooldowns.insert(ck, now + 10 * 60_000);
        let score = format!("{:.2} : {:.2}", scores.0, scores.1);
        for (p, moved, is_left) in [(left, false, true), (right, true, false)] {
            self.recent.entry(p.id.clone()).or_default().push_back(now);
            self.scripts.insert(p.id.clone(), Script { rule: "tug".into(), until: now + WALK + PULL, during_attention: false, moved });
            let mine = lead.map(|l| l == is_left);
            let bubble = match (mine, brothers) {
                (Some(true), false) => crate::tr!("我领先！{score}", "I'm winning! {score}"),
                (Some(false), false) => crate::tr!("再使把劲…", "Pull harder…"),
                (None, false) => crate::tr!("势均力敌 {score}", "Neck and neck {score}"),
                (Some(true), true) => crate::tr!("师兄让让～ {score}", "Go easy on me, bro~ {score}"),
                (Some(false), true) => crate::tr!("同门相煎何太急……", "Sibling rivalry, huh……"),
                (None, true) => crate::tr!("师兄弟不分上下 {score}", "Siblings, dead even {score}"),
            };
            out.push(Out::Ws(crate::msg(
                "pet.script",
                &json!({
                    "pet": p.id, "rule": "tug", "action": "walk", "ms": PULL, "delay": WALK, "bubble": bubble,
                    "tug": { "lead": mine, "offset": scores.0 - scores.1 },
                }),
            )));
            db.log_interaction(now, &p.id, "tug", false);
        }
        out.push(Out::Control(Control::Tug { mover: right.id.clone(), anchor: left.id.clone() }));
        crate::progress::bump(db, "tugs");
        true
    }

    /// Achievements, the weekly season, hats and shop extras (every 5 s).
    fn refresh_progress(&mut self, now: u64, views: &[PetView], hub: &mut Hub, db: &Db, out: &mut Vec<Out>) {
        use crate::progress;
        // Whoever stands for Claude Code (or anyone) announces achievements.
        let herald = Self::camp_pet(views, "claude-code").or_else(|| views.first()).map(|v| v.id.clone());
        let cap = self.t.balance.economy.daily_cap;
        for (a, paid) in progress::check(db, now, self.t.fish.len() as u64, cap) {
            out.push(Out::Ws(crate::msg(
                "achievement",
                &json!({ "pet": herald, "id": a.id, "name": a.label(), "desc": a.about(), "coins": paid }),
            )));
        }
        // Weekly season: settle last week on the first tick of a new one.
        let scorer = |d: Option<&CampDay>| self.score(d);
        if let Some(s) = progress::settle(db, now, scorer) {
            let winner = Self::camp_pet(views, &s.winner).map(|v| v.id.clone());
            out.push(Out::Ws(crate::msg("season", &json!({ "pet": winner, "season": s }))));
        }
        let champ = progress::champion(db, now);
        hub.hats.clear();
        hub.stages.clear();
        for v in views {
            let custom = hub.lib.get(&v.pack).and_then(|p| crate::games::thresholds_of(p.evolutions.as_ref()));
            if let Some(t) = &custom {
                self.games.evo.insert(v.pack.clone(), t.clone());
            }
            let stage = self.stage_of(&v.pack, custom, db);
            if stage > 0 {
                hub.stages.insert(v.id.clone(), stage);
            }
            let season = match (champ.as_deref(), v.camp.as_deref()) {
                (Some(c), Some(camp)) if c == camp => Some("crown".to_string()),
                (Some(_), Some("claude-code" | "codex")) => Some("loser_hat".to_string()),
                _ => None,
            };
            if let Some(h) = season.or_else(|| progress::hat_of(db, &v.pack)) {
                hub.hats.insert(v.id.clone(), h);
            }
        }
        let owned = progress::owned(db);
        self.extra_quips = owned.iter().flat_map(|i| progress::quip_lines(i).iter().map(|s| s.to_string())).collect();
        self.rope = progress::rope(db);
    }

    /// "pack_bag" in the 30 minutes before off time, "dance" on Friday after 17:00.
    fn rhythm(&self, now: u64) -> Option<&'static str> {
        let (h, m) = local_hm(now);
        let (oh, om) = self
            .t
            .balance
            .work
            .off_time
            .split_once(':')
            .and_then(|(a, b)| Some((a.trim().parse::<u32>().ok()?, b.trim().parse::<u32>().ok()?)))
            .unwrap_or((18, 0));
        let mins = h * 60 + m;
        let off = oh * 60 + om;
        if mins + 30 >= off && mins < off {
            return Some("pack_bag");
        }
        if crate::db::weekday(now) == 4 && h >= 17 {
            return Some("dance");
        }
        None
    }

    /// Monday morning grumble, water and breathing reminders (once a minute).
    fn reminders(&mut self, now: u64, views: &[PetView], hub: &Hub, db: &Db, out: &mut Vec<Out>) {
        if self.frozen {
            return;
        }
        // Kegel: the user picked these times, so they ring even in focus mode.
        Self::kegel_due(now, views, hub, db, out);
        if self.focus {
            return;
        }
        let today = day_of(now);
        let (h, _) = local_hm(now);
        let Some(main) = Self::camp_pet(views, "claude-code").or_else(|| views.first()).cloned() else { return };
        // Spring Festival: wish a happy new year on 除夕 evening and on 初一.
        if let Some(which) = crate::spring::festive_day(crate::spring::today(now)) {
            if (which == "day" || h >= 18) && db.kv_get("spring_said").as_deref() != Some(today.as_str()) && matches!(main.state, State::Idle | State::Done) {
                db.kv_set("spring_said", &today);
                let bubble = if which == "eve" { crate::i18n::t("除夕快乐！恭喜发财，红包拿来 🧧", "Happy New Year's Eve! 🧧") } else { crate::i18n::t("新年快乐！恭喜发财 🧧🎆", "Happy Lunar New Year! 🧧🎆") };
                out.push(Out::Ws(crate::msg("pet.script", &json!({ "pet": main.id, "rule": "spring", "action": "cheer", "bubble": bubble, "ms": 5000, "burst": "confetti" }))));
            }
        }
        if crate::db::weekday(now) == 0 && h < 10 && db.kv_get("monday_said").as_deref() != Some(today.as_str()) && matches!(main.state, State::Idle | State::Done) {
            db.kv_set("monday_said", &today);
            out.push(Out::Ws(crate::msg("pet.script", &json!({ "pet": main.id, "rule": "rhythm", "action": "sulk", "bubble": crate::i18n::t("周一……好困", "Monday…… so sleepy"), "ms": 3500 }))));
        }
        // Water: every 60 minutes, while idle (off by default).
        if hub.cfg.features.water && matches!(main.state, State::Idle | State::Done) {
            let last: u64 = db.kv_get("water_at").and_then(|v| v.parse().ok()).unwrap_or(0);
            if now.saturating_sub(last) >= 3_600_000 {
                db.kv_set("water_at", &now.to_string());
                out.push(Out::Ws(crate::msg("pet.script", &json!({ "pet": main.id, "rule": "water", "action": "wave", "bubble": crate::i18n::t("一个小时了，你一下都没动。", "An hour. You haven't moved once."), "ms": 4000 }))));
            }
        }
        // Deep breath: more than 6 waits in 10 minutes (off by default).
        if hub.cfg.features.breathe {
            self.attention_times.retain(|t| now.saturating_sub(*t) < 10 * 60_000);
            if self.attention_times.len() > 6 && db.kv_get("breathe_at").and_then(|v| v.parse::<u64>().ok()).is_none_or(|t| now.saturating_sub(t) > 30 * 60_000) {
                db.kv_set("breathe_at", &now.to_string());
                self.attention_times.clear();
                for v in views {
                    out.push(Out::Ws(crate::msg("pet.script", &json!({ "pet": v.id, "rule": "breathe", "action": "breathe", "bubble": crate::i18n::t("我慢一点，你跟着我就行。", "I'll go slow. Just follow me."), "ms": 30_000 }))));
                }
            }
        }
    }

    /// Kegel reminder: once a day per time, any time in the hour after it
    /// (so a reminder missed while a session waited still comes).
    fn kegel_due(now: u64, views: &[PetView], hub: &Hub, db: &Db, out: &mut Vec<Out>) {
        let (h, m) = local_hm(now);
        let mins = h * 60 + m;
        let today = day_of(now);
        let mut due = false;
        for t in &hub.cfg.features.kegel {
            let Some(at) = parse_hm(t) else { continue };
            if mins < at || mins >= at + 60 {
                continue;
            }
            let key = format!("kegel:{:02}:{:02}", at / 60, at % 60);
            if db.kv_get(&key).as_deref() == Some(today.as_str()) {
                continue;
            }
            db.kv_set(&key, &today);
            due = true;
        }
        if due {
            if let Some(m) = kegel_script(views, None, hub.cfg.features.kegel_reps) {
                out.push(Out::Ws(m));
            }
        }
    }

    /// Run the progress refresh on the next tick (after a purchase).
    pub fn refresh_soon(&mut self) {
        self.last_stats = 0;
    }

    /// `/pet tug` or the right-click menu: play a tug scene now. Needs both
    /// camps online and nobody waiting on the user.
    pub fn request_tug(&mut self, hub: &Hub) -> Result<String, String> {
        let views = hub.pets();
        let online = |c: &str| views.iter().any(|v| v.camp.as_deref() == Some(c) && v.online > 0);
        if !online("claude-code") || !online("codex") {
            return Err(crate::tr!("拔河需要 Claude Code 和 Codex 都有会话在线", "Tug-of-war needs both Claude Code and Codex sessions online"));
        }
        if self.frozen {
            return Err(crate::tr!("有会话在等你，先处理它", "A session is waiting for you — handle it first"));
        }
        if hub.cfg.features.focus_mode {
            return Err(crate::tr!("专注模式下不拔河，按 Ctrl+Alt+F 关掉专注模式", "No tug-of-war in focus mode — press Ctrl+Alt+F to turn it off"));
        }
        self.tug_requested = Some(self.last_eval.max(self.last_stats));
        self.last_stats = 0;
        Ok(crate::tr!("开始拔河！", "Tug-of-war on!"))
    }

    /// Built-in reactions (tug, shadow) obey the same caps as rules.
    #[allow(clippy::too_many_arguments)]
    fn react(&mut self, camp: &str, id: &str, action: &str, bubble: &str, now: u64, views: &[PetView], db: &Db, out: &mut Vec<Out>) {
        let rule = Rule {
            id: id.into(),
            enabled: true,
            on: "builtin".into(),
            from: "self".into(),
            result: None,
            min_turn_s: 0,
            state: None,
            actor_state: vec![],
            chance: 1.0,
            cooldown: Some("10m".into()),
            action: action.into(),
            working_action: None,
            bubble: Some(crate::balance::Lines::All(vec![bubble.into()])),
            bubble_en: None,
            ms: 2500,
            movement: None,
            during_attention: false,
        };
        let Some(pet) = Self::camp_pet(views, camp) else { return };
        let key = (camp.to_string(), pet.sessions.first().map(|c| c.session.clone()).unwrap_or_default());
        self.fire(&rule, &key, now, views, db, out);
    }

    fn daily_report(&mut self, now: u64, db: &Db, out: &mut Vec<Out>) {
        let (h, m) = local_hm(now);
        let (oh, om) = self
            .t
            .balance
            .work
            .off_time
            .split_once(':')
            .and_then(|(a, b)| Some((a.trim().parse::<u32>().ok()?, b.trim().parse::<u32>().ok()?)))
            .unwrap_or((18, 0));
        if (h, m) < (oh, om) {
            return;
        }
        let today = day_of(now);
        if db.kv_get("report_shown").as_deref() == Some(today.as_str()) {
            return;
        }
        db.kv_set("report_shown", &today);
        if db.report(&today).turns > 0 {
            out.push(Out::Control(Control::Report));
        }
    }
}


/// "HH:MM" (or "H:MM") to minutes after midnight.
pub fn parse_hm(t: &str) -> Option<u32> {
    let (h, m) = t.trim().split_once(':')?;
    let (h, m): (u32, u32) = (h.trim().parse().ok()?, m.trim().parse().ok()?);
    (h < 24 && m < 60).then_some(h * 60 + m)
}

/// A guided kegel set on `pet` (default: the Claude Code pet, else any).
pub fn kegel_script(views: &[PetView], pet: Option<&str>, reps: u32) -> Option<String> {
    let reps = reps.clamp(1, 30);
    let v = match pet {
        Some(id) => views.iter().find(|v| v.id == id),
        None => World::camp_pet(views, "claude-code").or_else(|| views.first()),
    }?;
    Some(crate::msg(
        "pet.script",
        &json!({
            "pet": v.id, "rule": "kegel", "action": "kegel", "reps": reps,
            "bubble": crate::tr!("到点了。{reps} 组，我先做。", "It's time. {reps} sets — I'll go first."),
            "ms": 3000 + reps as u64 * 6000,
        }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::pack::Library;
    use crate::protocol::{Detail, Event};

    fn ev(source: &str, session: &str, kind: EventKind, ts: u64) -> Event {
        Event {
            v: 1,
            source: source.into(),
            session: session.into(),
            project: format!("/w/{session}"),
            event: kind,
            sub: false,
            detail: Detail::default(),
            pid: 0,
            ts,
        }
    }

    fn setup() -> (World, Hub, Db) {
        let mut t = Tables::builtin();
        t.balance.quips.enabled = false;
        let mut w = World::new(t, 42);
        w.games_on = false;
        (w, Hub::new(Config::default(), Library::default()), Db::in_memory())
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
    fn kegel_rings_once_per_time_within_the_hour() {
        use chrono::TimeZone;
        let (_, mut h, db) = setup();
        let at = |hh: u32, mm: u32| chrono::Local.with_ymd_and_hms(2026, 11, 3, hh, mm, 0).unwrap().timestamp_millis() as u64;
        h.cfg.features.kegel = vec!["10:30".into(), "bad".into()];
        h.cfg.features.kegel_reps = 5;
        h.apply(&ev("claude-code", "a", EventKind::TurnStart, at(10, 0)), at(10, 0));
        let views = h.pets();
        let run = |ts: u64| {
            let mut out = vec![];
            World::kegel_due(ts, &views, &h, &db, &mut out);
            msgs(&out, "pet.script")
        };
        assert!(run(at(10, 29)).is_empty(), "too early");
        let m = run(at(10, 45));
        assert_eq!(m.len(), 1, "late but within the hour");
        assert_eq!(m[0]["action"], "kegel");
        assert_eq!(m[0]["ms"], 3000 + 5 * 6000);
        assert!(run(at(10, 46)).is_empty(), "once a day");
        assert_eq!(parse_hm("9:05"), Some(545));
        assert_eq!(parse_hm("24:00"), None);
    }

    #[test]
    fn fishing_casts_after_5s_and_lands_a_fish() {
        let (mut w, mut h, db) = setup();
        let t0 = 1_793_600_000_000;
        h.apply(&ev("claude-code", "a", EventKind::TurnStart, t0), t0);
        w.tick(t0 + 1000, &mut h, &db);
        assert!(h.activities.is_empty(), "not yet");
        w.tick(t0 + 6000, &mut h, &db);
        assert_eq!(h.activities.get("claude-code").map(String::as_str), Some("fishing"));
        assert_eq!(h.pets()[0].activity.as_deref(), Some("fishing"));
        let mut done = ev("claude-code", "a", EventKind::TurnDone, t0 + 120_000);
        done.detail.result = Some(TurnResult::Ok);
        h.apply(&done, t0 + 120_000);
        w.tick(t0 + 120_100, &mut h, &db);
        let out = w.tick(t0 + 121_500, &mut h, &db);
        let fx = msgs(&out, "pet.fx");
        assert_eq!(fx.len(), 1);
        assert_eq!(fx[0]["items"][0]["outcome"], "fish");
        assert!(fx[0]["coins"].as_i64().unwrap() >= 2, "clean turn: at least 1 × 1.5 rounded");
        assert!(db.balance() >= 2);
        assert!(h.activities.is_empty(), "done: rod packed up");
    }

    #[test]
    fn error_turn_lands_a_boot_and_drops_bugs() {
        let (mut w, mut h, db) = setup();
        let t0 = 1_793_600_000_000;
        h.apply(&ev("claude-code", "a", EventKind::TurnStart, t0), t0);
        let mut fail = ev("claude-code", "a", EventKind::ToolEnd, t0 + 1000);
        fail.detail.ok = Some(false);
        h.apply(&fail, t0 + 1000);
        let mut done = ev("claude-code", "a", EventKind::TurnDone, t0 + 30_000);
        done.detail.result = Some(TurnResult::Error);
        h.apply(&done, t0 + 30_000);
        let out = w.tick(t0 + 30_000, &mut h, &db);
        assert_eq!(msgs(&out, "bug.spawn").len(), 4, "1 for the failed tool + 3 for the error turn");
        let out = w.tick(t0 + 32_000, &mut h, &db);
        let fx = msgs(&out, "pet.fx");
        assert_eq!(fx[0]["items"][0]["outcome"], "boot");
        assert_eq!(fx[0]["coins"], 0, "a boot is worth nothing");
    }

    #[test]
    fn boss_petrifies_and_squashing_it_frees_the_pet() {
        let (mut w, mut h, db) = setup();
        let t0 = 1_793_600_000_000;
        h.apply(&ev("claude-code", "a", EventKind::TurnStart, t0), t0);
        for i in 0..3 {
            let mut fail = ev("claude-code", "a", EventKind::ToolEnd, t0 + 1000 + i);
            fail.detail.ok = Some(false);
            h.apply(&fail, t0 + 1000 + i);
        }
        assert_eq!(h.pets()[0].state, State::Stone);
        w.tick(t0 + 2000, &mut h, &db);
        let boss = w.bugs.iter().find(|b| b.boss).cloned().expect("boss spawned");
        assert_eq!(boss.hp, 10);
        for _ in 0..9 {
            assert!(!w.squash(boss.id, t0 + 3000, &mut h, &db).unwrap().killed);
        }
        let r = w.squash(boss.id, t0 + 3000, &mut h, &db).unwrap();
        assert!(r.killed && r.boss);
        assert_eq!(r.coins, 5);
        assert_eq!(h.pets()[0].state, State::Thinking, "stone lifted");
    }

    #[test]
    fn every_fifth_bug_pays_a_coin() {
        let (mut w, mut h, db) = setup();
        let t0 = 1_793_600_000_000;
        h.apply(&ev("claude-code", "a", EventKind::TurnStart, t0), t0);
        for i in 0..5 {
            let mut fail = ev("claude-code", "a", EventKind::ToolEnd, t0 + i);
            fail.detail.ok = Some(false);
            h.apply(&fail, t0 + i);
            let mut ok = ev("claude-code", "a", EventKind::ToolEnd, t0 + i);
            ok.detail.ok = Some(true);
            h.apply(&ok, t0 + i);
        }
        w.tick(t0 + 10, &mut h, &db);
        let ids: Vec<u64> = w.bugs.iter().map(|b| b.id).collect();
        assert_eq!(ids.len(), 5);
        let coins: Vec<i64> = ids.iter().map(|id| w.squash(*id, t0 + 20, &mut h, &db).unwrap().coins).collect();
        assert_eq!(coins, [0, 0, 0, 0, 1]);
    }

    #[test]
    fn attention_freezes_gameplay_but_urge_still_runs() {
        let (mut w, mut h, db) = setup();
        let t0 = 1_793_600_000_000;
        h.apply(&ev("claude-code", "a", EventKind::SessionStart, t0), t0);
        h.apply(&ev("codex", "c", EventKind::TurnStart, t0), t0);
        w.tick(t0, &mut h, &db);
        // Codex waits on the user: Claude Code (idle) runs over to urge.
        h.apply(&ev("codex", "c", EventKind::Attention, t0 + 1000), t0 + 1000);
        let out = w.tick(t0 + 1000, &mut h, &db);
        assert!(w.frozen);
        let scripts = msgs(&out, "pet.script");
        assert_eq!(scripts.len(), 1);
        assert_eq!(scripts[0]["rule"], "urge");
        assert!(out.iter().any(|o| matches!(o, Out::Control(Control::Move { target: Some(_), .. }))));
        // A Claude Code error now would normally invite a taunt from Codex,
        // but everything else is frozen.
        let mut done = ev("claude-code", "a", EventKind::TurnDone, t0 + 2000);
        done.detail.result = Some(TurnResult::Error);
        h.apply(&done, t0 + 2000);
        for i in 0..50 {
            let out = w.tick(t0 + 2000 + i, &mut h, &db);
            assert!(msgs(&out, "pet.script").is_empty());
        }
        // Squashing is paused too.
        let id = w.bugs[0].id;
        assert!(w.squash(id, t0 + 3000, &mut h, &db).is_err());
    }

    #[test]
    fn no_more_than_six_interactions_per_pet_per_hour() {
        let (mut w, mut h, db) = setup();
        for r in &mut w.t.rules {
            r.chance = 1.0;
            r.cooldown = None;
        }
        let t0 = 1_793_600_000_000;
        h.cfg.features.arena = false; // isolate the rule engine from tug scenes
        h.apply(&ev("claude-code", "a", EventKind::SessionStart, t0), t0);
        let mut fired = 0;
        for i in 0..20u64 {
            let ts = t0 + i * 60_000;
            h.apply(&ev("codex", "c", EventKind::TurnStart, ts), ts);
            let mut done = ev("codex", "c", EventKind::TurnDone, ts + 10);
            done.detail.result = Some(TurnResult::Error);
            h.apply(&done, ts + 10);
            let out = w.tick(ts + 10, &mut h, &db);
            fired += msgs(&out, "pet.script").iter().filter(|s| s["pet"] == "claude-code").count();
            w.tick(ts + 50_000, &mut h, &db); // let the script finish
        }
        assert_eq!(fired, 6);
    }

    #[test]
    fn tug_scene_uses_the_real_pets_when_the_score_moves() {
        let (mut w, mut h, db) = setup();
        let t0 = 1_793_600_000_000;
        h.apply(&ev("claude-code", "a", EventKind::SessionStart, t0), t0);
        h.apply(&ev("codex", "c", EventKind::SessionStart, t0), t0);
        // Both camps just came online: an opening bout, even at 0 : 0.
        let out = w.tick(t0, &mut h, &db);
        let scripts = msgs(&out, "pet.script");
        assert_eq!(scripts.len(), 2, "both main pets take part");
        assert!(scripts.iter().all(|s| s["rule"] == "tug" && s["tug"]["lead"].is_null()));
        assert!(out.iter().any(|o| matches!(o, Out::Control(Control::Tug { mover, anchor }) if mover == "codex" && anchor == "claude-code")));
        let turn = |r| crate::session::FinishedTurn { started_at: t0, ended_at: t0 + 1000, result: r, tool_calls: 1, tool_errors: 0, attentions: 0 };
        for _ in 0..3 {
            db.record_turn("claude-code", "a", "/p", &turn(TurnResult::Ok));
        }
        // Cooldown: the score moved, but not within 10 minutes of the last bout.
        let out = w.tick(t0 + 60_000, &mut h, &db);
        assert!(msgs(&out, "pet.script").iter().all(|s| s["rule"] != "tug"));
        // After the cooldown the moved score earns a new bout, Claude Code leading.
        let out = w.tick(t0 + 11 * 60_000, &mut h, &db);
        let scripts = msgs(&out, "pet.script");
        let cc = scripts.iter().find(|s| s["pet"] == "claude-code" && s["rule"] == "tug").expect("second bout");
        assert_eq!(cc["tug"]["lead"], true);
        assert!(cc["bubble"].as_str().unwrap().contains("1.00 : 0.00"));
        // On request: right away, cooldown or not.
        // On request: waits for the running bout to end, then plays despite the cooldown.
        w.request_tug(&h).unwrap();
        let out = w.tick(t0 + 11 * 60_000 + 100, &mut h, &db);
        assert!(msgs(&out, "pet.script").iter().all(|s| s["rule"] != "tug"), "pets still busy");
        let mut played = false;
        for i in 0..=3 {
            let out = w.tick(t0 + 11 * 60_000 + 6_000 + i * 5_000, &mut h, &db);
            played |= msgs(&out, "pet.script").iter().any(|s| s["rule"] == "tug");
        }
        assert!(played, "requested bout plays once the pets are free");
    }

    #[test]
    fn halving_after_ten_fish_and_daily_cap() {
        let (mut w, _h, db) = setup();
        let t0 = 1_793_600_000_000;
        let key = ("claude-code".to_string(), "a".to_string());
        w.t.balance.fishing.odds = vec![crate::balance::Odds { under_min: 0, p: [0, 0, 0, 100] }];
        let mut coins = vec![];
        for i in 0..12 {
            coins.push(w.land(&key, TurnResult::Ok, false, 60_000, t0 + i, &db).coins);
        }
        assert_eq!(&coins[..10], &[20; 10]);
        assert_eq!(coins[10], 10, "11th fish in the hour: half");
        assert_eq!(coins[11], 10);
        assert_eq!(db.income_on(&day_of(t0)), 220);
        let more: i64 = (0..20).map(|i| w.land(&key, TurnResult::Ok, false, 60_000, t0 + 100 + i, &db).coins).sum();
        assert_eq!(more, 80, "capped at 300 for the day");
    }

    #[test]
    fn tug_offset_follows_efficiency() {
        let (mut w, mut h, db) = setup();
        let t0 = 1_793_600_000_000;
        h.apply(&ev("claude-code", "a", EventKind::TurnStart, t0), t0);
        h.apply(&ev("codex", "c", EventKind::TurnStart, t0), t0);
        let turn = |r| crate::session::FinishedTurn { started_at: t0, ended_at: t0 + 1000, result: r, tool_calls: 1, tool_errors: 0, attentions: 0 };
        for _ in 0..4 {
            db.record_turn("claude-code", "a", "/p", &turn(TurnResult::Ok));
        }
        for r in [TurnResult::Ok, TurnResult::Error, TurnResult::Error] {
            db.record_turn("codex", "c", "/p", &turn(r));
        }
        let out = w.tick(t0 + 10_000, &mut h, &db);
        let st = msgs(&out, "world.state");
        let off = st.last().unwrap()["tug"]["offset"].as_f64().unwrap();
        // A = 1, B = (1 - 2) / 3 = -1/3 -> (A - B) / 2 = 2/3
        assert!((off - 2.0 / 3.0).abs() < 1e-9, "{off}");
        assert!(st.last().unwrap()["shadow"].is_null());
    }
}
