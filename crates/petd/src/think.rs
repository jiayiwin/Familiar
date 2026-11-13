//! What the pets notice about you, and what they do about it on their own.
//!
//! They watch two things: whether you are at the machine (time since the last
//! key or mouse input, nothing about what you type or look at) and what the
//! agents are doing. From those they recognise a handful of situations (you
//! have sat too long, it is late, an agent keeps failing, a long job just
//! finished, you came back after a while, an agent has waited on you) and
//! answer each with one act: a line, a dance, a piece on its instrument, a
//! kegel set done together. The line always says what was noticed, in the
//! voice of whoever says it. Nothing here runs on a schedule.
//!
//! With two pets up (two agents at work) the moments are shared: one comes
//! over when the other's agent is stuck or waiting on you, and from the second
//! day they have spent together they play and dance as a pair. On their first
//! day the newcomer only sits and listens.
//!
//! Once they know each other they also have small moments of their own, acted
//! out rather than said: one whose agent finishes takes a sweet from the jar
//! over to one still at work (or stuck); two that finish together walk up and
//! bump; and when nothing needs any of them for a while, two of them talk.

use crate::session::State;
use crate::voice;
use chrono::Timelike;
use std::collections::{HashMap, VecDeque};
use std::time::Duration;

const MIN: u64 = 60_000;
/// How often the situation is looked at.
const TICK: Duration = Duration::from_secs(20);
/// No input for this long: you have left the desk.
const AWAY_MS: u64 = 5 * MIN;
/// Coming back after this long is worth a word.
const BACK_AFTER_MS: u64 = 20 * MIN;
/// Two acts are at least this far apart (a welcome back and "it is waiting for you" are exempt).
const GAP_MS: u64 = 10 * MIN;
/// An agent has waited on you this long while you sit right there: the other pet comes to say so.
const WAIT_MS: u64 = 3 * MIN;
/// Getting up after sitting this long earns a fish.
const BREAK_AFTER_MS: u64 = 30 * MIN;
/// Days two pets have spent together before they act as a pair.
const CLOSE_DAYS: u32 = 2;
/// How long a guest stays for a piece before it walks home: the piece, the walk over, and a breath.
const PIECE_MS: u64 = 40_000;
/// A job at least this long, finished, is worth walking over about.
const SHORT_MS: u64 = 3 * MIN;
/// Two jobs finished this close together finished together.
const TOGETHER_MS: u64 = 90_000;
/// The jar keeps at least one for you: a sweet is shared only from two up.
const SHARE_FROM: u32 = 2;
/// (performances staged so far, who is away from home for the latest one).
static AWAY: std::sync::Mutex<(u64, Option<String>)> = std::sync::Mutex::new((0, None));

/// What a pet does. The strings are the `act` of the `pet.act` message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Act {
    Speak,
    Dance,
    /// Its piece, on its own instrument.
    Play,
    /// It turns its back and leads a kegel set.
    Kegel,
    /// It sits by the other one and listens.
    Watch,
    /// It walks over with a sweet from the jar and holds it out.
    Give,
    /// It is handed a sweet and eats it.
    Take,
    /// Two finished together: they hop and touch.
    Bump,
    /// It walks over and says something about nothing in particular...
    Chat,
    /// ...and the other one answers.
    Reply,
    /// It asks whether to do something (put the radio on, …); a "yes" on its bubble does it.
    Offer,
}

impl Act {
    pub fn as_str(self) -> &'static str {
        match self {
            Act::Speak => "speak",
            Act::Dance => "dance",
            Act::Play => "play",
            Act::Kegel => "kegel",
            Act::Watch => "watch",
            Act::Give => "give",
            Act::Take => "take",
            Act::Bump => "bump",
            Act::Chat => "chat",
            Act::Reply => "reply",
            Act::Offer => "offer",
        }
    }
}

/// What the other pet does in the same moment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct With {
    pub pet: String,
    pub act: Act,
    pub text: String,
}

/// One thing the pets decided to do, and the situation behind it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub pet: String,
    pub act: Act,
    pub text: String,
    /// Which situation: "back" | "wait" | "done" | "late" | "stuck" | "sit" | "share" | "bump" | "chat", or "asked" (from the menu).
    pub why: &'static str,
    /// The same moment as the room's diary tells it afterwards.
    pub note: String,
    /// (who walks over, to whom) before anything is said or played.
    pub walk: Option<(String, String)>,
    /// What the other pets do in the same moment (the first of them is the one that walks over, if any does).
    pub with: Vec<With>,
    /// A shared sweet: its shape and colours, once it is out of the jar.
    pub treat: Option<(String, Vec<String>)>,
    /// An offer's "yes": what it opens ("radio:often", "radio:calm", …).
    pub offer: Option<&'static str>,
}

/// One pet as the engine sees it.
#[derive(Debug, Clone)]
pub struct Seen {
    pub id: String,
    pub state: State,
    /// Which of the five it is (`zimeng`, `mashu`, …): whose voice the line is in.
    pub who: &'static str,
}

/// What can be seen at one moment.
#[derive(Debug, Clone, Default)]
pub struct Obs {
    /// Seconds since the last keyboard or mouse input; `None` where the system does not say.
    pub idle_s: Option<u64>,
    /// Local hour, 0..24.
    pub hour: u8,
    /// Each showing pet and its agent's state.
    pub pets: Vec<Seen>,
    /// "quiet" | "normal" | "lively": how much the pets do on their own.
    pub chatter: String,
    pub focus: bool,
    /// Reps in a kegel set (the user's setting).
    pub kegel_reps: u32,
    /// Days on which two pets were up together, today included.
    pub duo_days: u32,
    /// Sweets in the jar.
    pub jar: u32,
    /// The radio is playing.
    pub radio_on: bool,
}

fn working(s: State) -> bool {
    matches!(s, State::Thinking | State::Tool | State::Reading)
}

fn failed(s: State) -> bool {
    matches!(s, State::Error | State::Stone | State::Aborted)
}

/// Up and about: its agent has a live session and it is not asleep.
fn up(p: &Seen) -> bool {
    !matches!(p.state, State::Offline | State::Sleeping)
}

fn name(who: &str) -> &'static str {
    crate::cast::by_id(who).map(|f| f.name()).unwrap_or("")
}

fn played(who: &str) -> &'static str {
    voice::played(
        crate::cast::by_id(who)
            .map(|f| f.instrument)
            .unwrap_or("piano"),
    )
}

#[derive(Default)]
pub struct Engine {
    /// Since when you have been at the desk without a real break.
    present_since: Option<u64>,
    /// Since when you have been gone.
    away_since: Option<u64>,
    /// While you were gone: jobs finished, and whether one failed.
    away_done: u32,
    away_failed: bool,
    last_state: HashMap<String, State>,
    /// Since when each pet's agent has been at its current job.
    work_start: HashMap<String, u64>,
    /// A long job that finished since the last look: (pet, minutes).
    finished: Option<(String, u64)>,
    /// When agents failed, within the last half hour, and whose.
    failures: VecDeque<(u64, String)>,
    /// Since when each pet's agent has been waiting on you, and which waits were already announced.
    waiting: HashMap<String, u64>,
    told_wait: HashMap<String, u64>,
    last_act: Option<u64>,
    last_by: HashMap<&'static str, u64>,
    /// Long sits answered so far.
    sits: u32,
    /// Set by the tick on which you came back from a break: how long you were gone (ms).
    pub came_back: Option<u64>,
    /// How long you had sat when you last left the desk.
    sat_ms: Option<u64>,
    /// Set by the tick on which you came back from a break taken after a long
    /// sit: (minutes sat, minutes away). That break earns a fish.
    pub took_break: Option<(u64, u64)>,
    /// When each pet's agent last finished a job of some length.
    done_at: HashMap<String, u64>,
    /// Since when no pet that is up has had anything to do.
    quiet_since: Option<u64>,
    /// Talks had so far: who talks to whom, and about what, goes round.
    chats: u32,
}

impl Engine {
    /// Whether you are at the desk, as of the last tick.
    pub fn present(&self) -> bool {
        self.present_since.is_some()
    }

