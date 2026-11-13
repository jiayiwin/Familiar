//! 来信. A letter is about you, not about what you did.
//!
//! Two things make one. The occasion is why it writes today: a long stuck
//! stretch that finally gave way, a run of late nights, coming back after days
//! away, something worked on for weeks gone quiet, a note it held for days
//! that you crossed off, a day fuller than any lately, a while of knowing you,
//! or, now and then, nothing at all. The reading is what it has come to
//! understand about who you are, from the way your days go: that you go quiet
//! and carry things alone when stuck, that the night is when you are most
//! yourself, that what you promise yourself always waits at the back… The
//! letter touches the occasion in a line and spends itself on the reading.
//!
//! No numbers, no clock times, no names of what you work on, no praise, no
//! advice. A model writes it when there is one (Claude with a key, or the local
//! model); otherwise it is put together from letters written for each occasion
//! and each reading (assets/letters.yaml). Nothing about your work is sent to
//! Claude: only why it is writing and what it understood, in plain words.
//!
//! At most one letter in [`GAP`], none while one is unread, never twice about
//! the same thing, and not the same reading twice in a row.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU64, Ordering};

use chrono::{Datelike, Local, NaiveDate, TimeZone, Timelike};

use crate::db::{day_of, Db, TurnRow};

const MIN: u64 = 60_000;
const HOUR: u64 = 60 * MIN;
const DAY: u64 = 24 * HOUR;
/// Two letters are at least this far apart.
const GAP: u64 = 4 * DAY;
/// The same kind of letter waits this long before it comes again.
const SAME_KIND: u64 = 14 * DAY;
/// How often to look for a reason to write.
const LOOK: u64 = 15 * MIN;

const PENDING: &str = "letter:pending";
const LAST: &str = "letter:last";
const LAST_READING: &str = "letter:reading";

/// Why it writes today.
#[derive(Debug, Clone)]
pub struct Occasion {
    pub kind: &'static str,
    /// What it is about: never written about twice.
    pub key: String,
    /// The part of the day it happened in (下午 / afternoon), or how long it has known you.
    pub when: String,
    /// The reading this occasion speaks to when nothing else in the portrait fits better.
    pub lead: &'static str,
    /// Readings this occasion would rather speak to, when the portrait has them.
    pub prefer: &'static [&'static str],
}

/// What it has come to understand about you, most telling first.
const READINGS: [&str; 11] = ["persist", "night", "self_last", "keeps_word", "juggler", "finisher", "no_break", "breaks", "early", "weekend", "steady"];

fn pick(en: bool, zh: &'static str, e: &'static str) -> &'static str {
    if en {
        e
    } else {
        zh
    }
}

fn local(ms: u64) -> chrono::DateTime<Local> {
    Local.timestamp_millis_opt(ms as i64).single().unwrap_or_else(Local::now)
}

fn part_of_day(ms: u64, en: bool) -> String {
    let (zh, e) = match local(ms).hour() {
        0..=4 => ("夜里", "night"),
        5..=11 => ("上午", "morning"),
        12..=17 => ("下午", "afternoon"),
        _ => ("晚上", "evening"),
    };
    pick(en, zh, e).into()
}

fn days_between(a: &str, b: &str) -> i64 {
    match (NaiveDate::parse_from_str(a, "%Y-%m-%d"), NaiveDate::parse_from_str(b, "%Y-%m-%d")) {
        (Ok(a), Ok(b)) => (b - a).num_days(),
        _ => 0,
    }
}

/// The last part of a project's path: what it is called.
fn base(p: &str) -> String {
    p.rsplit(['/', '\\']).find(|s| !s.is_empty()).unwrap_or("").to_string()
}

struct Seen<'a> {
    db: &'a Db,
    now: u64,
    en: bool,
    today: String,
    /// The last 60 days of turns, oldest first.
    turns: Vec<TurnRow>,
    /// Every day anything was seen, oldest first.
    days: Vec<String>,
}

impl Seen<'_> {
    fn day_ago(&self, n: u64) -> String {
        day_of(self.now.saturating_sub(n * DAY))
    }
    fn on(&self, day: &str) -> impl Iterator<Item = &TurnRow> {
        let day = day.to_string();
        self.turns.iter().filter(move |t| t.day == day)
    }
    fn since(&self, days: u64) -> impl Iterator<Item = &TurnRow> {
        let from = self.now.saturating_sub(days * DAY);
        self.turns.iter().filter(move |t| t.started_at >= from)
    }
}

