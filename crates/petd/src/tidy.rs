//! Tidying the desktop: where each icon goes, and how messy it is now.
//!
//! Pure arithmetic: the host reads the icons off the desktop and moves them.
//! Coordinates are physical screen pixels of each icon's cell (its top-left).
//! Icons stay on the screen they are on. On each screen they go into columns
//! from the top left, the way Windows itself lays them out: the system icons
//! first, then shortcuts, folders, documents, pictures, other media, archives
//! and installers, and the rest; a blank cell between one kind and the next.
//! System icons and shortcuts keep the order they had (回收站 stays on top);
//! files go in name order.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// 此电脑, 回收站 and the like: not files.
    System,
    /// Shortcuts: programs and links.
    App,
    Folder,
    Doc,
    Picture,
    Media,
    /// Archives and installers.
    Archive,
    Other,
}

/// What kind of icon a desktop item is: its parsing name (a path, or `::{…}`
/// for a system item), and whether the shell calls it a folder or a link.
pub fn kind_of(path: &str, folder: bool, link: bool) -> Kind {
    if path.starts_with("::") || path.is_empty() {
        return Kind::System;
    }
    let ext = std::path::Path::new(path)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "lnk" | "url" | "appref-ms" | "website" => Kind::App,
        // A zip is a folder to the shell; on the desktop it is an archive.
        "zip" | "rar" | "7z" | "tar" | "gz" | "tgz" | "bz2" | "xz" | "exe" | "msi" | "msix" | "appx" | "iso" | "dmg" | "apk" => Kind::Archive,
        "doc" | "docx" | "pdf" | "txt" | "md" | "rtf" | "odt" | "xls" | "xlsx" | "csv" | "ods" | "ppt" | "pptx" | "odp" | "key" | "pages" | "numbers" | "epub" | "xmind" => Kind::Doc,
        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "heic" | "svg" | "ico" | "tif" | "tiff" | "psd" | "ai" | "fig" | "sketch" => Kind::Picture,
        "mp4" | "mov" | "avi" | "mkv" | "wmv" | "webm" | "mp3" | "wav" | "flac" | "m4a" | "aac" | "ogg" => Kind::Media,
        _ if link => Kind::App,
        _ if folder => Kind::Folder,
        _ => Kind::Other,
    }
}

#[derive(Debug, Clone)]
pub struct Icon {
    pub x: i32,
    pub y: i32,
    pub kind: Kind,
    /// Its file name, for the order within a kind.
    pub name: String,
}

/// A screen's work area (without the taskbar), physical px.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Area {
    pub l: i32,
    pub t: i32,
    pub r: i32,
    pub b: i32,
}

/// The desktop's icon grid: cell size, and the margin from a screen's top-left to its first cell.
#[derive(Debug, Clone, Copy)]
pub struct Grid {
    pub cw: i32,
    pub ch: i32,
    pub ox: i32,
    pub oy: i32,
}

impl Grid {
    /// The grid as the icons themselves show it: the cell size the shell gives,
    /// and the margin most icons share on their own screen.
    pub fn from_icons(icons: &[Icon], areas: &[Area], cw: i32, ch: i32) -> Grid {
        let (cw, ch) = (cw.max(1), ch.max(1));
        let bare = Grid { cw, ch, ox: 0, oy: 0 };
        let most = |vals: Vec<i32>| {
            let mut n = std::collections::HashMap::new();
            for v in vals {
                *n.entry(v).or_insert(0) += 1;
            }
            n.into_iter().max_by_key(|(v, c)| (*c, -*v)).map(|(v, _)| v).unwrap_or(0)
        };
        let home = |i: &Icon| areas.get(area_of(i, areas, &bare)).copied().unwrap_or(Area { l: 0, t: 0, r: 0, b: 0 });
        Grid {
            cw,
            ch,
            ox: most(icons.iter().map(|i| (i.x - home(i).l).rem_euclid(cw)).collect()),
            oy: most(icons.iter().map(|i| (i.y - home(i).t).rem_euclid(ch)).collect()),
        }
    }

    /// The first column and row inside `a`.
    fn origin(&self, a: &Area) -> (i32, i32) {
        (a.l + self.ox, a.t + self.oy)
    }

