//! 储物间: what the coding agents keep on this machine, and what each new
//! session reads before you say a word.
//!
//! Two things, both counted here without sending anything anywhere and
//! without spending a token:
//!
//!   - disk: the session records Claude Code and Codex write. Claude Code
//!     clears its own after a month; Codex keeps everything. Sessions older
//!     than `OLD_DAYS` can be packed into one zip in Rill's own folder (the
//!     originals go once the zip is written and checked) and unpacked back to
//!     where they were, with their dates;
//!   - the start of a session: the instruction files it reads (CLAUDE.md /
//!     AGENTS.md from your home and up the project's folders), its memory
//!     index, the text start-up hooks put in, the skills listed for it, and
//!     the MCP servers it connects to, each with a rough token count, and
//!     which of them the last month's sessions never used. Nothing here is
//!     changed: it says what could go, and how.

use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Sessions untouched for this long are "old".
pub const OLD_DAYS: u64 = 30;
const MANIFEST: &str = "rill-manifest.json";

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub agents: Vec<Store>,
    /// Packed boxes in Rill's folder, newest first.
    pub boxes: Vec<Boxed>,
    /// Claude Code skills set aside in the storeroom (out of the list every session reads).
    pub parked: Vec<String>,
    /// When this was counted (ms).
    pub at: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Store {
    /// "claude" | "codex"
    pub id: &'static str,
    pub name: &'static str,
    pub root: String,
    /// The whole folder, the session records in it, and the old ones among them (bytes).
    pub total: u64,
    pub sessions: u64,
    pub session_files: u32,
    pub old: u64,
    pub old_files: u32,
    /// Claude Code clears its old sessions by itself (after `cleanupPeriodDays`, a month unless set).
    pub cleans_itself: bool,
    /// The project the newest session was in (what the start-up list is counted for).
    pub project: Option<String>,
    /// What a new session reads first, biggest first.
    pub startup: Vec<Item>,
    pub startup_tokens: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Item {
    /// "file" | "memory" | "hook" | "skill" | "mcp"
    pub kind: &'static str,
    pub name: String,
    pub path: Option<String>,
    /// Rough tokens this costs every session (0 when it cannot be told from here).
    pub tokens: u64,
    /// Not used once in the last month's sessions (known only where the records show it).
    pub unused: bool,
    /// What to do about it, when there is something to do.
    pub fix: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Boxed {
    pub file: String,
    pub agent: String,
    pub files: u32,
    /// Bytes of the sessions in it, and of the box itself.
    pub bytes: u64,
    pub packed: u64,
    pub at: u64,
}

/// Rough tokens of a text: a CJK character is about one, four other characters about one.
pub fn tokens(s: &str) -> u64 {
    let (mut cjk, mut other) = (0u64, 0u64);
    for c in s.chars() {
        if c as u32 >= 0x2E80 {
            cjk += 1;
        } else {
            other += 1;
        }
    }
    cjk + other.div_ceil(4)
}

fn user_home() -> PathBuf {
    PathBuf::from(
        std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .unwrap_or_default(),
    )
}

pub fn claude_root() -> PathBuf {
    std::env::var_os("CLAUDE_CONFIG_DIR")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| user_home().join(".claude"))
}

pub fn codex_root() -> PathBuf {
    crate::codex::homes()
        .into_iter()
        .find(|p| p.is_dir())
        .unwrap_or_else(|| user_home().join(".codex"))
}

fn root_of(agent: &str) -> Option<PathBuf> {
    match agent {
        "claude" => Some(claude_root()),
        "codex" => Some(codex_root()),
        _ => None,
    }
}

/// Where an agent keeps its session records.
fn session_dir(agent: &str, root: &Path) -> PathBuf {
    root.join(if agent == "claude" {
        "projects"
    } else {
        "sessions"
    })
}

fn mtime(m: &std::fs::Metadata) -> SystemTime {
    m.modified().unwrap_or(UNIX_EPOCH)
}

/// Every session record (`*.jsonl`) under `dir`, with its size and when it was last written.
fn sessions_in(dir: &Path) -> Vec<(PathBuf, u64, SystemTime)> {
    walkdir::WalkDir::new(dir)
        .into_iter()
        .flatten()
        .filter(|e| e.file_type().is_file() && e.path().extension().is_some_and(|x| x == "jsonl"))
        .filter_map(|e| {
            e.metadata()
                .ok()
                .map(|m| (e.path().to_path_buf(), m.len(), mtime(&m)))
        })
        .collect()
}

fn dir_size(dir: &Path) -> u64 {
    walkdir::WalkDir::new(dir)
        .into_iter()
        .flatten()
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .sum()
}

fn cutoff(days: u64) -> SystemTime {
    SystemTime::now() - Duration::from_secs(days * 86_400)
}

fn read(p: &Path) -> Option<String> {
    std::fs::read_to_string(p)
        .ok()
        .filter(|s| !s.trim().is_empty())
}

/// The first `n` lines of a file (session records can be huge; their start is all we need).
fn head_lines(p: &Path, n: usize) -> Vec<String> {
    use std::io::BufRead;
    let Ok(f) = std::fs::File::open(p) else {
        return vec![];
    };
    std::io::BufReader::new(f)
        .lines()
        .take(n)
        .map_while(Result::ok)
        .collect()
}

/// Instruction files from the project's folder up to the drive: the ones an agent reads for a session there.
fn up_the_tree(cwd: &Path, names: &[&str]) -> Vec<PathBuf> {
    let mut out = vec![];
    for dir in cwd.ancestors() {
        for n in names {
            let p = dir.join(n);
            if p.is_file() {
                out.push(p);
            }
        }
    }
    out
}

fn file_item(kind: &'static str, p: &Path, fix: Option<String>) -> Option<Item> {
    let text = read(p)?;
    Some(Item {
        kind,
        name: p.file_name()?.to_string_lossy().into_owned(),
        path: Some(p.display().to_string()),
        tokens: tokens(&text),
        unused: false,
        fix,
    })
}

/// A skill's line in the list an agent is given: its name and its `description:`.
fn skills(dir: &Path) -> Vec<(String, u64, PathBuf)> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return vec![];
    };
    let mut out = vec![];
    for e in rd.flatten() {
        let p = e.path().join("SKILL.md");
        let Some(text) = read(&p) else { continue };
        let name = e.file_name().to_string_lossy().into_owned();
        // The front matter between the first two `---`.
        let front = text
            .strip_prefix("---")
            .and_then(|r| r.split_once("\n---"))
            .map(|(f, _)| f)
            .unwrap_or("");
        let desc: String = front
            .lines()
            .skip_while(|l| !l.trim_start().starts_with("description:"))
            .take_while({
                let mut first = true;
                move |l| std::mem::take(&mut first) || l.starts_with(' ') || l.starts_with('\t')
            })
            .collect::<Vec<_>>()
            .join(" ");
        out.push((name.clone(), tokens(&name) + tokens(&desc) + 6, p));
    }
    out
}

