//! Sessions -> pets. Owns all live state; the server and timers lock it briefly.
//!
//! Routing (design §4):
//!   per-camp     one main pet per camp (+ one per project override), each live
//!                session of the pet shown as a mini clone beside it
//!   single       one pet for everything, no minis
//!   per-rule     one pet per `pets:` rule; unmatched sessions go to a fallback pet
//!   per-session  one pet per live session, up to `max_pets`

use crate::config::{Config, Mode};
use crate::pack::Library;
use crate::protocol::Event;
use crate::session::{FinishedTurn, Session, State, FORGET_AFTER};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

pub type Key = (String, String);

/// A whip that has not reached the agent yet.
#[derive(Debug, Clone)]
pub struct Nudge {
    /// `user` (from the menu) or `slow` (the other camp came over).
    pub reason: String,
    pub count: u32,
    pub ts: u64,
}

/// A lash older than this is not worth mentioning any more.
const NUDGE_TTL: u64 = 20 * 60_000;

/// What happened, for the world layer (drained every tick).
#[derive(Debug, Clone)]
pub enum Feed {
    Event { key: Key, ev: Event },
    Finished { key: Key, project: String, fin: FinishedTurn },
    State { key: Key, from: State, to: State },
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SessionCard {
    pub key: String,
    pub source: String,
    pub session: String,
    pub project: String,
    pub pid: u32,
    pub state: State,
    pub since: u64,
    pub started_at: u64,
    pub updated_at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Bubble {
    /// Key into the pack's `bubbles` table (attention, done, error…).
    pub key: String,
    /// Project prefix, only when the pet shows more than one session.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MiniView {
    pub key: String,
    pub source: String,
    pub project: String,
    pub state: State,
    pub since: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PetView {
    pub id: String,
    pub pack: String,
    /// Bumped whenever the pack library reloads, so windows refetch sprites.
    pub pack_rev: u64,
    pub state: State,
    /// When the shown state began (ms). Lets the window time one-shot bubbles.
    pub since: u64,
    pub camp: Option<String>,
    /// What a pack switch on this pet changes: `claude-code`, `codex`,
    /// `project:<pattern>` or `rule:<id>`.
    pub target: String,
    /// Shown under the pet when it is not a plain camp pet (project / rule name).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Hue rotation in degrees (same pack picked by both camps).
    pub hue: i32,
    /// Camp tag under the pet ("Claude" / "Codex").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub marker: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attention_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bubble: Option<Bubble>,
    pub online: usize,
    pub waiting: usize,
    pub sessions: Vec<SessionCard>,
    pub minis: Vec<MiniView>,
    pub minis_more: usize,
    /// Gameplay activity set by the world layer, e.g. "fishing".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activity: Option<String>,
    /// Display multiplier from `features.size` (S 0.75, M 1, L 1.35).
    pub size: f32,
    /// What the pet wears on its head: the season crown / loser hat wins over
    /// an equipped shop hat.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hat: Option<String>,
    /// The rest of the outfit from the shop: slot -> item (face, neck, back,
    /// feet, hand, aura). Hats have their own field because the season
    /// crown overrides them.
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub wear: std::collections::BTreeMap<String, String>,
    /// Evolution stage (0 = none; 1–3 at 100 / 500 / 2000 turns of this pack).
    pub stage: u8,
}

pub struct Hub {
    pub cfg: Config,
    pub lib: Library,
    pub lib_rev: u64,
    /// Last config.yaml parse error, shown in the gallery.
    pub cfg_error: Option<String>,
    pub sessions: HashMap<Key, Session>,
    /// Closed turns waiting for storage (SQLite lands in M4).
    pub finished: Vec<(Key, FinishedTurn)>,
    last_sent: HashMap<String, PetView>,
    last_layout: Vec<String>,
    /// Everything hidden (/pet hide, tray).
    pub hidden: bool,
    /// Single pets hidden from their right-click menu.
    pub hidden_pets: HashSet<String>,
    /// Session that sent the most recent event (decides "current camp").
    pub last_key: Option<Key>,
    pub feed: Vec<Feed>,
    /// pet id -> activity ("fishing"), owned by the world layer.
    pub activities: HashMap<String, String>,
    /// pet id -> hat, owned by the world layer.
    pub hats: HashMap<String, String>,
    /// Lashes waiting to be told to the agent behind a session, on its next hook.
    pub nudges: HashMap<Key, Nudge>,
    /// pet id -> (slot -> item) for the non-hat slots, owned by the world layer.
    pub wear: HashMap<String, std::collections::BTreeMap<String, String>>,
    /// pet id -> evolution stage, owned by the world layer.
    pub stages: HashMap<String, u8>,
}

/// One pet before it is turned into a view.
struct Group<'a> {
    id: String,
    target: String,
    camp: Option<String>,
    pack: String,
    label: Option<String>,
    minis: bool,
    sessions: Vec<&'a Session>,
}

fn short_hash(s: &str) -> String {
    // FNV-1a: stable across runs, enough to keep window labels unique.
    let mut h: u32 = 0x811c9dc5;
    for b in s.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(0x01000193);
    }
    format!("{h:08x}")
}

/// Window labels only allow a small character set.
fn safe_id(s: &str) -> String {
    if !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        s.to_string()
    } else {
        format!("x-{}", short_hash(s))
    }
}

