//! `--install codex` / `--uninstall codex`: add or remove our hooks in
//! `$CODEX_HOME/hooks.json` (default `~/.codex`).
//!
//! `--install gemini` does the same in Gemini CLI's `~/.gemini/settings.json`,
//! `--install cursor` in Cursor's `~/.cursor/hooks.json` (`--config-dir <dir>`
//! points either at another folder).
//!
//! The user's own hooks are kept as they are: our groups are appended after
//! theirs, so the indexes Codex uses to remember which hooks were trusted
//! do not move. Key order is preserved (serde_json `preserve_order`).

use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

const EVENTS: &[&str] = &[
    "SessionStart",
    "SessionEnd",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "PermissionRequest",
    "Stop",
    "Interrupt",
    "SubagentStart",
    "SubagentStop",
];

const MARK: &str = "rillbuddy-emit";
/// Hooks written before the renames (the project was called Familiar, and AgentPets before that).
const OLD_MARKS: [&str; 2] = ["familiar-emit", "agentpets-emit"];

/// A hook command that runs one of our emitters, under any of its names.
pub(crate) fn runs_us(cmd: &str) -> bool {
    cmd.contains(MARK) || OLD_MARKS.iter().any(|m| cmd.contains(m))
}

pub fn run(target: &str, install: bool, args: &[String]) {
    if target == "statusline" {
        match super::statusline::install(install) {
            Ok(msg) => println!("{msg}"),
            Err(e) => println!("失败：{e}"),
        }
        return;
    }
    if let Some(agent) = AGENTS.iter().find(|a| a.id == target) {
        let dir = super::flag(args, "--config-dir").map(PathBuf::from).unwrap_or_else(|| user_home().join(agent.dir));
        match agent_install(agent, &dir.join(agent.file), install) {
            Ok(msg) => println!("{msg}"),
            Err(e) => println!("失败：{e}"),
        }
        return;
    }
    if target == "opencode" {
        let dir = super::flag(args, "--config-dir").map(PathBuf::from).unwrap_or_else(|| user_home().join(".config").join("opencode"));
        // The plugin was familiar.js before the rename: it goes either way.
        let old = dir.join("plugins").join("familiar.js");
        if std::fs::read_to_string(&old).is_ok_and(|t| opencode_ours(&t)) {
            let _ = std::fs::remove_file(&old);
        }
        match opencode_install(&dir.join("plugins").join("rillbuddy.js"), install) {
            Ok(msg) => println!("{msg}"),
            Err(e) => println!("失败：{e}"),
        }
        return;
    }
    if target != "codex" {
        println!("支持 codex、gemini、cursor、opencode、statusline：rillbuddy-emit --install <名字>");
        return;
    }
    let codex_home = super::flag(args, "--codex-home").map(PathBuf::from).unwrap_or_else(codex_home);
    let file = codex_home.join("hooks.json");
    let result = if install { do_install(&file) } else { do_uninstall(&file) };
    match result {
        Ok(msg) => println!("{msg}"),
        Err(e) => println!("失败：{e}"),
    }
}

fn user_home() -> PathBuf {
    PathBuf::from(std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).unwrap_or_default())
}

/// An agent whose hooks live in a JSON file in its own config folder.
struct Agent {
    id: &'static str,
    name: &'static str,
    /// Config folder under the user's home, and the file in it.
    dir: &'static str,
    file: &'static str,
    events: &'static [&'static str],
    /// Gemini CLI groups hooks like Claude Code does (`{hooks: [{type, command}]}`),
    /// with timeouts in ms; Cursor lists bare `{command}` entries under a `version`.
    grouped: bool,
}

const AGENTS: &[Agent] = &[
    Agent {
        id: "gemini",
        name: "Gemini CLI",
        dir: ".gemini",
        file: "settings.json",
        events: &["SessionStart", "SessionEnd", "BeforeAgent", "BeforeTool", "AfterTool", "Notification", "AfterAgent"],
        grouped: true,
    },
    Agent {
        id: "cursor",
        name: "Cursor",
        dir: ".cursor",
        file: "hooks.json",
        events: &["sessionStart", "sessionEnd", "beforeSubmitPrompt", "preToolUse", "postToolUse", "postToolUseFailure", "stop"],
        grouped: false,
    },
];