/// What the last month's Claude Code sessions used: MCP servers (`mcp__name__…`) and skills (`"skill":"name"`).
fn claude_used(dir: &Path) -> (BTreeSet<String>, BTreeSet<String>) {
    let since = cutoff(OLD_DAYS);
    let (mut servers, mut skills) = (BTreeSet::new(), BTreeSet::new());
    let mcp = memchr::memmem::Finder::new(b"mcp__");
    let skill = memchr::memmem::Finder::new(b"\"skill\":");
    for (p, _, t) in sessions_in(dir) {
        if t < since {
            continue;
        }
        let Ok(bytes) = std::fs::read(&p) else {
            continue;
        };
        for at in mcp.find_iter(&bytes) {
            let rest = &bytes[at + 5..bytes.len().min(at + 5 + 80)];
            if let Some(end) = memchr::memmem::find(rest, b"__") {
                if let Ok(s) = std::str::from_utf8(&rest[..end]) {
                    if !s.is_empty()
                        && s.chars()
                            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
                    {
                        servers.insert(s.to_string());
                    }
                }
            }
        }
        for at in skill.find_iter(&bytes) {
            // `"skill":"name"`, or with a space after the colon.
            let rest = &bytes[at + 8..bytes.len().min(at + 8 + 120)];
            let Some(open) = rest
                .iter()
                .position(|c| !c.is_ascii_whitespace())
                .filter(|i| rest[*i] == b'"')
            else {
                continue;
            };
            let rest = &rest[open + 1..];
            if let Some(end) = memchr::memchr(b'"', rest) {
                if let Ok(s) = std::str::from_utf8(&rest[..end]) {
                    skills.insert(s.to_string());
                }
            }
        }
    }
    (servers, skills)
}

