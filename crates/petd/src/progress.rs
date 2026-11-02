//! Long-term progress (design §7.5, §7.3 "周赛季", §8): achievements, the
//! cosmetics shop and weekly seasons. Everything lives in the local SQLite
//! file; coins come from the wallet log.

use crate::db::{day_of, Db};
use chrono::{Datelike, Duration as CDuration, Local, NaiveDate, TimeZone, Timelike};
use rusqlite::{params, OptionalExtension};
use serde::Serialize;

// ------------------------------------------------------------ achievements

/// `name` / `desc` are the Chinese text; `label()` / `about()` (and the
/// serialized `name` / `desc`) follow the current language.
#[derive(Debug, Clone, Copy)]
pub struct Achievement {
    pub id: &'static str,
    pub name: &'static str,
    pub desc: &'static str,
    pub coins: i64,
    pub name_en: &'static str,
    pub desc_en: &'static str,
}

impl Achievement {
    pub fn label(&self) -> &'static str {
        crate::i18n::t(self.name, self.name_en)
    }
    pub fn about(&self) -> &'static str {
        crate::i18n::t(self.desc, self.desc_en)
    }
}

impl Serialize for Achievement {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut st = s.serialize_struct("Achievement", 4)?;
        st.serialize_field("id", self.id)?;
        st.serialize_field("name", self.label())?;
        st.serialize_field("desc", self.about())?;
        st.serialize_field("coins", &self.coins)?;
        st.end()
    }
}

macro_rules! ach {
    ($id:expr, $name:expr, $desc:expr, $coins:expr, $name_en:expr, $desc_en:expr) => {
        Achievement { id: $id, name: $name, desc: $desc, coins: $coins, name_en: $name_en, desc_en: $desc_en }
    };
}

/// The first 20 achievements (design §7.5 lists some of them).
pub const ACHIEVEMENTS: &[Achievement] = &[
    ach!("first_turn", "初次合作", "agent 完成第一轮任务", 10, "First Job", "Your agent finished its first turn"),
    ach!("clean_one", "一次通过", "某一轮没有任何工具失败", 10, "Clean Run", "A turn with zero tool failures"),
    ach!("streak10", "十连胜", "连续 10 轮一次通过", 50, "Ten in a Row", "10 clean turns in a row"),
    ach!("turns100", "百轮老手", "累计完成 100 轮", 50, "Century", "100 turns finished"),
    ach!("turns1000", "千锤百炼", "累计完成 1000 轮", 100, "Battle-Hardened", "1000 turns finished"),
    ach!("marathon", "马拉松", "单轮任务超过 30 分钟", 30, "Marathon", "A single turn longer than 30 minutes"),
    ach!("night_owl", "夜猫子", "23 点以后还在完成任务（早点休息）", 20, "Night Owl", "Still shipping after 11 pm (get some sleep)"),
    ach!("duo", "双剑合璧", "Claude Code 和 Codex 同一天都完成了任务", 20, "Dynamic Duo", "Claude Code and Codex both finished work on the same day"),
    ach!("fish1", "第一条鱼", "钓到第一条鱼", 10, "First Catch", "Catch your first fish"),
    ach!("fisher100", "钓鱼佬", "累计钓到 100 条鱼", 50, "Angler", "Catch 100 fish"),
    ach!("legend", "传说猎人", "钓到一条传说鱼", 50, "Legend Hunter", "Catch a legendary fish"),
    ach!("collector", "图鉴全收集", "集齐所有鱼种", 100, "Gotta Catch 'Em All", "Collect every fish species"),
    ach!("bug10", "捉虫新手", "拍死 10 只虫", 10, "Bug Swatter", "Squash 10 bugs"),
    ach!("bug1000", "Bug 克星", "累计拍死 1000 只虫", 100, "Bug Buster", "Squash 1000 bugs"),
    ach!("boss", "屠龙勇士", "打倒一只 Boss 虫", 30, "Dragon Slayer", "Defeat a boss bug"),
    ach!("free_fall", "自由落体", "把宠物甩到墙上撞晕", 10, "Free Fall", "Fling a pet into a wall until it's dizzy"),
    ach!("petter", "撸宠达人", "累计撸毛 50 次", 20, "Pet Whisperer", "Pet your pets 50 times"),
    ach!("tug_fan", "拔河观众", "看了 10 场拔河", 10, "Tug Fan", "Watch 10 tug-of-war matches"),
    ach!("rich", "小富翁", "金币余额达到 500", 30, "Moneybags", "Hold 500 coins"),
    ach!("shopper", "第一次剁手", "在商店买下第一件东西", 10, "Retail Therapy", "Buy your first thing in the shop"),
];