/// Stretches of three or more failed turns in a row (one agent, one day), and
/// whether a turn went through after: (first failure, the turn that went through).
fn stuck_runs<'a>(rows: impl Iterator<Item = &'a TurnRow>) -> Vec<(&'a TurnRow, Option<&'a TurnRow>)> {
    let mut by: BTreeMap<(&str, &str), Vec<&TurnRow>> = BTreeMap::new();
    for t in rows {
        by.entry((t.day.as_str(), t.source.as_str())).or_default().push(t);
    }
    let mut out = vec![];
    for seq in by.values() {
        let mut i = 0;
        while i < seq.len() {
            if seq[i].result == "ok" {
                i += 1;
                continue;
            }
            let mut j = i;
            while j + 1 < seq.len() && seq[j + 1].result != "ok" {
                j += 1;
            }
            if j + 1 - i >= 3 {
                out.push((seq[i], seq.get(j + 1).copied()));
            }
            i = j + 1;
        }
    }
    out
}

// ---- who you are, from the way your days go

/// What it has come to understand about you over the last weeks, most telling first.
fn portrait(s: &Seen) -> Vec<&'static str> {
    let mut has: BTreeSet<&'static str> = BTreeSet::new();
    let month: Vec<&TurnRow> = s.since(30).collect();
    let active: BTreeSet<&str> = month.iter().map(|t| t.day.as_str()).collect();

    // Stuck, it stays and tries again until it goes through.
    let runs = stuck_runs(month.iter().copied());
    let through = runs.iter().filter(|(_, fix)| fix.is_some()).count();
    if runs.len() >= 2 && through * 3 >= runs.len() * 2 {
        has.insert("persist");
    }
    // The night is when it works.
    let nights: BTreeSet<String> = month
        .iter()
        .filter(|t| !(4..23).contains(&local(t.started_at).hour()))
        .map(|t| day_of(t.started_at.saturating_sub(6 * HOUR)))
        .collect();
    if nights.len() >= 3 && nights.len() * 4 >= active.len() {
        has.insert("night");
    }
    // It comes in early.
    let mut firsts: BTreeMap<&str, u64> = BTreeMap::new();
    for t in &month {
        firsts.entry(t.day.as_str()).or_insert(t.started_at);
    }
    let mut starts: Vec<u32> = firsts.values().map(|ms| local(*ms).hour() * 60 + local(*ms).minute()).collect();
    starts.sort_unstable();
    if starts.len() >= 5 && starts[starts.len() / 2] < 9 * 60 + 15 {
        has.insert("early");
    }
    // Breaks at the desk, on the days it sat long.
    let desk: Vec<(u64, u32)> = (0..14).map(|n| s.db.desk_day(&s.day_ago(n))).filter(|(ms, _)| *ms >= 2 * HOUR).collect();
    if desk.len() >= 3 {
        let avg = desk.iter().map(|(_, b)| *b as f64).sum::<f64>() / desk.len() as f64;
        if avg >= 4.0 {
            has.insert("breaks");
        } else if avg <= 1.0 {
            has.insert("no_break");
        }
    }
    // The notes it writes itself.
    let notes: Vec<crate::todo::Todo> = crate::todo::all(s.db).into_iter().filter(|t| t.from == "you").collect();
    let done = notes.iter().filter(|t| t.done_at.is_some()).count();
    let waited = notes.iter().filter(|t| t.done_at.is_some_and(|d| d.saturating_sub(t.at) >= 3 * DAY)).count();
    let left = notes.iter().filter(|t| t.done_at.is_none() && s.now.saturating_sub(t.at) >= 3 * DAY).count();
    if waited + left >= 2 {
        has.insert("self_last");
    } else if done >= 3 && left == 0 {
        has.insert("keeps_word");
    }
    // Several agents at once, on several days.
    let busy = active.iter().filter(|d| s.db.kv_get(&format!("peak:{d}")).and_then(|v| v.parse::<u32>().ok()).unwrap_or(0) >= 3).count();
    if busy >= 3 {
        has.insert("juggler");
    }
    // Days off, and still here.
    let weekend = active
        .iter()
        .filter(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").is_ok_and(|d| d.weekday().num_days_from_monday() >= 5))
        .count();
    if weekend >= 2 {
        has.insert("weekend");
    }
    // Here almost every day.
    if s.days.iter().filter(|d| days_between(d, &s.today) < 21).count() >= 14 {
        has.insert("steady");
    }
    // Something seen through, and straight on to the next.
    let mut spans: BTreeMap<String, (String, usize)> = BTreeMap::new();
    for t in &s.turns {
        let b = base(&t.project);
        if b.is_empty() {
            continue;
        }
        let e = spans.entry(b).or_insert((t.day.clone(), 0));
        if e.0 != t.day || e.1 == 0 {
            e.1 += 1;
        }
        e.0 = t.day.clone();
    }
    let finished = spans.iter().any(|(p, (last, n))| {
        *n >= 4 && days_between(last, &s.today) >= 3 && spans.iter().any(|(q, (l2, _))| q != p && l2 > last)
    });
    if finished {
        has.insert("finisher");
    }
    READINGS.iter().copied().filter(|r| has.contains(r)).collect()
}

