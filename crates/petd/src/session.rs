//! Per-session state machine (design §4). Pure: time is always passed in.

use crate::protocol::{is_read_tool, Event, EventKind, TurnResult};
use serde::{Deserialize, Serialize};

pub const SEC: u64 = 1000;
pub const MIN: u64 = 60 * SEC;
/// done / error / aborted fall back to idle after this.
pub const SETTLE_AFTER: u64 = 60 * SEC;
/// idle or done this long -> sleeping.
pub const SLEEP_AFTER: u64 = 10 * MIN;
/// No events this long (and not waiting on the user) -> offline.
pub const OFFLINE_AFTER: u64 = 30 * MIN;
/// Safety net: attention with no events this long is downgraded to idle.
pub const ATTENTION_STALE_AFTER: u64 = 120 * MIN;
/// Stone without a boss fight (pre-M4) clears after this.
pub const STONE_CLEARS_AFTER: u64 = 10 * MIN;
/// Offline sessions are kept this long, then dropped from memory.
pub const FORGET_AFTER: u64 = 24 * 60 * MIN;
/// Consecutive tool failures in one turn that turn the pet to stone.
pub const STONE_FAILS: u32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
    Idle,
    Thinking,
    Tool,
    Reading,
    Attention,
    Done,
    Error,
    Stone,
    Aborted,
    Sleeping,
    Offline,
}

