//! familiar-emit: called once per agent hook.
//!
//! Hard rules: exit code is always 0, total time stays under ~300 ms, and the
//! only thing ever written to stdout in hook mode is a note petd wants the
//! agent to read (a whip), as the hook's `additionalContext`. If petd is
//! unreachable the event is spooled to disk and petd is started detached.
//!
//! Usage:
//!   familiar-emit --source claude-code     (hook mode, payload on stdin)
//!   familiar-emit --source codex           (hook mode, Codex payload on stdin)
//!   familiar-emit --cmd show|hide|stats    (used by the /pet command; prints a reply)
//!   familiar-emit --install codex          (add hooks to $CODEX_HOME/hooks.json)
//!   familiar-emit --uninstall codex

mod install;
mod map;
mod proc;

use std::io::{IsTerminal, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const DEFAULT_PORT: u16 = 47321;
const HOOK_BUDGET: Duration = Duration::from_millis(300);

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let _ = std::panic::catch_unwind(|| run(&args));
    std::process::exit(0);
}

fn run(args: &[String]) {
    let start = Instant::now();
    if let Some(i) = args.iter().position(|a| a == "--cmd") {
        return command_mode(&args[i + 1..]);
    }
    if let Some(target) = flag(args, "--install") {
        return install::run(target, true, args);
    }
    if let Some(target) = flag(args, "--uninstall") {
        return install::run(target, false, args);
    }
    let source = flag(args, "--source").unwrap_or("claude-code");

    let mut raw = Vec::new();
    let stdin = std::io::stdin();
    if !stdin.is_terminal() {
        let _ = stdin.lock().take(4 * 1024 * 1024).read_to_end(&mut raw);
    }
    // FAMILIAR_DEBUG=1 traces to stderr (never stdout) for troubleshooting hooks.
    let debug = std::env::var_os("FAMILIAR_DEBUG").is_some();
    let trace = |m: &dyn std::fmt::Display| {
        if debug {
            eprintln!("familiar-emit: {m}");
        }
    };
    let input = match serde_json::from_slice::<serde_json::Value>(&raw) {
        Ok(v) => v,
        Err(e) => return trace(&format!("stdin is not JSON ({} bytes): {e}", raw.len())),
    };

    let ev = match source {
        "claude-code" => map::claude_code(&input, proc::agent_pid(), now_ms()),
        other => match load_table(other) {
            Ok(t) => map::table(&input, &t, proc::agent_pid(), now_ms()),
            Err(e) => return trace(&e),
        },
    };
    let Some(ev) = ev else {
        return trace(&format!("no event for hook {:?}", input.get("hook_event_name")));
    };
    let body = ev.to_string();
    trace(&body);

    let remaining = HOOK_BUDGET.saturating_sub(start.elapsed());
    match post("/v1/event", &body, remaining) {
        Ok((code @ 200..=299, reply)) => {
            trace(&"sent");
            if let Some(note) = (code == 200 && source == "claude-code").then(|| agent_note(&input, &reply)).flatten() {
                let mut so = std::io::stdout().lock();
                let _ = writeln!(so, "{note}");
                let _ = so.flush();
            }
        }
        Ok((code, _)) => {
            trace(&format!("petd answered {code}, spooled"));
            spool(&body)
        }
        Err(SendError::Timeout) => {
            trace(&"timeout, spooled");
            spool(&body)
        }
        Err(SendError::Unreachable) => {
            trace(&"petd unreachable, spooled and launching it");
            spool(&body);
            launch_petd();
        }
    }
}

/// petd answered with a note for the agent: the hook output that hands it over
/// as extra context, on the Claude Code hooks that accept it.
fn agent_note(input: &serde_json::Value, reply: &str) -> Option<String> {
    let hook = input.get("hook_event_name")?.as_str()?;
    if !matches!(hook, "UserPromptSubmit" | "PreToolUse" | "PostToolUse" | "PostToolUseFailure") {
        return None;
    }
    let text = serde_json::from_str::<serde_json::Value>(reply).ok()?.get("nudge")?.as_str()?.to_string();
    Some(serde_json::json!({ "hookSpecificOutput": { "hookEventName": hook, "additionalContext": text } }).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_note_rides_on_hooks_that_take_context() {
        let reply = r#"{"nudge":"[Familiar] faster"}"#;
        let out = agent_note(&json!({ "hook_event_name": "PostToolUse", "session_id": "s" }), reply).unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["hookSpecificOutput"]["hookEventName"], "PostToolUse");
        assert_eq!(v["hookSpecificOutput"]["additionalContext"], "[Familiar] faster");
        assert!(agent_note(&json!({ "hook_event_name": "Stop", "session_id": "s" }), reply).is_none(), "Stop cannot carry context");
        assert!(agent_note(&json!({ "hook_event_name": "PreToolUse" }), "").is_none(), "no body, nothing to say");
    }
}