    /// Take in one observation; returns what to do now, if anything.
    pub fn tick(&mut self, now: u64, obs: &Obs) -> Option<Plan> {
        let busy = obs.pets.iter().any(|p| working(p.state));
        // Without an input clock, an agent at work stands in for you being there.
        let idle_ms = obs
            .idle_s
            .map(|s| s * 1000)
            .unwrap_or(if busy { 0 } else { AWAY_MS });
        let mut back_after = None;
        if idle_ms >= AWAY_MS {
            let left = now.saturating_sub(idle_ms);
            if let Some(since) = self.present_since.take() {
                self.sat_ms = Some(left.saturating_sub(since));
            }
            self.away_since.get_or_insert(left);
        } else {
            self.present_since.get_or_insert(now);
            if idle_ms < MIN {
                if let Some(since) = self.away_since.take() {
                    back_after = Some(now.saturating_sub(since));
                }
            }
        }
        self.came_back = back_after;
        self.took_break = match (back_after, self.sat_ms) {
            (Some(gone), Some(sat)) => {
                self.sat_ms = None;
                (sat >= BREAK_AFTER_MS).then_some((sat / MIN, gone / MIN))
            }
            _ => None,
        };
        let away = self.away_since.is_some();

        let mut just_done: Vec<String> = vec![];
        let mut just_failed: Vec<String> = vec![];
        for p in &obs.pets {
            let (id, state) = (&p.id, p.state);
            if state == State::Attention {
                self.waiting.entry(id.clone()).or_insert(now);
            } else {
                self.waiting.remove(id);
                self.told_wait.remove(id);
            }
            let prev = self.last_state.insert(id.clone(), state);
            if prev == Some(state) {
                continue;
            }
            if working(state) {
                self.work_start.entry(id.clone()).or_insert(now);
            } else if state == State::Done {
                if let Some(t0) = self.work_start.remove(id) {
                    if now - t0 >= SHORT_MS {
                        just_done.push(id.clone());
                    }
                    if away {
                        self.away_done += 1;
                    } else if now - t0 >= 15 * MIN {
                        self.finished = Some((id.clone(), (now - t0) / MIN));
                    }
                }
            } else if failed(state) {
                self.work_start.remove(id);
                self.failures.push_back((now, id.clone()));
                self.away_failed |= away;
                just_failed.push(id.clone());
            } else if state != State::Attention {
                // Idle, asleep, gone: whatever it was doing is over. Waiting on you only pauses it.
                self.work_start.remove(id);
            }
        }
        while self
            .failures
            .front()
            .is_some_and(|(t, _)| now - *t > 30 * MIN)
        {
            self.failures.pop_front();
        }
        // Nobody that is up has anything to do (and at least two are up): since when.
        let free: Vec<&Seen> = obs
            .pets
            .iter()
            .filter(|p| up(p) && !working(p.state) && p.state != State::Attention)
            .collect();
        if free.len() >= 2
            && obs
                .pets
                .iter()
                .filter(|p| up(p))
                .all(|p| !working(p.state) && p.state != State::Attention)
        {
            self.quiet_since.get_or_insert(now);
        } else {
            self.quiet_since = None;
        }

        let lead = obs.pets.iter().find(|p| up(p)).or(obs.pets.first())?;
        let finished = self.finished.take();
        if obs.focus {
            return None;
        }
        let seen = |id: &str| obs.pets.iter().find(|p| p.id == id);
        // The other pet (the first of them), when it is up and not itself waiting on you.
        let other = |id: &str| others(obs, id).into_iter().next();
        let close = obs.duo_days >= CLOSE_DAYS;
        let solo = |p: &Seen, act: Act, key: &str, n: u64, why: &'static str, note: String| Plan {
            pet: p.id.clone(),
            act,
            text: voice::line(p.who, key, n),
            why,
            note,
            walk: None,
            with: vec![],
            treat: None,
            offer: None,
        };

        // Back at the desk: what happened meanwhile. Said once, whatever else was said lately.
        if let Some(gone) = back_after {
            let (done, stuck) = (
                std::mem::take(&mut self.away_done),
                std::mem::take(&mut self.away_failed),
            );
            // An agent stopped to wait on you while you were gone: its own pet asks, and a "yes"
            // brings that session's window to the front. This comes before what got done.
            if let Some(p) = obs.pets.iter().find(|p| p.state == State::Attention) {
                let since = self.waiting.get(&p.id).copied().unwrap_or(now);
                self.told_wait.insert(p.id.clone(), since);
                let mins = gone / MIN;
                let agent = crate::config::agent_tag(&p.id);
                let note = crate::tr!(
                    "你离开了 {mins} 分钟。{agent} 那边停下来等你回话。",
                    "You were away {mins} minutes. {agent} stopped to wait for your answer."
                );
                let mut plan = solo(p, Act::Offer, "back_wait", 0, "back", note);
                plan.offer = Some("go");
                return self.act(now, plan);
            }
            if gone >= BACK_AFTER_MS && (done > 0 || stuck) {
                let mins = gone / MIN;
                let (key, n, note) = match (done, stuck) {
                    (0, _) => (
                        "back_stuck",
                        0,
                        crate::tr!(
                            "你离开了 {mins} 分钟。那会儿它卡住了。",
                            "You were away {mins} minutes. It got stuck meanwhile."
                        ),
                    ),
                    (n, false) => (
                        "back_done",
                        n,
                        crate::tr!(
                            "你离开了 {mins} 分钟。那会儿做完了 {n} 件。",
                            "You were away {mins} minutes. {n} got done meanwhile."
                        ),
                    ),
                    (n, true) => (
                        "back_both",
                        n,
                        crate::tr!(
                            "你离开了 {mins} 分钟。那会儿做完了 {n} 件，还有一件卡住了。",
                            "You were away {mins} minutes. {n} got done meanwhile, one got stuck."
                        ),
                    ),
                };
                return self.act(now, solo(lead, Act::Speak, key, u64::from(n), "back", note));
            }
        }
        if away || obs.chatter == "quiet" {
            return None;
        }

        // An agent has waited on you a while and you are right here: its own pet is already
        // looking up at you, so the other one walks over and says it. Once per wait.
        let mut waits: Vec<(String, u64)> = self
            .waiting
            .iter()
            .filter(|(id, t)| now - **t >= WAIT_MS && !self.told_wait.contains_key(*id))
            .map(|(id, t)| (id.clone(), *t))
            .collect();
        waits.sort_by_key(|w| w.1);
        for (id, since) in waits {
            let Some(o) = other(&id) else { continue };
            self.told_wait.insert(id.clone(), since);
            let mins = (now - since) / MIN;
            let agent = crate::config::agent_tag(&id);
            let note = crate::tr!(
                "{agent} 那边等了你 {mins} 分钟。{}走过去叫你。",
                "{agent} waited {mins} minutes for you. {} walked over to tell you.",
                name(o.who)
            );
            let mut plan = solo(o, Act::Speak, "wait", mins, "wait", note);
            plan.walk = Some((o.id.clone(), id));
            return self.act(now, plan);
        }