fn camp_target(source: &str) -> String {
    source.to_string()
}

impl Hub {
    pub fn new(cfg: Config, lib: Library) -> Self {
        Hub {
            cfg,
            lib,
            lib_rev: 1,
            cfg_error: None,
            sessions: HashMap::new(),
            finished: Vec::new(),
            last_sent: HashMap::new(),
            last_layout: Vec::new(),
            hidden: false,
            hidden_pets: HashSet::new(),
            last_key: None,
            feed: Vec::new(),
            activities: HashMap::new(),
            hats: HashMap::new(),
            nudges: HashMap::new(),
            wear: HashMap::new(),
            stages: HashMap::new(),
        }
    }

    /// Returns true when the change is urgent (someone is now waiting on the user).
    pub fn apply(&mut self, ev: &Event, now: u64) -> bool {
        if ev.session.is_empty() || ev.source.is_empty() {
            return false;
        }
        let key = (ev.source.clone(), ev.session.clone());
        let s = self.sessions.entry(key.clone()).or_insert_with(|| Session::new(ev, now));
        let was = s.state;
        let fin = s.apply(ev, now);
        let (to, project) = (s.state, s.project.clone());
        // A turn the event closed (e.g. a new prompt cutting the old turn
        // short) is reported before the event itself: the old turn ends
        // first, then the new one starts.
        if let Some(fin) = fin {
            self.push_feed(Feed::Finished { key: key.clone(), project, fin: fin.clone() });
            self.finished.push((key.clone(), fin));
            if self.finished.len() > 1000 {
                self.finished.drain(..500);
            }
        }
        self.push_feed(Feed::Event { key: key.clone(), ev: ev.clone() });
        if to != was {
            self.push_feed(Feed::State { key: key.clone(), from: was, to });
        }
        let urgent = to == State::Attention && was != State::Attention;
        self.last_key = Some(key);
        urgent
    }

    fn push_feed(&mut self, f: Feed) {
        if self.feed.len() >= 2000 {
            self.feed.drain(..1000);
        }
        self.feed.push(f);
    }

    pub fn tick(&mut self, now: u64) {
        let mut changes = Vec::new();
        for (k, s) in self.sessions.iter_mut() {
            let was = s.state;
            if s.tick(now) {
                changes.push(Feed::State { key: k.clone(), from: was, to: s.state });
            }
        }
        for c in changes {
            self.push_feed(c);
        }
        self.sessions.retain(|_, s| !(s.state == State::Offline && now.saturating_sub(s.state_since) > FORGET_AFTER));
    }

    /// Mark sessions whose agent process is gone as offline.
    pub fn reap(&mut self, now: u64, alive: impl Fn(u32) -> bool) -> usize {
        let mut n = 0;
        for (key, s) in self.sessions.iter_mut() {
            if s.state != State::Offline && s.pid != 0 && !alive(s.pid) {
                let was = s.state;
                if let Some(fin) = s.mark_dead(now) {
                    self.finished.push((key.clone(), fin));
                }
                self.feed.push(Feed::State { key: key.clone(), from: was, to: State::Offline });
                n += 1;
            }
        }
        n
    }

