//! Spring Festival (Lunar New Year): the countdown date and the red-envelope
//! game ("抢红包"). The lunar calendar has no simple formula, so the dates are
//! a table (正月初一 of each year).

use std::path::{Path, PathBuf};

use chrono::{Datelike, Duration, NaiveDate};
use serde::Serialize;
use serde_json::json;

use crate::db::{day_of, Db};

/// (year, month, day of 正月初一, zodiac zh, zodiac en)
const NEW_YEARS: &[(i32, u32, u32, &str, &str)] = &[
    (2025, 1, 29, "蛇", "Snake"),
    (2026, 2, 17, "马", "Horse"),
    (2027, 2, 6, "羊", "Goat"),
    (2028, 1, 26, "猴", "Monkey"),
    (2029, 2, 13, "鸡", "Rooster"),
    (2030, 2, 3, "狗", "Dog"),
    (2031, 1, 23, "猪", "Pig"),
    (2032, 2, 11, "鼠", "Rat"),
    (2033, 1, 31, "牛", "Ox"),
    (2034, 2, 19, "虎", "Tiger"),
    (2035, 2, 8, "兔", "Rabbit"),
    (2036, 1, 28, "龙", "Dragon"),
    (2037, 2, 15, "蛇", "Snake"),
    (2038, 2, 4, "马", "Horse"),
    (2039, 1, 24, "羊", "Goat"),
    (2040, 2, 12, "猴", "Monkey"),
];

/// The festival lasts until 初七; until then "next" is still this year's.
const FESTIVAL_DAYS: i64 = 7;
/// Rounds per day that pay coins (more rounds are just for fun).
pub const PAID_ROUNDS: u32 = 3;
/// Coins per round at most.
pub const ROUND_MAX: i64 = 15;
/// A round lasts 30 s; reject payouts that come faster than that.
const ROUND_GAP_MS: u64 = 25_000;
/// A 30 s round cannot honestly score more than this.
const SCORE_MAX: u32 = 400;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct NewYear {
    /// 正月初一, "YYYY-MM-DD"
    pub date: String,
    /// 除夕 (the day before)
    pub eve: String,
    pub year: i32,
    pub zodiac: String,
    pub zodiac_en: String,
}

fn ymd(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

/// The coming Spring Festival, or the one going on (through 初七).
pub fn next_new_year(today: NaiveDate) -> Option<NewYear> {
    NEW_YEARS.iter().find_map(|&(y, m, d, zh, en)| {
        let date = NaiveDate::from_ymd_opt(y, m, d)?;
        (today < date + Duration::days(FESTIVAL_DAYS)).then(|| NewYear {
            date: ymd(date),
            eve: ymd(date - Duration::days(1)),
            year: date.year(),
            zodiac: zh.into(),
            zodiac_en: en.into(),
        })
    })
}

pub fn today(now: u64) -> NaiveDate {
    NaiveDate::parse_from_str(&day_of(now), "%Y-%m-%d").unwrap_or_default()
}

/// Which festive day `today` is, if any: 除夕 or 初一.
pub fn festive_day(today: NaiveDate) -> Option<&'static str> {
    let ny = next_new_year(today)?;
    let date = NaiveDate::parse_from_str(&ny.date, "%Y-%m-%d").ok()?;
    if today == date - Duration::days(1) {
        Some("eve")
    } else if today == date {
        Some("day")
    } else {
        None
    }
}

fn rounds_key(day: &str) -> String {
    format!("hongbao:{day}")
}

pub fn rounds_today(db: &Db, now: u64) -> u32 {
    db.kv_get(&rounds_key(&day_of(now))).and_then(|v| v.parse().ok()).unwrap_or(0)
}

/// Countdown info for the page.
pub fn info(db: &Db, now: u64, home: &Path) -> serde_json::Value {
    let rounds = rounds_today(db, now);
    let music = music_file(home).map(|f| json!({ "name": music_name(home, &f), "picked": is_picked(&f), "lyrics": lyrics_file(home, &f).is_some() }));
    json!({
        "music": music,
        "next": next_new_year(today(now)),
        "today": day_of(now),
        "rounds": rounds,
        "paid_rounds": PAID_ROUNDS,
        "round_max": ROUND_MAX,
    })
}

#[derive(Debug, Serialize, PartialEq)]
pub struct Payout {
    pub coins: i64,
    /// Paid rounds left today.
    pub left: u32,
}

