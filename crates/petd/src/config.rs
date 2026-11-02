//! config.yaml (design §4, §5.1, §9). Unknown keys are ignored so newer
//! configs still load on older builds.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Mode {
    #[default]
    PerCamp,
    Single,
    PerRule,
    PerSession,
}

impl Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::PerCamp => "per-camp",
            Mode::Single => "single",
            Mode::PerRule => "per-rule",
            Mode::PerSession => "per-session",
        }
    }
    pub fn parse(s: &str) -> Option<Mode> {
        Some(match s {
            "per-camp" => Mode::PerCamp,
            "single" => Mode::Single,
            "per-rule" => Mode::PerRule,
            "per-session" => Mode::PerSession,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Match {
    pub source: Option<String>,
    pub project: Option<String>,
}

impl Match {
    pub fn matches(&self, source: &str, project: &str) -> bool {
        self.source.as_deref().is_none_or(|s| s == source)
            && self.project.as_deref().is_none_or(|p| project_matches(p, project))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PetRule {
    pub id: String,
    pub pack: String,
    #[serde(default, rename = "match")]
    pub matcher: Match,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Selection {
    #[serde(rename = "claude-code")]
    pub claude_code: Option<String>,
    pub codex: Option<String>,
    pub projects: BTreeMap<String, String>,
    pub favorites: Vec<String>,
    pub rotation: String,
}

impl Default for Selection {
    fn default() -> Self {
        Selection { claude_code: None, codex: None, projects: BTreeMap::new(), favorites: vec![], rotation: "off".into() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Features {
    pub focus_mode: bool,
    pub sounds: bool,
    /// Tug-of-war / shadow race at the bottom of the screen.
    pub arena: bool,
    /// Pet size: s | m | l.
    pub size: String,
    /// Hourly "drink some water" reminder while idle.
    pub water: bool,
    /// 4-7-8 breathing when more than 6 waits pile up in 10 minutes.
    pub breathe: bool,
}

impl Default for Features {
    fn default() -> Self {
        Features { focus_mode: false, sounds: true, arena: true, size: "m".into(), water: false, breathe: false }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub mode: Mode,
    pub max_minis: u32,
    pub max_pets: u32,
    pub pets: Vec<PetRule>,
    pub fallback_pet: Option<String>,
    pub selection: Selection,
    pub features: Features,
    /// auto (system language) | zh | en
    pub language: String,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            mode: Mode::PerCamp,
            max_minis: 4,
            max_pets: 4,
            pets: vec![],
            fallback_pet: None,
            selection: Selection::default(),
            features: Features::default(),
            language: "auto".into(),
        }
    }
}

pub const DEFAULT_PACK: &str = "pixel-cat";
pub const DEFAULT_CODEX_PACK: &str = "pixel-bot";

/// A project pattern matches the project's folder name or its full path.
/// `*` and `?` are wildcards; comparison ignores case and slash direction.
pub fn project_matches(pattern: &str, project: &str) -> bool {
    let norm = |s: &str| s.replace('\\', "/").trim_end_matches('/').to_lowercase();
    let pat = norm(pattern);
    let full = norm(project);
    let base = full.rsplit('/').next().unwrap_or("").to_string();
    glob(&pat, &base) || glob(&pat, &full)
}

fn glob(p: &str, t: &str) -> bool {
    let (p, t): (Vec<char>, Vec<char>) = (p.chars().collect(), t.chars().collect());
    let (mut pi, mut ti, mut star, mut mark) = (0, 0, None, 0);
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ti;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|c| *c == '*')
}

impl Features {
    /// Display multiplier for `size`.
    pub fn size_scale(&self) -> f32 {
        match self.size.as_str() {
            "s" | "S" | "small" => 0.75,
            "l" | "L" | "large" => 1.35,
            _ => 1.0,
        }
    }
}

impl Config {
    pub fn load(path: &Path) -> Result<Config, String> {
        match std::fs::read_to_string(path) {
            Ok(t) if t.trim().is_empty() => Ok(Config::default()),
            Ok(t) => serde_yaml::from_str(&t).map_err(|e| format!("config.yaml: {e}")),
            Err(_) => Ok(Config::default()),
        }
    }

    /// Pack for a camp: selection, then the first matching rule, then fallback.
    pub fn pack_for_source(&self, source: &str) -> String {
        let sel = match source {
            "claude-code" => self.selection.claude_code.clone(),
            "codex" => self.selection.codex.clone(),
            _ => None,
        };
        sel.or_else(|| {
            self.pets.iter().find(|r| r.matcher.source.as_deref() == Some(source)).map(|r| r.pack.clone())
        })
        .or_else(|| self.fallback_pet.clone())
        .unwrap_or_else(|| if source == "codex" { DEFAULT_CODEX_PACK } else { DEFAULT_PACK }.into())
    }

    /// Per-project override from `selection.projects`: (pattern, pack).
    pub fn project_override(&self, project: &str) -> Option<(&str, &str)> {
        if project.is_empty() {
            return None;
        }
        self.selection.projects.iter().find(|(k, _)| project_matches(k, project)).map(|(k, v)| (k.as_str(), v.as_str()))
    }

    pub fn rule_for(&self, source: &str, project: &str) -> Option<&PetRule> {
        self.pets.iter().find(|r| r.matcher.matches(source, project))
    }
}

/// Change config.yaml in place. Keys this build does not know about are
/// kept; comments are not (serde_yaml cannot round-trip them).
pub fn edit(path: &Path, f: impl FnOnce(&mut serde_yaml::Mapping) -> Result<(), String>) -> Result<(), String> {
    use serde_yaml::Value;
    let mut root = match std::fs::read_to_string(path) {
        Ok(t) if !t.trim().is_empty() => serde_yaml::from_str::<Value>(&t).map_err(|e| format!("config.yaml: {e}"))?,
        _ => Value::Mapping(Default::default()),
    };
    if !root.is_mapping() {
        root = Value::Mapping(Default::default());
    }
    f(root.as_mapping_mut().unwrap())?;
    // Validate before writing so a bad edit never lands on disk.
    serde_yaml::from_value::<Config>(root.clone()).map_err(|e| e.to_string())?;
    let text = format!("{}{}", EDITED_HEADER, serde_yaml::to_string(&root).map_err(|e| e.to_string())?);
    let tmp = path.with_extension("yaml.tmp");
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

/// `map[key]` as a mapping, created if missing.
pub fn child<'a>(map: &'a mut serde_yaml::Mapping, key: &str) -> &'a mut serde_yaml::Mapping {
    use serde_yaml::Value;
    let k = Value::String(key.into());
    if !map.get(&k).is_some_and(Value::is_mapping) {
        map.insert(k.clone(), Value::Mapping(Default::default()));
    }
    map.get_mut(&k).unwrap().as_mapping_mut().unwrap()
}

const EDITED_HEADER: &str =
    "# Familiar 配置。保存后自动生效。 / Familiar config. Changes apply on save.\n# 由 Familiar 改写过（图鉴、右键菜单或 /pet switch），原有注释不保留。 / Rewritten by Familiar (Gallery, context menu or /pet switch); original comments are not kept.\n";

/// Written on first start so the user has something to edit.
pub const DEFAULT_YAML: &str = r#"# Familiar 配置。保存后自动生效。 / Familiar config. Changes apply on save.
language: auto           # 界面语言 / UI language: auto | zh | en
mode: per-camp           # per-camp（默认，主宠物 + 分身 / default: main pet + minis）| single | per-rule | per-session
max_minis: 4             # per-camp 下每只主宠物最多显示几个分身 / max minis per main pet in per-camp
max_pets: 4              # per-session 下最多几只宠物 / max pets in per-session

selection:
  claude-code: pixel-cat
  codex: pixel-bot
  projects: {}           # 按项目覆盖，如 { f1-server: pixel-bot }，键可以是文件夹名、完整路径或通配符 / per-project override; key: folder name, full path or wildcard
  favorites: [pixel-cat, pixel-bot]
  rotation: off          # off | daily-random（每天随机一只收藏 / random favorite each day）| weekday（按星期轮换收藏 / rotate favorites by weekday）

# per-rule 模式下每条规则一只宠物： / per-rule mode: one pet per rule:
# pets:
#   - { id: work, pack: pixel-cat, match: { project: "D:/work/*" } }
#   - { id: side, pack: pixel-bot, match: { source: codex } }

features:
  focus_mode: false
  sounds: true
  arena: true            # 拔河小剧场 / tug-of-war scenes
  size: m                # 宠物大小 / pet size: s | m | l
  water: false           # 每小时空闲时提醒喝水 / hourly water reminder when idle
  breathe: false         # 10 分钟内等你超过 6 次时，一起做 30 秒深呼吸 / 30s breathing when you are waited on 6+ times in 10 min

# 开会共享屏幕时隐藏宠物。内置名单认不出你的会议软件时， / Pets hide while you share your screen in a meeting.
# 在这里加"正在共享"工具条的窗口标题或类名关键词 / If your meeting app is not recognized, add words from its
# "sharing" toolbar window title or class name here
# meeting:
#   share_windows: ["正在共享"]

hotkeys:
  boss_key: Ctrl+Alt+H
  focus_toggle: Ctrl+Alt+F
  cycle_pet: Ctrl+Alt+P  # 在收藏里切换当前阵营的宠物 / cycle the current team's pet through favorites
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_design_example_and_ignores_unknown_keys() {
        let c: Config = serde_yaml::from_str(
            r#"
mode: per-camp
max_minis: 4
pets:
  - { id: claude-girl, pack: claude-girl, match: { source: claude-code } }
  - { id: pixel-cat,   pack: pixel-cat,   match: { source: codex } }
fallback_pet: claude-girl
features: { focus_mode: false, sounds: true, fishing: true }
limits: { interactions_per_hour: 6 }
"#,
        )
        .unwrap();
        assert_eq!(c.pack_for_source("codex"), "pixel-cat");
        assert_eq!(c.pack_for_source("claude-code"), "claude-girl");
        assert_eq!(c.pack_for_source("other"), "claude-girl");
        let d: Config = serde_yaml::from_str(DEFAULT_YAML).unwrap();
        assert_eq!(d.pack_for_source("claude-code"), "pixel-cat");
        assert_eq!(d.pack_for_source("codex"), "pixel-bot");
    }

    #[test]
    fn project_patterns() {
        assert!(project_matches("f1-server", "D:\\work\\F1-Server"));
        assert!(project_matches("D:/work/*", "D:\\work\\f1-server\\"));
        assert!(project_matches("web-*", "/home/me/web-admin"));
        assert!(!project_matches("web-*", "/home/me/api"));
        let mut c = Config::default();
        c.selection.projects.insert("f1-*".into(), "pixel-bot".into());
        assert_eq!(c.project_override("D:/w/f1-client"), Some(("f1-*", "pixel-bot")));
        assert_eq!(c.project_override(""), None);
    }

    #[test]
    fn edit_keeps_unknown_keys_and_rejects_bad_values() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("config.yaml");
        std::fs::write(&p, DEFAULT_YAML).unwrap();
        edit(&p, |m| {
            child(child(m, "selection"), "projects").insert("api".into(), "pixel-bot".into());
            m.insert("mode".into(), "single".into());
            Ok(())
        })
        .unwrap();
        let c = Config::load(&p).unwrap();
        assert_eq!(c.mode, Mode::Single);
        assert_eq!(c.selection.projects.get("api").map(String::as_str), Some("pixel-bot"));
        assert!(std::fs::read_to_string(&p).unwrap().contains("boss_key"), "unknown keys survive");
        assert!(edit(&p, |m| {
            m.insert("mode".into(), "nonsense".into());
            Ok(())
        })
        .is_err());
        assert_eq!(Config::load(&p).unwrap().mode, Mode::Single, "bad edit not written");
    }
}