// ---- why it writes today

/// Yesterday or the day before: one agent failing turn after turn for half an
/// hour or more, until one went through.
fn hard_won(s: &Seen) -> Option<Occasion> {
    for back in [1, 2] {
        let day = s.day_ago(back);
        let best = stuck_runs(s.on(&day))
            .into_iter()
            .filter_map(|(first, fix)| fix.map(|f| (f.ended_at.saturating_sub(first.started_at), first)))
            .filter(|(long, _)| *long >= 30 * MIN)
            .max_by_key(|(long, _)| *long);
        if let Some((_, first)) = best {
            return Some(Occasion {
                kind: "hard_won",
                key: format!("hard_won:{day}"),
                when: part_of_day(first.started_at, s.en),
                lead: "persist",
                prefer: &["breaks", "persist"],
            });
        }
    }
    None
}

/// Three or more of the last seven nights, still at it after midnight.
fn late_streak(s: &Seen) -> Option<Occasion> {
    let nights: BTreeSet<String> = s
        .since(7)
        .filter(|t| local(t.started_at).hour() < 5)
        .map(|t| day_of(t.started_at.saturating_sub(6 * HOUR)))
        .collect();
    (nights.len() >= 3).then(|| Occasion {
        kind: "late_streak",
        key: format!("late:{}", nights.last().cloned().unwrap_or_default()),
        when: String::new(),
        lead: "night",
        prefer: &["night"],
    })
}

/// Back today after three days or more without a sign of you.
fn back(s: &Seen) -> Option<Occasion> {
    let here_today = s.on(&s.today).count() >= 2 || s.db.desk_day(&s.today).0 >= 20 * MIN;
    let prev = s.days.iter().rev().find(|d| d.as_str() < s.today.as_str())?;
    (here_today && days_between(prev, &s.today) >= 4).then(|| Occasion {
        kind: "back",
        key: format!("back:{}", s.today),
        when: String::new(),
        lead: "leaves_quietly",
        prefer: &[],
    })
}

/// A note you wrote yourself, held for three days or more, crossed off in the last day and a half.
fn held(s: &Seen) -> Option<Occasion> {
    let t = crate::todo::all(s.db)
        .into_iter()
        .filter(|t| t.from == "you")
        .filter(|t| t.done_at.is_some_and(|d| s.now.saturating_sub(d) < 36 * HOUR && d.saturating_sub(t.at) >= 3 * DAY))
        .max_by_key(|t| t.done_at)?;
    Some(Occasion { kind: "held", key: format!("held:{}", t.id), when: String::new(), lead: "self_last", prefer: &["keeps_word", "self_last"] })
}

/// Something you worked on for a week or more, on five days or more, gone quiet for three days.
fn long_run(s: &Seen) -> Option<Occasion> {
    let mut by: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for t in &s.turns {
        let b = base(&t.project);
        if !b.is_empty() {
            by.entry(b).or_default().insert(t.day.clone());
        }
    }
    by.into_iter()
        .filter_map(|(p, ds)| {
            let (first, last) = (ds.first()?.clone(), ds.last()?.clone());
            let quiet = days_between(&last, &s.today);
            (ds.len() >= 5 && days_between(&first, &last) >= 7 && (3..=10).contains(&quiet)).then_some((p, ds.len(), last))
        })
        .max_by_key(|(_, n, _)| *n)
        .map(|(p, _, last)| Occasion { kind: "long_run", key: format!("long:{p}:{last}"), when: String::new(), lead: "finisher", prefer: &["finisher", "persist"] })
}

const TOGETHER: [u64; 7] = [7, 30, 100, 200, 365, 730, 1000];