/// Totals the achievements look at.
#[derive(Debug, Default, Clone, Serialize)]
pub struct Totals {
    pub turns: u64,
    pub clean: u64,
    pub best_streak: u64,
    pub longest_ms: u64,
    pub night: bool,
    pub duo: bool,
    pub fish: u64,
    pub legendary: u64,
    pub species: u64,
    pub bugs: u64,
    pub bosses: u64,
    pub dizzy: u64,
    pub petted: u64,
    pub tugs: u64,
    pub balance: i64,
    pub bought: u64,
}

fn unlocked(t: &Totals, id: &str, species_total: u64) -> bool {
    match id {
        "first_turn" => t.turns >= 1,
        "clean_one" => t.clean >= 1,
        "streak10" => t.best_streak >= 10,
        "turns100" => t.turns >= 100,
        "turns1000" => t.turns >= 1000,
        "marathon" => t.longest_ms >= 30 * 60_000,
        "night_owl" => t.night,
        "duo" => t.duo,
        "fish1" => t.fish >= 1,
        "fisher100" => t.fish >= 100,
        "legend" => t.legendary >= 1,
        "collector" => species_total > 0 && t.species >= species_total,
        "bug10" => t.bugs >= 10,
        "bug1000" => t.bugs >= 1000,
        "boss" => t.bosses >= 1,
        "free_fall" => t.dizzy >= 1,
        "petter" => t.petted >= 50,
        "tug_fan" => t.tugs >= 10,
        "rich" => t.balance >= 500,
        "shopper" => t.bought >= 1,
        _ => false,
    }
}

