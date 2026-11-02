//! Gameplay numbers (balance.yaml), fish species (fish.yaml) and interaction
//! rules (interactions.yaml). Built-in copies are compiled in; files in
//! ~/.familiar override them (balance.yaml is deep-merged key by key).
//!
//! Text fields may carry an English twin (`lines_en`, fish `name_en` /
//! `note_en`, rule `bubble_en`). In English mode the twin replaces the Chinese
//! value at load time; a missing twin keeps the Chinese text.

use serde::{Deserialize, Serialize};
use serde_yaml::Value;
use std::collections::BTreeMap;
use std::path::Path;

pub const BALANCE_YAML: &str = include_str!("../assets/balance.yaml");
pub const FISH_YAML: &str = include_str!("../assets/fish.yaml");
pub const INTERACTIONS_YAML: &str = include_str!("../assets/interactions.yaml");

#[derive(Debug, Clone, Deserialize)]
pub struct Balance {
    pub economy: Economy,
    pub fishing: Fishing,
    pub bugs: Bugs,
    pub tug: Tug,
    pub quips: Quips,
    pub interactions: InteractionLimits,
    pub work: Work,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Economy {
    pub daily_cap: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Odds {
    /// Upper bound in minutes; 0 = everything longer.
    pub under_min: u64,
    pub p: [u32; 4],
}

#[derive(Debug, Clone, Deserialize)]
pub struct Fishing {
    pub start_after_s: u64,
    pub min_turn_s: u64,
    pub coins: BTreeMap<String, i64>,
    pub clean_bonus: f64,
    pub hourly_soft_cap: u32,
    pub odds: Vec<Odds>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Bugs {
    pub per_tool_fail: u32,
    pub per_turn_error: u32,
    pub max: usize,
    pub ttl_min: u64,
    pub boss_hp: u32,
    pub coins_per: u32,
    pub coins: i64,
    pub boss_coins: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Tug {
    pub scoring: String,
    pub min_turns: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Quips {
    pub enabled: bool,
    pub min_s: u64,
    pub max_s: u64,
    pub no_repeat_min: u64,
    pub lines: Vec<String>,
    #[serde(default)]
    pub lines_en: Option<Vec<String>>,
}

impl Quips {
    /// Use `lines_en` in English mode when present; drop it either way.
    fn localize(&mut self, en: bool) {
        if let Some(l) = self.lines_en.take().filter(|l| !l.is_empty()) {
            if en {
                self.lines = l;
            }
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct InteractionLimits {
    pub per_pet_hour: usize,
    pub tick_ms: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Work {
    pub off_time: String,
}

pub const RARITIES: [&str; 4] = ["common", "rare", "epic", "legendary"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Fish {
    pub id: String,
    pub name: String,
    pub rarity: String,
    #[serde(default)]
    pub note: String,
    /// English twins from fish.yaml; consumed at load time (always `None` afterwards).
    #[serde(default, skip_serializing)]
    pub name_en: Option<String>,
    #[serde(default, alias = "desc_en", skip_serializing)]
    pub note_en: Option<String>,
}

impl Fish {
    /// Swap in the English twins when `en`; clear them either way.
    fn localize(mut self, en: bool) -> Fish {
        let (name_en, note_en) = (non_empty(self.name_en.take()), non_empty(self.note_en.take()));
        if en {
            if let Some(n) = name_en {
                self.name = n;
            }
            if let Some(n) = note_en {
                self.note = n;
            }
        }
        self
    }
}

#[derive(Debug, Clone, Deserialize)]
struct FishFile {
    fish: Vec<Fish>,
}

impl FishFile {
    fn pick(self, en: bool) -> Vec<Fish> {
        self.fish.into_iter().map(|f| f.localize(en)).collect()
    }
}

/// Bubble lines: one list for everyone, or one per camp.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum Lines {
    All(Vec<String>),
    PerCamp(BTreeMap<String, Vec<String>>),
}

impl Lines {
    pub fn for_camp(&self, camp: &str) -> &[String] {
        match self {
            Lines::All(v) => v,
            Lines::PerCamp(m) => m.get(camp).or_else(|| m.values().next()).map(|v| v.as_slice()).unwrap_or(&[]),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Rule {
    pub id: String,
    #[serde(default = "yes")]
    pub enabled: bool,
    pub on: String,
    #[serde(default = "other")]
    pub from: String,
    #[serde(default)]
    pub result: Option<String>,
    #[serde(default)]
    pub min_turn_s: u64,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub actor_state: Vec<String>,
    #[serde(default = "one")]
    pub chance: f64,
    #[serde(default)]
    pub cooldown: Option<String>,
    pub action: String,
    #[serde(default)]
    pub working_action: Option<String>,
    #[serde(default)]
    pub bubble: Option<Lines>,
    /// English bubbles (same shape); consumed at load time (always `None` afterwards).
    #[serde(default)]
    pub bubble_en: Option<Lines>,
    #[serde(default = "default_ms")]
    pub ms: u64,
    #[serde(default, rename = "move")]
    pub movement: Option<String>,
    #[serde(default)]
    pub during_attention: bool,
}

fn yes() -> bool {
    true
}
fn other() -> String {
    "other".into()
}
fn one() -> f64 {
    1.0
}
fn default_ms() -> u64 {
    3000
}

fn non_empty(s: Option<String>) -> Option<String> {
    s.filter(|s| !s.trim().is_empty())
}

impl Lines {
    fn is_empty(&self) -> bool {
        match self {
            Lines::All(v) => v.is_empty(),
            Lines::PerCamp(m) => m.values().all(|v| v.is_empty()),
        }
    }
}

impl Rule {
    /// Swap in `bubble_en` when `en`; clear it either way.
    fn localize(mut self, en: bool) -> Rule {
        if let Some(b) = self.bubble_en.take().filter(|b| !b.is_empty()) {
            if en {
                self.bubble = Some(b);
            }
        }
        self
    }

    pub fn cooldown_ms(&self) -> u64 {
        self.cooldown.as_deref().map(parse_duration).unwrap_or(0)
    }
}

/// "10m", "30s", "2h", "1500ms" -> milliseconds.
pub fn parse_duration(s: &str) -> u64 {
    let s = s.trim();
    let (num, mul) = if let Some(n) = s.strip_suffix("ms") {
        (n, 1)
    } else if let Some(n) = s.strip_suffix('s') {
        (n, 1000)
    } else if let Some(n) = s.strip_suffix('m') {
        (n, 60_000)
    } else if let Some(n) = s.strip_suffix('h') {
        (n, 3_600_000)
    } else {
        (s, 1000)
    };
    num.trim().parse::<f64>().map(|v| (v * mul as f64) as u64).unwrap_or(0)
}

#[derive(Debug, Clone, Deserialize)]
struct RuleFile {
    rules: Vec<Rule>,
}

impl RuleFile {
    fn pick(self, en: bool) -> Vec<Rule> {
        self.rules.into_iter().map(|r| r.localize(en)).collect()
    }
}

fn merge(base: &mut Value, over: Value) {
    match (base, over) {
        (Value::Mapping(b), Value::Mapping(o)) => {
            for (k, v) in o {
                match b.get_mut(&k) {
                    Some(slot) => merge(slot, v),
                    None => {
                        b.insert(k, v);
                    }
                }
            }
        }
        (slot, v) => *slot = v,
    }
}

/// Everything the world layer needs, plus load warnings for the log.
#[derive(Debug, Clone)]
pub struct Tables {
    pub balance: Balance,
    pub fish: Vec<Fish>,
    pub rules: Vec<Rule>,
    pub warnings: Vec<String>,
}

impl Tables {
    pub fn builtin() -> Tables {
        Tables::builtin_in(crate::i18n::is_en())
    }

    fn builtin_in(en: bool) -> Tables {
        let mut balance: Balance = serde_yaml::from_str(BALANCE_YAML).expect("built-in balance.yaml");
        balance.quips.localize(en);
        Tables {
            balance,
            fish: serde_yaml::from_str::<FishFile>(FISH_YAML).expect("built-in fish.yaml").pick(en),
            rules: serde_yaml::from_str::<RuleFile>(INTERACTIONS_YAML).expect("built-in interactions.yaml").pick(en),
            warnings: vec![],
        }
    }

    /// Built-ins overridden by `home/balance.yaml`, `home/fish.yaml` and
    /// `home/interactions/*.yaml`. A broken file is reported and skipped.
    pub fn load(home: &Path) -> Tables {
        Tables::load_in(home, crate::i18n::is_en())
    }

    fn load_in(home: &Path, en: bool) -> Tables {
        let mut t = Tables::builtin_in(en);
        if let Ok(text) = std::fs::read_to_string(home.join("balance.yaml")) {
            let mut base: Value = serde_yaml::from_str(BALANCE_YAML).unwrap();
            match serde_yaml::from_str::<Value>(&text) {
                Ok(over) => {
                    // A user list of quips without its own English twin must
                    // not be shadowed by the built-in English lines.
                    let user_quips = over.get("quips");
                    if user_quips.is_some_and(|q| q.get("lines").is_some() && q.get("lines_en").is_none()) {
                        if let Some(Value::Mapping(q)) = base.get_mut("quips") {
                            q.remove("lines_en");
                        }
                    }
                    merge(&mut base, over);
                    match serde_yaml::from_value::<Balance>(base) {
                        Ok(mut b) => {
                            b.quips.localize(en);
                            t.balance = b
                        }
                        Err(e) => t.warnings.push(format!("balance.yaml: {e}")),
                    }
                }
                Err(e) => t.warnings.push(format!("balance.yaml: {e}")),
            }
        }
        if let Ok(text) = std::fs::read_to_string(home.join("fish.yaml")) {
            match serde_yaml::from_str::<FishFile>(&text) {
                Ok(f) if RARITIES.iter().all(|r| f.fish.iter().any(|x| x.rarity == *r)) => t.fish = f.pick(en),
                Ok(_) => t.warnings.push("fish.yaml: 四种稀有度每种至少要有一条鱼".into()),
                Err(e) => t.warnings.push(format!("fish.yaml: {e}")),
            }
        }
        if let Ok(rd) = std::fs::read_dir(home.join("interactions")) {
            let mut files: Vec<_> = rd.flatten().map(|e| e.path()).collect();
            files.sort();
            for f in files.into_iter().filter(|p| p.extension().is_some_and(|x| x == "yaml" || x == "yml")) {
                match std::fs::read_to_string(&f).map_err(|e| e.to_string()).and_then(|s| {
                    serde_yaml::from_str::<RuleFile>(&s).map_err(|e| e.to_string())
                }) {
                    Ok(rf) => {
                        for r in rf.pick(en) {
                            t.rules.retain(|x| x.id != r.id);
                            t.rules.push(r);
                        }
                    }
                    Err(e) => t.warnings.push(format!("{}: {e}", f.display())),
                }
            }
        }
        t.rules.retain(|r| r.enabled);
        t
    }

    pub fn fish_of(&self, rarity: &str) -> Vec<&Fish> {
        self.fish.iter().filter(|f| f.rarity == rarity).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_parse() {
        let t = Tables::builtin();
        assert_eq!(t.balance.economy.daily_cap, 300);
        assert_eq!(t.fish.len(), 12);
        assert_eq!(t.rules.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), ["taunt", "comfort", "applaud", "urge"]);
        assert_eq!(t.rules[0].cooldown_ms(), 600_000);
        assert!(t.rules[3].during_attention);
        assert_eq!(t.rules[0].bubble.as_ref().unwrap().for_camp("codex").len(), 3);
    }

    #[test]
    fn english_twins_cover_every_builtin_text() {
        let raw: Balance = serde_yaml::from_str(BALANCE_YAML).unwrap();
        assert_eq!(raw.quips.lines_en.as_ref().map(|l| l.len()), Some(raw.quips.lines.len()));
        let fish = serde_yaml::from_str::<FishFile>(FISH_YAML).unwrap().fish;
        assert!(fish.iter().all(|f| f.name_en.is_some() && f.note_en.is_some()), "every fish has English");
        let rules = serde_yaml::from_str::<RuleFile>(INTERACTIONS_YAML).unwrap().rules;
        assert!(rules.iter().all(|r| r.bubble.is_none() || r.bubble_en.is_some()), "every bubble has English");

        let zh = Tables::builtin_in(false);
        assert_eq!(zh.fish[0].name, "分号小鱼");
        assert_eq!(zh.balance.quips.lines[0], "又改 {file} 了，第 {n} 次");
        assert_eq!(zh.rules[0].bubble.as_ref().unwrap().for_camp("codex")[0], "翻车了吧～");
        let en = Tables::builtin_in(true);
        assert_eq!(en.fish[0].name, "Semicolon Minnow");
        assert!(en.balance.quips.lines.iter().all(|l| l.is_ascii()));
        assert!(en.rules.iter().all(|r| r.bubble.as_ref().unwrap().for_camp("codex").iter().all(|l| l.is_ascii())));
    }

    #[test]
    fn user_chinese_overrides_fall_back_to_chinese_in_english() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("balance.yaml"), "quips: { lines: [\"自己的吐槽\"] }\n").unwrap();
        std::fs::write(
            dir.path().join("fish.yaml"),
            "fish:\n  - { id: a, name: 甲鱼, rarity: common }\n  - { id: b, name: 乙鱼, rarity: rare, name_en: Bee Fish }\n  - { id: c, name: 丙鱼, rarity: epic }\n  - { id: d, name: 丁鱼, rarity: legendary }\n",
        )
        .unwrap();
        std::fs::create_dir(dir.path().join("interactions")).unwrap();
        std::fs::write(dir.path().join("interactions/mine.yaml"), "rules:\n  - { id: taunt, on: turn_done, action: taunt, bubble: [\"哼\"] }\n").unwrap();
        let t = Tables::load_in(dir.path(), true);
        assert!(t.warnings.is_empty(), "{:?}", t.warnings);
        assert_eq!(t.balance.quips.lines, ["自己的吐槽"]);
        assert_eq!(t.fish[0].name, "甲鱼");
        assert_eq!(t.fish[1].name, "Bee Fish");
        let taunt = t.rules.iter().find(|r| r.id == "taunt").unwrap();
        assert_eq!(taunt.bubble.as_ref().unwrap().for_camp("codex"), ["哼"]);
        let comfort = t.rules.iter().find(|r| r.id == "comfort").unwrap();
        assert!(comfort.bubble.as_ref().unwrap().for_camp("codex")[0].is_ascii(), "built-in rule still English");
    }

    #[test]
    fn user_files_override() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("balance.yaml"), "economy: { daily_cap: 50 }\nfishing: { clean_bonus: 2 }\n").unwrap();
        std::fs::create_dir(dir.path().join("interactions")).unwrap();
        std::fs::write(
            dir.path().join("interactions/mine.yaml"),
            "rules:\n  - { id: taunt, enabled: false, on: turn_done, action: taunt }\n  - { id: hi, on: state, state: done, action: wave }\n",
        )
        .unwrap();
        let t = Tables::load(dir.path());
        assert!(t.warnings.is_empty(), "{:?}", t.warnings);
        assert_eq!(t.balance.economy.daily_cap, 50);
        assert_eq!(t.balance.fishing.clean_bonus, 2.0);
        assert_eq!(t.balance.fishing.hourly_soft_cap, 10, "untouched keys keep defaults");
        assert!(t.rules.iter().all(|r| r.id != "taunt"), "disabled by user");
        assert!(t.rules.iter().any(|r| r.id == "hi"));
    }

    #[test]
    fn durations() {
        assert_eq!(parse_duration("10m"), 600_000);
        assert_eq!(parse_duration("30s"), 30_000);
        assert_eq!(parse_duration("1500ms"), 1500);
        assert_eq!(parse_duration("2h"), 7_200_000);
    }
}
