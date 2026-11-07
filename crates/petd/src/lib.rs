//! petd: the Familiar daemon core. The Tauri app embeds it (design §2:
//! "petd = Tauri main process"); it has no GUI code of its own.

pub mod balance;
pub mod bond;
pub mod local_llm;
pub mod codex;
pub mod config;
pub mod db;
pub mod duo;
pub mod games;
pub mod games_db;
pub mod hub;
pub mod i18n;
pub mod install;
pub mod ops;
pub mod pack;
pub mod paths;
pub mod progress;
pub mod protocol;
pub mod server;
pub mod session;
pub mod spring;
pub mod whip;
pub mod xiangsheng;
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
    Layout { ids: Vec<String> },
    /// Packs were added, removed or edited (tray shows broken ones).
    LibraryChanged,
    /// config.yaml was reloaded (tray shows the mode).
    ConfigChanged,
    /// Walk a pet next to another pet, or back home when `target` is None.
    Move { pet: String, target: Option<String> },
    /// Tug of war: `mover` walks up to `anchor`, windows edge to edge, so the
    /// rope each pet draws to its window edge joins up.
    Tug { mover: String, anchor: String },
    /// Two-pet game: `mover` walks to `gap` logical px from `anchor`.
    Duo { mover: String, anchor: String, gap: f64 },
    /// Show today's report card.
    Report,
    /// Open the Spring Festival countdown (`/pet spring`).
    Spring,
    /// Open the radio (`/pet radio`).
    Radio,
    /// Open the ring menu on a pet (`/pet menu`).
    Menu { pet: String },
    /// Throw a pet (tests and `/pet fling`): logical px/s.
    Fling { pet: String, vx: f64, vy: f64 },
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// A WS message: `{"type": kind, "data": …}`.
pub fn msg<T: Serialize>(kind: &str, data: &T) -> String {
    serde_json::json!({ "type": kind, "data": data }).to_string()
}

#[derive(Debug, Default, Clone)]
pub struct Options {
    /// Built-in packs shipped with the app.
    pub builtin_pets: Option<PathBuf>,
    /// Override ~/.familiar (tests).
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
            StartError::AlreadyRunning => f.write_str(crate::i18n::t("Familiar 已在运行", "Familiar is already running")),
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
    let paths = opts.home.clone().map(Paths::at).unwrap_or_else(Paths::from_env);
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
    let mut roots = Vec::new();
    if let Some(b) = &opts.builtin_pets {
        roots.push((b.clone(), true));
    }
    roots.push((paths.pets(), false));
    // Packs and tables pick their language when loaded.
    i18n::set_from(&cfg.language);
    let lib = pack::Library::scan(&roots);
    log_broken(&paths, &lib);
    let mut h = Hub::new(cfg, lib);
    h.cfg_error = cfg_error;
    let hub = Arc::new(Mutex::new(h));
    let tables = balance::Tables::load(&paths.home);
    for w in &tables.warnings {
        paths.log(w);
    }
    adopt_legacy_db(&paths.home);
    let db = Arc::new(db::Db::open(&paths.home.join("familiar.db")).map_err(|e| StartError::Io(format!("familiar.db: {e}")))?);
    db.compact(now_ms());
    let world = Arc::new(Mutex::new(world::World::new(tables, now_ms())));

    // Bind before replaying the spool so new events queue in the backlog
    // instead of racing the replay.
    let want = opts.port.unwrap_or(DEFAULT_PORT);
    let listener = match tokio::net::TcpListener::bind(("127.0.0.1", want)).await {
        Ok(l) => l,
        Err(_) => tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.map_err(io)?,
    };
    let port = listener.local_addr().map_err(io)?.port();
    std::fs::write(paths.port(), port.to_string()).map_err(io)?;

    let replayed = replay_spool(&paths, &hub);
    paths.log(&format!("petd listening on 127.0.0.1:{port}, replayed {replayed} spooled events"));

    let (ws_tx, _) = broadcast::channel(256);
    let (control, _) = broadcast::channel(16);
    let shared = server::Shared {
        hub: hub.clone(),
        token: token.clone(),
        paths: paths.clone(),
        ws_tx,
        control: control.clone(),
        urgent: Arc::new(tokio::sync::Notify::new()),
        roots,
        world,
        db,
    };
    let app = server::router(shared.clone());
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    tokio::spawn(background(shared.clone()));
    spawn_codex_watch(shared.clone());

    Ok(Petd { port, token, paths, hub, control, shared, first_run, _lock: lock })
}