    /// Boss bug squashed: un-stone that session.
    pub fn clear_stone(&mut self, key: &Key, now: u64) {
        if let Some(s) = self.sessions.get_mut(key) {
            let was = s.state;
            if s.clear_stone(now) {
                let to = s.state;
                self.push_feed(Feed::State { key: key.clone(), from: was, to });
            }
        }
    }

    /// Pet currently showing a session (if any).
    pub fn pet_of(&self, key: &Key) -> Option<PetView> {
        let skey = format!("{}/{}", key.0, key.1);
        self.pets().into_iter().find(|v| v.sessions.iter().any(|c| c.key == skey))
    }

    fn pack_or_fallback(&self, want: &str) -> String {
        self.lib.pick(want).map(|p| p.id.clone()).unwrap_or_else(|| want.to_string())
    }

    /// Camp pet or project-override pet for one session.
    fn camp_slot(&self, s: &Session) -> (String, String, String, Option<String>) {
        if let Some((pat, pack)) = self.cfg.project_override(&s.project) {
            let id = format!("p-{}", short_hash(&pat.to_lowercase()));
            return (id, format!("project:{pat}"), pack.to_string(), Some(s.project_name().to_string()));
        }
        (safe_id(&s.source), camp_target(&s.source), self.cfg.pack_for_source(&s.source), None)
    }

    /// Queue a lash for every live session behind `pet`; `take_nudge` hands it on.
    pub fn nudge(&mut self, pet: &str, reason: &str, now: u64) {
        let keys: Vec<Key> = self
            .groups()
            .into_iter()
            .filter(|g| g.id == pet)
            .flat_map(|g| g.sessions)
            .filter(|s| s.state != State::Offline)
            .map(|s| (s.source.clone(), s.session.clone()))
            .collect();
        for k in keys {
            let n = self.nudges.entry(k).or_insert(Nudge { reason: reason.into(), count: 0, ts: now });
            n.count += 1;
            n.ts = now;
            n.reason = reason.into();
        }
    }

    /// The note for the agent behind this session, if a lash is waiting: what
    /// it meant and what to do about it, by what the session is doing now.
    pub fn take_nudge(&mut self, source: &str, session: &str, now: u64) -> Option<String> {
        let key = (source.to_string(), session.to_string());
        let n = self.nudges.remove(&key)?;
        if now.saturating_sub(n.ts) > NUDGE_TTL {
            return None;
        }
        let state = self.sessions.get(&key).map(|s| s.state).unwrap_or(State::Idle);
        let nth = if n.count > 1 { crate::tr!("（已经第 {} 下了）", " (that makes {})", n.count) } else { String::new() };
        let body = match (n.reason.as_str(), state) {
            ("slow", _) => crate::tr!(
                "另一边的宠物嫌你这一轮拖得太久，拿着鞭子过来抽了一下{}。先把手头这一步收个尾，用一句话说明进展，别再开新的摊子。",
                "The other team's pet came over and cracked the whip: this turn has been dragging{}. Finish the step in hand, say where things stand in one line, and don't open anything new.",
                nth
            ),
            (_, State::Error | State::Stone | State::Aborted) => crate::tr!(
                "主人对你抽了一鞭子{}。刚才出了错：先修它，别绕开；修好后用一句话说明原因。",
                "Your user cracked the whip at you{}. Something just failed: fix that first, don't route around it, and give the cause in one line once it works.",
                nth
            ),
            (_, State::Thinking | State::Tool | State::Reading) => crate::tr!(
                "主人对你抽了一鞭子{}，意思是：快一点。下一条回复开头先用一句话说清做到哪、还剩什么，然后少解释、多动手；卡住了就直说卡在哪。",
                "Your user cracked the whip at you{}. It means: faster. Open your next reply with one line on where you are and what is left, then do more and explain less; if you are stuck, say where.",
                nth
            ),
            _ => crate::tr!(
                "主人对你抽了一鞭子{}。他想知道进度。下一条回复先用两三句话汇报：做完了什么，还剩什么，有没有需要他拍板的事。",
                "Your user cracked the whip at you{}. They want to know where things stand. Open your next reply with two or three lines: what is done, what is left, anything that needs their call.",
                nth
            ),
        };
        Some(format!("[Familiar] {body}"))
    }