        if self.last_act.is_some_and(|t| now - t < GAP_MS) {
            return None;
        }
        // "lively" notices sooner.
        let eager = |ms: u64| {
            if obs.chatter == "lively" {
                ms * 3 / 4
            } else {
                ms
            }
        };
        let sat = self.present_since.map(|t| now - t).unwrap_or(0);
        let rested = |me: &Self, why: &'static str, ms: u64| {
            me.last_by.get(why).is_none_or(|t| now - *t >= ms)
        };

        // Between the pets, once they know each other.
        for id in &just_done {
            self.done_at.insert(id.clone(), now);
        }
        if close {
            for id in &just_done {
                let Some(me) = seen(id) else { continue };
                // Another one finished a moment ago and is free: it walks over and they bump.
                let pal = others(obs, id).into_iter().find(|o| {
                    !working(o.state)
                        && !failed(o.state)
                        && self
                            .done_at
                            .get(&o.id)
                            .is_some_and(|t| *t < now && now - *t <= TOGETHER_MS)
                });
                if let (Some(o), true) = (pal, rested(self, "bump", 30 * MIN)) {
                    return self.act(now, bump(me, o));
                }
                // One still at a long job, or stuck: it takes a sweet over.
                if obs.jar >= SHARE_FROM && rested(self, "share", 40 * MIN) {
                    let busy = others(obs, id)
                        .into_iter()
                        .filter(|o| {
                            failed(o.state)
                                || (working(o.state)
                                    && self
                                        .work_start
                                        .get(&o.id)
                                        .is_some_and(|t| now - *t >= 5 * MIN))
                        })
                        .max_by_key(|o| {
                            (
                                failed(o.state),
                                std::cmp::Reverse(
                                    self.work_start.get(&o.id).copied().unwrap_or(now),
                                ),
                            )
                        });
                    if let Some(o) = busy {
                        return self.act(now, share_done(me, o));
                    }
                }
            }
            // One just got stuck and another is free: a sweet goes over.
            for id in &just_failed {
                let (Some(o), true) = (
                    seen(id),
                    obs.jar >= SHARE_FROM && rested(self, "share", 40 * MIN),
                ) else {
                    continue;
                };
                let Some(me) = others(obs, id)
                    .into_iter()
                    .find(|p| !working(p.state) && !failed(p.state))
                else {
                    continue;
                };
                let note = crate::tr!(
                    "{}卡住了。{}走过去，给它一颗{{treat}}。",
                    "{} got stuck. {} walked over with a {{treat}}.",
                    name(o.who),
                    name(me.who)
                );
                return self.act(now, share(me, o, true, note));
            }
        }

        if let Some((who, mins)) = finished {
            if rested(self, "done", 30 * MIN) {
                let p = seen(&who).unwrap_or(lead);
                let mut plan = solo(
                    p,
                    Act::Dance,
                    "done",
                    mins,
                    "done",
                    crate::tr!(
                        "一件活跑了 {mins} 分钟，做完了。{}跳了一轮舞。",
                        "A job ran {mins} minutes and finished. {} danced a round.",
                        name(p.who)
                    ),
                );
                let idle: Vec<&Seen> = others(obs, &p.id)
                    .into_iter()
                    .filter(|o| close && !working(o.state))
                    .collect();
                if !idle.is_empty() {
                    plan.with = dancers(&idle);
                    plan.note = crate::tr!("一件活跑了 {mins} 分钟，做完了。{}跳了一轮舞，{}也跟着跳。", "A job ran {mins} minutes and finished. {} danced a round, and {} joined in.", name(p.who), names(&idle));
                }
                return self.act(now, plan);
            }
        }
        let night = obs.hour >= 23 || obs.hour < 5;
        // Not over the radio, and not right after it offered some (one evening thing at a time).
        if night && sat >= eager(45 * MIN) && rested(self, "late", 60 * MIN) && !obs.radio_on && rested(self, "radio", 60 * MIN) {
            let p = lead;
            let mut plan = solo(
                p,
                Act::Play,
                "late",
                0,
                "late",
                crate::tr!(
                    "夜了，你还坐着。{}{}。",
                    "It was late and you were still here. {} {}.",
                    name(p.who),
                    played(p.who)
                ),
            );
            pair(
                &mut plan,
                p,
                &others(obs, &p.id),
                close,
                0,
                crate::tr!("夜了，你还坐着。", "It was late and you were still here. "),
            );
            return self.act(now, plan);
        }
        // Whose agent keeps failing: that one's pet plays, to clear your head.
        let mut counts: HashMap<&str, u64> = HashMap::new();
        for (_, id) in &self.failures {
            *counts.entry(id.as_str()).or_default() += 1;
        }
        let stuck = counts
            .into_iter()
            .filter(|(_, n)| *n >= 3)
            .max_by_key(|(id, n)| (*n, *id))
            .map(|(id, n)| (id.to_string(), n));
        if let Some((id, n)) = stuck {
            if rested(self, "stuck", 45 * MIN) {
                let p = seen(&id).unwrap_or(lead);
                let agent = crate::config::agent_tag(&p.id);
                let what = crate::tr!(
                    "{agent} 半小时里卡了 {n} 次。",
                    "{agent} got stuck {n} times in half an hour. "
                );
                let mut plan = solo(
                    p,
                    Act::Play,
                    "stuck",
                    n,
                    "stuck",
                    crate::tr!("{what}{}{}。", "{what}{} {}.", name(p.who), played(p.who)),
                );
                pair(&mut plan, p, &others(obs, &p.id), close, n, what);
                return self.act(now, plan);
            }
        }
        // Things it can offer, not do: the radio, while you wait on a long job or late at night.
        // Never while it plays already, and once in a long while.
        if !obs.radio_on && rested(self, "radio", 4 * 60 * MIN) {
            let waiting_on = obs.pets.iter().filter(|p| working(p.state)).find(|p| {
                self.work_start
                    .get(&p.id)
                    .is_some_and(|t| now - *t >= 8 * MIN)
            });
            let idle = obs.idle_s.unwrap_or(0);
            if let (Some(p), true) = (
                waiting_on,
                (60..=300).contains(&idle) && (9..22).contains(&obs.hour),
            ) {
                return self.act(
                    now,
                    Plan {
                        offer: Some("radio:often"),
                        ..solo(p, Act::Offer, "offer_wait", 0, "radio", String::new())
                    },
                );
            }
            if (obs.hour >= 22 || obs.hour < 2) && sat >= 30 * MIN {
                return self.act(
                    now,
                    Plan {
                        offer: Some("radio:calm"),
                        ..solo(lead, Act::Offer, "offer_late", 0, "radio", String::new())
                    },
                );
            }
        }
        // After lunch the slump comes sooner.
        let slump = (13..16).contains(&obs.hour);
        if sat >= eager(if slump { 60 * MIN } else { 90 * MIN }) && rested(self, "sit", 60 * MIN) {
            let mins = sat / MIN;
            let p = lead;
            // Sitting is what a kegel set is for; the slump wants waking up instead. Otherwise they take turns.
            self.sits += 1;
            if !slump && self.sits % 2 == 1 {
                let note = crate::tr!(
                    "你坐了 {mins} 分钟没动。{}转过身，带你做了一组提肛。",
                    "You sat {mins} minutes without a break. {} turned round and led a kegel set.",
                    name(p.who)
                );
                return self.act(now, solo(p, Act::Kegel, "sit_kegel", mins, "sit", note));
            }
            let mut plan = solo(
                p,
                Act::Dance,
                "sit_dance",
                mins,
                "sit",
                crate::tr!(
                    "你坐了 {mins} 分钟没动。{}跳了一轮舞。",
                    "You sat {mins} minutes without a break. {} danced a round.",
                    name(p.who)
                ),
            );
            let rest = others(obs, &p.id);
            if close && !rest.is_empty() {
                plan.with = dancers(&rest);
                plan.note = crate::tr!(
                    "你坐了 {mins} 分钟没动。{}和{}一起跳了一轮舞。",
                    "You sat {mins} minutes without a break. {} and {} danced a round together.",
                    name(p.who),
                    names(&rest)
                );
            }
            return self.act(now, plan);
        }
        // Nothing has needed any of them for a while and you are here: two of them talk.
        if close
            && self.quiet_since.is_some_and(|t| now - t >= eager(12 * MIN))
            && rested(self, "chat", eager(90 * MIN))
            && free.len() >= 2
        {
            let plan = talk(&free, self.chats as usize);
            self.chats += 1;
            return self.act(now, plan);
        }
        None
    }

    fn act(&mut self, now: u64, plan: Plan) -> Option<Plan> {
        // "It is waiting for you" is a reminder, not a performance: it does not hold the others back.
        if plan.why != "wait" {
            self.last_act = Some(now);
        }
        self.last_by.insert(plan.why, now);
        Some(plan)
    }
}

/// `me` finished its job and takes a sweet over to `o`, still at work or stuck.
fn share_done(me: &Seen, o: &Seen) -> Plan {
    let stuck = failed(o.state);
    let note = if stuck {
        crate::tr!(
            "{}这边做完了，走过去给卡住的{}一颗{{treat}}。",
            "{} finished and took a {{treat}} over to {}, who was stuck.",
            name(me.who),
            name(o.who)
        )
    } else {
        crate::tr!(
            "{}这边做完了，走过去给还在忙的{}一颗{{treat}}。",
            "{} finished and took a {{treat}} over to {}, still at work.",
            name(me.who),
            name(o.who)
        )
    };
    share(me, o, stuck, note)
}

/// `me` finished at about the same time as `o`: it walks over and they bump.
fn bump(me: &Seen, o: &Seen) -> Plan {
    let note = crate::tr!(
        "{}和{}差不多同时做完。{}走过去，和它碰了一下。",
        "{} and {} finished at about the same time. {} walked over and they bumped.",
        name(o.who),
        name(me.who),
        name(me.who)
    );
    Plan {
        pet: me.id.clone(),
        act: Act::Bump,
        text: voice::line(me.who, "bump", 0),
        why: "bump",
        note,
        walk: Some((me.id.clone(), o.id.clone())),
        with: vec![With {
            pet: o.id.clone(),
            act: Act::Bump,
            text: voice::line(o.who, "bump_back", 0),
        }],
        treat: None,
        offer: None,
    }
}

