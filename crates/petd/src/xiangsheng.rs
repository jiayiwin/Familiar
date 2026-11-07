//! 相声 (crosstalk): two pets walk up to each other and perform a short piece.
//! The overlay draws the little table, the big speech bubbles and the
//! audience's laughs; each pet window plays its own actions. Everything runs
//! on one timeline that starts `DUO_WALK` ms after the "xs" message.
//!
//! Pieces come from assets/xiangsheng.yaml plus ~/.familiar/xiangsheng/: more
//! YAML in the same format, or plain scripts (`*.txt`, 甲：… / 乙：… lines)
//! the user brings for their own use.

use crate::duo::DUO_WALK;
use crate::hub::PetView;
use crate::session::State;
use crate::world::{Out, Script, World};
use crate::Control;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::Path;

pub const XS_YAML: &str = include_str!("../assets/xiangsheng.yaml");

/// Gap between the two windows, logical px (room for the table).
const GAP: f64 = 30.0;
/// Pause between lines, and the extra pause while the audience laughs.
const BEAT: u64 = 260;
const LAUGH: u64 = 1300;
/// The same piece is not picked again for this long (unless asked for).
const REPEAT: u64 = 12 * 3_600_000;
/// Pets start a piece on their own only if it is this short.
const AUTO_MAX: u64 = 4 * 60_000;
/// Longer lines are split into several bubbles.
const CHUNK: usize = 40;

#[derive(Debug, Clone, Deserialize)]
pub struct Piece {
    pub id: String,
    pub title: String,
    /// `[who, text]` or `[who, text, flags]`.
    pub lines: Vec<Vec<String>>,
    /// "zh" / "en" for the user's .txt scripts (shown in any UI language);
    /// built-in pieces follow the UI language.
    #[serde(default)]
    pub lang: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct Book {
    #[serde(default)]
    zh: Vec<Piece>,
    #[serde(default)]
    en: Vec<Piece>,
}

impl Book {
    fn pick(self, en: bool) -> Vec<Piece> {
        if en { self.en } else { self.zh }
    }
}

/// Built-in pieces in one language.
pub fn builtin(en: bool) -> Vec<Piece> {
    serde_yaml::from_str::<Book>(XS_YAML).expect("built-in xiangsheng.yaml").pick(en)
}

/// The user's folder for their own pieces.
pub fn dir(home: &Path) -> std::path::PathBuf {
    home.join("xiangsheng")
}

/// Built-ins plus `home/xiangsheng/*.yaml` and `*.txt`; a piece with the same
/// id replaces the built-in one. Broken files are reported and skipped.
pub fn load(home: &Path, en: bool, warnings: &mut Vec<String>) -> Vec<Piece> {
    let mut all = builtin(en);
    let Ok(rd) = std::fs::read_dir(dir(home)) else { return all };
    let ext = |p: &Path| p.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).unwrap_or_default();
    let mut files: Vec<_> = rd.flatten().map(|e| e.path()).filter(|p| matches!(ext(p).as_str(), "yaml" | "yml" | "txt")).collect();
    files.sort();
    for f in files {
        if ext(&f) == "txt" {
            match read_text(&f).map(|t| parse_txt(&t, f.file_stem().and_then(|s| s.to_str()).unwrap_or("相声"))) {
                Some(Some(p)) => {
                    all.retain(|q| q.id != p.id);
                    all.push(p);
                }
                Some(None) => warnings.push(format!("{}: no 甲：/乙： lines", f.display())),
                None => warnings.push(format!("{}: can't read", f.display())),
            }
            continue;
        }
        let parsed = std::fs::read_to_string(&f).map_err(|e| e.to_string()).and_then(|t| serde_yaml::from_str::<Book>(&t).map_err(|e| e.to_string()));
        match parsed {
            Ok(b) => {
                for p in b.pick(en).into_iter().filter(|p| p.lines.iter().any(|l| l.len() >= 2)) {
                    all.retain(|q| q.id != p.id);
                    all.push(p);
                }
            }
            Err(e) => warnings.push(format!("{}: {e}", f.display())),
        }
    }
    all
}

/// A text file as UTF-8, or GBK (what Notepad on a Chinese Windows often saves).
fn read_text(f: &Path) -> Option<String> {
    let bytes = std::fs::read(f).ok()?;
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&bytes);
    match std::str::from_utf8(bytes) {
        Ok(s) => Some(s.to_string()),
        Err(_) => {
            let (s, _, bad) = encoding_rs::GBK.decode(bytes);
            (!bad).then(|| s.into_owned())
        }
    }
}