/// Coins for a finished round of 抢红包: 1 per 10 points, up to `ROUND_MAX`,
/// for the first `PAID_ROUNDS` rounds a day. Daily income cap still applies.
pub fn payout(db: &Db, now: u64, score: u32, daily_cap: i64) -> Result<Payout, String> {
    let last: u64 = db.kv_get("hongbao_at").and_then(|v| v.parse().ok()).unwrap_or(0);
    if now.saturating_sub(last) < ROUND_GAP_MS {
        return Err(crate::tr!("这一局还没结束呢", "This round isn't over yet"));
    }
    db.kv_set("hongbao_at", &now.to_string());
    let day = day_of(now);
    let rounds = rounds_today(db, now) + 1;
    db.kv_set(&rounds_key(&day), &rounds.to_string());
    let left = PAID_ROUNDS.saturating_sub(rounds);
    if rounds > PAID_ROUNDS {
        return Ok(Payout { coins: 0, left: 0 });
    }
    let want = (score.min(SCORE_MAX) as i64 / 10).min(ROUND_MAX);
    let coins = db.earn(want, "hongbao", "", now, daily_cap);
    Ok(Payout { coins, left })
}

// ---- background music: the user's own file in <home>/music

const AUDIO: &[&str] = &["mp3", "m4a", "aac", "ogg", "oga", "wav", "flac", "webm"];

pub fn music_dir(home: &Path) -> PathBuf {
    home.join("music")
}

fn is_audio(p: &Path) -> bool {
    p.extension().and_then(|e| e.to_str()).is_some_and(|e| AUDIO.contains(&e.to_ascii_lowercase().as_str()))
}

fn is_picked(p: &Path) -> bool {
    p.file_stem().and_then(|s| s.to_str()).is_some_and(|s| s.eq_ignore_ascii_case("spring"))
}

/// The song for the Spring Festival page: a file the user put in
/// `<home>/music` (or picked on the page, saved as `spring.<ext>`).
/// `spring.*` wins, then a name with 恭喜发财 in it, then any audio file.
pub fn music_file(home: &Path) -> Option<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(music_dir(home)).ok()?.flatten().map(|e| e.path()).filter(|p| p.is_file() && is_audio(p)).collect();
    files.sort();
    let stem = |p: &PathBuf| p.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
    files
        .iter()
        .find(|p| is_picked(p))
        .or_else(|| files.iter().find(|p| stem(p).contains("恭喜发财")))
        .or_else(|| files.first())
        .cloned()
}

/// Display name of the song; a picked file's original name is kept in spring.txt.
pub fn music_name(home: &Path, file: &Path) -> String {
    if is_picked(file) {
        if let Ok(n) = std::fs::read_to_string(music_dir(home).join("spring.txt")) {
            if !n.trim().is_empty() {
                return n.trim().to_string();
            }
        }
    }
    file.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string()
}

/// Lyrics (.lrc) for the song: same name as the song, or, when the song is
/// 恭喜发财, any .lrc with 恭喜发财 in its name.
pub fn lyrics_file(home: &Path, song: &Path) -> Option<PathBuf> {
    let same = song.with_extension("lrc");
    if same.is_file() {
        return Some(same);
    }
    let name = music_name(home, song);
    if !name.contains("恭喜发财") {
        return None;
    }
    let mut lrcs: Vec<PathBuf> = std::fs::read_dir(music_dir(home))
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("lrc")))
        .filter(|p| p.file_stem().and_then(|s| s.to_str()).is_some_and(|s| s.contains("恭喜发财")))
        .collect();
    lrcs.sort();
    lrcs.into_iter().next()
}

pub fn content_type(p: &Path) -> &'static str {
    match p.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase().as_str() {
        "mp3" => "audio/mpeg",
        "m4a" | "aac" => "audio/mp4",
        "ogg" | "oga" => "audio/ogg",
        "wav" => "audio/wav",
        "flac" => "audio/flac",
        "webm" => "audio/webm",
        _ => "application/octet-stream",
    }
}

