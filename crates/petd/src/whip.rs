//! 小皮鞭. A crack of the whip for the slow and the beaten. Three ways to earn one:
//! the other camp has been grinding on one turn for too long (interactions.yaml
//! rule `whip`, `on: slow`), you lost a game (duo / duel / race), or the user
//! picks it from the right-click menu. The overlay draws the lash and plays the
//! sounds (`whip` message); the pets say their lines through ordinary
//! `pet.script` messages, so every window stays in sync on one timeline.

use crate::hub::PetView;
use crate::session::State;
use crate::world::{Out, Script, World};
use crate::Control;
use serde_json::json;

/// Walk time when the whipper has to come over first (same as `physics::walk`).
pub const WHIP_WALK: u64 = 1200;
/// The lash lands this long after the stroke starts: a wind-up back and over
/// the head, then the strike (whip.ts animates to the same clock).
pub const WHIP_HIT: u64 = 760;
/// How long the two of them hold the moment.
pub const WHIP_MS: u64 = 2600;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    /// Lost a game: the winner holds the whip.
    Game,
    /// Too long on one turn: the idle camp comes over.
    Slow,
    /// From the right-click menu: the user holds the whip.
    User,
}

impl Reason {
    fn id(self) -> &'static str {
        match self {
            Reason::Game => "game",
            Reason::Slow => "slow",
            Reason::User => "user",
        }
    }
}

// (zh, en). Short, specific, no cheering — the bond.yaml voice.
const GAME_MASTER: &[(&str, &str)] = &[
    ("输了。站好。", "You lost. Stand still."),
    ("这一下是规矩。", "This one's the rule."),
    ("别躲。输的是你。", "Don't flinch. You're the one who lost."),
    ("下回赢我，鞭子归你。", "Beat me next time and the whip is yours."),
    ("手别抖。", "Hold still."),
];
const GAME_OUCH: &[(&str, &str)] = &[
    ("……大人，轻点。", "…easy, master."),
    ("疼。记着了。", "That stung. Noted."),
    ("下一局。这一下我记着。", "Next round. I'll remember this one."),
    ("……再来也行。", "…again, if you like."),
    ("是我输了。", "I lost. Fair."),
];
const SLOW_OUCH: &[(&str, &str)] = &[
    ("在做了。", "I'm on it."),
    ("……知道了，大人。", "…yes, master."),
    ("鞭子抽不动编译器。", "The whip won't speed up the compiler."),
    ("好。我快一点。", "Fine. Faster."),
];
const USER_WORKING: &[(&str, &str)] = &[
    ("在干了，大人。", "Working, master."),
    ("……嗯。手上有活。", "…mm. Hands are busy."),
    ("抽不快的。但你开心就好。", "Won't make it faster. But if it pleases you."),
];
const USER_CALM: &[(&str, &str)] = &[
    ("我没活。你就是想抽。", "I've no work. You just wanted to."),
    ("……嗯。再来。", "…mm. Again."),
    ("醒了，大人。", "I'm awake, master."),
    ("轻点。不是不要。", "Gently. I didn't say stop."),
];
const USER_ERROR: &[(&str, &str)] = &[
    ("这一下该挨。", "I had that one coming."),
    ("是我错了。", "My fault."),
    ("……再来一下。我记住。", "…once more. So I remember."),
];
const USER_ATTENTION: (&str, &str) = ("等你呢。先回话。", "I'm waiting on you. Answer first.");

