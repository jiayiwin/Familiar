//! Hook payload -> unified event (protocol v1).
//!
//! Only the tool name and a file basename survive into `detail`; prompts,
//! commands, file contents and outputs are dropped here and never leave. The
//! one exception: at the end of a turn, the single line where the agent names
//! its "next step" (下一步：… / Next step: …), which the pet may keep for you
//! (手上的事). Nothing else of its reply leaves.

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
            let said = input.get("last_assistant_message").and_then(Value::as_str).map(String::from).or_else(|| {
                input.get("transcript_path").and_then(Value::as_str).filter(|p| !p.is_empty()).and_then(|p| last_reply(std::path::Path::new(p)))
            });
            if let Some(n) = said.as_deref().and_then(next_step) {
                detail.insert("next".into(), n.into());
            }
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

/// Table-driven mapping (adapters/<agent>/mapping.yaml). Used for Codex,
/// Gemini CLI and Cursor so hook names and payload fields can change without
/// touching code.
///
/// `session` and `project` name where to find the value in the payload:
/// alternatives separated by `|` (the first that holds text wins), each a
/// dotted path where a number picks an array item (`workspace_roots.0`).
#[derive(Debug, serde::Deserialize)]
pub struct Table {
    pub source: String,
    #[serde(default = "d_session")]
    pub session: String,
    #[serde(default = "d_project")]
    pub project: String,
    /// Printed on stdout after the event is sent, for agents that insist on
    /// JSON from every hook (Gemini CLI: `{}`).
    #[serde(default)]
    pub stdout: Option<String>,
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
    /// The rule only applies when these payload fields hold these values.
    #[serde(default)]
    pub when: std::collections::HashMap<String, String>,
    /// `ok` is this, whatever the payload says (a hook that only fires on failure).
    #[serde(default)]
    pub ok_value: Option<bool>,
    /// A boolean payload field that means the user interrupted the tool.
    #[serde(default)]
    pub interrupt_key: Option<String>,
    /// A payload field whose value picks the turn result through `results`
    /// (`result` is the fallback).
    #[serde(default)]
    pub result_key: Option<String>,
    #[serde(default)]
    pub results: std::collections::HashMap<String, String>,
    /// Look in `last_assistant_message` for the agent's "next step" line.
    #[serde(default)]
    pub next: bool,
}

fn d_session() -> String {
    "session_id".into()
}
fn d_project() -> String {
    "cwd".into()
}

pub const CODEX_TABLE: &str = include_str!("../../../adapters/codex/mapping.yaml");
pub const GEMINI_TABLE: &str = include_str!("../../../adapters/gemini/mapping.yaml");
pub const CURSOR_TABLE: &str = include_str!("../../../adapters/cursor/mapping.yaml");
pub const OPENCODE_TABLE: &str = include_str!("../../../adapters/opencode/mapping.yaml");

/// Text at `spec` in the payload (see `Table`).
fn lookup<'a>(input: &'a Value, spec: &str) -> &'a str {
    spec.split('|')
        .filter_map(|path| {
            path.trim().split('.').try_fold(input, |v, key| match key.parse::<usize>() {
                Ok(i) if v.is_array() => v.get(i),
                _ => v.get(key),
            })
        })
        .filter_map(Value::as_str)
        .find(|s| !s.is_empty())
        .unwrap_or("")
}

