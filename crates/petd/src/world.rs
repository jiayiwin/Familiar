//! The world layer: what the pets do with real agent events.
//!
//!   fishing      a pet whose sessions work for 5 s casts a line; each finished
//!                turn lands a fish (by turn length), a boot (error) or nothing
//!   bugs         every failed tool drops a bug, an error turn drops 3, three
//!                failures in a row drop a Boss that petrifies the pet until it
//!                is squashed
//!   quips        working pets mutter now and then
//!   interactions YAML rules (none built in; users can add their own)
//!   the bond     moments worth remembering, and the lines they come back as
//!
//! Hard rules: while any session waits on the user everything freezes except
//! reminder rules (`during_attention`), and no pet starts more than
//! `per_pet_hour` interactions in an hour.

use crate::balance::{Rule, Tables, RARITIES};
use crate::db::{day_of, local_hm, Db};
use crate::hub::{Feed, Hub, Key, PetView};
use crate::protocol::{EventKind, TurnResult};
use crate::session::State;
use crate::Control;
use serde::Serialize;
use serde_json::json;
use std::collections::{HashMap, VecDeque};

const CATCH_BATCH_MS: u64 = 1200;
const STATS_EVERY_MS: u64 = 5000;

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
}

/// Messages for windows (WS) and the host app.
#[derive(Debug, Clone)]
pub enum Out {
    Ws(String),
    Control(Control),
}

#[derive(Default)]
pub(crate) struct SessionTrace {
    consec_errors: u32,
    noted_late_night: bool,
    noted_error_burst: bool,
    noted_stone: bool,
    /// The "next step" its last turn named; written down for you when the session ends.
    next: Option<(String, String)>,
}

pub struct World {
    pub t: Tables,
    /// The kind of sweet the last feed took out of the jar (its look goes to the window that animates it).
    pub last_fed: Option<String>,
    pub bugs: Vec<Bug>,
    pub(crate) next_id: u64,
    pub(crate) working_since: HashMap<String, u64>,
    pub(crate) pending: HashMap<String, (u64, Vec<CatchFx>)>,
    pub(crate) cooldowns: HashMap<(String, String), u64>,
    pub(crate) recent: HashMap<String, VecDeque<u64>>,
    pub(crate) scripts: HashMap<String, Script>,
    pub(crate) quip_next: HashMap<String, u64>,
    pub(crate) quip_used: HashMap<String, u64>,
    pub(crate) file_edits: HashMap<(Key, String), u32>,
    pub(crate) rng: u64,
    pub frozen: bool,
    pub(crate) focus: bool,
    /// `features.sounds`: windows play their instruments and alert notes.
    pub(crate) sounds: bool,
    pub(crate) last_eval: u64,
    pub(crate) last_stats: u64,
    pub(crate) last_minute: u64,
    pub(crate) last_state: String,
    /// `features.chatter`: 0 quiet, 1 normal, 2 lively.
    pub(crate) chatter: u8,
    /// Per-session event tracking for moment detection.
    pub(crate) session_traces: HashMap<Key, SessionTrace>,
}

