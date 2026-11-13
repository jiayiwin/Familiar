//! petd: the Rill daemon core. The Tauri app embeds it; it has no GUI code
//! of its own.

pub mod balance;
pub mod bond;
pub mod cast;
pub mod codex;
pub mod config;
pub mod db;
pub mod drawer;
pub mod hub;
pub mod i18n;
pub mod night;
pub mod local_llm;
pub mod ops;
pub mod paths;
pub mod song;
pub mod protocol;
pub mod server;
pub mod session;
pub mod storeroom;
pub mod think;
pub mod tidy;
pub mod todo;
pub mod usage;
pub mod voice;
pub mod world;

use fs4::fs_std::FileExt;
use hub::Hub;
use paths::Paths;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::broadcast;

pub const DEFAULT_PORT: u16 = 47321;

/// Requests from petd to the host app.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Control {
    Show,
    Hide,
    /// The set of pets changed: create / close windows to match.
    Layout {
        ids: Vec<String>,
    },
    /// config.yaml was reloaded.
    ConfigChanged,
    /// Walk a pet next to another pet, or back home when `target` is None.
    Move {
        pet: String,
        target: Option<String>,
    },
    /// Show today's report card.
    Report,
    /// Open the radio (`/pet radio`).
    Radio,
    /// Open the right-click menu on a pet (`/pet menu`).
    Menu {
        pet: String,
    },
    /// Throw a pet (tests and `/pet fling`): logical px/s.
    Fling {
        pet: String,
        vx: f64,
        vy: f64,
    },
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// A WS message: `{"type": kind, "data": …}`.
pub fn msg<T: Serialize>(kind: &str, data: &T) -> String {
    serde_json::json!({ "type": kind, "data": data }).to_string()
}

#[derive(Debug, Default, Clone)]
pub struct Options {
    /// Override ~/.rillbuddy (tests).
    pub home: Option<PathBuf>,
    /// Override the port (tests use 0).
    pub port: Option<u16>,
}

#[derive(Debug)]
pub enum StartError {
    AlreadyRunning,
    Io(String),
}

impl std::fmt::Display for StartError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StartError::AlreadyRunning => {
                f.write_str(crate::i18n::t("睿洱已在运行", "Rill is already running"))
            }
            StartError::Io(e) => write!(f, "{e}"),
        }
    }
}

/// Single-instance lock. Hold the returned file for the life of the process.
pub fn lock(paths: &Paths) -> Result<std::fs::File, StartError> {
    paths.ensure().map_err(|e| StartError::Io(e.to_string()))?;
    let f = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(paths.lock())
        .map_err(|e| StartError::Io(e.to_string()))?;
    match f.try_lock_exclusive() {
        Ok(true) => Ok(f),
        _ => Err(StartError::AlreadyRunning),
    }
}

pub struct Petd {
    pub port: u16,
    pub token: String,
    pub paths: Paths,
    pub hub: Arc<Mutex<Hub>>,
    pub control: broadcast::Sender<Control>,
    pub shared: server::Shared,
    /// config.yaml did not exist before this start.
    pub first_run: bool,
    _lock: std::fs::File,
}

/// Start the daemon on the current tokio runtime. Returns once it is listening.
pub async fn start(opts: Options) -> Result<Petd, StartError> {
    let paths = opts
        .home
        .clone()
        .map(Paths::at)
        .unwrap_or_else(Paths::from_env);
    let lock = lock(&paths)?;
    let io = |e: std::io::Error| StartError::Io(e.to_string());
    let token = paths.ensure_token().map_err(io)?;
    let first_run = !paths.config().exists();
    if first_run {
        let _ = std::fs::write(paths.config(), config::DEFAULT_YAML);
    }
    if let Ok(me) = std::env::current_exe() {
        let _ = std::fs::write(paths.petd_path(), me.display().to_string());
    }

    let (cfg, cfg_error) = match config::Config::load(&paths.config()) {
        Ok(c) => (c, None),
        Err(e) => {
            paths.log(&e);
            (config::Config::default(), Some(e))
        }
    };
    // Tables pick their language when loaded.
    i18n::set_from(&cfg.language);
    let mut h = Hub::new(cfg);
    h.cfg_error = cfg_error;
    let hub = Arc::new(Mutex::new(h));
    let tables = balance::Tables::load(&paths.home);
    for w in &tables.warnings {
        paths.log(w);
    }
    adopt_legacy_db(&paths.home);
    let db = Arc::new(
        db::Db::open(&paths.home.join("rillbuddy.db"))
            .map_err(|e| StartError::Io(format!("rillbuddy.db: {e}")))?,
    );
    db.compact(now_ms());
    let world = Arc::new(Mutex::new(world::World::new(tables, now_ms())));

    // Bind before replaying the spool so new events queue in the backlog
    // instead of racing the replay.
    let want = opts.port.unwrap_or(DEFAULT_PORT);
    let listener = match tokio::net::TcpListener::bind(("127.0.0.1", want)).await {
        Ok(l) => l,
        Err(_) => tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .map_err(io)?,
    };
    let port = listener.local_addr().map_err(io)?.port();
    std::fs::write(paths.port(), port.to_string()).map_err(io)?;

    let replayed = replay_spool(&paths, &hub);
    paths.log(&format!(
        "petd listening on 127.0.0.1:{port}, replayed {replayed} spooled events"
    ));

    let (ws_tx, _) = broadcast::channel(256);
    let (control, _) = broadcast::channel(16);
    let shared = server::Shared {
        hub: hub.clone(),
        token: token.clone(),
        paths: paths.clone(),
        ws_tx,
        control: control.clone(),
        urgent: Arc::new(tokio::sync::Notify::new()),
        world,
        db,
    };
    let app = server::router(shared.clone());
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    tokio::spawn(background(shared.clone()));
    tokio::spawn(think::run(shared.clone()));
    spawn_codex_watch(shared.clone());

    Ok(Petd {
        port,
        token,
        paths,
        hub,
        control,
        shared,
        first_run,
        _lock: lock,
    })
}

