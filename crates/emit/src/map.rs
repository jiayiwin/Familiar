//! Hook payload -> unified event (protocol v1).
//!
//! Only the tool name and a file basename survive into `detail`; prompts,
//! commands, file contents and outputs are dropped here and never leave.

use serde_json::{json, Map, Value};

/// Build the unified event for a Claude Code hook payload.
/// Returns `None` for hooks that should not produce an event.
pub fn claude_code(input: &Value, pid: u32, ts: u64) -> Option<Value> {
    let hook = input.get("hook_event_name")?.as_str()?;
    let session = input.get("session_id").and_then(Value::as_str).unwrap_or("");
    if session.is_empty() {
        return None;
    }
    // Hooks fired inside a subagent carry its agent_id.
    let mut sub = input.get("agent_id").and_then(Value::as_str).is_some_and(|s| !s.is_empty());
    let mut detail = Map::new();

    let event = match hook {
        "SessionStart" => "session.start",
        "SessionEnd" => "session.end",
        "UserPromptSubmit" => "turn.start",
        "PreToolUse" => {
            put_tool(input, &mut detail);
            "tool.start"
        }
        "PostToolUse" => {
            put_tool(input, &mut detail);
            detail.insert("ok".into(), Value::Bool(!response_failed(input.get("tool_response"))));
            "tool.end"
        }
        "PostToolUseFailure" => {
            put_tool(input, &mut detail);
            detail.insert("ok".into(), Value::Bool(false));
            if input.get("is_interrupt").and_then(Value::as_bool) == Some(true) {
                detail.insert("interrupt".into(), Value::Bool(true));
            }
            "tool.end"
        }
        "PermissionRequest" => {
            put_tool(input, &mut detail);
            detail.insert("kind".into(), "permission".into());
            "attention"
        }
        "Notification" => {
            let kind = match input.get("notification_type").and_then(Value::as_str) {
                Some("permission_prompt") => "permission",
                Some("elicitation_dialog") => "input",
                // idle_prompt fires ~60 s after a finished turn; the pet already
                // shows `done`, so it is not treated as "the agent needs you".
                Some(_) => return None,
                // Older versions send no type: fall back on the message text.
                None => {
                    let msg = input.get("message").and_then(Value::as_str).unwrap_or("");
                    if msg.contains("waiting for your input") {
                        return None;
                    }
                    "permission"
                }
            };
            detail.insert("kind".into(), kind.into());
            "attention"
        }
        "Stop" => {
            detail.insert("result".into(), "ok".into());
            "turn.done"
        }
        "StopFailure" => {
            detail.insert("result".into(), "error".into());
            "turn.done"
        }
        "SubagentStop" => {
            sub = true;
            detail.insert("result".into(), "ok".into());
            "turn.done"
        }
        _ => return None,
    };

    let mut ev = json!({
        "v": 1,
        "source": "claude-code",
        "session": session,
        "project": input.get("cwd").and_then(Value::as_str).unwrap_or(""),
        "event": event,
        "sub": sub,
        "pid": pid,
        "ts": ts,
    });
    if !detail.is_empty() {
        ev["detail"] = Value::Object(detail);
    }
    Some(ev)
}

/// Table-driven mapping (adapters/<agent>/mapping.yaml). Used for Codex so
/// hook names and payload fields can change without touching code.
#[derive(Debug, serde::Deserialize)]
pub struct Table {
    pub source: String,
    #[serde(default = "d_session")]
    pub session: String,
    #[serde(default = "d_project")]
    pub project: String,
    #[serde(default)]
    pub sub_when: Option<String>,
    #[serde(default)]
    pub file_keys: Vec<String>,
    #[serde(default)]
    pub patch_tools: Vec<String>,
    #[serde(default)]
    pub read_commands: Vec<String>,
    pub events: std::collections::HashMap<String, Rule>,
}

#[derive(Debug, serde::Deserialize)]
pub struct Rule {
    pub event: String,
    /// Put tool name / file basename / read hint into detail.
    #[serde(default)]
    pub tool: bool,
    /// Work out `ok` from tool_response.
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub result: Option<String>,
    #[serde(default)]
    pub sub: bool,
}

fn d_session() -> String {
    "session_id".into()
}
fn d_project() -> String {
    "cwd".into()
}

pub const CODEX_TABLE: &str = include_str!("../../../adapters/codex/mapping.yaml");

pub fn table(input: &Value, t: &Table, pid: u32, ts: u64) -> Option<Value> {
    let hook = input.get("hook_event_name")?.as_str()?;
    let rule = t.events.get(hook)?;
    let session = input.get(&t.session).and_then(Value::as_str).unwrap_or("");
    if session.is_empty() {
        return None;
    }
    let sub = rule.sub
        || t.sub_when.as_ref().is_some_and(|k| input.get(k).and_then(Value::as_str).is_some_and(|s| !s.is_empty()));
    let mut detail = Map::new();
    if rule.tool {
        put_tool_table(input, t, &mut detail);
    }
    if rule.ok {
        detail.insert("ok".into(), Value::Bool(!response_failed(input.get("tool_response"))));
    }
    if let Some(k) = &rule.kind {
        detail.insert("kind".into(), k.as_str().into());
    }
    if let Some(r) = &rule.result {
        detail.insert("result".into(), r.as_str().into());
    }
    let mut ev = json!({
        "v": 1,
        "source": t.source,
        "session": session,
        "project": input.get(&t.project).and_then(Value::as_str).unwrap_or(""),
        "event": rule.event,
        "sub": sub,
        "pid": pid,
        "ts": ts,
    });
    if !detail.is_empty() {
        ev["detail"] = Value::Object(detail);
    }
    Some(ev)
}