    fn groups(&self) -> Vec<Group<'_>> {
        let mut all: Vec<&Session> = self.sessions.values().collect();
        all.sort_by(|a, b| a.started_at.cmp(&b.started_at).then_with(|| a.session.cmp(&b.session)));
        let live = |s: &&Session| s.state != State::Offline;
        let mut out: Vec<Group> = Vec::new();

        match self.cfg.mode {
            Mode::Single => {
                let sessions: Vec<&Session> = all.iter().copied().filter(live).collect();
                let top = sessions.iter().max_by_key(|s| (s.state.priority(), s.state_since));
                let source = top.map(|s| s.source.clone()).unwrap_or_else(|| "claude-code".into());
                out.push(Group {
                    id: "main".into(),
                    target: camp_target(&source),
                    camp: None,
                    pack: self.cfg.pack_for_source(&source),
                    label: None,
                    minis: false,
                    sessions,
                });
            }
            Mode::PerCamp => {
                for s in &all {
                    let (id, target, pack, label) = self.camp_slot(s);
                    // Project pets exist only while they have live sessions;
                    // camp pets stay (asleep) while any session is remembered.
                    let is_project = target.starts_with("project:");
                    if is_project && !live(s) {
                        continue;
                    }
                    if let Some(g) = out.iter_mut().find(|g| g.id == id) {
                        g.sessions.push(s);
                    } else {
                        out.push(Group {
                            id,
                            target,
                            camp: Some(s.source.clone()),
                            pack,
                            label,
                            minis: true,
                            sessions: vec![s],
                        });
                    }
                }
            }
            Mode::PerRule => {
                for r in &self.cfg.pets {
                    out.push(Group {
                        id: format!("r-{}", safe_id(&r.id)),
                        target: format!("rule:{}", r.id),
                        camp: r.matcher.source.clone(),
                        pack: r.pack.clone(),
                        label: Some(r.id.clone()),
                        minis: false,
                        sessions: vec![],
                    });
                }
                for s in &all {
                    match self.cfg.rule_for(&s.source, &s.project) {
                        Some(r) => {
                            let id = format!("r-{}", safe_id(&r.id));
                            if let Some(g) = out.iter_mut().find(|g| g.id == id) {
                                g.sessions.push(s);
                            }
                        }
                        None if live(s) => {
                            if let Some(g) = out.iter_mut().find(|g| g.id == "fallback") {
                                g.sessions.push(s);
                            } else {
                                out.push(Group {
                                    id: "fallback".into(),
                                    target: camp_target(&s.source),
                                    camp: None,
                                    pack: self
                                        .cfg
                                        .fallback_pet
                                        .clone()
                                        .unwrap_or_else(|| self.cfg.pack_for_source(&s.source)),
                                    label: None,
                                    minis: false,
                                    sessions: vec![s],
                                });
                            }
                        }
                        None => {}
                    }
                }
            }
            Mode::PerSession => {
                let sessions: Vec<&Session> = all.iter().copied().filter(live).collect();
                let max = self.cfg.max_pets.max(1) as usize;
                let own = if sessions.len() > max { max - 1 } else { sessions.len() };
                for s in &sessions[..own] {
                    let (_, target, pack, _) = self.camp_slot(s);
                    out.push(Group {
                        id: format!("s-{}", short_hash(&format!("{}/{}", s.source, s.session))),
                        target,
                        camp: Some(s.source.clone()),
                        pack,
                        label: Some(s.project_name().to_string()),
                        minis: false,
                        sessions: vec![s],
                    });
                }
                if sessions.len() > own {
                    let rest: Vec<&Session> = sessions[own..].to_vec();
                    let source = rest[0].source.clone();
                    out.push(Group {
                        id: "s-more".into(),
                        target: camp_target(&source),
                        camp: None,
                        pack: self.cfg.pack_for_source(&source),
                        label: None,
                        minis: false,
                        sessions: rest,
                    });
                }
            }
        }