fn command_mode(rest: &[String]) {
    let cmd = rest.first().map(String::as_str).unwrap_or("show");
    let body = serde_json::json!({ "cmd": cmd, "args": &rest[rest.len().min(1)..] }).to_string();
    match post("/v1/cmd", &body, Duration::from_secs(2)) {
        Ok((_, text)) => println!("{text}"),
        Err(SendError::Unreachable) => {
            let started = launch_petd();
            println!("{}", if started { "Familiar 正在启动，稍后再试一次。" } else { "找不到 Familiar 程序，请先安装或手动启动一次。" });
        }
        Err(SendError::Timeout) => println!("Familiar 没有响应。"),
    }
}

/// Mapping table for a table-driven agent: ~/.familiar/adapters/<name>.yaml
/// if present, else the copy built into the binary.
fn load_table(source: &str) -> Result<map::Table, String> {
    let custom = home().join("adapters").join(format!("{source}.yaml"));
    if let Ok(text) = std::fs::read_to_string(&custom) {
        return serde_yaml::from_str(&text).map_err(|e| format!("{}: {e}", custom.display()));
    }
    match source {
        "codex" => serde_yaml::from_str(map::CODEX_TABLE).map_err(|e| format!("built-in codex table: {e}")),
        _ => Err(format!("unknown source {source}")),
    }
}

pub(crate) fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(String::as_str)
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

pub(crate) fn home() -> PathBuf {
    if let Some(h) = std::env::var_os("FAMILIAR_HOME") {
        return PathBuf::from(h);
    }
    let base = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).unwrap_or_default();
    PathBuf::from(base).join(".familiar")
}

fn read_trim(name: &str) -> Option<String> {
    std::fs::read_to_string(home().join(name)).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

enum SendError {
    Unreachable,
    Timeout,
}

/// Minimal HTTP/1.1 POST to petd on loopback. No client library: keeps the
/// binary small and start-up near zero.
fn post(path: &str, body: &str, budget: Duration) -> Result<(u16, String), SendError> {
    let deadline = Instant::now() + budget;
    let port = read_trim("port").and_then(|p| p.parse().ok()).unwrap_or(DEFAULT_PORT);
    let token = read_trim("token").ok_or(SendError::Unreachable)?;
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let left = |d: Instant| d.saturating_duration_since(Instant::now()).max(Duration::from_millis(1));

    let mut s = TcpStream::connect_timeout(&addr, left(deadline).min(Duration::from_millis(150)))
        .map_err(|_| SendError::Unreachable)?;
    let _ = s.set_nodelay(true);
    s.set_write_timeout(Some(left(deadline))).map_err(|_| SendError::Timeout)?;
    let req = format!(
        "POST {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nX-Familiar-Token: {token}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    s.write_all(req.as_bytes()).map_err(|_| SendError::Timeout)?;
    s.set_read_timeout(Some(left(deadline))).map_err(|_| SendError::Timeout)?;
    let mut resp = Vec::new();
    s.read_to_end(&mut resp).map_err(|_| SendError::Timeout)?;
    let text = String::from_utf8_lossy(&resp);
    let status = text.split_whitespace().nth(1).and_then(|c| c.parse().ok()).unwrap_or(0);
    let body = text.split_once("\r\n\r\n").map(|(_, b)| b.to_string()).unwrap_or_default();
    Ok((status, body))
}

fn spool(body: &str) {
    let dir = home().join("spool");
    let _ = std::fs::create_dir_all(&dir);
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let name = format!("{:020}-{:08x}.json", nanos / 1_000_000, (nanos as u32) ^ std::process::id().rotate_left(13));
    let tmp = dir.join(format!("{name}.tmp"));
    if std::fs::write(&tmp, body).is_ok() {
        let _ = std::fs::rename(&tmp, dir.join(name));
    }
}

/// Find and start petd. Order: $FAMILIAR_PETD, ~/.familiar/petd_path
/// (written by petd on every start), then an `familiar` exe next to us.
fn launch_petd() -> bool {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(p) = std::env::var_os("FAMILIAR_PETD") {
        candidates.push(p.into());
    }
    if let Some(p) = read_trim("petd_path") {
        candidates.push(p.into());
    }
    if let Ok(me) = std::env::current_exe() {
        if let Some(dir) = me.parent() {
            candidates.push(dir.join(if cfg!(windows) { "familiar.exe" } else { "familiar" }));
        }
    }
    candidates.into_iter().filter(|p| p.is_file()).any(|p| proc::spawn_detached(&p))
}
