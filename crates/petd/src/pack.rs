//! Pet packs (design §5): load pet.json, validate, resolve every action's
//! fallback chain at load time so the renderer never sees a missing action.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

/// Every action the system may ask for. Packs may declare more.
pub const KNOWN_ACTIONS: &[&str] = &[
    "idle", "thinking", "tool", "attention", "done", "error", //
    "reading", "aborted", "stone", "sleep", "wake", "celebrate", //
    "squish", "dizzy", "petted", "annoyed", "fall", //
    "fish", "fish_reel", "pack_bag", "peek", //
    "walk", "eat", "throw", "catch", "taunt", "sulk", "steal", "chase", "cheer", "comfort", "wave",
];

/// Actions the pet holds for as long as a state lasts.
const HELD: &[&str] = &["idle", "thinking", "tool", "reading", "attention", "stone", "sleep", "fish"];

/// Default fallback when an action has neither a sheet nor an explicit fallback.
pub fn default_fallback(action: &str) -> Option<&'static str> {
    Some(match action {
        "idle" => return None,
        "reading" => "tool",
        "aborted" | "stone" => "error",
        "celebrate" => "done",
        _ => "idle",
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Frame {
    pub w: u32,
    pub h: u32,
    #[serde(default = "one_f")]
    pub scale: f32,
    #[serde(default)]
    pub anchor: Option<[u32; 2]>,
}

fn one_f() -> f32 {
    1.0
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RawAction {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sheet: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frames: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fps: Option<f32>,
    #[serde(default, rename = "loop", skip_serializing_if = "Option::is_none")]
    pub looped: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub then: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sound: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tint: Option<String>,
}

/// The pet.json manifest.
///
/// Optional English fields, used instead of the Chinese ones when the UI
/// language is English (each falls back to its Chinese counterpart when
/// missing or empty):
/// - `name_en`: display name
/// - `description_en`: one-line description
/// - `bubbles_en`: same shape as `bubbles`; merged key by key, so a state
///   missing here keeps its Chinese lines
/// - `tags_en`: tag list
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    #[serde(default = "one_u")]
    pub schema: u32,
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub author: String,
    pub frame: Frame,
    pub actions: BTreeMap<String, RawAction>,
    #[serde(default)]
    pub bubbles: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub evolutions: Option<serde_json::Value>,
    /// Extra sound sets, e.g. `squish: [a.wav, b.wav, c.wav]` (one is picked at random).
    #[serde(default)]
    pub sounds: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name_en: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description_en: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub bubbles_en: BTreeMap<String, Vec<String>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags_en: Vec<String>,
}

impl Manifest {
    /// Swap in the `*_en` fields when the current language is English.
    fn localize(&mut self, en: bool) {
        if !en {
            return;
        }
        if let Some(n) = self.name_en.take().filter(|s| !s.trim().is_empty()) {
            self.name = n;
        }
        if let Some(d) = self.description_en.take().filter(|s| !s.trim().is_empty()) {
            self.description = d;
        }
        for (k, v) in std::mem::take(&mut self.bubbles_en) {
            if !v.is_empty() {
                self.bubbles.insert(k, v);
            }
        }
        if !self.tags_en.is_empty() {
            self.tags = std::mem::take(&mut self.tags_en);
        }
    }
}

fn one_u() -> u32 {
    1
}

/// An action after fallback resolution: always has a sheet.
#[derive(Debug, Clone, Serialize)]
pub struct Action {
    pub sheet: String,
    pub frames: u32,
    pub fps: f32,
    #[serde(rename = "loop")]
    pub looped: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub then: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sound: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tint: Option<String>,
    /// Which action actually provided the frames.
    pub from: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Pack {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub frame: Frame,
    pub actions: BTreeMap<String, Action>,
    pub bubbles: BTreeMap<String, Vec<String>>,
    pub tags: Vec<String>,
    pub sounds: BTreeMap<String, Vec<String>>,
    pub description: String,
    /// `[{ "turns": 100, "dir": "sprites/stage1" }, …]`: thresholds and optional
    /// sprite folders per evolution stage.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evolutions: Option<serde_json::Value>,
    pub builtin: bool,
    #[serde(skip)]
    pub dir: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
pub struct PackError {
    pub dir: String,
    pub id: Option<String>,
    pub errors: Vec<String>,
}

pub fn load(dir: &Path, builtin: bool) -> Result<Pack, PackError> {
    let fail = |id: Option<String>, errors: Vec<String>| PackError { dir: dir.display().to_string(), id, errors };
    let text = std::fs::read_to_string(dir.join("pet.json")).map_err(|e| fail(None, vec![format!("pet.json: {e}")]))?;
    let mut m: Manifest = serde_json::from_str(&text).map_err(|e| fail(None, vec![format!("pet.json: {e}")]))?;
    m.localize(crate::i18n::is_en());
    let id = Some(m.id.clone());
    let mut errors = Vec::new();

    if m.id.trim().is_empty() || m.id.contains(['/', '\\', '.']) {
        errors.push("id 必须是非空的简单名字".into());
    }
    if m.frame.w == 0 || m.frame.h == 0 {
        errors.push("frame.w / frame.h 必须大于 0".into());
    }

    // Check every declared sheet once.
    for (name, a) in &m.actions {
        if let Some(sheet) = &a.sheet {
            let frames = a.frames.unwrap_or(1);
            match png_size(&dir.join(sheet)) {
                Err(e) => errors.push(format!("{name}: {sheet}: {e}")),
                Ok((w, h)) => {
                    if w != frames * m.frame.w || h != m.frame.h {
                        errors.push(format!(
                            "{name}: {sheet} 是 {w}×{h}，应为 {}×{}（{frames} 帧 × {}×{}）",
                            frames * m.frame.w,
                            m.frame.h,
                            m.frame.w,
                            m.frame.h
                        ));
                    }
                }
            }
        }
        if let Some(s) = &a.sound {
            if !dir.join(s).is_file() {
                errors.push(format!("{name}: 音效 {s} 不存在"));
            }
        }
    }
    for (set, files) in &m.sounds {
        for f in files {
            if !dir.join(f).is_file() {
                errors.push(format!("sounds.{set}: 音效 {f} 不存在"));
            }
        }
    }
    if m.actions.get("idle").and_then(|a| a.sheet.as_ref()).is_none() {
        errors.push("缺少必需动作 idle（需要 sheet）".into());
    }

    let mut actions = BTreeMap::new();
    let names: Vec<String> = KNOWN_ACTIONS.iter().map(|s| s.to_string()).chain(m.actions.keys().cloned()).collect();
    for name in names {
        if actions.contains_key(&name) {
            continue;
        }
        match resolve(&m, &name) {
            Ok(a) => {
                actions.insert(name, a);
            }
            Err(e) => errors.push(e),
        }
    }
    for (name, a) in &actions {
        if let Some(t) = &a.then {
            if !actions.contains_key(t) {
                errors.push(format!("{name}: then 指向不存在的动作 {t}"));
            }
        }
    }

    if !errors.is_empty() {
        errors.dedup();
        return Err(fail(id, errors));
    }
    Ok(Pack {
        id: m.id,
        name: m.name,
        version: m.version,
        author: m.author,
        frame: m.frame,
        actions,
        bubbles: m.bubbles,
        tags: m.tags,
        sounds: m.sounds,
        description: m.description,
        evolutions: m.evolutions,
        builtin,
        dir: dir.to_path_buf(),
    })
}

/// Follow fallbacks until an action with a sheet. The requesting action's own
/// tint/sound/then/loop override what it inherits.
fn resolve(m: &Manifest, name: &str) -> Result<Action, String> {
    let own = m.actions.get(name);
    let mut seen = vec![name.to_string()];
    let mut cur = name.to_string();
    loop {
        let raw = m.actions.get(&cur);
        if let Some(sheet) = raw.and_then(|a| a.sheet.clone()) {
            let src = raw.unwrap();
            let pick = |f: fn(&RawAction) -> Option<String>| own.and_then(f).or_else(|| f(src));
            // States the pet sits in (stone, attention…) keep looping even
            // when they borrow a one-shot sheet such as error.
            let held = HELD.contains(&name) && cur != name;
            let looped = own.and_then(|a| a.looped).or(if held { Some(true) } else { src.looped }).unwrap_or(true);
            return Ok(Action {
                sheet,
                frames: src.frames.unwrap_or(1),
                fps: src.fps.unwrap_or(6.0),
                looped,
                then: if looped { None } else { pick(|a| a.then.clone()) },
                sound: own.and_then(|a| a.sound.clone()).or_else(|| if cur == name { src.sound.clone() } else { None }),
                tint: own.and_then(|a| a.tint.clone()),
                from: cur,
            });
        }
        let next = raw
            .and_then(|a| a.fallback.clone())
            .or_else(|| default_fallback(&cur).map(String::from))
            .ok_or_else(|| format!("{name}: 无法解析（{} 没有 sheet）", seen.join(" → ")))?;
        if seen.contains(&next) {
            seen.push(next);
            return Err(format!("{name}: fallback 成环 {}", seen.join(" → ")));
        }
        seen.push(next.clone());
        cur = next;
    }
}

/// Width and height from a PNG's IHDR chunk.
pub fn png_size(path: &Path) -> Result<(u32, u32), String> {
    use std::io::Read;
    let mut f = std::fs::File::open(path).map_err(|_| "文件不存在".to_string())?;
    let mut h = [0u8; 24];
    f.read_exact(&mut h).map_err(|_| "不是有效的 PNG".to_string())?;
    if &h[..8] != b"\x89PNG\r\n\x1a\n" || &h[12..16] != b"IHDR" {
        return Err("不是有效的 PNG".into());
    }
    Ok((u32::from_be_bytes(h[16..20].try_into().unwrap()), u32::from_be_bytes(h[20..24].try_into().unwrap())))
}

/// All packs from the given roots. Later roots override earlier ones with the same id.
#[derive(Debug, Default, Clone, Serialize)]
pub struct Library {
    pub packs: HashMap<String, Pack>,
    pub broken: Vec<PackError>,
}

impl Library {
    pub fn scan(roots: &[(PathBuf, bool)]) -> Self {
        let mut lib = Library::default();
        for (root, builtin) in roots {
            let Ok(rd) = std::fs::read_dir(root) else { continue };
            let mut dirs: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.join("pet.json").is_file()).collect();
            dirs.sort();
            for d in dirs {
                match load(&d, *builtin) {
                    Ok(p) => {
                        lib.packs.insert(p.id.clone(), p);
                    }
                    Err(e) => lib.broken.push(e),
                }
            }
        }
        lib
    }

    pub fn get(&self, id: &str) -> Option<&Pack> {
        self.packs.get(id)
    }

    /// Requested pack, else the first built-in, else anything.
    pub fn pick(&self, want: &str) -> Option<&Pack> {
        self.get(want).or_else(|| {
            let mut v: Vec<&Pack> = self.packs.values().collect();
            v.sort_by_key(|p| (!p.builtin, p.id.clone()));
            v.into_iter().next()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(w: u32, h: u32) -> Vec<u8> {
        let mut v = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
        v.extend(w.to_be_bytes());
        v.extend(h.to_be_bytes());
        v.extend([8, 6, 0, 0, 0]);
        v
    }

    fn write_pack(dir: &Path, json: &str, sheets: &[(&str, u32, u32)]) {
        std::fs::create_dir_all(dir.join("sprites")).unwrap();
        std::fs::write(dir.join("pet.json"), json).unwrap();
        for (n, w, h) in sheets {
            std::fs::write(dir.join(n), png(*w, *h)).unwrap();
        }
    }

    #[test]
    fn minimal_pack_resolves_everything_to_idle() {
        let t = tempfile::tempdir().unwrap();
        write_pack(
            t.path(),
            r#"{"id":"mini","frame":{"w":8,"h":8},"actions":{"idle":{"sheet":"sprites/idle.png","frames":2}}}"#,
            &[("sprites/idle.png", 16, 8)],
        );
        let p = load(t.path(), false).unwrap();
        for a in KNOWN_ACTIONS {
            assert_eq!(p.actions[*a].sheet, "sprites/idle.png", "{a}");
        }
        assert_eq!(p.actions["stone"].from, "idle");
    }

    #[test]
    fn chain_and_tint() {
        let t = tempfile::tempdir().unwrap();
        write_pack(
            t.path(),
            r##"{"id":"c","frame":{"w":8,"h":8},"actions":{
                "idle":{"sheet":"sprites/idle.png"},
                "tool":{"sheet":"sprites/tool.png","frames":2},
                "error":{"sheet":"sprites/err.png","loop":false,"then":"idle"},
                "stone":{"fallback":"error","tint":"#999"}}}"##,
            &[("sprites/idle.png", 8, 8), ("sprites/tool.png", 16, 8), ("sprites/err.png", 8, 8)],
        );
        let p = load(t.path(), false).unwrap();
        assert_eq!(p.actions["reading"].from, "tool");
        assert_eq!(p.actions["stone"].from, "error");
        assert_eq!(p.actions["stone"].tint.as_deref(), Some("#999"));
        assert_eq!(p.actions["aborted"].from, "error");
        assert_eq!(p.actions["error"].then.as_deref(), Some("idle"));
    }

    #[test]
    fn reports_bad_sizes_missing_files_and_cycles() {
        let t = tempfile::tempdir().unwrap();
        write_pack(
            t.path(),
            r#"{"id":"bad","frame":{"w":8,"h":8},"actions":{
                "idle":{"sheet":"sprites/idle.png","frames":3},
                "tool":{"sheet":"sprites/nope.png"},
                "a":{"fallback":"b"},"b":{"fallback":"a"}}}"#,
            &[("sprites/idle.png", 16, 8)],
        );
        let e = load(t.path(), false).unwrap_err();
        let all = e.errors.join("\n");
        assert!(all.contains("应为 24×8"), "{all}");
        assert!(all.contains("nope.png"), "{all}");
        assert!(all.contains("成环"), "{all}");
    }

    #[test]
    fn english_fields_fall_back_per_key() {
        let mut m: Manifest = serde_json::from_str(
            r#"{"id":"e","name":"猫","frame":{"w":8,"h":8},"actions":{},"description":"一只猫",
                "bubbles":{"done":["好了"],"error":["坏了"]},"tags":["猫"],
                "name_en":"Cat","bubbles_en":{"done":["Done!"]}}"#,
        )
        .unwrap();
        let zh = m.clone();
        m.localize(true);
        assert_eq!(m.name, "Cat");
        assert_eq!(m.description, "一只猫");
        assert_eq!(m.bubbles["done"], ["Done!"]);
        assert_eq!(m.bubbles["error"], ["坏了"]);
        assert_eq!(m.tags, ["猫"]);
        let mut z = zh.clone();
        z.localize(false);
        assert_eq!(z.name, "猫");
        assert_eq!(z.bubbles["done"], ["好了"]);
    }
}