impl World {
    pub fn new(t: Tables, seed: u64) -> World {
        World {
            t,
            last_fed: None,
            bugs: vec![],
            next_id: 1,
            working_since: HashMap::new(),
            pending: HashMap::new(),
            cooldowns: HashMap::new(),
            recent: HashMap::new(),
            scripts: HashMap::new(),
            quip_next: HashMap::new(),
            quip_used: HashMap::new(),
            file_edits: HashMap::new(),
            rng: seed | 1,
            frozen: false,
            focus: false,
            sounds: true,
            last_eval: 0,
            last_stats: 0,
            last_minute: 0,
            last_state: String::new(),
            chatter: 2,
            session_traces: HashMap::new(),
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
        views
            .iter()
            .find(|v| v.id == camp)
            .or_else(|| views.iter().find(|v| v.camp.as_deref() == Some(camp)))
    }

    pub(crate) fn pet_for(views: &[PetView], key: &Key) -> Option<PetView> {
        let skey = format!("{}/{}", key.0, key.1);
        views
            .iter()
            .find(|v| v.sessions.iter().any(|c| c.key == skey))
            .or_else(|| Self::camp_pet(views, &key.0))
            .cloned()
    }

    /// An ended session's last "next step" goes into 手上的事, marked as the agent's.
    fn leave_next(&mut self, key: &Key, next: Option<(String, String)>, views: &[PetView], now: u64, db: &Db, out: &mut Vec<Out>) {
        let Some((text, project)) = next else { return };
        let project = std::path::Path::new(&project).file_name().map(|n| n.to_string_lossy().into_owned()).filter(|p| !p.is_empty());
        let pet = Self::pet_for(views, key).map(|v| v.id);
        if let Some(t) = crate::todo::add(db, &text, &key.0, project.as_deref(), pet.as_deref(), None, now) {
            out.push(Out::Ws(crate::msg("todo.changed", &json!({ "added": t, "pet": pet, "open": crate::todo::open_count(db) }))));
        }
    }

    pub fn tick(&mut self, now: u64, hub: &mut Hub, db: &Db) -> Vec<Out> {
        let mut out = Vec::new();
        let feed = std::mem::take(&mut hub.feed);
        let views = hub.pets();
        self.focus = hub.cfg.features.focus_mode;
        self.sounds = hub.cfg.features.sounds;
        self.chatter = match hub.cfg.features.chatter.as_str() {
            "quiet" => 0,
            "lively" => 2,
            _ => 1,
        };

        // Freeze while anyone waits on the user.
        let frozen = hub.sessions.values().any(|s| s.state == State::Attention);
        if frozen && !self.frozen {
            let ids: Vec<String> = self
                .scripts
                .iter()
                .filter(|(_, s)| !s.during_attention)
                .map(|(k, _)| k.clone())
                .collect();
            for id in ids {
                self.cancel(&id, &mut out);
            }
        }
        self.frozen = frozen;
        // A pet that itself starts waiting drops whatever it was doing.
        let waiting: Vec<String> = self
            .scripts
            .keys()
            .filter(|id| {
                views
                    .iter()
                    .any(|v| &v.id == *id && v.state == State::Attention)
            })
            .cloned()
            .collect();
        for id in waiting {
            self.cancel(&id, &mut out);
        }

        for f in feed {
            match f {
                Feed::Finished { key, project, fin } => {
                    db.record_turn(&key.0, &key.1, &project, &fin);
                    // Night owl: one gentle reminder per night.
                    let (h, _) = local_hm(now);
                    if !(4..23).contains(&h)
                        && !self.focus
                        && db.kv_get("night_reminded").as_deref() != Some(day_of(now).as_str())
                    {
                        if let Some(pet) = Self::pet_for(&views, &key) {
                            db.kv_set("night_reminded", &day_of(now));
                            out.push(Out::Ws(crate::msg("pet.script", &json!({ "pet": pet.id, "rule": "night", "bubble": crate::i18n::t("外面早黑透了。我陪到你关机。", "It's been dark out for hours. I'll stay till you shut down."), "ms": 5000 }))));
                        }
                    }
                    let ms = fin.ended_at.saturating_sub(fin.started_at);
                    if !self.focus && ms >= self.t.balance.fishing.min_turn_s * 1000 {
                        if let Some(pet) = Self::pet_for(&views, &key) {
                            let fx = self.land(&key, fin.result, ms, now, db);
                            let e = self.pending.entry(pet.id.clone()).or_insert((now, vec![]));
                            e.1.push(fx);
                        }
                    }
                    // What waited on this agent is ready to be brought up (you stopped it yourself: you are there already).
                    if fin.result != TurnResult::Aborted {
                        crate::todo::agent_stopped(db, &key.0);
                    }
                    if fin.result != TurnResult::Aborted {
                        let is_error = fin.result == TurnResult::Error || fin.tool_errors > 0;
                        let trust_delta = if is_error { -0.3 } else { 0.2 };
                        db.bond_update(0.1, trust_delta, 0.0, now);
                        let frag_delay_a = 7_200_000u64 + (self.rand() * 68_400_000.0) as u64;
                        let frag_delay_b = 7_200_000u64 + (self.rand() * 68_400_000.0) as u64;
                        let trace = self.session_traces.entry(key.clone()).or_default();
                        if is_error {
                            trace.consec_errors += 1;
                        } else {
                            trace.consec_errors = 0;
                        }
                        if !(4..23).contains(&h) && !trace.noted_late_night {
                            trace.noted_late_night = true;
                            db.log_bond_moment(now, "late_night", "{}");
                            db.queue_fragment(now, now + frag_delay_a, "late_night");
                        }
                        if trace.consec_errors >= 3 && !trace.noted_error_burst {
                            trace.noted_error_burst = true;
                            db.log_bond_moment(
                                now,
                                "error_burst",
                                &format!("{{\"count\":{}}}", trace.consec_errors),
                            );
                            db.queue_fragment(now, now + frag_delay_b, "error_burst");
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
                        EventKind::ToolEnd
                            if ev.detail.ok == Some(false) && ev.detail.interrupt != Some(true) =>
                        {
                            let n = self.t.balance.bugs.per_tool_fail;
                            self.spawn_bugs(&key, n, false, now, &views, &mut out);
                        }
                        EventKind::TurnDone if ev.detail.result == Some(TurnResult::Error) => {
                            let n = self.t.balance.bugs.per_turn_error;
                            self.spawn_bugs(&key, n, false, now, &views, &mut out);
                        }
                        EventKind::TurnDone => {
                            if let Some(next) = ev.detail.next.clone().filter(|n| !n.is_empty()) {
                                self.session_traces.entry(key.clone()).or_default().next = Some((next, ev.project.clone()));
                            }
                        }
                        EventKind::SessionEnd => {
                            self.file_edits.retain(|(k, _), _| k != &key);
                            if let Some(t) = self.session_traces.remove(&key) {
                                self.leave_next(&key, t.next, &views, now, db, &mut out);
                            }
                        }
                        _ => {}
                    }
                }
                Feed::State { key, to, from } => {
                    // The agent's process went away without saying goodbye: its next step is still written down.
                    if to == State::Offline {
                        let next = self.session_traces.get_mut(&key).and_then(|t| t.next.take());
                        self.leave_next(&key, next, &views, now, db, &mut out);
                    }
                    if to == State::Stone && from != State::Stone {
                        let has_boss = self
                            .bugs
                            .iter()
                            .any(|b| b.boss && b.session.as_ref() == Some(&key));
                        if !has_boss {
                            self.spawn_bugs(&key, 1, true, now, &views, &mut out);
                        }
                        let frag_delay = 7_200_000u64 + (self.rand() * 68_400_000.0) as u64;
                        let trace = self.session_traces.entry(key.clone()).or_default();
                        if !trace.noted_stone {
                            trace.noted_stone = true;
                            db.log_bond_moment(now, "stone", "{}");
                            db.queue_fragment(now, now + frag_delay, "stone");
                        }
                        db.bond_update(0.0, -1.0, -2.0, now);
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
                if working
                    && !self.focus
                    && !self.frozen
                    && now.saturating_sub(since) >= start_after
                {
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
                    if matches!(v.state, State::Idle | State::Done)
                        && !hub.activities.contains_key(&v.id)
                    {
                        hub.activities.insert(v.id.clone(), act.to_string());
                    }
                }
            }
        }

        // Catches landed close together reel in as one animation ("+N").
        if !self.frozen {
            let due: Vec<String> = self
                .pending
                .iter()
                .filter(|(_, (t0, _))| now.saturating_sub(*t0) >= CATCH_BATCH_MS)
                .map(|(k, _)| k.clone())
                .collect();
            for pet in due {
                if let Some((_, items)) = self.pending.remove(&pet) {
                    out.push(Out::Ws(crate::msg(
                        "pet.fx",
                        &json!({ "pet": pet, "kind": "catch", "items": items }),
                    )));
                }
            }
        }

        // Finished scripts: walk home if the pet went somewhere.
        let done: Vec<String> = self
            .scripts
            .iter()
            .filter(|(_, s)| now >= s.until)
            .map(|(k, _)| k.clone())
            .collect();
        for id in done {
            if let Some(s) = self.scripts.remove(&id) {
                if s.moved {
                    out.push(Out::Control(Control::Move {
                        pet: id,
                        target: None,
                    }));
                }
            }
        }

        if now.saturating_sub(self.last_eval) >= self.t.balance.interactions.tick_ms {
            self.last_eval = now;
            self.quips(now, &views, &mut out);
        }
        if now.saturating_sub(self.last_stats) >= STATS_EVERY_MS {
            self.last_stats = now;
            // For the report: the most sessions online at once.
            db.note_online(now, views.iter().map(|v| v.online).sum());
            self.daily_report(now, db, &mut out);
        }
        if now.saturating_sub(self.last_minute) >= 60_000 {
            self.last_minute = now;
            if !self.frozen && !self.focus {
                self.monday(now, &views, db, &mut out);
            }
        }

        let state = self.state_msg();
        if state != self.last_state {
            out.push(Out::Ws(state.clone()));
            self.last_state = state;
        }

        // Deliver due fragments as brief bubbles
        let frags = db.due_fragments(now);
        for (fid, moment_kind) in frags {
            db.mark_fragment_delivered(fid);
            if let Some(pet) = views.first() {
                let pick = (self.rand() * 10.0) as usize;
                let text = fragment_text(&moment_kind, pick);
                if !text.is_empty() {
                    out.push(Out::Ws(crate::msg(
                        "pet.script",
                        &serde_json::json!({
                            "pet": pet.id, "rule": "fragment", "bubble": text, "ms": 7000
                        }),
                    )));
                }
            }
        }

        out
    }

    /// The world state windows need: also sent whole to a window that just connected.
    pub fn state_msg(&self) -> String {
        crate::msg(
            "world.state",
            &json!({ "frozen": self.frozen, "focus": self.focus, "sounds": self.sounds, "bugs": self.bugs }),
        )
    }

    pub(crate) fn cancel(&mut self, pet: &str, out: &mut Vec<Out>) {
        if let Some(s) = self.scripts.remove(pet) {
            out.push(Out::Ws(crate::msg(
                "pet.cancel",
                &json!({ "pet": pet, "rule": s.rule }),
            )));
            if s.moved {
                // Teleport home: the reminder must not wait for a walk.
                out.push(Out::Control(Control::Move {
                    pet: pet.to_string(),
                    target: None,
                }));
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
    fn land(&mut self, key: &Key, result: TurnResult, ms: u64, now: u64, db: &Db) -> CatchFx {
        match result {
            TurnResult::Error => {
                db.add_catch(now, "boot", "", "", &key.0, &key.1);
                CatchFx {
                    outcome: "boot".into(),
                    fish_id: String::new(),
                    name: crate::i18n::t("空糖纸", "An empty wrapper").into(),
                    rarity: String::new(),
                }
            }
            TurnResult::Aborted => {
                db.add_catch(now, "escape", "", "", &key.0, &key.1);
                CatchFx {
                    outcome: "escape".into(),
                    fish_id: String::new(),
                    name: String::new(),
                    rarity: String::new(),
                }
            }
            TurnResult::Ok => self.catch(ms / 60_000, &key.0, &key.1, now, db),
        }
    }

    /// One sweet into the jar. `minutes` is how long it took to earn (longer
    /// runs land rarer ones). Which one depends on the moment: where it comes
    /// from (an agent's turn, a kegel set, a break), the hour, the season. One
    /// that belongs to this moment is three times as likely as an everyday one.
    fn catch(&mut self, minutes: u64, source: &str, session: &str, now: u64, db: &Db) -> CatchFx {
        use chrono::{Datelike, Timelike};
        let rarity = self.roll_rarity(minutes);
        let from = if source == "you" { session } else { "agent" };
        let when = chrono::DateTime::from_timestamp_millis(now as i64)
            .map(|t| t.with_timezone(&chrono::Local))
            .unwrap_or_else(chrono::Local::now);
        let (hour, month) = (when.hour() as u8, when.month() as u8);
        let mut pool: Vec<crate::balance::Fish> = vec![];
        for f in self
            .t
            .fish_of(rarity)
            .into_iter()
            .filter(|f| f.fits(from, hour, month))
        {
            for _ in 0..if f.special() { 3 } else { 1 } {
                pool.push(f.clone());
            }
        }
        // What you earn yourself is its own thing whatever the roll: a plum for the set, a mint leaf for the break.
        if source == "you" {
            if let Some(own) = self
                .t
                .fish
                .iter()
                .find(|f| f.from.as_deref() == Some(session))
            {
                pool = vec![own.clone()];
            }
        }
        let rarity = pool
            .first()
            .map(|f| {
                crate::balance::RARITIES
                    .iter()
                    .copied()
                    .find(|r| *r == f.rarity)
                    .unwrap_or(rarity)
            })
            .unwrap_or(rarity);
        let fish = self
            .pick(&pool)
            .cloned()
            .unwrap_or_else(|| crate::balance::Fish {
                id: "fish".into(),
                name: crate::i18n::t("水果糖", "Fruit Drop").into(),
                rarity: rarity.into(),
                note: String::new(),
                shape: "drop".into(),
                colors: vec!["#f6b45e".into(), "#fbe2b0".into()],
                from: None,
                hours: None,
                months: None,
                name_en: None,
                note_en: None,
            });
        db.add_catch(now, "fish", &fish.id, rarity, source, session);
        CatchFx {
            outcome: "fish".into(),
            fish_id: fish.id,
            name: fish.name,
            rarity: rarity.into(),
        }
    }

    /// A fish you earned yourself, with no agent involved: `why` is "kegel" (a
    /// set done to the end) or "break" (up from a long sit).
    pub fn earn(&mut self, why: &str, now: u64, db: &Db) -> CatchFx {
        self.catch(0, "you", why, now, db)
    }

    /// "喂它": the newest fish in the basket goes to the pet. What it says,
    /// what that earned you (a whisper, a secret, a letter), and the feeds so far.
    /// `who` is which of the five eats it (`zimeng`, `mashu`, …): it answers in its own voice.
    pub fn feed(
        &mut self,
        pet: &str,
        who: &str,
        now: u64,
        db: &Db,
    ) -> (String, crate::bond::BondReward, i64) {
        let Some(kind) = db.eat_one() else {
            return (
                crate::voice::line(who, "no_fish", 0),
                crate::bond::BondReward::NoFish,
                0,
            );
        };
        self.last_fed = Some(kind);
        let (bond_text, reward, feed_count) = crate::bond::on_feed(db, crate::i18n::is_en(), who);
        if matches!(reward, crate::bond::BondReward::Letter { .. }) {
            db.log_event(now, "letter", pet, 1);
        }
        let text = if bond_text.is_empty() {
            crate::voice::line(who, "mm", 0)
        } else {
            bond_text
        };
        (text, reward, feed_count)
    }

    // ------------------------------------------------------------ bugs

    fn spawn_bugs(
        &mut self,
        key: &Key,
        n: u32,
        boss: bool,
        now: u64,
        views: &[PetView],
        out: &mut Vec<Out>,
    ) {
        if self.focus {
            return;
        }
        let Some(pet) = Self::pet_for(views, key) else {
            return;
        };
        for _ in 0..n {
            let hp = if boss { self.t.balance.bugs.boss_hp } else { 1 };
            let bug = Bug {
                id: self.next_id,
                boss,
                hp,
                max_hp: hp,
                pet: pet.id.clone(),
                born: now,
                session: Some(key.clone()),
            };
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
    pub fn squash(
        &mut self,
        id: u64,
        now: u64,
        hub: &mut Hub,
        db: &Db,
    ) -> Result<SquashResult, String> {
        if self.frozen {
            return Err(crate::tr!(
                "有会话在等你，先去处理它",
                "A session is waiting for you — go handle it first"
            ));
        }
        let i = self
            .bugs
            .iter()
            .position(|b| b.id == id)
            .ok_or_else(|| crate::tr!("这只虫已经不在了", "That bug is already gone"))?;
        self.bugs[i].hp = self.bugs[i].hp.saturating_sub(1);
        let b = self.bugs[i].clone();
        if b.hp > 0 {
            return Ok(SquashResult {
                id,
                hp: b.hp,
                killed: false,
                boss: b.boss,
            });
        }
        self.bugs.remove(i);
        if let (true, Some(k)) = (b.boss, &b.session) {
            hub.clear_stone(k, now);
        }
        db.record_bug_kill(now, b.boss);
        Ok(SquashResult {
            id,
            hp: 0,
            killed: true,
            boss: b.boss,
        })
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
    fn rules_turn_done(
        &mut self,
        key: &Key,
        result: TurnResult,
        ms: u64,
        now: u64,
        views: &[PetView],
        db: &Db,
        out: &mut Vec<Out>,
    ) {
        let res = match result {
            TurnResult::Ok => "ok",
            TurnResult::Error => "error",
            TurnResult::Aborted => "aborted",
        };
        let rules: Vec<Rule> = self
            .t
            .rules
            .iter()
            .filter(|r| {
                r.on == "turn_done"
                    && r.result.as_deref().is_none_or(|x| x == res)
                    && ms >= r.min_turn_s * 1000
            })
            .cloned()
            .collect();
        for r in rules {
            self.fire(&r, key, now, views, db, out);
        }
    }

    fn rules_state(
        &mut self,
        key: &Key,
        to: State,
        now: u64,
        views: &[PetView],
        db: &Db,
        out: &mut Vec<Out>,
    ) {
        let name = serde_json::to_value(to)
            .ok()
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_default();
        let rules: Vec<Rule> = self
            .t
            .rules
            .iter()
            .filter(|r| r.on == "state" && r.state.as_deref() == Some(name.as_str()))
            .cloned()
            .collect();
        for r in rules {
            self.fire(&r, key, now, views, db, out);
        }
    }

    /// Hourly cap check without recording. `interactions.per_pet_hour` is the
    /// lively figure; normal chatter allows half of it, quiet one.
    pub(crate) fn under_cap(&mut self, pet: &str, now: u64) -> bool {
        let full = self.t.balance.interactions.per_pet_hour;
        let cap = match self.chatter {
            0 => full.min(1),
            1 => full.div_ceil(2),
            _ => full,
        };
        let q = self.recent.entry(pet.to_string()).or_default();
        while q
            .front()
            .is_some_and(|t| now.saturating_sub(*t) >= 3_600_000)
        {
            q.pop_front();
        }
        q.len() < cap
    }

    pub(crate) fn fire(
        &mut self,
        r: &Rule,
        key: &Key,
        now: u64,
        views: &[PetView],
        db: &Db,
        out: &mut Vec<Out>,
    ) {
        if self.focus || (self.frozen && !r.during_attention) {
            return;
        }
        let target = Self::pet_for(views, key);
        let actor = match r.from.as_str() {
            "self" => target.clone(),
            _ => Self::other_camp(&key.0)
                .and_then(|c| Self::camp_pet(views, c))
                .cloned(),
        };
        let Some(actor) = actor else { return };
        if self.scripts.contains_key(&actor.id) || actor.state == State::Attention {
            return;
        }
        if target.as_ref().is_some_and(|t| t.id == actor.id) && r.from != "self" {
            return;
        }
        let actor_state = serde_json::to_value(actor.state)
            .ok()
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_default();
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
        self.recent
            .entry(actor.id.clone())
            .or_default()
            .push_back(now);
        let working = matches!(actor.state, State::Thinking | State::Tool | State::Reading);
        let action = if working {
            r.working_action.clone().unwrap_or_else(|| r.action.clone())
        } else {
            r.action.clone()
        };
        let camp = actor.camp.clone().unwrap_or_default();
        let lines: Vec<String> = r
            .bubble
            .as_ref()
            .map(|b| b.for_camp(&camp).to_vec())
            .unwrap_or_default();
        let bubble = self.pick(&lines).cloned();
        let moved = r.movement.as_deref() == Some("to_target") && target.is_some();
        let walk_ms = if moved { 1200 } else { 0 };
        self.scripts.insert(
            actor.id.clone(),
            Script {
                rule: r.id.clone(),
                until: now + r.ms + walk_ms,
                during_attention: r.during_attention,
                moved,
            },
        );
        if moved {
            out.push(Out::Control(Control::Move {
                pet: actor.id.clone(),
                target: target.as_ref().map(|t| t.id.clone()),
            }));
        }
        out.push(Out::Ws(crate::msg(
            "pet.script",
            &json!({ "pet": actor.id, "rule": r.id, "action": action, "bubble": bubble, "ms": r.ms, "delay": walk_ms }),
        )));
        db.log_interaction(now, &actor.id, &r.id, self.frozen);
    }

    // ------------------------------------------------------------ quips

    fn quips(&mut self, now: u64, views: &[PetView], out: &mut Vec<Out>) {
        let q = self.t.balance.quips.clone();
        // `min_s`..`max_s` is the lively pace; normal chatter leaves 2.5 times as long between lines.
        if self.chatter == 0 {
            self.quip_next.clear();
            return;
        }
        let stretch = if self.chatter == 1 { 5 } else { 2 };
        if !q.enabled || self.focus || self.frozen {
            return;
        }
        for v in views {
            let working = matches!(v.state, State::Thinking | State::Tool | State::Reading);
            if !working || self.scripts.contains_key(&v.id) {
                self.quip_next.remove(&v.id);
                continue;
            }
            let gap = (q.min_s * 1000
                + (self.rand() * ((q.max_s.saturating_sub(q.min_s)) * 1000) as f64) as u64)
                * stretch
                / 2;
            let due = *self.quip_next.entry(v.id.clone()).or_insert(now + gap);
            if now < due {
                continue;
            }
            self.quip_next.insert(v.id.clone(), now + gap);
            // Variables come from the busiest session of this pet.
            let card = v
                .sessions
                .iter()
                .find(|c| matches!(c.state, State::Tool | State::Reading | State::Thinking));
            let file = card.and_then(|c| c.file.clone());
            let tool = card.and_then(|c| c.tool.clone());
            let project = card.map(|c| c.project.clone()).filter(|p| !p.is_empty());
            let n = card.and_then(|c| {
                let key = (c.source.clone(), c.session.clone());
                file.as_ref()
                    .and_then(|f| self.file_edits.get(&(key, f.clone())).copied())
            });
            let fill = |line: &str| -> Option<String> {
                let mut s = line.to_string();
                for (k, v) in [
                    ("{file}", file.clone()),
                    ("{tool}", tool.clone()),
                    ("{project}", project.clone()),
                    ("{n}", n.map(|n| n.to_string())),
                ] {
                    if s.contains(k) {
                        s = s.replace(k, &v?);
                    }
                }
                Some(s)
            };
            let fresh = q.no_repeat_min * 60_000;
            let options: Vec<String> = q
                .lines
                .iter()
                .filter(|l| {
                    self.quip_used
                        .get(*l)
                        .is_none_or(|t| now.saturating_sub(*t) >= fresh)
                })
                .filter(|l| !l.contains("{n}") || n.is_some_and(|n| n >= 3))
                .filter_map(|l| fill(l).map(|text| (l.clone(), text)))
                .map(|(l, t)| format!("{l}\u{1}{t}"))
                .collect();
            if let Some(choice) = self.pick(&options).cloned() {
                let (line, text) = choice.split_once('\u{1}').unwrap();
                self.quip_used.insert(line.to_string(), now);
                out.push(Out::Ws(crate::msg(
                    "pet.script",
                    &json!({ "pet": v.id, "rule": "quip", "bubble": text, "ms": 4000 }),
                )));
            }
        }
    }

    // ------------------------------------------------------------ the day

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

    /// Monday morning, once: the main pet says how it feels about it.
    fn monday(&mut self, now: u64, views: &[PetView], db: &Db, out: &mut Vec<Out>) {
        let today = day_of(now);
        let (h, _) = local_hm(now);
        let Some(main) = Self::camp_pet(views, "claude-code").or_else(|| views.first()) else {
            return;
        };
        if crate::db::weekday(now) == 0
            && h < 10
            && db.kv_get("monday_said").as_deref() != Some(today.as_str())
            && matches!(main.state, State::Idle | State::Done)
        {
            db.kv_set("monday_said", &today);
            out.push(Out::Ws(crate::msg("pet.script", &json!({ "pet": main.id, "rule": "rhythm", "action": "sulk", "bubble": crate::i18n::t("周一……好困", "Monday…… so sleepy"), "ms": 3500 }))));
        }
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
            "bubble": crate::tr!("{reps} 下。我先做。", "{reps} reps. I'll go first."),
            "ms": 3000 + reps as u64 * 6000,
        }),
    ))
}

fn fragment_text(kind: &str, pick_idx: usize) -> &'static str {
    let en = crate::i18n::is_en();
    match (kind, en) {
        ("late_night", false) => {
            let v = [
                "那么晚了你还在。",
                "屏幕很亮。",
                "我没睡。",
                "深夜是另一种白天。",
            ];
            v[pick_idx % v.len()]
        }
        ("late_night", true) => {
            let v = [
                "Still here.",
                "The screen was bright.",
                "I didn't sleep either.",
            ];
            v[pick_idx % v.len()]
        }
        ("error_burst", false) => {
            let v = [
                "那一串，我数了的。",
                "后来好了就是。",
                "没事。",
                "我都看见了。",
            ];
            v[pick_idx % v.len()]
        }
        ("error_burst", true) => {
            let v = ["I counted those.", "Then it worked.", "It's okay."];
            v[pick_idx % v.len()]
        }
        ("stone", false) => {
            let v = ["卡住的时候我也不知道说什么。", "后来就过去了。", "我在。"];
            v[pick_idx % v.len()]
        }
        ("stone", true) => {
            let v = [
                "I didn't know what to say either.",
                "Then it passed.",
                "I was there.",
            ];
            v[pick_idx % v.len()]
        }
        ("radio_on", false) => {
            let v = ["你在听的时候我也听见了。", "那台不错。", "背景音很好。"];
            v[pick_idx % v.len()]
        }
        ("radio_on", true) => {
            let v = ["I heard it too.", "Good station.", "Nice background."];
            v[pick_idx % v.len()]
        }
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
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

    /// At the lively pace, as these tests were written.
    fn setup() -> (World, Hub, Db) {
        let mut t = Tables::builtin();
        t.balance.quips.enabled = false;
        let mut c = Config::default();
        c.features.chatter = "lively".into();
        (World::new(t, 42), Hub::new(c), Db::in_memory())
    }

    #[test]
    fn chatter_sets_the_hourly_cap() {
        let (mut w, _, _) = setup();
        let caps: Vec<usize> = [0u8, 1, 2]
            .iter()
            .map(|c| {
                w.chatter = *c;
                w.recent.clear();
                (0..10)
                    .take_while(|_| {
                        let ok = w.under_cap("p", 0);
                        if ok {
                            w.recent.entry("p".into()).or_default().push_back(0);
                        }
                        ok
                    })
                    .count()
            })
            .collect();
        assert_eq!(caps, [1, 3, 6], "quiet, normal, lively");
    }

    fn msgs(out: &[Out], kind: &str) -> Vec<serde_json::Value> {
        out.iter()
            .filter_map(|o| match o {
                Out::Ws(s) => serde_json::from_str::<serde_json::Value>(s)
                    .ok()
                    .filter(|v| v["type"] == kind),
                _ => None,
            })
            .map(|v| v["data"].clone())
            .collect()
    }

    #[test]
    fn clock_times_parse() {
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
        assert_eq!(
            h.activities.get("claude-code").map(String::as_str),
            Some("fishing")
        );
        assert_eq!(h.pets()[0].activity.as_deref(), Some("fishing"));
        let mut done = ev("claude-code", "a", EventKind::TurnDone, t0 + 120_000);
        done.detail.result = Some(TurnResult::Ok);
        h.apply(&done, t0 + 120_000);
        w.tick(t0 + 120_100, &mut h, &db);
        let out = w.tick(t0 + 121_500, &mut h, &db);
        let fx = msgs(&out, "pet.fx");
        assert_eq!(fx.len(), 1);
        assert_eq!(
            (
                fx[0]["kind"].as_str(),
                fx[0]["items"][0]["outcome"].as_str()
            ),
            (Some("catch"), Some("fish"))
        );
        assert!(
            fx[0].get("coins").is_none() && fx[0]["items"][0].get("coins").is_none(),
            "a fish is a fish, not money"
        );
        assert_eq!(db.fish_left(), 1, "it is in the basket");
        assert!(h.activities.is_empty(), "done: rod packed up");
    }

    #[test]
    fn what_you_earn_yourself_is_its_own_sweet_and_the_hour_decides_the_rest() {
        let (mut w, _h, db) = setup();
        let t0 = 1_800_000_000_000;
        // A set done to the end is a salted plum; a break after a long sit, a mint leaf.
        assert_eq!(w.earn("kegel", t0, &db).fish_id, "plum");
        assert_eq!(w.earn("break", t0, &db).fish_id, "todo");
        assert_eq!(db.fish_left(), 2);
        // Whatever an agent's turn lands can turn up at that hour and in that month, and is not one of yours.
        use chrono::{Datelike, Timelike};
        for i in 0..200u64 {
            let now = t0 + i * 3_600_000 * 7;
            let fx = w.catch(i % 40, "claude-code", "s", now, &db);
            let f =
                w.t.fish
                    .iter()
                    .find(|f| f.id == fx.fish_id)
                    .expect("a kind from the catalogue")
                    .clone();
            let when = chrono::DateTime::from_timestamp_millis(now as i64)
                .unwrap()
                .with_timezone(&chrono::Local);
            assert!(
                f.fits("agent", when.hour() as u8, when.month() as u8),
                "{} at {when}",
                f.id
            );
        }
    }

    #[test]
    fn feeding_takes_a_fish_from_the_basket_and_the_pet_answers() {
        let (mut w, _h, db) = setup();
        let t0 = 1_793_600_000_000;
        let (text, reward, n) = w.feed("claude-code", "zimeng", t0, &db);
        assert!(
            matches!(reward, crate::bond::BondReward::NoFish) && n == 0 && !text.is_empty(),
            "empty basket: it says so"
        );
        db.add_catch(t0, "fish", "todo", "common", "claude-code", "a");
        let (text, reward, n) = w.feed("claude-code", "zimeng", t0, &db);
        assert!(!text.is_empty() && n == 1 && !matches!(reward, crate::bond::BondReward::NoFish));
        assert_eq!(
            w.last_fed.as_deref(),
            Some("todo"),
            "the window is told which one it ate"
        );
        assert_eq!(db.fish_left(), 0);
        // Feeds counted for a pet pack by an earlier build carry over.
        let db2 = Db::in_memory();
        db2.kv_set("bond:feeds:pixel-bot", "23");
        db2.add_catch(t0, "fish", "todo", "common", "claude-code", "a");
        assert_eq!(w.feed("claude-code", "zimeng", t0, &db2).2, 24);
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
        assert_eq!(
            msgs(&out, "bug.spawn").len(),
            4,
            "1 for the failed tool + 3 for the error turn"
        );
        let out = w.tick(t0 + 32_000, &mut h, &db);
        let fx = msgs(&out, "pet.fx");
        assert_eq!(fx[0]["items"][0]["outcome"], "boot");
        assert_eq!(db.fish_left(), 0, "a boot is not a fish");
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
        let boss = w
            .bugs
            .iter()
            .find(|b| b.boss)
            .cloned()
            .expect("boss spawned");
        assert_eq!(boss.hp, 10);
        for _ in 0..9 {
            assert!(!w.squash(boss.id, t0 + 3000, &mut h, &db).unwrap().killed);
        }
        let r = w.squash(boss.id, t0 + 3000, &mut h, &db).unwrap();
        assert!(r.killed && r.boss);
        assert_eq!(h.pets()[0].state, State::Thinking, "stone lifted");
        assert_eq!(db.report(&day_of(t0 + 3000)).bosses, 1);
    }

    #[test]
    fn attention_freezes_the_world_but_reminders_still_run() {
        let (mut w, mut h, db) = setup();
        // A reminder of the user's own: allowed while someone waits.
        w.t.rules = serde_yaml::from_str::<Vec<Rule>>("- { id: urge, on: state, from: other, state: attention, actor_state: [idle, done, sleeping], chance: 1, action: wave, move: to_target, during_attention: true }").unwrap();
        let t0 = 1_793_600_000_000;
        h.apply(&ev("claude-code", "a", EventKind::SessionStart, t0), t0);
        h.apply(&ev("codex", "c", EventKind::TurnStart, t0), t0);
        w.tick(t0, &mut h, &db);
        // Codex waits on the user: Claude Code (idle) runs over to urge.
        h.apply(
            &ev("codex", "c", EventKind::Attention, t0 + 1000),
            t0 + 1000,
        );
        let out = w.tick(t0 + 1000, &mut h, &db);
        assert!(w.frozen);
        let scripts = msgs(&out, "pet.script");
        assert_eq!(scripts.len(), 1);
        assert_eq!(scripts[0]["rule"], "urge");
        assert!(out.iter().any(|o| matches!(
            o,
            Out::Control(Control::Move {
                target: Some(_),
                ..
            })
        )));
        // Anything else (a rule of the user's own, say) stays frozen.
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
        // A rule of the user's own that fires on every error of the other side.
        w.t.rules = serde_yaml::from_str::<Vec<Rule>>(
            "- { id: poke, on: turn_done, result: error, action: wave, chance: 1 }",
        )
        .unwrap();
        let t0 = 1_793_600_000_000;
        h.apply(&ev("claude-code", "a", EventKind::SessionStart, t0), t0);
        let mut fired = 0;
        for i in 0..20u64 {
            let ts = t0 + i * 60_000;
            h.apply(&ev("codex", "c", EventKind::TurnStart, ts), ts);
            let mut done = ev("codex", "c", EventKind::TurnDone, ts + 10);
            done.detail.result = Some(TurnResult::Error);
            h.apply(&done, ts + 10);
            let out = w.tick(ts + 10, &mut h, &db);
            fired += msgs(&out, "pet.script")
                .iter()
                .filter(|s| s["pet"] == "claude-code")
                .count();
            w.tick(ts + 50_000, &mut h, &db); // let the script finish
        }
        assert_eq!(fired, 6);
    }

    #[test]
    fn world_state_carries_only_what_windows_still_use() {
        let (mut w, mut h, db) = setup();
        let t0 = 1_793_600_000_000;
        h.apply(&ev("claude-code", "a", EventKind::TurnStart, t0), t0);
        let out = w.tick(t0, &mut h, &db);
        let st = msgs(&out, "world.state");
        let mut keys: Vec<&str> = st[0]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort();
        assert_eq!(keys, ["bugs", "focus", "frozen", "sounds"]);
        assert!(
            msgs(&w.tick(t0 + 100, &mut h, &db), "world.state").is_empty(),
            "sent on change only"
        );
    }
}