/// Who a script line belongs to: 甲 / 逗哏 / A is the joker, 乙 / 捧哏 / B the straight man.
fn speaker(line: &str) -> Option<(&'static str, &str)> {
    let l = line.trim_start();
    let (head, rest) = l.split_once(['：', ':'])?;
    let who = match head.trim().trim_matches(['【', '】', '[', ']']) {
        "甲" | "逗" | "逗哏" | "A" | "a" => "a",
        "乙" | "捧" | "捧哏" | "B" | "b" => "b",
        "合" | "甲乙" | "甲、乙" | "AB" | "ab" | "Both" | "both" => "ab",
        _ => return None,
    };
    Some((who, rest))
}

/// Take stage directions out of a line: （笑）, (众笑), [掌声] … Says whether one was a laugh.
fn strip_directions(text: &str) -> (String, bool) {
    let mut out = String::new();
    let mut laugh = false;
    let mut depth = 0;
    let mut note = String::new();
    for c in text.chars() {
        match c {
            '（' | '(' | '[' | '【' => {
                depth += 1;
                note.clear();
            }
            '）' | ')' | ']' | '】' if depth > 0 => {
                depth -= 1;
                if ["笑", "掌声", "鼓掌", "laugh", "applause"].iter().any(|k| note.to_lowercase().contains(k)) {
                    laugh = true;
                }
            }
            _ if depth > 0 => note.push(c),
            _ => out.push(c),
        }
    }
    (out.split_whitespace().collect::<Vec<_>>().join(" ").trim().to_string(), laugh)
}

/// A plain script: an optional 《title》 first line, then 甲：/乙： lines. Lines
/// without a speaker continue the previous one; a line that is only a
/// direction like （笑） makes the audience laugh after the previous line.
pub fn parse_txt(text: &str, stem: &str) -> Option<Piece> {
    let mut title = stem.trim().to_string();
    let mut lines: Vec<Vec<String>> = vec![];
    for (i, raw) in text.lines().enumerate() {
        let raw = raw.trim();
        if raw.is_empty() {
            continue;
        }
        if lines.is_empty() && i < 3 && speaker(raw).is_none() {
            let t = raw.trim_start_matches("标题").trim_start_matches(['：', ':']).trim().trim_matches(['《', '》']).trim();
            if !t.is_empty() && t.chars().count() <= 30 {
                title = t.to_string();
            }
            continue;
        }
        let (who, body) = match speaker(raw) {
            Some((w, b)) => (Some(w), b),
            None => (None, raw),
        };
        let (clean, laugh) = strip_directions(body);
        match (who, lines.last_mut()) {
            (Some(w), _) if !clean.is_empty() => lines.push(vec![w.into(), clean, if laugh { "laugh".into() } else { String::new() }]),
            (_, Some(prev)) if laugh && clean.is_empty() => prev[2] = "laugh".into(),
            (None, Some(prev)) if !clean.is_empty() => {
                prev[1].push_str(&clean);
                if laugh {
                    prev[2] = "laugh".into();
                }
            }
            _ => {}
        }
    }
    if lines.len() < 2 {
        return None;
    }
    let cjk = lines.iter().map(|l| l[1].chars().filter(|c| *c as u32 >= 0x2E80).count()).sum::<usize>();
    let total = lines.iter().map(|l| l[1].chars().count()).sum::<usize>();
    let lang = if cjk * 5 >= total { "zh" } else { "en" };
    Some(Piece { id: format!("txt:{stem}"), title, lines, lang: Some(lang.into()) })
}

/// Split a long line at sentence ends (then commas) into bubbles of at most `CHUNK` characters.
fn chunks(text: &str) -> Vec<String> {
    if text.chars().count() <= CHUNK {
        return vec![text.to_string()];
    }
    let mut parts: Vec<String> = vec![];
    let mut cur = String::new();
    for c in text.chars() {
        cur.push(c);
        let n = cur.chars().count();
        if "。！？；!?;…".contains(c) || (n >= CHUNK / 2 && "，、,".contains(c)) || n >= CHUNK {
            parts.push(std::mem::take(&mut cur));
        }
    }
    if !cur.trim().is_empty() {
        parts.push(cur);
    }
    // Merge short neighbours back up to CHUNK.
    let mut out: Vec<String> = vec![];
    for p in parts {
        match out.last_mut() {
            Some(last) if last.chars().count() + p.chars().count() <= CHUNK || p.chars().all(|c| !c.is_alphanumeric()) => last.push_str(&p),
            _ => out.push(p),
        }
    }
    out.into_iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
}

