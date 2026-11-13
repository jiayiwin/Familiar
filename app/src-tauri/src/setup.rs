//! Installed-app chores: start with Windows, and hooking up Claude Code and
//! Codex from the tray. Each one runs only when the user picks it, except
//! autostart, which is switched on once after a fresh install.

use std::path::{Path, PathBuf};
use std::process::Command;

const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_NAME: &str = "RillBuddy";
/// Autostart entries from before the renames (they point at the old exe).
const LEGACY_RUN_NAMES: [&str; 3] = ["AgentPets", "Familiar", "Tutti"];

fn has_run_value(name: &str) -> bool {
    run(Command::new("reg").args(["query", RUN_KEY, "/v", name])).is_ok()
}

fn hidden(cmd: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    cmd
}

fn run(cmd: &mut Command) -> Result<String, String> {
    let out = hidden(cmd).output().map_err(|e| e.to_string())?;
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    if out.status.success() {
        Ok(text)
    } else {
        Err(text.trim().to_string())
    }
}

/// Is this exe a build inside a source tree (target/debug, target/release)?
pub fn is_dev_build() -> bool {
    std::env::current_exe()
        .map(|p| p.components().any(|c| c.as_os_str().eq_ignore_ascii_case("target")))
        .unwrap_or(true)
}

/// The source tree this exe was built in (`<repo>/target/<profile>/rillbuddy.exe`), if it still runs from there.
pub fn repo_root() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let root = exe.parent()?.parent()?.parent()?;
    root.join(".claude-plugin").is_dir().then(|| root.to_path_buf())
}

pub fn autostart_on() -> bool {
    has_run_value(RUN_NAME)
}

pub fn set_autostart(on: bool) -> Result<(), String> {
    // Drop the pre-rename entries (they point at the old exe).
    for old in LEGACY_RUN_NAMES {
        if has_run_value(old) {
            let _ = run(Command::new("reg").args(["delete", RUN_KEY, "/v", old, "/f"]));
        }
    }
    if on {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let value = format!("\"{}\"", exe.display());
        run(Command::new("reg").args(["add", RUN_KEY, "/v", RUN_NAME, "/t", "REG_SZ", "/d", &value, "/f"]))?;
    } else if autostart_on() {
        run(Command::new("reg").args(["delete", RUN_KEY, "/v", RUN_NAME, "/f"]))?;
    }
    Ok(())
}

/// After a fresh install, start with Windows by default (once; turning it off
/// in the tray sticks). Source-tree builds never touch the registry.
pub fn first_install(home: &Path) {
    let flag = home.join(".autostart-init");
    if is_dev_build() {
        return;
    }
    // It started with Windows under an earlier name, or from the exe before the rename
    // (familiar.exe): it still does, from this exe.
    if autostart_on() || LEGACY_RUN_NAMES.iter().any(|n| has_run_value(n)) {
        let _ = set_autostart(true);
    }
    if flag.exists() {
        return;
    }
    if set_autostart(true).is_ok() {
        let _ = std::fs::write(flag, "1");
    }
}

/// The Claude Code plugin bundled next to the exe (marketplace root).
pub fn plugin_root(resource_dir: Option<PathBuf>) -> Option<PathBuf> {
    let installed = resource_dir.map(|d| d.join("plugin"));
    if let Some(p) = installed.filter(|p| p.join(".claude-plugin").is_dir()) {
        return Some(p);
    }
    repo_root()
}

fn emitter(root: &Path) -> PathBuf {
    root.join("adapters").join("claude-code").join("bin").join(if cfg!(windows) { "rillbuddy-emit.exe" } else { "rillbuddy-emit" })
}

fn claude(args: &[&str]) -> Result<String, String> {
    // `claude` is usually an npm shim (claude.cmd), so go through cmd.
    if cfg!(windows) {
        run(Command::new("cmd").arg("/C").arg("claude").args(args))
    } else {
        run(Command::new("claude").args(args))
    }
}