    fn rows(&self, a: &Area) -> i32 {
        let (_, y0) = self.origin(a);
        ((a.b - y0) / self.ch).max(1)
    }
}

/// Which area an icon belongs to: the one its middle is in, else the nearest.
fn area_of(i: &Icon, areas: &[Area], g: &Grid) -> usize {
    let (cx, cy) = (i.x + g.cw / 2, i.y + g.ch / 2);
    if let Some(k) = areas.iter().position(|a| cx >= a.l && cx < a.r && cy >= a.t && cy < a.b) {
        return k;
    }
    let far = |a: &Area| {
        let dx = (a.l - cx).max(0).max(cx - a.r) as i64;
        let dy = (a.t - cy).max(0).max(cy - a.b) as i64;
        dx * dx + dy * dy
    };
    (0..areas.len()).min_by_key(|k| far(&areas[*k])).unwrap_or(0)
}

/// Which side of a screen its icons are kept against, and the columns' margin from that edge.
#[derive(Clone, Copy)]
struct Side {
    right: bool,
    margin: i32,
}

impl Side {
    fn x(&self, a: &Area, g: &Grid, col: i32) -> i32 {
        if self.right {
            a.r - self.margin - (col + 1) * g.cw
        } else {
            a.l + self.margin + col * g.cw
        }
    }
    /// The column nearest `x`, and whether `x` is on it.
    fn col(&self, a: &Area, g: &Grid, x: i32) -> (i32, bool) {
        let d = if self.right { a.r - self.margin - g.cw - x } else { x - a.l - self.margin };
        let c = (d as f64 / g.cw as f64).round() as i32;
        (c, (d - c * g.cw).abs() <= g.cw / 6)
    }
}

/// The icons that are already in line and stay put: on the grid, alone in
/// their cell, within the columns a packed desktop needs (counted from the
/// side the icons are kept on). The rest are strays. Per area, the side.
fn settled(icons: &[Icon], areas: &[Area], g: &Grid) -> (Vec<bool>, Vec<Side>) {
    let mut keep = vec![false; icons.len()];
    let mut sides = Vec::new();
    for (k, a) in areas.iter().enumerate() {
        let mine: Vec<usize> = (0..icons.len()).filter(|i| area_of(&icons[*i], areas, g) == k).collect();
        let (_, y0) = g.origin(a);
        let rows = g.rows(a);
        // Packed, with a blank cell between kinds, they need about this many columns; one more is slack.
        let kinds = mine.iter().map(|i| icons[*i].kind).collect::<std::collections::HashSet<_>>().len() as i32;
        let need = (mine.len() as i32 + kinds - 1 + rows - 1) / rows;
        let left = Side { right: false, margin: g.ox };
        // On the right, the margin most of them share from the right edge.
        let mut n = std::collections::HashMap::new();
        for i in &mine {
            *n.entry((a.r - icons[*i].x - g.cw).rem_euclid(g.cw)).or_insert(0) += 1;
        }
        let right = Side { right: true, margin: n.into_iter().max_by_key(|(m, c)| (*c, -*m)).map(|(m, _)| m).unwrap_or(g.ox) };
        let ok = |side: &Side| {
            let mut taken = std::collections::HashSet::new();
            let mut kept = Vec::new();
            for i in &mine {
                let ic = &icons[*i];
                let (col, on) = side.col(a, g, ic.x);
                let dy = ic.y - y0;
                let row = (dy as f64 / g.ch as f64).round() as i32;
                let on = on && (dy - row * g.ch).abs() <= g.ch / 6 && row >= 0 && row < rows && col >= 0;
                if on && col <= need && taken.insert((col, row)) {
                    kept.push(*i);
                }
            }
            kept
        };
        let (l, r) = (ok(&left), ok(&right));
        let (kept, side) = if r.len() > l.len() { (r, right) } else { (l, left) };
        for i in kept {
            keep[i] = true;
        }
        sides.push(side);
    }
    (keep, sides)
}