/// The command line an agent runs for our hook. Gemini CLI and Cursor both
/// run hook commands through PowerShell on Windows, where a quoted program
/// path needs the call operator.
fn agent_command(exe: &Path, id: &str) -> String {
    let p = exe.display().to_string().replace('\\', "/");
    if cfg!(windows) {
        format!("& \"{p}\" --source {id}")
    } else {
        format!("\"{p}\" --source {id}")
    }
}

fn entry_is_ours(e: &Value) -> bool {
    e.get("command").and_then(Value::as_str).is_some_and(runs_us)
}

/// Remove our entries from an agent's hooks. Returns how many went.
fn agent_strip(a: &Agent, root: &mut Value) -> usize {
    let Some(hooks) = root.get_mut("hooks").and_then(Value::as_object_mut) else { return 0 };
    if a.grouped {
        return strip_ours(hooks);
    }
    let mut n = 0;
    for list in hooks.values_mut() {
        if let Some(arr) = list.as_array_mut() {
            let before = arr.len();
            arr.retain(|e| !entry_is_ours(e));
            n += before - arr.len();
        }
    }
    hooks.retain(|_, l| l.as_array().is_none_or(|a| !a.is_empty()));
    n
}

/// The agent's config with our hooks appended after the user's own.
fn agent_merge(a: &Agent, mut root: Value, exe: &Path) -> Value {
    if !root.is_object() {
        root = json!({});
    }
    agent_strip(a, &mut root);
    let obj = root.as_object_mut().unwrap();
    if !a.grouped && !obj.contains_key("version") {
        obj.insert("version".into(), json!(1));
    }
    if !obj.get("hooks").is_some_and(Value::is_object) {
        obj.insert("hooks".into(), json!({}));
    }
    let hooks = obj.get_mut("hooks").unwrap().as_object_mut().unwrap();
    let cmd = agent_command(exe, a.id);
    for ev in a.events {
        let ours = if a.grouped {
            json!({ "hooks": [{ "name": "rillbuddy", "type": "command", "command": cmd, "timeout": 5000 }] })
        } else {
            json!({ "command": cmd, "timeout": 5 })
        };
        if let Some(arr) = hooks.entry(ev.to_string()).or_insert_with(|| json!([])).as_array_mut() {
            arr.push(ours);
        }
    }
    root
}

fn agent_install(a: &Agent, file: &Path, install: bool) -> Result<String, String> {
    let mut root = read(file)?;
    if install {
        let exe = stage_emitter()?;
        write(file, &agent_merge(a, root, &exe))?;
        Ok(format!("已把睿洱的 {} 个 hook 写入 {}（原有的 hook 保留，写入前已备份）。\n重开 {} 后生效。", a.events.len(), file.display(), a.name))
    } else {
        let n = agent_strip(a, &mut root);
        if n == 0 {
            return Ok(format!("{} 里没有睿洱的 hook。", file.display()));
        }
        // Gemini CLI's settings.json had no hooks before us: do not leave an empty section behind.
        if a.grouped && root.get("hooks").and_then(Value::as_object).is_some_and(|h| h.is_empty()) {
            if let Some(obj) = root.as_object_mut() {
                obj.shift_remove("hooks");
            }
        }
        write(file, &root)?;
        Ok(format!("已从 {} 移除 {n} 个睿洱 hook。", file.display()))
    }
}

/// OpenCode has no command hooks: a small plugin file forwards its events to us.
const OPENCODE_PLUGIN: &str = include_str!("../../../adapters/opencode/rillbuddy.js");