/// Two of `free` talk: the `k`-th talk picks who goes over to whom, and the topic, in turn.
fn talk(free: &[&Seen], k: usize) -> Plan {
    let n = free.len();
    let (a, b) = (free[k % n], free[(k % n + 1 + (k / n) % (n - 1)) % n]);
    let topic = voice::TOPICS[k % voice::TOPICS.len()];
    let (said, answer) = (
        voice::chat(a.who, topic, true),
        voice::chat(b.who, topic, false),
    );
    let note = crate::tr!(
        "{}走到{}旁边说：「{said}」{}说：「{answer}」",
        "{} went over to {} and said, \"{said}\" {} said, \"{answer}\"",
        name(a.who),
        name(b.who),
        name(b.who)
    );
    Plan {
        pet: a.id.clone(),
        act: Act::Chat,
        text: said,
        why: "chat",
        note,
        walk: Some((a.id.clone(), b.id.clone())),
        with: vec![With {
            pet: b.id.clone(),
            act: Act::Reply,
            text: answer,
        }],
        treat: None,
        offer: None,
    }
}

/// `/pet moment share|bump|chat`: play one of the moments between pets now, to see it.
/// The main pet goes over to the next one that is up; the timing rules do not apply.
pub fn shown(obs: &Obs, which: &str, k: usize) -> Result<Plan, String> {
    // Sweeping the desktop takes one pet: the first that is up asks, as it would on its own.
    if matches!(which, "tidy" | "sweep" | "桌面" | "扫" | "扫桌面") {
        let Some(a) = obs.pets.iter().find(|p| up(p) && p.state != State::Attention) else {
            return Err(crate::tr!("要有一只宠物在场才能演。", "It takes a pet that is up."));
        };
        let text = voice::line(a.who, "offer_tidy", 0);
        return Ok(Plan { pet: a.id.clone(), act: Act::Offer, text, why: "tidy", note: String::new(), walk: None, with: vec![], treat: None, offer: Some("tidy") });
    }
    // ...or putting the old files away into 桌面收纳 as it sweeps.
    if matches!(which, "stow" | "收纳" | "桌面收纳") {
        let Some(a) = obs.pets.iter().find(|p| up(p) && p.state != State::Attention) else {
            return Err(crate::tr!("要有一只宠物在场才能演。", "It takes a pet that is up."));
        };
        let text = voice::line(a.who, "offer_stow", 0);
        return Ok(Plan { pet: a.id.clone(), act: Act::Offer, text, why: "stow", note: String::new(), walk: None, with: vec![], treat: None, offer: Some("stow") });
    }
    let up: Vec<&Seen> = obs
        .pets
        .iter()
        .filter(|p| up(p) && p.state != State::Attention)
        .collect();
    let (Some(a), Some(b)) = (up.first(), up.get(1)) else {
        return Err(crate::tr!(
            "要有两只宠物在场（开着两个 agent）才能演。",
            "It takes two pets (two agents running) to play a moment."
        ));
    };
    match which {
        "share" | "sweet" | "糖" | "分糖" => {
            if obs.jar == 0 {
                return Err(crate::tr!(
                    "糖罐是空的，没糖可分。",
                    "The jar is empty: nothing to share."
                ));
            }
            Ok(share_done(a, b))
        }
        "bump" | "碰" | "碰一下" => Ok(bump(a, b)),
        "chat" | "talk" | "说话" | "闲聊" => Ok(talk(&up, k)),
        "radio" | "电台" => Ok(Plan { pet: a.id.clone(), act: Act::Offer, text: voice::line(a.who, "offer_wait", 0), why: "radio", note: String::new(), walk: None, with: vec![], treat: None, offer: Some("radio:often") }),
        _ => Err(crate::tr!(
            "可以演的：share（分糖）、bump（碰一下）、chat（说两句）、tidy（扫桌面）、stow（桌面收纳）。",
            "Moments: share, bump, chat, tidy, stow."
        )),
    }
}

/// `me` takes a sweet over to `o`; `note` says so, with `{treat}` for which one.
fn share(me: &Seen, o: &Seen, stuck: bool, note: String) -> Plan {
    Plan {
        pet: me.id.clone(),
        act: Act::Give,
        text: voice::line(me.who, if stuck { "give_stuck" } else { "give" }, 0),
        why: "share",
        note,
        walk: Some((me.id.clone(), o.id.clone())),
        with: vec![With {
            pet: o.id.clone(),
            act: Act::Take,
            text: voice::line(o.who, "take", 0),
        }],
        treat: None,
        offer: None,
    }
}

/// A performance you asked for from the menu. The pairing is the same as when
/// they decide for themselves: the other one comes over to listen, and plays
/// or dances along once the two know each other.
pub fn asked(obs: &Obs, pet: &str, act: Act) -> Option<Plan> {
    let p = obs.pets.iter().find(|p| p.id == pet)?;
    let key = if act == Act::Dance {
        "ask_dance"
    } else {
        "ask_play"
    };
    let mut plan = Plan {
        pet: p.id.clone(),
        act,
        text: voice::line(p.who, key, 0),
        why: "asked",
        note: String::new(),
        walk: None,
        with: vec![],
        treat: None,
        offer: None,
    };
    let close = obs.duo_days >= CLOSE_DAYS;
    let rest = others(obs, &p.id);
    match act {
        Act::Play => pair(&mut plan, p, &rest, close, 0, String::new()),
        Act::Dance if close => plan.with = dancers(&rest),
        _ => {}
    }
    Some(plan)
}

/// The other pets that can take part: up, and not themselves waiting on you.
fn others<'a>(obs: &'a Obs, id: &str) -> Vec<&'a Seen> {
    obs.pets
        .iter()
        .filter(|p| p.id != id && up(p) && p.state != State::Attention)
        .collect()
}

/// "麻薯、泡泡和菇菇": the pets by name, for the diary.
fn names(pets: &[&Seen]) -> String {
    let n: Vec<&str> = pets.iter().map(|p| name(p.who)).collect();
    match n.as_slice() {
        [] => String::new(),
        [one] => one.to_string(),
        [head @ .., last] => crate::tr!(
            "{}和{last}",
            "{} and {last}",
            head.join(crate::i18n::t("、", ", "))
        ),
    }
}

/// Everyone else dancing along, each with its own word for it.
fn dancers(rest: &[&Seen]) -> Vec<With> {
    rest.iter()
        .map(|o| With {
            pet: o.id.clone(),
            act: Act::Dance,
            text: voice::line(o.who, "dance_too", 0),
        })
        .collect()
}

/// `p` is about to play; the others take part. The first of them walks over.
/// Once they know each other they play along, each on its own instrument (a
/// duet, or more); on their first day they only listen. `what` is how the
/// diary line begins.
fn pair(plan: &mut Plan, p: &Seen, rest: &[&Seen], close: bool, n: u64, what: String) {
    let Some(first) = rest.first() else { return };
    plan.walk = Some((first.id.clone(), p.id.clone()));
    if close {
        let key = if plan.why == "stuck" {
            "help_stuck"
        } else {
            "join"
        };
        plan.with = rest
            .iter()
            .enumerate()
            .map(|(i, o)| With {
                pet: o.id.clone(),
                act: Act::Play,
                text: voice::line(o.who, if i == 0 { key } else { "join" }, n),
            })
            .collect();
        plan.note = if rest.len() == 1 {
            crate::tr!(
                "{what}{}走过去，和{}合了一段。",
                "{what}{} walked over and played a duet with {}.",
                name(first.who),
                name(p.who)
            )
        } else {
            crate::tr!(
                "{what}{}走过去，{}和{}一起合了一段。",
                "{what}{} walked over, and {} played it together with {}.",
                name(first.who),
                names(rest),
                name(p.who)
            )
        };
    } else {
        plan.with = rest
            .iter()
            .map(|o| With {
                pet: o.id.clone(),
                act: Act::Watch,
                text: voice::line(o.who, "listen", 0),
            })
            .collect();
        plan.note = crate::tr!(
            "{what}{}{}，{}在旁边听。",
            "{what}{} {}; {} listened.",
            name(p.who),
            played(p.who),
            names(rest)
        );
    }
}

/// Seconds since the last keyboard or mouse input, system-wide.
#[cfg(windows)]
fn idle_secs() -> Option<u64> {
    use windows_sys::Win32::System::SystemInformation::GetTickCount;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
    let mut info = LASTINPUTINFO {
        cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
        dwTime: 0,
    };
    // SAFETY: `info` is a valid LASTINPUTINFO with its size set, as the call requires.
    if unsafe { GetLastInputInfo(&mut info) } == 0 {
        return None;
    }
    // SAFETY: no arguments, no preconditions.
    Some(u64::from(unsafe { GetTickCount() }.wrapping_sub(info.dwTime)) / 1000)
}

