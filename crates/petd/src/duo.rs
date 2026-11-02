//! Two-pet mini games the pets act out on the desktop: badminton, ping-pong
//! and arm wrestling. Like the tug of war, the real pets play (one walks up
//! to the other); the overlay draws the shuttle / ball / net and each pet
//! window draws its own racket or arm. Everything runs on one timeline that
//! starts `DUO_WALK` ms after the "duo" message.

use crate::hub::PetView;
use crate::session::State;
use crate::world::{Out, Script, World};
use crate::Control;
use serde_json::json;

/// Time for the walk over, then the game starts (same on every window).
pub const DUO_WALK: u64 = 1300;

pub struct DuoGame {
    pub id: &'static str,
    pub name: &'static str,
    /// Gap between the two windows, logical px.
    pub gap: f64,
    pub open: &'static str,
    pub win: &'static str,
    pub lose: &'static str,
    pub name_en: &'static str,
    pub open_en: &'static str,
    pub win_en: &'static str,
    pub lose_en: &'static str,
}

impl DuoGame {
    /// Display name in the current language.
    pub fn label(&self) -> &'static str {
        crate::i18n::t(self.name, self.name_en)
    }
    fn open_line(&self) -> &'static str {
        crate::i18n::t(self.open, self.open_en)
    }
    fn win_line(&self) -> &'static str {
        crate::i18n::t(self.win, self.win_en)
    }
    fn lose_line(&self) -> &'static str {
        crate::i18n::t(self.lose, self.lose_en)
    }
}

pub const DUO_GAMES: &[DuoGame] = &[
    DuoGame { id: "badminton", name: "羽毛球", gap: 300.0, open: "来打羽毛球！", win: "扣杀得分！", lose: "没接住……", name_en: "Badminton", open_en: "Badminton time!", win_en: "Smash! Point!", lose_en: "Missed it…" },
    DuoGame { id: "pingpong", name: "乒乓球", gap: 130.0, open: "乒乓球走起～", win: "这球我拿下了！", lose: "擦边球不算！", name_en: "Ping-pong", open_en: "Ping-pong, let's go~", win_en: "That point's mine!", lose_en: "Edge ball doesn't count!" },
    DuoGame { id: "armwrestle", name: "掰手腕", gap: 0.0, open: "掰个手腕？", win: "力拔山兮！", lose: "手腕好酸……", name_en: "Arm wrestling", open_en: "Arm wrestle?", win_en: "Mighty muscles!", lose_en: "My wrist hurts…" },
];