/// The plugin with the path of this emitter written into it.
fn opencode_plugin(exe: &Path) -> String {
    let path = serde_json::to_string(&exe.display().to_string().replace('\\', "/")).unwrap_or_default();
    OPENCODE_PLUGIN.replace("\"__RILLBUDDY_EMIT__\"", &path)
}

/// A plugin file we wrote (this one, or the one from before the rename).
fn opencode_ours(text: &str) -> bool {
    runs_us(text) && (text.contains("RillBuddy: forwards OpenCode session events") || text.contains("Familiar: forwards OpenCode session events"))
}

fn opencode_install(file: &Path, install: bool) -> Result<String, String> {
    let existing = std::fs::read_to_string(file).ok();
    if existing.as_deref().is_some_and(|t| !opencode_ours(t)) {
        return Err(format!("{} 不是睿洱写的文件，没有动它", file.display()));
    }
    if install {
        let exe = stage_emitter()?;
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(file, opencode_plugin(&exe)).map_err(|e| e.to_string())?;
        Ok(format!("已把睿洱的插件写到 {}。\n重开 OpenCode 后生效。", file.display()))
    } else if existing.is_some() {
        std::fs::remove_file(file).map_err(|e| e.to_string())?;
        Ok(format!("已删除 {}。", file.display()))
    } else {
        Ok(format!("{} 不存在，没有可卸载的。", file.display()))
    }
}

fn codex_home() -> PathBuf {
    if let Some(h) = std::env::var_os("CODEX_HOME").filter(|h| !h.is_empty()) {
        return h.into();
    }
    let base = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).unwrap_or_default();
    PathBuf::from(base).join(".codex")
}

/// Copy this exe to ~/.rillbuddy/bin so the hooks do not depend on where the
/// repo or installer lives.
pub(crate) fn stage_emitter() -> Result<PathBuf, String> {
    let me = std::env::current_exe().map_err(|e| e.to_string())?;
    let dir = super::home().join("bin");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let dst = dir.join(me.file_name().unwrap_or_default());
    let same = std::fs::canonicalize(&me).ok() == std::fs::canonicalize(&dst).ok();
    if !same {
        // A hook may be running the old copy; write next to it and swap.
        let tmp = dst.with_extension("new");
        std::fs::copy(&me, &tmp).map_err(|e| format!("复制 emitter 失败：{e}"))?;
        if std::fs::rename(&tmp, &dst).is_err() {
            let _ = std::fs::remove_file(&dst);
            std::fs::rename(&tmp, &dst).map_err(|e| format!("替换 emitter 失败：{e}"))?;
        }
    }
    Ok(dst)
}

pub fn hook_command(exe: &Path) -> (String, Option<String>) {
    let p = exe.display().to_string().replace('\\', "/");
    if p.contains(' ') {
        // Quoted paths break under PowerShell, so give Windows a cmd form.
        let win = format!("cmd /d /s /c \"\"{}\" --source codex\"", exe.display());
        (format!("\"{p}\" --source codex"), Some(win))
    } else {
        (format!("{p} --source codex"), None)
    }
}

