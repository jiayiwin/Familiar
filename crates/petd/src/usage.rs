//! Claude Code usage limits (the 5-hour and the weekly window), as Claude
//! Code's own status line reports them. The emitter's `--statusline` mode
//! passes the numbers on; nothing is read from the account or the network.

use crate::codex::{judge, Limits, Window};
use serde_json::{json, Value};
use std::sync::{Mutex, OnceLock};

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

fn window(v: Option<&Value>, minutes: u64) -> Option<Window> {
    let v = v?;
    Some(Window {
        used_percent: v.get("used_percentage")?.as_f64()?,
        window_minutes: minutes,
        resets_at: v.get("resets_at").and_then(Value::as_u64).unwrap_or(0),
    })
}

/// `rate_limits` from the status line input. Either window may be missing.
pub fn parse(limits: &Value) -> Option<Limits> {
    let l = Limits {
        primary: window(limits.get("five_hour"), 300),
        secondary: window(limits.get("seven_day"), 10_080),
        plan: None,
    };
    (l.primary.is_some() || l.secondary.is_some()).then_some(l)
}

/// Latest reading, without the windows that have reset since.
pub fn current(now_s: u64) -> Option<Limits> {
    let mut l = state().lock().unwrap().last.clone()?;
    let live = |w: Option<Window>| w.filter(|w| w.resets_at == 0 || w.resets_at > now_s);
    l.primary = live(l.primary);
    l.secondary = live(l.secondary);
    (l.primary.is_some() || l.secondary.is_some()).then_some(l)
}

/// A new reading: the WS messages for the Claude Code pet, if it has something to say.
pub fn update(limits: &Value, pet: Option<&str>, now_ms: u64) -> Vec<String> {
    let Some(cur) = parse(limits) else {
        return vec![];
    };
    let mut st = state().lock().unwrap();
    let mut out = vec![];
    if let Some(pet) = pet {
        if st
            .last
            .as_ref()
            .is_some_and(|prev| *prev != cur && judge(prev, &cur, now_ms / 1000).is_some())
        {
            out.push(crate::msg(
                "usage",
                &json!({ "pet": pet, "kind": "refresh", "used": cur.used() }),
            ));
        }
        if cur.used() >= 90.0 && now_ms.saturating_sub(st.warned_at) >= 2 * 3_600_000 {
            st.warned_at = now_ms;
            let five = cur.primary.as_ref().is_some_and(|w| w.used_percent >= 90.0);
            out.push(crate::msg("usage", &json!({ "pet": pet, "kind": "low", "used": cur.used(), "window": if five { "five_hour" } else { "seven_day" } })));
        }
    }
    st.last = Some(cur);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_line_limits_become_windows_and_a_warning() {
        assert!(parse(&json!({})).is_none());
        let l = parse(&json!({ "five_hour": { "used_percentage": 23.5, "resets_at": 1_800_000_000u64 }, "spend_limit": { "used_percentage": 5 } })).unwrap();
        assert_eq!(
            (
                l.primary.as_ref().unwrap().window_minutes,
                l.primary.as_ref().unwrap().used_percent
            ),
            (300, 23.5)
        );
        assert!(l.secondary.is_none());

        let now = 1_793_600_000_000u64;
        let at = now / 1000 + 3600;
        let reading = |five: f64, week: f64| json!({ "five_hour": { "used_percentage": five, "resets_at": at }, "seven_day": { "used_percentage": week, "resets_at": at + 86_400 } });
        assert!(update(&reading(40.0, 10.0), Some("claude-code"), now).is_empty());
        let low = update(&reading(93.0, 10.0), Some("claude-code"), now + 1000);
        assert_eq!(low.len(), 1);
        assert!(
            low[0].contains("\"low\"") && low[0].contains("five_hour"),
            "{}",
            low[0]
        );
        assert!(
            update(&reading(95.0, 10.0), Some("claude-code"), now + 2000).is_empty(),
            "warned once in two hours"
        );
        // The window ran out and came back empty: the pet notices.
        let back = update(&reading(2.0, 12.0), Some("claude-code"), now + 3000);
        assert!(
            back.len() == 1 && back[0].contains("\"refresh\""),
            "{back:?}"
        );
        assert_eq!(
            current(now / 1000).unwrap().primary.unwrap().used_percent,
            2.0
        );
        // Past its reset time a window is no longer shown.
        let later = current(at + 10).unwrap();
        assert!(later.primary.is_none() && later.secondary.is_some());
    }
}