pub const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS achievements (id TEXT PRIMARY KEY, ts INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS inventory (item TEXT PRIMARY KEY, ts INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS equipped (pack TEXT NOT NULL, slot TEXT NOT NULL, item TEXT NOT NULL, PRIMARY KEY (pack, slot));
CREATE TABLE IF NOT EXISTS seasons (
  week TEXT PRIMARY KEY, cc REAL NOT NULL, cx REAL NOT NULL, winner TEXT NOT NULL, ts INTEGER NOT NULL
);
"#;

fn kv_u64(db: &Db, key: &str) -> u64 {
    db.kv_get(key).and_then(|v| v.parse().ok()).unwrap_or(0)
}

/// Bump a counter kept in kv (dizzy hits, tug bouts watched…).
pub fn bump(db: &Db, key: &str) {
    let n = kv_u64(db, key) + 1;
    db.kv_set(key, &n.to_string());
}

/// Track the clean-turn streak after every finished turn.
pub fn on_turn(db: &Db, clean: bool) {
    let cur = if clean { kv_u64(db, "streak") + 1 } else { 0 };
    db.kv_set("streak", &cur.to_string());
    if cur > kv_u64(db, "best_streak") {
        db.kv_set("best_streak", &cur.to_string());
    }
}

pub fn totals(db: &Db) -> Totals {
    db.with(|c| {
        let one = |sql: &str| -> i64 { c.query_row(sql, [], |r| r.get::<_, Option<i64>>(0)).ok().flatten().unwrap_or(0) };
        let turns = one("SELECT COUNT(*) FROM turns WHERE result != 'aborted'") + one("SELECT SUM(turns) FROM daily");
        let clean =
            one("SELECT COUNT(*) FROM turns WHERE result = 'ok' AND tool_errors = 0") + one("SELECT SUM(clean) FROM daily");
        let longest = one("SELECT MAX(ms) FROM turns WHERE result = 'ok'");
        let night = one("SELECT COUNT(*) FROM turns WHERE result = 'ok' AND CAST(strftime('%H', ended_at / 1000, 'unixepoch', 'localtime') AS INTEGER) >= 23") > 0;
        let duo = one("SELECT COUNT(*) FROM (SELECT day FROM turns WHERE result = 'ok' GROUP BY day HAVING COUNT(DISTINCT source) >= 2)") > 0;
        Ok(Totals {
            turns: turns as u64,
            clean: clean as u64,
            longest_ms: longest as u64,
            night,
            duo,
            fish: one("SELECT COUNT(*) FROM catches WHERE outcome IN ('fish', 'eaten', 'stolen')") as u64,
            legendary: one("SELECT COUNT(*) FROM catches WHERE outcome IN ('fish', 'eaten', 'stolen') AND rarity = 'legendary'") as u64,
            species: one("SELECT COUNT(DISTINCT fish_id) FROM catches WHERE outcome IN ('fish', 'eaten', 'stolen')") as u64,
            bugs: one("SELECT COUNT(*) FROM bug_kills WHERE boss = 0") as u64,
            bosses: one("SELECT COUNT(*) FROM bug_kills WHERE boss = 1") as u64,
            petted: one("SELECT SUM(petted) FROM pet_stats") as u64,
            balance: one("SELECT SUM(delta) FROM wallet_log"),
            bought: one("SELECT COUNT(*) FROM inventory") as u64,
            ..Totals::default()
        })
    })
    .map(|mut t| {
        t.best_streak = kv_u64(db, "best_streak");
        t.dizzy = kv_u64(db, "dizzy");
        t.tugs = kv_u64(db, "tugs");
        t
    })
    .unwrap_or_default()
}

#[derive(Debug, Clone, Serialize)]
pub struct AchievementRow {
    #[serde(flatten)]
    pub def: Achievement,
    pub unlocked_at: Option<u64>,
}

pub fn achievements(db: &Db) -> Vec<AchievementRow> {
    let got: std::collections::HashMap<String, u64> = db
        .with(|c| {
            let mut st = c.prepare("SELECT id, ts FROM achievements")?;
            let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u64)))?;
            Ok(rows.flatten().collect())
        })
        .unwrap_or_default();
    ACHIEVEMENTS.iter().map(|a| AchievementRow { def: *a, unlocked_at: got.get(a.id).copied() }).collect()
}

/// Unlock whatever is newly earned; pays each one's coins. Returns the new ones.
pub fn check(db: &Db, now: u64, species_total: u64, daily_cap: i64) -> Vec<(Achievement, i64)> {
    let t = totals(db);
    let have: std::collections::HashSet<String> = achievements(db).into_iter().filter(|r| r.unlocked_at.is_some()).map(|r| r.def.id.to_string()).collect();
    let mut out = vec![];
    for a in ACHIEVEMENTS {
        if have.contains(a.id) || !unlocked(&t, a.id, species_total) {
            continue;
        }
        let inserted = db
            .with(|c| c.execute("INSERT OR IGNORE INTO achievements (id, ts) VALUES (?1, ?2)", params![a.id, now as i64]))
            .unwrap_or(0);
        if inserted > 0 {
            let paid = db.earn(a.coins, "achievement", a.id, now, daily_cap);
            out.push((*a, paid));
        }
    }
    out
}

// ------------------------------------------------------------ shop