        // The Claude Code camp pet always stands (asleep when idle), so the
        // user's main pet never vanishes just because petd restarted or only
        // Codex is busy. Other camps appear once they have sessions.
        if self.cfg.mode == Mode::PerCamp && !out.iter().any(|g| g.id == "claude-code") {
            out.insert(
                0,
                Group {
                    id: "claude-code".into(),
                    target: "claude-code".into(),
                    camp: Some("claude-code".into()),
                    pack: self.cfg.pack_for_source("claude-code"),
                    label: None,
                    minis: true,
                    sessions: vec![],
                },
            );
        }
        if out.is_empty() {
            out.push(Group {
                id: if self.cfg.mode == Mode::PerCamp { "claude-code".into() } else { "main".into() },
                target: "claude-code".into(),
                camp: Some("claude-code".into()),
                pack: self.cfg.pack_for_source("claude-code"),
                label: None,
                minis: false,
                sessions: vec![],
            });
        }
        out
    }

    /// Every visible pet with its state.
    pub fn pets(&self) -> Vec<PetView> {
        let mut views: Vec<PetView> =
            self.groups().into_iter().filter(|g| !self.hidden_pets.contains(&g.id)).map(|g| self.view(g)).collect();
        for v in &mut views {
            v.activity = self.activities.get(&v.id).cloned();
            v.hat = self.hats.get(&v.id).cloned();
            v.wear = self.wear.get(&v.id).cloned().unwrap_or_default();
            v.stage = self.stages.get(&v.id).copied().unwrap_or(0);
        }

        // Every camp pet wears its camp tag; the same pack in both camps also
        // gets a hue shift on the Codex side.
        let packs_of = |camp: &str, views: &[PetView]| -> HashSet<String> {
            views.iter().filter(|v| v.camp.as_deref() == Some(camp)).map(|v| v.pack.clone()).collect()
        };
        let shared: HashSet<String> =
            packs_of("claude-code", &views).intersection(&packs_of("codex", &views)).cloned().collect();
        for v in &mut views {
            match v.camp.as_deref() {
                Some("claude-code") => v.marker = Some("Claude".into()),
                Some("codex") => {
                    v.marker = Some("Codex".into());
                    if shared.contains(&v.pack) {
                        v.hue = 180;
                    }
                }
                _ => {}
            }
        }
        views
    }

    fn view(&self, g: Group<'_>) -> PetView {
        let live: Vec<&Session> = g.sessions.iter().copied().filter(|s| s.state != State::Offline).collect();
        let top = live.iter().max_by_key(|s| (s.state.priority(), s.state_since)).copied();
        let camp = g.camp.clone().or_else(|| {
            let mut sources: Vec<&str> = live.iter().map(|s| s.source.as_str()).collect();
            sources.sort();
            sources.dedup();
            (sources.len() == 1).then(|| sources[0].to_string())
        });
        let state = top.map(|s| s.state).unwrap_or(State::Sleeping);
        let multi = live.len() > 1;
        let bubble = top.and_then(|s| {
            let key = match s.state {
                State::Attention => "attention",
                State::Done => "done",
                State::Error => "error",
                State::Stone => "stone",
                _ => return None,
            };
            Some(Bubble { key: key.into(), project: multi.then(|| s.project_name().to_string()) })
        });

        let card = |s: &Session| SessionCard {
            key: format!("{}/{}", s.source, s.session),
            source: s.source.clone(),
            session: s.session.clone(),
            project: s.project_name().to_string(),
            pid: s.pid,
            state: s.state,
            since: s.state_since,
            started_at: s.started_at,
            updated_at: s.last_event,
            tool: s.last_tool.clone(),
            file: s.last_file.clone(),
        };
        let mut sessions: Vec<SessionCard> = live.iter().map(|s| card(s)).collect();
        sessions.sort_by_key(|c| (c.state != State::Attention, c.started_at));

        // Minis: only when the pet stands for more than one session.
        let (minis, minis_more) = if g.minis && live.len() > 1 {
            let max = self.cfg.max_minis as usize;
            let all: Vec<MiniView> = sessions
                .iter()
                .map(|c| MiniView {
                    key: c.key.clone(),
                    source: c.source.clone(),
                    project: c.project.clone(),
                    state: c.state,
                    since: c.since,
                })
                .collect();
            let more = all.len().saturating_sub(max);
            (all.into_iter().take(max).collect(), more)
        } else {
            (vec![], 0)
        };

        PetView {
            id: g.id,
            pack: self.pack_or_fallback(&g.pack),
            pack_rev: self.lib_rev,
            state,
            since: top.map(|s| s.state_since).unwrap_or(0),
            camp,
            target: g.target,
            label: g.label,
            hue: 0,
            marker: None,
            attention_kind: top.and_then(|s| s.attention_kind.clone()),
            bubble,
            online: live.len(),
            waiting: live.iter().filter(|s| s.state == State::Attention).count(),
            sessions,
            minis,
            minis_more,
            activity: None,
            size: self.cfg.features.size_scale(),
            hat: None,
            wear: Default::default(),
            stage: 0,
        }
    }

    /// Views that changed since the last call.
    pub fn changed(&mut self) -> Vec<PetView> {
        let mut out = Vec::new();
        for v in self.pets() {
            if self.last_sent.get(&v.id) != Some(&v) {
                self.last_sent.insert(v.id.clone(), v.clone());
                out.push(v);
            }
        }
        out
    }

    /// The pet ids, when they differ from the last call.
    pub fn layout_changed(&mut self) -> Option<Vec<String>> {
        let ids: Vec<String> = self.pets().into_iter().map(|v| v.id).collect();
        // Order only matters for where a new window first appears.
        let (mut a, mut b) = (ids.clone(), self.last_layout.clone());
        a.sort();
        b.sort();
        if a == b {
            return None;
        }
        self.last_sent.retain(|k, _| ids.contains(k));
        self.last_layout = ids.clone();
        Some(ids)
    }

    /// Switch target of the pet showing the most recent event ("current camp").
    pub fn current_target(&self) -> String {
        let Some(key) = &self.last_key else { return "claude-code".into() };
        let skey = format!("{}/{}", key.0, key.1);
        self.pets()
            .into_iter()
            .find(|v| v.sessions.iter().any(|c| c.key == skey))
            .map(|v| v.target)
            .unwrap_or_else(|| key.0.clone())
    }

    /// Project names seen in this run, for the gallery's "use for project" picker.
    pub fn projects_seen(&self) -> Vec<String> {
        let mut v: Vec<String> =
            self.sessions.values().map(|s| s.project_name().to_string()).filter(|p| !p.is_empty()).collect();
        v.sort();
        v.dedup();
        v
    }

    /// Plain-text summary for `/pet stats`.
    pub fn stats_text(&self) -> String {
        let mut live: Vec<&Session> = self.sessions.values().filter(|s| s.state != State::Offline).collect();
        if live.is_empty() {
            return crate::tr!("当前没有在线会话。", "No sessions online.");
        }
        live.sort_by_key(|s| s.started_at);
        let now = crate::now_ms();
        let mut out = crate::tr!(
            "在线会话 {} 个（路由模式 {}）：\n",
            "{} sessions online (routing mode {}):\n",
            live.len(),
            self.cfg.mode.as_str()
        );
        for s in live {
            let mins = now.saturating_sub(s.started_at) / 60_000;
            out.push_str(&crate::tr!(
                "- [{}] {} · {} · 已运行 {} 分钟 · 完成 {} 轮（一次通过 {}）\n",
                "- [{}] {} · {} · up {} min · {} turns done ({} clean)\n",
                s.source,
                s.project_name(),
                s.state.label(),
                mins,
                s.turns,
                s.turns_clean
            ));
        }
        out
    }
}

