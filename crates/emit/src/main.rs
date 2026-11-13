//! rillbuddy-emit: called once per agent hook.
//!
//! Hard rules: exit code is always 0, total time stays under ~300 ms, and
//! nothing is written to stdout in hook mode except what the agent's hook
//! protocol asks for (a permission decision, a table's fixed answer). If petd
//! is unreachable the event is spooled to disk and petd is started detached.
//!
//! One hook is allowed to take long, and only when the user turned it on
//! (`features.approve`): a permission request then waits for the user's answer
//! on the pet's approval card, and what is asked (the command, file or address)
//! goes to petd for that card to show.
//!
//! Prompts, file contents, tool output and transcripts are never forwarded.
//!
//! Usage:
//!   rillbuddy-emit --source claude-code     (hook mode, payload on stdin)
//!   rillbuddy-emit --source codex           (hook mode, Codex payload on stdin)
//!   rillbuddy-emit --cmd show|hide|stats    (used by the /pet command; prints a reply)
//!   rillbuddy-emit --install codex          (add hooks to $CODEX_HOME/hooks.json)
//!   rillbuddy-emit --uninstall codex
//!   rillbuddy-emit --statusline [-- cmd]    (Claude Code status line: reports usage limits to petd)
//!   rillbuddy-emit --install statusline     (set that up in Claude Code's settings.json; --uninstall undoes it)