impl World {
    fn whip_line(&mut self, set: &[(&'static str, &'static str)]) -> &'static str {
        self.pick(set).map(|(zh, en)| crate::i18n::t(zh, en)).unwrap_or("")
    }

    /// The stroke itself, at `delay` ms from now: the overlay's lash + sounds,
    /// the whipper's line (when a pet holds the whip) and the victim's.
    /// Scripts (who is busy, who walks home) are the caller's business.
    pub(crate) fn whip(&mut self, from: Option<&str>, to: &str, to_state: State, reason: Reason, delay: u64, out: &mut Vec<Out>) {
        let (master, ouch) = match reason {
            Reason::Game => (Some(self.whip_line(GAME_MASTER)), self.whip_line(GAME_OUCH)),
            Reason::Slow => (None, self.whip_line(SLOW_OUCH)), // the rule's own bubble is the master line
            Reason::User => (
                None,
                match to_state {
                    State::Thinking | State::Tool | State::Reading => self.whip_line(USER_WORKING),
                    State::Error | State::Stone | State::Aborted => self.whip_line(USER_ERROR),
                    State::Attention => crate::i18n::t(USER_ATTENTION.0, USER_ATTENTION.1),
                    _ => self.whip_line(USER_CALM),
                },
            ),
        };
        out.push(Out::Ws(crate::msg("whip", &json!({ "from": from, "to": to, "reason": reason.id(), "delay": delay, "hit": WHIP_HIT }))));
        if let (Some(from), Some(line)) = (from, master) {
            out.push(Out::Ws(crate::msg(
                "pet.script",
                &json!({ "pet": from, "rule": "whip", "action": "throw", "bubble": line, "ms": WHIP_MS, "delay": delay }),
            )));
        }
        // A pet waiting on the user keeps its attention pose: line only.
        let action = if to_state == State::Attention { None } else { Some("dizzy") };
        out.push(Out::Ws(crate::msg(
            "pet.script",
            &json!({ "pet": to, "rule": "whip", "action": action, "bubble": ouch, "ms": WHIP_MS, "delay": delay + WHIP_HIT }),
        )));
    }

    /// After a duel or race: the winner says its piece, walks over and cracks
    /// the whip; the loser sulks, then takes it. Both are busy until it is over.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn whip_walkover(&mut self, winner: &PetView, loser: &PetView, win_text: &str, lose_text: &str, rule: &str, now: u64, out: &mut Vec<Out>) {
        let until = now + WHIP_WALK + WHIP_HIT + WHIP_MS;
        self.scripts.insert(winner.id.clone(), Script { rule: rule.into(), until, during_attention: false, moved: true });
        self.scripts.insert(loser.id.clone(), Script { rule: rule.into(), until, during_attention: false, moved: false });
        out.push(Out::Control(Control::Move { pet: winner.id.clone(), target: Some(loser.id.clone()) }));
        // Bubble only while walking: an action would cut the walk animation.
        out.push(Out::Ws(crate::msg("pet.script", &json!({ "pet": winner.id, "rule": rule, "bubble": win_text, "ms": WHIP_WALK, "delay": 0 }))));
        out.push(Out::Ws(crate::msg("pet.script", &json!({ "pet": loser.id, "rule": rule, "action": "sulk", "bubble": lose_text, "ms": WHIP_WALK + 200, "delay": 0 }))));
        self.whip(Some(&winner.id), &loser.id, loser.state, Reason::Game, WHIP_WALK, out);
    }

    /// Lost a round to the user (rock-paper-scissors): the user holds the whip,
    /// the pet takes it like any beaten player, `delay` ms after the verdict.
    pub fn lost_to_user(&mut self, pet: &str, views: &[PetView], delay: u64) -> Vec<Out> {
        let Some(v) = views.iter().find(|v| v.id == pet) else { return vec![] };
        let mut out = vec![];
        self.whip(None, pet, v.state, Reason::Game, delay, &mut out);
        out
    }

    /// Right-click → 小皮鞭. The user holds the whip; the pet answers to its state.
    pub fn user_whip(&mut self, pet: &str, views: &[PetView], now: u64, db: &crate::db::Db) -> Result<Vec<Out>, String> {
        let v = views.iter().find(|v| v.id == pet).ok_or_else(|| crate::tr!("找不到这只宠物", "Can't find that pet"))?;
        let mut out = vec![];
        db.log_interaction(now, pet, "whip", self.frozen);
        self.whip(None, pet, v.state, Reason::User, 0, &mut out);
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::balance::Tables;
    use crate::config::Config;
    use crate::db::Db;
    use crate::hub::Hub;
    use crate::pack::Library;
    use crate::protocol::{Detail, Event, EventKind};

    fn ws(out: &[Out], kind: &str) -> Vec<serde_json::Value> {
        out.iter()
            .filter_map(|o| match o {
                Out::Ws(s) => serde_json::from_str::<serde_json::Value>(s).ok().filter(|v| v["type"] == kind).map(|v| v["data"].clone()),
                _ => None,
            })
            .collect()
    }

    fn two_pets() -> Vec<PetView> {
        let mut h = Hub::new(Config::default(), Library::default());
        let t0 = 1_793_600_000_000;
        for (s, id) in [("claude-code", "a"), ("codex", "b")] {
            let e = Event { v: 1, source: s.into(), session: id.into(), project: "/w/p".into(), event: EventKind::SessionStart, sub: false, detail: Detail::default(), pid: 0, ts: t0 };
            h.apply(&e, t0);
        }
        h.pets()
    }

    #[test]
    fn user_whip_is_a_lash_and_one_line() {
        let mut w = World::new(Tables::builtin(), 3);
        let views = two_pets();
        let out = w.user_whip("codex", &views, 1_793_600_000_000, &Db::in_memory()).unwrap();
        let lash = ws(&out, "whip");
        assert_eq!(lash.len(), 1);
        assert_eq!(lash[0]["to"], "codex");
        assert!(lash[0]["from"].is_null(), "the user holds the whip");
        let lines = ws(&out, "pet.script");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0]["pet"], "codex");
        assert_eq!(lines[0]["delay"], WHIP_HIT);
        assert!(w.scripts.is_empty(), "a user whip never blocks the pet");
    }

    #[test]
    fn walkover_moves_the_winner_and_busies_both() {
        let mut w = World::new(Tables::builtin(), 3);
        let views = two_pets();
        let (a, b) = (views[0].clone(), views[1].clone());
        let mut out = vec![];
        w.whip_walkover(&a, &b, "赢", "输", "duel", 0, &mut out);
        assert!(out.iter().any(|o| matches!(o, Out::Control(Control::Move { pet, target: Some(t) }) if pet == &a.id && t == &b.id)));
        assert_eq!(ws(&out, "whip")[0]["from"], a.id);
        assert!(w.scripts[&a.id].moved && !w.scripts[&b.id].moved);
        let scripts = ws(&out, "pet.script");
        // Winner: walk bubble, then the stroke; loser: sulk, then the hit.
        assert_eq!(scripts.iter().filter(|s| s["pet"] == a.id).count(), 2);
        assert_eq!(scripts.iter().filter(|s| s["pet"] == b.id).count(), 2);
        assert!(scripts.iter().any(|s| s["pet"] == a.id && s["action"] == "throw" && s["delay"] == WHIP_WALK));
        assert!(scripts.iter().any(|s| s["pet"] == b.id && s["action"] == "dizzy" && s["delay"] == WHIP_WALK + WHIP_HIT));
    }
}
