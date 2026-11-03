//! User actions shared by /pet, the HTTP API, the tray and the right-click
//! menu: switch packs, favorites, routing mode, hide one pet.

use crate::config::{self, Config, Mode};
use crate::server::Shared;
use crate::Control;
use serde_yaml::Value;

/// Reload config.yaml into the hub. Keeps the old config on a parse error.
pub fn reload_config(sh: &Shared) {
    let res = Config::load(&sh.paths.config());
    let mut hub = sh.hub.lock().unwrap();
    let mut lang_changed = false;
    match res {
        Ok(c) => {
            lang_changed = crate::i18n::set_from(&c.language);
            hub.cfg = c;
            hub.cfg_error = None;
        }
        Err(e) => {
            if hub.cfg_error.as_deref() != Some(e.as_str()) {
                sh.paths.log(&e);
            }
            hub.cfg_error = Some(e);
        }
    }
    drop(hub);
    if lang_changed {
        // Data files and packs carry both languages; reload them.
        sh.world.lock().unwrap().t = crate::balance::Tables::load(&sh.paths.home);
        crate::rescan_packs(sh);
        let _ = sh.ws_tx.send(crate::msg("lang", &serde_json::json!({ "lang": crate::i18n::code() })));
    }
    let _ = sh.control.send(Control::ConfigChanged);
    sh.urgent.notify_one();
}

/// `language:` in config.yaml (auto | zh | en).
pub fn set_language(sh: &Shared, lang: &str) -> Result<String, String> {
    let l = match lang.to_ascii_lowercase().as_str() {
        "auto" => "auto",
        "zh" | "cn" | "中文" => "zh",
        "en" | "english" => "en",
        _ => return Err(crate::tr!("语言只能是 auto、zh、en", "Language must be auto, zh or en")),
    };
    save(sh, |m| {
        m.insert("language".into(), Value::String(l.into()));
        Ok(())
    })?;
    Ok(crate::tr!("界面语言：{l}", "UI language: {l}"))
}

fn save(sh: &Shared, f: impl FnOnce(&mut serde_yaml::Mapping) -> Result<(), String>) -> Result<(), String> {
    config::edit(&sh.paths.config(), f)?;
    reload_config(sh);
    Ok(())
}

/// Human name of a switch target, for replies.
pub fn target_name(target: &str) -> String {
    match target {
        "claude-code" => crate::tr!("Claude Code 阵营", "Claude Code team"),
        "codex" => crate::tr!("Codex 阵营", "Codex team"),
        t => match t.split_once(':') {
            Some(("project", p)) => crate::tr!("项目 {p}", "Project {p}"),
            Some(("rule", r)) => crate::tr!("规则 {r}", "Rule {r}"),
            _ => t.into(),
        },
    }
}

/// Use `pack` for `target` (`claude-code`, `codex`, `project:<pattern>`, `rule:<id>`).
pub fn select(sh: &Shared, target: &str, pack: &str) -> Result<String, String> {
    if sh.hub.lock().unwrap().lib.get(pack).is_none() {
        return Err(crate::tr!("没有叫 {pack} 的宠物包", "No pet pack named {pack}"));
    }
    let pack_v = Value::String(pack.into());
    match target {
        "claude-code" | "codex" => save(sh, |m| {
            config::child(m, "selection").insert(target.into(), pack_v);
            Ok(())
        })?,
        t => match t.split_once(':') {
            Some(("project", pat)) if !pat.is_empty() => save(sh, |m| {
                config::child(config::child(m, "selection"), "projects").insert(pat.into(), pack_v);
                Ok(())
            })?,
            Some(("rule", id)) => save(sh, |m| {
                let rules = m.get_mut("pets").and_then(Value::as_sequence_mut).ok_or_else(|| crate::tr!("config.yaml 里没有 pets 规则", "config.yaml has no pets rules"))?;
                let rule = rules
                    .iter_mut()
                    .find(|r| r.get("id").and_then(Value::as_str) == Some(id))
                    .and_then(Value::as_mapping_mut)
                    .ok_or_else(|| crate::tr!("找不到规则 {id}", "No rule {id}"))?;
                rule.insert("pack".into(), pack_v);
                Ok(())
            })?,
            _ => return Err(crate::tr!("不认识的切换目标 {target}", "Unknown switch target {target}")),
        },
    }
    Ok(crate::tr!("{}现在用 {pack}。", "{} now uses {pack}.", target_name(target)))
}

/// Remove a per-project override; the project goes back to its camp's pet.
pub fn clear_project(sh: &Shared, pattern: &str) -> Result<String, String> {
    save(sh, |m| {
        config::child(config::child(m, "selection"), "projects").remove(pattern);
        Ok(())
    })?;
    Ok(crate::tr!("项目 {pattern} 恢复使用阵营宠物。", "Project {pattern} is back to its team's pet."))
}

