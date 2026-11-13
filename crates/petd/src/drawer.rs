//! 收纳: the desktop and the Downloads folder, looked over for what has sat
//! untouched for weeks, and put away into one folder beside it on request.
//!
//! What it looks at: plain files of the user's own Desktop and Downloads
//! folders (not the public desktop, not inside folders). What it never
//! touches: folders, shortcuts, programs that are not installers, scripts,
//! hidden or system files, links, and anything changed in the last two weeks.
//!
//! What it does, and only when asked:
//! - stow: moves the old files into 「桌面收纳」 (or 「下载收纳」) by kind,
//!   screenshots also by month. Every move is written down first, so one
//!   "put back" returns each file to where it was.
//! - recycle: installers and byte-for-byte copies found by the scan go to the
//!   Recycle Bin (never deleted outright; Windows asks if one is too big for it).
//! - reveal: shows a file in Explorer.
//!
//! Every action re-scans and only acts on paths that scan offers, so nothing
//! from outside (a stale page, a typo) can move or recycle another file.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Unchanged this long: old.
pub const OLD_DAYS: u64 = 14;
/// An installer left this long is offered for the Recycle Bin.
const INSTALLER_DAYS: u64 = 7;
/// At least this big: a large file (shown, never removed by us).
const BIG: u64 = 500 << 20;
/// Copies are looked for only among files up to this size.
const COPY_MAX: u64 = 2 << 30;
const DAY_MS: u64 = 86_400_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    Desktop,
    Downloads,
}

impl Place {
    pub fn parse(s: &str) -> Option<Place> {
        match s {
            "desktop" | "desk" => Some(Place::Desktop),
            "downloads" | "download" => Some(Place::Downloads),
            _ => None,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Place::Desktop => "desktop",
            Place::Downloads => "downloads",
        }
    }

    /// The folder's name for what is put away in it.
    pub fn drawer(self) -> String {
        match self {
            Place::Desktop => crate::tr!("桌面收纳", "Desktop put-away"),
            Place::Downloads => crate::tr!("下载收纳", "Downloads put-away"),
        }
    }

