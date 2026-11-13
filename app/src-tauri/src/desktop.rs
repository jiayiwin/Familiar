//! The desktop's icons, for the pets' sweep: where they are, where they go,
//! moving them a little at a time while a pet sweeps, and putting them back.
//!
//! It goes through the shell's own view of the desktop (IFolderView, as any
//! program that arranges icons does): no files are touched, only where their
//! icons sit. Before a sweep the old places are kept, so one "put them back"
//! undoes it. Coordinates out of here are physical screen pixels.
//!
//! COM objects live on one thread of their own; everything else asks it.

use petd::tidy::{Area, Kind};
use serde::Serialize;

#[derive(Serialize, Clone, Debug)]
pub struct Item {
    pub key: String,
    pub kind: Kind,
    /// Where its cell is now, and where it goes.
    pub x: i32,
    pub y: i32,
    pub tx: i32,
    pub ty: i32,
    /// It goes into the put-away folder (tx, ty is the folder's cell) and leaves the desktop there.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub stow: bool,
}

#[derive(Serialize, Debug)]
pub struct Plan {
    pub items: Vec<Item>,
    /// One icon's cell.
    pub cw: i32,
    pub ch: i32,
    pub areas: Vec<Area>,
    /// Strays by the same count the offer used.
    pub strays: u32,
}

#[cfg(windows)]
pub use win::{begin, end, fullness, in_front, moves, plan, stow_plan, strays, undo};

#[cfg(not(windows))]
mod none {
    use super::Plan;
    pub fn plan() -> Option<Plan> {
        None
    }
    pub fn strays() -> Option<u32> {
        None
    }
    pub fn fullness() -> Option<(u32, u32)> {
        None
    }
    pub fn stow_plan(_paths: Vec<String>, _drawer: String) -> Option<Plan> {
        None
    }
    pub fn begin() -> bool {
        false
    }
    pub fn moves(_m: Vec<(String, i32, i32)>) {}
    pub fn end() {}
    pub fn undo() -> u32 {
        0
    }
    pub fn in_front() -> bool {
        false
    }
}
#[cfg(not(windows))]
pub use none::{begin, end, fullness, in_front, moves, plan, stow_plan, strays, undo};

/// While the desktop is in front of you: count the strays every moment (a
/// read takes ~20 ms), and once the count holds still (you have stopped
/// moving things about) let the first pet that is up offer to sweep. petd
/// decides whether it may; only an offer made counts as today's.
pub fn watch(app: std::sync::Arc<crate::App>) {
    std::thread::spawn(move || {
        use std::time::{Duration, Instant};
        let mut since: Option<Instant> = None;
        let mut last: Option<u32> = None;
        loop {
            std::thread::sleep(Duration::from_millis(1500));
            if !in_front() {
                since = None;
                last = None;
                continue;
            }
            let t = *since.get_or_insert_with(Instant::now);
            if t.elapsed() < Duration::from_secs(2) {
                continue;
            }
            let sh = &app.petd.shared;
            if sh.db.kv_get("tidy:asked").as_deref() == Some(petd::db::day_of(petd::now_ms()).as_str()) || !crate::windows::visible(&app) {
                continue;
            }
            let Some(n) = strays() else { continue };
            petd::tidy::saw(n);
            if last == Some(n) {
                // Nearly covered, much of it old: put away first (once a week); else just sweep.
                let full = fullness().is_some_and(|(icons, cells)| petd::drawer::offer_full(sh, icons, cells));
                if !full {
                    petd::tidy::offer(sh, n);
                }
            }
            last = Some(n);
        }
    });
}

