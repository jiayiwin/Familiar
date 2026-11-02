//! ~/.familiar layout.

use std::io::Write;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Paths {
    pub home: PathBuf,
}

impl Paths {
    /// `$FAMILIAR_HOME`, else `~/.familiar`.
    pub fn from_env() -> Self {
        if let Some(h) = std::env::var_os("FAMILIAR_HOME") {
            return Paths { home: h.into() };
        }
        let base = PathBuf::from(std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).unwrap_or_default());
        let home = base.join(".familiar");
        // The project was called AgentPets: move its data folder over once.
        let legacy = base.join(".agentpets");
        if !home.exists() && legacy.is_dir() {
            let _ = std::fs::rename(&legacy, &home);
        }
        Paths { home }
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
    pub fn pets(&self) -> PathBuf {
        self.home.join("pets")
    }
    pub fn interactions(&self) -> PathBuf {
        self.home.join("interactions")
    }
    pub fn logs(&self) -> PathBuf {
        self.home.join("logs")
    }

    pub fn ensure(&self) -> std::io::Result<()> {
        for d in [self.home.clone(), self.spool(), self.pets(), self.interactions(), self.logs()] {
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
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(self.logs().join("petd.log")) {
            let _ = f.write_all(line.as_bytes());
        }
    }
}
