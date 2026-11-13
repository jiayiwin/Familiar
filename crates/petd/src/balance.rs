//! The world layer's numbers (balance.yaml), the sweets and fruit the pets eat (treats.yaml) and interaction
//! rules (interactions.yaml). Built-in copies are compiled in; files in
//! ~/.rillbuddy override them (balance.yaml is deep-merged key by key).
//!
//! Text fields may carry an English twin (`lines_en`, a treat's `name_en` /
//! `note_en`, rule `bubble_en`). In English mode the twin replaces the Chinese
//! value at load time; a missing twin keeps the Chinese text.

use serde::{Deserialize, Serialize};
use serde_yaml::Value;
use std::collections::BTreeMap;
use std::path::Path;

pub const BALANCE_YAML: &str = include_str!("../assets/balance.yaml");
pub const FISH_YAML: &str = include_str!("../assets/treats.yaml");
pub const INTERACTIONS_YAML: &str = include_str!("../assets/interactions.yaml");

#[derive(Debug, Clone, Deserialize)]
pub struct Balance {
    pub fishing: Fishing,
    pub bugs: Bugs,
    pub quips: Quips,
    pub interactions: InteractionLimits,
    pub work: Work,
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
    pub odds: Vec<Odds>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Bugs {
    pub per_tool_fail: u32,
    pub per_turn_error: u32,
    pub max: usize,
    pub ttl_min: u64,
    pub boss_hp: u32,
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
    /// How it is drawn (wrapped, drop, bean, star, cloud, lolly, cube, moon, slice, berry, grape, cherry, peach, leaf, flower) and in which two colours.
    #[serde(default)]
    pub shape: String,
    #[serde(default)]
    pub colors: Vec<String>,
    /// Where it comes from: "agent" (a finished turn), "kegel", "break"; none = from anywhere.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    /// [from, to): the hours of the day it turns up in (may wrap past midnight).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hours: Option<[u8; 2]>,
    /// [from, to]: the months it turns up in (may wrap past December).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub months: Option<[u8; 2]>,
    /// English twins from treats.yaml; consumed at load time (always `None` afterwards).
    #[serde(default, skip_serializing)]
    pub name_en: Option<String>,
    #[serde(default, alias = "desc_en", skip_serializing)]
    pub note_en: Option<String>,
}

impl Fish {
    /// Whether this one can turn up now: `from` is where the catch comes from
    /// ("agent" | "kegel" | "break"), `hour` and `month` the local time.
    pub fn fits(&self, from: &str, hour: u8, month: u8) -> bool {
        let within = |lo: u8, hi: u8, v: u8| {
            if lo <= hi {
                v >= lo && v <= hi
            } else {
                v >= lo || v <= hi
            }
        };
        self.from.as_deref().is_none_or(|f| f == from)
            && self.hours.is_none_or(|[a, b]| {
                if a <= b {
                    hour >= a && hour < b
                } else {
                    hour >= a || hour < b
                }
            })
            && self.months.is_none_or(|[a, b]| within(a, b, month))
    }

    /// Tied to a moment (a source, an hour, a season): such a one is likelier than the everyday ones when its moment comes.
    pub fn special(&self) -> bool {
        self.from.is_some() || self.hours.is_some() || self.months.is_some()
    }

    /// Swap in the English twins when `en`; clear them either way.
    fn localize(mut self, en: bool) -> Fish {
        let (name_en, note_en) = (
            non_empty(self.name_en.take()),
            non_empty(self.note_en.take()),
        );
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
    /// `fish:` is what the file's list was called before the pets ate sweets.
    #[serde(alias = "fish")]
    treats: Vec<Fish>,
}

impl FishFile {
    fn pick(self, en: bool) -> Vec<Fish> {
        self.treats.into_iter().map(|f| f.localize(en)).collect()
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
            Lines::PerCamp(m) => m
                .get(camp)
                .or_else(|| m.values().next())
                .map(|v| v.as_slice())
                .unwrap_or(&[]),
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
    num.trim()
        .parse::<f64>()
        .map(|v| (v * mul as f64) as u64)
        .unwrap_or(0)
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
        let mut balance: Balance =
            serde_yaml::from_str(BALANCE_YAML).expect("built-in balance.yaml");
        balance.quips.localize(en);
        Tables {
            balance,
            fish: serde_yaml::from_str::<FishFile>(FISH_YAML)
                .expect("built-in treats.yaml")
                .pick(en),
            rules: serde_yaml::from_str::<RuleFile>(INTERACTIONS_YAML)
                .expect("built-in interactions.yaml")
                .pick(en),
            warnings: vec![],
        }
    }

