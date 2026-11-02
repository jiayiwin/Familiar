//! `--install codex` / `--uninstall codex`: add or remove our hooks in
//! `$CODEX_HOME/hooks.json` (default `~/.codex`).
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

const MARK: &str = "familiar-emit";
/// Hooks written before the rename (the project was called AgentPets).
const LEGACY_MARK: &str = "agentpets-emit";

pub fn run(target: &str, install: bool, args: &[String]) {
    if target != "codex" {
        println!("只支持 codex：familiar-emit --install codex");
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

fn codex_home() -> PathBuf {
    if let Some(h) = std::env::var_os("CODEX_HOME").filter(|h| !h.is_empty()) {
        return h.into();
    }
    let base = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).unwrap_or_default();
    PathBuf::from(base).join(".codex")
}

/// Copy this exe to ~/.familiar/bin so the hooks do not depend on where the
/// repo or installer lives.
fn stage_emitter() -> Result<PathBuf, String> {
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

fn read(file: &Path) -> Result<Value, String> {
    match std::fs::read_to_string(file) {
        Ok(t) if t.trim().is_empty() => Ok(json!({})),
        Ok(t) => serde_json::from_str(&t).map_err(|e| format!("{} 不是合法 JSON：{e}", file.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(json!({})),
        Err(e) => Err(e.to_string()),
    }
}

fn write(file: &Path, v: &Value) -> Result<(), String> {
    if file.exists() {
        let bak = file.with_extension("json.familiar-bak");
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
        && hs.iter().all(|h| h.get("command").and_then(Value::as_str).is_some_and(|c| c.contains(MARK) || c.contains(LEGACY_MARK)))
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
        "已把 Familiar 的 {} 个 hook 写入 {}\nemitter：{}\n\n下一步：在终端里运行 Codex 命令行版（codex），输入 /hooks，把新出现的 Familiar hook 标记为信任。\n桌面应用里没有 /hooks；信任记录和桌面应用共用同一个 Codex 配置目录。\nCodex 只运行信任过的 hook，没信任之前宠物收不到 Codex 的事件。",
        EVENTS.len(),
        file.display(),
        exe.display()
    ))
}

fn do_uninstall(file: &Path) -> Result<String, String> {
    let mut root = read(file)?;
    let n = root.get_mut("hooks").and_then(Value::as_object_mut).map(strip_ours).unwrap_or(0);
    if n == 0 {
        return Ok(format!("{} 里没有 Familiar 的 hook。", file.display()));
    }
    write(file, &root)?;
    Ok(format!("已从 {} 移除 {n} 组 Familiar hook。", file.display()))
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
        let exe = Path::new("C:/Users/me/.familiar/bin/familiar-emit.exe");
        let once = merge(user.clone(), exe);
        let twice = merge(once.clone(), exe);
        assert_eq!(once, twice, "re-install must not duplicate");
        let stop = once["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop.len(), 2);
        assert_eq!(stop[0]["hooks"][0]["command"], "orca-hook.cmd", "user group keeps index 0");
        assert_eq!(stop[1]["hooks"][0]["command"], "C:/Users/me/.familiar/bin/familiar-emit.exe --source codex");
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
    fn paths_with_spaces_get_a_windows_form() {
        let (cmd, win) = hook_command(Path::new("C:/Program Files/Familiar/familiar-emit.exe"));
        assert!(cmd.starts_with('"'));
        assert!(win.unwrap().starts_with("cmd /d /s /c"));
    }
}