pub fn set_favorite(sh: &Shared, pack: &str, on: bool) -> Result<(), String> {
    save(sh, |m| {
        let sel = config::child(m, "selection");
        let k = Value::String("favorites".into());
        if !sel.get(&k).is_some_and(Value::is_sequence) {
            sel.insert(k.clone(), Value::Sequence(vec![]));
        }
        let favs = sel.get_mut(&k).unwrap().as_sequence_mut().unwrap();
        favs.retain(|v| v.as_str() != Some(pack));
        if on {
            favs.push(Value::String(pack.into()));
        }
        Ok(())
    })
}

pub fn set_mode(sh: &Shared, mode: &str) -> Result<String, String> {
    let m = Mode::parse(mode).ok_or_else(|| crate::tr!("没有 {mode} 模式。可选：per-camp、single、per-rule、per-session", "No mode {mode}. Options: per-camp, single, per-rule, per-session"))?;
    save(sh, |map| {
        map.insert("mode".into(), m.as_str().into());
        Ok(())
    })?;
    Ok(crate::tr!("路由模式改为 {}。", "Routing mode set to {}.", m.as_str()))
}

/// Pet size: s / m / l.
pub fn set_size(sh: &Shared, size: &str) -> Result<String, String> {
    let s = match size {
        "s" | "S" => "s",
        "m" | "M" => "m",
        "l" | "L" => "l",
        _ => return Err(crate::tr!("大小只能是 s、m、l", "Size must be s, m or l")),
    };
    save(sh, |m| {
        config::child(m, "features").insert("size".into(), Value::String(s.into()));
        Ok(())
    })?;
    Ok(crate::tr!("宠物大小改为 {}。", "Pet size set to {}.", s.to_uppercase()))
}

/// Health reminders, as the gallery edits them.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Reminders {
    pub kegel: Vec<String>,
    pub kegel_reps: u32,
    pub water: bool,
    pub breathe: bool,
}

pub fn reminders(sh: &Shared) -> Reminders {
    let f = &sh.hub.lock().unwrap().cfg.features;
    Reminders { kegel: f.kegel.clone(), kegel_reps: f.kegel_reps, water: f.water, breathe: f.breathe }
}

/// Clean "HH:MM" list: valid, sorted, no duplicates.
pub fn clean_times(times: &[String]) -> Result<Vec<String>, String> {
    let mut out: Vec<u32> = vec![];
    for t in times {
        let m = crate::world::parse_hm(t).ok_or_else(|| crate::tr!("时间格式不对：{t}（例：10:30）", "Bad time: {t} (e.g. 10:30)"))?;
        if !out.contains(&m) {
            out.push(m);
        }
    }
    out.sort();
    Ok(out.into_iter().map(|m| format!("{:02}:{:02}", m / 60, m % 60)).collect())
}

pub fn set_reminders(sh: &Shared, r: &Reminders) -> Result<String, String> {
    let times = clean_times(&r.kegel)?;
    let reps = r.kegel_reps.clamp(1, 30);
    save(sh, |m| {
        let f = config::child(m, "features");
        f.insert("kegel".into(), Value::Sequence(times.iter().map(|t| Value::String(t.clone())).collect()));
        f.insert("kegel_reps".into(), Value::Number(reps.into()));
        f.insert("water".into(), Value::Bool(r.water));
        f.insert("breathe".into(), Value::Bool(r.breathe));
        Ok(())
    })?;
    Ok(if times.is_empty() {
        crate::tr!("提醒已保存，提肛提醒关闭。", "Saved. Kegel reminders are off.")
    } else {
        crate::tr!("提醒已保存：每天 {} 提肛。", "Saved: kegels every day at {}.", times.join(crate::i18n::t("、", ", ")))
    })
}

/// Start a guided kegel set now on `pet` (default: the main pet).
pub fn kegel_now(sh: &Shared, pet: Option<&str>) -> Result<String, String> {
    let (views, reps) = {
        let hub = sh.hub.lock().unwrap();
        (hub.pets(), hub.cfg.features.kegel_reps)
    };
    let m = crate::world::kegel_script(&views, pet, reps).ok_or_else(|| crate::tr!("现在没有显示的宠物", "No pet is showing right now"))?;
    let _ = sh.ws_tx.send(m);
    Ok(crate::tr!("开始提肛，跟着宠物做 {reps} 组。", "Kegel set started: {reps} reps with your pet."))
}