    /// Built-ins overridden by `home/balance.yaml`, `home/treats.yaml` and
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
                    if user_quips
                        .is_some_and(|q| q.get("lines").is_some() && q.get("lines_en").is_none())
                    {
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
        // Before the pets ate sweets the file was fish.yaml: it is still read, until it is renamed.
        let own = std::fs::read_to_string(home.join("treats.yaml")).ok().or_else(|| {
            let old = std::fs::read_to_string(home.join("fish.yaml")).ok()?;
            t.warnings.push("fish.yaml 现在叫 treats.yaml：照旧读取，改个名就不再提示 / fish.yaml is now treats.yaml: still read, rename it to stop this note".into());
            Some(old)
        });
        if let Some(text) = own {
            match serde_yaml::from_str::<FishFile>(&text) {
                // Each rarity needs one that can turn up whatever the hour, the season or the source.
                Ok(f) if RARITIES.iter().all(|r| f.treats.iter().any(|x| x.rarity == *r && !x.special())) => t.fish = f.pick(en),
                Ok(_) => t.warnings.push("treats.yaml: 四种稀有度每种至少要有一种随时都有的（不写 from / hours / months）".into()),
                Err(e) => t.warnings.push(format!("treats.yaml: {e}")),
            }
        }
        if let Ok(rd) = std::fs::read_dir(home.join("interactions")) {
            let mut files: Vec<_> = rd.flatten().map(|e| e.path()).collect();
            files.sort();
            for f in files
                .into_iter()
                .filter(|p| p.extension().is_some_and(|x| x == "yaml" || x == "yml"))
            {
                match std::fs::read_to_string(&f)
                    .map_err(|e| e.to_string())
                    .and_then(|s| serde_yaml::from_str::<RuleFile>(&s).map_err(|e| e.to_string()))
                {
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
        assert_eq!(t.balance.bugs.boss_hp, 10);
        assert_eq!(t.fish.len(), 22);
        // Whatever the hour, the season or the source, every rarity has something to give.
        for r in RARITIES {
            assert!(t.fish.iter().any(|f| f.rarity == r && !f.special()), "{r}");
        }
        let by = |id: &str| t.fish.iter().find(|f| f.id == id).unwrap();
        assert!(
            by("nullptr").fits("agent", 23, 6)
                && by("nullptr").fits("agent", 2, 6)
                && !by("nullptr").fits("agent", 12, 6),
            "the moon gummy is a night thing"
        );
        assert!(
            by("snow").fits("agent", 12, 1)
                && by("snow").fits("agent", 12, 12)
                && !by("snow").fits("agent", 12, 7),
            "first snow is a winter thing"
        );
        assert!(
            by("plum").fits("kegel", 12, 6)
                && !by("plum").fits("agent", 12, 6)
                && by("bean").fits("agent", 12, 6)
                && !by("bean").fits("break", 12, 6)
        );
        assert!(
            t.fish
                .iter()
                .all(|f| !f.shape.is_empty() && f.colors.len() == 2),
            "each can be drawn"
        );
        // No built-in rules: what the pets do together is acted out (think.rs). The file is for yours.
        assert!(t.rules.is_empty());
    }

    #[test]
    fn english_twins_cover_every_builtin_text() {
        let raw: Balance = serde_yaml::from_str(BALANCE_YAML).unwrap();
        assert_eq!(
            raw.quips.lines_en.as_ref().map(|l| l.len()),
            Some(raw.quips.lines.len())
        );
        let fish = serde_yaml::from_str::<FishFile>(FISH_YAML).unwrap().treats;
        assert!(
            fish.iter()
                .all(|f| f.name_en.is_some() && f.note_en.is_some()),
            "every treat has English"
        );
        let rules = serde_yaml::from_str::<RuleFile>(INTERACTIONS_YAML)
            .unwrap()
            .rules;
        assert!(
            rules
                .iter()
                .all(|r| r.bubble.is_none() || r.bubble_en.is_some()),
            "every bubble has English"
        );

        let zh = Tables::builtin_in(false);
        assert_eq!(zh.fish[0].name, "牛奶糖");
        assert_eq!(zh.balance.quips.lines[0], "又改 {file} 了，第 {n} 次");
        let en = Tables::builtin_in(true);
        assert_eq!(en.fish[0].name, "Milk Caramel");
        assert!(en.balance.quips.lines.iter().all(|l| l.is_ascii()));
        assert!(en.rules.iter().all(|r| r
            .bubble
            .as_ref()
            .unwrap()
            .for_camp("codex")
            .iter()
            .all(|l| l.is_ascii())));
    }

    #[test]
    fn user_chinese_overrides_fall_back_to_chinese_in_english() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("balance.yaml"),
            "quips: { lines: [\"自己的吐槽\"] }\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("treats.yaml"),
            "treats:\n  - { id: a, name: 甲糖, rarity: common }\n  - { id: b, name: 乙糖, rarity: rare, name_en: Bee Drop }\n  - { id: c, name: 丙糖, rarity: epic }\n  - { id: d, name: 丁糖, rarity: legendary }\n",
        )
        .unwrap();
        std::fs::create_dir(dir.path().join("interactions")).unwrap();
        std::fs::write(
            dir.path().join("interactions/mine.yaml"),
            "rules:\n  - { id: taunt, on: turn_done, action: taunt, bubble: [\"哼\"] }\n",
        )
        .unwrap();
        let t = Tables::load_in(dir.path(), true);
        assert!(t.warnings.is_empty(), "{:?}", t.warnings);
        assert_eq!(t.balance.quips.lines, ["自己的吐槽"]);
        assert_eq!(t.fish[0].name, "甲糖");
        assert_eq!(t.fish[1].name, "Bee Drop");
        let taunt = t.rules.iter().find(|r| r.id == "taunt").unwrap();
        assert_eq!(taunt.bubble.as_ref().unwrap().for_camp("codex"), ["哼"]);
        assert_eq!(t.rules.len(), 1, "yours is the only one");
    }

    #[test]
    fn an_old_fish_file_is_still_read() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("fish.yaml"),
            "fish:
  - { id: a, name: 甲糖, rarity: common }
  - { id: b, name: 乙糖, rarity: rare }
  - { id: c, name: 丙糖, rarity: epic }
  - { id: d, name: 丁糖, rarity: legendary }
",
        )
        .unwrap();
        let t = Tables::load_in(dir.path(), false);
        assert_eq!(t.fish[0].name, "甲糖");
        assert!(
            t.warnings.iter().any(|w| w.contains("treats.yaml")),
            "{:?}",
            t.warnings
        );
        // Once there is a treats.yaml, that one wins and nothing is said.
        std::fs::write(
            dir.path().join("treats.yaml"),
            "treats:
  - { id: a, name: 新糖, rarity: common }
  - { id: b, name: 乙糖, rarity: rare }
  - { id: c, name: 丙糖, rarity: epic }
  - { id: d, name: 丁糖, rarity: legendary }
",
        )
        .unwrap();
        let t = Tables::load_in(dir.path(), false);
        assert_eq!(t.fish[0].name, "新糖");
        assert!(t.warnings.is_empty());
    }

    #[test]
    fn user_files_override() {
        let dir = tempfile::tempdir().unwrap();
        // Keys from an earlier build (economy) are ignored, not an error.
        std::fs::write(
            dir.path().join("balance.yaml"),
            "economy: { daily_cap: 50 }\nfishing: { min_turn_s: 2 }\n",
        )
        .unwrap();
        std::fs::create_dir(dir.path().join("interactions")).unwrap();
        std::fs::write(
            dir.path().join("interactions/mine.yaml"),
            "rules:\n  - { id: hi, on: state, state: done, action: wave }\n  - { id: bye, enabled: false, on: state, state: done, action: wave }\n",
        )
        .unwrap();
        let t = Tables::load(dir.path());
        assert!(t.warnings.is_empty(), "{:?}", t.warnings);
        assert_eq!(t.balance.fishing.min_turn_s, 2);
        assert_eq!(
            t.balance.fishing.start_after_s, 5,
            "untouched keys keep defaults"
        );
        assert!(
            t.rules.iter().all(|r| r.id != "bye"),
            "switched off by its own file"
        );
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