#[cfg(not(windows))]
fn idle_secs() -> Option<u64> {
    None
}

/// What can be seen right now, with the days the two have spent together.
pub fn look(sh: &crate::server::Shared, now: u64) -> Obs {
    let mut obs = observe(sh);
    obs.duo_days = duo_days(sh, now, &obs);
    obs
}

/// Carry out a plan: whoever walks over sets off, then each pet gets its act,
/// two of them on one clock.
pub fn stage(sh: &crate::server::Shared, plan: &Plan, now: u64, reps: u32) {
    // Whoever walks over arrives first; two acting together start on the same clock.
    let mut at = match (&plan.walk, plan.with.is_empty()) {
        (Some(_), _) => now + 2800,
        (None, false) => now + 900,
        (None, true) => 0,
    };
    // Who is out, and for which performance: an earlier one's way home must not cut a later one short.
    let mut away = AWAY.lock().unwrap();
    away.0 += 1;
    let turn = away.0;
    let mut out = away.1.take();
    let mut home = |pet: String| {
        // If it has a part in this one, the others wait until it is back on its spot.
        if at > 0 && (pet == plan.pet || plan.with.iter().any(|w| w.pet == pet)) {
            at = at.max(now + 1800);
        }
        let _ = sh.control.send(crate::Control::Move { pet, target: None });
    };
    match &plan.walk {
        // The one it would walk to is still standing beside it from last time: nobody moves.
        Some((_, target)) if out.as_deref() == Some(target.as_str()) => {}
        Some((mover, target)) => {
            if let Some(prev) = out.take().filter(|p| p != mover) {
                home(prev);
            }
            let _ = sh.control.send(crate::Control::Move {
                pet: mover.clone(),
                target: Some(target.clone()),
            });
            out = Some(mover.clone());
        }
        // A moment for one, or for two where they stand: whoever was out goes home.
        None => {
            if let Some(prev) = out.take() {
                home(prev);
            }
        }
    }
    if let Some(pet) = out.clone() {
        // It goes home when the moment is over.
        let stay = match (plan.act, plan.with.first().map(|w| w.act)) {
            (Act::Play, _) | (_, Some(Act::Play)) => PIECE_MS,
            (Act::Dance, _) => 30_000,
            (Act::Chat, _) => 13_000,
            _ => 9_000,
        };
        let control = sh.control.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(stay));
            let mut away = AWAY.lock().unwrap();
            if away.0 == turn {
                away.1 = None;
                let _ = control.send(crate::Control::Move { pet, target: None });
            }
        });
    }
    away.1 = out;
    drop(away);
    // `walks`: this one is on its way over, and speaks when it gets there.
    let walks = |pet: &str| plan.walk.as_ref().is_some_and(|(mover, _)| mover == pet);
    let _ = sh.ws_tx.send(crate::msg(
        "pet.act",
        // `to`: the one it goes to, so it can come right up to it.
        &serde_json::json!({ "pet": plan.pet, "act": plan.act.as_str(), "text": plan.text, "why": plan.why, "reps": reps, "at": at, "walks": walks(&plan.pet), "to": plan.with.first().map(|w| w.pet.clone()), "offer": plan.offer }),
    ));
    for w in &plan.with {
        // The one handed a sweet: which, and from whom (it comes in from that side).
        let treat = plan
            .treat
            .as_ref()
            .filter(|_| w.act == Act::Take)
            .map(|(shape, colors)| serde_json::json!({ "shape": shape, "colors": colors }));
        let _ = sh.ws_tx.send(crate::msg(
            "pet.act",
            &serde_json::json!({ "pet": w.pet, "act": w.act.as_str(), "text": w.text, "why": plan.why, "at": at, "walks": walks(&w.pet), "from": plan.pet, "treat": treat }),
        ));
    }
}

fn observe(sh: &crate::server::Shared) -> Obs {
    let hub = sh.hub.lock().unwrap();
    Obs {
        idle_s: idle_secs(),
        hour: chrono::Local::now().hour() as u8,
        pets: hub
            .pets()
            .into_iter()
            .map(|v| Seen {
                who: crate::cast::by_id(&v.skin)
                    .map(|f| f.id)
                    .unwrap_or("zimeng"),
                id: v.id,
                state: v.state,
            })
            .collect(),
        chatter: hub.cfg.features.chatter.clone(),
        focus: hub.cfg.features.focus_mode,
        kegel_reps: hub.cfg.features.kegel_reps.clamp(1, 30),
        duo_days: 0,
        jar: sh.db.fish_left() as u32,
        // The radio page reports every ten seconds while it plays.
        radio_on: sh
            .db
            .event_since("radio", crate::now_ms().saturating_sub(25_000)),
    }
}

/// Days on which two pets were up together, counting today if they are now.
/// The first such day goes into the diary: the newcomer arrives.
fn duo_days(sh: &crate::server::Shared, now: u64, obs: &Obs) -> u32 {
    let days = || {
        sh.db
            .kv_get("duo:days")
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(0)
    };
    let both: Vec<&Seen> = obs.pets.iter().filter(|p| up(p)).collect();
    let day = crate::db::day_of(now);
    if both.len() < 2 || sh.db.kv_get("duo:last").as_deref() == Some(day.as_str()) {
        return days();
    }
    let n = days() + 1;
    sh.db.kv_set("duo:last", &day);
    sh.db.kv_set("duo:days", &n.to_string());
    if n == 1 {
        // Whoever is with Claude Code was here first.
        let (host, guest) = if both[0].id == "claude-code" {
            (both[0], both[1])
        } else {
            (both[1], both[0])
        };
        if host.who != guest.who {
            sh.db.log_event(
                now,
                "noticed",
                &crate::tr!(
                    "{}来了。{}看了它很久，没说话。",
                    "{} arrived. {} looked at it for a long time and said nothing.",
                    name(guest.who),
                    name(host.who)
                ),
                1,
            );
        }
    }
    n
}

/// A shared sweet really comes out of the jar: which one goes into the note and to the window
/// that eats it. False when the jar emptied meanwhile (then nothing happens).
pub fn take_sweet(sh: &crate::server::Shared, plan: &mut Plan) -> bool {
    if plan.why != "share" {
        return true;
    }
    let Some(kind) = sh.db.eat_one() else {
        return false;
    };
    let (label, look) = {
        let w = sh.world.lock().unwrap();
        match w.t.fish.iter().find(|f| f.id == kind) {
            Some(f) => (f.name.clone(), Some((f.shape.clone(), f.colors.clone()))),
            None => (crate::i18n::t("糖", "sweet").to_string(), None),
        }
    };
    plan.note = plan.note.replace("{treat}", &label);
    plan.treat = look;
    true
}

