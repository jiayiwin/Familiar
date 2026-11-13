// RillBuddy: forwards OpenCode session events to the local pet daemon.
// Installed by `rillbuddy-emit --install opencode`; remove it with
// `rillbuddy-emit --uninstall opencode` (or just delete this file).
//
// What leaves OpenCode through this file: the session id, the working
// directory, the tool's name, the path of the file a tool touches and the
// first word of a shell command. No prompt, no file content, no command text,
// no output. It goes to rillbuddy-emit on this machine and nowhere else.
//
// Written against the published plugin interfaces of both OpenCode
// generations (V1 calls `server()`, V2 calls `setup()`).

import { spawn } from "node:child_process"

const EMIT = "__RILLBUDDY_EMIT__"

/** Child session -> the session that started it (a subagent's work shows on its parent). */
const parents = new Map()
/** Sessions whose turn already ended in an error, so the idle that follows is not a second ending. */
const failed = new Set()

function send(hook, sessionID, cwd, extra) {
  if (!sessionID) return
  const parent = parents.get(sessionID)
  const body = { hook_event_name: hook, session_id: parent ?? sessionID, cwd: cwd || "", ...extra }
  if (parent) body.agent_id = sessionID
  try {
    const p = spawn(EMIT, ["--source", "opencode"], { stdio: ["pipe", "ignore", "ignore"], windowsHide: true })
    p.on("error", () => {})
    p.stdin.on("error", () => {})
    p.stdin.end(JSON.stringify(body))
  } catch {
    // The pet missing one event is never worth breaking the agent for.
  }
}

/** Only what the pet shows: the file a tool touches, and a shell command's first word. */
function slim(input) {
  const i = input && typeof input === "object" ? input : {}
  const out = {}
  const file = i.filePath ?? i.file_path ?? i.path
  if (typeof file === "string") out.file_path = file
  if (typeof i.command === "string") out.command = i.command.trim().split(/\s+/)[0]
  return out
}

function created(info) {
  if (info && info.id && info.parentID) parents.set(info.id, info.parentID)
}

function ended(sessionID, cwd) {
  if (failed.delete(sessionID)) return
  send("TurnDone", sessionID, cwd)
}

/** Events both generations publish under the same names. */
function onEvent(type, data, cwd) {
  const sid = data?.sessionID ?? data?.info?.id
  switch (type) {
    case "session.created":
      created(data?.info ?? data)
      send("SessionStart", sid, cwd)
      break
    case "session.deleted":
      send("SessionEnd", sid, cwd)
      parents.delete(sid)
      break
    case "session.idle":
      ended(sid, cwd)
      break
    case "session.error":
    case "session.execution.failed":
      failed.add(sid)
      send("TurnFailed", sid, cwd)
      break
    case "session.execution.interrupted":
      failed.add(sid)
      send("TurnAborted", sid, cwd)
      break
    case "permission.asked":
      send("Permission", sid, cwd)
      break
    case "message.part.updated": {
      // V1 only reports a failed tool here: its after-hook is skipped when the tool throws.
      const part = data?.part
      if (part?.type === "tool" && part?.state?.status === "error") {
        send("ToolEnd", part.sessionID, cwd, { tool_name: part.tool, tool_response: { success: false } })
      }
      break
    }
  }
}

export default {
  id: "rillbuddy",

  // OpenCode V2.
  async setup(ctx) {
    const here = ctx?.location?.directory
    await ctx.session.hook("prompt", (e) => {
      failed.delete(e.sessionID)
      send("TurnStart", e.sessionID, here)
    })
    await ctx.tool.hook("execute.before", (e) => send("ToolStart", e.sessionID, here, { tool_name: e.tool, tool_input: slim(e.input) }))
    await ctx.tool.hook("execute.after", (e) =>
      send("ToolEnd", e.sessionID, here, { tool_name: e.tool, tool_input: slim(e.input), tool_response: { success: e.status !== "error" } }),
    )
    const stop = new AbortController()
    void (async () => {
      try {
        const types = ["session.created", "session.deleted", "session.idle", "session.execution.failed", "session.execution.interrupted", "permission.asked"]
        for await (const e of ctx.event.subscribe({ signal: stop.signal, types })) onEvent(e.type, e.data, e.location?.directory ?? here)
      } catch {
        // The stream closed with the plugin.
      }
    })()
    return () => stop.abort()
  },

  // OpenCode V1 (1.18.29 and newer accept an object entry point).
  async server({ directory }) {
    return {
      "chat.message": async (input) => {
        failed.delete(input.sessionID)
        send("TurnStart", input.sessionID, directory)
      },
      "tool.execute.before": async (input, output) => send("ToolStart", input.sessionID, directory, { tool_name: input.tool, tool_input: slim(output?.args) }),
      "tool.execute.after": async (input) =>
        send("ToolEnd", input.sessionID, directory, { tool_name: input.tool, tool_input: slim(input.args), tool_response: { success: true } }),
      event: async ({ event }) => onEvent(event.type, event.properties, directory),
    }
  },
}