    /// The user's own folder (where Windows keeps it, redirected or not).
    /// Debug builds can be pointed at a stand-in (`RILLBUDDY_DESKTOP_DIR`, `RILLBUDDY_DOWNLOADS_DIR`) for testing.
    pub fn dir(self) -> Option<PathBuf> {
        #[cfg(debug_assertions)]
        {
            let var = match self {
                Place::Desktop => "RILLBUDDY_DESKTOP_DIR",
                Place::Downloads => "RILLBUDDY_DOWNLOADS_DIR",
            };
            if let Some(d) = std::env::var_os(var) {
                return Some(PathBuf::from(d));
            }
        }
        known_folder(self)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct File {
    pub name: String,
    pub path: String,
    /// shot | doc | picture | media | archive | installer | other
    pub kind: &'static str,
    pub bytes: u64,
    /// Days since it was last changed (or landed here, whichever is later).
    pub days: u64,
    /// For a copy: the name of the file it copies.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub same: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stowed {
    pub id: String,
    pub place: String,
    pub at: u64,
    pub files: u32,
    pub bytes: u64,
    /// The folder they went into.
    pub drawer: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Disk {
    /// "C:"
    pub drive: String,
    pub free: u64,
    pub total: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub place: &'static str,
    pub dir: String,
    /// Icons the folder shows (files and folders, the public desktop's too for the desktop).
    pub icons: u32,
    pub files: u32,
    pub old: Vec<File>,
    pub installers: Vec<File>,
    pub copies: Vec<File>,
    pub big: Vec<File>,
    pub stowed: Vec<Stowed>,
    pub disk: Option<Disk>,
    pub at: u64,
}

/// One move, written down before it is made.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Move {
    from: String,
    to: String,
    bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Manifest {
    #[serde(flatten)]
    head: Stowed,
    moves: Vec<Move>,
}

// ---------------------------------------------------------------- what a file is

fn ext_of(name: &str) -> String {
    Path::new(name).extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

/// An .exe that installs something (by its name), as opposed to a program that runs from here.
fn installer_name(name: &str) -> bool {
    let n = name.to_lowercase();
    ["setup", "install", "安装", "update", "升级", "_x64", "-x64", "win64", "x86_64", "amd64", "_x86", "-x86"]
        .iter()
        .any(|k| n.contains(k))
}

fn screenshot_name(name: &str) -> bool {
    let n = name.to_lowercase();
    ["截图", "截屏", "屏幕截图", "screenshot", "screen shot", "snipaste", "微信图片", "qq截图", "wechat"]
        .iter()
        .any(|k| n.contains(k))
}

/// The kind of a plain file, or None when it is never put away (shortcuts, programs, scripts, settings).
pub fn kind_of(name: &str) -> Option<&'static str> {
    let ext = ext_of(name);
    Some(match ext.as_str() {
        "lnk" | "url" | "appref-ms" | "website" | "ini" | "bat" | "cmd" | "ps1" | "vbs" | "js" | "sys" | "dll" | "com" | "scr" => return None,
        "exe" => {
            if installer_name(name) {
                "installer"
            } else {
                return None;
            }
        }
        "msi" | "msix" | "msixbundle" | "appx" | "appxbundle" | "dmg" | "apk" | "pkg" => "installer",
        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "heic" | "tif" | "tiff" if screenshot_name(name) => "shot",
        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "heic" | "svg" | "ico" | "tif" | "tiff" | "psd" | "ai" | "fig" | "sketch" | "raw" | "cr2" | "nef" => "picture",
        "doc" | "docx" | "pdf" | "txt" | "md" | "rtf" | "odt" | "xls" | "xlsx" | "csv" | "ods" | "ppt" | "pptx" | "odp" | "key" | "pages" | "numbers" | "epub" | "xmind" | "wps" | "et" | "dps" | "mobi" | "ofd" => "doc",
        "mp4" | "mov" | "avi" | "mkv" | "wmv" | "webm" | "flv" | "mp3" | "wav" | "flac" | "m4a" | "aac" | "ogg" | "amr" => "media",
        "zip" | "rar" | "7z" | "tar" | "gz" | "tgz" | "bz2" | "xz" | "iso" | "zst" => "archive",
        _ => "other",
    })
}

/// The sub-folder in the drawer for a kind.
fn shelf(kind: &str) -> String {
    match kind {
        "shot" => crate::tr!("截图", "Screenshots"),
        "doc" => crate::tr!("文档", "Documents"),
        "picture" => crate::tr!("图片", "Pictures"),
        "media" => crate::tr!("音视频", "Audio & video"),
        "archive" => crate::tr!("压缩包", "Archives"),
        "installer" => crate::tr!("安装包", "Installers"),
        _ => crate::tr!("其他", "Other"),
    }
}

/// A file as found: path, name, size, the later of changed/created (ms), and whether reading it would fetch it from the cloud.
struct Found {
    path: PathBuf,
    name: String,
    bytes: u64,
    touched: u64,
    cloud: bool,
}

fn ms_of(t: std::io::Result<std::time::SystemTime>) -> u64 {
    t.ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Plain files directly in `dir` (no folders, no links, nothing hidden), and how many entries it shows.
fn list(dir: &Path) -> (Vec<Found>, u32) {
    let mut out = Vec::new();
    let mut shown = 0u32;
    let Ok(rd) = std::fs::read_dir(dir) else { return (out, 0) };
    for e in rd.flatten() {
        let Ok(md) = std::fs::symlink_metadata(e.path()) else { continue };
        let name = e.file_name().to_string_lossy().into_owned();
        let (hidden, link, cloud) = attrs(&md);
        if hidden || name.eq_ignore_ascii_case("desktop.ini") {
            continue;
        }
        shown += 1;
        if link || !md.is_file() {
            continue;
        }
        out.push(Found {
            path: e.path(),
            name,
            bytes: md.len(),
            touched: ms_of(md.modified()).max(ms_of(md.created())),
            cloud,
        });
    }
    (out, shown)
}

/// (hidden or system, a link or junction, a cloud placeholder not on this disk)
#[cfg(windows)]
fn attrs(md: &std::fs::Metadata) -> (bool, bool, bool) {
    use std::os::windows::fs::MetadataExt;
    let a = md.file_attributes();
    const HIDDEN: u32 = 0x2;
    const SYSTEM: u32 = 0x4;
    const REPARSE: u32 = 0x400;
    const OFFLINE: u32 = 0x1000;
    const RECALL_ON_OPEN: u32 = 0x40000;
    const RECALL_ON_DATA_ACCESS: u32 = 0x400000;
    (a & (HIDDEN | SYSTEM) != 0, a & REPARSE != 0 || md.file_type().is_symlink(), a & (OFFLINE | RECALL_ON_OPEN | RECALL_ON_DATA_ACCESS) != 0)
}

#[cfg(not(windows))]
fn attrs(md: &std::fs::Metadata) -> (bool, bool, bool) {
    (false, md.file_type().is_symlink(), false)
}

/// Byte for byte the same.
fn same_bytes(a: &Path, b: &Path) -> bool {
    use std::io::Read;
    let (Ok(fa), Ok(fb)) = (std::fs::File::open(a), std::fs::File::open(b)) else { return false };
    let (mut ra, mut rb) = (std::io::BufReader::with_capacity(1 << 16, fa), std::io::BufReader::with_capacity(1 << 16, fb));
    let (mut ba, mut bb) = (vec![0u8; 1 << 16], vec![0u8; 1 << 16]);
    loop {
        let na = match read_full(&mut ra, &mut ba) {
            Ok(n) => n,
            Err(_) => return false,
        };
        let nb = match read_full(&mut rb, &mut bb) {
            Ok(n) => n,
            Err(_) => return false,
        };
        if na != nb || ba[..na] != bb[..nb] {
            return false;
        }
        if na == 0 {
            return true;
        }
    }
    fn read_full(r: &mut impl Read, buf: &mut [u8]) -> std::io::Result<usize> {
        let mut n = 0;
        while n < buf.len() {
            match r.read(&mut buf[n..])? {
                0 => break,
                k => n += k,
            }
        }
        Ok(n)
    }
}

// ---------------------------------------------------------------- the scan

/// Look the folder over. `home`: Rill's data folder (where the put-aways are written down); `now` in ms.
pub fn scan(home: &Path, place: Place, dir: &Path, now: u64) -> Report {
    let (found, mut icons) = list(dir);
    if place == Place::Desktop {
        if let Some(public) = public_desktop() {
            icons += list(&public).1;
        }
    }
    let days = |f: &Found| now.saturating_sub(f.touched) / DAY_MS;
    let file = |f: &Found, kind: &'static str, same: Option<String>| File {
        name: f.name.clone(),
        path: f.path.to_string_lossy().into_owned(),
        kind,
        bytes: f.bytes,
        days: days(f),
        same,
    };
    let mut old = Vec::new();
    let mut installers = Vec::new();
    let keep = kept(home);
    for f in &found {
        let Some(kind) = kind_of(&f.name) else { continue };
        if days(f) >= OLD_DAYS && !keep.contains(&key(&f.path.to_string_lossy())) {
            old.push(file(f, kind, None));
        }
        if kind == "installer" && days(f) >= INSTALLER_DAYS {
            installers.push(file(f, kind, None));
        }
    }
    // Copies: same size, then the same bytes. The original is the one that came first (then the shorter name).
    let mut by_size: std::collections::BTreeMap<u64, Vec<&Found>> = Default::default();
    for f in &found {
        if f.bytes > 0 && f.bytes <= COPY_MAX && !f.cloud && kind_of(&f.name).is_some() {
            by_size.entry(f.bytes).or_default().push(f);
        }
    }
    let mut copies = Vec::new();
    for group in by_size.values().filter(|g| g.len() >= 2 && g.len() <= 12) {
        let mut g = group.clone();
        g.sort_by(|a, b| a.touched.cmp(&b.touched).then(a.name.chars().count().cmp(&b.name.chars().count())).then(a.name.cmp(&b.name)));
        let mut taken = vec![false; g.len()];
        for i in 0..g.len() {
            if taken[i] {
                continue;
            }
            for j in i + 1..g.len() {
                if !taken[j] && same_bytes(&g[i].path, &g[j].path) {
                    taken[j] = true;
                    copies.push(file(g[j], kind_of(&g[j].name).unwrap_or("other"), Some(g[i].name.clone())));
                }
            }
        }
    }
    let gone: std::collections::HashSet<&str> = installers.iter().chain(&copies).map(|f| f.path.as_str()).collect();
    let mut big: Vec<File> = found
        .iter()
        .filter(|f| f.bytes >= BIG && !gone.contains(f.path.to_string_lossy().as_ref()))
        .map(|f| file(f, kind_of(&f.name).unwrap_or("other"), None))
        .collect();
    big.sort_by(|a, b| b.bytes.cmp(&a.bytes));
    old.sort_by(|a, b| b.days.cmp(&a.days).then(a.name.cmp(&b.name)));
    installers.sort_by(|a, b| b.days.cmp(&a.days));
    Report {
        place: place.id(),
        dir: dir.to_string_lossy().into_owned(),
        icons,
        files: found.iter().filter(|f| kind_of(&f.name).is_some()).count() as u32,
        old,
        installers,
        copies,
        big,
        stowed: stowed(home, place),
        disk: disk(dir),
        at: now,
    }
}

/// The folder as it is now, at its usual place; None where there is no such folder.
pub fn look(home: &Path, place: Place) -> Option<Report> {
    let dir = place.dir()?;
    dir.is_dir().then(|| scan(home, place, &dir, crate::now_ms()))
}

// ---------------------------------------------------------------- put away, put back

fn book(home: &Path) -> PathBuf {
    home.join("drawer")
}

/// Files you put back: they stay where you want them, and are never put away again.
fn keep_file(home: &Path) -> PathBuf {
    home.join("drawer-keep.json")
}

pub fn kept(home: &Path) -> std::collections::HashSet<String> {
    std::fs::read(keep_file(home)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn keep(home: &Path, paths: &[String]) {
    let mut k = kept(home);
    k.extend(paths.iter().map(|p| key(p)));
    let _ = std::fs::write(keep_file(home), serde_json::to_vec(&k).unwrap_or_default());
}

/// Put-aways of `place` that can still be put back, latest first.
pub fn stowed(home: &Path, place: Place) -> Vec<Stowed> {
    let mut out: Vec<Stowed> = std::fs::read_dir(book(home))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| std::fs::read(e.path()).ok())
        .filter_map(|b| serde_json::from_slice::<Manifest>(&b).ok())
        .map(|m| m.head)
        .filter(|s| s.place == place.id())
        .collect();
    out.sort_by(|a, b| b.at.cmp(&a.at));
    out
}

fn key(p: &str) -> String {
    if cfg!(windows) {
        p.replace('/', "\\").to_lowercase()
    } else {
        p.to_string()
    }
}

/// A free name for `name` in `dir`: "a.png", then "a (2).png", …
fn free_in(dir: &Path, name: &str) -> PathBuf {
    let first = dir.join(name);
    if !first.exists() {
        return first;
    }
    let p = Path::new(name);
    let stem = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let ext = p.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    (2..10_000).map(|n| dir.join(format!("{stem} ({n}){ext}"))).find(|p| !p.exists()).unwrap_or(first)
}

/// Make the drawer (the folder appears on the desktop before the files fly into it). Its path.
pub fn prepare(place: Place, dir: &Path) -> Result<PathBuf, String> {
    let d = dir.join(place.drawer());
    std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
    Ok(d)
}

/// What a put-away did.
#[derive(Debug, Serialize)]
pub struct Done {
    pub stowed: Option<Stowed>,
    pub moved: u32,
    /// Offered but not moved (open in a program, gone meanwhile).
    pub skipped: u32,
}

/// Put the folder's old files away. `only`: just these of them (the icons the
/// pet swept into the folder); None: all of them.
pub fn stow(home: &Path, place: Place, dir: &Path, only: Option<&[String]>, now: u64) -> Result<Done, String> {
    let r = scan(home, place, dir, now);
    let wanted: Option<std::collections::HashSet<String>> = only.map(|o| o.iter().map(|p| key(p)).collect());
    let files: Vec<&File> = r.old.iter().filter(|f| wanted.as_ref().is_none_or(|w| w.contains(&key(&f.path)))).collect();
    if files.is_empty() {
        return Ok(Done { stowed: None, moved: 0, skipped: 0 });
    }
    let drawer = prepare(place, dir)?;
    let id = format!("{}-{now}", place.id());
    std::fs::create_dir_all(book(home)).map_err(|e| e.to_string())?;
    let book_file = book(home).join(format!("{id}.json"));
    let mut m = Manifest {
        head: Stowed { id, place: place.id().into(), at: now, files: 0, bytes: 0, drawer: drawer.to_string_lossy().into_owned() },
        moves: Vec::new(),
    };
    let mut skipped = 0;
    for f in files {
        let mut sub = drawer.join(shelf(f.kind));
        if f.kind == "shot" {
            let t = std::fs::metadata(&f.path).and_then(|m| m.modified()).ok().map(chrono::DateTime::<chrono::Local>::from);
            if let Some(t) = t {
                sub = sub.join(t.format("%Y-%m").to_string());
            }
        }
        if std::fs::create_dir_all(&sub).is_err() {
            skipped += 1;
            continue;
        }
        let to = free_in(&sub, &f.name);
        // Written down before it moves: a crash in between leaves a note that points at the file either way.
        m.moves.push(Move { from: f.path.clone(), to: to.to_string_lossy().into_owned(), bytes: f.bytes });
        write(&book_file, &m)?;
        if std::fs::rename(&f.path, &to).is_ok() {
            m.head.files += 1;
            m.head.bytes += f.bytes;
        } else {
            m.moves.pop();
            skipped += 1;
        }
    }
    if m.moves.is_empty() {
        let _ = std::fs::remove_file(&book_file);
        prune(&drawer);
        return Ok(Done { stowed: None, moved: 0, skipped });
    }
    write(&book_file, &m)?;
    Ok(Done { moved: m.head.files, stowed: Some(m.head), skipped })
}

fn write(path: &Path, m: &Manifest) -> Result<(), String> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(m).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

/// Empty folders left in the drawer go, and the drawer too once it is empty.
fn prune(drawer: &Path) {
    fn walk(d: &Path) {
        if let Ok(rd) = std::fs::read_dir(d) {
            for e in rd.flatten() {
                if e.file_type().is_ok_and(|t| t.is_dir()) {
                    walk(&e.path());
                    let _ = std::fs::remove_dir(e.path());
                }
            }
        }
    }
    walk(drawer);
    let _ = std::fs::remove_dir(drawer);
}

/// Put back what one put-away moved. How many went back, and how many could not
/// (a file of the same name is there again, or the put-away file was moved on).
pub fn unstow(home: &Path, id: &str) -> Result<(u32, u32), String> {
    if id.is_empty() || id.contains(['/', '\\', '.']) {
        return Err("bad id".into());
    }
    let path = book(home).join(format!("{id}.json"));
    let mut m: Manifest = serde_json::from_slice(&std::fs::read(&path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let (mut back, mut stuck) = (0u32, 0u32);
    let mut left = Vec::new();
    let mut home_again = Vec::new();
    for mv in std::mem::take(&mut m.moves) {
        let (from, to) = (Path::new(&mv.from), Path::new(&mv.to));
        if !to.exists() {
            // Gone from the drawer (you moved or deleted it): nothing to bring back.
            continue;
        }
        if from.exists() || std::fs::rename(to, from).is_err() {
            stuck += 1;
            left.push(mv);
        } else {
            back += 1;
            home_again.push(mv.from.clone());
        }
    }
    keep(home, &home_again);
    prune(Path::new(&m.head.drawer));
    if left.is_empty() {
        let _ = std::fs::remove_file(&path);
    } else {
        m.head.files = left.len() as u32;
        m.head.bytes = left.iter().map(|l| l.bytes).sum();
        m.moves = left;
        write(&path, &m)?;
    }
    Ok((back, stuck))
}

/// Installers and copies the scan offers go to the Recycle Bin; others asked for are ignored. How many went.
pub fn recycle(home: &Path, place: Place, dir: &Path, paths: &[String], now: u64) -> Result<u32, String> {
    let r = scan(home, place, dir, now);
    let allowed: std::collections::HashSet<String> = r.installers.iter().chain(&r.copies).map(|f| key(&f.path)).collect();
    let go: Vec<String> = r
        .installers
        .iter()
        .chain(&r.copies)
        .filter(|f| paths.iter().any(|p| key(p) == key(&f.path)))
        .map(|f| f.path.clone())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .filter(|p| allowed.contains(&key(p)))
        .collect();
    if go.is_empty() {
        return Ok(0);
    }
    to_recycle_bin(&go)?;
    Ok(go.iter().filter(|p| !Path::new(p).exists()).count() as u32)
}

/// Show a file the scan found in Explorer, selected.
pub fn reveal(home: &Path, place: Place, dir: &Path, path: &str, now: u64) -> bool {
    let r = scan(home, place, dir, now);
    let ok = r.old.iter().chain(&r.installers).chain(&r.copies).chain(&r.big).any(|f| key(&f.path) == key(path));
    ok && show_in_explorer(Path::new(path))
}

// ---------------------------------------------------------------- Windows

#[cfg(windows)]
fn known_folder(place: Place) -> Option<PathBuf> {
    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::UI::Shell::{SHGetKnownFolderPath, FOLDERID_Desktop, FOLDERID_Downloads};
    let id = match place {
        Place::Desktop => &FOLDERID_Desktop,
        Place::Downloads => &FOLDERID_Downloads,
    };
    unsafe {
        let mut p: *mut u16 = std::ptr::null_mut();
        let hr = SHGetKnownFolderPath(id, 0, std::ptr::null_mut(), &mut p);
        if hr != 0 || p.is_null() {
            if !p.is_null() {
                CoTaskMemFree(p as _);
            }
            return None;
        }
        let n = (0..).take_while(|i| *p.add(*i) != 0).count();
        let s = String::from_utf16_lossy(std::slice::from_raw_parts(p, n));
        CoTaskMemFree(p as _);
        Some(PathBuf::from(s))
    }
}

#[cfg(not(windows))]
fn known_folder(place: Place) -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from)?;
    Some(home.join(match place {
        Place::Desktop => "Desktop",
        Place::Downloads => "Downloads",
    }))
}

#[cfg(windows)]
fn public_desktop() -> Option<PathBuf> {
    std::env::var_os("PUBLIC").map(|p| PathBuf::from(p).join("Desktop"))
}

#[cfg(not(windows))]
fn public_desktop() -> Option<PathBuf> {
    None
}

/// Free and total space of the drive `dir` is on.
#[cfg(windows)]
pub fn disk(dir: &Path) -> Option<Disk> {
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    let s = dir.to_string_lossy();
    let drive: String = s.chars().take(2).collect();
    let root: Vec<u16> = format!("{drive}\\").encode_utf16().chain([0]).collect();
    let (mut free, mut total) = (0u64, 0u64);
    let ok = unsafe { GetDiskFreeSpaceExW(root.as_ptr(), &mut free, &mut total, std::ptr::null_mut()) } != 0;
    (ok && total > 0).then(|| Disk { drive: drive.to_uppercase(), free, total })
}

#[cfg(not(windows))]
pub fn disk(_dir: &Path) -> Option<Disk> {
    None
}

/// The drive Windows is on is running short: under a tenth free, or under 10 GB.
pub fn short(d: &Disk) -> bool {
    d.free * 10 < d.total || d.free < 10 << 30
}

#[cfg(windows)]
fn to_recycle_bin(paths: &[String]) -> Result<(), String> {
    use windows_sys::Win32::UI::Shell::{SHFileOperationW, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT, FOF_WANTNUKEWARNING, FO_DELETE, SHFILEOPSTRUCTW};
    // A list of paths, each ending in a nul, the list in another.
    let mut from: Vec<u16> = Vec::new();
    for p in paths {
        from.extend(p.encode_utf16());
        from.push(0);
    }
    from.push(0);
    let mut op: SHFILEOPSTRUCTW = unsafe { std::mem::zeroed() };
    op.wFunc = FO_DELETE;
    op.pFrom = from.as_ptr();
    // Into the Recycle Bin, never past it: one too big for the bin makes Windows ask first.
    op.fFlags = (FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_SILENT | FOF_NOERRORUI | FOF_WANTNUKEWARNING) as u16;
    let r = unsafe { SHFileOperationW(&mut op) };
    if r != 0 {
        return Err(format!("SHFileOperation {r:#x}"));
    }
    Ok(())
}

#[cfg(not(windows))]
fn to_recycle_bin(_paths: &[String]) -> Result<(), String> {
    Err("not on this system".into())
}

#[cfg(windows)]
fn show_in_explorer(p: &Path) -> bool {
    use std::os::windows::process::CommandExt;
    std::process::Command::new("explorer.exe")
        .raw_arg(format!("/select,\"{}\"", p.display()))
        .spawn()
        .is_ok()
}

#[cfg(not(windows))]
fn show_in_explorer(_p: &Path) -> bool {
    false
}

// ---------------------------------------------------------------- the pets' part

/// A line in the diary for a put-away.
pub fn noted(sh: &crate::server::Shared, place: Place, n: u32) {
    let text = match place {
        Place::Desktop => crate::tr!("桌面上 {n} 个很久没碰的文件，收进了「桌面收纳」。", "{n} files left untouched on the desktop went into the put-away folder."),
        Place::Downloads => crate::tr!("下载文件夹里 {n} 个很久没碰的文件，收进了「下载收纳」。", "{n} files left untouched in Downloads went into the put-away folder."),
    };
    sh.db.log_event(crate::now_ms(), "noticed", &text, 1);
}

/// The first pet that is up, and which of the five it is; None in focus mode,
/// when they keep quiet, outside waking hours, or when none is free.
pub fn asker(sh: &crate::server::Shared) -> Option<(String, &'static str)> {
    use chrono::Timelike;
    if !(9..23).contains(&chrono::Local::now().hour()) {
        return None;
    }
    let hub = sh.hub.lock().unwrap();
    if hub.cfg.features.focus_mode || hub.cfg.features.chatter == "quiet" {
        return None;
    }
    use crate::session::State;
    let pets = hub.pets();
    let p = pets.iter().find(|v| !matches!(v.state, State::Offline | State::Sleeping | State::Attention))?;
    Some((p.id.clone(), crate::cast::by_id(&p.skin).map(|f| f.id).unwrap_or("zimeng")))
}

fn asked_within(sh: &crate::server::Shared, key: &str, days: u64) -> bool {
    sh.db.kv_get(key).and_then(|v| v.parse::<u64>().ok()).is_some_and(|t| crate::now_ms().saturating_sub(t) < days * DAY_MS)
}

fn ask(sh: &crate::server::Shared, pet: &str, text: String, why: &str, offer: &str) {
    let _ = sh.ws_tx.send(crate::msg("pet.act", &serde_json::json!({ "pet": pet, "act": "offer", "text": text, "why": why, "offer": offer })));
}

/// The desktop is nearly covered (`icons` of `cells` taken) while you look at it,
/// and much of it is old: a pet offers to put the old files away. Once a week at most.
pub fn offer_full(sh: &crate::server::Shared, icons: u32, cells: u32) -> bool {
    if cells == 0 || icons * 10 < cells * 8 || asked_within(sh, "drawer:desk:asked", 7) {
        return false;
    }
    let Some(r) = look(&sh.paths.home, Place::Desktop) else { return false };
    if r.old.len() < 8 {
        return false;
    }
    let Some((pet, who)) = asker(sh) else { return false };
    sh.db.kv_set("drawer:desk:asked", &crate::now_ms().to_string());
    ask(sh, &pet, crate::voice::line(who, "offer_stow", 0), "stow", "stow");
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put(dir: &Path, name: &str, body: &[u8]) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, body).unwrap();
        p
    }

    /// Created times can't be set back, so "now" in these tests is weeks ahead of today.
    fn later() -> u64 {
        crate::now_ms() + 40 * DAY_MS
    }

    #[test]
    fn kinds_and_what_never_moves() {
        assert_eq!(kind_of("屏幕截图 2026-03-02.png"), Some("shot"));
        assert_eq!(kind_of("Screenshot 2026-03-02 101010.png"), Some("shot"));
        assert_eq!(kind_of("海报.png"), Some("picture"));
        assert_eq!(kind_of("周报.docx"), Some("doc"));
        assert_eq!(kind_of("Git-2.45-64-bit-setup.exe"), Some("installer"));
        assert_eq!(kind_of("setup_x64.msi"), Some("installer"));
        assert_eq!(kind_of("小工具.exe"), None, "a program that runs from here stays");
        assert_eq!(kind_of("微信.lnk"), None);
        assert_eq!(kind_of("run.bat"), None);
        assert_eq!(kind_of("desktop.ini"), None);
        assert_eq!(kind_of("素材.zip"), Some("archive"));
    }

    #[test]
    fn the_scan_finds_old_files_installers_and_copies() {
        let home = tempfile::tempdir().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        put(d, "周报.docx", b"week");
        put(d, "报价单.xlsx", b"price-list");
        put(d, "报价单 (1).xlsx", b"price-list");
        put(d, "安装包 setup.exe", b"MZ....");
        put(d, "工具.exe", b"MZ..");
        put(d, "快捷.lnk", b"L");
        std::fs::create_dir(d.join("项目")).unwrap();
        let r = scan(home.path(), Place::Downloads, d, later());
        let names = |v: &[File]| v.iter().map(|f| f.name.clone()).collect::<Vec<_>>();
        let mut old = names(&r.old);
        old.sort();
        assert_eq!(old, ["周报.docx", "安装包 setup.exe", "报价单 (1).xlsx", "报价单.xlsx"]);
        assert_eq!(names(&r.installers), ["安装包 setup.exe"]);
        assert_eq!(r.copies.len(), 1, "{:?}", r.copies);
        assert_eq!(r.copies[0].same.as_deref().map(|s| s.contains("报价单")), Some(true));
        assert_eq!(r.icons, 7, "every entry shows");
        // Nothing is old yet today.
        assert!(scan(home.path(), Place::Downloads, d, crate::now_ms()).old.is_empty());
    }

    #[test]
    fn put_away_and_put_back() {
        let home = tempfile::tempdir().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        put(d, "屏幕截图 1.png", b"png");
        put(d, "周报.docx", b"week");
        put(d, "工具.exe", b"MZ");
        let now = later();
        let done = stow(home.path(), Place::Desktop, d, None, now).unwrap();
        assert_eq!((done.moved, done.skipped), (2, 0));
        assert!(!d.join("周报.docx").exists() && !d.join("屏幕截图 1.png").exists());
        assert!(d.join("工具.exe").exists(), "programs stay");
        let drawer = d.join(Place::Desktop.drawer());
        assert!(drawer.join(shelf("doc")).join("周报.docx").exists());
        let s = stowed(home.path(), Place::Desktop);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].files, 2);
        // Something new took one name meanwhile: that one stays in the drawer, the rest go back.
        put(d, "周报.docx", b"new week");
        let (back, stuck) = unstow(home.path(), &s[0].id).unwrap();
        assert_eq!((back, stuck), (1, 1));
        assert!(d.join("屏幕截图 1.png").exists());
        assert_eq!(std::fs::read(d.join("周报.docx")).unwrap(), b"new week");
        assert_eq!(stowed(home.path(), Place::Desktop)[0].files, 1);
        // Once the name is free, the last one goes back too, and the drawer is gone.
        std::fs::remove_file(d.join("周报.docx")).unwrap();
        assert_eq!(unstow(home.path(), &s[0].id).unwrap(), (1, 0));
        assert_eq!(std::fs::read(d.join("周报.docx")).unwrap(), b"week");
        assert!(!drawer.exists());
        assert!(stowed(home.path(), Place::Desktop).is_empty());
        // What you put back stays where you put it: it is never offered again.
        let r = scan(home.path(), Place::Desktop, d, now);
        assert!(r.old.is_empty(), "{:?}", r.old.iter().map(|f| &f.name).collect::<Vec<_>>());
    }

    #[test]
    fn only_what_the_scan_offers_is_acted_on() {
        let home = tempfile::tempdir().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        put(d, "周报.docx", b"week");
        put(d, "新的.docx", b"new");
        let now = later();
        // Asked for a file outside the folder: it does not move.
        let outside = tempfile::tempdir().unwrap();
        let o = put(outside.path(), "别处.docx", b"x");
        let done = stow(home.path(), Place::Desktop, d, Some(&[o.to_string_lossy().into_owned()]), now).unwrap();
        assert_eq!(done.moved, 0);
        assert!(o.exists());
        // Recycling only takes installers and copies the scan found.
        assert_eq!(recycle(home.path(), Place::Desktop, d, &[d.join("周报.docx").to_string_lossy().into_owned()], now).unwrap(), 0);
        assert!(d.join("周报.docx").exists());
        assert!(unstow(home.path(), "../x").is_err());
    }
}
