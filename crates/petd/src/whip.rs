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

// The other pet, watching from its own window.
const WATCH_TAUNT: &[(&str, &str)] = &[
    ("该。", "Serves you right."),
    ("我看着呢。再来一下。", "I'm watching. Once more."),
    ("慢的是你，不是我。", "You're the slow one. Not me."),
    ("疼吗。我猜疼。", "Hurt? I'd guess so."),
    ("下回站我后面。", "Stand behind me next time."),
];
const WATCH_PITY: &[(&str, &str)] = &[
    ("……轻点。它在做了。", "…gently. It's working."),
    ("抽它不如抽我。", "You'd do better whipping me."),
    ("别看。马上就过去。", "Don't look. It's almost over."),
    ("它今天没偷懒。", "It didn't slack today."),
    ("等会儿我陪你坐一会儿。", "I'll sit with you after."),
];
const WATCH_NERVOUS: &[(&str, &str)] = &[
    ("……我没看。", "…I didn't see anything."),
    ("下一个别是我。", "Not me next. Please."),
    ("我在干活。真的。", "I'm working. Really."),
    ("鞭子声有点大。", "That whip is loud."),
];
/// How long the onlooker holds its face.
const WATCH_MS: u64 = 2200;

impl World {
    fn whip_line(&mut self, set: &[(&'static str, &'static str)]) -> &'static str {
        self.pick(set).map(|(zh, en)| crate::i18n::t(zh, en)).unwrap_or("")
    }

    /// The stroke itself, at `delay` ms from now: the overlay's lash + sounds,
    /// the whipper's line (when a pet holds the whip) and the victim's.
    /// Scripts (who is busy, who walks home) are the caller's business.
    /// Every lash lands in the day's report (`whip` event on the victim).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn whip(&mut self, from: Option<&str>, to: &str, to_state: State, reason: Reason, delay: u64, db: &crate::db::Db, out: &mut Vec<Out>) {
        db.log_event(crate::now_ms(), "whip", to, 1);
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
        // A lost game is between the pets; a whip for slowness, or from the
        // user's own hand, is something the agent should hear about.
        if matches!(reason, Reason::User | Reason::Slow) && to_state != State::Attention {
            out.push(Out::Control(Control::Nudge { pet: to.into(), reason: reason.id().into() }));
        }
    }

    /// The pet that was not whipped, watching: a jeer, a kind word or a nervous
    /// glance, by how much it likes the one being whipped and whether it wears
    /// the crown. The other camp's main pet if there is one, else any other pet.
    fn whip_watch(&mut self, victim: &PetView, views: &[PetView], delay: u64, now: u64, db: &crate::db::Db, out: &mut Vec<Out>) {
        let free = |v: &&PetView| v.id != victim.id && v.label.is_none() && !self.scripts.contains_key(&v.id);
        let by = views
            .iter()
            .filter(free)
            .find(|v| v.camp.is_some() && v.camp != victim.camp)
            .or_else(|| views.iter().find(free));
        let Some(by) = by else { return };
        let aff = crate::games_db::affinity(db, &victim.pack, &by.pack);
        let crowned = by.camp.is_some() && crate::progress::champion(db, now) == by.camp;
        let (set, action) = match () {
            _ if self.rand() < 0.2 => (WATCH_NERVOUS, "peek"),
            _ if aff >= 60 => (WATCH_PITY, "comfort"),
            _ if aff < 30 || crowned => (WATCH_TAUNT, "taunt"),
            _ => (WATCH_NERVOUS, "peek"),
        };
        let line = self.whip_line(set);
        out.push(Out::Ws(crate::msg(
            "pet.script",
            &json!({ "pet": by.id, "rule": "whip_watch", "action": action, "bubble": line, "ms": WATCH_MS, "delay": delay + WHIP_HIT + 350 }),
        )));
    }

    /// After a duel or race: the winner says its piece, walks over and cracks
    /// the whip; the loser sulks, then takes it. Both are busy until it is over.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn whip_walkover(&mut self, winner: &PetView, loser: &PetView, win_text: &str, lose_text: &str, rule: &str, now: u64, db: &crate::db::Db, out: &mut Vec<Out>) {
        let until = now + WHIP_WALK + WHIP_HIT + WHIP_MS;
        self.scripts.insert(winner.id.clone(), Script { rule: rule.into(), until, during_attention: false, moved: true });
        self.scripts.insert(loser.id.clone(), Script { rule: rule.into(), until, during_attention: false, moved: false });
        out.push(Out::Control(Control::Move { pet: winner.id.clone(), target: Some(loser.id.clone()) }));
        // Bubble only while walking: an action would cut the walk animation.
        out.push(Out::Ws(crate::msg("pet.script", &json!({ "pet": winner.id, "rule": rule, "bubble": win_text, "ms": WHIP_WALK, "delay": 0 }))));
        out.push(Out::Ws(crate::msg("pet.script", &json!({ "pet": loser.id, "rule": rule, "action": "sulk", "bubble": lose_text, "ms": WHIP_WALK + 200, "delay": 0 }))));
        self.whip(Some(&winner.id), &loser.id, loser.state, Reason::Game, WHIP_WALK, db, out);
    }

    /// Lost a round to the user (rock-paper-scissors): the user holds the whip,
    /// the pet takes it like any beaten player, `delay` ms after the verdict.
    pub fn lost_to_user(&mut self, pet: &str, views: &[PetView], delay: u64, db: &crate::db::Db) -> Vec<Out> {
        let Some(v) = views.iter().find(|v| v.id == pet) else { return vec![] };
        let mut out = vec![];
        self.whip(None, pet, v.state, Reason::Game, delay, db, &mut out);
        self.whip_watch(v, views, delay, crate::now_ms(), db, &mut out);
        out
    }

    /// Right-click → 小皮鞭. The user holds the whip; the pet answers to its state.
    pub fn user_whip(&mut self, pet: &str, views: &[PetView], now: u64, db: &crate::db::Db) -> Result<Vec<Out>, String> {
        let v = views.iter().find(|v| v.id == pet).ok_or_else(|| crate::tr!("找不到这只宠物", "Can't find that pet"))?;
        let mut out = vec![];
        db.log_interaction(now, pet, "whip", self.frozen);
        self.whip(None, pet, v.state, Reason::User, 0, db, &mut out);
        self.whip_watch(v, views, 0, now, db, &mut out);
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
        let db = Db::in_memory();
        let out = w.user_whip("codex", &views, 1_793_600_000_000, &db).unwrap();
        assert_eq!(db.report(&crate::db::day_of(crate::now_ms())).whipped.get("codex"), Some(&1), "the lash is on the report");
        let lash = ws(&out, "whip");
        assert_eq!(lash.len(), 1);
        assert_eq!(lash[0]["to"], "codex");
        assert!(lash[0]["from"].is_null(), "the user holds the whip");
        let lines = ws(&out, "pet.script");
        assert_eq!(lines.len(), 2, "the victim's line and the onlooker's");
        assert_eq!(lines[0]["pet"], "codex");
        assert_eq!(lines[0]["delay"], WHIP_HIT);
        assert_eq!(lines[1]["pet"], "claude-code");
        assert_eq!(lines[1]["rule"], "whip_watch");
        assert!(lines[1]["delay"].as_u64().unwrap() > WHIP_HIT, "the onlooker reacts after the lash lands");
        assert!(["taunt", "comfort", "peek"].contains(&lines[1]["action"].as_str().unwrap()));
        assert!(
            out.iter().any(|o| matches!(o, Out::Control(Control::Nudge { pet, reason }) if pet == "codex" && reason == "user")),
            "the agent hears about a whip from the user"
        );
        assert!(w.scripts.is_empty(), "a user whip never blocks the pet");
    }

    #[test]
    fn a_lost_game_is_not_the_agents_business() {
        let mut w = World::new(Tables::builtin(), 3);
        let views = two_pets();
        let out = w.lost_to_user("codex", &views, 0, &Db::in_memory());
        assert!(!out.iter().any(|o| matches!(o, Out::Control(Control::Nudge { .. }))));
        assert!(ws(&out, "pet.script").iter().any(|s| s["rule"] == "whip_watch"), "but the other pet still watches");
    }

    #[test]
    fn alone_nobody_watches() {
        let mut w = World::new(Tables::builtin(), 3);
        let views: Vec<PetView> = two_pets().into_iter().filter(|v| v.id == "codex").collect();
        let out = w.user_whip("codex", &views, 1_793_600_000_000, &Db::in_memory()).unwrap();
        assert_eq!(ws(&out, "pet.script").len(), 1);
    }

    #[test]
    fn walkover_moves_the_winner_and_busies_both() {
        let mut w = World::new(Tables::builtin(), 3);
        let views = two_pets();
        let (a, b) = (views[0].clone(), views[1].clone());
        let mut out = vec![];
        w.whip_walkover(&a, &b, "赢", "输", "duel", 0, &Db::in_memory(), &mut out);
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