/// Focus mode: status and reminders only; no interactions, sounds or games.
pub fn set_focus(sh: &Shared, on: bool) -> Result<String, String> {
    save(sh, |m| {
        config::child(m, "features").insert("focus_mode".into(), Value::Bool(on));
        Ok(())
    })?;
    Ok(if on {
        crate::tr!("专注模式已开启：只保留状态显示和提醒。", "Focus mode on: only status and reminders.")
    } else {
        crate::tr!("专注模式已关闭。", "Focus mode off.")
    })
}

pub fn hide_pet(sh: &Shared, id: &str) {
    sh.hub.lock().unwrap().hidden_pets.insert(id.to_string());
    sh.urgent.notify_one();
}

pub fn show_all(sh: &Shared) {
    {
        let mut hub = sh.hub.lock().unwrap();
        hub.hidden = false;
        hub.hidden_pets.clear();
    }
    let _ = sh.control.send(Control::Show);
    sh.urgent.notify_one();
}

pub fn hide_all(sh: &Shared) {
    sh.hub.lock().unwrap().hidden = true;
    let _ = sh.control.send(Control::Hide);
}

/// Installed favorites, in order.
fn favorites(sh: &Shared) -> Vec<String> {
    let hub = sh.hub.lock().unwrap();
    hub.cfg.selection.favorites.iter().filter(|f| hub.lib.get(f).is_some()).cloned().collect()
}

/// Ctrl+Alt+P: next favorite for the camp that had the latest event.
pub fn cycle_favorite(sh: &Shared) -> Result<String, String> {
    let favs = favorites(sh);
    if favs.len() < 2 {
        return Err(crate::tr!("收藏里至少要有两只宠物才能切换（在图鉴里点 ★）", "You need at least two favorites to cycle (tap ★ in the Gallery)"));
    }
    let (target, current) = {
        let hub = sh.hub.lock().unwrap();
        let target = hub.current_target();
        let current = hub.pets().into_iter().find(|v| v.target == target).map(|v| v.pack).unwrap_or_default();
        (target, current)
    };
    let next = favs.iter().position(|f| *f == current).map(|i| favs[(i + 1) % favs.len()].clone()).unwrap_or_else(|| favs[0].clone());
    select(sh, &target, &next)
}

/// `selection.rotation`: daily-random picks a random favorite per camp each
/// day, weekday walks the favorites Monday to Sunday. Runs once per day.
pub fn rotate_daily(sh: &Shared, now: u64) -> Option<String> {
    let rotation = sh.hub.lock().unwrap().cfg.selection.rotation.clone();
    if rotation != "daily-random" && rotation != "weekday" {
        return None;
    }
    let today = crate::db::day_of(now);
    if sh.db.kv_get("rotation_day").as_deref() == Some(today.as_str()) {
        return None;
    }
    sh.db.kv_set("rotation_day", &today);
    let favs = favorites(sh);
    if favs.is_empty() {
        return None;
    }
    let weekday = crate::db::weekday(now) as usize; // 0 = Monday
    let mut done = vec![];
    for (i, camp) in ["claude-code", "codex"].iter().enumerate() {
        let pick = if rotation == "weekday" {
            favs[(weekday + i) % favs.len()].clone()
        } else {
            // Day-seeded so both camps get different but stable picks.
            let seed = today.bytes().fold(i as u64 * 7919 + 17, |h, b| h.wrapping_mul(31).wrapping_add(b as u64));
            favs[(seed % favs.len() as u64) as usize].clone()
        };
        if select(sh, camp, &pick).is_ok() {
            done.push(format!("{camp}: {pick}"));
        }
    }
    Some(done.join(", "))
}

/// `/pet switch [pack]`: list packs, or switch the current camp's pet.
pub fn switch_cmd(sh: &Shared, args: &[String]) -> String {
    let (ids, target, favs) = {
        let hub = sh.hub.lock().unwrap();
        let mut ids: Vec<String> = hub.lib.packs.keys().cloned().collect();
        ids.sort();
        (ids, hub.current_target(), hub.cfg.selection.favorites.clone())
    };
    match args.first() {
        None => crate::tr!(
            "可用宠物包：{}\n收藏：{}\n用法：/pet switch <包名>（切换{}）",
            "Pet packs: {}\nFavorites: {}\nUsage: /pet switch <pack> (switches {})",
            ids.join(crate::i18n::t("、", ", ")),
            if favs.is_empty() { crate::i18n::t("无", "none").to_string() } else { favs.join(crate::i18n::t("、", ", ")) },
            target_name(&target)
        ),
        Some(pack) => match select(sh, &target, pack) {
            Ok(m) => m,
            Err(e) => crate::tr!("{e}。可用：{}", "{e}. Available: {}", ids.join(crate::i18n::t("、", ", "))),
        },
    }
}
