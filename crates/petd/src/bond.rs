use crate::db::Db;

pub enum BondReward {
    NoFish,
    Whisper(String),
    Secret(String),
}

/// The feeds so far. Counted per pet pack before there was one pet; the
/// highest of those counts carries over, so nobody starts again from nothing.
fn feeds(db: &Db) -> i64 {
    const KEY: &str = "bond:feeds:zimeng";
    if let Some(n) = db.kv_get(KEY).and_then(|v| v.parse().ok()) {
        return n;
    }
    db.with(|c| {
        c.query_row(
            "SELECT COALESCE(MAX(CAST(value AS INTEGER)), 0) FROM kv WHERE key LIKE 'bond:feeds:%'",
            [],
            |r| r.get(0),
        )
    })
    .unwrap_or(0)
}

static BOND_YAML: &str = include_str!("../assets/bond.yaml");

struct BondContent {
    whispers: Vec<String>,
    secrets: Vec<String>,
}

/// What `who` says when fed. A pet with lines of its own (`<id>_whispers_zh`,
/// …) uses them; the others speak as 紫梦, whose lines carry no prefix.
fn load(en: bool, who: &str) -> BondContent {
    let v: serde_yaml::Value = serde_yaml::from_str(BOND_YAML).unwrap_or(serde_yaml::Value::Null);
    let lang = if en { "en" } else { "zh" };
    let list = |k: &str| -> Vec<String> {
        v.get(k)
            .and_then(|x| x.as_sequence())
            .map(|s| {
                s.iter()
                    .filter_map(|e| e.as_str().map(str::trim).map(String::from))
                    .collect()
            })
            .unwrap_or_default()
    };
    let own = |kind: &str| {
        let mine = list(&format!("{who}_{kind}_{lang}"));
        if mine.is_empty() {
            list(&format!("{kind}_{lang}"))
        } else {
            mine
        }
    };
    BondContent {
        whispers: own("whispers"),
        secrets: own("secrets"),
    }
}

/// One fish fed to `who` (`zimeng`, `mashu`, …): what it says, what it earned
/// you, and the count so far. The count is one, whoever eats.
pub fn on_feed(db: &Db, en: bool, who: &str) -> (String, BondReward, i64) {
    let count = feeds(db) + 1;
    db.kv_set("bond:feeds:zimeng", &count.to_string());

    let content = load(en, who);

    if count % 5 == 0 && !content.secrets.is_empty() {
        let idx = ((count / 5) as usize - 1) % content.secrets.len();
        let text = content.secrets.get(idx).cloned().unwrap_or_default();
        return (text.clone(), BondReward::Secret(text), count);
    }

    if !content.whispers.is_empty() {
        let idx = (count as usize - 1) % content.whispers.len();
        let text = content.whispers.get(idx).cloned().unwrap_or_default();
        return (text.clone(), BondReward::Whisper(text), count);
    }

    (
        crate::i18n::t("……嗯。", "…mm.").to_string(),
        BondReward::Whisper(String::new()),
        count,
    )
}

/// Time of day and season in words, so the letter can be about today.
pub fn moment(en: bool) -> String {
    use chrono::{Datelike, Local, Timelike};
    let now = Local::now();
    let h = now.hour();
    let (tod_zh, tod_en) = match h {
        5..=8 => ("清晨", "early morning"),
        9..=11 => ("上午", "morning"),
        12..=13 => ("中午", "midday"),
        14..=17 => ("下午", "afternoon"),
        18..=20 => ("傍晚", "evening"),
        21..=23 => ("深夜", "late at night"),
        _ => ("凌晨", "the small hours"),
    };
    let (sea_zh, sea_en) = match now.month() {
        3..=5 => ("春天", "spring"),
        6..=8 => ("夏天", "summer"),
        9..=11 => ("秋天", "autumn"),
        _ => ("冬天", "winter"),
    };
    let (wd_zh, wd_en) = match now.weekday().num_days_from_monday() {
        5 | 6 => ("周末", "a weekend"),
        4 => ("周五", "a Friday"),
        0 => ("周一", "a Monday"),
        _ => ("工作日", "a weekday"),
    };
    if en {
        format!("It is {tod_en}, {wd_en}, {sea_en}.")
    } else {
        format!("现在是{sea_zh}的一个{wd_zh}，{tod_zh}。")
    }
}