/// A week, a month, a season… of days it saw you.
fn together(s: &Seen) -> Option<Occasion> {
    let n = s.days.len() as u64;
    let m = *TOGETHER.iter().rev().find(|m| **m <= n)?;
    let (zh, en) = match m {
        7 => ("一个星期", "a week"),
        30 => ("一个月", "a month"),
        100 => ("一个季节", "a season"),
        200 => ("大半年", "the better part of a year"),
        365 => ("一整年", "a whole year"),
        730 => ("两年", "two years"),
        _ => ("很久很久", "a long, long time"),
    };
    Some(Occasion { kind: "together", key: format!("together:{m}"), when: pick(s.en, zh, en).into(), lead: "", prefer: &[] })
}

/// Yesterday: more turns than on any of the last weeks' days, by a long way.
fn big_day(s: &Seen) -> Option<Occasion> {
    let y = s.day_ago(1);
    let mut per: BTreeMap<&str, u32> = BTreeMap::new();
    for t in &s.turns {
        *per.entry(t.day.as_str()).or_default() += 1;
    }
    let n = *per.get(y.as_str())?;
    let mut others: Vec<u32> = per.iter().filter(|(d, _)| **d != y.as_str()).map(|(_, n)| *n).collect();
    if others.len() < 10 || n < 12 {
        return None;
    }
    others.sort_unstable();
    if n < *others.last()? || (n as f64) < others[others.len() / 2] as f64 * 1.6 {
        return None;
    }
    let peak: u32 = s.db.kv_get(&format!("peak:{y}")).and_then(|v| v.parse().ok()).unwrap_or(0);
    Some(Occasion {
        kind: "big_day",
        key: format!("big:{y}"),
        when: String::new(),
        lead: if peak >= 3 { "juggler" } else { "no_break" },
        prefer: &["juggler", "no_break"],
    })
}

/// Nothing in particular: now and then, once it has seen enough of your days.
fn ordinary(s: &Seen, last: Option<u64>) -> Option<Occasion> {
    if last.is_some_and(|l| s.now.saturating_sub(l) < 14 * DAY) {
        return None;
    }
    let recent = s.days.iter().filter(|d| days_between(d, &s.today) < 21).count();
    // About one day in three, decided by the date, so it does not hang on the minute it looks.
    let roll = s.today.bytes().fold(7u32, |h, b| h.wrapping_mul(31).wrapping_add(b as u32));
    (recent >= 10 && roll % 3 == 0).then(|| Occasion { kind: "ordinary", key: format!("ordinary:{}", s.today), when: String::new(), lead: "steady", prefer: &READINGS })
}

/// A reason to write now, and what the letter will be about: the first occasion
/// not written about yet, and the readings it speaks to.
pub fn pick_letter(db: &Db, now: u64, en: bool) -> Option<(Occasion, Vec<&'static str>)> {
    let s = Seen { db, now, en, today: day_of(now), turns: db.turns_since(now.saturating_sub(60 * DAY)), days: db.days_seen() };
    let last = db.kv_get(LAST).and_then(|v| v.parse().ok());
    let o = [hard_won(&s), back(&s), held(&s), long_run(&s), together(&s), late_streak(&s), big_day(&s), ordinary(&s, last)]
        .into_iter()
        .flatten()
        .find(|o| !done(db, &o.key) && (o.kind == "together" || kind_at(db, o.kind).is_none_or(|t| now.saturating_sub(t) >= SAME_KIND)))?;
    let known = portrait(&s);
    let said = db.kv_get(LAST_READING).unwrap_or_default();
    let readings = if o.kind == "together" {
        known.into_iter().take(3).collect()
    } else {
        let r = o.prefer.iter().copied().find(|r| known.contains(r) && *r != said).unwrap_or(o.lead);
        vec![r]
    };
    Some((o, readings))
}

fn done(db: &Db, key: &str) -> bool {
    db.kv_get(&format!("letter:done:{key}")).is_some_and(|v| !v.is_empty())
}

fn kind_at(db: &Db, kind: &str) -> Option<u64> {
    db.kv_get(&format!("letter:kind:{kind}")).and_then(|v| v.parse().ok())
}

/// It is going to be written: never about this again, and the clocks start.
fn mark(db: &Db, o: &Occasion, readings: &[&str], now: u64) {
    db.kv_set(&format!("letter:done:{}", o.key), &now.to_string());
    db.kv_set(&format!("letter:kind:{}", o.kind), &now.to_string());
    db.kv_set(LAST, &now.to_string());
    if let [one] = readings {
        db.kv_set(LAST_READING, one);
    }
    // A while written about means the shorter whiles are too.
    if o.kind == "together" {
        let m: u64 = o.key.trim_start_matches("together:").parse().unwrap_or(0);
        for k in TOGETHER.iter().filter(|k| **k <= m) {
            db.kv_set(&format!("letter:done:together:{k}"), &now.to_string());
        }
    }
}

