//! Local SQLite store (~/.familiar/familiar.db): turns, wallet log, fish
//! basket, bug kills, pet mood, interaction log. Nothing leaves the machine.
//!
//! The wallet balance is the sum of wallet_log (no balance column), so it can
//! always be audited and rolled back (design §8).

use chrono::{Local, TimeZone, Timelike};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::path::Path;
use std::sync::Mutex;

use crate::session::FinishedTurn;

pub struct Db {
    conn: Mutex<Connection>,
}

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS turns (
  id INTEGER PRIMARY KEY,
  day TEXT NOT NULL, source TEXT NOT NULL, session TEXT NOT NULL, project TEXT NOT NULL,
  started_at INTEGER NOT NULL, ended_at INTEGER NOT NULL, ms INTEGER NOT NULL,
  result TEXT NOT NULL, tool_calls INTEGER NOT NULL, tool_errors INTEGER NOT NULL, attentions INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS turns_day ON turns(day);
CREATE TABLE IF NOT EXISTS daily (
  day TEXT NOT NULL, source TEXT NOT NULL, turns INTEGER NOT NULL, clean INTEGER NOT NULL,
  errors INTEGER NOT NULL, ms INTEGER NOT NULL, PRIMARY KEY (day, source)
);
CREATE TABLE IF NOT EXISTS wallet_log (
  id INTEGER PRIMARY KEY, ts INTEGER NOT NULL, day TEXT NOT NULL,
  delta INTEGER NOT NULL, reason TEXT NOT NULL, ref TEXT NOT NULL DEFAULT ''
);
CREATE INDEX IF NOT EXISTS wallet_day ON wallet_log(day);
CREATE TABLE IF NOT EXISTS catches (
  id INTEGER PRIMARY KEY, ts INTEGER NOT NULL, day TEXT NOT NULL, outcome TEXT NOT NULL,
  fish_id TEXT NOT NULL DEFAULT '', rarity TEXT NOT NULL DEFAULT '', coins INTEGER NOT NULL DEFAULT 0,
  source TEXT NOT NULL, session TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS catches_day ON catches(day);
CREATE TABLE IF NOT EXISTS bug_kills (
  id INTEGER PRIMARY KEY, ts INTEGER NOT NULL, day TEXT NOT NULL, boss INTEGER NOT NULL, coins INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS pet_stats (
  pack TEXT PRIMARY KEY, mood INTEGER NOT NULL DEFAULT 0, petted INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS interactions_log (
  id INTEGER PRIMARY KEY, ts INTEGER NOT NULL, actor TEXT NOT NULL, rule TEXT NOT NULL,
  attention_active INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS kv (key TEXT PRIMARY KEY, value TEXT NOT NULL);
"#;

/// Local calendar day of a ms timestamp, "YYYY-MM-DD".
pub fn day_of(ts: u64) -> String {
    Local.timestamp_millis_opt(ts as i64).single().map(|d| d.format("%Y-%m-%d").to_string()).unwrap_or_default()
}

/// Milliseconds since local midnight.
pub fn ms_of_day(ts: u64) -> u64 {
    Local
        .timestamp_millis_opt(ts as i64)
        .single()
        .map(|d| (d.num_seconds_from_midnight() as u64) * 1000 + (ts % 1000))
        .unwrap_or(0)
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct CampDay {
    pub turns: u32,
    pub clean: u32,
    pub errors: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct CatchRow {
    pub ts: u64,
    pub outcome: String,
    pub fish_id: String,
    pub rarity: String,
    pub coins: i64,
    pub source: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub day: String,
    pub work_ms: u64,
    pub turns: u32,
    pub clean: u32,
    pub errors: u32,
    pub by_camp: std::collections::BTreeMap<String, CampDay>,
    pub catches: Vec<CatchRow>,
    pub bugs: u32,
    pub bosses: u32,
    pub coins_earned: i64,
    pub balance: i64,
}

impl Db {
    pub fn open(path: &Path) -> rusqlite::Result<Db> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.execute_batch(SCHEMA)?;
        conn.execute_batch(crate::progress::SCHEMA)?;
        conn.execute_batch(crate::games_db::SCHEMA)?;
        Ok(Db { conn: Mutex::new(conn) })
    }

    /// Run a query on the connection (for modules with their own tables).
    pub fn with<T>(&self, f: impl FnOnce(&Connection) -> rusqlite::Result<T>) -> rusqlite::Result<T> {
        f(&self.conn.lock().unwrap())
    }

    pub fn in_memory() -> Db {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA).unwrap();
        conn.execute_batch(crate::progress::SCHEMA).unwrap();
        conn.execute_batch(crate::games_db::SCHEMA).unwrap();
        Db { conn: Mutex::new(conn) }
    }

    pub fn record_turn(&self, source: &str, session: &str, project: &str, t: &FinishedTurn) {
        let c = self.conn.lock().unwrap();
        let _ = c.execute(
            "INSERT INTO turns (day, source, session, project, started_at, ended_at, ms, result, tool_calls, tool_errors, attentions)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                day_of(t.ended_at),
                source,
                session,
                project,
                t.started_at as i64,
                t.ended_at as i64,
                t.ended_at.saturating_sub(t.started_at) as i64,
                result_str(t.result),
                t.tool_calls,
                t.tool_errors,
                t.attentions
            ],
        );
    }

    pub fn balance(&self) -> i64 {
        let c = self.conn.lock().unwrap();
        c.query_row("SELECT COALESCE(SUM(delta), 0) FROM wallet_log", [], |r| r.get(0)).unwrap_or(0)
    }

    pub fn income_on(&self, day: &str) -> i64 {
        let c = self.conn.lock().unwrap();
        c.query_row("SELECT COALESCE(SUM(delta), 0) FROM wallet_log WHERE day = ?1 AND delta > 0", [day], |r| r.get(0))
            .unwrap_or(0)
    }

    /// Add coins. Income is capped per day; the part over the cap is not
    /// paid. Returns what was actually granted.
    pub fn earn(&self, amount: i64, reason: &str, reference: &str, now: u64, daily_cap: i64) -> i64 {
        if amount <= 0 {
            return 0;
        }
        let day = day_of(now);
        let room = (daily_cap - self.income_on(&day)).max(0);
        let grant = amount.min(room);
        if grant > 0 {
            let c = self.conn.lock().unwrap();
            let _ = c.execute(
                "INSERT INTO wallet_log (ts, day, delta, reason, ref) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![now as i64, day, grant, reason, reference],
            );
        }
        grant
    }

    #[allow(clippy::too_many_arguments)]
    pub fn add_catch(&self, now: u64, outcome: &str, fish_id: &str, rarity: &str, coins: i64, source: &str, session: &str) {
        let c = self.conn.lock().unwrap();
        let _ = c.execute(
            "INSERT INTO catches (ts, day, outcome, fish_id, rarity, coins, source, session) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![now as i64, day_of(now), outcome, fish_id, rarity, coins, source, session],
        );
    }

    /// Fish (not boots or escapes) caught in the hour before `now`.
    pub fn fish_last_hour(&self, now: u64) -> u32 {
        let c = self.conn.lock().unwrap();
        c.query_row(
            "SELECT COUNT(*) FROM catches WHERE outcome = 'fish' AND ts > ?1",
            [now.saturating_sub(3_600_000) as i64],
            |r| r.get(0),
        )
        .unwrap_or(0)
    }

    pub fn record_bug_kill(&self, now: u64, boss: bool, coins: i64) {
        let c = self.conn.lock().unwrap();
        let _ = c.execute(
            "INSERT INTO bug_kills (ts, day, boss, coins) VALUES (?1, ?2, ?3, ?4)",
            params![now as i64, day_of(now), boss as i64, coins],
        );
    }

    pub fn normal_kills_total(&self) -> u64 {
        let c = self.conn.lock().unwrap();
        c.query_row("SELECT COUNT(*) FROM bug_kills WHERE boss = 0", [], |r| r.get::<_, i64>(0)).unwrap_or(0) as u64
    }

    pub fn add_mood(&self, pack: &str, delta: i64) {
        let c = self.conn.lock().unwrap();
        let _ = c.execute(
            "INSERT INTO pet_stats (pack, mood, petted) VALUES (?1, ?2, 1)
             ON CONFLICT(pack) DO UPDATE SET mood = mood + ?2, petted = petted + 1",
            params![pack, delta],
        );
    }

    pub fn mood(&self, pack: &str) -> i64 {
        let c = self.conn.lock().unwrap();
        c.query_row("SELECT mood FROM pet_stats WHERE pack = ?1", [pack], |r| r.get(0)).optional().ok().flatten().unwrap_or(0)
    }

    pub fn log_interaction(&self, now: u64, actor: &str, rule: &str, attention_active: bool) {
        let c = self.conn.lock().unwrap();
        let _ = c.execute(
            "INSERT INTO interactions_log (ts, actor, rule, attention_active) VALUES (?1, ?2, ?3, ?4)",
            params![now as i64, actor, rule, attention_active as i64],
        );
    }

    /// Per-camp (turns, clean, errors) for one day; subagents never get here.
    pub fn camp_day(&self, day: &str) -> std::collections::BTreeMap<String, CampDay> {
        let c = self.conn.lock().unwrap();
        let mut out = std::collections::BTreeMap::new();
        let mut st = match c.prepare(
            "SELECT source, COUNT(*),
                    SUM(CASE WHEN result = 'ok' AND tool_errors = 0 THEN 1 ELSE 0 END),
                    SUM(CASE WHEN result = 'error' THEN 1 ELSE 0 END)
             FROM turns WHERE day = ?1 AND result != 'aborted' GROUP BY source",
        ) {
            Ok(s) => s,
            Err(_) => return out,
        };
        let rows = st.query_map([day], |r| {
            Ok((r.get::<_, String>(0)?, CampDay { turns: r.get(1)?, clean: r.get(2)?, errors: r.get(3)? }))
        });
        if let Ok(rows) = rows {
            for (k, v) in rows.flatten() {
                out.insert(k, v);
            }
        }
        out
    }

    /// (turns, clean, errors) of one session on one day (same-camp tug).
    pub fn session_day(&self, day: &str, session: &str) -> CampDay {
        let c = self.conn.lock().unwrap();
        c.query_row(
            "SELECT COUNT(*),
                    COALESCE(SUM(CASE WHEN result = 'ok' AND tool_errors = 0 THEN 1 ELSE 0 END), 0),
                    COALESCE(SUM(CASE WHEN result = 'error' THEN 1 ELSE 0 END), 0)
             FROM turns WHERE day = ?1 AND session = ?2 AND result != 'aborted'",
            [day, session],
            |r| Ok(CampDay { turns: r.get(0)?, clean: r.get(1)?, errors: r.get(2)? }),
        )
        .unwrap_or_default()
    }

    /// Times of day (ms since midnight) of every finished turn on `day`, sorted.
    pub fn turn_times(&self, day: &str) -> Vec<u64> {
        let c = self.conn.lock().unwrap();
        let mut st = match c.prepare("SELECT ended_at FROM turns WHERE day = ?1 AND result != 'aborted' ORDER BY ended_at") {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        st.query_map([day], |r| r.get::<_, i64>(0)).map(|rows| rows.flatten().map(|t| ms_of_day(t as u64)).collect()).unwrap_or_default()
    }

    /// The day (other than `today`) with the most finished turns.
    pub fn best_day(&self, today: &str) -> Option<(String, u32)> {
        let c = self.conn.lock().unwrap();
        c.query_row(
            "SELECT day, COUNT(*) AS n FROM turns WHERE day != ?1 AND result != 'aborted' GROUP BY day ORDER BY n DESC, day DESC LIMIT 1",
            [today],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .ok()
        .flatten()
    }

    /// How many of each fish were ever caught: fish_id -> (count, first ts).
    pub fn fish_counts(&self) -> std::collections::BTreeMap<String, (u32, u64)> {
        let c = self.conn.lock().unwrap();
        let mut out = std::collections::BTreeMap::new();
        if let Ok(mut st) = c.prepare("SELECT fish_id, COUNT(*), MIN(ts) FROM catches WHERE outcome IN ('fish', 'eaten', 'stolen') GROUP BY fish_id") {
            if let Ok(rows) = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, u32>(1)?, r.get::<_, i64>(2)?))) {
                for (id, n, ts) in rows.flatten() {
                    out.insert(id, (n, ts as u64));
                }
            }
        }
        out
    }

    pub fn report(&self, day: &str) -> Report {
        let by_camp = self.camp_day(day);
        let c = self.conn.lock().unwrap();
        let work_ms: i64 = c
            .query_row("SELECT COALESCE(SUM(ms), 0) FROM turns WHERE day = ?1", [day], |r| r.get(0))
            .unwrap_or(0);
        let mut catches = vec![];
        if let Ok(mut st) =
            c.prepare("SELECT ts, outcome, fish_id, rarity, coins, source FROM catches WHERE day = ?1 ORDER BY ts")
        {
            if let Ok(rows) = st.query_map([day], |r| {
                Ok(CatchRow {
                    ts: r.get::<_, i64>(0)? as u64,
                    outcome: r.get(1)?,
                    fish_id: r.get(2)?,
                    rarity: r.get(3)?,
                    coins: r.get(4)?,
                    source: r.get(5)?,
                })
            }) {
                catches = rows.flatten().collect();
            }
        }
        let (bugs, bosses): (u32, u32) = c
            .query_row(
                "SELECT COALESCE(SUM(CASE WHEN boss = 0 THEN 1 ELSE 0 END), 0), COALESCE(SUM(boss), 0) FROM bug_kills WHERE day = ?1",
                [day],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap_or((0, 0));
        let coins_earned: i64 = c
            .query_row("SELECT COALESCE(SUM(delta), 0) FROM wallet_log WHERE day = ?1 AND delta > 0", [day], |r| r.get(0))
            .unwrap_or(0);
        let balance: i64 = c.query_row("SELECT COALESCE(SUM(delta), 0) FROM wallet_log", [], |r| r.get(0)).unwrap_or(0);
        drop(c);
        let (turns, clean, errors) =
            by_camp.values().fold((0, 0, 0), |(a, b, e), d| (a + d.turns, b + d.clean, e + d.errors));
        Report {
            day: day.to_string(),
            work_ms: work_ms as u64,
            turns,
            clean,
            errors,
            by_camp,
            catches,
            bugs,
            bosses,
            coins_earned,
            balance,
        }
    }

    pub fn kv_get(&self, key: &str) -> Option<String> {
        let c = self.conn.lock().unwrap();
        c.query_row("SELECT value FROM kv WHERE key = ?1", [key], |r| r.get(0)).optional().ok().flatten()
    }

    pub fn kv_set(&self, key: &str, value: &str) {
        let c = self.conn.lock().unwrap();
        let _ = c.execute(
            "INSERT INTO kv (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = ?2",
            params![key, value],
        );
    }

    /// Fold turns older than 90 days into `daily`, then delete them.
    pub fn compact(&self, now: u64) {
        let cutoff = day_of(now.saturating_sub(90 * 86_400_000));
        let c = self.conn.lock().unwrap();
        let _ = c.execute_batch(&format!(
            "INSERT OR REPLACE INTO daily (day, source, turns, clean, errors, ms)
               SELECT day, source, COUNT(*),
                      SUM(CASE WHEN result = 'ok' AND tool_errors = 0 THEN 1 ELSE 0 END),
                      SUM(CASE WHEN result = 'error' THEN 1 ELSE 0 END), SUM(ms)
               FROM turns WHERE day < '{cutoff}' GROUP BY day, source;
             DELETE FROM turns WHERE day < '{cutoff}';"
        ));
    }
}

fn result_str(r: crate::protocol::TurnResult) -> &'static str {
    match r {
        crate::protocol::TurnResult::Ok => "ok",
        crate::protocol::TurnResult::Error => "error",
        crate::protocol::TurnResult::Aborted => "aborted",
    }
}

/// Local weekday of a timestamp, 0 = Monday.
pub fn weekday(ts: u64) -> u32 {
    use chrono::Datelike;
    Local.timestamp_millis_opt(ts as i64).single().map(|d| d.weekday().num_days_from_monday()).unwrap_or(0)
}

/// Local hour and minute now (for the off-work report).
pub fn local_hm(ts: u64) -> (u32, u32) {
    Local.timestamp_millis_opt(ts as i64).single().map(|d| (d.hour(), d.minute())).unwrap_or((0, 0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::TurnResult;

    fn turn(start: u64, end: u64, result: TurnResult, errors: u32) -> FinishedTurn {
        FinishedTurn { started_at: start, ended_at: end, result, tool_calls: 3, tool_errors: errors, attentions: 0 }
    }

    #[test]
    fn wallet_is_capped_per_day() {
        let db = Db::in_memory();
        let now = 1_793_600_000_000;
        assert_eq!(db.earn(250, "fish", "", now, 300), 250);
        assert_eq!(db.earn(80, "fish", "", now, 300), 50, "only 50 left under the cap");
        assert_eq!(db.earn(10, "fish", "", now, 300), 0);
        assert_eq!(db.balance(), 300);
        assert_eq!(db.earn(10, "fish", "", now + 86_400_000, 300), 10, "new day, new cap");
    }

    #[test]
    fn camp_scores_and_report() {
        let db = Db::in_memory();
        let t0 = 1_793_600_000_000;
        db.record_turn("claude-code", "a", "/p", &turn(t0, t0 + 60_000, TurnResult::Ok, 0));
        db.record_turn("claude-code", "a", "/p", &turn(t0, t0 + 120_000, TurnResult::Ok, 2));
        db.record_turn("claude-code", "a", "/p", &turn(t0, t0 + 10_000, TurnResult::Error, 1));
        db.record_turn("codex", "b", "/p", &turn(t0, t0 + 5_000, TurnResult::Aborted, 0));
        let d = db.camp_day(&day_of(t0 + 60_000));
        assert_eq!(d["claude-code"], CampDay { turns: 3, clean: 1, errors: 1 });
        assert!(!d.contains_key("codex"), "aborted turns don't count");
        db.add_catch(t0, "fish", "todo", "common", 2, "claude-code", "a");
        db.record_bug_kill(t0, false, 0);
        db.record_bug_kill(t0, true, 5);
        let r = db.report(&day_of(t0));
        assert_eq!((r.turns, r.clean, r.errors), (3, 1, 1));
        assert_eq!(r.work_ms, 60_000 + 120_000 + 10_000 + 5_000);
        assert_eq!((r.bugs, r.bosses), (1, 1));
        assert_eq!(r.catches.len(), 1);
    }
}
