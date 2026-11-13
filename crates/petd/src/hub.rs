//! Sessions -> pets. Owns all live state; the server and timers lock it briefly.
//!
//! One pet per agent (`source`): it stands for every session of that agent and
//! shows the most pressing state among them.

use crate::config::Config;
use crate::protocol::Event;
use crate::session::{FinishedTurn, Session, State, FORGET_AFTER};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

pub type Key = (String, String);

/// A permission request waiting for the user's answer on the pet's card.
pub struct Ask {
    pub id: u64,
    pub key: Key,
    pub tool: String,
    /// The command, file or address being asked about.
    pub detail: String,
    /// The agent's own one-line description, when it gave one.
    pub note: String,
    /// When the wait ends (ms) and how long it is in all.
    pub until: u64,
    pub wait: u64,
    /// Wakes the hook that is holding the request.
    tx: Option<tokio::sync::oneshot::Sender<&'static str>>,
}

/// What happened, for the world layer (drained every tick).
#[derive(Debug, Clone)]
pub enum Feed {
    Event {
        key: Key,
        ev: Event,
    },
    Finished {
        key: Key,
        project: String,
        fin: FinishedTurn,
    },
    State {
        key: Key,
        from: State,
        to: State,
    },
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
    /// Which line the pet says: attention, done, error, stone.
    pub key: String,
    /// Project prefix, only when the pet shows more than one session.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PetView {
    pub id: String,
    pub state: State,
    /// When the shown state began (ms). Lets the window time one-shot bubbles.
    pub since: u64,
    /// The agent this pet stands for (`claude-code`, `codex`, …).
    pub camp: Option<String>,
    /// Agent tag under the pet ("Claude" / "Codex").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub marker: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attention_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bubble: Option<Bubble>,
    pub online: usize,
    pub waiting: usize,
    pub sessions: Vec<SessionCard>,
    /// What the world layer has the pet doing, e.g. "fishing".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activity: Option<String>,
    /// Display multiplier from `features.size` (S 0.75, M 1, L 1.35).
    pub size: f32,
    /// Which of the five pets this is (`zimeng`, `mashu`, …): its body, its skin, its voice.
    pub skin: String,
}

pub struct Hub {
    pub cfg: Config,
    /// Last config.yaml parse error.
    pub cfg_error: Option<String>,
    pub sessions: HashMap<Key, Session>,
    /// Closed turns waiting for storage.
    pub finished: Vec<(Key, FinishedTurn)>,
    last_sent: HashMap<String, PetView>,
    last_layout: Vec<String>,
    /// Everything hidden (/pet hide, tray).
    pub hidden: bool,
    /// Single pets hidden from their right-click menu.
    pub hidden_pets: HashSet<String>,
    /// Session that sent the most recent event.
    pub last_key: Option<Key>,
    pub feed: Vec<Feed>,
    /// pet id -> activity ("fishing"), owned by the world layer.
    pub activities: HashMap<String, String>,
    /// Permission requests held for the approval card, oldest first.
    pub asks: Vec<Ask>,
    next_ask: u64,
}

/// One pet before it is turned into a view.
struct Group<'a> {
    id: String,
    camp: String,
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
    if !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        s.to_string()
    } else {
        format!("x-{}", short_hash(s))
    }
}

impl Hub {
    pub fn new(cfg: Config) -> Self {
        Hub {
            cfg,
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
            asks: vec![],
            next_ask: 1,
        }
    }