#[cfg(test)]
mod nudge_tests {
    use super::*;
    use crate::config::Config;
    use crate::pack::Library;
    use crate::protocol::{Detail, Event, EventKind};

    fn hub_with(sessions: &[(&str, &str)]) -> (Hub, u64) {
        let mut h = Hub::new(Config::default(), Library::default());
        let t0 = 1_793_600_000_000;
        for (source, id) in sessions {
            let e = Event { v: 1, source: (*source).into(), session: (*id).into(), project: "/w/p".into(), event: EventKind::ToolStart, sub: false, detail: Detail::default(), pid: 0, ts: t0 };
            h.apply(&e, t0);
        }
        (h, t0)
    }

    #[test]
    fn a_whip_reaches_every_session_of_the_pet_once() {
        let (mut h, t0) = hub_with(&[("claude-code", "s1"), ("claude-code", "s2"), ("codex", "c1")]);
        let pet = h.pets().into_iter().find(|v| v.camp.as_deref() == Some("claude-code")).unwrap().id;
        h.nudge(&pet, "user", t0);
        h.nudge(&pet, "user", t0 + 1000);
        let note = h.take_nudge("claude-code", "s1", t0 + 2000).expect("s1 hears about it");
        assert!(note.starts_with("[Familiar]"));
        assert!(note.contains("2") || note.contains("第"), "the second lash is counted: {note}");
        assert!(h.take_nudge("claude-code", "s1", t0 + 3000).is_none(), "said once");
        assert!(h.take_nudge("claude-code", "s2", t0 + 2000).is_some());
        assert!(h.take_nudge("codex", "c1", t0 + 2000).is_none(), "the other camp was not whipped");
    }