/// `name` / `desc` are the Chinese text; `label()` / `about()` (and the
/// serialized `name` / `desc`) follow the current language.
#[derive(Debug, Clone, Copy)]
pub struct Item {
    pub id: &'static str,
    pub name: &'static str,
    /// hat | rope | quips
    pub kind: &'static str,
    pub price: i64,
    pub desc: &'static str,
    pub name_en: &'static str,
    pub desc_en: &'static str,
}

impl Item {
    pub fn label(&self) -> &'static str {
        crate::i18n::t(self.name, self.name_en)
    }
    pub fn about(&self) -> &'static str {
        crate::i18n::t(self.desc, self.desc_en)
    }
}

impl Serialize for Item {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut st = s.serialize_struct("Item", 5)?;
        st.serialize_field("id", self.id)?;
        st.serialize_field("name", self.label())?;
        st.serialize_field("kind", self.kind)?;
        st.serialize_field("price", &self.price)?;
        st.serialize_field("desc", self.about())?;
        st.end()
    }
}

/// Cosmetics only: nothing here changes who wins anything (design §8).
pub const CATALOG: &[Item] = &[
    Item { id: "flower", name: "小红花", kind: "hat", price: 50, desc: "别在头上的一朵小红花", name_en: "Little Flower", desc_en: "A tiny red flower tucked behind the ear" },
    Item { id: "bow", name: "蝴蝶结", kind: "hat", price: 60, desc: "粉色大蝴蝶结", name_en: "Bow", desc_en: "A big pink bow" },
    Item { id: "straw_hat", name: "草帽", kind: "hat", price: 80, desc: "夏天钓鱼专用", name_en: "Straw Hat", desc_en: "For summer fishing trips" },
    Item { id: "party_hat", name: "派对帽", kind: "hat", price: 100, desc: "每天都是庆功宴", name_en: "Party Hat", desc_en: "Every day is a launch party" },
    Item { id: "beanie", name: "毛线帽", kind: "hat", price: 120, desc: "冬天加班也不冷", name_en: "Beanie", desc_en: "Stay warm through winter overtime" },
    Item { id: "wizard_hat", name: "法师帽", kind: "hat", price: 200, desc: "写代码就是施法", name_en: "Wizard Hat", desc_en: "Coding is basically spellcasting" },
    Item { id: "halo", name: "光环", kind: "hat", price: 300, desc: "一次通过的天使", name_en: "Halo", desc_en: "For angels who pass on the first try" },
    Item { id: "rope_rainbow", name: "彩虹拔河绳", kind: "rope", price: 150, desc: "拔河时用彩虹绳", name_en: "Rainbow Rope", desc_en: "A rainbow rope for tug-of-war" },
    Item { id: "rope_gold", name: "金色拔河绳", kind: "rope", price: 250, desc: "拔河时用金绳", name_en: "Golden Rope", desc_en: "A golden rope for tug-of-war" },
    Item { id: "quips_meme", name: "沙雕吐槽包", kind: "quips", price: 120, desc: "干活时多了一批沙雕台词", name_en: "Silly Quip Pack", desc_en: "A batch of goofy lines while working" },
    Item { id: "quips_poet", name: "文艺吐槽包", kind: "quips", price: 120, desc: "干活时多了一批文艺台词", name_en: "Poetic Quip Pack", desc_en: "A batch of poetic lines while working" },
];

pub fn quip_lines(item: &str) -> &'static [&'static str] {
    match item {
        "quips_meme" if crate::i18n::is_en() => &["If it runs, ship it", "That bug is a feature", "Gonna slack off a bit, you carry on", "Another beautiful day (not really)", "Have you tried turning it off and on?"],
        "quips_poet" if crate::i18n::is_en() => &["Between the keystrokes, verses fall", "Compiles like wind, tests like song", "A whole spring hides in {file}", "Waiting for it to think, like waiting for rain", "Life is like code: commit with care"],
        "quips_meme" => &["代码能跑就行，要什么自行车", "这个 bug 是 feature", "我先摸会儿鱼，你们继续", "又是美好的一天（假的）", "重启试试？"],
        "quips_poet" => &["键盘声里，一行行诗落下", "编译如风，测试如歌", "{file} 里藏着一整个春天", "等它想明白，像等一场雨", "人生如代码，提交需谨慎"],
        _ => &[],
    }
}

