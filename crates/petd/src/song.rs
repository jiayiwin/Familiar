//! 收工曲: a day's turns, which the pets play back as a piece (the music is
//! written in the page, app/src/song). Here: the turns of a day, and the offer
//! to play today's, made once in the half hour before off time.

use serde::Serialize;

/// One turn as the piece reads it.
#[derive(Debug, Serialize, PartialEq)]
pub struct Turn {
    pub source: String,
    pub started_at: u64,
    pub ended_at: u64,
    pub result: String,
    pub tool_calls: u32,
    pub tool_errors: u32,
}

/// Fewer turns than this and there is nothing to play.
pub const MIN_TURNS: usize = 6;

/// In the half hour before off time ("HH:MM"), at `h`:`m`.
fn before_off(off: &str, h: u32, m: u32) -> bool {
    let (oh, om) = off
        .split_once(':')
        .and_then(|(a, b)| Some((a.trim().parse::<u32>().ok()?, b.trim().parse::<u32>().ok()?)))
        .unwrap_or((18, 0));
    let (now, off) = (h * 60 + m, oh * 60 + om);
    now + 30 >= off && now < off
}

/// Once a day, while you are at the desk before off time: a pet asks to play today's.
pub fn tick(sh: &crate::server::Shared, active: bool) {
    use chrono::Timelike;
    if !active {
        return;
    }
    let now = crate::now_ms();
    let today = crate::db::day_of(now);
    if sh.db.kv_get("song:asked").as_deref() == Some(today.as_str()) {
        return;
    }
    let off = sh.world.lock().unwrap().t.balance.work.off_time.clone();
    let t = chrono::Local::now();
    if !before_off(&off, t.hour(), t.minute()) || sh.db.day_turns(&today).len() < MIN_TURNS {
        return;
    }
    let Some((pet, who)) = crate::drawer::asker(sh) else { return };
    sh.db.kv_set("song:asked", &today);
    let text = crate::voice::line(who, "offer_song", 0);
    let _ = sh.ws_tx.send(crate::msg("pet.act", &serde_json::json!({ "pet": pet, "act": "offer", "text": text, "why": "song", "offer": "song" })));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_asks_only_in_the_half_hour_before_off_time() {
        assert!(before_off("18:00", 17, 30));
        assert!(before_off("18:00", 17, 59));
        assert!(!before_off("18:00", 18, 0));
        assert!(!before_off("18:00", 17, 29));
        assert!(before_off("19:30", 19, 10));
        assert!(before_off("nonsense", 17, 45), "an unreadable off time means 18:00");
    }

    #[test]
    fn a_days_turns_come_back_in_order() {
        let db = crate::db::Db::open(std::path::Path::new(":memory:")).unwrap();
        let day = crate::db::day_of(crate::now_ms());
        assert!(db.day_turns(&day).is_empty());
        db.add_turn_for_test(&day, "codex", 2_000, 3_000, "ok", 4, 0);
        db.add_turn_for_test(&day, "claude-code", 1_000, 1_500, "error", 2, 1);
        let got = db.day_turns(&day);
        assert_eq!(got.iter().map(|t| t.source.as_str()).collect::<Vec<_>>(), ["claude-code", "codex"]);
        assert_eq!((got[0].result.as_str(), got[0].tool_errors), ("error", 1));
    }
}