    #[test]
    fn an_old_lash_is_forgotten() {
        let (mut h, t0) = hub_with(&[("claude-code", "s1")]);
        let pet = h.pets()[0].id.clone();
        h.nudge(&pet, "slow", t0);
        assert!(h.take_nudge("claude-code", "s1", t0 + NUDGE_TTL + 1).is_none());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Detail, EventKind};

    fn ev(source: &str, session: &str, project: &str, kind: EventKind, ts: u64) -> Event {
        Event {
            v: 1,
            source: source.into(),
            session: session.into(),
            project: project.into(),
            event: kind,
            sub: false,
            detail: Detail::default(),
            pid: 0,
            ts,
        }
    }

    fn hub(mode: Mode) -> Hub {
        let cfg = Config { mode, ..Config::default() };
        Hub::new(cfg, Library::default())
    }

    /// The acceptance setup: 2 Claude Code sessions + 1 Codex session, one waiting.
    fn three(h: &mut Hub) {
        h.apply(&ev("claude-code", "a", "/w/alpha", EventKind::TurnStart, 1), 1);
        h.apply(&ev("claude-code", "b", "/w/f1-server", EventKind::Attention, 2), 2);
        h.apply(&ev("codex", "c", "/w/gamma", EventKind::ToolStart, 3), 3);
    }