// ---- the letter

/// A letter is waiting to be opened.
pub fn waiting(db: &Db) -> bool {
    db.kv_get(PENDING).is_some_and(|v| !v.is_empty())
}

/// The waiting letter, taken out of the box ({ text, pet_name, skin, kind }).
pub fn take(db: &Db) -> Option<serde_json::Value> {
    let v = db.kv_get(PENDING).filter(|v| !v.is_empty())?;
    db.kv_set(PENDING, "");
    serde_json::from_str(&v).ok()
}

static LETTERS_YAML: &str = include_str!("../assets/letters.yaml");

fn yaml() -> serde_yaml::Value {
    serde_yaml::from_str(LETTERS_YAML).unwrap_or(serde_yaml::Value::Null)
}

fn lang(en: bool) -> &'static str {
    if en {
        "en"
    } else {
        "zh"
    }
}

/// The `n`th of a list (in turn), or a single string.
fn nth(v: &serde_yaml::Value, n: usize) -> Option<String> {
    match v {
        serde_yaml::Value::Sequence(s) if !s.is_empty() => s[n % s.len()].as_str().map(|x| x.trim_end().to_string()),
        serde_yaml::Value::String(x) => Some(x.trim_end().to_string()),
        _ => None,
    }
}

fn entry(y: &serde_yaml::Value, table: &str, en: bool, key: &str, n: usize) -> Option<String> {
    nth(y.get(format!("{table}_{}", lang(en)))?.get(key)?, n)
}

/// The understandings, a line each.
fn lines(readings: &[&str], en: bool) -> Vec<String> {
    let y = yaml();
    readings.iter().filter_map(|r| entry(&y, "line", en, r, 0)).collect()
}

/// Why it is writing, in plain words (for a model).
fn why(o: &Occasion, en: bool) -> String {
    let w = &o.when;
    match (o.kind, en) {
        ("hard_won", false) => format!("前两天{w}，有一件事卡了很久，最后过去了。"),
        ("hard_won", true) => format!("A couple of days ago, in the {w}, something stayed stuck a long time and finally went through."),
        ("late_streak", false) => "这几天，对方都走得很晚，过了半夜还在。".into(),
        ("late_streak", true) => "These past days they've stayed very late, past midnight.".into(),
        ("back", false) => "对方离开了几天，今天回来了。".into(),
        ("back", true) => "They were away for some days, and today they came back.".into(),
        ("long_run", false) => "一件对方做了很久的事，这几天不再出现了。".into(),
        ("long_run", true) => "Something they worked on for a long time has stopped appearing these days.".into(),
        ("held", false) => "对方写给自己的一张纸条，你替对方拿了好些天，今天被勾掉了。".into(),
        ("held", true) => "A note they wrote for themselves, which you held for days, was ticked off today.".into(),
        ("big_day", false) => "昨天是这阵子最满的一天。".into(),
        ("big_day", true) => "Yesterday was the fullest day in a while.".into(),
        ("together", false) => format!("认识对方{w}了。"),
        ("together", true) => format!("You have known them {w}."),
        (_, false) => "没什么特别的事，只是想写。".into(),
        (_, true) => "Nothing in particular; you just want to write.".into(),
    }
}

/// The letter put together from the ones written for this occasion and this reading.
pub fn from_template(o: &Occasion, readings: &[&str], name: &str, en: bool, n: usize) -> String {
    let y = yaml();
    let open = entry(&y, "open", en, o.kind, n).unwrap_or_default().replace("{part}", &o.when).replace("{while}", &o.when);
    let body = if o.kind == "together" {
        let l = lines(readings, en);
        if l.is_empty() {
            nth(y.get(format!("unknown_{}", lang(en))).unwrap_or(&serde_yaml::Value::Null), 0).unwrap_or_default()
        } else {
            let known = nth(y.get(format!("known_{}", lang(en))).unwrap_or(&serde_yaml::Value::Null), 0).unwrap_or_default();
            format!("{}\n\n{known}", l.join(if en { " " } else { "" }))
        }
    } else {
        entry(&y, "body", en, readings.first().copied().unwrap_or(o.lead), n).unwrap_or_default()
    };
    let ps = entry(&y, "ps", en, o.kind, n).unwrap_or_default();
    format!("{open}\n\n{body}\n\n——{name}\n{ps}")
}

