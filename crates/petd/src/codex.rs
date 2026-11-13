//! Codex usage limits, read from Codex's own session logs (read-only).
//!
//! Codex writes a `token_count` event with `rate_limits` (used %, window,
//! reset time) into `$CODEX_HOME/sessions/YYYY/MM/DD/rollout-*.jsonl`. We
//! check the newest logs once a minute so the Codex pet can say when the
//! quota runs low and celebrate when a reset card (banked reset) is used.
//! Using the card itself is an account action in Codex (`/usage`, or
//! Settings → Usage); Rill never does it.

use serde::Serialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Window {
    pub used_percent: f64,
    pub window_minutes: u64,
    /// Unix seconds.
    pub resets_at: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Limits {
    pub primary: Option<Window>,
    pub secondary: Option<Window>,
    pub plan: Option<String>,
}

impl Limits {
    pub fn used(&self) -> f64 {
        [&self.primary, &self.secondary]
            .iter()
            .filter_map(|w| w.as_ref().map(|w| w.used_percent))
            .fold(0.0, f64::max)
    }
}

/// "5 小时" / "每周" / "每月" for a window length.
pub fn window_name(minutes: u64) -> String {
    match minutes {
        300 => crate::tr!("5 小时", "5-hour"),
        10080 => crate::tr!("每周", "Weekly"),
        43200 => crate::tr!("每月", "Monthly"),
        m if m % 1440 == 0 => crate::tr!("{} 天", "{}-day", m / 1440),
        m if m % 60 == 0 => crate::tr!("{} 小时", "{}-hour", m / 60),
        m => crate::tr!("{m} 分钟", "{m}-minute"),
    }
}

/// "2 小时后" / "3 天后" until a reset.
pub fn until_text(resets_at: u64, now_s: u64) -> String {
    let s = resets_at.saturating_sub(now_s);
    if s >= 86_400 {
        crate::tr!("{} 天后", "in {}d", s / 86_400)
    } else if s >= 3600 {
        crate::tr!("{} 小时后", "in {}h", s / 3600)
    } else {
        crate::tr!("{} 分钟后", "in {}m", (s / 60).max(1))
    }
}

/// Codex config folders on this machine: `$CODEX_HOME`, then `~/.codex`.
pub fn homes() -> Vec<PathBuf> {
    let mut out = vec![];
    if let Some(h) = std::env::var_os("CODEX_HOME").filter(|h| !h.is_empty()) {
        out.push(PathBuf::from(h));
    }
    if let Some(home) = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")) {
        let p = PathBuf::from(home).join(".codex");
        if !out.contains(&p) {
            out.push(p);
        }
    }
    out
}

fn window(v: &Value) -> Option<Window> {
    Some(Window {
        used_percent: v.get("used_percent")?.as_f64()?,
        window_minutes: v.get("window_minutes").and_then(Value::as_u64).unwrap_or(0),
        resets_at: v.get("resets_at").and_then(Value::as_u64).unwrap_or(0),
    })
}

/// The last rate-limit record with numbers in one log line.
pub fn parse_line(line: &str) -> Option<Limits> {
    if !line.contains("\"rate_limits\"") || !line.contains("\"used_percent\"") {
        return None;
    }
    let v: Value = serde_json::from_str(line).ok()?;
    let r = v.get("payload")?.get("rate_limits")?;
    let l = Limits {
        primary: r.get("primary").and_then(window),
        secondary: r.get("secondary").and_then(window),
        plan: r.get("plan_type").and_then(Value::as_str).map(String::from),
    };
    (l.primary.is_some() || l.secondary.is_some()).then_some(l)
}

fn last_in_file(p: &Path) -> Option<Limits> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(p).ok()?;
    let len = f.metadata().ok()?.len();
    let start = len.saturating_sub(1 << 20);
    f.seek(SeekFrom::Start(start)).ok()?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf).ok()?;
    let text = String::from_utf8_lossy(&buf);
    text.lines().rev().find_map(parse_line)
}

/// Newest limits across the Codex homes (logs touched in the last 8 days).
pub fn read_latest() -> Option<(Limits, SystemTime)> {
    let cutoff = SystemTime::now() - Duration::from_secs(8 * 86_400);
    let mut files: Vec<(SystemTime, PathBuf)> = vec![];
    for home in homes() {
        let root = home.join("sessions");
        // sessions/YYYY/MM/DD: only walk the newest few day folders.
        let mut days: Vec<PathBuf> = vec![];
        for y in dirs(&root).into_iter().rev().take(2) {
            for m in dirs(&y).into_iter().rev().take(2) {
                days.extend(dirs(&m).into_iter().rev().take(8));
            }
        }
        for d in days {
            for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
                let p = e.path();
                if p.extension().and_then(|x| x.to_str()) != Some("jsonl") {
                    continue;
                }
                if let Ok(t) = e.metadata().and_then(|m| m.modified()) {
                    if t >= cutoff {
                        files.push((t, p));
                    }
                }
            }
        }
    }
    files.sort_by(|a, b| b.0.cmp(&a.0));
    files
        .into_iter()
        .take(12)
        .find_map(|(t, p)| last_in_file(&p).map(|l| (l, t)))
}