/// Install the Rill plugin into Claude Code (marketplace + plugin).
pub fn connect_claude(root: &Path) -> Result<String, String> {
    if claude(&["--version"]).is_err() {
        return Err(petd::tr!(
            "没找到 claude 命令。先安装 Claude Code 命令行版，或按 README 手动接入。",
            "Could not find the claude command. Install the Claude Code CLI first, or hook it up by hand (see the README)."
        ));
    }
    let path = root.display().to_string().replace('\\', "/");
    match claude(&["plugin", "marketplace", "add", &path]) {
        Ok(_) => {}
        // Already added (maybe from the source tree): refresh it instead.
        Err(e) if e.contains("already") || e.contains("已") => {
            claude(&["plugin", "marketplace", "update", "rillbuddy"])?;
        }
        Err(e) => return Err(e),
    }
    match claude(&["plugin", "install", "rillbuddy@rillbuddy"]) {
        Ok(_) => {}
        Err(e) if e.contains("already") || e.contains("已") => {
            claude(&["plugin", "update", "rillbuddy@rillbuddy"])?;
        }
        Err(e) => return Err(e),
    }
    // The same plugin under its name from before the rename would send every event twice.
    if old_plugin_on() {
        let _ = claude(&["plugin", "disable", OLD_PLUGIN]);
        let _ = std::fs::write(petd::paths::Paths::from_env().home.join(".old-plugin-off"), petd::now_ms().to_string());
    }
    Ok(petd::tr!("Claude Code 已接入。重开 Claude Code 后生效。", "Claude Code is hooked up. Restart Claude Code to take effect."))
}

/// Point Claude Code's status line at the emitter so the pets learn the usage
/// limits (a status line the user already has is kept and still shown).
pub fn connect_usage(root: &Path) -> Result<String, String> {
    let exe = emitter(root);
    if !exe.is_file() {
        return Err(petd::tr!("找不到 {}", "Not found: {}", exe.display()));
    }
    let out = run(Command::new(&exe).args(["--install", "statusline"]))?;
    Ok(out.trim().to_string())
}

/// Append the Rill hooks to another agent's own config (Gemini CLI, Cursor, OpenCode).
pub fn connect_other(root: &Path, id: &str) -> Result<String, String> {
    let exe = emitter(root);
    if !exe.is_file() {
        return Err(petd::tr!("找不到 {}", "Not found: {}", exe.display()));
    }
    let out = run(Command::new(&exe).args(["--install", id]))?;
    Ok(out.trim().to_string())
}

/// Append the Rill hooks to Codex (backs up hooks.json first).
pub fn connect_codex(root: &Path) -> Result<String, String> {
    let exe = emitter(root);
    if !exe.is_file() {
        return Err(petd::tr!("找不到 {}", "Not found: {}", exe.display()));
    }
    let out = run(Command::new(&exe).args(["--install", "codex"]))?;
    Ok(petd::tr!(
        "{}\n\n最后一步：在终端运行 codex，输入 /hooks，把睿洱的 hook 标记为信任。",
        "{}\n\nLast step: run codex in a terminal, type /hooks and mark the Rill hooks as trusted.",
        out.trim()
    ))
}

// ---- After the rename (Familiar → RillBuddy) ----------------------------------------------
//
// Hook-ups made before it name `familiar-emit`, the `familiar@familiar` plugin and ~/.familiar.
// They keep working: ~/.familiar is left as a junction to ~/.rillbuddy, the emitter copies they
// run are refreshed, and petd still takes the old token header. Moving them to the new names
// rewrites the user's agent config, so a pet asks first; Codex is left as it is (its hooks would
// have to be trusted again), until it is hooked up again from Settings.

const OLD_PLUGIN: &str = "familiar@familiar";
const OLD_EMIT: &str = "familiar-emit";

fn user_home() -> PathBuf {
    PathBuf::from(std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).unwrap_or_default())
}

fn claude_dir() -> PathBuf {
    std::env::var_os("CLAUDE_CONFIG_DIR").filter(|d| !d.is_empty()).map(PathBuf::from).unwrap_or_else(|| user_home().join(".claude"))
}

fn codex_dir() -> PathBuf {
    std::env::var_os("CODEX_HOME").filter(|d| !d.is_empty()).map(PathBuf::from).unwrap_or_else(|| user_home().join(".codex"))
}

fn text(p: &Path) -> String {
    std::fs::read_to_string(p).unwrap_or_default()
}

fn claude_settings() -> serde_json::Value {
    serde_json::from_str(&text(&claude_dir().join("settings.json"))).unwrap_or_default()
}

/// The plugin from before the rename is switched on in Claude Code.
fn old_plugin_on() -> bool {
    claude_settings().pointer("/enabledPlugins/familiar@familiar").and_then(|v| v.as_bool()) == Some(true)
}

fn old_plugin_installed() -> bool {
    text(&claude_dir().join("plugins").join("installed_plugins.json")).contains(&format!("\"{OLD_PLUGIN}\""))
}