/// Save a song picked on the page as `<home>/music/spring.<ext>`.
pub fn save_music(home: &Path, name: &str, ext: &str, bytes: &[u8]) -> Result<(), String> {
    let ext = ext.to_ascii_lowercase();
    if !AUDIO.contains(&ext.as_str()) {
        return Err(crate::tr!("不支持这种音频格式", "Unsupported audio format"));
    }
    remove_music(home);
    let dir = music_dir(home);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::write(dir.join(format!("spring.{ext}")), bytes).map_err(|e| e.to_string())?;
    let name: String = name.chars().filter(|c| !c.is_control()).take(80).collect();
    let _ = std::fs::write(dir.join("spring.txt"), name);
    Ok(())
}

/// Remove the picked song (`spring.*`); files the user put there by hand stay.
pub fn remove_music(home: &Path) {
    if let Ok(rd) = std::fs::read_dir(music_dir(home)) {
        for p in rd.flatten().map(|e| e.path()) {
            if is_picked(&p) {
                let _ = std::fs::remove_file(p);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn next_new_year_rolls_over_after_the_festival() {
        assert_eq!(next_new_year(d("2026-11-04")).unwrap().date, "2027-02-06");
        assert_eq!(next_new_year(d("2026-11-04")).unwrap().eve, "2027-02-05");
        assert_eq!(next_new_year(d("2026-11-04")).unwrap().zodiac, "羊");
        // During the festival it stays on this year's, then moves on.
        assert_eq!(next_new_year(d("2027-02-12")).unwrap().date, "2027-02-06");
        assert_eq!(next_new_year(d("2027-02-13")).unwrap().date, "2028-01-26");
        assert!(next_new_year(d("2041-01-01")).is_none());
        assert_eq!(festive_day(d("2027-02-05")), Some("eve"));
        assert_eq!(festive_day(d("2027-02-06")), Some("day"));
        assert_eq!(festive_day(d("2027-02-07")), None);
    }

    #[test]
    fn payout_is_capped_per_round_and_per_day() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(&dir.path().join("t.db")).unwrap();
        let t0 = 1_800_000_000_000u64;
        assert_eq!(payout(&db, t0, 87, 300).unwrap(), Payout { coins: 8, left: 2 });
        // Too soon: the round can't be over.
        assert!(payout(&db, t0 + 5_000, 50, 300).is_err());
        assert_eq!(payout(&db, t0 + 30_000, 9999, 300).unwrap(), Payout { coins: ROUND_MAX, left: 1 });
        assert_eq!(payout(&db, t0 + 60_000, 30, 300).unwrap(), Payout { coins: 3, left: 0 });
        // Fourth round of the day: for fun only.
        assert_eq!(payout(&db, t0 + 90_000, 200, 300).unwrap(), Payout { coins: 0, left: 0 });
        assert_eq!(rounds_today(&db, t0 + 90_000), 4);
    }

    #[test]
    fn music_prefers_the_picked_song_then_gongxi_facai() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        assert!(music_file(home).is_none());
        std::fs::create_dir_all(music_dir(home)).unwrap();
        std::fs::write(music_dir(home).join("a.mp3"), b"x").unwrap();
        std::fs::write(music_dir(home).join("notes.txt"), b"x").unwrap();
        std::fs::write(music_dir(home).join("刘德华 - 恭喜发财.mp3"), b"x").unwrap();
        assert_eq!(music_name(home, &music_file(home).unwrap()), "刘德华 - 恭喜发财");
        save_music(home, "我的歌", "M4A", b"y").unwrap();
        let f = music_file(home).unwrap();
        assert_eq!(f.file_name().unwrap(), "spring.m4a");
        assert_eq!(music_name(home, &f), "我的歌");
        assert!(save_music(home, "x", "exe", b"z").is_err());
        remove_music(home);
        let song = music_file(home).unwrap();
        assert_eq!(music_name(home, &song), "刘德华 - 恭喜发财");
        // Lyrics: same name first, else any 恭喜发财 .lrc.
        assert!(lyrics_file(home, &song).is_none());
        std::fs::write(music_dir(home).join("恭喜发财 (live).lrc"), b"[00:01.00]x").unwrap();
        assert_eq!(lyrics_file(home, &song).unwrap().file_name().unwrap(), "恭喜发财 (live).lrc");
        std::fs::write(song.with_extension("lrc"), b"[00:01.00]y").unwrap();
        assert_eq!(lyrics_file(home, &song).unwrap(), song.with_extension("lrc"));
        assert!(lyrics_file(home, &music_dir(home).join("a.mp3")).is_none());
    }
}