fn put_tool_table(input: &Value, t: &Table, detail: &mut Map<String, Value>) {
    let tool = input.get("tool_name").and_then(Value::as_str).unwrap_or("");
    if !tool.is_empty() {
        detail.insert("tool".into(), tool.into());
    }
    let ti = input.get("tool_input");
    let mut file = t
        .file_keys
        .iter()
        .find_map(|k| ti.and_then(|v| v.get(k)).and_then(Value::as_str))
        .map(basename)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    if file.is_none() && t.patch_tools.iter().any(|p| p == tool) {
        file = ti.and_then(patch_file);
    }
    if let Some(f) = file {
        detail.insert("file".into(), f.into());
    }
    if let Some(cmd) = ti.and_then(|v| v.get("command")) {
        if let Some(word) = first_word(cmd) {
            if t.read_commands.iter().any(|r| r.eq_ignore_ascii_case(&word)) {
                detail.insert("read".into(), Value::Bool(true));
            }
        }
    }
}

fn basename(p: &str) -> &str {
    p.rsplit(['/', '\\']).find(|s| !s.is_empty()).unwrap_or("")
}

/// First file named in an apply_patch body (`*** Update File: path`).
fn patch_file(ti: &Value) -> Option<String> {
    let mut texts: Vec<&str> = Vec::new();
    match ti {
        Value::String(s) => texts.push(s),
        Value::Object(m) => texts.extend(m.values().filter_map(Value::as_str)),
        _ => {}
    }
    for text in texts {
        for line in text.lines() {
            for tag in ["*** Update File:", "*** Add File:", "*** Delete File:"] {
                if let Some(rest) = line.trim().strip_prefix(tag) {
                    let b = basename(rest.trim());
                    if !b.is_empty() {
                        return Some(b.to_string());
                    }
                }
            }
        }
    }
    None
}

/// The program a shell command starts with, lower-cased, without `.exe`.
/// `git log …` becomes `git-log`. Only this word is inspected; the command
/// itself is never forwarded.
fn first_word(cmd: &Value) -> Option<String> {
    let line = match cmd {
        Value::String(s) => s.clone(),
        Value::Array(a) => a.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(" "),
        _ => return None,
    };
    let mut words = line.split_whitespace().map(|w| w.trim_matches(|c| c == '"' || c == '\'' || c == '&'));
    let mut w = words.next()?;
    if w.is_empty() {
        w = words.next()?;
    }
    let w = basename(w).to_ascii_lowercase();
    let w = w.strip_suffix(".exe").unwrap_or(&w).to_string();
    if w == "git" {
        return Some(format!("git-{}", words.next().unwrap_or("").to_ascii_lowercase()));
    }
    Some(w)
}

fn put_tool(input: &Value, detail: &mut Map<String, Value>) {
    if let Some(t) = input.get("tool_name").and_then(Value::as_str) {
        detail.insert("tool".into(), t.into());
    }
    let ti = input.get("tool_input");
    let path = ["file_path", "notebook_path", "path"]
        .iter()
        .find_map(|k| ti.and_then(|t| t.get(*k)).and_then(Value::as_str));
    if let Some(p) = path {
        let base = p.rsplit(['/', '\\']).find(|s| !s.is_empty()).unwrap_or("");
        if !base.is_empty() {
            detail.insert("file".into(), base.into());
        }
    }
}