/// One line on the timeline (ms from the start of the show, after the walk).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Line {
    /// "a" (逗哏), "b" (捧哏) or "ab" (both).
    pub who: String,
    pub text: String,
    pub at: u64,
    pub ms: u64,
    /// "laugh" (the audience laughs after it), "clap" (the bow) or "".
    pub fx: &'static str,
    /// Action for the speaker(s), if any.
    pub act: Option<String>,
}

const ACTIONS: &[&str] = &["cheer", "sulk", "annoyed", "taunt", "wave", "dizzy", "thinking"];

/// How long a bubble stays up: ~5 CJK or ~15 Latin characters a second.
pub fn read_ms(text: &str) -> u64 {
    let cjk = text.chars().filter(|c| *c as u32 >= 0x2E80).count() as u64;
    let other = text.chars().count() as u64 - cjk;
    (1200 + cjk * 210 + other * 62).clamp(1500, 8000)
}

/// The whole show: opening, the piece, the bow. Returns the lines and the length.
pub fn timeline(p: &Piece, en: bool) -> (Vec<Line>, u64) {
    let en = p.lang.as_deref().map_or(en, |l| l == "en");
    let mut raw: Vec<(String, String, String)> = vec![];
    let mut push = |who: &str, text: String, flags: &str| raw.push((who.into(), text, flags.into()));
    if en {
        push("a", "Hi everyone! We've got a little comedy bit for you.".into(), "wave");
        push("b", "What's it called?".into(), "");
        push("a", format!("“{}”!", p.title), "");
    } else {
        push("a", "大家好！今天我们俩给大家说段相声。".into(), "wave");
        push("b", "说哪段儿？".into(), "");
        push("a", format!("《{}》！", p.title), "");
    }
    for l in &p.lines {
        let (Some(who), Some(text)) = (l.first(), l.get(1)) else { continue };
        let who = match who.as_str() {
            "b" => "b",
            "ab" | "both" => "ab",
            _ => "a",
        };
        let flags = l.get(2).map(String::as_str).unwrap_or("");
        let parts = chunks(text);
        let n = parts.len();
        for (i, part) in parts.into_iter().enumerate() {
            // The laugh and the action belong to the end of the line.
            push(who, part, if i + 1 == n { flags } else { "" });
        }
    }
    push("ab", if en { "Thank you, you've been great!" } else { "谢谢大家！" }.into(), "wave clap");

    let mut out = vec![];
    let mut t = 0u64;
    for (who, text, flags) in raw {
        let ms = read_ms(&text);
        let words: Vec<&str> = flags.split_whitespace().collect();
        let fx = if words.contains(&"clap") {
            "clap"
        } else if words.contains(&"laugh") {
            "laugh"
        } else {
            ""
        };
        let act = words.iter().find(|w| ACTIONS.contains(w)).map(|w| w.to_string());
        out.push(Line { who, text, at: t, ms, fx, act });
        t += ms + BEAT + if fx == "laugh" { LAUGH } else { 0 };
    }
    let end = out.last().map(|l| l.at + l.ms + 600).unwrap_or(0);
    (out, end)
}

/// The show's length for a piece.
pub fn length(p: &Piece) -> u64 {
    timeline(p, crate::i18n::is_en()).1
}

impl World {
    /// Re-read the user's folder (cheap), so a file dropped in works right away.
    pub fn reload_xs(&mut self, home: &Path) {
        let mut warn = vec![];
        self.t.xs = load(home, crate::i18n::is_en(), &mut warn);
    }

    /// Right-click → 游戏 → 说相声, `/pet xiangsheng [piece]`.
    pub fn request_xs(&mut self, piece: Option<&str>, pet: &str, now: u64, views: &[PetView], db: &crate::db::Db) -> Result<Vec<Out>, String> {
        if self.frozen {
            return Err(crate::tr!("有会话在等你，先处理它", "A session is waiting for you — handle it first"));
        }
        if self.focus {
            return Err(crate::tr!("专注模式中，先关掉专注模式", "Focus mode is on — turn it off first"));
        }
        let a = views.iter().find(|v| v.id == pet).or_else(|| views.first()).cloned().ok_or_else(|| crate::tr!("找不到这只宠物", "Can't find that pet"))?;
        let b = Self::duo_partner(views, &a.id)
            .cloned()
            .ok_or_else(|| crate::tr!("说相声得有两只宠物，等另一边上线吧", "A comedy duo needs two pets — wait for the other one to come online"))?;
        for p in [&a, &b] {
            if self.scripts.contains_key(&p.id) {
                return Err(crate::tr!("它们正忙着呢，等一下", "They're busy — hang on"));
            }
        }
        let mut out = vec![];
        self.play_xs(piece, &a, &b, now, db, views, &mut out)?;
        Ok(out)
    }