/// What is still hooked up under the old names and can be moved without asking anything
/// more of the user: "claude", "statusline", "gemini", "cursor", "opencode".
pub fn old_hookups() -> Vec<&'static str> {
    let mut out = Vec::new();
    if old_plugin_on() {
        out.push("claude");
    }
    if claude_settings().pointer("/statusLine/command").and_then(|v| v.as_str()).is_some_and(|c| c.contains(OLD_EMIT)) {
        out.push("statusline");
    }
    if text(&user_home().join(".gemini").join("settings.json")).contains(OLD_EMIT) {
        out.push("gemini");
    }
    if text(&user_home().join(".cursor").join("hooks.json")).contains(OLD_EMIT) {
        out.push("cursor");
    }
    if text(&user_home().join(".config").join("opencode").join("plugins").join("familiar.js")).contains(OLD_EMIT) {
        out.push("opencode");
    }
    out
}

/// Move what [`old_hookups`] found to the new names. Says what it did, one line each.
pub fn move_hookups(root: &Path) -> String {
    let mut said = Vec::new();
    for what in old_hookups() {
        let r = match what {
            "claude" => connect_claude(root),
            "statusline" => connect_usage(root),
            other => connect_other(root, other),
        };
        if let Err(e) = r {
            said.push(petd::tr!("{} 没换成：{}", "{} was not moved: {}", what, e));
        }
    }
    if said.is_empty() {
        petd::tr!("换好了。重开 Claude Code 后用新名字。", "Done. Claude Code uses the new name once restarted.")
    } else {
        said.join("\n")
    }
}

/// The copies of the emitter that hooks run (~/.rillbuddy/bin, under either name) get this
/// build's: a Codex hook from before the rename runs familiar-emit.exe through the junction.
fn refresh_emitters(root: &Path, home: &Path) {
    let src = emitter(root);
    let Ok(new) = std::fs::read(&src) else { return };
    for name in ["rillbuddy-emit", OLD_EMIT] {
        let dst = home.join("bin").join(if cfg!(windows) { format!("{name}.exe") } else { name.to_string() });
        if !dst.is_file() || std::fs::read(&dst).is_ok_and(|old| old == new) {
            continue;
        }
        // A hook may be running it: write next to it and swap.
        let tmp = dst.with_extension("new");
        if std::fs::write(&tmp, &new).is_ok() && std::fs::rename(&tmp, &dst).is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
    }
}

/// Once a day at most, while the old names are in use: tidy up after the rename. Returns
/// whether a pet should ask to move the hook-ups (what [`old_hookups`] finds).
pub fn after_rename(root: Option<&Path>, home: &Path) -> bool {
    if is_dev_build() {
        return false;
    }
    if let Some(root) = root {
        refresh_emitters(root, home);
    }
    // The old plugin was switched off a day ago or more (sessions started before have ended): remove it.
    let off = home.join(".old-plugin-off");
    let since = text(&off).trim().parse::<u64>().ok();
    if since.is_some_and(|t| petd::now_ms().saturating_sub(t) > 24 * 3_600_000) && !old_plugin_on() {
        if old_plugin_installed() {
            let _ = claude(&["plugin", "uninstall", OLD_PLUGIN]);
            let _ = claude(&["plugin", "marketplace", "remove", "familiar"]);
        }
        let _ = std::fs::remove_file(&off);
    }
    // Nothing runs anything through ~/.familiar any more: the junction goes (only the link).
    if let Some(link) = petd::paths::Paths::old_link() {
        let codex = text(&codex_dir().join("hooks.json")).contains(OLD_EMIT);
        if !codex && !old_plugin_installed() && old_hookups().is_empty() {
            let _ = run(Command::new("cmd").arg("/C").arg("rmdir").arg(&link));
        }
    }
    let todo = old_hookups();
    if todo.is_empty() {
        return false;
    }
    // Asked in the last three days: not again yet.
    let last = text(&home.join(".rename-asked")).trim().parse::<u64>().unwrap_or(0);
    petd::now_ms().saturating_sub(last) >= 3 * 24 * 3_600_000
}

pub fn mark_asked(home: &Path) {
    let _ = std::fs::write(home.join(".rename-asked"), petd::now_ms().to_string());
}

/// The windows' own data (letters kept, radio favourites) was under the old app id: take it
/// over once, before any window opens.
pub fn adopt_old_webview_data() {
    if is_dev_build() {
        return;
    }
    let Some(base) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) else { return };
    let (old, new) = (base.join("dev.familiar.desktop"), base.join("dev.rillbuddy.desktop"));
    if !new.exists() && old.is_dir() {
        let _ = std::fs::rename(old, new);
    }
}