/// The database was called familiar.db, and agentpets.db before that. If an
/// old one is still there, it holds the real history: use it (a rillbuddy.db
/// created since is kept as rillbuddy.db.bak).
fn adopt_legacy_db(home: &std::path::Path) {
    for old in ["familiar", "agentpets"] {
        if home.join(format!("{old}.db")).is_file() {
            adopt_db(home, old);
            return;
        }
    }
}

fn adopt_db(home: &std::path::Path, old: &str) {
    let cur = home.join("rillbuddy.db");
    if cur.exists() {
        for ext in ["", "-wal", "-shm"] {
            let from = home.join(format!("rillbuddy.db{ext}"));
            if from.exists() {
                let _ = std::fs::rename(&from, home.join(format!("rillbuddy.db.bak{ext}")));
            }
        }
    }
    for ext in ["", "-wal", "-shm"] {
        let from = home.join(format!("{old}.db{ext}"));
        if from.exists() {
            let _ = std::fs::rename(&from, home.join(format!("rillbuddy.db{ext}")));
        }
    }
}

/// Once a minute: Codex usage limits from its session logs (read-only).
fn spawn_codex_watch(sh: server::Shared) {
    std::thread::spawn(move || loop {
        let pet = {
            let views = sh.hub.lock().unwrap().pets();
            world::World::camp_pet(&views, "codex").map(|v| v.id.clone())
        };
        for m in codex::poll(pet.as_deref(), now_ms()) {
            let _ = sh.ws_tx.send(m);
        }
        std::thread::sleep(std::time::Duration::from_secs(60));
    });
}

/// Apply spooled events in file-name (= time) order, then delete them.
pub fn replay_spool(paths: &Paths, hub: &Arc<Mutex<Hub>>) -> usize {
    let Ok(rd) = std::fs::read_dir(paths.spool()) else {
        return 0;
    };
    let mut files: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("json"))
        .collect();
    files.sort();
    let now = now_ms();
    let mut n = 0;
    let mut h = hub.lock().unwrap();
    for f in files {
        if let Ok(text) = std::fs::read_to_string(&f) {
            if let Ok(ev) = serde_json::from_str::<protocol::Event>(&text) {
                h.apply(&ev, now);
                n += 1;
            }
        }
        let _ = std::fs::remove_file(&f);
    }
    n
}

fn mtime(p: &std::path::Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

/// Timers, liveness checks, hot reload and the rate-limited push to windows
/// (≤ 10 updates/s per pet; attention goes out immediately).
async fn background(sh: server::Shared) {
    let mut tick = tokio::time::interval(Duration::from_millis(100));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut n: u64 = 0;
    let mut cfg_mtime = mtime(&sh.paths.config());
    loop {
        tokio::select! {
            _ = tick.tick() => {}
            _ = sh.urgent.notified() => {}
        }
        n += 1;
        let now = now_ms();
        if n % 10 == 0 {
            sh.hub.lock().unwrap().tick(now);
        }
        if n % 50 == 0 {
            replay_spool(&sh.paths, &sh.hub);
        }
        // Zombie check every 5 s: closing a waiting terminal stops its pet waiting.
        if n % 50 == 0 {
            let dead = sh.hub.lock().unwrap().reap(now, pid_alive);
            if dead > 0 {
                sh.paths.log(&format!(
                    "{dead} session(s) marked offline: agent process gone"
                ));
            }
        }
        if n % 10 == 5 {
            let m = mtime(&sh.paths.config());
            if m != cfg_mtime {
                cfg_mtime = m;
                ops::reload_config(&sh);
            }
        }
        let outs = {
            let mut hub = sh.hub.lock().unwrap();
            sh.world.lock().unwrap().tick(now, &mut hub, &sh.db)
        };
        for o in outs {
            match o {
                world::Out::Ws(m) => {
                    let _ = sh.ws_tx.send(m);
                }
                world::Out::Control(c) => {
                    let _ = sh.control.send(c);
                }
            }
        }
        let (layout, changed) = {
            let mut hub = sh.hub.lock().unwrap();
            (hub.layout_changed(), hub.changed())
        };
        if let Some(ids) = layout {
            let _ = sh.ws_tx.send(msg("pets.layout", &ids));
            let _ = sh.control.send(Control::Layout { ids });
        }
        for v in changed {
            let _ = sh.ws_tx.send(msg("pet.state", &v));
        }
    }
}

#[cfg(windows)]
pub fn pid_alive(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, STILL_ACTIVE};
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            // Access denied still means it exists; only "invalid parameter" means gone.
            return windows_sys::Win32::Foundation::GetLastError()
                == windows_sys::Win32::Foundation::ERROR_ACCESS_DENIED;
        }
        let mut code = 0u32;
        let ok = GetExitCodeProcess(h, &mut code);
        CloseHandle(h);
        ok != 0 && code == STILL_ACTIVE as u32
    }
}

#[cfg(target_os = "linux")]
pub fn pid_alive(pid: u32) -> bool {
    std::path::Path::new(&format!("/proc/{pid}")).exists()
}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn pid_alive(_pid: u32) -> bool {
    true
}