impl State {
    /// Shown to people (status lists).
    pub fn label(self) -> &'static str {
        use crate::i18n::t;
        match self {
            State::Idle => t("空闲", "idle"),
            State::Thinking => t("思考中", "thinking"),
            State::Tool => t("干活中", "working"),
            State::Reading => t("读代码", "reading"),
            State::Attention => t("等你", "waiting for you"),
            State::Done => t("完成", "done"),
            State::Error => t("出错", "error"),
            State::Stone => t("石化", "petrified"),
            State::Aborted => t("被打断", "interrupted"),
            State::Sleeping => t("睡着了", "asleep"),
            State::Offline => t("离线", "offline"),
        }
    }

    /// attention > stone > error > tool = reading > thinking > aborted > done > idle > sleeping > offline
    pub fn priority(self) -> u8 {
        match self {
            State::Attention => 10,
            State::Stone => 9,
            State::Error => 8,
            State::Tool | State::Reading => 7,
            State::Thinking => 6,
            State::Aborted => 5,
            State::Done => 4,
            State::Idle => 3,
            State::Sleeping => 2,
            State::Offline => 1,
        }
    }

    pub fn is_busy(self) -> bool {
        matches!(self, State::Thinking | State::Tool | State::Reading)
    }

    pub fn is_calm(self) -> bool {
        matches!(self, State::Idle | State::Done)
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Turn {
    pub started_at: u64,
    pub tool_calls: u32,
    pub tool_errors: u32,
    pub consecutive_fails: u32,
    pub attentions: u32,
    pub stoned: bool,
}

/// A finished turn, handed to storage / gameplay.
#[derive(Debug, Clone, Serialize)]
pub struct FinishedTurn {
    pub started_at: u64,
    pub ended_at: u64,
    pub result: TurnResult,
    pub tool_calls: u32,
    pub tool_errors: u32,
    pub attentions: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct Session {
    pub source: String,
    pub session: String,
    pub project: String,
    pub pid: u32,
    pub state: State,
    pub state_since: u64,
    /// When the session last became idle/done; drives the sleep timer.
    pub calm_since: u64,
    pub last_event: u64,
    pub started_at: u64,
    pub attention_kind: Option<String>,
    pub last_tool: Option<String>,
    pub last_file: Option<String>,
    pub turn: Option<Turn>,
    pub turns: u32,
    pub turns_clean: u32,
    pub turns_error: u32,
    /// Set when the attention safety net fired; reported in the daily report.
    pub stale_attentions: u32,
}

impl Session {
    pub fn new(ev: &Event, now: u64) -> Self {
        let ts = ev.ts.min(now);
        Session {
            source: ev.source.clone(),
            session: ev.session.clone(),
            project: ev.project.clone(),
            pid: ev.pid,
            state: State::Idle,
            state_since: ts,
            calm_since: ts,
            last_event: ts,
            started_at: ts,
            attention_kind: None,
            last_tool: None,
            last_file: None,
            turn: None,
            turns: 0,
            turns_clean: 0,
            turns_error: 0,
            stale_attentions: 0,
        }
    }

    /// Short project name for bubbles and cards.
    pub fn project_name(&self) -> &str {
        self.project
            .rsplit(['/', '\\'])
            .find(|s| !s.is_empty())
            .unwrap_or("")
    }

    fn set(&mut self, s: State, ts: u64) {
        if s == self.state {
            return;
        }
        if s.is_calm() && !self.state.is_calm() {
            self.calm_since = ts;
        }
        if s != State::Attention {
            self.attention_kind = None;
        }
        self.state = s;
        self.state_since = ts;
    }

    fn finish_turn(&mut self, result: TurnResult, ts: u64) -> Option<FinishedTurn> {
        let t = self.turn.take()?;
        self.turns += 1;
        match result {
            TurnResult::Ok if t.tool_errors == 0 => self.turns_clean += 1,
            TurnResult::Error => self.turns_error += 1,
            _ => {}
        }
        Some(FinishedTurn {
            started_at: t.started_at,
            ended_at: ts,
            result,
            tool_calls: t.tool_calls,
            tool_errors: t.tool_errors,
            attentions: t.attentions,
        })
    }

    /// Apply one event. Returns a finished turn when one closes.
    pub fn apply(&mut self, ev: &Event, now: u64) -> Option<FinishedTurn> {
        let ts = ev.ts.min(now).max(self.last_event);
        self.last_event = ts;
        if ev.pid != 0 {
            self.pid = ev.pid;
        }
        if !ev.project.is_empty() {
            self.project = ev.project.clone();
        }

        // Waiting on the user always wins, even when a subagent asks.
        if ev.event == EventKind::Attention {
            if let Some(t) = self.turn.as_mut() {
                t.attentions += 1;
            }
            self.set(State::Attention, ts);
            self.attention_kind = Some(
                ev.detail
                    .kind
                    .clone()
                    .unwrap_or_else(|| "permission".into()),
            );
            return None;
        }

        // Subagent events only count as progress on the parent.
        if ev.sub {
            if self.state == State::Attention {
                // The user answered and the subagent carried on.
                self.set(State::Tool, ts);
            }
            if ev.event == EventKind::ToolStart {
                if let Some(t) = self.turn.as_mut() {
                    t.tool_calls += 1;
                }
            }
            return None;
        }

        match ev.event {
            EventKind::SessionStart => {
                if self.state == State::Offline || self.turn.is_none() {
                    self.set(State::Idle, ts);
                }
                None
            }
            EventKind::SessionEnd => {
                let fin = self.finish_turn(TurnResult::Aborted, ts);
                self.set(State::Offline, ts);
                fin
            }
            EventKind::TurnStart => {
                // A new prompt while a turn was still open means the user cut it short.
                let fin = self.finish_turn(TurnResult::Aborted, ts);
                self.turn = Some(Turn {
                    started_at: ts,
                    ..Turn::default()
                });
                self.set(State::Thinking, ts);
                fin
            }
            EventKind::ToolStart => {
                let turn = self.turn.get_or_insert_with(|| Turn {
                    started_at: ts,
                    ..Turn::default()
                });
                turn.tool_calls += 1;
                let stoned = turn.stoned;
                self.last_tool = ev.detail.tool.clone();
                self.last_file = ev.detail.file.clone();
                let reading = ev.detail.read == Some(true)
                    || ev.detail.tool.as_deref().is_some_and(is_read_tool);
                self.set(
                    if stoned {
                        State::Stone
                    } else if reading {
                        State::Reading
                    } else {
                        State::Tool
                    },
                    ts,
                );
                None
            }
            EventKind::ToolEnd => {
                if ev.detail.interrupt == Some(true) {
                    let fin = self.finish_turn(TurnResult::Aborted, ts);
                    self.set(State::Aborted, ts);
                    return fin;
                }
                let turn = self.turn.get_or_insert_with(|| Turn {
                    started_at: ts,
                    ..Turn::default()
                });
                if ev.detail.ok == Some(false) {
                    turn.tool_errors += 1;
                    turn.consecutive_fails += 1;
                    if turn.consecutive_fails >= STONE_FAILS {
                        turn.stoned = true;
                    }
                    let s = if turn.stoned {
                        State::Stone
                    } else {
                        State::Error
                    };
                    self.set(s, ts);
                } else {
                    turn.consecutive_fails = 0;
                    let s = if turn.stoned {
                        State::Stone
                    } else {
                        State::Thinking
                    };
                    self.set(s, ts);
                }
                None
            }
            EventKind::TurnDone => {
                let result = ev.detail.result.unwrap_or(TurnResult::Ok);
                let fin = self.finish_turn(result, ts);
                self.set(
                    match result {
                        TurnResult::Ok => State::Done,
                        TurnResult::Error => State::Error,
                        TurnResult::Aborted => State::Aborted,
                    },
                    ts,
                );
                fin
            }
            EventKind::Attention => unreachable!(),
        }
    }

    /// Time-driven transitions. Returns true when the state changed.
    pub fn tick(&mut self, now: u64) -> bool {
        let before = self.state;
        let in_state = now.saturating_sub(self.state_since);
        let quiet = now.saturating_sub(self.last_event);
        match self.state {
            State::Offline => {}
            State::Attention => {
                if quiet >= ATTENTION_STALE_AFTER {
                    self.stale_attentions += 1;
                    self.set(State::Idle, now);
                }
            }
            _ if quiet >= OFFLINE_AFTER => self.set(State::Offline, now),
            State::Done | State::Error | State::Aborted if in_state >= SETTLE_AFTER => {
                self.set(State::Idle, self.state_since + SETTLE_AFTER)
            }
            State::Stone if quiet >= STONE_CLEARS_AFTER => self.set(State::Idle, now),
            _ => {}
        }
        if self.state.is_calm() && now.saturating_sub(self.calm_since) >= SLEEP_AFTER {
            self.set(State::Sleeping, now);
        }
        self.state != before
    }

    /// The Boss bug was squashed: lift the stone for the rest of the turn.
    pub fn clear_stone(&mut self, now: u64) -> bool {
        let mut changed = false;
        if let Some(t) = self.turn.as_mut() {
            changed = t.stoned;
            t.stoned = false;
            t.consecutive_fails = 0;
        }
        if self.state == State::Stone {
            self.set(State::Thinking, now);
            changed = true;
        }
        changed
    }

    pub fn mark_dead(&mut self, now: u64) -> Option<FinishedTurn> {
        let fin = self.finish_turn(TurnResult::Aborted, now);
        self.set(State::Offline, now);
        fin
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::Detail;

    fn ev(kind: EventKind, ts: u64) -> Event {
        Event {
            v: 1,
            source: "claude-code".into(),
            session: "s1".into(),
            project: "/work/f1-server".into(),
            event: kind,
            sub: false,
            detail: Detail::default(),
            pid: 0,
            ts,
        }
    }
    fn tool_start(name: &str, ts: u64) -> Event {
        let mut e = ev(EventKind::ToolStart, ts);
        e.detail.tool = Some(name.into());
        e
    }
    fn tool_end(ok: bool, ts: u64) -> Event {
        let mut e = ev(EventKind::ToolEnd, ts);
        e.detail.ok = Some(ok);
        e
    }
    fn done(r: TurnResult, ts: u64) -> Event {
        let mut e = ev(EventKind::TurnDone, ts);
        e.detail.result = Some(r);
        e
    }

    #[test]
    fn normal_turn() {
        let mut s = Session::new(&ev(EventKind::SessionStart, 0), 0);
        s.apply(&ev(EventKind::TurnStart, 10), 10);
        assert_eq!(s.state, State::Thinking);
        s.apply(&tool_start("Grep", 20), 20);
        assert_eq!(s.state, State::Reading);
        s.apply(&tool_end(true, 30), 30);
        assert_eq!(s.state, State::Thinking);
        s.apply(&tool_start("Edit", 40), 40);
        assert_eq!(s.state, State::Tool);
        s.apply(&tool_end(true, 50), 50);
        let fin = s.apply(&done(TurnResult::Ok, 60), 60).unwrap();
        assert_eq!(s.state, State::Done);
        assert_eq!(fin.tool_calls, 2);
        assert_eq!(s.turns_clean, 1);
    }

    #[test]
    fn three_fails_turn_to_stone_until_next_turn() {
        let mut s = Session::new(&ev(EventKind::SessionStart, 0), 0);
        s.apply(&ev(EventKind::TurnStart, 1), 1);
        for i in 0..2 {
            s.apply(&tool_start("Bash", 10 + i * 10), 10 + i * 10);
            s.apply(&tool_end(false, 15 + i * 10), 15 + i * 10);
            assert_eq!(s.state, State::Error);
        }
        s.apply(&tool_start("Bash", 40), 40);
        s.apply(&tool_end(false, 45), 45);
        assert_eq!(s.state, State::Stone);
        s.apply(&tool_start("Bash", 50), 50);
        assert_eq!(
            s.state,
            State::Stone,
            "stays stone for the rest of the turn"
        );
        s.apply(&ev(EventKind::TurnStart, 60), 60);
        assert_eq!(s.state, State::Thinking);
    }

    #[test]
    fn success_resets_consecutive_fail_count() {
        let mut s = Session::new(&ev(EventKind::SessionStart, 0), 0);
        s.apply(&ev(EventKind::TurnStart, 1), 1);
        for (i, ok) in [false, false, true, false, false].iter().enumerate() {
            s.apply(&tool_end(*ok, 10 + i as u64), 10 + i as u64);
        }
        assert_eq!(s.state, State::Error);
    }

    #[test]
    fn attention_is_released_by_next_event_and_counted() {
        let mut s = Session::new(&ev(EventKind::SessionStart, 0), 0);
        s.apply(&ev(EventKind::TurnStart, 1), 1);
        let mut a = ev(EventKind::Attention, 2);
        a.detail.kind = Some("permission".into());
        s.apply(&a, 2);
        assert_eq!(s.state, State::Attention);
        assert_eq!(s.attention_kind.as_deref(), Some("permission"));
        s.apply(&tool_end(true, 3), 3);
        assert_eq!(s.state, State::Thinking);
        assert_eq!(s.attention_kind, None);
        assert_eq!(s.turn.as_ref().unwrap().attentions, 1);
    }

    #[test]
    fn subagent_events_do_not_change_face_but_attention_does() {
        let mut s = Session::new(&ev(EventKind::SessionStart, 0), 0);
        s.apply(&ev(EventKind::TurnStart, 1), 1);
        s.apply(&tool_start("Task", 2), 2);
        let mut sub_fail = tool_end(false, 3);
        sub_fail.sub = true;
        s.apply(&sub_fail, 3);
        assert_eq!(s.state, State::Tool);
        let mut sub_attn = ev(EventKind::Attention, 4);
        sub_attn.sub = true;
        s.apply(&sub_attn, 4);
        assert_eq!(s.state, State::Attention);
    }

    #[test]
    fn new_prompt_mid_turn_records_abort() {
        let mut s = Session::new(&ev(EventKind::SessionStart, 0), 0);
        s.apply(&ev(EventKind::TurnStart, 1), 1);
        let fin = s.apply(&ev(EventKind::TurnStart, 5), 5).unwrap();
        assert_eq!(fin.result, TurnResult::Aborted);
        assert_eq!(s.state, State::Thinking);
    }

    #[test]
    fn interrupt_aborts() {
        let mut s = Session::new(&ev(EventKind::SessionStart, 0), 0);
        s.apply(&ev(EventKind::TurnStart, 1), 1);
        let mut e = tool_end(false, 2);
        e.detail.interrupt = Some(true);
        assert_eq!(s.apply(&e, 2).unwrap().result, TurnResult::Aborted);
        assert_eq!(s.state, State::Aborted);
    }

    #[test]
    fn timers() {
        let mut s = Session::new(&ev(EventKind::SessionStart, 0), 0);
        s.apply(&ev(EventKind::TurnStart, 0), 0);
        s.apply(&done(TurnResult::Ok, 1000), 1000);
        assert!(!s.tick(1000 + SETTLE_AFTER - 1));
        assert!(s.tick(1000 + SETTLE_AFTER));
        assert_eq!(s.state, State::Idle);
        assert!(s.tick(1000 + SLEEP_AFTER));
        assert_eq!(s.state, State::Sleeping);
        // Any event wakes it.
        s.apply(
            &ev(EventKind::TurnStart, 1000 + SLEEP_AFTER + 5),
            1000 + SLEEP_AFTER + 5,
        );
        assert_eq!(s.state, State::Thinking);
        // Silence goes offline, but never while waiting on the user.
        assert!(s.tick(1000 + SLEEP_AFTER + 5 + OFFLINE_AFTER));
        assert_eq!(s.state, State::Offline);
    }

    #[test]
    fn attention_survives_silence_until_safety_net() {
        let mut s = Session::new(&ev(EventKind::SessionStart, 0), 0);
        s.apply(&ev(EventKind::Attention, 0), 0);
        assert!(!s.tick(OFFLINE_AFTER * 2));
        assert_eq!(s.state, State::Attention);
        assert!(s.tick(ATTENTION_STALE_AFTER));
        assert_eq!(s.state, State::Idle);
        assert_eq!(s.stale_attentions, 1);
    }

    #[test]
    fn spooled_events_use_their_own_timestamps() {
        // petd was down; a done from 20 minutes ago replays now.
        let now = 30 * MIN;
        let mut s = Session::new(&ev(EventKind::SessionStart, 0), now);
        s.apply(&ev(EventKind::TurnStart, MIN), now);
        s.apply(&done(TurnResult::Ok, 10 * MIN), now);
        s.tick(now);
        assert_eq!(s.state, State::Sleeping);
    }
}
