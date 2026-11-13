//! ~/.rillbuddy layout.

use std::io::Write;
use std::path::PathBuf;

/// A junction or symlink (not a real folder).
pub fn is_link(p: &std::path::Path) -> bool {
    std::fs::symlink_metadata(p).is_ok_and(|m| {
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            m.file_type().is_symlink() || m.file_attributes() & 0x400 != 0 // FILE_ATTRIBUTE_REPARSE_POINT
        }
        #[cfg(not(windows))]
        {
            m.file_type().is_symlink()
        }
    })
}

/// `link` comes to point at the folder `to` (a junction on Windows: no rights needed).
fn link_dir(link: &std::path::Path, to: &std::path::Path) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = std::process::Command::new("cmd")
            .arg("/C")
            .arg("mklink")
            .arg("/J")
            .arg(link)
            .arg(to)
            .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
    #[cfg(unix)]
    {
        let _ = std::os::unix::fs::symlink(to, link);
    }
}

#[derive(Debug, Clone)]
pub struct Paths {
    pub home: PathBuf,
}

impl Paths {
    /// `$RILLBUDDY_HOME` (or the older `$FAMILIAR_HOME`), else `~/.rillbuddy`.
    pub fn from_env() -> Self {
        if let Some(h) = std::env::var_os("RILLBUDDY_HOME").or_else(|| std::env::var_os("FAMILIAR_HOME")) {
            return Paths { home: h.into() };
        }
        let base = PathBuf::from(
            std::env::var_os("USERPROFILE")
                .or_else(|| std::env::var_os("HOME"))
                .unwrap_or_default(),
        );
        Paths { home: Self::settle(&base) }
    }

    /// The data folder under the user's folder `base`, moved over from an older name if need be.
    fn settle(base: &std::path::Path) -> PathBuf {
        let home = base.join(".rillbuddy");
        if home.exists() {
            return home;
        }
        // The data folder was ~/.familiar, and ~/.agentpets before that: move it over once.
        let old = base.join(".familiar");
        if old.is_dir() && !is_link(&old) {
            if std::fs::rename(&old, &home).is_err() {
                // Something still has a file open in there: keep using it, and try again next start.
                return old;
            }
            // Hooks written before the rename run ~/.familiar/bin/… and read ~/.familiar/token:
            // a junction keeps them working until they are pointed here.
            link_dir(&old, &home);
        } else if !old.exists() {
            let legacy = base.join(".agentpets");
            if legacy.is_dir() {
                let _ = std::fs::rename(&legacy, &home);
            }
        }
        home
    }

    /// The old ~/.familiar, while it is a junction left by the rename (see [`Paths::from_env`]).
    pub fn old_link() -> Option<PathBuf> {
        let base = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?;
        let old = PathBuf::from(base).join(".familiar");
        is_link(&old).then_some(old)
    }

    pub fn at(home: impl Into<PathBuf>) -> Self {
        Paths { home: home.into() }
    }

    pub fn config(&self) -> PathBuf {
        self.home.join("config.yaml")
    }
    pub fn token(&self) -> PathBuf {
        self.home.join("token")
    }
    pub fn port(&self) -> PathBuf {
        self.home.join("port")
    }
    pub fn lock(&self) -> PathBuf {
        self.home.join("petd.lock")
    }
    pub fn petd_path(&self) -> PathBuf {
        self.home.join("petd_path")
    }
    pub fn spool(&self) -> PathBuf {
        self.home.join("spool")
    }
    pub fn interactions(&self) -> PathBuf {
        self.home.join("interactions")
    }
    pub fn logs(&self) -> PathBuf {
        self.home.join("logs")
    }

    pub fn ensure(&self) -> std::io::Result<()> {
        for d in [
            self.home.clone(),
            self.spool(),
            self.interactions(),
            self.logs(),
        ] {
            std::fs::create_dir_all(d)?;
        }
        Ok(())
    }

    /// Read the token, creating it on first run.
    pub fn ensure_token(&self) -> std::io::Result<String> {
        if let Ok(t) = std::fs::read_to_string(self.token()) {
            let t = t.trim().to_string();
            if t.len() >= 16 {
                return Ok(t);
            }
        }
        let mut b = [0u8; 24];
        getrandom::fill(&mut b).map_err(|e| std::io::Error::other(e.to_string()))?;
        let t: String = b.iter().map(|x| format!("{x:02x}")).collect();
        std::fs::write(self.token(), &t)?;
        Ok(t)
    }

    pub fn log(&self, msg: &str) {
        let line = format!("{} {msg}\n", crate::now_ms());
        eprint!("{line}");
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.logs().join("petd.log"))
        {
            let _ = f.write_all(line.as_bytes());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_old_data_folder_moves_over_and_old_paths_still_reach_it() {
        let base = std::env::temp_dir().join(format!("rillbuddy-paths-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join(".familiar").join("bin")).unwrap();
        std::fs::write(base.join(".familiar").join("token"), "t0k3n").unwrap();
        let home = Paths::settle(&base);
        assert_eq!(home, base.join(".rillbuddy"));
        assert_eq!(std::fs::read_to_string(home.join("token")).unwrap(), "t0k3n");
        assert!(is_link(&base.join(".familiar")), "the old name is left as a link");
        assert!(!is_link(&home));
        // Hooks from before the rename read the token through the old path.
        assert_eq!(std::fs::read_to_string(base.join(".familiar").join("token")).unwrap(), "t0k3n");
        // Next start: nothing moves again.
        assert_eq!(Paths::settle(&base), home);
        assert!(home.join("token").is_file());
        #[cfg(windows)]
        let _ = std::process::Command::new("cmd").arg("/C").arg("rmdir").arg(base.join(".familiar")).status();
        let _ = std::fs::remove_dir_all(&base);
    }
}