/// The database was called agentpets.db before the rename. If it is still
/// there, it holds the real history: use it (a familiar.db created since is
/// kept as familiar.db.bak).
fn adopt_legacy_db(home: &std::path::Path) {
    let legacy = home.join("agentpets.db");
    if !legacy.is_file() {
        return;
    }
    let cur = home.join("familiar.db");
    if cur.exists() {
        for ext in ["", "-wal", "-shm"] {
            let from = home.join(format!("familiar.db{ext}"));
            if from.exists() {
                let _ = std::fs::rename(&from, home.join(format!("familiar.db.bak{ext}")));
            }
        }
    }
    for ext in ["", "-wal", "-shm"] {
        let from = home.join(format!("agentpets.db{ext}"));
        if from.exists() {
            let _ = std::fs::rename(&from, home.join(format!("familiar.db{ext}")));
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
    let Ok(rd) = std::fs::read_dir(paths.spool()) else { return 0 };
    let mut files: Vec<PathBuf> =
        rd.flatten().map(|e| e.path()).filter(|p| p.extension().and_then(|x| x.to_str()) == Some("json")).collect();
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

/// Rescan the pack folders now and tell windows (install, remove, hot reload).
pub fn rescan_packs(sh: &server::Shared) {
    let lib = pack::Library::scan(&sh.roots);
    log_broken(&sh.paths, &lib);
    {
        let mut hub = sh.hub.lock().unwrap();
        hub.lib = lib;
        hub.lib_rev += 1;
    }
    let _ = sh.ws_tx.send(msg("packs.changed", &serde_json::json!({})));
    let _ = sh.control.send(Control::LibraryChanged);
    sh.urgent.notify_one();
}

fn log_broken(paths: &Paths, lib: &pack::Library) {
    for b in &lib.broken {
        paths.log(&format!("pack {} invalid: {}", b.dir, b.errors.join("; ")));
    }
}

fn mtime(p: &std::path::Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

/// Cheap change detector for the pack folders: names, sizes and mtimes of
/// every pack directory and the files inside it (two levels deep).
pub fn library_fingerprint(roots: &[(PathBuf, bool)]) -> u64 {
    use std::hash::{Hash, Hasher};
    fn walk(dir: &std::path::Path, depth: u32, h: &mut impl Hasher) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        let mut ents: Vec<_> = rd.flatten().collect();
        ents.sort_by_key(|e| e.file_name());
        for e in ents {
            e.file_name().hash(h);
            if let Ok(m) = e.metadata() {
                m.len().hash(h);
                if let Ok(t) = m.modified() {
                    t.hash(h);
                }
                if m.is_dir() && depth > 0 {
                    walk(&e.path(), depth - 1, h);
                }
            }
        }
    }
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for (root, _) in roots {
        root.hash(&mut h);
        walk(root, 2, &mut h);
    }
    h.finish()
}

/// Timers, liveness checks, hot reload and the rate-limited push to windows
/// (≤ 10 updates/s per pet; attention goes out immediately).
async fn background(sh: server::Shared) {
    let mut tick = tokio::time::interval(Duration::from_millis(100));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut n: u64 = 0;
    let mut cfg_mtime = mtime(&sh.paths.config());
    let mut lib_print = library_fingerprint(&sh.roots);
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
        // Zombie check every 5 s: closing a waiting terminal clears its mini
        // well inside the 30 s the design allows.
        if n % 50 == 0 {
            let dead = sh.hub.lock().unwrap().reap(now, pid_alive);
            if dead > 0 {
                sh.paths.log(&format!("{dead} session(s) marked offline: agent process gone"));
            }
        }
        // Daily pet rotation (checked once a minute, applied once a day).
        if n % 600 == 300 {
            if let Some(r) = ops::rotate_daily(&sh, now) {
                sh.paths.log(&format!("daily rotation: {r}"));
            }
        }
        if n % 10 == 5 {
            let m = mtime(&sh.paths.config());
            if m != cfg_mtime {
                cfg_mtime = m;
                ops::reload_config(&sh);
            }
        }
        if n % 20 == 7 {
            let print = library_fingerprint(&sh.roots);
            if print != lib_print {
                lib_print = print;
                rescan_packs(&sh);
                sh.paths.log("pet packs reloaded");
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
    use windows_sys::Win32::System::Threading::{GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            // Access denied still means it exists; only "invalid parameter" means gone.
            return windows_sys::Win32::Foundation::GetLastError() == windows_sys::Win32::Foundation::ERROR_ACCESS_DENIED;
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
