//! `--statusline`: Claude Code runs this as its status line command. The JSON
//! it is given on stdin is the only local place Claude Code reports the
//! account's usage limits (`rate_limits`), so this hands those numbers to petd
//! and then prints a status line: the user's own command's output when one was
//! wrapped (`--statusline --their <hex of their command>`: hex, so its quoting
//! survives the shell untouched), a short line of ours otherwise.
//!
//! `--install statusline` / `--uninstall statusline` set this up in Claude
//! Code's settings.json, keeping (and later restoring) a status line the user
//! already had.

use serde_json::{json, Value};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

const MARK: &str = "--statusline";
const THEIR: &str = "--their";

fn hex(s: &str) -> String {
    s.bytes().map(|b| format!("{b:02x}")).collect()
}

fn unhex(h: &str) -> Option<String> {
    let b: Option<Vec<u8>> = (0..h.len()).step_by(2).map(|i| u8::from_str_radix(h.get(i..i + 2)?, 16).ok()).collect();
    String::from_utf8(b?).ok().filter(|s| !s.is_empty())
}

/// The command the user had before ours wrapped it.
fn theirs_in(cmd: &str) -> Option<String> {
    let mut words = cmd.split_whitespace();
    words.find(|w| *w == THEIR)?;
    unhex(words.next()?)
}

pub fn run(args: &[String]) {
    let mut raw = Vec::new();
    let _ = std::io::stdin().lock().take(4 * 1024 * 1024).read_to_end(&mut raw);
    let input: Value = serde_json::from_slice(&raw).unwrap_or(Value::Null);
    if let Some(limits) = input.get("rate_limits").filter(|l| l.is_object()) {
        let body = json!({ "source": "claude-code", "rate_limits": limits }).to_string();
        let _ = super::post("/v1/usage", &body, Duration::from_millis(120));
    }
    let line = super::flag(args, THEIR).and_then(unhex).and_then(|theirs| through(&theirs, &raw));
    let mut so = std::io::stdout().lock();
    let _ = so.write_all(&line.unwrap_or_else(|| ours(&input).into_bytes()));
    let _ = so.flush();
}

/// The line shown when the user had no status line of their own.
fn ours(input: &Value) -> String {
    let s = |p: &str| input.pointer(p).and_then(Value::as_str).unwrap_or_default();
    let dir = s("/workspace/current_dir");
    let dir = dir.trim_end_matches(['/', '\\']).rsplit(['/', '\\']).next().unwrap_or_default();
    let pct = |p: &str| input.pointer(p).and_then(Value::as_f64).map(|v| format!("{v:.0}%"));
    let mut parts: Vec<String> = [s("/model/display_name"), dir].iter().filter(|x| !x.is_empty()).map(|x| x.to_string()).collect();
    if let Some(v) = pct("/rate_limits/five_hour/used_percentage") {
        parts.push(format!("5h {v}"));
    }
    if let Some(v) = pct("/rate_limits/seven_day/used_percentage") {
        parts.push(format!("7d {v}"));
    }
    parts.join(" · ")
}

/// Run the user's own status line command on the same input and return what it prints.
fn through(cmd: &str, stdin: &[u8]) -> Option<Vec<u8>> {
    use std::process::{Command, Stdio};
    let mut c = if cfg!(windows) {
        // Claude Code runs commands through Git Bash on Windows when it has one.
        let bash = std::env::var("CLAUDE_CODE_GIT_BASH_PATH").ok().filter(|p| !p.is_empty());
        if bash.is_some() || std::env::var_os("MSYSTEM").is_some() || std::env::var_os("SHELL").is_some() {
            let mut c = Command::new(bash.unwrap_or_else(|| "bash".into()));
            c.args(["-c", cmd]);
            c
        } else {
            let mut c = Command::new("cmd");
            c.args(["/d", "/s", "/c", cmd]);
            c
        }
    } else {
        let mut c = Command::new("sh");
        c.args(["-c", cmd]);
        c
    };
    let mut child = c.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().ok()?;
    if let Some(mut si) = child.stdin.take() {
        let _ = si.write_all(stdin);
    }
    let out = child.wait_with_output().ok()?;
    (!out.stdout.is_empty()).then_some(out.stdout)
}

fn settings_file() -> PathBuf {
    if let Some(d) = std::env::var_os("CLAUDE_CONFIG_DIR").filter(|d| !d.is_empty()) {
        return PathBuf::from(d).join("settings.json");
    }
    let base = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).unwrap_or_default();
    PathBuf::from(base).join(".claude").join("settings.json")
}

fn is_ours(cmd: &str) -> bool {
    super::install::runs_us(cmd) && cmd.contains(MARK)
}