mod install;
mod map;
mod proc;
mod statusline;

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
    if args.iter().any(|a| a == "--statusline") {
        return statusline::run(args);
    }
    let source = flag(args, "--source").unwrap_or("claude-code");
    // A hook must never linger: whatever it is stuck on (a stdin that is never
    // closed, a socket), it is gone after this long.
    let lifetime = watchdog(Duration::from_secs(10));

    let mut raw = Vec::new();
    let stdin = std::io::stdin();
    if !stdin.is_terminal() {
        let _ = stdin.lock().take(4 * 1024 * 1024).read_to_end(&mut raw);
    }
    // RILLBUDDY_DEBUG=1 traces to stderr (never stdout) for troubleshooting hooks.
    let debug = std::env::var_os("RILLBUDDY_DEBUG").or_else(|| std::env::var_os("FAMILIAR_DEBUG")).is_some();
    let trace = |m: &dyn std::fmt::Display| {
        if debug {
            eprintln!("rillbuddy-emit: {m}");
        }
    };
    let input = match serde_json::from_slice::<serde_json::Value>(&raw) {
        Ok(v) => v,
        Err(e) => return trace(&format!("stdin is not JSON ({} bytes): {e}", raw.len())),
    };

    let mut after: Option<String> = None;
    let ev = match source {
        // Cursor also runs Claude Code's hooks, in Claude Code's format. Its
        // sessions are not Claude Code's: they come in through `--source cursor`.
        "claude-code" if input.get("cursor_version").is_some() => return trace(&"a Cursor session on the Claude Code hook: left to the Cursor adapter"),
        "claude-code" => map::claude_code(&input, proc::agent_pid(), now_ms()),
        other => match load_table(other) {
            Ok(t) => {
                after = t.stdout.clone();
                map::table(&input, &t, proc::agent_pid(), now_ms())
            }
            Err(e) => return trace(&e),
        },
    };
    // Some agents want JSON back from every hook, event or not.
    let _answer = Answer(after);
    let Some(ev) = ev else {
        return trace(&format!("no event for hook {:?}", input.get("hook_event_name")));
    };
    let body = ev.to_string();
    trace(&body);

    let remaining = HOOK_BUDGET.saturating_sub(start.elapsed());
    match post("/v1/event", &body, remaining) {
        Ok((code @ 200..=299, reply)) => {
            trace(&"sent");
            // The user answers permission requests on the pet's card: hand the
            // request over and wait for that answer (this one hook may take long).
            if let Some(decision) = (code == 200 && source == "claude-code").then(|| approval(&input, &reply, &ev, &lifetime)).flatten() {
                let mut so = std::io::stdout().lock();
                let _ = writeln!(so, "{decision}");
                let _ = so.flush();
                trace(&"permission answered on the card");
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

/// Ends the process when its time is up. Returns the deadline (ms since the
/// epoch), which the one long hook (waiting for the approval card) pushes back.
fn watchdog(first: Duration) -> std::sync::Arc<std::sync::atomic::AtomicU64> {
    use std::sync::atomic::{AtomicU64, Ordering};
    let until = std::sync::Arc::new(AtomicU64::new(now_ms() + first.as_millis() as u64));
    let seen = until.clone();
    std::thread::spawn(move || loop {
        let left = seen.load(Ordering::Relaxed).saturating_sub(now_ms());
        if left == 0 {
            std::process::exit(0);
        }
        std::thread::sleep(Duration::from_millis(left.min(1000)));
    });
    until
}

/// What a table-driven agent expects on stdout (`stdout:` in its table),
/// written when the hook is done, whichever way it ends.
struct Answer(Option<String>);

impl Drop for Answer {
    fn drop(&mut self) {
        if let Some(text) = &self.0 {
            let mut so = std::io::stdout().lock();
            let _ = writeln!(so, "{text}");
            let _ = so.flush();
        }
    }
}

/// petd said the user answers permission requests on the pet's card
/// (`approve`: how long it waits, ms). Sends what is being asked, waits for the
/// answer, and returns the hook output that carries an allow or a deny. No
/// answer in time (or "decide in the terminal") returns nothing, and Claude
/// Code asks in the terminal as it always does.
fn approval(input: &serde_json::Value, reply: &str, ev: &serde_json::Value, lifetime: &std::sync::atomic::AtomicU64) -> Option<String> {
    if input.get("hook_event_name")?.as_str()? != "PermissionRequest" {
        return None;
    }
    let wait = serde_json::from_str::<serde_json::Value>(reply).ok()?.get("approve")?.as_u64()?;
    lifetime.store(now_ms() + wait + 10_000, std::sync::atomic::Ordering::Relaxed);
    let (tool, detail, note) = asked(input);
    let body = serde_json::json!({ "source": ev["source"], "session": ev["session"], "tool": tool, "detail": detail, "note": note }).to_string();
    let (code, answer) = post("/v1/approve", &body, Duration::from_millis(wait + 3000)).ok()?;
    if code != 200 {
        return None;
    }
    decision(serde_json::from_str::<serde_json::Value>(&answer).ok()?.get("behavior")?.as_str()?)
}

/// The PermissionRequest hook output for an answer from the card.
fn decision(behavior: &str) -> Option<String> {
    let d = match behavior {
        "allow" => serde_json::json!({ "behavior": "allow" }),
        "deny" => serde_json::json!({ "behavior": "deny", "message": "The user denied this from the Rill pet's approval card." }),
        _ => return None,
    };
    Some(serde_json::json!({ "hookSpecificOutput": { "hookEventName": "PermissionRequest", "decision": d } }).to_string())
}

/// What a permission request is about, for the card: the tool, the command /
/// file / address it wants to touch, and the agent's own description of it.
fn asked(input: &serde_json::Value) -> (String, String, String) {
    let tool = input.get("tool_name").and_then(|t| t.as_str()).unwrap_or_default().to_string();
    let ti = input.get("tool_input");
    let field = |k: &str| ti.and_then(|t| t.get(k)).and_then(|v| v.as_str()).map(str::to_string);
    let detail = ["command", "file_path", "notebook_path", "url", "path", "pattern", "query", "prompt"]
        .iter()
        .find_map(|k| field(k))
        .or_else(|| ti.filter(|t| t.as_object().is_some_and(|o| !o.is_empty())).map(|t| t.to_string()))
        .unwrap_or_default();
    let cut = |s: String, n: usize| -> String { s.chars().take(n).collect() };
    (tool, cut(detail, 1200), cut(field("description").unwrap_or_default(), 200))
}

fn command_mode(rest: &[String]) {
    let cmd = rest.first().map(String::as_str).unwrap_or("show");
    let body = serde_json::json!({ "cmd": cmd, "args": &rest[rest.len().min(1)..] }).to_string();
    match post("/v1/cmd", &body, Duration::from_secs(2)) {
        Ok((_, text)) => println!("{text}"),
        Err(SendError::Unreachable) => {
            let started = launch_petd();
            println!("{}", if started { "睿洱正在启动，稍后再试一次。" } else { "找不到睿洱程序，请先安装或手动启动一次。" });
        }
        Err(SendError::Timeout) => println!("睿洱没有响应。"),
    }
}

/// Mapping table for a table-driven agent: ~/.rillbuddy/adapters/<name>.yaml
/// if present, else the copy built into the binary.
fn load_table(source: &str) -> Result<map::Table, String> {
    let custom = home().join("adapters").join(format!("{source}.yaml"));
    if let Ok(text) = std::fs::read_to_string(&custom) {
        return serde_yaml::from_str(&text).map_err(|e| format!("{}: {e}", custom.display()));
    }
    let built_in = match source {
        "codex" => map::CODEX_TABLE,
        "gemini" => map::GEMINI_TABLE,
        "cursor" => map::CURSOR_TABLE,
        "opencode" => map::OPENCODE_TABLE,
        _ => return Err(format!("unknown source {source}")),
    };
    serde_yaml::from_str(built_in).map_err(|e| format!("built-in {source} table: {e}"))
}

pub(crate) fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(String::as_str)
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

pub(crate) fn home() -> PathBuf {
    if let Some(h) = std::env::var_os("RILLBUDDY_HOME").or_else(|| std::env::var_os("FAMILIAR_HOME")) {
        return PathBuf::from(h);
    }
    let base = PathBuf::from(std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).unwrap_or_default());
    let home = base.join(".rillbuddy");
    // An app from before the rename keeps its data in ~/.familiar until the new one moves it.
    let old = base.join(".familiar");
    if !home.exists() && old.is_dir() {
        return old;
    }
    home
}

fn read_trim(name: &str) -> Option<String> {
    std::fs::read_to_string(home().join(name)).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

pub(crate) enum SendError {
    Unreachable,
    Timeout,
}

/// Minimal HTTP/1.1 POST to petd on loopback. No client library: keeps the
/// binary small and start-up near zero.
pub(crate) fn post(path: &str, body: &str, budget: Duration) -> Result<(u16, String), SendError> {
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
        "POST {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nX-RillBuddy-Token: {token}\r\nX-Familiar-Token: {token}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
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

/// Find and start petd. Order: $RILLBUDDY_PETD, ~/.rillbuddy/petd_path
/// (written by petd on every start), then the app's exe next to us.
fn launch_petd() -> bool {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(p) = std::env::var_os("RILLBUDDY_PETD") {
        candidates.push(p.into());
    }
    if let Some(p) = read_trim("petd_path") {
        candidates.push(p.into());
    }
    if let Ok(me) = std::env::current_exe() {
        if let Some(dir) = me.parent() {
            for name in ["rillbuddy", "familiar"] {
                candidates.push(dir.join(if cfg!(windows) { format!("{name}.exe") } else { name.to_string() }));
            }
        }
    }
    candidates.into_iter().filter(|p| p.is_file()).any(|p| proc::spawn_detached(&p))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_card_sees_what_is_asked_and_its_answer_becomes_a_decision() {
        let bash = json!({ "hook_event_name": "PermissionRequest", "tool_name": "Bash", "tool_input": { "command": "rm -rf node_modules", "description": "Remove node_modules" } });
        assert_eq!(asked(&bash), ("Bash".into(), "rm -rf node_modules".into(), "Remove node_modules".into()));
        let edit = json!({ "tool_name": "Edit", "tool_input": { "file_path": "src/a.rs", "old_string": "x", "new_string": "y" } });
        assert_eq!(asked(&edit).1, "src/a.rs");
        let odd = json!({ "tool_name": "mcp__x__y", "tool_input": { "n": 3 } });
        assert_eq!(asked(&odd).1, r#"{"n":3}"#);
        assert_eq!(asked(&json!({ "tool_name": "T" })), ("T".into(), String::new(), String::new()));

        let allow: serde_json::Value = serde_json::from_str(&decision("allow").unwrap()).unwrap();
        assert_eq!(allow["hookSpecificOutput"]["hookEventName"], "PermissionRequest");
        assert_eq!(allow["hookSpecificOutput"]["decision"]["behavior"], "allow");
        let deny: serde_json::Value = serde_json::from_str(&decision("deny").unwrap()).unwrap();
        assert_eq!(deny["hookSpecificOutput"]["decision"]["behavior"], "deny");
        assert!(deny["hookSpecificOutput"]["decision"]["message"].as_str().is_some());
        assert!(decision("terminal").is_none(), "the terminal asks");
        // Only a permission hook waits, and only when petd says the card is on.
        let life = std::sync::atomic::AtomicU64::new(0);
        assert!(approval(&json!({ "hook_event_name": "PreToolUse" }), r#"{"approve":1000}"#, &json!({}), &life).is_none());
        assert!(approval(&bash, "{}", &json!({}), &life).is_none());
    }

}