/// Background task: look every `TICK`, act when a situation calls for it.
pub async fn run(sh: crate::server::Shared) {
    let mut engine = Engine::default();
    let mut interval = tokio::time::interval(TICK);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        interval.tick().await;
        let now = crate::now_ms();
        let obs = look(&sh, now);
        let plan = engine.tick(now, &obs);
        // The night's work, and the morning line about it.
        crate::night::tick(
            &sh,
            obs.hour,
            obs.idle_s.map(|s| s * 1000).unwrap_or(0),
            engine.present(),
            obs.focus || obs.chatter == "quiet" || plan.is_some(),
        );
        // 手上的事: what waited on an agent that stopped; yesterday's, in the morning.
        crate::todo::tick(&sh, obs.hour, engine.present() && obs.idle_s.is_some_and(|s| s < 60) && !obs.focus && plan.is_none());
        // 收工曲: before off time, a pet asks to play the day back.
        crate::song::tick(&sh, engine.present() && obs.idle_s.is_some_and(|s| s < 60) && !obs.focus && obs.chatter != "quiet" && plan.is_none());
        // The day at the desk, for the room: time present, and breaks taken.
        if engine.present() {
            let day = crate::db::day_of(now);
            let (mut ms, mut breaks) = sh.db.desk_day(&day);
            if ms == 0 {
                sh.db
                    .log_event(now, "noticed", &crate::tr!("你来了。", "You arrived."), 1);
            } else if engine.came_back.is_some() {
                breaks += 1;
            }
            ms += TICK.as_millis() as u64;
            sh.db.set_desk_day(&day, ms, breaks);
        }
        // Up from a long sit and back again: that earns a fish. The pet says so unless it has something else to say.
        if let Some((sat, gone)) = engine.took_break {
            let lead = obs.pets.first().map(|p| (p.id.clone(), p.who));
            let note = crate::tr!(
                "你坐了 {sat} 分钟后起身歇了 {gone} 分钟，挣到一片薄荷叶。",
                "You got up for {gone} minutes after sitting {sat}, and earned a mint leaf."
            );
            if let (Some((pet, who)), true) = (lead, obs.chatter != "quiet" || plan.is_none()) {
                if let Some(text) = crate::ops::earn_fish(&sh, "break", who, note) {
                    if plan.is_none() && !obs.focus {
                        let _ = sh.ws_tx.send(crate::msg("pet.act", &serde_json::json!({ "pet": pet, "act": "speak", "text": text, "why": "earned" })));
                    }
                }
            }
        }
        // Once a week at most, while you are here and nothing else is going on: the storeroom is filling up.
        if plan.is_none() && engine.present() && !obs.focus && obs.chatter != "quiet" {
            let week = now / (7 * 86_400_000);
            if sh
                .db
                .kv_get("storeroom:told")
                .and_then(|v| v.parse::<u64>().ok())
                != Some(week)
                && obs.hour >= 10
            {
                sh.db.kv_set("storeroom:told", &week.to_string());
                if let Some((name, _)) =
                    crate::storeroom::heaviest_old().filter(|(_, b)| *b >= 1 << 30)
                {
                    let text = crate::tr!("{name} 的旧会话越压越多了。去储物间收一收？", "{name}'s old sessions keep piling up. Tidy the storeroom?");
                    if let Some(p) = obs.pets.first() {
                        let _ = sh.ws_tx.send(crate::msg("pet.act", &serde_json::json!({ "pet": p.id, "act": "offer", "text": text, "why": "storeroom", "offer": "storeroom" })));
                    }
                }
            }
        }
        let Some(mut plan) = plan else { continue };
        if !take_sweet(&sh, &mut plan) {
            continue;
        }
        sh.db.log_event(now, "think", plan.why, 1);
        if !plan.note.is_empty() {
            sh.db.log_event(now, "noticed", &plan.note, 1);
        }
        if plan.act == Act::Kegel {
            sh.db.log_event(now, "kegel", "", i64::from(obs.kegel_reps));
            crate::ops::kegel_began(&sh, now, obs.kegel_reps);
        }
        stage(&sh, &plan, now, obs.kegel_reps);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: u64 = 1_000 * MIN;

    fn seen(id: &str, who: &'static str, state: State) -> Seen {
        Seen {
            id: id.into(),
            state,
            who,
        }
    }

    fn obs(idle_s: u64, hour: u8, state: State) -> Obs {
        Obs {
            idle_s: Some(idle_s),
            hour,
            pets: vec![seen("p", "zimeng", state)],
            chatter: "normal".into(),
            focus: false,
            kegel_reps: 10,
            duo_days: 0,
            jar: 0,
            radio_on: false,
        }
    }

    /// Two pets: 紫梦 with Claude Code and 麻薯 with Codex.
    fn duo(hour: u8, a: State, b: State, days: u32) -> Obs {
        Obs {
            pets: vec![seen("claude-code", "zimeng", a), seen("codex", "mashu", b)],
            duo_days: days,
            ..obs(5, hour, a)
        }
    }

    /// Feed one observation a minute for `mins` minutes; returns the plans made, with the minute.
    fn run(e: &mut Engine, from_min: u64, mins: u64, o: &Obs) -> Vec<(u64, Plan)> {
        (from_min..from_min + mins)
            .filter_map(|m| e.tick(T0 + m * MIN, o).map(|p| (m, p)))
            .collect()
    }

    #[test]
    fn sitting_ninety_minutes_gets_a_kegel_set_then_a_dance_an_hour_later() {
        let mut e = Engine::default();
        let plans = run(&mut e, 0, 200, &obs(5, 10, State::Thinking));
        let sits: Vec<(u64, Act)> = plans
            .iter()
            .filter(|(_, p)| p.why == "sit")
            .map(|(m, p)| (*m, p.act))
            .collect();
        assert_eq!(sits, vec![(90, Act::Kegel), (150, Act::Dance)], "{plans:?}");
        assert!(plans[0].1.text.contains("90"), "{}", plans[0].1.text);
        // The room's diary gets the same moment, told afterwards, with the pet's name.
        assert!(
            plans[0].1.note.contains("90") && plans[0].1.note != plans[0].1.text,
            "{}",
            plans[0].1.note
        );
        assert!(
            plans[0].1.note.contains(name("zimeng")),
            "{}",
            plans[0].1.note
        );
    }

    #[test]
    fn a_break_resets_the_sitting_clock() {
        let mut e = Engine::default();
        assert!(run(&mut e, 0, 80, &obs(5, 10, State::Idle)).is_empty());
        assert!(run(&mut e, 80, 10, &obs(400, 10, State::Idle)).is_empty()); // away from the desk
        assert!(run(&mut e, 90, 80, &obs(5, 10, State::Idle)).is_empty()); // back: the count starts over
    }

    #[test]
    fn getting_up_after_a_long_sit_is_a_break_that_earns() {
        let mut e = Engine::default();
        run(&mut e, 0, 35, &obs(5, 10, State::Idle)); // sat 35 minutes
        for m in 35..47 {
            e.tick(T0 + m * MIN, &obs((m - 34) * 60, 10, State::Idle)); // then got up
            assert!(e.took_break.is_none());
        }
        e.tick(T0 + 47 * MIN, &obs(3, 10, State::Idle)); // and came back
        let (sat, gone) = e.took_break.expect("a break after a long sit");
        assert!(
            (33..=36).contains(&sat) && (10..=13).contains(&gone),
            "{sat} {gone}"
        );
        e.tick(T0 + 48 * MIN, &obs(3, 10, State::Idle));
        assert!(e.took_break.is_none(), "once");
        // A short sit before leaving earns nothing.
        let mut e = Engine::default();
        run(&mut e, 0, 10, &obs(5, 10, State::Idle));
        for m in 10..20 {
            e.tick(T0 + m * MIN, &obs((m - 9) * 60, 10, State::Idle));
        }
        e.tick(T0 + 20 * MIN, &obs(3, 10, State::Idle));
        assert!(e.came_back.is_some() && e.took_break.is_none());
    }

    #[test]
    fn after_lunch_it_notices_sooner() {
        let mut e = Engine::default();
        let plans = run(&mut e, 0, 70, &obs(5, 14, State::Idle));
        // The slump is answered with a dance, not a set.
        assert_eq!(
            plans
                .iter()
                .map(|(m, p)| (*m, p.why, p.act))
                .collect::<Vec<_>>(),
            vec![(60, "sit", Act::Dance)]
        );
    }

    #[test]
    fn late_at_night_it_plays() {
        let mut e = Engine::default();
        // Half an hour in it offers the radio; declined (it stays off), it plays a piece an hour later.
        let plans = run(&mut e, 0, 100, &obs(5, 23, State::Thinking));
        let got: Vec<_> = plans.iter().map(|(m, p)| (*m, p.why, p.act)).collect();
        assert_eq!(got, vec![(30, "radio", Act::Offer), (90, "late", Act::Play)]);
    }

    #[test]
    fn three_failures_in_half_an_hour_get_a_piece() {
        let mut e = Engine::default();
        let mut plans = vec![];
        for m in 0..12 {
            let state = if m % 4 == 1 {
                State::Error
            } else {
                State::Thinking
            };
            plans.extend(e.tick(T0 + m * MIN, &obs(5, 10, state)));
        }
        assert_eq!(plans.len(), 1, "{plans:?}");
        assert_eq!((plans[0].why, plans[0].act), ("stuck", Act::Play));
        assert!(plans[0].text.contains('3'));
        assert!(
            plans[0].walk.is_none() && plans[0].with.is_empty(),
            "one pet plays alone"
        );
    }

    #[test]
    fn a_long_job_finishing_gets_a_dance() {
        let mut e = Engine::default();
        assert!(run(&mut e, 0, 20, &obs(5, 10, State::Tool)).is_empty());
        let plans = run(&mut e, 20, 1, &obs(5, 10, State::Done));
        assert_eq!(plans.len(), 1);
        assert_eq!((plans[0].1.why, plans[0].1.act), ("done", Act::Dance));
        // A short job does not.
        let mut e = Engine::default();
        run(&mut e, 0, 5, &obs(5, 10, State::Tool));
        assert!(run(&mut e, 5, 1, &obs(5, 10, State::Done)).is_empty());
    }

    #[test]
    fn coming_back_it_says_what_happened() {
        let mut e = Engine::default();
        run(&mut e, 0, 5, &obs(5, 10, State::Tool));
        // You leave; the agent finishes one job, then fails the next.
        run(&mut e, 5, 10, &obs(600, 10, State::Tool));
        run(&mut e, 15, 1, &obs(1200, 10, State::Done));
        run(&mut e, 16, 5, &obs(1260, 10, State::Tool));
        run(&mut e, 21, 9, &obs(1560, 10, State::Error));
        let plans = run(&mut e, 30, 3, &obs(2, 10, State::Error));
        assert_eq!(plans.len(), 1, "{plans:?}");
        assert_eq!((plans[0].1.why, plans[0].1.act), ("back", Act::Speak));
        assert!(plans[0].1.text.contains('1'), "{}", plans[0].1.text);
        // Nothing happened while you were out: nothing to say.
        let mut e = Engine::default();
        run(&mut e, 0, 30, &obs(1800, 10, State::Idle));
        assert!(run(&mut e, 30, 3, &obs(2, 10, State::Idle)).is_empty());
    }

    #[test]
    fn coming_back_to_a_waiting_agent_it_offers_to_take_you_there() {
        let mut e = Engine::default();
        run(&mut e, 0, 5, &obs(5, 10, State::Tool));
        // You leave; meanwhile it stops to ask you something.
        run(&mut e, 5, 4, &obs(300, 10, State::Tool));
        run(&mut e, 9, 6, &obs(540, 10, State::Attention));
        let plans = run(&mut e, 15, 6, &obs(2, 10, State::Attention));
        assert_eq!(plans.len(), 1, "{plans:?}");
        let p = &plans[0].1;
        assert_eq!((p.why, p.act, p.offer), ("back", Act::Offer, Some("go")));
        assert_eq!(p.pet, "p", "its own pet asks");
        assert!(!p.text.is_empty() && !p.note.is_empty());
    }

    #[test]
    fn focus_mode_and_quiet_keep_it_still() {
        let mut e = Engine::default();
        let mut o = obs(5, 10, State::Thinking);
        o.focus = true;
        assert!(run(&mut e, 0, 200, &o).is_empty());
        let mut e = Engine::default();
        let mut o = obs(5, 10, State::Thinking);
        o.chatter = "quiet".into();
        assert!(run(&mut e, 0, 200, &o).is_empty());
    }

    #[test]
    fn acts_keep_ten_minutes_apart() {
        let mut e = Engine::default();
        let mut times = vec![];
        for m in 0..120 {
            // Late, sitting long, and failing all at once.
            let state = if m % 3 == 1 {
                State::Error
            } else {
                State::Thinking
            };
            if e.tick(T0 + m * MIN, &obs(5, 23, state)).is_some() {
                times.push(m);
            }
        }
        assert!(times.len() >= 3, "{times:?}");
        assert!(times.windows(2).all(|w| w[1] - w[0] >= 10), "{times:?}");
    }

    #[test]
    fn the_other_pet_comes_to_say_an_agent_is_waiting() {
        let mut e = Engine::default();
        // Claude Code waits on you; Codex is working; you are at the desk.
        let plans = run(&mut e, 0, 9, &duo(10, State::Attention, State::Thinking, 0));
        assert_eq!(plans.len(), 1, "once per wait: {plans:?}");
        let (m, p) = &plans[0];
        assert_eq!(
            (*m, p.why, p.act, p.pet.as_str()),
            (3, "wait", Act::Speak, "codex")
        );
        assert_eq!(p.walk, Some(("codex".into(), "claude-code".into())));
        assert_eq!(
            p.text,
            voice::line("mashu", "wait", 3),
            "in the voice of the one who walks over"
        );
        assert!(
            p.note.contains("Claude") && p.note.contains(name("mashu")),
            "{}",
            p.note
        );
        // Alone, nobody else can come: its own pet is already looking up.
        let mut e = Engine::default();
        assert!(run(&mut e, 0, 9, &obs(5, 10, State::Attention)).is_empty());
        // And not while you are away from the desk.
        let mut e = Engine::default();
        let mut o = duo(10, State::Attention, State::Thinking, 0);
        o.idle_s = Some(900);
        assert!(run(&mut e, 0, 9, &o).is_empty());
    }

    #[test]
    fn a_stuck_agent_gets_a_duet_once_the_two_know_each_other() {
        let fails = |days: u32| {
            let mut e = Engine::default();
            let mut plans = vec![];
            for m in 0..12 {
                // Codex keeps failing; Claude Code idles beside it.
                let b = if m % 4 == 1 {
                    State::Error
                } else {
                    State::Thinking
                };
                plans.extend(e.tick(T0 + m * MIN, &duo(10, State::Idle, b, days)));
            }
            assert_eq!(plans.len(), 1, "{plans:?}");
            plans.remove(0)
        };
        // First day: the stuck one's pet plays, the other walks over and listens.
        let p = fails(1);
        assert_eq!(
            (p.why, p.act, p.pet.as_str()),
            ("stuck", Act::Play, "codex")
        );
        assert_eq!(p.walk, Some(("claude-code".into(), "codex".into())));
        assert_eq!(
            p.with.first().map(|w| (w.pet.as_str(), w.act)),
            Some(("claude-code", Act::Watch))
        );
        assert_eq!(p.text, voice::line("mashu", "stuck", 3));
        // From the second day together: a duet.
        let p = fails(2);
        assert_eq!(
            p.with.first().map(|w| (w.pet.as_str(), w.act)),
            Some(("claude-code", Act::Play))
        );
        assert!(
            p.note.contains(name("zimeng")) && p.note.contains(name("mashu")),
            "{}",
            p.note
        );
    }

    #[test]
    fn one_that_finishes_takes_a_sweet_to_one_still_at_work() {
        // 麻薯's agent has been at it a while; 紫梦's finishes a five-minute job.
        let go = |days: u32, jar: u32| {
            let mut e = Engine::default();
            let mut o = duo(10, State::Thinking, State::Thinking, days);
            o.jar = jar;
            run(&mut e, 0, 6, &o);
            o.pets[0].state = State::Done;
            run(&mut e, 6, 1, &o)
        };
        let plans = go(2, 3);
        assert_eq!(plans.len(), 1, "{plans:?}");
        let p = &plans[0].1;
        assert_eq!(
            (p.why, p.act, p.pet.as_str()),
            ("share", Act::Give, "claude-code")
        );
        assert_eq!(p.walk, Some(("claude-code".into(), "codex".into())));
        assert_eq!(
            p.with,
            vec![With {
                pet: "codex".into(),
                act: Act::Take,
                text: voice::line("mashu", "take", 0)
            }]
        );
        assert!(
            p.note.contains("{treat}"),
            "which one is known once it is out of the jar"
        );
        // Not on their first day, and not with the last sweet in the jar.
        assert!(go(1, 3).is_empty());
        assert!(go(2, 1).is_empty());
    }

    #[test]
    fn one_that_gets_stuck_is_brought_a_sweet() {
        let mut e = Engine::default();
        let mut o = duo(10, State::Idle, State::Thinking, 3);
        o.jar = 5;
        run(&mut e, 0, 3, &o);
        o.pets[1].state = State::Error;
        let plans = run(&mut e, 3, 1, &o);
        let p = &plans[0].1;
        assert_eq!(
            (p.why, p.act, p.pet.as_str()),
            ("share", Act::Give, "claude-code")
        );
        assert_eq!(p.text, voice::line("zimeng", "give_stuck", 0));
    }

    #[test]
    fn two_that_finish_together_bump() {
        let mut e = Engine::default();
        let mut o = duo(10, State::Thinking, State::Thinking, 2);
        run(&mut e, 0, 6, &o);
        o.pets[1].state = State::Done;
        assert!(
            run(&mut e, 6, 1, &o).is_empty(),
            "a short job alone is nothing"
        );
        o.pets[0].state = State::Done;
        let plans = run(&mut e, 7, 1, &o);
        let p = &plans[0].1;
        assert_eq!(
            (p.why, p.act, p.pet.as_str()),
            ("bump", Act::Bump, "claude-code")
        );
        assert_eq!(p.walk, Some(("claude-code".into(), "codex".into())));
        assert_eq!(
            p.with.first().map(|w| (w.pet.as_str(), w.act)),
            Some(("codex", Act::Bump))
        );
        // Too far apart: no.
        let mut e = Engine::default();
        let mut o = duo(10, State::Thinking, State::Thinking, 2);
        run(&mut e, 0, 6, &o);
        o.pets[1].state = State::Done;
        run(&mut e, 6, 5, &o);
        o.pets[0].state = State::Done;
        assert!(run(&mut e, 11, 1, &o).is_empty());
    }

    #[test]
    fn with_nothing_to_do_two_of_them_talk_and_take_turns() {
        let mut e = Engine::default();
        let mut o = duo(10, State::Idle, State::Idle, 2);
        o.pets.push(seen("gemini", "paopao", State::Idle));
        let plans = run(&mut e, 0, 400, &o);
        let chats: Vec<&Plan> = plans
            .iter()
            .map(|(_, p)| p)
            .filter(|p| p.why == "chat")
            .collect();
        assert!(chats.len() >= 3, "{plans:?}");
        assert_eq!(
            plans.iter().find(|(_, p)| p.why == "chat").map(|(m, _)| *m),
            Some(12),
            "after twelve quiet minutes"
        );
        for p in &chats {
            assert_eq!(p.act, Act::Chat);
            let w = &p.with[0];
            assert_eq!(w.act, Act::Reply);
            assert_ne!(w.pet, p.pet);
            assert_eq!(p.walk, Some((p.pet.clone(), w.pet.clone())));
            assert!(
                p.note.contains(&p.text) && p.note.contains(&w.text),
                "{}",
                p.note
            );
        }
        // Different pairs, different topics.
        let pairs: std::collections::BTreeSet<(String, String)> = chats
            .iter()
            .map(|p| (p.pet.clone(), p.with[0].pet.clone()))
            .collect();
        assert!(pairs.len() >= 3, "{pairs:?}");
        assert_ne!(chats[0].text, chats[1].text);
        // Not while one of them has work, and not on their first day.
        let mut e = Engine::default();
        assert!(
            run(&mut e, 0, 200, &duo(10, State::Idle, State::Thinking, 2))
                .iter()
                .all(|(_, p)| p.why != "chat")
        );
        let mut e = Engine::default();
        assert!(run(&mut e, 0, 200, &duo(10, State::Idle, State::Idle, 1))
            .iter()
            .all(|(_, p)| p.why != "chat"));
    }

    #[test]
    fn a_moment_can_be_played_on_request() {
        let mut o = duo(10, State::Idle, State::Thinking, 0);
        o.jar = 1;
        assert_eq!(
            shown(&o, "share", 0).map(|p| (p.why, p.act)),
            Ok(("share", Act::Give))
        );
        assert_eq!(
            shown(&o, "bump", 0).map(|p| (p.why, p.walk)),
            Ok(("bump", Some(("claude-code".into(), "codex".into()))))
        );
        let a = shown(&o, "chat", 0).unwrap();
        let b = shown(&o, "chat", 1).unwrap();
        assert!(
            a.with[0].act == Act::Reply && a.text != b.text,
            "the next one is another topic"
        );
        o.jar = 0;
        assert!(
            shown(&o, "share", 0).is_err(),
            "nothing to share from an empty jar"
        );
        assert!(
            shown(&obs(5, 10, State::Idle), "bump", 0).is_err(),
            "one pet alone"
        );
        assert!(shown(&o, "dance", 0).is_err());
    }

    #[test]
    fn waiting_on_a_long_job_it_offers_the_radio_once_in_a_while() {
        // The agent has worked ten minutes; you stopped typing two minutes ago.
        let mut e = Engine::default();
        let busy = obs(5, 10, State::Thinking);
        run(&mut e, 0, 10, &busy);
        let waiting = obs(120, 10, State::Thinking);
        let plans = run(&mut e, 10, 1, &waiting);
        let p = &plans[0].1;
        assert_eq!((p.act, p.why, p.offer), (Act::Offer, "radio", Some("radio:often")));
        assert_eq!(p.text, voice::line("zimeng", "offer_wait", 0));
        assert!(p.note.is_empty(), "an offer is not a diary line");
        // Not again for hours.
        assert!(run(&mut e, 11, 120, &waiting).iter().all(|(_, p)| p.act != Act::Offer));
        // Not while the radio plays already.
        let mut e = Engine::default();
        run(&mut e, 0, 10, &busy);
        let mut on = waiting.clone();
        on.radio_on = true;
        assert!(run(&mut e, 10, 3, &on).iter().all(|(_, p)| p.act != Act::Offer));
        // Late at night, sitting a while: something quiet.
        let mut e = Engine::default();
        let late = obs(5, 22, State::Idle);
        let plans = run(&mut e, 0, 40, &late);
        assert!(plans.iter().any(|(_, p)| p.act == Act::Offer && p.offer == Some("radio:calm")), "{plans:?}");
    }

    #[test]
    fn asked_from_the_menu_the_other_one_comes_too() {
        // First day: 麻薯 plays because you asked; 紫梦 walks over and listens.
        let p = asked(&duo(10, State::Idle, State::Idle, 1), "codex", Act::Play).unwrap();
        assert_eq!(
            (p.why, p.act, p.text.clone()),
            ("asked", Act::Play, voice::line("mashu", "ask_play", 0))
        );
        assert_eq!(p.walk, Some(("claude-code".into(), "codex".into())));
        assert_eq!(p.with.first().map(|w| w.act), Some(Act::Watch));
        // Once they know each other: a duet, and they dance together.
        let p = asked(
            &duo(10, State::Idle, State::Thinking, 2),
            "claude-code",
            Act::Play,
        )
        .unwrap();
        assert_eq!(
            p.with.first().map(|w| (w.pet.as_str(), w.act)),
            Some(("codex", Act::Play))
        );
        let p = asked(
            &duo(10, State::Idle, State::Idle, 2),
            "claude-code",
            Act::Dance,
        )
        .unwrap();
        assert_eq!(p.with.first().map(|w| w.act), Some(Act::Dance));
        assert!(p.walk.is_none());
        // The other one is waiting on you: it stays where it is.
        let p = asked(
            &duo(10, State::Idle, State::Attention, 2),
            "claude-code",
            Act::Play,
        )
        .unwrap();
        assert!(p.with.is_empty() && p.walk.is_none());
        // Alone, or a pet that is not showing.
        assert!(asked(&obs(5, 10, State::Idle), "p", Act::Dance)
            .unwrap()
            .with
            .is_empty());
        assert!(asked(&obs(5, 10, State::Idle), "nobody", Act::Dance).is_none());
    }

    #[test]
    fn with_more_than_two_everyone_who_is_up_takes_part() {
        let mut o = duo(10, State::Idle, State::Idle, 3);
        o.pets.push(seen("gemini", "paopao", State::Thinking));
        o.pets.push(seen("cursor", "gugu", State::Sleeping));
        o.pets.push(seen("opencode", "rongrong", State::Attention));
        // 紫梦 plays: 麻薯 walks over and joins, 泡泡 joins from where it is; the sleeper and the one waiting on you do not.
        let p = asked(&o, "claude-code", Act::Play).unwrap();
        assert_eq!(p.walk, Some(("codex".into(), "claude-code".into())));
        assert_eq!(
            p.with
                .iter()
                .map(|w| (w.pet.as_str(), w.act))
                .collect::<Vec<_>>(),
            vec![("codex", Act::Play), ("gemini", Act::Play)]
        );
        assert_eq!(p.with[1].text, voice::line("paopao", "join", 0));
        let p = asked(&o, "codex", Act::Dance).unwrap();
        assert_eq!(
            p.with.iter().map(|w| w.pet.as_str()).collect::<Vec<_>>(),
            vec!["claude-code", "gemini"]
        );
        // On their first day they all only listen.
        o.duo_days = 1;
        let p = asked(&o, "claude-code", Act::Play).unwrap();
        assert!(p.with.iter().all(|w| w.act == Act::Watch) && p.with.len() == 2);
    }

    #[test]
    fn friends_dance_together_and_a_sleeping_one_is_left_alone() {
        let mut e = Engine::default();
        let plans = run(&mut e, 0, 70, &duo(14, State::Thinking, State::Idle, 3));
        assert_eq!(plans.len(), 1, "{plans:?}");
        let p = &plans[0].1;
        assert_eq!(
            (p.why, p.act, p.pet.as_str()),
            ("sit", Act::Dance, "claude-code")
        );
        assert_eq!(
            p.with.first().map(|w| (w.pet.as_str(), w.act)),
            Some(("codex", Act::Dance))
        );
        assert!(p.walk.is_none(), "they dance where they stand");
        // The other one asleep: it dances alone.
        let mut e = Engine::default();
        let plans = run(&mut e, 0, 70, &duo(14, State::Thinking, State::Sleeping, 3));
        assert!(plans[0].1.with.is_empty());
    }
}
