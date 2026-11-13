//! config.yaml. Unknown keys are ignored, so configs written by other builds
//! (older ones had routing modes, pet packs and more switches) still load.

use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Features {
    pub focus_mode: bool,
    pub sounds: bool,
    /// Pet size: s | m | l.
    pub size: String,
    /// Reps per kegel set: squeeze 3 s, relax 3 s.
    pub kegel_reps: u32,
    /// How much the pets say and do on their own: quiet | normal | lively.
    pub chatter: String,
    /// Answer Claude Code's permission requests from a card above the pet.
    /// While it is on, a request waits for that card (up to `approve_wait_s`)
    /// before the terminal shows its own prompt.
    pub approve: bool,
    pub approve_wait_s: u32,
    /// 夜里整理: in the evening, with you away, put old files away and box old sessions (all undoable).
    pub night: bool,
}

impl Default for Features {
    fn default() -> Self {
        Features {
            focus_mode: false,
            sounds: true,
            size: "m".into(),
            kegel_reps: 10,
            chatter: "normal".into(),
            approve: false,
            approve_wait_s: 30,
            night: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub features: Features,
    /// Which pet stands for which agent: source id -> pet id (see `cast`).
    pub companions: std::collections::BTreeMap<String, String>,
    /// auto (system language) | zh | en
    pub language: String,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            features: Features::default(),
            companions: Default::default(),
            language: "auto".into(),
        }
    }
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
}

/// Change config.yaml in place. Keys this build does not know about are
/// kept; comments are not (serde_yaml cannot round-trip them).
pub fn edit(
    path: &Path,
    f: impl FnOnce(&mut serde_yaml::Mapping) -> Result<(), String>,
) -> Result<(), String> {
    use serde_yaml::Value;
    // One edit at a time: two settings changed in the same instant would each
    // read the old file and the second write would drop the first.
    static EDITING: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _one = EDITING.lock().unwrap_or_else(|e| e.into_inner());
    let mut root = match std::fs::read_to_string(path) {
        Ok(t) if !t.trim().is_empty() => {
            serde_yaml::from_str::<Value>(&t).map_err(|e| format!("config.yaml: {e}"))?
        }
        _ => Value::Mapping(Default::default()),
    };
    if !root.is_mapping() {
        root = Value::Mapping(Default::default());
    }
    f(root.as_mapping_mut().unwrap())?;
    // Validate before writing so a bad edit never lands on disk.
    serde_yaml::from_value::<Config>(root.clone()).map_err(|e| e.to_string())?;
    let text = format!(
        "{}{}",
        EDITED_HEADER,
        serde_yaml::to_string(&root).map_err(|e| e.to_string())?
    );
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
    "# 睿洱配置。保存后自动生效。 / Rill config. Changes apply on save.\n# 由睿洱改写过（设置页、托盘或右键菜单），原有注释不保留。 / Rewritten by Rill (settings, tray or context menu); original comments are not kept.\n";

/// Agents beyond the two that Rill grew up with, each with an adapter:
/// (source id, name shown to the user, short tag under the pet).
pub const OTHER_AGENTS: &[(&str, &str, &str)] = &[
    ("gemini", "Gemini CLI", "Gemini"),
    ("cursor", "Cursor", "Cursor"),
    ("opencode", "OpenCode", "OpenCode"),
];

/// The name of an agent as the user knows it.
pub fn agent_name(source: &str) -> String {
    match source {
        "claude-code" => "Claude Code".into(),
        "codex" => "Codex".into(),
        s => OTHER_AGENTS
            .iter()
            .find(|a| a.0 == s)
            .map(|a| a.1.to_string())
            .unwrap_or_else(|| s.to_string()),
    }
}

/// The short tag under an agent's pet. An agent from a custom adapter shows its id.
pub fn agent_tag(source: &str) -> String {
    match source {
        "claude-code" => "Claude".into(),
        "codex" => "Codex".into(),
        s => OTHER_AGENTS
            .iter()
            .find(|a| a.0 == s)
            .map(|a| a.2.to_string())
            .unwrap_or_else(|| s.to_string()),
    }
}

/// Written on first start so the user has something to edit.
pub const DEFAULT_YAML: &str = r#"# 睿洱配置。保存后自动生效。 / Rill config. Changes apply on save.
language: auto           # 界面语言 / UI language: auto | zh | en

features:
  focus_mode: false
  sounds: true
  size: m                # 宠物大小 / pet size: s | m | l
  kegel_reps: 10         # 提肛每次做几组（收紧 3 秒、放松 3 秒）/ reps per kegel set (squeeze 3s, relax 3s)
  chatter: normal        # 宠物自己说话、做事的多少 / how much the pets say and do on their own: quiet | normal | lively
  approve: false         # 在宠物上方的卡片里允许 / 拒绝 Claude Code 的权限请求 / answer Claude Code permission requests from a card above the pet
  approve_wait_s: 30     # 卡片等你多久（秒），之后回到终端里问 / how long the card waits before the terminal asks instead

# 开会共享屏幕时隐藏宠物。内置名单认不出你的会议软件时， / Pets hide while you share your screen in a meeting.
# 在这里加"正在共享"工具条的窗口标题或类名关键词 / If your meeting app is not recognized, add words from its
# "sharing" toolbar window title or class name here
# meeting:
#   share_windows: ["正在共享"]

hotkeys:
  boss_key: Ctrl+Alt+H
  focus_toggle: Ctrl+Alt+F
  # 记一件事：选中一段字再按，就从那段字开始写 / Write something down; with text selected, it starts from that
  note: Ctrl+Alt+N
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_older_config_with_keys_that_are_gone_still_loads() {
        let c: Config = serde_yaml::from_str(
            r#"
mode: per-camp
max_minis: 4
pets:
  - { id: work, pack: pixel-cat, match: { source: codex } }
fallback_pet: pixel-cat
selection: { claude-code: pixel-cat, favorites: [pixel-cat], rotation: off }
features: { focus_mode: true, sounds: false, arena: true, water: true, kegel: ["10:30"], unlock: all, size: l }
limits: { interactions_per_hour: 6 }
"#,
        )
        .unwrap();
        assert!(c.features.focus_mode && !c.features.sounds);
        assert_eq!((c.features.size.as_str(), c.features.kegel_reps), ("l", 10));
        let d: Config = serde_yaml::from_str(DEFAULT_YAML).unwrap();
        assert_eq!(
            (d.language.as_str(), d.features.chatter.as_str()),
            ("auto", "normal")
        );
    }

    #[test]
    fn edit_keeps_unknown_keys_and_rejects_bad_values() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("config.yaml");
        std::fs::write(
            &p,
            format!("{DEFAULT_YAML}selection: {{ claude-code: pixel-cat }}\n"),
        )
        .unwrap();
        edit(&p, |m| {
            child(m, "features").insert("size".into(), "l".into());
            Ok(())
        })
        .unwrap();
        let c = Config::load(&p).unwrap();
        assert_eq!(c.features.size, "l");
        let text = std::fs::read_to_string(&p).unwrap();
        assert!(
            text.contains("boss_key") && text.contains("pixel-cat"),
            "keys this build does not read survive"
        );
        assert!(edit(&p, |m| {
            child(m, "features").insert("kegel_reps".into(), "many".into());
            Ok(())
        })
        .is_err());
        assert_eq!(
            Config::load(&p).unwrap().features.size,
            "l",
            "bad edit not written"
        );
    }
}