pub fn table(input: &Value, t: &Table, pid: u32, ts: u64) -> Option<Value> {
    let hook = input.get("hook_event_name")?.as_str()?;
    let rule = t.events.get(hook)?;
    if !rule.when.iter().all(|(k, want)| input.get(k).and_then(Value::as_str) == Some(want.as_str())) {
        return None;
    }
    let session = lookup(input, &t.session);
    if session.is_empty() {
        return None;
    }
    let sub = rule.sub
        || t.sub_when.as_ref().is_some_and(|k| input.get(k).and_then(Value::as_str).is_some_and(|s| !s.is_empty()));
    let mut detail = Map::new();
    if rule.tool {
        put_tool_table(input, t, &mut detail);
    }
    if let Some(ok) = rule.ok_value {
        detail.insert("ok".into(), Value::Bool(ok));
    } else if rule.ok {
        detail.insert("ok".into(), Value::Bool(!response_failed(input.get("tool_response"))));
    }
    if rule.interrupt_key.as_ref().is_some_and(|k| input.get(k).and_then(Value::as_bool) == Some(true)) {
        detail.insert("interrupt".into(), Value::Bool(true));
    }
    if let Some(k) = &rule.kind {
        detail.insert("kind".into(), k.as_str().into());
    }
    let picked = rule.result_key.as_ref().and_then(|k| input.get(k)).and_then(Value::as_str).and_then(|v| rule.results.get(v));
    if let Some(r) = picked.or(rule.result.as_ref()) {
        detail.insert("result".into(), r.as_str().into());
    }
    if rule.next {
        if let Some(n) = input.get("last_assistant_message").and_then(Value::as_str).and_then(next_step) {
            detail.insert("next".into(), n.into());
        }
    }
    let mut ev = json!({
        "v": 1,
        "source": t.source,
        "session": session,
        "project": lookup(input, &t.project),
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

/// The line where a reply names its next step: 「下一步：补上超时的测试」,
/// "**Next steps:**" followed by a list item, "后续：…". Only a step stated as a
/// thing to do: an offer ("要我继续吗？", "Want me to…?") is not one. The last
/// such line wins; at most 80 characters of it.
pub fn next_step(text: &str) -> Option<String> {
    const HEADS: [&str; 7] = ["下一步", "后续", "接下来", "next steps", "next step", "follow-ups", "follow-up"];
    let clean = |s: &str| -> String {
        let s = s.replace("**", "").replace('`', "");
        let s = s.trim().trim_start_matches(['#', '*', '-', '>', '•', ' ', '\t']);
        // A numbered item: "1. ", "2) ".
        let s = match s.split_once(['.', ')', '、']) {
            Some((n, rest)) if !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()) => rest,
            _ => s,
        };
        s.trim().to_string()
    };
    let lines: Vec<&str> = text.lines().collect();
    let mut found = None;
    for (i, raw) in lines.iter().enumerate() {
        let line = clean(raw);
        let low = line.to_lowercase();
        let Some(head) = HEADS.iter().find(|h| low.starts_with(*h)) else { continue };
        // The heading is followed by a colon close by ("下一步建议：", "Next steps:").
        let after: String = line.chars().skip(head.chars().count()).collect();
        let Some(colon) = after.char_indices().find(|(_, c)| *c == ':' || *c == '：').map(|(i, _)| i) else { continue };
        if after[..colon].chars().count() > 4 {
            continue;
        }
        let mut what = after[colon..].trim_start_matches([':', '：']).trim().to_string();
        if what.is_empty() {
            what = lines[i + 1..].iter().map(|l| clean(l)).find(|l| !l.is_empty()).unwrap_or_default();
        }
        let wl = what.to_lowercase();
        let offer = what.ends_with(['?', '？'])
            || ["要我", "需要我", "是否", "要不要", "want me", "should i", "shall i", "let me know"].iter().any(|w| wl.contains(w));
        if what.is_empty() || offer {
            continue;
        }
        let what = what.trim_end_matches(['。', '.', '；', ';']).to_string();
        let mut short: String = what.chars().take(80).collect();
        if what.chars().count() > 80 {
            short.push('…');
        }
        found = Some(short);
    }
    found
}

/// The text of the last assistant reply in a Claude Code transcript (its tail only).
fn last_reply(path: &std::path::Path) -> Option<String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    let from = len.saturating_sub(512 * 1024);
    f.seek(SeekFrom::Start(from)).ok()?;
    let mut buf = Vec::new();
    f.take(512 * 1024).read_to_end(&mut buf).ok()?;
    let text = String::from_utf8_lossy(&buf);
    for line in text.lines().rev() {
        let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
        if v.get("type").and_then(Value::as_str) != Some("assistant") {
            continue;
        }
        let said: Vec<&str> = v
            .pointer("/message/content")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter(|c| c.get("type").and_then(Value::as_str) == Some("text")).filter_map(|c| c.get("text").and_then(Value::as_str)).collect())
            .unwrap_or_default();
        if !said.is_empty() {
            return Some(said.join("\n"));
        }
    }
    None
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
        // Gemini CLI: a failed tool's response carries `error`.
        || r.get("error").is_some_and(|e| !e.is_null())
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

    #[test]
    fn gemini_cli_payloads() {
        let t: Table = serde_yaml::from_str(GEMINI_TABLE).unwrap();
        assert_eq!(t.stdout.as_deref(), Some("{}"), "Gemini CLI wants JSON back from every hook");
        let g = |v: Value| table(&v, &t, 7, 1);
        let base = json!({ "session_id": "g1", "cwd": "D:/work/app", "transcript_path": "", "timestamp": "2026-11-10T00:00:00Z" });
        let with = |extra: Value| {
            let mut b = base.clone();
            b.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
            b
        };
        let start = g(with(json!({ "hook_event_name": "BeforeAgent", "prompt": "secret prompt" }))).unwrap();
        assert_eq!((start["source"].as_str(), start["event"].as_str(), start["project"].as_str()), (Some("gemini"), Some("turn.start"), Some("D:/work/app")));
        assert!(!start.to_string().contains("secret"));
        let tool = g(with(json!({ "hook_event_name": "BeforeTool", "tool_name": "replace", "tool_input": { "file_path": "/w/src/main.rs", "old_string": "a" } }))).unwrap();
        assert_eq!(tool["detail"], json!({ "tool": "replace", "file": "main.rs" }));
        let sh = g(with(json!({ "hook_event_name": "BeforeTool", "tool_name": "run_shell_command", "tool_input": { "command": "rg TODO src" } }))).unwrap();
        assert_eq!(sh["detail"]["read"], true);
        let ok = g(with(json!({ "hook_event_name": "AfterTool", "tool_name": "read_file", "tool_response": { "llmContent": "x", "returnDisplay": "x" } }))).unwrap();
        assert_eq!(ok["detail"]["ok"], true);
        let bad = g(with(json!({ "hook_event_name": "AfterTool", "tool_name": "run_shell_command", "tool_response": { "llmContent": "", "error": { "type": "x", "message": "boom" } } }))).unwrap();
        assert_eq!(bad["detail"]["ok"], false);
        let ask = g(with(json!({ "hook_event_name": "Notification", "notification_type": "ToolPermission", "message": "Tool x requires execution", "details": { "command": "rm -rf /" } }))).unwrap();
        assert_eq!((ask["event"].as_str(), ask["detail"]["kind"].as_str()), (Some("attention"), Some("permission")));
        assert!(!ask.to_string().contains("rm -rf"));
        assert!(g(with(json!({ "hook_event_name": "Notification", "notification_type": "SomethingElse" }))).is_none());
        assert_eq!(g(with(json!({ "hook_event_name": "AfterAgent", "prompt_response": "done" }))).unwrap()["detail"]["result"], "ok");
        assert!(g(with(json!({ "hook_event_name": "AfterModel" }))).is_none(), "fires for every chunk: not hooked");
    }

    #[test]
    fn opencode_plugin_payloads() {
        let t: Table = serde_yaml::from_str(OPENCODE_TABLE).unwrap();
        let o = |v: Value| table(&v, &t, 7, 1);
        let start = o(json!({ "hook_event_name": "TurnStart", "session_id": "ses_1", "cwd": "D:/work/api" })).unwrap();
        assert_eq!((start["source"].as_str(), start["event"].as_str(), start["sub"].as_bool()), (Some("opencode"), Some("turn.start"), Some(false)));
        let tool = o(json!({ "hook_event_name": "ToolStart", "session_id": "ses_1", "cwd": "D:/work/api", "tool_name": "edit", "tool_input": { "file_path": "D:/work/api/src/app.ts" } })).unwrap();
        assert_eq!(tool["detail"], json!({ "tool": "edit", "file": "app.ts" }));
        let sh = o(json!({ "hook_event_name": "ToolStart", "session_id": "ses_1", "tool_name": "shell", "tool_input": { "command": "rg" } })).unwrap();
        assert_eq!(sh["detail"]["read"], true);
        let bad = o(json!({ "hook_event_name": "ToolEnd", "session_id": "ses_1", "tool_name": "bash", "tool_response": { "success": false } })).unwrap();
        assert_eq!(bad["detail"]["ok"], false);
        // A subagent's session reports on its parent.
        let child = o(json!({ "hook_event_name": "ToolStart", "session_id": "ses_1", "agent_id": "ses_2", "tool_name": "read" })).unwrap();
        assert_eq!((child["session"].as_str(), child["sub"].as_bool()), (Some("ses_1"), Some(true)));
        assert_eq!(o(json!({ "hook_event_name": "Permission", "session_id": "ses_1" })).unwrap()["detail"]["kind"], "permission");
        for (hook, result) in [("TurnDone", "ok"), ("TurnFailed", "error"), ("TurnAborted", "aborted")] {
            assert_eq!(o(json!({ "hook_event_name": hook, "session_id": "ses_1" })).unwrap()["detail"]["result"], result);
        }
    }

    #[test]
    fn cursor_payloads() {
        let t: Table = serde_yaml::from_str(CURSOR_TABLE).unwrap();
        assert!(t.stdout.is_none());
        let c = |v: Value| table(&v, &t, 7, 1);
        let base = json!({ "conversation_id": "c1", "generation_id": "g", "workspace_roots": ["D:/work/site"], "cursor_version": "2.0" });
        let with = |extra: Value| {
            let mut b = base.clone();
            b.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
            b
        };
        let start = c(with(json!({ "hook_event_name": "beforeSubmitPrompt", "prompt": "secret" }))).unwrap();
        assert_eq!((start["source"].as_str(), start["session"].as_str(), start["project"].as_str(), start["event"].as_str()), (Some("cursor"), Some("c1"), Some("D:/work/site"), Some("turn.start")));
        let tool = c(with(json!({ "hook_event_name": "preToolUse", "tool_name": "Write", "tool_input": { "file_path": "D:\\work\\site\\index.html" }, "cwd": "D:/work/site" }))).unwrap();
        assert_eq!(tool["detail"], json!({ "tool": "Write", "file": "index.html" }));
        assert_eq!(c(with(json!({ "hook_event_name": "postToolUse", "tool_name": "Shell", "tool_output": "{}" }))).unwrap()["detail"]["ok"], true);
        let fail = c(with(json!({ "hook_event_name": "postToolUseFailure", "tool_name": "Shell", "failure_type": "error", "is_interrupt": false }))).unwrap();
        assert_eq!(fail["detail"], json!({ "tool": "Shell", "ok": false }));
        let cut = c(with(json!({ "hook_event_name": "postToolUseFailure", "tool_name": "Shell", "is_interrupt": true }))).unwrap();
        assert_eq!(cut["detail"]["interrupt"], true);
        for (status, result) in [("completed", "ok"), ("aborted", "aborted"), ("error", "error"), ("something-new", "ok")] {
            assert_eq!(c(with(json!({ "hook_event_name": "stop", "status": status }))).unwrap()["detail"]["result"], result, "{status}");
        }
        // No workspace open: the tool's own cwd names the project.
        let lone = c(json!({ "hook_event_name": "preToolUse", "session_id": "s9", "workspace_roots": [], "cwd": "D:/tmp", "tool_name": "Read" })).unwrap();
        assert_eq!((lone["session"].as_str(), lone["project"].as_str()), (Some("s9"), Some("D:/tmp")));
        assert!(c(with(json!({ "hook_event_name": "afterAgentThought" }))).is_none());
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

        // The next step it names is kept, and nothing else of the reply.
        let named = codex(json!({ "session_id": "01a1", "hook_event_name": "Stop", "last_assistant_message": "改好了，测试都过了。\n\n下一步：给登录补上超时的测试。\n要我现在做吗？" })).unwrap();
        assert_eq!(named["detail"]["next"], "给登录补上超时的测试");
        assert!(!named.to_string().contains("改好了"));
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


#[cfg(test)]
mod next_tests {
    use super::next_step;

    #[test]
    fn the_next_step_is_the_line_that_names_one() {
        assert_eq!(next_step("做完了。\n\n**下一步：** 把 `api-server` 的 PR 合进去。").as_deref(), Some("把 api-server 的 PR 合进去"));
        assert_eq!(next_step("Done.\n\n## Next steps:\n1. Add a timeout test for login\n2. Update the docs").as_deref(), Some("Add a timeout test for login"));
        assert_eq!(next_step("后续建议：清理旧的迁移脚本").as_deref(), Some("清理旧的迁移脚本"));
        // Offers and narration are not things to do.
        assert_eq!(next_step("下一步：要我继续把文档也改了吗？"), None);
        assert_eq!(next_step("Next step: want me to run the full suite?"), None);
        assert_eq!(next_step("下一步我会先看日志，再决定怎么修。"), None);
        assert_eq!(next_step("全部改完了，没有别的。"), None);
    }
}