/// Where each icon goes (same order as `icons`). On a screen where most icons
/// are already in line, those stay put and the strays are swept into the free
/// cells after them; where most are strewn about, all of them are sorted.
pub fn layout(icons: &[Icon], areas: &[Area], g: Grid) -> Vec<(i32, i32)> {
    let mut out: Vec<(i32, i32)> = icons.iter().map(|i| (i.x, i.y)).collect();
    if areas.is_empty() {
        return out;
    }
    let (keep, sides) = settled(icons, areas, &g);
    for (k, a) in areas.iter().enumerate() {
        let mut mine: Vec<usize> = (0..icons.len()).filter(|i| area_of(&icons[*i], areas, &g) == k).collect();
        let kept = mine.iter().filter(|i| keep[**i]).count();
        let gentle = kept * 2 >= mine.len();
        // Where it stood, column by column: the order a person reads the desktop in.
        let read = |i: &Icon| ((i.x - a.l + g.cw / 2).div_euclid(g.cw), i.y);
        mine.sort_by(|p, q| {
            let (p, q) = (&icons[*p], &icons[*q]);
            p.kind.cmp(&q.kind).then_with(|| match p.kind {
                Kind::System | Kind::App => read(p).cmp(&read(q)),
                _ => p.name.to_lowercase().cmp(&q.name.to_lowercase()),
            })
        });
        let (_, y0) = g.origin(a);
        let rows = g.rows(a);
        if gentle {
            // The ones in line stay; each stray takes the next free cell, column by column.
            let side = sides[k];
            let taken: std::collections::HashSet<(i32, i32)> = mine
                .iter()
                .filter(|i| keep[**i])
                .map(|i| (side.col(a, &g, icons[*i].x).0, ((icons[*i].y - y0) as f64 / g.ch as f64).round() as i32))
                .collect();
            let mut cells = (0..).flat_map(|c| (0..rows).map(move |r| (c, r))).filter(|c| !taken.contains(c));
            for i in mine.into_iter().filter(|i| !keep[*i]) {
                let (c, r) = cells.next().unwrap_or((0, 0));
                out[i] = (side.x(a, &g, c), y0 + r * g.ch);
            }
            continue;
        }
        let x0 = a.l + g.ox;
        let (mut col, mut row) = (0, 0);
        let mut last: Option<Kind> = None;
        for i in mine {
            let kind = icons[i].kind;
            if last.is_some_and(|l| l != kind) && row > 0 {
                // A blank cell between kinds; a kind never starts in a column's last cell if it has more.
                row += 1;
                if row >= rows - 1 {
                    col += 1;
                    row = 0;
                }
            }
            last = Some(kind);
            out[i] = (x0 + col * g.cw, y0 + row * g.ch);
            row += 1;
            if row >= rows {
                col += 1;
                row = 0;
            }
        }
    }
    out
}

/// How messy the desktop is: the icons that are not already in line (strayed
/// out past the columns they would fill if packed, sharing a cell, or off the
/// grid). A desktop packed in any order of its own, on either side, is tidy.
pub fn strays(icons: &[Icon], areas: &[Area], g: Grid) -> u32 {
    if areas.is_empty() {
        return 0;
    }
    settled(icons, areas, &g).0.iter().filter(|k| !**k).count() as u32
}

/// Strays that make a desktop worth offering to sweep.
pub const OFFER_FROM: u32 = 4;

static STRAYS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// The host counted the strays on the desktop just now.
pub fn saw(strays: u32) {
    STRAYS.store(strays, std::sync::atomic::Ordering::Relaxed);
}

/// Strays at the host's last look (0 before it has looked).
pub fn last_strays() -> u32 {
    STRAYS.load(std::sync::atomic::Ordering::Relaxed)
}

/// You are looking at the desktop and `strays` icons lie about on it: the
/// first pet that is up offers to sweep. Once a day at most, never in focus
/// mode or when they are told to keep quiet, and only in waking hours. True
/// when it asked.
pub fn offer(sh: &crate::server::Shared, strays: u32) -> bool {
    use chrono::Timelike;
    if strays < OFFER_FROM {
        return false;
    }
    let now = crate::now_ms();
    let day = crate::db::day_of(now);
    if sh.db.kv_get("tidy:asked").as_deref() == Some(day.as_str()) || !(8..24).contains(&chrono::Local::now().hour()) {
        return false;
    }
    let (pet, who) = {
        let hub = sh.hub.lock().unwrap();
        if hub.cfg.features.focus_mode || hub.cfg.features.chatter == "quiet" {
            return false;
        }
        use crate::session::State;
        let pets = hub.pets();
        let Some(p) = pets.iter().find(|v| !matches!(v.state, State::Offline | State::Sleeping | State::Attention)) else {
            return false;
        };
        (p.id.clone(), crate::cast::by_id(&p.skin).map(|f| f.id).unwrap_or("zimeng"))
    };
    sh.db.kv_set("tidy:asked", &day);
    let text = crate::voice::line(who, "offer_tidy", 0);
    let _ = sh.ws_tx.send(crate::msg("pet.act", &serde_json::json!({ "pet": pet, "act": "offer", "text": text, "why": "tidy", "offer": "tidy" })));
    true
}