    /// Returns true when the change is urgent (someone is now waiting on the user).
    pub fn apply(&mut self, ev: &Event, now: u64) -> bool {
        if ev.session.is_empty() || ev.source.is_empty() {
            return false;
        }
        let key = (ev.source.clone(), ev.session.clone());
        let s = self
            .sessions
            .entry(key.clone())
            .or_insert_with(|| Session::new(ev, now));
        let was = s.state;
        let fin = s.apply(ev, now);
        let (to, project) = (s.state, s.project.clone());
        // A turn the event closed (e.g. a new prompt cutting the old turn
        // short) is reported before the event itself: the old turn ends
        // first, then the new one starts.
        if let Some(fin) = fin {
            self.push_feed(Feed::Finished {
                key: key.clone(),
                project,
                fin: fin.clone(),
            });
            self.finished.push((key.clone(), fin));
            if self.finished.len() > 1000 {
                self.finished.drain(..500);
            }
        }
        self.push_feed(Feed::Event {
            key: key.clone(),
            ev: ev.clone(),
        });
        if to != was {
            self.push_feed(Feed::State {
                key: key.clone(),
                from: was,
                to,
            });
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
                changes.push(Feed::State {
                    key: k.clone(),
                    from: was,
                    to: s.state,
                });
            }
        }
        for c in changes {
            self.push_feed(c);
        }
        self.sessions.retain(|_, s| {
            !(s.state == State::Offline && now.saturating_sub(s.state_since) > FORGET_AFTER)
        });
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
                self.feed.push(Feed::State {
                    key: key.clone(),
                    from: was,
                    to: State::Offline,
                });
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
                self.push_feed(Feed::State {
                    key: key.clone(),
                    from: was,
                    to,
                });
            }
        }
    }

    /// Pet currently showing a session (if any).
    pub fn pet_of(&self, key: &Key) -> Option<PetView> {
        let skey = format!("{}/{}", key.0, key.1);
        self.pets()
            .into_iter()
            .find(|v| v.sessions.iter().any(|c| c.key == skey))
    }

    /// How long a permission request from this session may wait for the card
    /// (ms): only when the user switched that on, the pets are showing, and the
    /// agent is one whose hook can carry the answer back.
    pub fn approve_wait(&self, source: &str) -> Option<u64> {
        let f = &self.cfg.features;
        (f.approve && !self.hidden && source == "claude-code")
            .then(|| f.approve_wait_s.clamp(5, 120) as u64 * 1000)
    }

    /// Hold a permission request for the card. Returns its id, the channel the
    /// answer arrives on, and how long to wait.
    pub fn ask(
        &mut self,
        source: &str,
        session: &str,
        tool: &str,
        detail: &str,
        note: &str,
        now: u64,
    ) -> Option<(u64, tokio::sync::oneshot::Receiver<&'static str>, u64)> {
        let wait = self.approve_wait(source)?;
        let (tx, rx) = tokio::sync::oneshot::channel();
        let id = self.next_ask;
        self.next_ask += 1;
        let cut = |s: &str, n: usize| -> String { s.chars().take(n).collect() };
        self.asks.push(Ask {
            id,
            key: (source.to_string(), session.to_string()),
            tool: cut(tool, 60),
            detail: cut(detail, 1200),
            note: cut(note, 200),
            until: now + wait,
            wait,
            tx: Some(tx),
        });
        Some((id, rx, wait))
    }

    /// The card's view of one request (or of all that are waiting).
    pub fn ask_views(&self, only: Option<u64>) -> Vec<serde_json::Value> {
        self.asks
            .iter()
            .filter(|a| only.is_none_or(|id| a.id == id))
            .map(|a| {
                let project = self.sessions.get(&a.key).map(|s| s.project_name().to_string()).unwrap_or_default();
                serde_json::json!({
                    "id": a.id, "pet": self.pet_of(&a.key).map(|p| p.id), "source": a.key.0, "project": project,
                    "tool": a.tool, "detail": a.detail, "note": a.note, "until": a.until, "wait": a.wait,
                })
            })
            .collect()
    }

    /// The user's answer from the card: `allow`, `deny`, or `terminal` (let the terminal ask).
    pub fn answer_ask(&mut self, id: u64, behavior: &str) -> bool {
        let Some(a) = self.asks.iter_mut().find(|a| a.id == id) else {
            return false;
        };
        let b = match behavior {
            "allow" => "allow",
            "deny" => "deny",
            _ => "terminal",
        };
        a.tx.take().is_some_and(|tx| tx.send(b).is_ok())
    }

    /// The hook stopped waiting (answered, timed out, or went away).
    pub fn end_ask(&mut self, id: u64) {
        self.asks.retain(|a| a.id != id);
    }

    fn groups(&self) -> Vec<Group<'_>> {
        let mut all: Vec<&Session> = self.sessions.values().collect();
        all.sort_by(|a, b| {
            a.started_at
                .cmp(&b.started_at)
                .then_with(|| a.session.cmp(&b.session))
        });
        // The Claude Code pet always stands (asleep when idle): with no agent
        // running there is exactly one pet, and it never vanishes because petd
        // restarted or only Codex is busy. Another agent's pet is there while
        // that agent has a session open, and goes when its last one closes.
        let mut out = vec![Group {
            id: "claude-code".into(),
            camp: "claude-code".into(),
            sessions: vec![],
        }];
        for s in all {
            let id = safe_id(&s.source);
            match out.iter_mut().find(|g| g.id == id) {
                Some(g) => g.sessions.push(s),
                None => out.push(Group {
                    id,
                    camp: s.source.clone(),
                    sessions: vec![s],
                }),
            }
        }
        out.retain(|g| {
            g.id == "claude-code" || g.sessions.iter().any(|s| s.state != State::Offline)
        });
        out
    }

    /// Every visible pet with its state.
    pub fn pets(&self) -> Vec<PetView> {
        self.groups()
            .into_iter()
            .filter(|g| !self.hidden_pets.contains(&g.id))
            .map(|g| self.view(g))
            .collect()
    }

    fn view(&self, g: Group<'_>) -> PetView {
        let live: Vec<&Session> = g
            .sessions
            .iter()
            .copied()
            .filter(|s| s.state != State::Offline)
            .collect();
        let top = live
            .iter()
            .max_by_key(|s| (s.state.priority(), s.state_since))
            .copied();
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
            Some(Bubble {
                key: key.into(),
                project: multi.then(|| s.project_name().to_string()),
            })
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

        PetView {
            skin: crate::cast::of(&self.cfg, &g.camp).id.into(),
            activity: self.activities.get(&g.id).cloned(),
            marker: Some(crate::config::agent_tag(&g.camp)),
            id: g.id,
            state,
            since: top.map(|s| s.state_since).unwrap_or(0),
            camp: Some(g.camp),
            attention_kind: top.and_then(|s| s.attention_kind.clone()),
            bubble,
            online: live.len(),
            waiting: live.iter().filter(|s| s.state == State::Attention).count(),
            sessions,
            size: self.cfg.features.size_scale(),
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

    /// Plain-text summary for `/pet stats`.
    pub fn stats_text(&self) -> String {
        let mut live: Vec<&Session> = self
            .sessions
            .values()
            .filter(|s| s.state != State::Offline)
            .collect();
        if live.is_empty() {
            return crate::tr!("当前没有在线会话。", "No sessions online.");
        }
        live.sort_by_key(|s| s.started_at);
        let now = crate::now_ms();
        let mut out = crate::tr!("在线会话 {} 个：\n", "{} sessions online:\n", live.len());
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

    fn hub() -> Hub {
        Hub::new(Config::default())
    }

    /// The acceptance setup: 2 Claude Code sessions + 1 Codex session, one waiting.
    fn three(h: &mut Hub) {
        h.apply(
            &ev("claude-code", "a", "/w/alpha", EventKind::TurnStart, 1),
            1,
        );
        h.apply(
            &ev("claude-code", "b", "/w/f1-server", EventKind::Attention, 2),
            2,
        );
        h.apply(&ev("codex", "c", "/w/gamma", EventKind::ToolStart, 3), 3);
    }

    #[test]
    fn one_pet_per_agent_with_the_most_pressing_state() {
        let mut h = hub();
        three(&mut h);
        let v = h.pets();
        assert_eq!(
            v.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
            ["claude-code", "codex"]
        );
        let cc = &v[0];
        assert_eq!(cc.state, State::Attention);
        assert_eq!((cc.online, cc.waiting), (2, 1));
        assert_eq!(
            cc.sessions[0].project, "f1-server",
            "the waiting session comes first"
        );
        assert_eq!(
            cc.bubble.as_ref().unwrap().project.as_deref(),
            Some("f1-server"),
            "two sessions: the bubble says which"
        );
        assert_eq!(
            (cc.camp.as_deref(), cc.marker.as_deref()),
            (Some("claude-code"), Some("Claude"))
        );
        let cx = &v[1];
        assert_eq!(cx.state, State::Tool);
        assert_eq!((cx.online, cx.marker.as_deref()), (1, Some("Codex")));
    }

    #[test]
    fn a_third_agent_gets_its_own_pet_and_tag() {
        let mut h = hub();
        for (source, id) in [("claude-code", "s1"), ("gemini", "g1"), ("my agent", "m1")] {
            h.apply(&ev(source, id, "/w/p", EventKind::ToolStart, 1), 1);
        }
        let pets = h.pets();
        let of = |camp: &str| {
            pets.iter()
                .find(|v| v.camp.as_deref() == Some(camp))
                .unwrap_or_else(|| panic!("no pet for {camp}"))
        };
        assert_eq!(of("gemini").marker.as_deref(), Some("Gemini"));
        assert_eq!(
            of("my agent").marker.as_deref(),
            Some("my agent"),
            "an agent from a custom adapter shows its id"
        );
        assert!(
            of("my agent").id.starts_with("x-"),
            "a window label only takes plain characters"
        );
    }

    #[test]
    fn permission_requests_wait_for_the_card_only_when_asked_to() {
        let mut h = hub();
        let t0 = 1_793_600_000_000;
        h.apply(
            &ev("claude-code", "s1", "/w/p", EventKind::ToolStart, t0),
            t0,
        );
        h.apply(&ev("codex", "c1", "/w/p", EventKind::ToolStart, t0), t0);
        assert!(
            h.ask("claude-code", "s1", "Bash", "ls", "", t0).is_none(),
            "off by default"
        );
        h.cfg.features.approve = true;
        assert_eq!(h.approve_wait("claude-code"), Some(30_000));
        assert!(
            h.approve_wait("codex").is_none(),
            "Codex has no way to carry the answer back"
        );
        h.hidden = true;
        assert!(
            h.approve_wait("claude-code").is_none(),
            "nothing pops up while the pets are hidden"
        );
        h.hidden = false;

        let (id, mut rx, wait) = h
            .ask(
                "claude-code",
                "s1",
                "Bash",
                &"x".repeat(5000),
                "List files",
                t0,
            )
            .unwrap();
        assert_eq!(wait, 30_000);
        let v = h.ask_views(None);
        assert_eq!(v.len(), 1);
        assert_eq!(
            (
                v[0]["tool"].as_str(),
                v[0]["note"].as_str(),
                v[0]["project"].as_str()
            ),
            (Some("Bash"), Some("List files"), Some("p"))
        );
        assert_eq!(
            v[0]["detail"].as_str().unwrap().len(),
            1200,
            "a long command is cut"
        );
        assert_eq!(v[0]["pet"], "claude-code");
        assert!(rx.try_recv().is_err(), "still waiting");
        assert!(h.answer_ask(id, "allow"));
        assert_eq!(rx.try_recv().unwrap(), "allow");
        assert!(!h.answer_ask(id, "deny"), "answered once");
        h.end_ask(id);
        assert!(h.ask_views(None).is_empty());

        let (id, mut rx, _) = h.ask("claude-code", "s1", "Edit", "a.rs", "", t0).unwrap();
        assert!(h.answer_ask(id, "anything else"));
        assert_eq!(rx.try_recv().unwrap(), "terminal");
        assert!(!h.answer_ask(999, "allow"));
    }

    #[test]
    fn no_sessions_means_one_sleeping_pet() {
        let v = hub().pets();
        assert_eq!(v.len(), 1);
        assert_eq!(
            (v[0].id.as_str(), v[0].state),
            ("claude-code", State::Sleeping)
        );
    }

    #[test]
    fn layout_and_diffs() {
        let mut h = hub();
        assert_eq!(h.layout_changed(), Some(vec!["claude-code".to_string()]));
        assert_eq!(h.layout_changed(), None);
        assert_eq!(h.changed().len(), 1);
        assert_eq!(h.changed().len(), 0);
        // Codex joins; the sleeping Claude Code pet stays.
        h.apply(&ev("codex", "c", "/w/c", EventKind::TurnStart, 1), 1);
        assert_eq!(
            h.layout_changed().unwrap(),
            vec!["claude-code".to_string(), "codex".to_string()]
        );
        assert_eq!(h.pets()[0].state, State::Sleeping);
        h.apply(&ev("claude-code", "a", "/w/a", EventKind::TurnStart, 2), 2);
        assert_eq!(h.layout_changed(), None);
        h.hidden_pets.insert("codex".into());
        assert_eq!(h.layout_changed().unwrap(), vec!["claude-code".to_string()]);
    }

    #[test]
    fn reap_marks_dead_processes_offline() {
        let mut h = hub();
        let mut a = ev("claude-code", "a", "/w/a", EventKind::TurnStart, 1);
        a.pid = 1;
        let mut b = ev("claude-code", "b", "/w/b", EventKind::Attention, 2);
        b.pid = 2;
        h.apply(&a, 1);
        h.apply(&b, 2);
        assert_eq!(h.pets()[0].online, 2);
        assert_eq!(h.reap(5, |pid| pid != 2), 1);
        let v = &h.pets()[0];
        assert_eq!(v.online, 1);
        assert_eq!(
            v.state,
            State::Thinking,
            "attention gone with the dead session"
        );
    }
}