/// The letter prompts (system, user): who is writing, why today, and what it understood.
pub fn prompts(o: &Occasion, readings: &[&str], name: &str, who: &str, en: bool, closeness: f64) -> (String, String) {
    let nature = crate::voice::nature(who, en);
    let when = crate::bond::moment(en);
    let why = why(o, en);
    let understood = lines(readings, en);
    let understood = if understood.is_empty() {
        pick(en, "（还说不上来。你还在慢慢认识对方。）", "(Nothing you could put into words yet. You are still getting to know them.)").to_string()
    } else {
        understood.iter().map(|l| format!("- {l}")).collect::<Vec<_>>().join("\n")
    };
    let near = match (closeness, en) {
        (c, false) if c >= 60.0 => "你们已经很熟了，话可以说得直一点。",
        (c, true) if c >= 60.0 => "You know each other well by now; you can say things plainly.",
        (c, false) if c < 20.0 => "你们还在慢慢熟起来，话要收着一点。",
        (c, true) if c < 20.0 => "You're still getting to know each other; hold back a little.",
        _ => "",
    };
    if en {
        (
            format!(
                "You are {name}, a small creature who lives on someone's desktop and keeps them company while they and their agents work.\n\
                 {nature}\n\
                 You are writing them a letter, the kind a soul companion writes: not about what they did, but about who they are. \
                 Over the days you've come to understand something about them that maybe no one has said to them. Say it, and say what you feel about it.\n\
                 Rules: no numbers, no clock times, no names of the things they work on, no account of their day. No greeting, no thanks, no praise, no advice, no summary, no slogans. \
                 Plain words and one real image; a little left unsaid. Never mention sweets, feeding, or being a program."
            ),
            format!(
                "Why today (touch it in a line, no more): {why}\nWhat you've come to understand about them:\n{understood}\n{near}\n{when}\n\
                 Three short paragraphs: the moment in a line or two; what you understood about them; what you feel about it. \
                 End with the signature line \"——{name}\", then on the next line, in parentheses, one small thing you've kept about them."
            ),
        )
    } else {
        (
            format!(
                "你是{name}，住在一个人电脑桌面上的小东西，陪对方和对方的 agent 一起干活。\n\
                 {nature}\n\
                 你在给对方写一封信，像灵魂伴侣会写的那种：不写对方做了什么，写对方是一个什么样的人。\
                 这些日子你慢慢看懂了对方身上的一点东西，也许从来没有人对对方说过。把它说出来，再说说你对它的感受。\n\
                 规矩：不出现数字、钟点、对方做的事情的名字，不复述对方的一天。不写称呼和开场白，不感谢，不夸，不劝，不总结，不喊口号。\
                 用朴素的话和一个真实的画面，留一点没说破的。不提糖，不提喂，不提自己是程序。直接用「你」称呼对方。"
            ),
            format!(
                "今天写信的由头（一笔带过就好）：{why}\n这些日子你看懂的对方：\n{understood}\n{near}\n{when}\n\
                 写三小段：一两句点到今天的由头；你看懂的对方；你对它的感受。\
                 结尾单独一行落款「——{name}」，再另起一行，用括号写一件你记着的、关于对方的小事。"
            ),
        )
    }
}

/// A model's letter, if it reads like one: some body, signed, and no numbers in it.
fn signed(text: &str, name: &str) -> Option<String> {
    let t = text.trim();
    if t.chars().count() < 40 || t.chars().any(|c| c.is_ascii_digit()) {
        return None;
    }
    Some(if t.contains("——") { t.to_string() } else { format!("{t}\n\n——{name}") })
}

async fn call_claude(api_key: &str, system: &str, user: &str) -> Option<String> {
    let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(30)).build().ok()?;
    let res = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&serde_json::json!({
            "model": "claude-sonnet-5-5",
            "max_tokens": 700,
            "system": system,
            "messages": [{ "role": "user", "content": user }]
        }))
        .send()
        .await
        .ok()?;
    if !res.status().is_success() {
        return None;
    }
    let j: serde_json::Value = res.json().await.ok()?;
    j.get("content")?.get(0)?.get("text")?.as_str().map(String::from)
}

