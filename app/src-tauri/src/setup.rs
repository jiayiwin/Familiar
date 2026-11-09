//! Installed-app chores: start with Windows, and hooking up Claude Code and
//! Codex from the tray. Each one runs only when the user picks it, except
//! autostart, which is switched on once after a fresh install.

use std::path::{Path, PathBuf};
use std::process::Command;

const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_NAME: &str = "Familiar";
/// Autostart entry from before the rename.
const LEGACY_RUN_NAME: &str = "AgentPets";

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

/// The source tree this exe was built in (`<repo>/target/<profile>/familiar.exe`), if it still runs from there.
pub fn repo_root() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let root = exe.parent()?.parent()?.parent()?;
    root.join(".claude-plugin").is_dir().then(|| root.to_path_buf())
}

pub fn autostart_on() -> bool {
    run(Command::new("reg").args(["query", RUN_KEY, "/v", RUN_NAME])).is_ok()
}

pub fn set_autostart(on: bool) -> Result<(), String> {
    // Drop the pre-rename entry (it points at the old exe).
    if run(Command::new("reg").args(["query", RUN_KEY, "/v", LEGACY_RUN_NAME])).is_ok() {
        let _ = run(Command::new("reg").args(["delete", RUN_KEY, "/v", LEGACY_RUN_NAME, "/f"]));
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
    if is_dev_build() || flag.exists() {
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
    root.join("adapters").join("claude-code").join("bin").join(if cfg!(windows) { "familiar-emit.exe" } else { "familiar-emit" })
}

fn claude(args: &[&str]) -> Result<String, String> {
    // `claude` is usually an npm shim (claude.cmd), so go through cmd.
    if cfg!(windows) {
        run(Command::new("cmd").arg("/C").arg("claude").args(args))
    } else {
        run(Command::new("claude").args(args))
    }
}

/// Install the Familiar plugin into Claude Code (marketplace + plugin).
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
            claude(&["plugin", "marketplace", "update", "familiar"])?;
        }
        Err(e) => return Err(e),
    }
    match claude(&["plugin", "install", "familiar@familiar"]) {
        Ok(_) => {}
        Err(e) if e.contains("already") || e.contains("已") => {
            claude(&["plugin", "update", "familiar@familiar"])?;
        }
        Err(e) => return Err(e),
    }
    Ok(petd::tr!("Claude Code 已接入。重开 Claude Code 后生效。", "Claude Code is hooked up. Restart Claude Code to take effect."))
}

/// Append the Familiar hooks to Codex (backs up hooks.json first).
pub fn connect_codex(root: &Path) -> Result<String, String> {
    let exe = emitter(root);
    if !exe.is_file() {
        return Err(petd::tr!("找不到 {}", "Not found: {}", exe.display()));
    }
    let out = run(Command::new(&exe).args(["--install", "codex"]))?;
    Ok(petd::tr!(
        "{}\n\n最后一步：在终端运行 codex，输入 /hooks，把 Familiar 的 hook 标记为信任。",
        "{}\n\nLast step: run codex in a terminal, type /hooks and mark the Familiar hooks as trusted.",
        out.trim()
    ))
}