pub(crate) fn read(file: &Path) -> Result<Value, String> {
    match std::fs::read_to_string(file) {
        Ok(t) if t.trim().is_empty() => Ok(json!({})),
        Ok(t) => serde_json::from_str(&t).map_err(|e| format!("{} 不是合法 JSON：{e}", file.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(json!({})),
        Err(e) => Err(e.to_string()),
    }
}

pub(crate) fn write(file: &Path, v: &Value) -> Result<(), String> {
    if file.exists() {
        let bak = file.with_extension("json.rillbuddy-bak");
        std::fs::copy(file, &bak).map_err(|e| format!("备份失败：{e}"))?;
    } else if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(v).map_err(|e| e.to_string())? + "\n";
    std::fs::write(file, text).map_err(|e| e.to_string())
}

/// Drop every group whose handlers are all ours. Returns how many were removed.
fn strip_ours(hooks: &mut Map<String, Value>) -> usize {
    let mut n = 0;
    for groups in hooks.values_mut() {
        if let Some(arr) = groups.as_array_mut() {
            let before = arr.len();
            arr.retain(|g| !is_ours(g));
            n += before - arr.len();
        }
    }
    hooks.retain(|_, g| g.as_array().is_none_or(|a| !a.is_empty()));
    n
}

fn is_ours(group: &Value) -> bool {
    let Some(hs) = group.get("hooks").and_then(Value::as_array) else { return false };
    !hs.is_empty()
        && hs.iter().all(|h| h.get("command").and_then(Value::as_str).is_some_and(runs_us))
}

pub fn merge(mut root: Value, exe: &Path) -> Value {
    if !root.is_object() {
        root = json!({});
    }
    let obj = root.as_object_mut().unwrap();
    if !obj.get("hooks").is_some_and(Value::is_object) {
        obj.insert("hooks".into(), json!({}));
    }
    let hooks = obj.get_mut("hooks").unwrap().as_object_mut().unwrap();
    strip_ours(hooks);
    let (cmd, win) = hook_command(exe);
    for ev in EVENTS {
        let mut h = json!({ "type": "command", "command": cmd, "timeout": 5 });
        if let Some(w) = &win {
            h["commandWindows"] = w.as_str().into();
        }
        let entry = hooks.entry(ev.to_string()).or_insert_with(|| json!([]));
        if let Some(arr) = entry.as_array_mut() {
            arr.push(json!({ "hooks": [h] }));
        }
    }
    root
}

fn do_install(file: &Path) -> Result<String, String> {
    let exe = stage_emitter()?;
    let root = read(file)?;
    write(file, &merge(root, &exe))?;
    Ok(format!(
        "已把睿洱的 {} 个 hook 写入 {}\nemitter：{}\n\n下一步：在终端里运行 Codex 命令行版（codex），输入 /hooks，把新出现的睿洱 hook 标记为信任。\n桌面应用里没有 /hooks；信任记录和桌面应用共用同一个 Codex 配置目录。\nCodex 只运行信任过的 hook，没信任之前宠物收不到 Codex 的事件。",
        EVENTS.len(),
        file.display(),
        exe.display()
    ))
}

fn do_uninstall(file: &Path) -> Result<String, String> {
    let mut root = read(file)?;
    let n = root.get_mut("hooks").and_then(Value::as_object_mut).map(strip_ours).unwrap_or(0);
    if n == 0 {
        return Ok(format!("{} 里没有睿洱的 hook。", file.display()));
    }
    write(file, &root)?;
    Ok(format!("已从 {} 移除 {n} 组睿洱 hook。", file.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_keeps_user_hooks_first_and_is_idempotent() {
        let user = json!({
            "hooks": {
                "Stop": [ { "hooks": [ { "type": "command", "command": "orca-hook.cmd", "timeout": 10 } ] } ],
                "PreToolUse": [ { "matcher": "Bash", "hooks": [ { "type": "command", "command": "policy.py" } ] } ]
            },
            "other": 1
        });
        let exe = Path::new("C:/Users/me/.rillbuddy/bin/rillbuddy-emit.exe");
        let once = merge(user.clone(), exe);
        let twice = merge(once.clone(), exe);
        assert_eq!(once, twice, "re-install must not duplicate");
        let old = merge(user.clone(), Path::new("C:/Users/me/.familiar/bin/familiar-emit.exe"));
        assert_eq!(merge(old, exe), once, "hooks from before the rename are replaced, not doubled");
        let stop = once["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop.len(), 2);
        assert_eq!(stop[0]["hooks"][0]["command"], "orca-hook.cmd", "user group keeps index 0");
        assert_eq!(stop[1]["hooks"][0]["command"], "C:/Users/me/.rillbuddy/bin/rillbuddy-emit.exe --source codex");
        assert_eq!(once["other"], 1);
        // Original key order survives.
        let keys: Vec<&String> = once["hooks"].as_object().unwrap().keys().collect();
        assert_eq!(keys[0], "Stop");
        assert_eq!(keys[1], "PreToolUse");

        let mut back = once.clone();
        let n = strip_ours(back["hooks"].as_object_mut().unwrap());
        assert_eq!(n, EVENTS.len());
        assert_eq!(back, user);
    }

    #[test]
    fn gemini_and_cursor_keep_user_hooks_and_come_out_clean() {
        let exe = Path::new("C:/Users/me/.rillbuddy/bin/rillbuddy-emit.exe");
        let gemini = AGENTS.iter().find(|a| a.id == "gemini").unwrap();
        let user = json!({ "theme": "dark", "hooks": { "BeforeTool": [ { "matcher": "write_file", "hooks": [ { "name": "mine", "type": "command", "command": "check.sh" } ] } ] } });
        let once = agent_merge(gemini, user.clone(), exe);
        assert_eq!(agent_merge(gemini, once.clone(), exe), once, "re-install must not duplicate");
        let before = once["hooks"]["BeforeTool"].as_array().unwrap();
        assert_eq!(before.len(), 2);
        assert_eq!(before[0]["hooks"][0]["command"], "check.sh", "the user's hook stays first");
        let ours = &before[1]["hooks"][0];
        assert!(ours["command"].as_str().unwrap().ends_with("rillbuddy-emit.exe\" --source gemini"), "{ours}");
        assert_eq!((ours["type"].as_str(), ours["timeout"].as_u64()), (Some("command"), Some(5000)), "Gemini CLI timeouts are in ms");
        assert!(once["hooks"].get("AfterModel").is_none(), "fires per chunk: never hooked");
        let mut back = once;
        assert_eq!(agent_strip(gemini, &mut back), gemini.events.len());
        assert_eq!(back, user);

        let cursor = AGENTS.iter().find(|a| a.id == "cursor").unwrap();
        let user = json!({ "version": 1, "hooks": { "stop": [ { "command": "./hooks/audit.sh" } ] } });
        let once = agent_merge(cursor, user.clone(), exe);
        assert_eq!(agent_merge(cursor, once.clone(), exe), once);
        let stop = once["hooks"]["stop"].as_array().unwrap();
        assert_eq!((stop.len(), stop[0]["command"].as_str()), (2, Some("./hooks/audit.sh")));
        assert!(stop[1]["command"].as_str().unwrap().contains("--source cursor"));
        assert_eq!(agent_merge(cursor, json!({}), exe)["version"], 1, "a fresh hooks.json gets its version");
        let mut back = once;
        assert_eq!(agent_strip(cursor, &mut back), cursor.events.len());
        assert_eq!(back, user);
    }

    #[test]
    fn the_opencode_plugin_carries_the_emitter_path_and_only_replaces_itself() {
        let js = opencode_plugin(Path::new("C:\\Users\\me\\.rillbuddy\\bin\\rillbuddy-emit.exe"));
        assert!(js.contains(r#"const EMIT = "C:/Users/me/.rillbuddy/bin/rillbuddy-emit.exe""#), "the placeholder is filled in");
        assert!(!js.contains("__RILLBUDDY_EMIT__"));

        let dir = std::env::temp_dir().join(format!("rillbuddy-oc-test-{}", std::process::id()));
        let file = dir.join("plugins").join("rillbuddy.js");
        assert!(opencode_install(&file, false).unwrap().contains("不存在"));
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, "export default {} // somebody else's plugin").unwrap();
        assert!(opencode_install(&file, false).is_err(), "a file we did not write is left alone");
        assert!(opencode_install(&file, true).is_err());
        std::fs::write(&file, &js).unwrap();
        assert!(opencode_install(&file, false).unwrap().contains("已删除"));
        assert!(!file.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn paths_with_spaces_get_a_windows_form() {
        let (cmd, win) = hook_command(Path::new("C:/Program Files/RillBuddy/rillbuddy-emit.exe"));
        assert!(cmd.starts_with('"'));
        assert!(win.unwrap().starts_with("cmd /d /s /c"));
    }
}