/// settings.json with our status line in it: a status line the user already
/// had is kept (hex-encoded after `--their`) and still produces the line they see.
pub fn wrap(mut root: Value, exe: &Path) -> Result<Value, String> {
    if !root.is_object() {
        root = json!({});
    }
    let me = format!("\"{}\" {MARK}", exe.display().to_string().replace('\\', "/"));
    let obj = root.as_object_mut().unwrap();
    match obj.get_mut("statusLine") {
        None | Some(Value::Null) => {
            obj.insert("statusLine".into(), json!({ "type": "command", "command": me }));
        }
        Some(sl) => {
            let cmd = sl.get("command").and_then(Value::as_str).map(str::to_string);
            let kind = sl.get("type").and_then(Value::as_str).unwrap_or("command");
            let Some(cmd) = cmd.filter(|_| kind == "command") else {
                return Err("现有的 statusLine 不是一条命令，没有改动 / the existing statusLine is not a command; left as it is".into());
            };
            let theirs = if is_ours(&cmd) { theirs_in(&cmd) } else { Some(cmd) };
            sl["command"] = match theirs {
                Some(t) => format!("{me} {THEIR} {}", hex(&t)).into(),
                None => me.into(),
            };
        }
    }
    Ok(root)
}

/// The reverse of `wrap`. Returns whether anything was ours.
pub fn unwrap(root: &mut Value) -> bool {
    let Some(obj) = root.as_object_mut() else { return false };
    let Some(cmd) = obj.get("statusLine").and_then(|s| s.get("command")).and_then(Value::as_str).map(str::to_string) else { return false };
    if !is_ours(&cmd) {
        return false;
    }
    match theirs_in(&cmd) {
        Some(theirs) => obj["statusLine"]["command"] = theirs.into(),
        None => {
            obj.shift_remove("statusLine");
        }
    }
    true
}

pub fn install(yes: bool) -> Result<String, String> {
    let file = settings_file();
    let mut root = super::install::read(&file)?;
    if yes {
        let exe = super::install::stage_emitter()?;
        let had = root.get("statusLine").and_then(|s| s.get("command")).and_then(Value::as_str).is_some_and(|c| !is_ours(c));
        super::install::write(&file, &wrap(root, &exe)?)?;
        Ok(format!(
            "已把用量读取接到 Claude Code 的状态栏（{}）。{}\n下一次状态栏刷新后，宠物就知道 5 小时和每周额度用了多少。",
            file.display(),
            if had { "你原来的状态栏命令照常显示。" } else { "状态栏会显示模型、目录和用量。" }
        ))
    } else if unwrap(&mut root) {
        super::install::write(&file, &root)?;
        Ok(format!("已从 {} 移除，状态栏恢复原样。", file.display()))
    } else {
        Ok(format!("{} 里没有睿洱的状态栏命令。", file.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrapping_keeps_the_users_status_line_and_unwrapping_restores_it() {
        let exe = Path::new("C:\\Users\\me\\.rillbuddy\\bin\\rillbuddy-emit.exe");
        let me = "\"C:/Users/me/.rillbuddy/bin/rillbuddy-emit.exe\" --statusline";

        let fresh = wrap(json!({ "model": "x" }), exe).unwrap();
        assert_eq!(fresh["statusLine"], json!({ "type": "command", "command": me }));
        assert_eq!(wrap(fresh.clone(), exe).unwrap(), fresh, "installing twice changes nothing");
        let mut back = fresh.clone();
        assert!(unwrap(&mut back));
        assert_eq!(back, json!({ "model": "x" }));

        // Quotes and a `--` of their own come back exactly as they were.
        let theirs = r#"bash -c "~/.claude/line.sh --short -- 'a b'""#;
        let user = json!({ "statusLine": { "type": "command", "command": theirs, "padding": 0 } });
        let wrapped = wrap(user.clone(), exe).unwrap();
        assert_eq!(wrapped["statusLine"]["command"], format!("{me} --their {}", hex(theirs)));
        assert_eq!(theirs_in(wrapped["statusLine"]["command"].as_str().unwrap()).as_deref(), Some(theirs));
        assert_eq!(wrapped["statusLine"]["padding"], 0);
        assert_eq!(wrap(wrapped.clone(), exe).unwrap(), wrapped, "not wrapped twice");
        let mut back = wrapped;
        assert!(unwrap(&mut back));
        assert_eq!(back, user);

        // One written before the rename is ours too: pointed at the new emitter, their command kept.
        let old = format!("\"C:/Users/me/.familiar/bin/familiar-emit.exe\" --statusline --their {}", hex(theirs));
        let old = json!({ "statusLine": { "type": "command", "command": old } });
        assert_eq!(wrap(old, exe).unwrap()["statusLine"]["command"], format!("{me} --their {}", hex(theirs)));

        assert!(!unwrap(&mut user.clone()), "someone else's status line is not ours to remove");
        assert!(wrap(json!({ "statusLine": { "type": "other" } }), exe).is_err());
    }

    #[test]
    fn our_own_line() {
        let input = json!({
            "model": { "display_name": "Opus" }, "workspace": { "current_dir": "D:\\work\\demo-project\\" },
            "rate_limits": { "five_hour": { "used_percentage": 23.5, "resets_at": 1 }, "seven_day": { "used_percentage": 41.2, "resets_at": 2 } }
        });
        assert_eq!(ours(&input), "Opus · demo-project · 5h 24% · 7d 41%");
        assert_eq!(ours(&json!({ "model": { "display_name": "Opus" } })), "Opus");
        assert_eq!(ours(&Value::Null), "");
    }
}