/// MCP server names in a JSON object's `mcpServers`.
fn mcp_names(v: Option<&serde_json::Value>) -> Vec<String> {
    v.and_then(|v| v.get("mcpServers"))
        .and_then(|m| m.as_object())
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default()
}

/// How MCP names show up in tool names: anything that is not a letter, digit, `_` or `-` becomes `_`.
fn tool_name(server: &str) -> String {
    server
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn claude_startup(root: &Path) -> (Option<String>, Vec<Item>) {
    let mut items = vec![];
    let projects = root.join("projects");
    // The newest main session (not a subagent's): its project, and what its start-up hooks put in.
    let newest = std::fs::read_dir(&projects)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .flat_map(|d| std::fs::read_dir(d.path()).into_iter().flatten().flatten())
        .filter(|e| e.path().extension().is_some_and(|x| x == "jsonl"))
        .filter_map(|e| e.metadata().ok().map(|m| (mtime(&m), e.path())))
        .max_by_key(|(t, _)| *t)
        .map(|(_, p)| p);
    let mut cwd: Option<PathBuf> = None;
    let mut hooks: BTreeMap<String, u64> = BTreeMap::new();
    if let Some(p) = &newest {
        for line in head_lines(p, 400) {
            if !line.contains("\"cwd\"") && !line.contains("SessionStart") {
                continue;
            }
            let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) else {
                continue;
            };
            if cwd.is_none() {
                cwd = v.get("cwd").and_then(|c| c.as_str()).map(PathBuf::from);
            }
            let a = v.get("attachment");
            if a.and_then(|a| a.get("hookEvent")).and_then(|e| e.as_str()) == Some("SessionStart") {
                let a = a.unwrap();
                let text = match a.get("content") {
                    Some(serde_json::Value::String(s)) => s.clone(),
                    Some(serde_json::Value::Array(xs)) => xs
                        .iter()
                        .filter_map(|x| {
                            x.as_str()
                                .or_else(|| x.get("text").and_then(|t| t.as_str()))
                        })
                        .collect::<Vec<_>>()
                        .join("\n"),
                    _ => String::new(),
                };
                if text.trim().is_empty() {
                    continue;
                }
                let cmd = a.get("command").and_then(|c| c.as_str()).unwrap_or("hook");
                // A short name for the hook: its program, without the folders.
                let short = cmd
                    .split_whitespace()
                    .next()
                    .unwrap_or(cmd)
                    .rsplit(['/', '\\'])
                    .next()
                    .unwrap_or(cmd)
                    .to_string();
                *hooks.entry(short).or_default() += tokens(&text);
            }
        }
    }
    // Instruction files: yours, and the project's from its folder up.
    if let Some(it) = file_item("file", &root.join("CLAUDE.md"), Some(crate::tr!("每个会话都会读：只留真正每次都要的，其余挪进单独的文件，需要时再让它读。", "Read by every session: keep only what is needed every time; move the rest into separate files it reads when asked."))) {
        items.push(it);
    }
    if let Some(c) = &cwd {
        for p in up_the_tree(c, &["CLAUDE.md", "CLAUDE.local.md", ".claude/CLAUDE.md"]) {
            if let Some(it) = file_item("file", &p, Some(crate::tr!("在这个文件夹下开的会话都会读：重复的、过时的段落可以删，长的说明挪进单独的文件。", "Read by every session in this folder: drop repeated or outdated parts, move long explanations into separate files."))) {
                items.push(it);
            }
        }
        // Its memory index for this project.
        if let Some(dir) = newest.as_ref().and_then(|p| p.parent()) {
            if let Some(mut it) = file_item("memory", &dir.join("memory").join("MEMORY.md"), None) {
                it.name = "MEMORY.md".into();
                if it.tokens > 1500 {
                    it.fix = Some(crate::tr!("记忆索引偏长：合并重复的条目，删掉已经不对的。", "The memory index is long: merge repeated entries, drop the ones no longer true."));
                }
                items.push(it);
            }
        }
    }
    for (name, t) in hooks {
        items.push(Item {
            kind: "hook",
            fix: (t > 300).then(|| crate::tr!("每次开会话都会塞进这么多字：如果用不上，在 settings.json 的 hooks.SessionStart 里去掉它，或者让它只输出必要的一两句。", "Every session starts with this much of its text: if you don't need it, remove it from hooks.SessionStart in settings.json, or have it print only a line or two.")),
            name,
            path: None,
            tokens: t,
            unused: false,
        });
    }
    // Skills and MCP servers, and whether the last month used them.
    let (used_mcp, used_skills) = claude_used(&projects);
    for (name, t, p) in skills(&root.join("skills")) {
        let unused = !used_skills.contains(&name);
        items.push(Item {
            kind: "skill",
            fix: unused.then(|| crate::tr!("一个月没用过：每个会话仍会列出它的说明。用不上就把这个文件夹挪出 skills。", "Not used in a month, yet every session lists it. If you don't need it, move its folder out of skills.")),
            name,
            path: Some(p.display().to_string()),
            tokens: t,
            unused,
        });
    }
    let conf: Option<serde_json::Value> = std::fs::read_to_string(user_home().join(".claude.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok());
    let mut servers: Vec<(String, &'static str)> = mcp_names(conf.as_ref())
        .into_iter()
        .map(|n| (n, "user"))
        .collect();
    if let (Some(c), Some(conf)) = (&cwd, &conf) {
        let key = c.display().to_string();
        let projects = conf.get("projects").and_then(|p| p.as_object());
        let entry = projects.and_then(|p| p.get(&key).or_else(|| p.get(&key.replace('\\', "/"))));
        servers.extend(mcp_names(entry).into_iter().map(|n| (n, "local")));
        let file: Option<serde_json::Value> = std::fs::read_to_string(c.join(".mcp.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok());
        servers.extend(mcp_names(file.as_ref()).into_iter().map(|n| (n, "project")));
    }
    for (name, scope) in servers {
        let unused = !used_mcp.contains(&tool_name(&name));
        items.push(Item {
            kind: "mcp",
            fix: unused.then(|| crate::tr!("一个月没调用过。用不上的话：claude mcp remove \"{name}\" -s {scope}", "Not called in a month. If you don't need it: claude mcp remove \"{name}\" -s {scope}")),
            name,
            path: None,
            tokens: 0,
            unused,
        });
    }
    (cwd.map(|c| c.display().to_string()), items)
}

fn codex_startup(root: &Path) -> (Option<String>, Vec<Item>) {
    let mut items = vec![];
    // The newest session's folder.
    let newest = sessions_in(&root.join("sessions"))
        .into_iter()
        .max_by_key(|(_, _, t)| *t)
        .map(|(p, _, _)| p);
    let cwd = newest.as_ref().and_then(|p| {
        head_lines(p, 3).iter().find_map(|l| {
            serde_json::from_str::<serde_json::Value>(l)
                .ok()?
                .pointer("/payload/cwd")?
                .as_str()
                .map(PathBuf::from)
        })
    });
    for n in ["AGENTS.override.md", "AGENTS.md"] {
        if let Some(it) = file_item(
            "file",
            &root.join(n),
            Some(crate::tr!(
                "每个会话都会读：只留真正每次都要的。",
                "Read by every session: keep only what is needed every time."
            )),
        ) {
            items.push(it);
        }
    }
    if let Some(c) = &cwd {
        for p in up_the_tree(c, &["AGENTS.override.md", "AGENTS.md"]) {
            if let Some(it) = file_item(
                "file",
                &p,
                Some(crate::tr!(
                    "在这个文件夹下开的会话都会读：重复的、过时的段落可以删。",
                    "Read by every session in this folder: drop repeated or outdated parts."
                )),
            ) {
                items.push(it);
            }
        }
    }
    for (name, t, p) in skills(&root.join("skills")) {
        items.push(Item {
            kind: "skill",
            name,
            path: Some(p.display().to_string()),
            tokens: t,
            unused: false,
            fix: None,
        });
    }
    // MCP servers in config.toml: `[mcp_servers.name]`.
    if let Some(conf) = read(&root.join("config.toml")) {
        for l in conf.lines() {
            let l = l.trim();
            if let Some(name) = l
                .strip_prefix("[mcp_servers.")
                .and_then(|r| r.strip_suffix(']'))
            {
                if !name.contains('.') {
                    items.push(Item {
                        kind: "mcp",
                        name: name.trim_matches('"').to_string(),
                        path: None,
                        tokens: 0,
                        unused: false,
                        fix: None,
                    });
                }
            }
        }
    }
    (cwd.map(|c| c.display().to_string()), items)
}

/// Count everything. Reads the last month's Claude Code sessions once, so it takes a few seconds.
pub fn count(home: &Path) -> Report {
    let mut agents = vec![];
    let since = cutoff(OLD_DAYS);
    for (id, name) in [("claude", "Claude Code"), ("codex", "Codex")] {
        let root = root_of(id).unwrap();
        if !root.is_dir() {
            continue;
        }
        let files = sessions_in(&session_dir(id, &root));
        let old: Vec<_> = files.iter().filter(|(_, _, t)| *t < since).collect();
        let (project, mut startup) = if id == "claude" {
            claude_startup(&root)
        } else {
            codex_startup(&root)
        };
        startup.sort_by(|a, b| b.tokens.cmp(&a.tokens).then(b.unused.cmp(&a.unused)));
        agents.push(Store {
            id,
            name,
            root: root.display().to_string(),
            total: dir_size(&root),
            sessions: files.iter().map(|f| f.1).sum(),
            session_files: files.len() as u32,
            old: old.iter().map(|f| f.1).sum(),
            old_files: old.len() as u32,
            cleans_itself: id == "claude",
            project,
            startup_tokens: startup.iter().map(|i| i.tokens).sum(),
            startup,
        });
    }
    Report {
        agents,
        boxes: boxes(home),
        parked: parked(home),
        at: crate::now_ms(),
    }
}

fn parked_dir(home: &Path) -> PathBuf {
    box_dir(home).join("skills")
}

/// Skills set aside so far.
pub fn parked(home: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(parked_dir(home))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

/// A plain folder name (no separators, no `..`).
fn plain(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\', ':'])
}

/// Set Claude Code skills aside: their folders move from its `skills` into the storeroom, so new
/// sessions no longer list them. Returns how many moved. `back` puts them back.
pub fn park(home: &Path, names: &[String], back: bool) -> Result<u32, String> {
    park_in(home, &claude_root().join("skills"), names, back)
}

fn park_in(home: &Path, skills: &Path, names: &[String], back: bool) -> Result<u32, String> {
    let _busy = Busy::take()?;
    let shelf = parked_dir(home);
    std::fs::create_dir_all(&shelf).map_err(|e| e.to_string())?;
    let mut n = 0;
    for name in names.iter().filter(|n| plain(n)) {
        let (from, to) = if back {
            (shelf.join(name), skills.join(name))
        } else {
            (skills.join(name), shelf.join(name))
        };
        if !from.is_dir() || to.exists() {
            continue;
        }
        std::fs::rename(&from, &to).map_err(|e| format!("{name}: {e}"))?;
        n += 1;
    }
    Ok(n)
}

/// Each agent's old sessions on disk: (agent id, bytes). Sizes only: quick enough to check now and then.
pub fn old_by_agent() -> Vec<(&'static str, u64)> {
    let since = cutoff(OLD_DAYS);
    ["claude", "codex"]
        .into_iter()
        .filter_map(|id| {
            let root = root_of(id)?;
            Some((
                id,
                sessions_in(&session_dir(id, &root))
                    .iter()
                    .filter(|f| f.2 < since)
                    .map(|f| f.1)
                    .sum::<u64>(),
            ))
        })
        .collect()
}

/// The agent with the most old sessions on disk, and how much.
pub fn heaviest_old() -> Option<(&'static str, u64)> {
    old_by_agent()
        .into_iter()
        .map(|(id, b)| (if id == "claude" { "Claude Code" } else { "Codex" }, b))
        .max_by_key(|(_, b)| *b)
}

fn box_dir(home: &Path) -> PathBuf {
    home.join("storeroom")
}

/// The boxes packed so far.
pub fn boxes(home: &Path) -> Vec<Boxed> {
    let mut out: Vec<Boxed> = std::fs::read_dir(box_dir(home))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "zip"))
        .filter_map(|e| {
            let m = manifest(&e.path()).ok()?;
            Some(Boxed {
                file: e.path().display().to_string(),
                agent: m.agent,
                files: m.files.len() as u32,
                bytes: m.files.iter().map(|f| f.bytes).sum(),
                packed: e.metadata().ok()?.len(),
                at: m.at,
            })
        })
        .collect();
    out.sort_by(|a, b| b.at.cmp(&a.at));
    out
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
struct Manifest {
    agent: String,
    root: String,
    at: u64,
    files: Vec<Packed>,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
struct Packed {
    /// Path under the agent's folder, with `/`.
    path: String,
    bytes: u64,
    /// When it was last written (seconds since 1970), put back on unpacking.
    mtime: u64,
}

fn manifest(zip_path: &Path) -> Result<Manifest, String> {
    let f = std::fs::File::open(zip_path).map_err(|e| e.to_string())?;
    let mut z = zip::ZipArchive::new(f).map_err(|e| e.to_string())?;
    let mut s = String::new();
    z.by_name(MANIFEST)
        .map_err(|e| e.to_string())?
        .read_to_string(&mut s)
        .map_err(|e| e.to_string())?;
    serde_json::from_str(&s).map_err(|e| e.to_string())
}

/// Only one packing or unpacking at a time.
static BUSY: AtomicBool = AtomicBool::new(false);

/// Holds `BUSY` while it lives.
struct Busy;
impl Busy {
    fn take() -> Result<Busy, String> {
        if BUSY.swap(true, Ordering::SeqCst) {
            return Err(crate::tr!("正在收拾，等一下", "Busy tidying, hang on"));
        }
        Ok(Busy)
    }
}
impl Drop for Busy {
    fn drop(&mut self) {
        BUSY.store(false, Ordering::SeqCst);
    }
}

/// Pack an agent's old sessions into one zip in Rill's folder, then remove the originals.
/// `progress(done, total)` is called as it goes. Sessions written in the last `OLD_DAYS` are never touched.
pub fn pack(home: &Path, agent: &str, progress: impl Fn(u32, u32)) -> Result<Boxed, String> {
    let root = root_of(agent).ok_or_else(|| crate::tr!("没有这个 agent", "No such agent"))?;
    pack_from(home, agent, &root, progress)
}

fn pack_from(
    home: &Path,
    agent: &str,
    root: &Path,
    progress: impl Fn(u32, u32),
) -> Result<Boxed, String> {
    let _busy = Busy::take()?;
    let root = root.to_path_buf();
    let since = cutoff(OLD_DAYS);
    let old: Vec<_> = sessions_in(&session_dir(agent, &root))
        .into_iter()
        .filter(|(_, _, t)| *t < since)
        .collect();
    if old.is_empty() {
        return Err(crate::tr!(
            "没有一个月以前的会话了",
            "No sessions older than a month"
        ));
    }
    std::fs::create_dir_all(box_dir(home)).map_err(|e| e.to_string())?;
    let at = crate::now_ms();
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let path = box_dir(home).join(format!("{agent}-{stamp}.zip"));
    let rel = |p: &Path| {
        p.strip_prefix(&root)
            .map(|r| r.to_string_lossy().replace('\\', "/"))
            .ok()
    };
    let mut list = vec![];
    {
        let f = std::fs::File::create(&path).map_err(|e| e.to_string())?;
        let mut z = zip::ZipWriter::new(std::io::BufWriter::new(f));
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .large_file(true);
        let total = old.len() as u32;
        for (i, (p, bytes, t)) in old.iter().enumerate() {
            let Some(name) = rel(p) else { continue };
            let data = std::fs::read(p).map_err(|e| format!("{}: {e}", p.display()))?;
            z.start_file(name.clone(), opts)
                .map_err(|e| e.to_string())?;
            z.write_all(&data).map_err(|e| e.to_string())?;
            list.push(Packed {
                path: name,
                bytes: *bytes,
                mtime: t
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0),
            });
            if i % 10 == 0 {
                progress(i as u32, total);
            }
        }
        let m = Manifest {
            agent: agent.into(),
            root: root.display().to_string(),
            at,
            files: list.clone(),
        };
        z.start_file(MANIFEST, opts).map_err(|e| e.to_string())?;
        z.write_all(serde_json::to_string(&m).unwrap().as_bytes())
            .map_err(|e| e.to_string())?;
        z.finish()
            .map_err(|e| e.to_string())?
            .flush()
            .map_err(|e| e.to_string())?;
    }
    // Check the box before anything is removed: every file in it, at its size.
    let check = (|| -> Result<(), String> {
        let mut z = zip::ZipArchive::new(std::fs::File::open(&path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        for f in &list {
            if z.by_name(&f.path).map_err(|e| e.to_string())?.size() != f.bytes {
                return Err(crate::tr!(
                    "{} 打包后大小不对",
                    "{} came out the wrong size",
                    f.path
                ));
            }
        }
        Ok(())
    })();
    if let Err(e) = check {
        let _ = std::fs::remove_file(&path);
        return Err(e);
    }
    for f in &list {
        let _ = std::fs::remove_file(root.join(&f.path));
    }
    // Folders left empty (Codex keeps one per day).
    let mut dirs: Vec<PathBuf> = list
        .iter()
        .filter_map(|f| root.join(&f.path).parent().map(Path::to_path_buf))
        .collect();
    dirs.sort();
    dirs.dedup();
    let top = session_dir(agent, &root);
    for d in dirs.iter().rev() {
        for a in d.ancestors() {
            if a == top || !a.starts_with(&top) || std::fs::remove_dir(a).is_err() {
                break;
            }
        }
    }
    progress(list.len() as u32, list.len() as u32);
    Ok(Boxed {
        file: path.display().to_string(),
        agent: agent.into(),
        files: list.len() as u32,
        bytes: list.iter().map(|f| f.bytes).sum(),
        packed: std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0),
        at,
    })
}

/// Put a box's sessions back where they were (a file that is there again is left alone), then remove the box.
pub fn unpack(home: &Path, file: &str) -> Result<u32, String> {
    let path = PathBuf::from(file);
    if path.parent() != Some(box_dir(home).as_path()) {
        return Err(crate::tr!(
            "不是储物间里的箱子",
            "Not a box from the storeroom"
        ));
    }
    let _busy = Busy::take()?;
    let m = manifest(&path)?;
    let root = PathBuf::from(&m.root);
    let mut z = zip::ZipArchive::new(std::fs::File::open(&path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let mut n = 0;
    for f in &m.files {
        // Only plain relative paths under the agent's folder.
        if f.path.contains("..") || f.path.starts_with('/') || f.path.contains(':') {
            continue;
        }
        let to = root.join(&f.path);
        if to.exists() {
            continue;
        }
        if let Some(d) = to.parent() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        let mut data = vec![];
        z.by_name(&f.path)
            .map_err(|e| e.to_string())?
            .read_to_end(&mut data)
            .map_err(|e| e.to_string())?;
        let out = std::fs::File::create(&to).map_err(|e| e.to_string())?;
        (&out).write_all(&data).map_err(|e| e.to_string())?;
        let _ = out.set_modified(UNIX_EPOCH + Duration::from_secs(f.mtime));
        n += 1;
    }
    drop(z);
    std::fs::remove_file(&path).map_err(|e| e.to_string())?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Packing and parking take the one storeroom lock: their tests take turns.
    static TURN: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// `cargo test -p rillbuddy-petd count_this_machine -- --ignored --nocapture`: what this machine's agents keep (read only).
    #[test]
    #[ignore]
    fn count_this_machine() {
        let t0 = std::time::Instant::now();
        let r = count(&std::env::temp_dir());
        println!("counted in {:?}", t0.elapsed());
        for a in &r.agents {
            println!("{} total {} MB, sessions {} MB ({}), old {} MB ({}), project {:?}, startup ~{} tokens", a.name, a.total >> 20, a.sessions >> 20, a.session_files, a.old >> 20, a.old_files, a.project, a.startup_tokens);
            for i in &a.startup {
                println!(
                    "   {:6} {:>6} {:5} {}",
                    i.kind,
                    i.tokens,
                    if i.unused { "unused" } else { "" },
                    i.name
                );
            }
        }
    }

    #[test]
    fn skills_set_aside_and_back() {
        let _turn = TURN.lock().unwrap_or_else(|e| e.into_inner());
        let home = tempfile::tempdir().unwrap();
        let skills = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(skills.path().join("film")).unwrap();
        std::fs::write(
            skills.path().join("film/SKILL.md"),
            "---\ndescription: x\n---\n",
        )
        .unwrap();
        let names = vec![
            "film".to_string(),
            "../escape".to_string(),
            "missing".to_string(),
        ];
        assert_eq!(
            park_in(home.path(), skills.path(), &names, false).unwrap(),
            1
        );
        assert!(!skills.path().join("film").exists());
        assert_eq!(parked(home.path()), ["film"]);
        assert_eq!(
            park_in(home.path(), skills.path(), &names, true).unwrap(),
            1
        );
        assert!(skills.path().join("film/SKILL.md").exists());
        assert!(parked(home.path()).is_empty());
    }

    #[test]
    fn used_skills_and_servers_are_found_however_the_json_is_spaced() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("p")).unwrap();
        std::fs::write(
            dir.path().join("p/s.jsonl"),
            "{\"input\":{\"skill\":\"notes\"}}
{\"input\": {\"skill\": \"film\"}}
{\"name\":\"mcp__claude_ai_Docs__read\"}
",
        )
        .unwrap();
        let (servers, skills) = claude_used(dir.path());
        assert_eq!(skills.into_iter().collect::<Vec<_>>(), ["film", "notes"]);
        assert_eq!(servers.into_iter().collect::<Vec<_>>(), ["claude_ai_Docs"]);
        assert_eq!(tool_name("claude.ai Docs"), "claude_ai_Docs");
    }

    #[test]
    fn tokens_are_roughly_counted() {
        assert_eq!(tokens("abcd"), 1);
        assert_eq!(tokens("你好"), 2);
        assert_eq!(tokens(""), 0);
    }

    #[test]
    fn a_skill_line_is_its_name_and_description() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("tidy")).unwrap();
        std::fs::write(dir.path().join("tidy/SKILL.md"), "---\nname: tidy\ndescription: Tidy the desk,\n  and the drawers.\nother: x\n---\nBody that is not listed.\n").unwrap();
        let s = skills(dir.path());
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].0, "tidy");
        assert!(
            s[0].1 < 30,
            "only the front matter's description: {}",
            s[0].1
        );
    }

    #[test]
    fn old_sessions_pack_and_unpack_with_their_dates() {
        let _turn = TURN.lock().unwrap_or_else(|e| e.into_inner());
        let home = tempfile::tempdir().unwrap();
        let agent_root = tempfile::tempdir().unwrap();
        // Codex keeps sessions in sessions/YYYY/MM/DD.
        let day = agent_root.path().join("sessions/2026/01/02");
        std::fs::create_dir_all(&day).unwrap();
        let old = day.join("rollout-a.jsonl");
        std::fs::write(&old, "{\"type\":\"session_meta\"}\n".repeat(200)).unwrap();
        let long_ago = SystemTime::now() - Duration::from_secs(60 * 86_400);
        std::fs::File::options()
            .write(true)
            .open(&old)
            .unwrap()
            .set_modified(long_ago)
            .unwrap();
        let fresh = agent_root
            .path()
            .join("sessions/2026/11/12/rollout-b.jsonl");
        std::fs::create_dir_all(fresh.parent().unwrap()).unwrap();
        std::fs::write(&fresh, "{}\n").unwrap();

        let b = pack_from(home.path(), "codex", agent_root.path(), |_, _| {}).unwrap();
        assert_eq!(b.files, 1);
        assert!(b.packed < b.bytes, "it is smaller in the box");
        assert!(
            !old.exists() && !day.exists(),
            "the original and its empty day folder are gone"
        );
        assert!(fresh.exists(), "a recent one is never touched");
        assert_eq!(boxes(home.path()).len(), 1);

        assert_eq!(unpack(home.path(), &b.file).unwrap(), 1);
        assert!(old.exists());
        let t = std::fs::metadata(&old).unwrap().modified().unwrap();
        assert!(
            t < SystemTime::now() - Duration::from_secs(59 * 86_400),
            "it keeps its date, so it is still old"
        );
        assert!(boxes(home.path()).is_empty());
        // Only boxes in the storeroom can be unpacked.
        assert!(unpack(home.path(), "C:/elsewhere/x.zip").is_err());
    }
}