/// The letter: by Claude (with a key), by the local model (when downloaded), or
/// put together from the ones written for this occasion and reading.
#[allow(clippy::too_many_arguments)]
pub async fn write(o: &Occasion, readings: &[&str], name: &str, who: &str, en: bool, closeness: f64, home: std::path::PathBuf, n: usize) -> String {
    let (system, user) = prompts(o, readings, name, who, en, closeness);
    let key = std::env::var("ANTHROPIC_API_KEY").unwrap_or_default();
    if !key.is_empty() {
        if let Some(t) = call_claude(&key, &system, &user).await.and_then(|t| signed(&t, name)) {
            return t;
        }
    }
    if let Ok(Some(t)) = tokio::task::spawn_blocking(move || crate::local_llm::generate_local(&home, &system, &user)).await {
        if let Some(t) = signed(&t, name) {
            return t;
        }
    }
    from_template(o, readings, name, en, n)
}

static LOOKED: AtomicU64 = AtomicU64::new(0);

/// Every so often, while you are at the desk: is there a reason to write to
/// you? If so, the pet that would speak now writes, and says so.
pub fn tick(sh: &crate::server::Shared, active: bool) {
    if !active {
        return;
    }
    let now = crate::now_ms();
    if now.saturating_sub(LOOKED.load(Ordering::Relaxed)) < LOOK {
        return;
    }
    LOOKED.store(now, Ordering::Relaxed);
    if waiting(&sh.db) || sh.db.kv_get(LAST).and_then(|v| v.parse::<u64>().ok()).is_some_and(|t| now.saturating_sub(t) < GAP) {
        return;
    }
    let Some((pet, who)) = crate::drawer::asker(sh) else { return };
    let en = crate::i18n::is_en();
    let Some((o, readings)) = pick_letter(&sh.db, now, en) else { return };
    mark(&sh.db, &o, &readings, now);
    let n_key = format!("letter:n:{}", o.kind);
    let n: usize = sh.db.kv_get(&n_key).and_then(|v| v.parse().ok()).unwrap_or(0);
    sh.db.kv_set(&n_key, &(n + 1).to_string());
    let name = crate::cast::by_id(who).map(|f| if en { f.en } else { f.zh }).unwrap_or(if en { "Zimeng" } else { "紫梦" }).to_string();
    let closeness = sh.db.bond_scores().0;
    let sh = sh.clone();
    tokio::spawn(async move {
        let text = write(&o, &readings, &name, who, en, closeness, sh.paths.home.clone(), n).await;
        let letter = serde_json::json!({ "text": text, "pet_name": name, "skin": who, "kind": o.kind, "count": 0 });
        sh.db.kv_set(PENDING, &letter.to_string());
        sh.db.log_event(crate::now_ms(), "letter", &pet, 1);
        let line = crate::voice::line(who, "letter_ready", 0);
        let _ = sh.ws_tx.send(crate::msg("pet.act", &serde_json::json!({ "pet": pet, "act": "speak", "text": line })));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(day_back: u64, h: u32, m: u32) -> u64 {
        let d = Local::now().date_naive() - chrono::Duration::days(day_back as i64);
        Local.from_local_datetime(&d.and_hms_opt(h, 0, 0).unwrap()).single().unwrap().timestamp_millis() as u64 + m as u64 * MIN
    }

    /// Noon today: the tests look from there, whatever the clock says.
    fn noon() -> u64 {
        at(0, 12, 0)
    }

    fn db() -> Db {
        Db::open(std::path::Path::new(":memory:")).unwrap()
    }

    fn no_numbers(text: &str) {
        assert!(!text.chars().any(|c| c.is_ascii_digit()), "a letter has no numbers: {text}");
        for w in ["一个多小时", "点多", "api-server", "{"] {
            assert!(!text.contains(w), "no account of the day ({w}): {text}");
        }
    }

    #[test]
    fn nothing_happened_means_no_letter() {
        let d = db();
        assert!(pick_letter(&d, noon(), false).is_none());
        for i in 0..4 {
            d.add_turn_in("codex", "/w/site", at(1, 10, i * 10), at(1, 10, i * 10 + 5), "ok", 0);
        }
        assert!(pick_letter(&d, noon(), false).is_none());
    }

    #[test]
    fn a_stuck_stretch_that_gave_way_becomes_a_letter_about_you_not_about_it() {
        let d = db();
        d.add_turn_in("claude-code", "/w/api-server", at(1, 14, 0), at(1, 14, 10), "ok", 0);
        for i in 0..4 {
            d.add_turn_in("claude-code", "/w/api-server", at(1, 14, 20 + i * 15), at(1, 14, 30 + i * 15), "error", 3);
        }
        d.add_turn_in("claude-code", "/w/api-server", at(1, 15, 30), at(1, 15, 50), "ok", 0);
        let (o, readings) = pick_letter(&d, noon(), false).expect("a letter");
        assert_eq!((o.kind, o.when.as_str()), ("hard_won", "下午"));
        assert_eq!(readings, ["persist"]);
        let text = from_template(&o, &readings, "紫梦", false, 0);
        no_numbers(&text);
        assert!(text.contains("不出声") && text.contains("——紫梦\n（"), "{text}");
        // The model is told why and what it understood, never what or when.
        let (system, user) = prompts(&o, &readings, "紫梦", "zimeng", false, 50.0);
        assert!(!user.contains("api-server") && !user.chars().any(|c| c.is_ascii_digit()), "{user}");
        assert!(system.contains("灵魂伴侣") && user.contains("卡住了宁愿自己再试一次"), "{user}");
        mark(&d, &o, &readings, noon());
        assert!(pick_letter(&d, noon(), false).is_none(), "never twice about the same thing");
    }

    #[test]
    fn coming_back_is_about_how_you_leave() {
        let d = db();
        d.add_turn_in("codex", "/w/old-thing", at(6, 11, 0), at(6, 11, 20), "ok", 0);
        d.add_turn_in("codex", "/w/new-thing", at(0, 10, 0), at(0, 10, 20), "ok", 0);
        d.add_turn_in("codex", "/w/new-thing", at(0, 11, 0), at(0, 11, 10), "ok", 0);
        let (o, readings) = pick_letter(&d, noon(), true).expect("a letter");
        assert_eq!((o.kind, readings.as_slice()), ("back", ["leaves_quietly"].as_slice()));
        let text = from_template(&o, &readings, "Mashu", true, 1);
        no_numbers(&text);
        assert!(text.contains("I need a break") && text.ends_with(')'), "{text}");
    }

    #[test]
    fn late_nights_are_read_as_who_you_are_at_night() {
        let d = db();
        for n in [1, 2, 4] {
            d.add_turn_in("claude-code", "/w/game", at(n, 1, 10), at(n, 1, 30), "ok", 0);
        }
        let (o, readings) = pick_letter(&d, noon(), false).expect("a letter");
        assert_eq!((o.kind, readings.as_slice()), ("late_streak", ["night"].as_slice()));
        no_numbers(&from_template(&o, &readings, "泡泡", false, 0));
        no_numbers(&from_template(&o, &readings, "泡泡", false, 1));
    }

    #[test]
    fn the_portrait_comes_from_the_way_the_days_go() {
        let d = db();
        // Stuck twice and through both times; late on several nights; early most mornings.
        for n in 1..=6u64 {
            d.add_turn_in("codex", "/w/a", at(n, 8, 30), at(n, 8, 50), "ok", 0);
            d.add_turn_in("codex", "/w/a", at(n, 23, 30), at(n, 23, 50), "ok", 0);
        }
        for n in [3u64, 5] {
            for i in 0..3 {
                d.add_turn_in("claude-code", "/w/b", at(n, 14, i * 10), at(n, 14, i * 10 + 5), "error", 2);
            }
            d.add_turn_in("claude-code", "/w/b", at(n, 15, 0), at(n, 15, 10), "ok", 0);
        }
        let s = Seen { db: &d, now: noon(), en: false, today: day_of(noon()), turns: d.turns_since(0), days: d.days_seen() };
        let p = portrait(&s);
        assert!(p.contains(&"persist") && p.contains(&"night") && p.contains(&"early"), "{p:?}");
        // A letter about knowing you lists what it understood.
        let o = Occasion { kind: "together", key: "together:7".into(), when: "一个星期".into(), lead: "", prefer: &[] };
        let text = from_template(&o, &p[..3], "紫梦", false, 0);
        no_numbers(&text);
        assert!(text.contains("认识你一个星期了") && text.contains("有人也知道"), "{text}");
    }

    #[test]
    fn every_occasion_and_reading_has_its_words() {
        let y = yaml();
        for en in [false, true] {
            for kind in ["hard_won", "late_streak", "back", "held", "long_run", "together", "big_day", "ordinary"] {
                assert!(entry(&y, "open", en, kind, 0).is_some(), "open {kind}");
                assert!(entry(&y, "ps", en, kind, 0).is_some(), "ps {kind}");
            }
            for r in READINGS.iter().chain(["leaves_quietly"].iter()) {
                let b = entry(&y, "body", en, r, 0).unwrap_or_else(|| panic!("body {r}"));
                no_numbers(&b);
                if *r != "leaves_quietly" {
                    assert!(entry(&y, "line", en, r, 0).is_some(), "line {r}");
                }
            }
        }
    }
}
