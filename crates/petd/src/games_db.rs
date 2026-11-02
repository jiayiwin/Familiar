//! Storage for M5 games: races, bets, duels, affinity, evolution counters.

use crate::db::{day_of, Db};
use rusqlite::{params, OptionalExtension};
use serde::Serialize;

pub const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS races (id INTEGER PRIMARY KEY, ts INTEGER NOT NULL, winner TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS bets (
  id INTEGER PRIMARY KEY, ts INTEGER NOT NULL, game TEXT NOT NULL, pick TEXT NOT NULL,
  stake INTEGER NOT NULL, odds REAL NOT NULL, payout INTEGER NOT NULL DEFAULT 0, settled INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS duels (id INTEGER PRIMARY KEY, ts INTEGER NOT NULL, project TEXT NOT NULL, winner TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS affinity (pair TEXT PRIMARY KEY, value INTEGER NOT NULL, flags INTEGER NOT NULL DEFAULT 0);
CREATE TABLE IF NOT EXISTS pack_turns (pack TEXT PRIMARY KEY, turns INTEGER NOT NULL);
"#;

/// Odds for each camp from the last 20 races: 0.9 / win rate, clamped to
/// 1.2–5.0; 1.8 each until there are 5 races (design §7.3).
pub fn race_odds(db: &Db) -> (f64, f64) {
    let winners: Vec<String> = db
        .with(|c| {
            let mut st = c.prepare("SELECT winner FROM races ORDER BY id DESC LIMIT 20")?;
            let rows = st.query_map([], |r| r.get::<_, String>(0))?;
            Ok(rows.flatten().collect())
        })
        .unwrap_or_default();
    let decided: Vec<&String> = winners.iter().filter(|w| *w != "tie").collect();
    if decided.len() < 5 {
        return (1.8, 1.8);
    }
    let n = decided.len() as f64;
    let odds = |camp: &str| {
        let p = decided.iter().filter(|w| w.as_str() == camp).count() as f64 / n;
        if p <= 0.0 {
            5.0
        } else {
            (0.9 / p).clamp(1.2, 5.0)
        }
    };
    (odds("claude-code"), odds("codex"))
}

pub fn record_race(db: &Db, now: u64, winner: &str) {
    let _ = db.with(|c| c.execute("INSERT INTO races (ts, winner) VALUES (?1, ?2)", params![now as i64, winner]));
}

pub fn record_duel(db: &Db, now: u64, project: &str, winner: &str) {
    let _ = db.with(|c| c.execute("INSERT INTO duels (ts, project, winner) VALUES (?1, ?2, ?3)", params![now as i64, project, winner]));
}

/// Take a stake out of the wallet. At most half the balance (design §8).
pub fn place_bet(db: &Db, now: u64, game: &str, pick: &str, stake: i64, odds: f64) -> Result<i64, String> {
    if stake <= 0 {
        return Err(crate::tr!("押注要大于 0", "Stake must be more than 0"));
    }
    let bal = db.balance();
    if stake * 2 > bal {
        return Err(crate::tr!("单注最多押当前金币的一半（{}）", "You can bet at most half your coins ({})", bal / 2));
    }
    db.with(|c| {
        c.execute(
            "INSERT INTO wallet_log (ts, day, delta, reason, ref) VALUES (?1, ?2, ?3, 'bet', ?4)",
            params![now as i64, day_of(now), -stake, format!("{game}:{pick}")],
        )?;
        c.execute(
            "INSERT INTO bets (ts, game, pick, stake, odds) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![now as i64, game, pick, stake, odds],
        )?;
        Ok(c.last_insert_rowid())
    })
    .map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Bet {
    pub id: i64,
    pub pick: String,
    pub stake: i64,
    pub odds: f64,
}

pub fn open_bets(db: &Db, game: &str) -> Vec<Bet> {
    db.with(|c| {
        let mut st = c.prepare("SELECT id, pick, stake, odds FROM bets WHERE game = ?1 AND settled = 0")?;
        let rows = st.query_map([game], |r| Ok(Bet { id: r.get(0)?, pick: r.get(1)?, stake: r.get(2)?, odds: r.get(3)? }))?;
        Ok(rows.flatten().collect())
    })
    .unwrap_or_default()
}

/// Settle every open bet of `game`: `won(pick)` is Some(true) win, Some(false)
/// lose, None refund. The stake comes back outside the income cap; only the
/// profit counts as income. Returns (net coins to the user, bets settled).
pub fn settle_bets(db: &Db, now: u64, game: &str, daily_cap: i64, won: impl Fn(&str) -> Option<bool>) -> (i64, usize) {
    let bets = open_bets(db, game);
    let mut net = 0;
    for b in &bets {
        let (back, profit) = match won(&b.pick) {
            Some(true) => (b.stake, ((b.stake as f64) * (b.odds - 1.0)).round() as i64),
            Some(false) => (0, 0),
            None => (b.stake, 0),
        };
        if back > 0 {
            let _ = db.with(|c| {
                c.execute(
                    "INSERT INTO wallet_log (ts, day, delta, reason, ref) VALUES (?1, '0000-00-00', ?2, 'bet_refund', ?3)",
                    params![now as i64, back, format!("{game}:{}", b.id)],
                )
            });
        }
        let paid = db.earn(profit, "bet_win", &format!("{game}:{}", b.id), now, daily_cap);
        net += back + paid - b.stake;
        let _ = db.with(|c| c.execute("UPDATE bets SET settled = 1, payout = ?1 WHERE id = ?2", params![back + paid, b.id]));
    }
    (net, bets.len())
}

fn pair_key(a: &str, b: &str) -> String {
    if a <= b {
        format!("{a}|{b}")
    } else {
        format!("{b}|{a}")
    }
}

pub fn affinity(db: &Db, a: &str, b: &str) -> i64 {
    db.with(|c| c.query_row("SELECT value FROM affinity WHERE pair = ?1", [pair_key(a, b)], |r| r.get(0)).optional())
        .ok()
        .flatten()
        .unwrap_or(0)
}

/// Raise affinity (0–100). Returns the thresholds newly crossed (20, 50, 80).
pub fn add_affinity(db: &Db, a: &str, b: &str, delta: i64) -> Vec<i64> {
    if a == b {
        return vec![];
    }
    let key = pair_key(a, b);
    let (old, flags): (i64, i64) = db
        .with(|c| c.query_row("SELECT value, flags FROM affinity WHERE pair = ?1", [&key], |r| Ok((r.get(0)?, r.get(1)?))).optional())
        .ok()
        .flatten()
        .unwrap_or((0, 0));
    let new = (old + delta).clamp(0, 100);
    let mut crossed = vec![];
    let mut f = flags;
    for (bit, at) in [(1, 20), (2, 50), (4, 80)] {
        if new >= at && f & bit == 0 {
            f |= bit;
            crossed.push(at);
        }
    }
    let _ = db.with(|c| {
        c.execute(
            "INSERT INTO affinity (pair, value, flags) VALUES (?1, ?2, ?3) ON CONFLICT(pair) DO UPDATE SET value = ?2, flags = ?3",
            params![key, new, f],
        )
    });
    crossed
}

pub fn add_pack_turn(db: &Db, pack: &str) -> i64 {
    db.with(|c| {
        c.execute(
            "INSERT INTO pack_turns (pack, turns) VALUES (?1, 1) ON CONFLICT(pack) DO UPDATE SET turns = turns + 1",
            [pack],
        )?;
        c.query_row("SELECT turns FROM pack_turns WHERE pack = ?1", [pack], |r| r.get(0))
    })
    .unwrap_or(0)
}

pub fn pack_turns(db: &Db, pack: &str) -> i64 {
    db.with(|c| c.query_row("SELECT turns FROM pack_turns WHERE pack = ?1", [pack], |r| r.get(0)).optional())
        .ok()
        .flatten()
        .unwrap_or(0)
}

/// Evolution stage from cumulative turns: 0 below 100, then 1 / 2 / 3.
pub fn stage(turns: i64, thresholds: &[i64]) -> u8 {
    thresholds.iter().filter(|t| turns >= **t).count() as u8
}

/// Mark one caught fish of today as eaten / stolen (or back to fish).
/// Returns the row id. `from_source` limits to one camp's basket.
pub fn take_fish(db: &Db, now: u64, from_source: Option<&str>, to: &str) -> Option<i64> {
    let day = day_of(now);
    db.with(|c| {
        let id: Option<i64> = match from_source {
            Some(s) => c
                .query_row(
                    "SELECT id FROM catches WHERE day = ?1 AND outcome = 'fish' AND source = ?2 ORDER BY rarity = 'common' DESC, id DESC LIMIT 1",
                    params![day, s],
                    |r| r.get(0),
                )
                .optional()?,
            None => c
                .query_row("SELECT id FROM catches WHERE outcome = 'fish' ORDER BY id DESC LIMIT 1", [], |r| r.get(0))
                .optional()?,
        };
        if let Some(id) = id {
            c.execute("UPDATE catches SET outcome = ?1 WHERE id = ?2", params![to, id])?;
        }
        Ok(id)
    })
    .ok()
    .flatten()
}

pub fn restore_fish(db: &Db, id: i64) {
    let _ = db.with(|c| c.execute("UPDATE catches SET outcome = 'fish' WHERE id = ?1", [id]));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn odds_follow_win_rate() {
        let db = Db::in_memory();
        assert_eq!(race_odds(&db), (1.8, 1.8));
        for w in ["claude-code", "claude-code", "claude-code", "codex", "claude-code"] {
            record_race(&db, 1, w);
        }
        let (cc, cx) = race_odds(&db);
        assert!((cc - 0.9 / 0.8).abs() < 1e-9 || cc == 1.2, "{cc}");
        assert!((cx - 4.5).abs() < 1e-9, "{cx}");
    }

    #[test]
    fn bets_win_lose_refund() {
        let db = Db::in_memory();
        let t = 1_793_600_000_000;
        db.earn(200, "test", "", t, 300);
        assert!(place_bet(&db, t, "race", "claude-code", 150, 2.0).is_err(), "max half");
        place_bet(&db, t, "race", "claude-code", 50, 2.0).unwrap();
        place_bet(&db, t, "race", "codex", 20, 1.5).unwrap();
        assert_eq!(db.balance(), 130);
        let (net, n) = settle_bets(&db, t, "race", 300, |p| Some(p == "claude-code"));
        assert_eq!(n, 2);
        assert_eq!(net, 50 - 20, "won 50 on CC, lost 20 on CX");
        assert_eq!(db.balance(), 230);
        place_bet(&db, t, "race", "codex", 30, 3.0).unwrap();
        let (net, _) = settle_bets(&db, t, "race", 300, |_| None);
        assert_eq!(net, 0, "refund");
        assert_eq!(db.balance(), 230);
    }

    #[test]
    fn affinity_thresholds_once() {
        let db = Db::in_memory();
        assert_eq!(add_affinity(&db, "cat", "bot", 19), Vec::<i64>::new());
        assert_eq!(add_affinity(&db, "bot", "cat", 1), vec![20]);
        assert_eq!(add_affinity(&db, "cat", "bot", 100), vec![50, 80]);
        assert_eq!(affinity(&db, "cat", "bot"), 100);
        assert!(add_affinity(&db, "cat", "bot", 5).is_empty());
    }

    #[test]
    fn stages() {
        assert_eq!(stage(99, &[100, 500, 2000]), 0);
        assert_eq!(stage(100, &[100, 500, 2000]), 1);
        assert_eq!(stage(2500, &[100, 500, 2000]), 3);
    }
}