fn response_failed(resp: Option<&Value>) -> bool {
    let Some(r) = resp else { return false };
    r.get("is_error").and_then(Value::as_bool) == Some(true)
        || r.get("success").and_then(Value::as_bool) == Some(false)
        || r.get("interrupted").and_then(Value::as_bool) == Some(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(v: Value) -> Option<Value> {
        claude_code(&v, 42, 1000)
    }

    #[test]
    fn pre_tool_use_keeps_only_tool_and_basename() {
        let ev = map(json!({
            "hook_event_name": "PreToolUse", "session_id": "s1", "cwd": "/work/f1",
            "tool_name": "Edit",
            "tool_input": { "file_path": "C:\\src\\app\\main.go", "old_string": "secret", "new_string": "x" }
        }))
        .unwrap();
        assert_eq!(ev["event"], "tool.start");
        assert_eq!(ev["detail"], json!({ "tool": "Edit", "file": "main.go" }));
        assert!(!ev.to_string().contains("secret"));
        assert_eq!(ev["project"], "/work/f1");
        assert_eq!(ev["pid"], 42);
    }

    #[test]
    fn prompt_text_never_leaves() {
        let ev = map(json!({ "hook_event_name": "UserPromptSubmit", "session_id": "s", "prompt": "my secret plan" })).unwrap();
        assert_eq!(ev["event"], "turn.start");
        assert!(!ev.to_string().contains("secret"));
    }

    #[test]
    fn notification_types() {
        let perm = map(json!({ "hook_event_name": "Notification", "session_id": "s", "notification_type": "permission_prompt" })).unwrap();
        assert_eq!(perm["detail"]["kind"], "permission");
        assert!(map(json!({ "hook_event_name": "Notification", "session_id": "s", "notification_type": "idle_prompt" })).is_none());
        assert!(map(json!({ "hook_event_name": "Notification", "session_id": "s", "message": "Claude is waiting for your input" })).is_none());
    }

    #[test]
    fn failures_and_subagents() {
        let f = map(json!({ "hook_event_name": "PostToolUseFailure", "session_id": "s", "tool_name": "Bash", "is_interrupt": true })).unwrap();
        assert_eq!(f["detail"]["ok"], false);
        assert_eq!(f["detail"]["interrupt"], true);
        let s = map(json!({ "hook_event_name": "SubagentStop", "session_id": "s" })).unwrap();
        assert_eq!(s["sub"], true);
        let t = map(json!({ "hook_event_name": "PreToolUse", "session_id": "s", "agent_id": "a1", "tool_name": "Read" })).unwrap();
        assert_eq!(t["sub"], true);
    }

    fn codex(v: Value) -> Option<Value> {
        let t: Table = serde_yaml::from_str(CODEX_TABLE).unwrap();
        table(&v, &t, 7, 1)
    }

    #[test]
    fn codex_real_payloads() {
        // Captured from codex-cli 0.162.1 on Windows.
        let pre = codex(json!({
            "session_id": "01a1", "turn_id": "t1", "cwd": "C:\\w\\proj", "hook_event_name": "PreToolUse",
            "tool_name": "Bash", "tool_input": { "command": "Get-Content -Raw -LiteralPath secret.txt" }
        }))
        .unwrap();
        assert_eq!(pre["source"], "codex");
        assert_eq!(pre["event"], "tool.start");
        assert_eq!(pre["detail"], json!({ "tool": "Bash", "read": true }));
        assert!(!pre.to_string().contains("secret"));

        let post = codex(json!({
            "session_id": "01a1", "cwd": "C:\\w\\proj", "hook_event_name": "PostToolUse",
            "tool_name": "Bash", "tool_input": { "command": "npm test" }, "tool_response": "hi\r\n"
        }))
        .unwrap();
        assert_eq!(post["detail"], json!({ "tool": "Bash", "ok": true }));

        let stop = codex(json!({ "session_id": "01a1", "hook_event_name": "Stop", "last_assistant_message": "secret" })).unwrap();
        assert_eq!(stop["event"], "turn.done");
        assert_eq!(stop["detail"]["result"], "ok");
        assert!(!stop.to_string().contains("secret"));

        let int = codex(json!({ "session_id": "01a1", "hook_event_name": "Interrupt" })).unwrap();
        assert_eq!(int["detail"]["result"], "aborted");
        let perm = codex(json!({ "session_id": "01a1", "hook_event_name": "PermissionRequest", "tool_name": "Bash", "tool_input": { "command": "rm -rf x" } })).unwrap();
        assert_eq!(perm["event"], "attention");
        assert_eq!(perm["detail"]["kind"], "permission");
        let sub = codex(json!({ "session_id": "01a1", "hook_event_name": "SubagentStop", "agent_id": "a" })).unwrap();
        assert_eq!(sub["sub"], true);
        assert!(codex(json!({ "session_id": "01a1", "hook_event_name": "PreCompact" })).is_none());
    }

    #[test]
    fn codex_patch_file_and_git_reads() {
        let p = codex(json!({
            "session_id": "s", "hook_event_name": "PreToolUse", "tool_name": "apply_patch",
            "tool_input": { "input": "*** Begin Patch\n*** Update File: src/app/main.rs\n@@\n-secret\n+x\n*** End Patch" }
        }))
        .unwrap();
        assert_eq!(p["detail"], json!({ "tool": "apply_patch", "file": "main.rs" }));
        let g = codex(json!({ "session_id": "s", "hook_event_name": "PreToolUse", "tool_name": "Bash", "tool_input": { "command": "git status --short" } })).unwrap();
        assert_eq!(g["detail"]["read"], true);
        let c = codex(json!({ "session_id": "s", "hook_event_name": "PreToolUse", "tool_name": "Bash", "tool_input": { "command": "git commit -m x" } })).unwrap();
        assert!(c["detail"].get("read").is_none());
    }

    #[test]
    fn unknown_or_sessionless_is_dropped() {
        assert!(map(json!({ "hook_event_name": "PreCompact", "session_id": "s" })).is_none());
        assert!(map(json!({ "hook_event_name": "Stop" })).is_none());
    }
}