fn dirs(p: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(p)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    v.sort();
    v
}

/// What changed between two readings.
#[derive(Debug, PartialEq)]
pub enum Change {
    /// A reset card was used: a window dropped (or its reset date moved)
    /// before its natural reset time.
    Card,
    /// A window reset on schedule after real use.
    Refresh,
}

pub fn judge(prev: &Limits, cur: &Limits, now_s: u64) -> Option<Change> {
    let mut refreshed = false;
    for (p, c) in [
        (&prev.primary, &cur.primary),
        (&prev.secondary, &cur.secondary),
    ] {
        let (Some(p), Some(c)) = (p, c) else { continue };
        let dropped = p.used_percent - c.used_percent >= 20.0;
        let early = p.resets_at > now_s + 60;
        if dropped && early {
            return Some(Change::Card);
        }
        if dropped && p.used_percent >= 50.0 {
            refreshed = true;
        }
    }
    refreshed.then_some(Change::Refresh)
}

struct State {
    last: Option<Limits>,
    warned_at: u64,
}

fn state() -> &'static Mutex<State> {
    static S: OnceLock<Mutex<State>> = OnceLock::new();
    S.get_or_init(|| {
        Mutex::new(State {
            last: None,
            warned_at: 0,
        })
    })
}

/// Latest reading (for the menu).
pub fn current() -> Option<Limits> {
    state().lock().unwrap().last.clone()
}

/// One check: returns the WS messages for the Codex pet, if any.
pub fn poll(codex_pet: Option<&str>, now_ms: u64) -> Vec<String> {
    let Some((cur, _)) = read_latest() else {
        return vec![];
    };
    let now_s = now_ms / 1000;
    let mut st = state().lock().unwrap();
    let mut out = vec![];
    let Some(pet) = codex_pet else {
        st.last = Some(cur);
        return out;
    };
    if let Some(prev) = st.last.as_ref() {
        if *prev != cur {
            match judge(prev, &cur, now_s) {
                Some(Change::Card) => out.push(crate::msg("codex.limits", &json!({ "pet": pet, "kind": "card", "from": prev.used(), "used": cur.used() }))),
                Some(Change::Refresh) => out.push(crate::msg("codex.limits", &json!({ "pet": pet, "kind": "refresh", "from": prev.used(), "used": cur.used() }))),
                None => {}
            }
        }
    }
    if cur.used() >= 90.0 && now_ms.saturating_sub(st.warned_at) >= 2 * 3_600_000 {
        st.warned_at = now_ms;
        out.push(crate::msg(
            "codex.limits",
            &json!({ "pet": pet, "kind": "low", "used": cur.used() }),
        ));
    }
    st.last = Some(cur);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn l(p: f64, p_reset: u64, s: f64, s_reset: u64) -> Limits {
        Limits {
            primary: Some(Window {
                used_percent: p,
                window_minutes: 300,
                resets_at: p_reset,
            }),
            secondary: Some(Window {
                used_percent: s,
                window_minutes: 10080,
                resets_at: s_reset,
            }),
            plan: None,
        }
    }

    #[test]
    fn parses_a_token_count_line() {
        let line = r#"{"timestamp":"2026-05-07T08:03:15.481Z","type":"event_msg","payload":{"type":"token_count","rate_limits":{"limit_id":"codex","primary":{"used_percent":81.0,"window_minutes":300,"resets_at":1778153543},"secondary":null,"plan_type":"pro"}}}"#;
        let got = parse_line(line).unwrap();
        assert_eq!(got.primary.unwrap().used_percent, 81.0);
        assert!(got.secondary.is_none());
        assert!(
            parse_line(r#"{"payload":{"rate_limits":{"primary":null,"secondary":null}}}"#)
                .is_none()
        );
    }

    #[test]
    fn tells_a_reset_card_from_a_scheduled_reset() {
        let now = 1_000_000;
        // Weekly quota drops days before its reset: a card.
        assert_eq!(
            judge(
                &l(30.0, now + 100, 95.0, now + 3 * 86_400),
                &l(0.0, now + 18_000, 0.0, now + 7 * 86_400),
                now
            ),
            Some(Change::Card)
        );
        // 5-hour window reset on time after heavy use: a refresh.
        assert_eq!(
            judge(
                &l(80.0, now - 10, 40.0, now + 86_400),
                &l(0.0, now + 18_000, 40.0, now + 86_400),
                now
            ),
            Some(Change::Refresh)
        );
        // Small drift: nothing.
        assert_eq!(
            judge(
                &l(30.0, now + 100, 40.0, now + 86_400),
                &l(25.0, now + 100, 40.0, now + 86_400),
                now
            ),
            None
        );
        assert_eq!(window_name(10080), "每周");
    }
}