    #[test]
    fn single_aggregates_everything() {
        let mut h = hub(Mode::Single);
        three(&mut h);
        let v = h.pets();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].id, "main");
        assert_eq!(v[0].state, State::Attention);
        assert_eq!((v[0].online, v[0].waiting), (3, 1));
        assert_eq!(v[0].bubble.as_ref().unwrap().project.as_deref(), Some("f1-server"));
        assert!(v[0].minis.is_empty(), "single mode has no minis");
        assert_eq!(v[0].camp, None, "mixed sources: no camp");
    }

    #[test]
    fn per_camp_has_minis_with_waiting_first() {
        let mut h = hub(Mode::PerCamp);
        three(&mut h);
        let v = h.pets();
        assert_eq!(v.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), ["claude-code", "codex"]);
        let cc = &v[0];
        assert_eq!(cc.state, State::Attention);
        assert_eq!(cc.minis.len(), 2);
        assert_eq!(cc.minis[0].project, "f1-server", "waiting mini sits next to the main pet");
        assert_eq!(cc.minis[0].state, State::Attention);
        let cx = &v[1];
        assert_eq!(cx.state, State::Tool);
        assert!(cx.minis.is_empty(), "one session: no minis");
        assert_eq!(cx.pack, "pixel-bot");
        assert_eq!(cx.hue, 0);
    }

    #[test]
    fn per_camp_caps_minis() {
        let mut h = hub(Mode::PerCamp);
        for i in 0..6 {
            h.apply(&ev("claude-code", &format!("s{i}"), &format!("/w/p{i}"), EventKind::TurnStart, i + 1), i + 1);
        }
        let v = &h.pets()[0];
        assert_eq!(v.minis.len(), 4);
        assert_eq!(v.minis_more, 2);
    }

    #[test]
    fn per_camp_project_override_gets_its_own_pet() {
        let mut h = hub(Mode::PerCamp);
        h.cfg.selection.projects.insert("f1-*".into(), "pixel-dog".into());
        three(&mut h);
        let v = h.pets();
        assert_eq!(v.len(), 3);
        let p = v.iter().find(|p| p.target == "project:f1-*").unwrap();
        assert_eq!(p.pack, "pixel-dog");
        assert_eq!(p.camp.as_deref(), Some("claude-code"), "still in its camp");
        assert_eq!(p.label.as_deref(), Some("f1-server"));
        assert!(p.id.starts_with("p-"));
    }

    #[test]
    fn per_rule_routes_and_falls_back() {
        let mut h = hub(Mode::PerRule);
        h.cfg.pets = serde_yaml::from_str(
            r#"
- { id: work, pack: pixel-cat, match: { project: "f1-*" } }
- { id: bots, pack: pixel-bot, match: { source: codex } }
"#,
        )
        .unwrap();
        three(&mut h);
        let v = h.pets();
        let ids: Vec<&str> = v.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, ["r-work", "r-bots", "fallback"]);
        assert_eq!(v[0].state, State::Attention);
        assert_eq!(v[1].state, State::Tool);
        assert_eq!(v[2].sessions[0].project, "alpha");
        assert_eq!(v[0].target, "rule:work");
    }

    #[test]
    fn per_session_one_pet_each_up_to_max() {
        let mut h = hub(Mode::PerSession);
        three(&mut h);
        let v = h.pets();
        assert_eq!(v.len(), 3);
        assert!(v.iter().all(|p| p.online == 1));
        assert_eq!(v.iter().filter(|p| p.state == State::Attention).count(), 1);
        h.cfg.max_pets = 2;
        let v = h.pets();
        assert_eq!(v.len(), 2);
        assert_eq!(v[1].id, "s-more");
        assert_eq!(v[1].online, 2);
    }

    #[test]
    fn same_pack_in_both_camps_gets_hue_and_markers() {
        let mut h = hub(Mode::PerCamp);
        h.cfg.selection.codex = Some("pixel-cat".into());
        three(&mut h);
        let v = h.pets();
        assert_eq!((v[0].marker.as_deref(), v[0].hue), (Some("Claude"), 0));
        assert_eq!((v[1].marker.as_deref(), v[1].hue), (Some("Codex"), 180));
    }

    #[test]
    fn no_sessions_means_one_sleeping_pet() {
        for m in [Mode::PerCamp, Mode::Single, Mode::PerSession] {
            let h = hub(m);
            let v = h.pets();
            assert_eq!(v.len(), 1);
            assert_eq!(v[0].state, State::Sleeping);
        }
    }

    #[test]
    fn layout_and_diffs() {
        let mut h = hub(Mode::PerCamp);
        assert_eq!(h.layout_changed(), Some(vec!["claude-code".to_string()]));
        assert_eq!(h.layout_changed(), None);
        assert_eq!(h.changed().len(), 1);
        assert_eq!(h.changed().len(), 0);
        // Codex joins; the sleeping Claude Code pet stays.
        h.apply(&ev("codex", "c", "/w/c", EventKind::TurnStart, 1), 1);
        assert_eq!(h.layout_changed().unwrap(), vec!["claude-code".to_string(), "codex".to_string()]);
        assert_eq!(h.pets()[0].state, State::Sleeping);
        h.apply(&ev("claude-code", "a", "/w/a", EventKind::TurnStart, 2), 2);
        assert_eq!(h.layout_changed(), None);
        h.hidden_pets.insert("codex".into());
        assert_eq!(h.layout_changed().unwrap(), vec!["claude-code".to_string()]);
    }

    #[test]
    fn reap_marks_dead_processes_offline_and_minis_go() {
        let mut h = hub(Mode::PerCamp);
        let mut a = ev("claude-code", "a", "/w/a", EventKind::TurnStart, 1);
        a.pid = 1;
        let mut b = ev("claude-code", "b", "/w/b", EventKind::Attention, 2);
        b.pid = 2;
        h.apply(&a, 1);
        h.apply(&b, 2);
        assert_eq!(h.pets()[0].minis.len(), 2);
        assert_eq!(h.reap(5, |pid| pid != 2), 1);
        let v = &h.pets()[0];
        assert_eq!(v.online, 1);
        assert!(v.minis.is_empty());
        assert_eq!(v.state, State::Thinking, "attention gone with the dead session");
    }

    #[test]
    fn current_target_follows_last_event() {
        let mut h = hub(Mode::PerCamp);
        three(&mut h);
        assert_eq!(h.current_target(), "codex");
        h.apply(&ev("claude-code", "a", "/w/alpha", EventKind::ToolStart, 4), 4);
        assert_eq!(h.current_target(), "claude-code");
    }
}
