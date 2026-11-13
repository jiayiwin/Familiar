//! Unified event protocol v1. petd only speaks this; it never sees raw hooks.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    #[serde(default = "one")]
    pub v: u32,
    pub source: String,
    pub session: String,
    #[serde(default)]
    pub project: String,
    pub event: EventKind,
    #[serde(default)]
    pub sub: bool,
    #[serde(default)]
    pub detail: Detail,
    #[serde(default)]
    pub pid: u32,
    pub ts: u64,
}

fn one() -> u32 {
    1
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventKind {
    #[serde(rename = "session.start")]
    SessionStart,
    #[serde(rename = "session.end")]
    SessionEnd,
    #[serde(rename = "turn.start")]
    TurnStart,
    #[serde(rename = "tool.start")]
    ToolStart,
    #[serde(rename = "tool.end")]
    ToolEnd,
    #[serde(rename = "attention")]
    Attention,
    #[serde(rename = "turn.done")]
    TurnDone,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Detail {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ok: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interrupt: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<TurnResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ms: Option<u64>,
    /// Emitter judged the shell command a read/search (Codex runs those through Bash).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read: Option<bool>,
    /// The "next step" an agent named at the end of a turn (one line, nothing else of its output).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TurnResult {
    Ok,
    Error,
    Aborted,
}

/// Tools that put the pet in `reading` instead of `tool`.
pub fn is_read_tool(name: &str) -> bool {
    matches!(
        name,
        "Read" | "Grep" | "Glob" | "LS" | "WebFetch" | "WebSearch" | "NotebookRead"
            // Codex tool names (shell reads arrive as Bash + detail.read).
            | "read_file" | "list_dir" | "grep_files" | "web_search" | "view_image"
            // Gemini CLI.
            | "read_many_files" | "glob" | "grep_search" | "search_file_content" | "list_directory" | "web_fetch" | "google_web_search"
            // OpenCode.
            | "read" | "grep" | "list" | "webfetch" | "websearch" | "codesearch"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_emitter_output() {
        let e: Event = serde_json::from_str(
            r#"{"v":1,"source":"claude-code","session":"s","project":"/p","event":"tool.end","sub":false,
                "detail":{"tool":"Bash","ok":false},"pid":7,"ts":1}"#,
        )
        .unwrap();
        assert_eq!(e.event, EventKind::ToolEnd);
        assert_eq!(e.detail.ok, Some(false));
        let d: Event = serde_json::from_str(
            r#"{"source":"codex","session":"s","event":"turn.done","detail":{"result":"aborted"},"ts":1}"#,
        )
        .unwrap();
        assert_eq!(d.detail.result, Some(TurnResult::Aborted));
    }
}