pub fn item(id: &str) -> Option<&'static Item> {
    CATALOG.iter().find(|i| i.id == id)
}

pub fn owned(db: &Db) -> Vec<String> {
    db.with(|c| {
        let mut st = c.prepare("SELECT item FROM inventory ORDER BY ts")?;
        let rows = st.query_map([], |r| r.get::<_, String>(0))?;
        Ok(rows.flatten().collect())
    })
    .unwrap_or_default()
}

/// Spend coins on an item. Not subject to the daily income cap.
pub fn buy(db: &Db, id: &str, now: u64) -> Result<String, String> {
    let it = item(id).ok_or_else(|| crate::tr!("没有这件商品", "No such item"))?;
    if owned(db).iter().any(|o| o == id) {
        return Err(crate::tr!("已经有「{}」了", "You already have \"{}\"", it.label()));
    }
    if db.balance() < it.price {
        return Err(crate::tr!("金币不够：「{}」要 {}，你有 {}", "Not enough coins: \"{}\" costs {}, you have {}", it.label(), it.price, db.balance()));
    }
    db.with(|c| {
        c.execute(
            "INSERT INTO wallet_log (ts, day, delta, reason, ref) VALUES (?1, ?2, ?3, 'shop', ?4)",
            params![now as i64, day_of(now), -it.price, id],
        )?;
        c.execute("INSERT INTO inventory (item, ts) VALUES (?1, ?2)", params![id, now as i64])
    })
    .map_err(|e| e.to_string())?;
    Ok(crate::tr!("买下了「{}」", "Bought \"{}\"", it.label()))
}

/// Equip (or with `item = None` take off) a cosmetic. Hats are per pack;
/// ropes are global (pack "*").
pub fn equip(db: &Db, pack: &str, slot: &str, item_id: Option<&str>) -> Result<(), String> {
    if let Some(id) = item_id {
        let it = item(id).ok_or_else(|| crate::tr!("没有这件商品", "No such item"))?;
        if it.kind != slot {
            return Err(crate::tr!("这件东西不能戴在这里", "That can't go there"));
        }
        if !owned(db).iter().any(|o| o == id) {
            return Err(crate::tr!("还没买「{}」", "You haven't bought \"{}\" yet", it.label()));
        }
    }
    let pack = if slot == "rope" { "*" } else { pack };
    db.with(|c| match item_id {
        Some(id) => c.execute(
            "INSERT INTO equipped (pack, slot, item) VALUES (?1, ?2, ?3) ON CONFLICT(pack, slot) DO UPDATE SET item = ?3",
            params![pack, slot, id],
        ),
        None => c.execute("DELETE FROM equipped WHERE pack = ?1 AND slot = ?2", params![pack, slot]),
    })
    .map(|_| ())
    .map_err(|e| e.to_string())
}

pub fn equipped(db: &Db) -> Vec<(String, String, String)> {
    db.with(|c| {
        let mut st = c.prepare("SELECT pack, slot, item FROM equipped")?;
        let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        Ok(rows.flatten().collect())
    })
    .unwrap_or_default()
}

pub fn hat_of(db: &Db, pack: &str) -> Option<String> {
    db.with(|c| {
        c.query_row("SELECT item FROM equipped WHERE pack = ?1 AND slot = 'hat'", [pack], |r| r.get::<_, String>(0)).optional()
    })
    .ok()
    .flatten()
}

pub fn rope(db: &Db) -> Option<String> {
    db.with(|c| c.query_row("SELECT item FROM equipped WHERE pack = '*' AND slot = 'rope'", [], |r| r.get::<_, String>(0)).optional())
        .ok()
        .flatten()
}

