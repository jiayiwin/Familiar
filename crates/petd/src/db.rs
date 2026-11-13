//! Local SQLite store (~/.rillbuddy/rillbuddy.db): turns, fish basket, bug
//! kills, interaction log, day events, the bond. Nothing leaves the machine.
//!
//! A database from an earlier build also holds tables this one no longer
//! reads (wallet, shop, games); they are left alone.

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
CREATE TABLE IF NOT EXISTS catches (
  id INTEGER PRIMARY KEY, ts INTEGER NOT NULL, day TEXT NOT NULL, outcome TEXT NOT NULL,
  fish_id TEXT NOT NULL DEFAULT '', rarity TEXT NOT NULL DEFAULT '',
  source TEXT NOT NULL, session TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS catches_day ON catches(day);
CREATE TABLE IF NOT EXISTS bug_kills (
  id INTEGER PRIMARY KEY, ts INTEGER NOT NULL, day TEXT NOT NULL, boss INTEGER NOT NULL, coins INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS interactions_log (
  id INTEGER PRIMARY KEY, ts INTEGER NOT NULL, actor TEXT NOT NULL, rule TEXT NOT NULL,
  attention_active INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS kv (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS events (
  id INTEGER PRIMARY KEY, ts INTEGER NOT NULL, day TEXT NOT NULL, kind TEXT NOT NULL,
  what TEXT NOT NULL DEFAULT '', n INTEGER NOT NULL DEFAULT 1
);
CREATE INDEX IF NOT EXISTS events_day ON events(day, kind);
CREATE TABLE IF NOT EXISTS bond_moments (
  id INTEGER PRIMARY KEY, ts INTEGER NOT NULL, kind TEXT NOT NULL, detail TEXT NOT NULL DEFAULT '{}'
);
CREATE INDEX IF NOT EXISTS bond_moments_ts ON bond_moments(ts);
CREATE TABLE IF NOT EXISTS bond_fragments (
  id INTEGER PRIMARY KEY,
  created_at INTEGER NOT NULL,
  fire_at INTEGER NOT NULL,
  moment_kind TEXT NOT NULL,
  delivered INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS bond_fragments_fire ON bond_fragments(fire_at) WHERE delivered = 0;
"#;

/// Local calendar day of a ms timestamp, "YYYY-MM-DD".
pub fn day_of(ts: u64) -> String {
    Local
        .timestamp_millis_opt(ts as i64)
        .single()
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_default()
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
    pub source: String,
}

/// A turn as a letter reads it.
#[derive(Debug, Clone)]
pub struct TurnRow {
    pub day: String,
    pub source: String,
    pub project: String,
    pub started_at: u64,
    pub ended_at: u64,
    pub result: String,
    pub tool_errors: u32,
}

/// A notable moment in the user's work session, stored for letter context.
#[derive(Debug, Clone, Serialize)]
pub struct BondMoment {
    pub ts: u64,
    pub kind: String,
    pub detail: String,
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
    /// Seconds of radio: stations played in the radio window + crosstalk pieces.
    pub radio_s: u64,
    pub kegel_sets: u32,
    pub kegel_reps: u32,
    pub letters: u32,
    /// Most agent sessions online at once that day.
    pub peak_agents: u32,
}

impl Db {
    pub fn open(path: &Path) -> rusqlite::Result<Db> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.execute_batch(SCHEMA)?;
        Ok(Db {
            conn: Mutex::new(conn),
        })
    }

    /// Run a query on the connection (for modules with their own tables).
    pub fn with<T>(
        &self,
        f: impl FnOnce(&Connection) -> rusqlite::Result<T>,
    ) -> rusqlite::Result<T> {
        f(&self.conn.lock().unwrap())
    }

    pub fn in_memory() -> Db {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA).unwrap();
        Db {
            conn: Mutex::new(conn),
        }
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

    pub fn add_catch(
        &self,
        now: u64,
        outcome: &str,
        fish_id: &str,
        rarity: &str,
        source: &str,
        session: &str,
    ) {
        let c = self.conn.lock().unwrap();
        let _ = c.execute(
            "INSERT INTO catches (ts, day, outcome, fish_id, rarity, source, session) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![now as i64, day_of(now), outcome, fish_id, rarity, source, session],
        );
    }

    /// Take the newest fish out of the basket (it was fed to the pet). False when the basket is empty.
    pub fn eat_fish(&self) -> bool {
        self.eat_one().is_some()
    }

    /// Take the newest sweet out of the jar: which kind it was, or None when the jar is empty.
    pub fn eat_one(&self) -> Option<String> {
        self.with(|c| {
            let row: Option<(i64, String)> =
                c.query_row("SELECT id, fish_id FROM catches WHERE outcome = 'fish' ORDER BY id DESC LIMIT 1", [], |r| Ok((r.get(0)?, r.get(1)?))).optional()?;
            if let Some((id, _)) = &row {
                c.execute("UPDATE catches SET outcome = 'eaten' WHERE id = ?1", [id])?;
            }
            Ok(row.map(|(_, kind)| kind))
        })
        .unwrap_or(None)
    }

    /// Fish still in the basket: caught and not yet fed.
    pub fn fish_left(&self) -> i64 {
        self.with(|c| {
            c.query_row(
                "SELECT COUNT(*) FROM catches WHERE outcome = 'fish'",
                [],
                |r| r.get(0),
            )
        })
        .unwrap_or(0)
    }

    pub fn record_bug_kill(&self, now: u64, boss: bool) {
        let c = self.conn.lock().unwrap();
        // `coins` is spelled out because a database from an earlier build has the column without a default.
        let _ = c.execute(
            "INSERT INTO bug_kills (ts, day, boss, coins) VALUES (?1, ?2, ?3, 0)",
            params![now as i64, day_of(now), boss as i64],
        );
    }

    pub fn log_interaction(&self, now: u64, actor: &str, rule: &str, attention_active: bool) {
        let c = self.conn.lock().unwrap();
        let _ = c.execute(
            "INSERT INTO interactions_log (ts, actor, rule, attention_active) VALUES (?1, ?2, ?3, ?4)",
            params![now as i64, actor, rule, attention_active as i64],
        );
    }

    /// Per-agent (turns, clean, errors) for one day; subagents never get here.
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
            Ok((
                r.get::<_, String>(0)?,
                CampDay {
                    turns: r.get(1)?,
                    clean: r.get(2)?,
                    errors: r.get(3)?,
                },
            ))
        });
        if let Ok(rows) = rows {
            for (k, v) in rows.flatten() {
                out.insert(k, v);
            }
        }
        out
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
            .query_row(
                "SELECT COALESCE(SUM(ms), 0) FROM turns WHERE day = ?1",
                [day],
                |r| r.get(0),
            )
            .unwrap_or(0);
        let mut catches = vec![];
        if let Ok(mut st) = c.prepare(
            "SELECT ts, outcome, fish_id, rarity, source FROM catches WHERE day = ?1 ORDER BY ts",
        ) {
            if let Ok(rows) = st.query_map([day], |r| {
                Ok(CatchRow {
                    ts: r.get::<_, i64>(0)? as u64,
                    outcome: r.get(1)?,
                    fish_id: r.get(2)?,
                    rarity: r.get(3)?,
                    source: r.get(4)?,
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
        // Day events: (kind, what) -> (count, sum n).
        let mut ev: std::collections::BTreeMap<(String, String), (u32, i64)> = Default::default();
        if let Ok(mut st) = c.prepare("SELECT kind, what, COUNT(*), COALESCE(SUM(n), 0) FROM events WHERE day = ?1 GROUP BY kind, what") {
            if let Ok(rows) = st.query_map([day], |r| Ok(((r.get::<_, String>(0)?, r.get::<_, String>(1)?), (r.get::<_, u32>(2)?, r.get::<_, i64>(3)?)))) {
                ev = rows.flatten().collect();
            }
        }
        let kind = |k: &str| {
            ev.iter()
                .filter(|((kk, _), _)| kk == k)
                .map(|((_, w), v)| (w.clone(), *v))
                .collect::<Vec<_>>()
        };
        let radio_s = kind("radio")
            .iter()
            .map(|(_, (_, s))| *s)
            .sum::<i64>()
            .max(0) as u64;
        let (kegel_sets, kegel_reps) = kind("kegel")
            .iter()
            .fold((0u32, 0i64), |(a, b), (_, (n, s))| (a + n, b + s));
        let letters = kind("letter").iter().map(|(_, (n, _))| *n).sum();
        let peak_agents: u32 = c
            .query_row(
                "SELECT value FROM kv WHERE key = ?1",
                [format!("peak:{day}")],
                |r| r.get::<_, String>(0),
            )
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        drop(c);
        let (turns, clean, errors) = by_camp.values().fold((0, 0, 0), |(a, b, e), d| {
            (a + d.turns, b + d.clean, e + d.errors)
        });
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
            radio_s,
            kegel_sets,
            kegel_reps: kegel_reps.max(0) as u32,
            letters,
            peak_agents,
        }
    }

    /// One thing that happened: radio time (`n` = seconds), a kegel set (`n` =
    /// reps), a letter written, a line for the diary (`what`). The daily
    /// report adds them up.
    /// Whether an event of `kind` was logged since `since` (ms).
    pub fn event_since(&self, kind: &str, since: u64) -> bool {
        let c = self.conn.lock().unwrap();
        c.query_row(
            "SELECT 1 FROM events WHERE kind = ?1 AND ts >= ?2 LIMIT 1",
            params![kind, since as i64],
            |_| Ok(()),
        )
        .optional()
        .ok()
        .flatten()
        .is_some()
    }

    pub fn log_event(&self, now: u64, kind: &str, what: &str, n: i64) {
        let c = self.conn.lock().unwrap();
        let _ = c.execute(
            "INSERT INTO events (ts, day, kind, what, n) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![now as i64, day_of(now), kind, what, n],
        );
    }

    /// (time at the desk in ms, breaks taken) on `day`, as the think loop counts them.
    pub fn desk_day(&self, day: &str) -> (u64, u32) {
        let v = self.kv_get(&format!("desk:{day}")).unwrap_or_default();
        let mut it = v.split(',');
        (
            it.next().and_then(|s| s.parse().ok()).unwrap_or(0),
            it.next().and_then(|s| s.parse().ok()).unwrap_or(0),
        )
    }

    pub fn set_desk_day(&self, day: &str, ms: u64, breaks: u32) {
        self.kv_set(&format!("desk:{day}"), &format!("{ms},{breaks}"));
    }

    /// What the pet noticed on `day`, newest first: (when, the line).
    pub fn noticed(&self, day: &str) -> Vec<(u64, String)> {
        let c = self.conn.lock().unwrap();
        let mut st = match c.prepare("SELECT ts, what FROM events WHERE day = ?1 AND kind = 'noticed' ORDER BY ts DESC LIMIT 40") {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        st.query_map([day], |r| {
            Ok((r.get::<_, i64>(0)? as u64, r.get::<_, String>(1)?))
        })
        .map(|rows| rows.flatten().collect())
        .unwrap_or_default()
    }

    /// The newest lines of the diary, across days: (when, the line).
    pub fn noticed_recent(&self, limit: i64) -> Vec<(u64, String)> {
        let c = self.conn.lock().unwrap();
        let mut st = match c
            .prepare("SELECT ts, what FROM events WHERE kind = 'noticed' ORDER BY ts DESC LIMIT ?1")
        {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        st.query_map([limit], |r| {
            Ok((r.get::<_, i64>(0)? as u64, r.get::<_, String>(1)?))
        })
        .map(|rows| rows.flatten().collect())
        .unwrap_or_default()
    }

    /// Keep the day's high-water mark of agent sessions online at once.
    pub fn note_online(&self, now: u64, online: usize) {
        let key = format!("peak:{}", day_of(now));
        let best: usize = self.kv_get(&key).and_then(|v| v.parse().ok()).unwrap_or(0);
        if online > best {
            self.kv_set(&key, &online.to_string());
        }
    }

    /// A day's turns, oldest first (for 收工曲).
    pub fn day_turns(&self, day: &str) -> Vec<crate::song::Turn> {
        let c = self.conn.lock().unwrap();
        let Ok(mut st) = c.prepare(
            "SELECT source, started_at, ended_at, result, tool_calls, tool_errors FROM turns WHERE day = ?1 ORDER BY started_at",
        ) else {
            return Vec::new();
        };
        st.query_map([day], |r| {
            Ok(crate::song::Turn {
                source: r.get(0)?,
                started_at: r.get::<_, i64>(1)?.max(0) as u64,
                ended_at: r.get::<_, i64>(2)?.max(0) as u64,
                result: r.get(3)?,
                tool_calls: r.get::<_, i64>(4)?.max(0) as u32,
                tool_errors: r.get::<_, i64>(5)?.max(0) as u32,
            })
        })
        .map(|rows| rows.flatten().collect())
        .unwrap_or_default()
    }

    /// Every turn that started since `since` (ms), oldest first, with its project (for letters).
    pub fn turns_since(&self, since: u64) -> Vec<TurnRow> {
        let c = self.conn.lock().unwrap();
        let Ok(mut st) = c.prepare(
            "SELECT day, source, project, started_at, ended_at, result, tool_errors FROM turns WHERE started_at >= ?1 ORDER BY started_at",
        ) else {
            return Vec::new();
        };
        st.query_map([since as i64], |r| {
            Ok(TurnRow {
                day: r.get(0)?,
                source: r.get(1)?,
                project: r.get(2)?,
                started_at: r.get::<_, i64>(3)?.max(0) as u64,
                ended_at: r.get::<_, i64>(4)?.max(0) as u64,
                result: r.get(5)?,
                tool_errors: r.get::<_, i64>(6)?.max(0) as u32,
            })
        })
        .map(|rows| rows.flatten().collect())
        .unwrap_or_default()
    }

    /// Every day anything was seen (a turn, folded or not, or time at the desk), oldest first.
    pub fn days_seen(&self) -> Vec<String> {
        let c = self.conn.lock().unwrap();
        let Ok(mut st) = c.prepare(
            "SELECT day FROM turns UNION SELECT day FROM daily
             UNION SELECT substr(key, 6) FROM kv WHERE key LIKE 'desk:%' AND CAST(value AS INTEGER) > 0
             ORDER BY 1",
        ) else {
            return Vec::new();
        };
        st.query_map([], |r| r.get::<_, String>(0)).map(|rows| rows.flatten().collect()).unwrap_or_default()
    }

    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub fn add_turn_in(&self, source: &str, project: &str, start: u64, end: u64, result: &str, errors: u32) {
        let c = self.conn.lock().unwrap();
        c.execute(
            "INSERT INTO turns (day, source, session, project, started_at, ended_at, ms, result, tool_calls, tool_errors, attentions)
             VALUES (?1, ?2, 's', ?3, ?4, ?5, ?6, ?7, 3, ?8, 0)",
            params![day_of(end), source, project, start as i64, end as i64, (end - start) as i64, result, errors],
        )
        .unwrap();
    }

    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub fn add_turn_for_test(&self, day: &str, source: &str, start: u64, end: u64, result: &str, calls: u32, errors: u32) {
        let c = self.conn.lock().unwrap();
        c.execute(
            "INSERT INTO turns (day, source, session, project, started_at, ended_at, ms, result, tool_calls, tool_errors, attentions)
             VALUES (?1, ?2, 's', 'p', ?3, ?4, ?5, ?6, ?7, ?8, 0)",
            params![day, source, start as i64, end as i64, (end - start) as i64, result, calls, errors],
        )
        .unwrap();
    }

    pub fn kv_get(&self, key: &str) -> Option<String> {
        let c = self.conn.lock().unwrap();
        c.query_row("SELECT value FROM kv WHERE key = ?1", [key], |r| r.get(0))
            .optional()
            .ok()
            .flatten()
    }

    pub fn kv_set(&self, key: &str, value: &str) {
        let c = self.conn.lock().unwrap();
        let _ = c.execute(
            "INSERT INTO kv (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = ?2",
            params![key, value],
        );
    }

    pub fn log_bond_moment(&self, ts: u64, kind: &str, detail: &str) {
        let c = self.conn.lock().unwrap();
        let _ = c.execute(
            "INSERT INTO bond_moments (ts, kind, detail) VALUES (?1, ?2, ?3)",
            params![ts as i64, kind, detail],
        );
    }

    pub fn recent_bond_moments(&self, limit: i64) -> Vec<BondMoment> {
        let c = self.conn.lock().unwrap();
        let mut st = match c
            .prepare("SELECT ts, kind, detail FROM bond_moments ORDER BY ts DESC LIMIT ?1")
        {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        st.query_map([limit], |r| {
            Ok(BondMoment {
                ts: r.get::<_, i64>(0)? as u64,
                kind: r.get(1)?,
                detail: r.get(2)?,
            })
        })
        .map(|rows| rows.flatten().collect())
        .unwrap_or_default()
    }

    pub fn bond_scores(&self) -> (f64, f64, f64) {
        self.with(|c| {
            let get = |key: &str, default: f64| -> f64 {
                c.query_row("SELECT value FROM kv WHERE key = ?1", [key], |r| {
                    r.get::<_, String>(0)
                })
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(default)
            };
            let fam_raw = get("bond:familiarity", 0.0);
            let trust = get("bond:trust", 50.0);
            let vit = get("bond:vitality", 60.0);
            let last_ms = get("bond:last_active_ms", 0.0) as u64;
            let idle_days = if last_ms > 0 {
                (crate::now_ms().saturating_sub(last_ms) / 86_400_000).min(20) as f64
            } else {
                0.0
            };
            Ok(((fam_raw - idle_days).clamp(0.0, 100.0), trust, vit))
        })
        .unwrap_or((0.0, 50.0, 60.0))
    }

    pub fn queue_fragment(&self, created_at: u64, fire_at: u64, moment_kind: &str) {
        let c = self.conn.lock().unwrap();
        let _ = c.execute(
            "INSERT INTO bond_fragments (created_at, fire_at, moment_kind) VALUES (?1, ?2, ?3)",
            params![created_at as i64, fire_at as i64, moment_kind],
        );
    }

    pub fn due_fragments(&self, now: u64) -> Vec<(i64, String)> {
        let c = self.conn.lock().unwrap();
        let mut st = match c.prepare(
            "SELECT id, moment_kind FROM bond_fragments WHERE delivered = 0 AND fire_at <= ?1 LIMIT 3"
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        st.query_map([now as i64], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
        })
        .map(|rows| rows.flatten().collect())
        .unwrap_or_default()
    }

    pub fn mark_fragment_delivered(&self, id: i64) {
        let c = self.conn.lock().unwrap();
        let _ = c.execute(
            "UPDATE bond_fragments SET delivered = 1 WHERE id = ?1",
            [id],
        );
    }

    pub fn bond_update(&self, fam_delta: f64, trust_delta: f64, vit_delta: f64, now: u64) {
        let _ = self.with(|c| {
            let get = |key: &str, default: f64| -> f64 {
                c.query_row("SELECT value FROM kv WHERE key = ?1", [key], |r| {
                    r.get::<_, String>(0)
                })
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(default)
            };
            let fam = (get("bond:familiarity", 0.0) + fam_delta).clamp(0.0, 100.0);
            let trust = (get("bond:trust", 50.0) + trust_delta).clamp(0.0, 100.0);
            let vit = (get("bond:vitality", 60.0) + vit_delta).clamp(0.0, 100.0);
            c.execute(
                "INSERT OR REPLACE INTO kv VALUES ('bond:familiarity', ?1)",
                [&fam.to_string()],
            )?;
            c.execute(
                "INSERT OR REPLACE INTO kv VALUES ('bond:trust', ?1)",
                [&trust.to_string()],
            )?;
            c.execute(
                "INSERT OR REPLACE INTO kv VALUES ('bond:vitality', ?1)",
                [&vit.to_string()],
            )?;
            c.execute(
                "INSERT OR REPLACE INTO kv VALUES ('bond:last_active_ms', ?1)",
                [&now.to_string()],
            )?;
            Ok(())
        });
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
    Local
        .timestamp_millis_opt(ts as i64)
        .single()
        .map(|d| d.weekday().num_days_from_monday())
        .unwrap_or(0)
}

/// Local hour and minute now (for the off-work report).
pub fn local_hm(ts: u64) -> (u32, u32) {
    Local
        .timestamp_millis_opt(ts as i64)
        .single()
        .map(|d| (d.hour(), d.minute()))
        .unwrap_or((0, 0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::TurnResult;

    fn turn(start: u64, end: u64, result: TurnResult, errors: u32) -> FinishedTurn {
        FinishedTurn {
            started_at: start,
            ended_at: end,
            result,
            tool_calls: 3,
            tool_errors: errors,
            attentions: 0,
        }
    }

    #[test]
    fn camp_scores_and_report() {
        let db = Db::in_memory();
        let t0 = 1_793_600_000_000;
        db.record_turn(
            "claude-code",
            "a",
            "/p",
            &turn(t0, t0 + 60_000, TurnResult::Ok, 0),
        );
        db.record_turn(
            "claude-code",
            "a",
            "/p",
            &turn(t0, t0 + 120_000, TurnResult::Ok, 2),
        );
        db.record_turn(
            "claude-code",
            "a",
            "/p",
            &turn(t0, t0 + 10_000, TurnResult::Error, 1),
        );
        db.record_turn(
            "codex",
            "b",
            "/p",
            &turn(t0, t0 + 5_000, TurnResult::Aborted, 0),
        );
        let d = db.camp_day(&day_of(t0 + 60_000));
        assert_eq!(
            d["claude-code"],
            CampDay {
                turns: 3,
                clean: 1,
                errors: 1
            }
        );
        assert!(!d.contains_key("codex"), "aborted turns don't count");
        db.add_catch(t0, "fish", "todo", "common", "claude-code", "a");
        db.record_bug_kill(t0, false);
        db.record_bug_kill(t0, true);
        let r = db.report(&day_of(t0));
        assert_eq!((r.turns, r.clean, r.errors), (3, 1, 1));
        assert_eq!(r.work_ms, 60_000 + 120_000 + 10_000 + 5_000);
        assert_eq!((r.bugs, r.bosses), (1, 1));
        assert_eq!(r.catches.len(), 1);
    }

    #[test]
    fn the_basket_empties_one_fish_at_a_time() {
        let db = Db::in_memory();
        let t0 = 1_793_600_000_000;
        db.add_catch(t0, "fish", "todo", "common", "claude-code", "a");
        db.add_catch(t0 + 1, "boot", "", "", "claude-code", "a");
        db.add_catch(t0 + 2, "fish", "regex", "rare", "codex", "c");
        assert_eq!(db.fish_left(), 2, "a boot is not a fish");
        assert!(db.eat_fish() && db.eat_fish());
        assert!(!db.eat_fish(), "empty");
        assert_eq!(db.fish_left(), 0);
        assert_eq!(
            db.fish_counts().len(),
            2,
            "eaten fish stay in the collection"
        );
    }

    #[test]
    fn a_database_from_before_still_takes_bug_kills() {
        let db = Db::in_memory();
        db.with(|c| c.execute_batch("DROP TABLE bug_kills; CREATE TABLE bug_kills (id INTEGER PRIMARY KEY, ts INTEGER NOT NULL, day TEXT NOT NULL, boss INTEGER NOT NULL, coins INTEGER NOT NULL);")).unwrap();
        let t0 = 1_793_600_000_000;
        db.record_bug_kill(t0, true);
        assert_eq!(db.report(&day_of(t0)).bosses, 1);
    }
}