/// The diary line for a sweep done: who did it and how many icons it put away.
pub fn noted(sh: &crate::server::Shared, who: &str, n: u32) {
    let name = crate::cast::by_id(who).map(|f| f.name()).unwrap_or("");
    sh.db.log_event(
        crate::now_ms(),
        "noticed",
        &crate::tr!("桌面散了。{name}骑着扫帚过去，把 {n} 个图标排回了列里。", "The desktop had come loose. {name} rode its broom over and set {n} icons back in their columns."),
        1,
    );
}

/// Icons a tidy moves by more than half a cell.
pub fn moved(icons: &[Icon], to: &[(i32, i32)], g: Grid) -> u32 {
    icons.iter().zip(to).filter(|(i, (x, y))| (i.x - x).abs() > g.cw / 2 || (i.y - y).abs() > g.ch / 2).count() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    const G: Grid = Grid { cw: 100, ch: 110, ox: 10, oy: 4 };
    const SCREEN: Area = Area { l: 0, t: 0, r: 1920, b: 1040 };

    fn icon(x: i32, y: i32, name: &str) -> Icon {
        let folder = !name.contains('.') && !name.starts_with("::");
        Icon { x, y, kind: kind_of(name, folder, false), name: name.into() }
    }

    #[test]
    fn kinds() {
        assert_eq!(kind_of("::{645FF040-5081-101B-9F08-00AA002F954E}", true, false), Kind::System);
        assert_eq!(kind_of(r"C:\Users\a\Desktop\微信.lnk", false, true), Kind::App);
        assert_eq!(kind_of(r"C:\Users\a\Desktop\报告.PDF", false, false), Kind::Doc);
        assert_eq!(kind_of(r"C:\Users\a\Desktop\截图 1.png", false, false), Kind::Picture);
        assert_eq!(kind_of(r"C:\Users\a\Desktop\素材.zip", true, false), Kind::Archive);
        assert_eq!(kind_of(r"C:\Users\a\Desktop\setup.exe", false, false), Kind::Archive);
        assert_eq!(kind_of(r"C:\Users\a\Desktop\项目", true, false), Kind::Folder);
        assert_eq!(kind_of(r"C:\Users\a\Desktop\notes.foo", false, false), Kind::Other);
    }

    #[test]
    fn packs_by_kind_into_columns_from_the_top_left() {
        let icons = vec![
            icon(900, 500, "b.png"),
            icon(10, 4, "::{recycle}"),
            icon(1500, 300, "a.png"),
            icon(400, 800, "x.docx"),
            icon(700, 114, "setup.exe"),
        ];
        let to = layout(&icons, &[SCREEN], G);
        // system, then the doc, then the two pictures in name order, then the installer; a blank cell between kinds.
        assert_eq!(to[1], (10, 4));
        assert_eq!(to[3], (10, 4 + 2 * 110));
        assert_eq!(to[2], (10, 4 + 4 * 110));
        assert_eq!(to[0], (10, 4 + 5 * 110));
        assert_eq!(to[4], (10, 4 + 7 * 110));
        assert_eq!(moved(&icons, &to, G), 4);
    }

    #[test]
    fn system_icons_and_shortcuts_keep_their_order() {
        let icons = vec![
            icon(10, 4, "::{recycle}"),
            icon(10, 114, "::{this-pc}"),
            Icon { x: 10, y: 224, kind: Kind::App, name: "浏览器.lnk".into() },
            Icon { x: 10, y: 334, kind: Kind::App, name: "聊天.lnk".into() },
            Icon { x: 900, y: 300, kind: Kind::App, name: "音乐.lnk".into() },
        ];
        let to = layout(&icons, &[SCREEN], G);
        // Most are in line: they stay, and the stray goes into the next free cell.
        assert_eq!(&to[..4], &[(10, 4), (10, 114), (10, 224), (10, 334)]);
        assert_eq!(to[4], (10, 444));
        // Mostly strewn: everything is sorted, and still 回收站 before 此电脑 because that is how they stood.
        let strewn = vec![icon(10, 4, "::{recycle}"), icon(600, 300, "::{this-pc}"), icon(900, 300, "a.txt"), icon(700, 800, "b.png")];
        let to = layout(&strewn, &[SCREEN], G);
        assert_eq!(&to[..2], &[(10, 4), (10, 114)]);
    }

    #[test]
    fn a_mostly_tidy_desktop_keeps_its_order_and_takes_the_strays_in() {
        let mut icons: Vec<Icon> = (0..12).map(|k| icon(10 + (k / 9) * 100, 4 + (k % 9) * 110, &format!("f{k:02}.txt"))).collect();
        icons.push(icon(810, 334, "zz.png"));
        icons.push(icon(733, 501, "aa.png"));
        let to = layout(&icons, &[SCREEN], G);
        for (i, ic) in icons.iter().take(12).enumerate() {
            assert_eq!(to[i], (ic.x, ic.y));
        }
        // The packed ones fill column 0 and three cells of column 1; the strays come next, in name order.
        assert_eq!(to[13], (110, 4 + 3 * 110));
        assert_eq!(to[12], (110, 4 + 4 * 110));
        assert_eq!(moved(&icons, &to, G), 2);
        // Kept on the right edge: the strays go in on the right too.
        let mut right: Vec<Icon> = (0..12).map(|k| icon(1910 - 100 - (k / 9) * 100, 4 + (k % 9) * 110, &format!("f{k:02}.txt"))).collect();
        right.push(icon(810, 334, "zz.png"));
        let to = layout(&right, &[SCREEN], G);
        assert_eq!(to[12], (1710, 4 + 3 * 110));
    }

    #[test]
    fn a_full_column_goes_on_in_the_next() {
        let rows = (SCREEN.b - 4) / 110; // 9
        let icons: Vec<Icon> = (0..rows + 2).map(|k| icon(1000 + k * 7, 500, &format!("f{k:02}.txt"))).collect();
        let to = layout(&icons, &[SCREEN], G);
        assert_eq!(to[rows as usize - 1], (10, 4 + (rows - 1) * 110));
        assert_eq!(to[rows as usize], (110, 4));
        assert_eq!(to[rows as usize + 1], (110, 114));
    }

    #[test]
    fn icons_stay_on_their_own_screen() {
        let right = Area { l: 1920, t: 0, r: 3840, b: 1040 };
        let icons = vec![icon(3000, 600, "a.txt"), icon(500, 600, "b.txt")];
        let to = layout(&icons, &[SCREEN, right], G);
        assert_eq!(to[0], (1930, 4));
        assert_eq!(to[1], (10, 4));
    }

    #[test]
    fn packed_desktops_are_tidy_and_strays_are_not() {
        let packed: Vec<Icon> = (0..12).map(|k| icon(10 + (k / 9) * 100, 4 + (k % 9) * 110, &format!("f{k}.txt"))).collect();
        assert_eq!(strays(&packed, &[SCREEN], G), 0);
        // The same, kept on the right edge: also tidy.
        let right: Vec<Icon> = packed.iter().map(|i| Icon { x: 1910 - 100 - (i.x - 10), ..i.clone() }).collect();
        assert_eq!(strays(&right, &[SCREEN], G), 0);
        // Five dropped anywhere in the middle, one of them off the grid.
        let mut messy = packed.clone();
        for (k, (x, y)) in [(810, 334), (910, 444), (1210, 224), (1010, 664), (733, 501)].iter().enumerate() {
            messy.push(icon(*x, *y, &format!("new{k}.png")));
        }
        assert_eq!(strays(&messy, &[SCREEN], G), 5);
        // Two on one cell.
        let mut piled = packed.clone();
        piled.push(icon(10, 4, "on-top.txt"));
        assert_eq!(strays(&piled, &[SCREEN], G), 1);
    }
}