// ------------------------------------------------------------ seasons

/// "2026-W45": ISO week of a local timestamp.
pub fn week_of(ts: u64) -> String {
    let d = Local.timestamp_millis_opt(ts as i64).single().map(|d| d.date_naive()).unwrap_or_default();
    let w = d.iso_week();
    format!("{}-W{:02}", w.year(), w.week())
}

fn monday_of(week: &str) -> Option<NaiveDate> {
    let (y, w) = week.split_once("-W")?;
    NaiveDate::from_isoywd_opt(y.parse().ok()?, w.parse().ok()?, chrono::Weekday::Mon)
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Season {
    pub week: String,
    pub cc: f64,
    pub cx: f64,
    /// claude-code | codex | tie
    pub winner: String,
}

/// Sum of the daily tug scores over the 7 days of `week`.
pub fn week_scores(db: &Db, week: &str, score: impl Fn(Option<&crate::db::CampDay>) -> f64) -> Option<(f64, f64)> {
    let mon = monday_of(week)?;
    let (mut cc, mut cx) = (0.0, 0.0);
    for i in 0..7 {
        let day = (mon + CDuration::days(i)).format("%Y-%m-%d").to_string();
        let d = db.camp_day(&day);
        cc += score(d.get("claude-code"));
        cx += score(d.get("codex"));
    }
    Some((cc, cx))
}

/// Settle last week once (the first tick of a new week). Returns the result.
pub fn settle(db: &Db, now: u64, score: impl Fn(Option<&crate::db::CampDay>) -> f64) -> Option<Season> {
    let last = week_of(now.saturating_sub(7 * 86_400_000));
    if db.kv_get("season_settled").as_deref() == Some(last.as_str()) {
        return None;
    }
    db.kv_set("season_settled", &last);
    let (cc, cx) = week_scores(db, &last, score)?;
    if cc == 0.0 && cx == 0.0 {
        return None; // nobody played that week
    }
    let winner = if (cc - cx).abs() < 1e-9 { "tie" } else if cc > cx { "claude-code" } else { "codex" };
    let _ = db.with(|c| {
        c.execute(
            "INSERT OR REPLACE INTO seasons (week, cc, cx, winner, ts) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![last, cc, cx, winner, now as i64],
        )
    });
    Some(Season { week: last, cc, cx, winner: winner.into() })
}

/// The winner of last week, who wears the crown this week.
pub fn champion(db: &Db, now: u64) -> Option<String> {
    let last = week_of(now.saturating_sub(7 * 86_400_000));
    db.with(|c| c.query_row("SELECT winner FROM seasons WHERE week = ?1", [&last], |r| r.get::<_, String>(0)).optional())
        .ok()
        .flatten()
        .filter(|w| w != "tie")
}

pub fn seasons(db: &Db) -> Vec<Season> {
    db.with(|c| {
        let mut st = c.prepare("SELECT week, cc, cx, winner FROM seasons ORDER BY week DESC LIMIT 52")?;
        let rows = st.query_map([], |r| Ok(Season { week: r.get(0)?, cc: r.get(1)?, cx: r.get(2)?, winner: r.get(3)? }))?;
        Ok(rows.flatten().collect())
    })
    .unwrap_or_default()
}

/// Local hour (used to remind night owls).
pub fn hour(ts: u64) -> u32 {
    Local.timestamp_millis_opt(ts as i64).single().map(|d| d.hour()).unwrap_or(12)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::TurnResult;
    use crate::session::FinishedTurn;

    fn turn(t: u64, ms: u64, r: TurnResult, errs: u32) -> FinishedTurn {
        FinishedTurn { started_at: t, ended_at: t + ms, result: r, tool_calls: 2, tool_errors: errs, attentions: 0 }
    }

    #[test]
    fn achievements_unlock_once_and_pay() {
        let db = Db::in_memory();
        let t0 = 1_793_600_000_000;
        db.record_turn("claude-code", "a", "/p", &turn(t0, 60_000, TurnResult::Ok, 0));
        on_turn(&db, true);
        let got = check(&db, t0 + 1, 12, 300);
        let ids: Vec<&str> = got.iter().map(|(a, _)| a.id).collect();
        assert!(ids.contains(&"first_turn") && ids.contains(&"clean_one"), "{ids:?}");
        assert_eq!(db.balance(), 20);
        assert!(check(&db, t0 + 2, 12, 300).is_empty(), "never twice");
        assert_eq!(achievements(&db).iter().filter(|r| r.unlocked_at.is_some()).count(), 2);
    }

    #[test]
    fn streak_and_duo() {
        let db = Db::in_memory();
        let t0 = 1_793_600_000_000;
        for i in 0..10 {
            db.record_turn("claude-code", "a", "/p", &turn(t0 + i, 1000, TurnResult::Ok, 0));
            on_turn(&db, true);
        }
        on_turn(&db, false);
        db.record_turn("codex", "b", "/p", &turn(t0, 1000, TurnResult::Ok, 0));
        let t = totals(&db);
        assert_eq!(t.best_streak, 10);
        assert!(t.duo);
        let ids: Vec<&str> = check(&db, t0, 12, 300).iter().map(|(a, _)| a.id).collect();
        assert!(ids.contains(&"streak10") && ids.contains(&"duo"), "{ids:?}");
    }

    #[test]
    fn shop_buy_equip() {
        let db = Db::in_memory();
        let t0 = 1_793_600_000_000;
        assert!(buy(&db, "straw_hat", t0).unwrap_err().contains("金币不够"));
        db.earn(200, "test", "", t0, 300);
        buy(&db, "straw_hat", t0).unwrap();
        assert_eq!(db.balance(), 120);
        assert!(buy(&db, "straw_hat", t0).is_err(), "only once");
        assert!(equip(&db, "pixel-cat", "hat", Some("halo")).is_err(), "not owned");
        equip(&db, "pixel-cat", "hat", Some("straw_hat")).unwrap();
        assert_eq!(hat_of(&db, "pixel-cat").as_deref(), Some("straw_hat"));
        assert_eq!(hat_of(&db, "pixel-bot"), None);
        equip(&db, "pixel-cat", "hat", None).unwrap();
        assert_eq!(hat_of(&db, "pixel-cat"), None);
        assert_eq!(db.income_on(&day_of(t0)), 200, "spending does not count as income");
    }

    #[test]
    fn season_settles_last_week_once() {
        let db = Db::in_memory();
        // A Wednesday; settle on the Monday after.
        let wed = Local.with_ymd_and_hms(2026, 11, 4, 12, 0, 0).unwrap().timestamp_millis() as u64;
        for i in 0..3 {
            db.record_turn("claude-code", "a", "/p", &turn(wed + i, 1000, TurnResult::Ok, 0));
        }
        for r in [TurnResult::Ok, TurnResult::Error, TurnResult::Ok] {
            db.record_turn("codex", "b", "/p", &turn(wed, 1000, r, 0));
        }
        let mon = Local.with_ymd_and_hms(2026, 11, 9, 9, 0, 0).unwrap().timestamp_millis() as u64;
        let score = |d: Option<&crate::db::CampDay>| d.map(|d| if d.turns < 3 { 0.0 } else { (d.clean as f64 - d.errors as f64) / d.turns as f64 }).unwrap_or(0.0);
        let s = settle(&db, mon, score).expect("settled");
        assert_eq!(s.winner, "claude-code");
        assert!(settle(&db, mon + 1000, score).is_none(), "only once");
        assert_eq!(champion(&db, mon).as_deref(), Some("claude-code"));
        assert_eq!(seasons(&db).len(), 1);
    }
}
