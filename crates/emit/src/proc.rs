//! Process helpers: find the agent's main process, start petd detached.

use std::path::Path;

/// Executable names that count as "the agent process".
const AGENT_EXES: &[&str] = &["claude", "codex", "node", "bun", "gemini", "cursor", "cursor-agent", "opencode"];

fn is_agent_exe(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    let n = n.strip_suffix(".exe").unwrap_or(&n);
    AGENT_EXES.contains(&n)
}

/// PID of the agent that ran this hook.
///
/// Hooks run through a shell that exits right after us, so our direct parent
/// is useless for liveness checks. Walk up the tree to the first agent-like
/// executable. Returns 0 when it cannot be determined (petd then skips the
/// liveness check for that session).
pub fn agent_pid() -> u32 {
    let tree: std::collections::HashMap<u32, (u32, String)> = process_table().into_iter().collect();
    let mut pid = std::process::id();
    for _ in 0..12 {
        let Some(ppid) = tree.get(&pid).map(|(pp, _)| *pp) else { break };
        let Some((_, exe)) = tree.get(&ppid) else { break };
        if is_agent_exe(exe) {
            return ppid;
        }
        if ppid == 0 || ppid == pid {
            break;
        }
        pid = ppid;
    }
    0
}

/// (pid, (parent pid, exe name))
#[cfg(windows)]
fn process_table() -> Vec<(u32, (u32, String))> {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::*;
    let mut out = Vec::new();
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE {
            return out;
        }
        let mut e: PROCESSENTRY32W = std::mem::zeroed();
        e.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        if Process32FirstW(snap, &mut e) != 0 {
            loop {
                let len = e.szExeFile.iter().position(|&c| c == 0).unwrap_or(e.szExeFile.len());
                let name = String::from_utf16_lossy(&e.szExeFile[..len]);
                out.push((e.th32ProcessID, (e.th32ParentProcessID, name)));
                if Process32NextW(snap, &mut e) == 0 {
                    break;
                }
            }
        }
        CloseHandle(snap);
    }
    out
}

#[cfg(target_os = "linux")]
fn process_table() -> Vec<(u32, (u32, String))> {
    let mut out = Vec::new();
    let Ok(dir) = std::fs::read_dir("/proc") else { return out };
    for ent in dir.flatten() {
        let Ok(pid) = ent.file_name().to_string_lossy().parse::<u32>() else { continue };
        let Ok(stat) = std::fs::read_to_string(ent.path().join("stat")) else { continue };
        // pid (comm) state ppid ...
        let (Some(l), Some(r)) = (stat.find('('), stat.rfind(')')) else { continue };
        let comm = stat[l + 1..r].to_string();
        let ppid = stat[r + 1..].split_whitespace().nth(1).and_then(|s| s.parse().ok()).unwrap_or(0);
        out.push((pid, (ppid, comm)));
    }
    out
}

#[cfg(not(any(windows, target_os = "linux")))]
fn process_table() -> Vec<(u32, (u32, String))> {
    // macOS: fill in with sysctl(KERN_PROC) when the mac port lands.
    Vec::new()
}

/// Start petd so that it outlives the agent (and the agent's job object).
pub fn spawn_detached(exe: &Path) -> bool {
    use std::process::{Command, Stdio};
    let mut cmd = Command::new(exe);
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // The hook's stdout/stderr are pipes the agent reads until EOF. If petd
        // inherited them, the agent would wait for petd to exit (Codex times
        // the hook out after 5 s). Make ours non-inheritable before spawning.
        unsafe {
            use windows_sys::Win32::Foundation::{SetHandleInformation, HANDLE_FLAG_INHERIT, INVALID_HANDLE_VALUE};
            use windows_sys::Win32::System::Console::{GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE};
            for id in [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
                let h = GetStdHandle(id);
                if !h.is_null() && h != INVALID_HANDLE_VALUE {
                    SetHandleInformation(h, HANDLE_FLAG_INHERIT, 0);
                }
            }
        }
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
        let base = DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP;
        cmd.creation_flags(base | CREATE_BREAKAWAY_FROM_JOB);
        if cmd.spawn().is_ok() {
            return true;
        }
        // The job may forbid breakaway; still better than not starting at all.
        cmd.creation_flags(base);
        cmd.spawn().is_ok()
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
        cmd.spawn().is_ok()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn agent_exe_names() {
        assert!(super::is_agent_exe("claude.exe"));
        assert!(super::is_agent_exe("Node.EXE"));
        assert!(super::is_agent_exe("codex"));
        assert!(!super::is_agent_exe("bash.exe"));
    }
}