#[cfg(windows)]
mod win {
    use super::{Item, Plan};
    use petd::tidy::{self, Area, Grid, Icon};
    use std::sync::mpsc;
    use std::sync::{Mutex, OnceLock};
    use std::time::Duration;
    use ::windows::core::{Interface, PWSTR};
    use ::windows::Win32::Foundation::{HWND, POINT, S_OK};
    use ::windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoTaskMemFree, IServiceProvider, CLSCTX_ALL, COINIT_APARTMENTTHREADED};
    use ::windows::Win32::System::SystemServices::{SFGAO_FOLDER, SFGAO_LINK};
    use ::windows::Win32::System::Variant::{VARIANT, VT_I4};
    use ::windows::Win32::UI::Shell::Common::{ITEMIDLIST, STRRET};
    use ::windows::Win32::UI::Shell::{
        IEnumIDList, IFolderView, IFolderView2, IShellBrowser, IShellFolder, IShellWindows, ShellWindows, StrRetToStrW, CSIDL_DESKTOP, FWF_AUTOARRANGE, FWF_SNAPTOGRID, SHGDN_FORPARSING,
        SID_STopLevelBrowser, SVGIO_ALLVIEW, SVSI_POSITIONITEM, SWC_DESKTOP, SWFO_NEEDDISPATCH,
    };

    /// One icon's item id, freed when dropped.
    struct Pidl(*mut ITEMIDLIST);
    impl Drop for Pidl {
        fn drop(&mut self) {
            unsafe { CoTaskMemFree(Some(self.0 as *const _)) };
        }
    }

    struct View {
        fv: IFolderView,
        fv2: Option<IFolderView2>,
        /// Screen position of the icon list's client origin.
        off: (i32, i32),
        items: Vec<(String, Pidl, bool, bool)>,
    }

    #[derive(Default)]
    struct Ctx {
        view: Option<View>,
        /// Where every icon stood before the last sweep (list coordinates), and the folder flags then.
        saved: Vec<(String, POINT)>,
        flags: Option<u32>,
    }

    type Job = Box<dyn FnOnce(&mut Ctx) + Send>;

    fn worker() -> &'static Mutex<mpsc::Sender<Job>> {
        static W: OnceLock<Mutex<mpsc::Sender<Job>>> = OnceLock::new();
        W.get_or_init(|| {
            let (tx, rx) = mpsc::channel::<Job>();
            std::thread::spawn(move || {
                unsafe {
                    let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok();
                }
                let mut ctx = Ctx::default();
                for job in rx {
                    job(&mut ctx);
                }
            });
            Mutex::new(tx)
        })
    }

    fn run<R: Send + 'static>(f: impl FnOnce(&mut Ctx) -> R + Send + 'static) -> Option<R> {
        let (tx, rx) = mpsc::channel();
        worker().lock().ok()?.send(Box::new(move |c| {
            let _ = tx.send(f(c));
        })).ok()?;
        rx.recv_timeout(Duration::from_secs(6)).ok()
    }

    /// The desktop's folder view, its icon list's place on the screen, and its icons.
    unsafe fn open() -> ::windows::core::Result<Option<View>> {
        use windows_sys::Win32::Graphics::Gdi::ClientToScreen;
        use windows_sys::Win32::UI::WindowsAndMessaging::{FindWindowExW, IsWindowVisible};
        let sw: IShellWindows = CoCreateInstance(&ShellWindows, None, CLSCTX_ALL)?;
        let mut loc = VARIANT::default();
        (*loc.Anonymous.Anonymous).vt = VT_I4;
        (*loc.Anonymous.Anonymous).Anonymous.lVal = CSIDL_DESKTOP as i32;
        let root = VARIANT::default();
        let mut hwnd = 0i32;
        let disp = sw.FindWindowSW(&loc, &root, SWC_DESKTOP, &mut hwnd, SWFO_NEEDDISPATCH)?;
        let sp: IServiceProvider = disp.cast()?;
        let browser: IShellBrowser = sp.QueryService(&SID_STopLevelBrowser)?;
        let view = browser.QueryActiveShellView()?;
        let fv: IFolderView = view.cast()?;
        let fv2: Option<IFolderView2> = view.cast().ok();
        // The icon list inside the desktop's view: hidden when "show desktop icons" is off.
        let defview: HWND = view.GetWindow()?;
        let class: Vec<u16> = "SysListView32\0".encode_utf16().collect();
        let lv = FindWindowExW(defview.0 as _, std::ptr::null_mut(), class.as_ptr(), std::ptr::null());
        if lv.is_null() || IsWindowVisible(lv) == 0 {
            return Ok(None);
        }
        let mut o = windows_sys::Win32::Foundation::POINT { x: 0, y: 0 };
        ClientToScreen(lv, &mut o);
        let folder: IShellFolder = fv.GetFolder()?;
        let en: IEnumIDList = fv.Items(SVGIO_ALLVIEW)?;
        let mut items = Vec::new();
        loop {
            let mut one = [std::ptr::null_mut::<ITEMIDLIST>(); 1];
            let mut got = 0u32;
            if en.Next(&mut one, Some(&mut got)) != S_OK || got == 0 {
                break;
            }
            let pidl = Pidl(one[0]);
            let mut sr: STRRET = std::mem::zeroed();
            if folder.GetDisplayNameOf(pidl.0, SHGDN_FORPARSING, &mut sr).is_err() {
                continue;
            }
            let mut out = PWSTR::null();
            if StrRetToStrW(&mut sr, Some(pidl.0), &mut out).is_err() {
                continue;
            }
            let key = out.to_string().unwrap_or_default();
            CoTaskMemFree(Some(out.0 as *const _));
            let mut attrs = SFGAO_FOLDER.0 | SFGAO_LINK.0;
            let _ = folder.GetAttributesOf(&[pidl.0 as *const _], &mut attrs);
            items.push((key, pidl, attrs & SFGAO_FOLDER.0 != 0, attrs & SFGAO_LINK.0 != 0));
        }
        Ok(Some(View { fv, fv2, off: (o.x, o.y), items }))
    }

    /// Each monitor's work area, physical px.
    fn areas() -> Vec<Area> {
        use windows_sys::Win32::Foundation::{LPARAM, RECT};
        use windows_sys::Win32::Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO};
        unsafe extern "system" fn one(m: HMONITOR, _: HDC, _: *mut RECT, out: LPARAM) -> i32 {
            let mut info: MONITORINFO = std::mem::zeroed();
            info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
            if GetMonitorInfoW(m, &mut info) != 0 {
                let r = info.rcWork;
                (*(out as *mut Vec<Area>)).push(Area { l: r.left, t: r.top, r: r.right, b: r.bottom });
            }
            1
        }
        let mut out: Vec<Area> = Vec::new();
        unsafe { EnumDisplayMonitors(std::ptr::null_mut(), std::ptr::null(), Some(one), &mut out as *mut Vec<Area> as LPARAM) };
        out
    }

    /// The desktop as read: icons (screen px) and their keys, the grid, the screens, and whether Windows arranges it itself.
    struct Seen {
        icons: Vec<Icon>,
        keys: Vec<String>,
        grid: Grid,
        areas: Vec<Area>,
        arranged: bool,
    }

    /// Read the desktop afresh.
    fn look(c: &mut Ctx) -> Option<Seen> {
        let v = unsafe { open() }.ok().flatten()?;
        let mut sp = POINT::default();
        unsafe {
            if v.fv.GetSpacing(&mut sp).is_err() || sp.x <= 0 {
                sp = v.fv.GetDefaultSpacing().ok()?;
            }
        }
        let arranged = v.fv2.as_ref().and_then(|f| unsafe { f.GetCurrentFolderFlags() }.ok()).is_some_and(|f| f & FWF_AUTOARRANGE.0 as u32 != 0);
        let mut icons = Vec::new();
        let mut keys = Vec::new();
        for (key, pidl, folder, link) in &v.items {
            let Ok(p) = (unsafe { v.fv.GetItemPosition(pidl.0) }) else { continue };
            let name = std::path::Path::new(key).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| key.clone());
            icons.push(Icon { x: p.x + v.off.0, y: p.y + v.off.1, kind: tidy::kind_of(key, *folder, *link), name });
            keys.push(key.clone());
        }
        let areas = areas();
        let grid = Grid::from_icons(&icons, &areas, sp.x, sp.y);
        c.view = Some(v);
        Some(Seen { icons, keys, grid, areas, arranged })
    }

    pub fn plan() -> Option<Plan> {
        run(|c| {
            let Seen { icons, keys, grid: g, areas, arranged } = look(c)?;
            if arranged || icons.is_empty() {
                return None;
            }
            let to = tidy::layout(&icons, &areas, g);
            let strays = tidy::strays(&icons, &areas, g);
            let items = icons.iter().zip(keys).zip(&to).map(|((i, key), (tx, ty))| Item { key, kind: i.kind, x: i.x, y: i.y, tx: *tx, ty: *ty, stow: false }).collect();
            Some(Plan { items, cw: g.cw, ch: g.ch, areas, strays })
        })
        .flatten()
    }

    /// How covered the desktop is: icons on it, and the cells its screens hold.
    pub fn fullness() -> Option<(u32, u32)> {
        run(|c| {
            let Seen { icons, grid: g, areas, arranged, .. } = look(c)?;
            let cells: i32 = areas.iter().map(|a| ((a.r - a.l - g.ox) / g.cw).max(0) * ((a.b - a.t - g.oy) / g.ch).max(0)).sum();
            (!arranged).then_some((icons.len() as u32, cells.max(0) as u32))
        })
        .flatten()
    }

    /// The put-away sweep: icons of `paths` fly to the folder `drawer`; the
    /// rest, the folder among them, are laid out afresh without them.
    pub fn stow_plan(paths: Vec<String>, drawer: String) -> Option<Plan> {
        run(move |c| {
            let Seen { icons, keys, grid: g, areas, arranged } = look(c)?;
            if arranged || icons.is_empty() {
                return None;
            }
            let low = |s: &str| s.replace('/', "\\").to_lowercase();
            let going: std::collections::HashSet<String> = paths.iter().map(|p| low(p)).collect();
            let at = keys.iter().position(|k| low(k) == low(&drawer))?;
            let stay: Vec<usize> = (0..icons.len()).filter(|i| !going.contains(&low(&keys[*i]))).collect();
            let kept: Vec<Icon> = stay.iter().map(|i| icons[*i].clone()).collect();
            let to = tidy::layout(&kept, &areas, g);
            let mut target = vec![None; icons.len()];
            for (k, i) in stay.iter().enumerate() {
                target[*i] = Some(to[k]);
            }
            let folder = target[at]?;
            let items = icons
                .iter()
                .zip(keys)
                .enumerate()
                .map(|(i, (ic, key))| {
                    let (tx, ty) = target[i].unwrap_or(folder);
                    Item { key, kind: ic.kind, x: ic.x, y: ic.y, tx, ty, stow: target[i].is_none() }
                })
                .collect();
            Some(Plan { items, cw: g.cw, ch: g.ch, areas, strays: 0 })
        })
        .flatten()
    }

    pub fn strays() -> Option<u32> {
        run(|c| {
            let Seen { icons, grid: g, areas, arranged, .. } = look(c)?;
            Some(if arranged { 0 } else { tidy::strays(&icons, &areas, g) })
        })
        .flatten()
    }

    fn place(v: &View, moves: &[(String, i32, i32)]) {
        let mut ids = Vec::new();
        let mut pts = Vec::new();
        for (key, x, y) in moves {
            if let Some((_, p, _, _)) = v.items.iter().find(|(k, ..)| k == key) {
                ids.push(p.0 as *const ITEMIDLIST);
                pts.push(POINT { x: x - v.off.0, y: y - v.off.1 });
            }
        }
        if !ids.is_empty() {
            unsafe {
                let _ = v.fv.SelectAndPositionItems(ids.len() as u32, ids.as_ptr(), Some(pts.as_ptr()), SVSI_POSITIONITEM.0 as u32);
            }
        }
    }

    /// Before the sweep: keep every icon's place, and let icons sit between grid cells while they fly.
    pub fn begin() -> bool {
        run(|c| {
            let Some(v) = c.view.as_ref() else { return false };
            c.saved = v.items.iter().filter_map(|(k, p, ..)| unsafe { v.fv.GetItemPosition(p.0) }.ok().map(|pt| (k.clone(), pt))).collect();
            if let Some(f2) = &v.fv2 {
                if let Ok(f) = unsafe { f2.GetCurrentFolderFlags() } {
                    c.flags = Some(f);
                    if f & FWF_SNAPTOGRID.0 as u32 != 0 {
                        unsafe {
                            let _ = f2.SetCurrentFolderFlags(FWF_SNAPTOGRID.0 as u32, 0);
                        }
                    }
                }
            }
            !c.saved.is_empty()
        })
        .unwrap_or(false)
    }

    /// Icons to where they are on their way (screen px).
    pub fn moves(m: Vec<(String, i32, i32)>) {
        let _ = run(move |c| {
            if let Some(v) = &c.view {
                place(v, &m);
            }
        });
    }

    /// After the sweep: the grid as it was.
    pub fn end() {
        let _ = run(restore_flags);
    }

    fn restore_flags(c: &mut Ctx) {
        if let (Some(v), Some(f)) = (&c.view, c.flags.take()) {
            if let Some(f2) = &v.fv2 {
                unsafe {
                    let _ = f2.SetCurrentFolderFlags(FWF_SNAPTOGRID.0 as u32, f & FWF_SNAPTOGRID.0 as u32);
                }
            }
        }
    }

    /// Everything back where it was before the last sweep. How many went back.
    pub fn undo() -> u32 {
        run(|c| {
            if c.saved.is_empty() {
                return 0;
            }
            // The view may have changed since (a file added or gone): read it again, and match by name.
            let fresh = unsafe { open() }.ok().flatten();
            let Some(v) = fresh else { return 0 };
            let flags = v.fv2.as_ref().and_then(|f| unsafe { f.GetCurrentFolderFlags() }.ok());
            if let (Some(f2), Some(f)) = (&v.fv2, flags) {
                unsafe {
                    let _ = f2.SetCurrentFolderFlags(FWF_SNAPTOGRID.0 as u32, 0);
                }
                c.flags = Some(f);
            }
            let saved = std::mem::take(&mut c.saved);
            let back: Vec<(String, i32, i32)> = saved.iter().map(|(k, p)| (k.clone(), p.x + v.off.0, p.y + v.off.1)).collect();
            place(&v, &back);
            c.view = Some(v);
            restore_flags(c);
            back.len() as u32
        })
        .unwrap_or(0)
    }

    /// The desktop itself is the window in front (you clicked on it, or showed it).
    pub fn in_front() -> bool {
        use windows_sys::Win32::UI::WindowsAndMessaging::{GetClassNameW, GetForegroundWindow};
        unsafe {
            let h = GetForegroundWindow();
            if h.is_null() {
                return false;
            }
            let mut buf = [0u16; 32];
            let n = GetClassNameW(h, buf.as_mut_ptr(), buf.len() as i32);
            let class = String::from_utf16_lossy(&buf[..n.max(0) as usize]);
            class == "Progman" || class == "WorkerW"
        }
    }
}