    /// Two pets idle for a while sometimes perform a piece on their own.
    pub(crate) fn try_xs(&mut self, now: u64, views: &[PetView], db: &crate::db::Db, out: &mut Vec<Out>) {
        let (Some(a), Some(b)) = (Self::camp_pet(views, "claude-code").cloned(), Self::camp_pet(views, "codex").cloned()) else { return };
        let idle = |v: &PetView| matches!(v.state, State::Idle | State::Done) && now.saturating_sub(v.since) >= 300_000;
        if !idle(&a) || !idle(&b) || self.scripts.contains_key(&a.id) || self.scripts.contains_key(&b.id) {
            return;
        }
        if !self.under_cap(&a.id, now) || !self.under_cap(&b.id, now) || self.rand() >= 0.08 {
            return;
        }
        let ck = ("xs".to_string(), "pair".to_string());
        if self.cooldowns.get(&ck).is_some_and(|t| now < *t) {
            return;
        }
        self.cooldowns.insert(ck, now + 40 * 60_000);
        let (anchor, mover) = if self.rand() < 0.5 { (a, b) } else { (b, a) };
        let _ = self.play_xs(None, &anchor, &mover, now, db, views, out);
    }

    /// `anchor` stays and plays 逗哏; `mover` walks over and plays 捧哏.
    #[allow(clippy::too_many_arguments)]
    fn play_xs(&mut self, piece: Option<&str>, anchor: &PetView, mover: &PetView, now: u64, db: &crate::db::Db, views: &[PetView], out: &mut Vec<Out>) -> Result<(), String> {
        let p = match piece {
            Some(id) => self.t.xs.iter().find(|p| p.id == id || p.title == id).cloned().ok_or_else(|| crate::tr!("没有这段：{id}", "No such piece: {id}"))?,
            None => {
                // On their own the pets keep it short; long scripts are for when you ask.
                let fresh: Vec<Piece> =
                    self.t.xs.iter().filter(|p| length(p) <= AUTO_MAX).filter(|p| self.cooldowns.get(&("xs".to_string(), p.id.clone())).is_none_or(|t| now >= *t)).cloned().collect();
                let pool = if fresh.is_empty() { self.t.xs.iter().filter(|p| p.lang.is_none()).cloned().collect() } else { fresh };
                self.pick(&pool).cloned().ok_or_else(|| crate::tr!("还没有相声段子", "No comedy pieces yet"))?
            }
        };
        self.cooldowns.insert(("xs".to_string(), p.id.clone()), now + REPEAT);
        let (lines, ms) = timeline(&p, crate::i18n::is_en());
        let lang = p.lang.clone().unwrap_or_else(|| crate::i18n::code().to_string());
        for v in [anchor, mover] {
            self.recent.entry(v.id.clone()).or_default().push_back(now);
            let moved = v.id == mover.id;
            self.scripts.insert(v.id.clone(), Script { rule: "xs".into(), until: now + DUO_WALK + ms + 800, during_attention: false, moved });
            db.log_interaction(now, &v.id, "xiangsheng", false);
        }
        out.push(Out::Control(Control::Duo { mover: mover.id.clone(), anchor: anchor.id.clone(), gap: GAP }));
        out.push(Out::Ws(crate::msg(
            "xs",
            &json!({ "a": anchor.id, "b": mover.id, "id": p.id, "title": p.title, "walk": DUO_WALK, "ms": ms, "lines": lines, "voice": self.sounds, "lang": lang }),
        )));
        self.bond(db, views, 1, out);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::balance::Tables;
    use crate::config::Config;
    use crate::db::Db;
    use crate::hub::Hub;
    use crate::pack::Library;
    use crate::protocol::{Detail, Event, EventKind};

    #[test]
    fn builtin_pieces_parse_in_both_languages() {
        for en in [false, true] {
            let v = builtin(en);
            assert!(v.len() >= 5, "{en}: {}", v.len());
            for p in &v {
                assert!(p.lines.iter().all(|l| l.len() >= 2 && matches!(l[0].as_str(), "a" | "b" | "ab")), "{}", p.id);
                let (lines, ms) = timeline(p, en);
                assert_eq!(lines.first().unwrap().who, "a");
                assert_eq!(lines.last().unwrap().fx, "clap");
                // Lines never overlap and a show stays under three minutes.
                assert!(lines.windows(2).all(|w| w[1].at >= w[0].at + w[0].ms));
                assert!(ms < 180_000, "{}: {ms}", p.id);
            }
        }
    }

    #[test]
    fn plain_scripts_with_jia_and_yi() {
        let t = "《测试段子》\n\n甲：今天我给您说段贯口。\n乙：好啊。\n甲：蒸羊羔、蒸熊掌、蒸鹿尾儿、烧花鸭、烧雏鸡、烧子鹅、卤猪、卤鸭、酱鸡、腊肉、松花小肚儿、晾肉、香肠儿、什锦苏盘、熏鸡白肚儿……\n（观众笑）\n乙：（拍桌）去你的！\n接着说。\n";
        let p = parse_txt(t, "file").unwrap();
        assert_eq!(p.title, "测试段子");
        assert_eq!(p.id, "txt:file");
        assert_eq!(p.lang.as_deref(), Some("zh"));
        assert_eq!(p.lines.len(), 4);
        assert_eq!(p.lines[2][2], "laugh");
        assert_eq!(p.lines[3], vec!["b".to_string(), "去你的！接着说。".to_string(), String::new()]);
        // The long 贯口 becomes several bubbles; the laugh comes after the last one.
        let (lines, _) = timeline(&p, true);
        assert_eq!(lines[0].text, "大家好！今天我们俩给大家说段相声。");
        assert!(lines.iter().all(|l| l.text.chars().count() <= CHUNK + 2));
        assert!(lines.iter().filter(|l| l.text.contains("羊羔") || l.text.contains("白肚")).count() >= 2);
        assert!(lines.iter().any(|l| l.text.contains("白肚") && l.fx == "laugh"));
        assert!(parse_txt("随便写点什么", "x").is_none());
    }

    #[test]
    fn gbk_scripts_are_read() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("xiangsheng")).unwrap();
        let (gbk, _, _) = encoding_rs::GBK.encode("甲：你好。\r\n乙：好。\r\n");
        std::fs::write(dir.path().join("xiangsheng/老段子.txt"), &gbk).unwrap();
        let v = load(dir.path(), false, &mut vec![]);
        let p = v.iter().find(|p| p.id == "txt:老段子").unwrap();
        assert_eq!(p.lines[0][1], "你好。");
    }

    #[test]
    fn longer_lines_stay_up_longer() {
        assert!(read_ms("我负责最重要的环节，您听好了，这可是基本功。") > read_ms("嗯。"));
        assert_eq!(read_ms("Mm."), 1500);
    }

    #[test]
    fn user_pieces_replace_builtins_by_id() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("xiangsheng")).unwrap();
        std::fs::write(dir.path().join("xiangsheng/mine.yaml"), "zh:\n  - id: ai\n    title: 新的\n    lines:\n      - [a, 你好]\n      - [b, 好]\n").unwrap();
        std::fs::write(dir.path().join("xiangsheng/bad.yaml"), "zh: [").unwrap();
        let mut warn = vec![];
        let v = load(dir.path(), false, &mut warn);
        assert_eq!(v.iter().filter(|p| p.id == "ai").count(), 1);
        assert_eq!(v.iter().find(|p| p.id == "ai").unwrap().title, "新的");
        assert_eq!(warn.len(), 1);
    }

    #[test]
    fn request_walks_over_and_busies_both_pets() {
        let mut w = World::new(Tables::builtin(), 5);
        let mut h = Hub::new(Config::default(), Library::default());
        let db = Db::in_memory();
        let t0 = 1_793_600_000_000;
        // One pet: refused.
        let e = |s: &str, id: &str| Event { v: 1, source: s.into(), session: id.into(), project: "/w/p".into(), event: EventKind::SessionStart, sub: false, detail: Detail::default(), pid: 0, ts: t0 };
        h.apply(&e("claude-code", "a"), t0);
        assert!(w.request_xs(None, "claude-code", t0, &h.pets(), &db).is_err());
        h.apply(&e("codex", "b"), t0);
        let views = h.pets();
        let out = w.request_xs(Some("waiting"), "claude-code", t0, &views, &db).unwrap();
        assert!(out.iter().any(|o| matches!(o, Out::Control(Control::Duo { mover, anchor, .. }) if mover == "codex" && anchor == "claude-code")));
        let xs = out
            .iter()
            .find_map(|o| match o {
                Out::Ws(s) => serde_json::from_str::<serde_json::Value>(s).ok().filter(|v| v["type"] == "xs"),
                _ => None,
            })
            .unwrap()["data"]
            .clone();
        assert_eq!(xs["id"], "waiting");
        assert!(xs["lines"].as_array().unwrap().len() > 10);
        assert!(w.request_xs(None, "codex", t0 + 10, &views, &db).is_err());
        assert!(w.request_xs(Some("nope"), "codex", t0 + 10, &views, &Db::in_memory()).is_err());
    }
}