impl World {
    fn duo_partner<'a>(views: &'a [PetView], pet: &str) -> Option<&'a PetView> {
        let me = views.iter().find(|v| v.id == pet)?;
        if let Some(other) = me.camp.as_deref().and_then(Self::other_camp).and_then(|c| Self::camp_pet(views, c)) {
            return Some(other);
        }
        views.iter().find(|v| v.id != pet)
    }

    /// Right-click → 游戏 → 羽毛球 / 乒乓球 / 掰手腕 (`game` None: random).
    pub fn request_duo(&mut self, game: Option<&str>, pet: &str, now: u64, views: &[PetView], db: &crate::db::Db) -> Result<Vec<Out>, String> {
        if self.frozen {
            return Err(crate::tr!("有会话在等你，先处理它", "A session is waiting for you — handle it first"));
        }
        if self.focus {
            return Err(crate::tr!("专注模式中，先关掉专注模式", "Focus mode is on — turn it off first"));
        }
        let a = views.iter().find(|v| v.id == pet).cloned().ok_or_else(|| crate::tr!("找不到这只宠物", "Can't find that pet"))?;
        let b = Self::duo_partner(views, pet).cloned().ok_or_else(|| crate::tr!("得有两只宠物才能玩，等另一边上线吧", "Needs two pets — wait for the other one to come online"))?;
        for p in [&a, &b] {
            if self.scripts.contains_key(&p.id) {
                return Err(crate::tr!("它们正忙着呢，等一下", "They're busy — hang on"));
            }
        }
        let mut out = vec![];
        self.play_duo(game, &a, &b, now, db, views, &mut out);
        Ok(out)
    }

    /// Two idle pets sometimes start a game on their own.
    pub(crate) fn try_duo(&mut self, now: u64, views: &[PetView], db: &crate::db::Db, out: &mut Vec<Out>) {
        let (Some(a), Some(b)) = (Self::camp_pet(views, "claude-code").cloned(), Self::camp_pet(views, "codex").cloned()) else { return };
        let idle = |v: &PetView| matches!(v.state, State::Idle | State::Done) && now.saturating_sub(v.since) >= 120_000;
        if !idle(&a) || !idle(&b) || self.scripts.contains_key(&a.id) || self.scripts.contains_key(&b.id) {
            return;
        }
        if !self.under_cap(&a.id, now) || !self.under_cap(&b.id, now) || self.rand() >= 0.12 {
            return;
        }
        let ck = ("duo".to_string(), "pair".to_string());
        if self.cooldowns.get(&ck).is_some_and(|t| now < *t) {
            return;
        }
        self.cooldowns.insert(ck, now + 20 * 60_000);
        let (anchor, mover) = if self.rand() < 0.5 { (a, b) } else { (b, a) };
        self.play_duo(None, &anchor, &mover, now, db, views, out);
    }

    #[allow(clippy::too_many_arguments)]
    fn play_duo(&mut self, game: Option<&str>, anchor: &PetView, mover: &PetView, now: u64, db: &crate::db::Db, views: &[PetView], out: &mut Vec<Out>) {
        let g = match game.and_then(|id| DUO_GAMES.iter().find(|g| g.id == id)) {
            Some(g) => g,
            None => &DUO_GAMES[((self.rand() * DUO_GAMES.len() as f64) as usize).min(DUO_GAMES.len() - 1)],
        };
        let anchor_wins = self.rand() < 0.5;
        let (winner, loser) = if anchor_wins { (anchor, mover) } else { (mover, anchor) };
        // Rallies: the anchor serves; the last shot is the winner's, so the
        // hit count's parity follows from who wins.
        let (each, hits, ms) = match g.id {
            "badminton" => {
                let mut n = 4 + (self.rand() * 5.0) as u64;
                if (n % 2 == 1) != anchor_wins {
                    n += 1;
                }
                (950u64, n, n * 950 + 700)
            }
            "pingpong" => {
                let mut n = 7 + (self.rand() * 6.0) as u64;
                if (n % 2 == 1) != anchor_wins {
                    n += 1;
                }
                (520, n, n * 520 + 600)
            }
            _ => (0, 0, 5200),
        };
        let seed = (self.rand() * 1000.0).round();
        for p in [anchor, mover] {
            self.recent.entry(p.id.clone()).or_default().push_back(now);
            let moved = p.id == mover.id;
            self.scripts.insert(p.id.clone(), Script { rule: "duo".into(), until: now + DUO_WALK + ms + 2600, during_attention: false, moved });
            db.log_interaction(now, &p.id, g.id, false);
        }
        out.push(Out::Control(Control::Duo { mover: mover.id.clone(), anchor: anchor.id.clone(), gap: g.gap }));
        out.push(Out::Ws(crate::msg(
            "duo",
            &json!({
                "game": g.id, "anchor": anchor.id, "mover": mover.id, "winner": winner.id, "loser": loser.id,
                "hits": hits, "each": each, "ms": ms, "walk": DUO_WALK, "seed": seed,
            }),
        )));
        out.push(Out::Ws(crate::msg("pet.script", &json!({ "pet": anchor.id, "rule": "duo", "bubble": g.open_line(), "ms": 1400, "delay": 0 }))));
        let end = DUO_WALK + ms;
        out.push(Out::Ws(crate::msg("pet.script", &json!({ "pet": winner.id, "rule": "duo", "action": "cheer", "bubble": g.win_line(), "ms": 2400, "delay": end }))));
        out.push(Out::Ws(crate::msg("pet.script", &json!({ "pet": loser.id, "rule": "duo", "action": "sulk", "bubble": g.lose_line(), "ms": 2400, "delay": end }))));
        self.bond(db, views, 1, out);
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

    #[test]
    fn request_duo_walks_over_and_scripts_both_pets() {
        let mut w = World::new(Tables::builtin(), 3);
        let mut h = Hub::new(Config::default(), Library::default());
        let db = Db::in_memory();
        let t0 = 1_793_600_000_000;
        for (s, id) in [("claude-code", "a"), ("codex", "b")] {
            let e = Event { v: 1, source: s.into(), session: id.into(), project: "/w/p".into(), event: EventKind::SessionStart, sub: false, detail: Detail::default(), pid: 0, ts: t0 };
            h.apply(&e, t0);
        }
        let views = h.pets();
        let out = w.request_duo(Some("badminton"), "claude-code", t0, &views, &db).unwrap();
        assert!(out.iter().any(|o| matches!(o, Out::Control(Control::Duo { mover, anchor, .. }) if mover == "codex" && anchor == "claude-code")));
        let duo: serde_json::Value = out
            .iter()
            .find_map(|o| match o {
                Out::Ws(s) => serde_json::from_str::<serde_json::Value>(s).ok().filter(|v| v["type"] == "duo"),
                _ => None,
            })
            .unwrap()["data"]
            .clone();
        let hits = duo["hits"].as_u64().unwrap();
        assert_eq!(duo["winner"] == "claude-code", hits % 2 == 1);
        // Busy now: a second request is refused.
        assert!(w.request_duo(None, "codex", t0 + 10, &views, &db).is_err());
    }

    #[test]
    fn last_shot_belongs_to_the_winner() {
        // Shots alternate from the anchor: shot i (0-based) is the anchor's
        // when i is even, so the anchor wins exactly when the count is odd.
        for n in 4..14u64 {
            let last_is_anchor = (n - 1) % 2 == 0;
            assert_eq!(last_is_anchor, n % 2 == 1);
        }
    }
}
