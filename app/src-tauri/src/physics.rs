//! Dragging, throwing and walking pet windows (design §6 "物理").
//!
//! The pet window itself moves at ~60 fps: no hand-off to the overlay is
//! needed, so there is never a frame jump. Units are physical pixels; the
//! constants below are per logical pixel and scaled by the monitor.
//!
//!   gravity 2400 px/s², bounce 0.45, ground friction 0.85 per frame,
//!   at rest below 30 px/s, dizzy when hitting a wall faster than 1500 px/s,
//!   throw speed from the last 80 ms of the drag.
//!
//! Floors: the work-area bottom of each monitor (above the taskbar) and the
//! top edge of ordinary visible windows (title bars).

use crate::App;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, WebviewWindow};

const GRAVITY: f64 = 2400.0;
const BOUNCE: f64 = 0.45;
const FRICTION: f64 = 0.85;
const REST_SPEED: f64 = 30.0;
const DIZZY_SPEED: f64 = 1500.0;
const SAMPLE_MS: u128 = 80;
/// Logical px between the window bottom and the point that rests on a floor.
/// The pet's feet sit 18 px above the window bottom, with the camp tag below
/// them; landing the window bottom (not the feet) on the floor keeps that tag
/// above the taskbar, which would otherwise cover it.
pub const FOOT_H: f64 = 0.0;
const FRAME: Duration = Duration::from_millis(16);

#[derive(Clone, Copy, Debug)]
pub struct Rect {
    pub l: f64,
    pub t: f64,
    pub r: f64,
    pub b: f64,
}

/// Monitor work areas, physical px, with scale.
fn monitors(w: &WebviewWindow) -> Vec<(Rect, f64)> {
    w.available_monitors()
        .unwrap_or_default()
        .iter()
        .map(|m| {
            let a = m.work_area();
            (
                Rect {
                    l: a.position.x as f64,
                    t: a.position.y as f64,
                    r: (a.position.x + a.size.width as i32) as f64,
                    b: (a.position.y + a.size.height as i32) as f64,
                },
                m.scale_factor(),
            )
        })
        .collect()
}

fn monitor_at(mons: &[(Rect, f64)], x: f64, y: f64) -> Option<(Rect, f64)> {
    mons.iter().copied().find(|(r, _)| x >= r.l && x < r.r && y >= r.t - 1.0 && y <= r.b + 1.0)
}

/// Shared per-pet motion state.
#[derive(Default)]
pub struct Motion {
    /// Pets currently driven by a drag / flight / walk thread (by label).
    busy: Mutex<HashMap<String, u64>>,
    /// Where a pet stood before a scripted walk, to walk back to.
    homes: Mutex<HashMap<String, (i32, i32)>>,
    /// The pet the user is dragging right now (by label).
    pub dragging: Mutex<Option<String>>,
    next: Mutex<u64>,
}

impl Motion {
    /// Claim a pet for a new motion; any older motion of that pet stops.
    fn claim(&self, label: &str) -> u64 {
        let mut n = self.next.lock().unwrap();
        *n += 1;
        self.busy.lock().unwrap().insert(label.to_string(), *n);
        *n
    }
    fn owns(&self, label: &str, token: u64) -> bool {
        self.busy.lock().unwrap().get(label) == Some(&token)
    }
    fn release(&self, label: &str, token: u64) {
        let mut b = self.busy.lock().unwrap();
        if b.get(label) == Some(&token) {
            b.remove(label);
        }
    }
    pub fn is_busy(&self, label: &str) -> bool {
        self.busy.lock().unwrap().contains_key(label)
    }
}

fn phys(w: &WebviewWindow, kind: &str) {
    let _ = w.emit_to(w.label(), "phys", serde_json::json!({ "kind": kind }));
}

/// Follow the cursor while the left button is held, then throw.
pub fn drag(app: Arc<App>, w: WebviewWindow) {
    let label = w.label().to_string();
    let token = app.motion.claim(&label);
    std::thread::spawn(move || {
        let (Ok(pos), Some((cx, cy))) = (w.outer_position(), crate::windows::cursor()) else {
            app.motion.release(&label, token);
            return;
        };
        let grab = (cx - pos.x as f64, cy - pos.y as f64);
        *app.motion.dragging.lock().unwrap() = Some(label.clone());
        let mut samples: VecDeque<(Instant, f64, f64)> = VecDeque::new();
        while crate::windows::left_down() && app.motion.owns(&label, token) {
            if let Some((cx, cy)) = crate::windows::cursor() {
                let (x, y) = (cx - grab.0, cy - grab.1);
                let _ = w.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
                let now = Instant::now();
                samples.push_back((now, x, y));
                while samples.front().is_some_and(|s| now.duration_since(s.0).as_millis() > SAMPLE_MS) {
                    samples.pop_front();
                }
            }
            std::thread::sleep(Duration::from_millis(8));
        }
        *app.motion.dragging.lock().unwrap() = None;
        let (vx, vy) = match (samples.front(), samples.back()) {
            (Some(a), Some(b)) if b.0 > a.0 => {
                let dt = b.0.duration_since(a.0).as_secs_f64().max(0.016);
                ((b.1 - a.1) / dt, (b.2 - a.2) / dt)
            }
            _ => (0.0, 0.0),
        };
        fly(&app, &w, token, vx, vy);
        app.motion.release(&label, token);
    });
}

/// Throw a pet with a velocity in logical px/s (used by `/pet fling`, annoyed hops).
pub fn throw(app: Arc<App>, w: WebviewWindow, vx: f64, vy: f64) {
    let label = w.label().to_string();
    let token = app.motion.claim(&label);
    std::thread::spawn(move || {
        let sc = w.scale_factor().unwrap_or(1.0);
        fly(&app, &w, token, vx * sc, vy * sc);
        app.motion.release(&label, token);
    });
}

/// Ballistic flight until the pet rests on a floor. Velocities in physical px/s.
fn fly(app: &Arc<App>, w: &WebviewWindow, token: u64, mut vx: f64, mut vy: f64) {
    let label = w.label().to_string();
    let mons = monitors(w);
    let (Ok(pos), Ok(size)) = (w.outer_position(), w.outer_size()) else { return };
    let (ww, wh) = (size.width as f64, size.height as f64);
    let mut x = pos.x as f64;
    let mut y = pos.y as f64;
    let mut surfaces = crate::windows::window_tops(w);
    let mut last = Instant::now();
    let mut flying_sent = false;
    let mut dizzy_sent = false;
    let mut on_ground_frames = 0;
    let start = Instant::now();

    loop {
        if !app.motion.owns(&label, token) {
            return;
        }
        std::thread::sleep(FRAME);
        let now = Instant::now();
        let dt = now.duration_since(last).as_secs_f64().min(0.05);
        last = now;
        let sc = w.scale_factor().unwrap_or(1.0);
        let foot_off = wh - FOOT_H * sc;
        let (cx, foot) = (x + ww / 2.0, y + foot_off);
        let (mon, _) = monitor_at(&mons, cx, foot)
            .or_else(|| mons.first().copied())
            .unwrap_or((Rect { l: 0.0, t: 0.0, r: 1920.0, b: 1080.0 }, 1.0));

        vy += GRAVITY * sc * dt;
        let nx = x + vx * dt;
        let ny = y + vy * dt;
        let ncx = nx + ww / 2.0;
        let nfoot = ny + foot_off;

        // Walls: leaving every monitor horizontally bounces back.
        let mut hit_wall = 0.0;
        let mut x2 = nx;
        // Probe at a height inside the current monitor, so flying above its
        // top edge does not count as leaving it sideways.
        if monitor_at(&mons, ncx, nfoot.clamp(mon.t, mon.b - 1.0)).is_none() {
            if ncx < mon.l {
                x2 = mon.l - ww / 2.0;
            } else if ncx >= mon.r {
                x2 = mon.r - ww / 2.0 - 1.0;
            }
            hit_wall = vx.abs();
            vx = -vx * BOUNCE;
        }
        // Ceiling.
        let mut y2 = ny;
        let top = ny + wh * 0.4;
        if top < mon.t {
            y2 = mon.t - wh * 0.4;
            hit_wall = hit_wall.max(vy.abs());
            vy = -vy * BOUNCE;
        }
        if hit_wall > DIZZY_SPEED * sc && !dizzy_sent {
            dizzy_sent = true;
            phys(w, "dizzy");
        }

        // Floors: monitor bottom, or a title bar crossed while falling.
        let mut floor = mon.b;
        if vy > 0.0 {
            for s in &surfaces {
                let cx2 = x2 + ww / 2.0;
                if cx2 >= s.l && cx2 <= s.r && foot <= s.t + 1.0 && nfoot >= s.t && s.t < floor {
                    floor = s.t;
                }
            }
        }
        let landed = nfoot >= floor;
        if landed {
            y2 = floor - foot_off;
            if vy > 0.0 {
                vy = -vy * BOUNCE;
            }
            if vy.abs() < REST_SPEED * sc * 4.0 {
                vy = 0.0;
            }
            vx *= FRICTION;
            on_ground_frames += 1;
        } else {
            on_ground_frames = 0;
            if !flying_sent && (vx.abs() > REST_SPEED * sc || vy.abs() > REST_SPEED * sc * 4.0) {
                flying_sent = true;
                phys(w, "fly");
            }
        }
        x = x2;
        y = y2;
        let _ = w.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));

        let speed = (vx * vx + vy * vy).sqrt();
        if landed && speed < REST_SPEED * sc && on_ground_frames >= 2 {
            break;
        }
        if start.elapsed() > Duration::from_secs(12) {
            break;
        }
        if start.elapsed().as_millis() % 500 < 17 {
            surfaces = crate::windows::window_tops(w);
        }
    }
    if flying_sent {
        phys(w, "land");
    }
    // Resting on a window: fall when it moves or closes.
    watch_perch(app.clone(), w.clone(), token);
}

/// While a pet sits on a title bar, check now and then that the bar is still
/// under its feet; if not, let it fall to the next floor.
fn watch_perch(app: Arc<App>, w: WebviewWindow, _token: u64) {
    let label = w.label().to_string();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(600));
        if app.motion.is_busy(&label) {
            return; // a new motion took over
        }
        let (Ok(pos), Ok(size), Ok(sc)) = (w.outer_position(), w.outer_size(), w.scale_factor()) else { return };
        let foot = pos.y as f64 + size.height as f64 - FOOT_H * sc;
        let cx = pos.x as f64 + size.width as f64 / 2.0;
        let mons = monitors(&w);
        let on_floor = mons.iter().any(|(r, _)| cx >= r.l && cx < r.r && (foot - r.b).abs() <= 2.0);
        if on_floor {
            return;
        }
        let supported = crate::windows::window_tops(&w).iter().any(|s| cx >= s.l && cx <= s.r && (foot - s.t).abs() <= 3.0);
        if !supported {
            throw(app.clone(), w.clone(), 0.0, 0.0);
            return;
        }
    });
}

/// Pets never overlap. Every 200 ms, compare the pet bodies (the main sprite
/// rect each page reports, trimmed a little since sprites have empty edges).
/// When two overlap: a pet being dragged / thrown / walked pushes the other
/// one away; two idle pets part with the right-hand one hopping right (or
/// the left one hopping left when the right one has no room).
pub fn spawn_separator(h: AppHandle, app: Arc<App>) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(200));
        let mut bodies: Vec<(WebviewWindow, String, Rect)> = Vec::new();
        for (label, w) in h.webview_windows() {
            if !label.starts_with("pet-") || !w.is_visible().unwrap_or(false) {
                continue;
            }
            let rect = app.hits.lock().unwrap().get(&label).and_then(|(r, _)| r.last().copied());
            let (Some(r), Ok(pos), Ok(sc)) = (rect, w.outer_position(), w.scale_factor()) else { continue };
            let trim = r.w * 0.15;
            bodies.push((
                w,
                label,
                Rect {
                    l: pos.x as f64 + (r.x + trim) * sc,
                    t: pos.y as f64 + r.y * sc,
                    r: pos.x as f64 + (r.x + r.w - trim) * sc,
                    b: pos.y as f64 + (r.y + r.h) * sc,
                },
            ));
        }
        for i in 0..bodies.len() {
            for j in i + 1..bodies.len() {
                let (a, b) = (&bodies[i].2, &bodies[j].2);
                let overlap = a.l < b.r && b.l < a.r && a.t < b.b && b.t < a.b;
                if !overlap {
                    continue;
                }
                let (busy_a, busy_b) = (app.motion.is_busy(&bodies[i].1), app.motion.is_busy(&bodies[j].1));
                let (ca, cb) = ((a.l + a.r) / 2.0, (b.l + b.r) / 2.0);
                // (who moves, away from whom)
                let (mover, from) = match (busy_a, busy_b) {
                    (true, true) => continue,
                    (true, false) => (j, ca),
                    (false, true) => (i, cb),
                    (false, false) => {
                        let (right, left) = if ca >= cb { (i, j) } else { (j, i) };
                        let rb = &bodies[right].2;
                        let room = monitor_at(&monitors(&bodies[right].0), (rb.l + rb.r) / 2.0, rb.b - 1.0)
                            .map(|(m, _)| m.r - rb.r)
                            .unwrap_or(1e9);
                        if room > (rb.r - rb.l) * 1.5 {
                            (right, (bodies[left].2.l + bodies[left].2.r) / 2.0)
                        } else {
                            (left, (rb.l + rb.r) / 2.0)
                        }
                    }
                };
                let m = &bodies[mover];
                let dir = if (m.2.l + m.2.r) / 2.0 >= from { 1.0 } else { -1.0 };
                throw(app.clone(), m.0.clone(), dir * 650.0, -450.0);
                // Dragged onto another pet: they play catch (cooldown inside).
                let dragged = app.motion.dragging.lock().unwrap().clone();
                let other = if mover == i { j } else { i };
                if dragged.as_deref() == Some(bodies[other].1.as_str()) {
                    let a = bodies[other].1.trim_start_matches("pet-").to_string();
                    let b = bodies[mover].1.trim_start_matches("pet-").to_string();
                    let views = app.petd.hub.lock().unwrap().pets();
                    let outs = app.petd.shared.world.lock().unwrap().request_toss(&a, &b, petd::now_ms(), &views);
                    for o in outs {
                        if let petd::world::Out::Ws(msg) = o {
                            let _ = app.petd.shared.ws_tx.send(msg);
                        }
                    }
                }
            }
        }
    });
}

/// Tug of war: `mover` walks up to `anchor` until the two windows touch edge
/// to edge, then both pages are told which side they stand on, so each draws
/// its half of the rope to the shared edge.
pub fn walk_tug(app: Arc<App>, h: &AppHandle, mover: &str, anchor: &str) {
    walk_pair(app, h, mover, anchor, 0.0, "tug");
}

/// Walk `mover` to stand `gap` logical px beside `anchor` (0: windows touch),
/// then tell both pets which side they are on (`kind`: tug / duo).
pub fn walk_pair(app: Arc<App>, h: &AppHandle, mover: &str, anchor: &str, gap: f64, kind: &'static str) {
    let label = format!("pet-{mover}");
    let (Some(w), Some(aw)) = (h.get_webview_window(&label), h.get_webview_window(&format!("pet-{anchor}"))) else { return };
    let (Ok(pos), Ok(size), Ok(tp), Ok(ts)) = (w.outer_position(), w.outer_size(), aw.outer_position(), aw.outer_size()) else {
        return;
    };
    app.motion.homes.lock().unwrap().entry(label.clone()).or_insert((pos.x, pos.y));
    let (sw, aw_w) = (size.width as i32, ts.width as i32);
    let g = (gap * aw.scale_factor().unwrap_or(1.0)).round() as i32;
    let right = tp.x + aw_w + g;
    let left = tp.x - sw - g;
    let mut x = if pos.x >= tp.x { right } else { left };
    // Stay on the anchor's monitor.
    let acx = (tp.x + aw_w / 2) as f64;
    // Window centre: the bottom edge can sit below the work area (label row).
    let acy = (tp.y + ts.height as i32 / 2) as f64;
    if let Some((m, _)) = monitor_at(&monitors(&w), acx, acy) {
        if (x + sw) as f64 > m.r {
            x = left;
        }
        if (x as f64) < m.l {
            x = right;
        }
    }
    let y = tp.y + ts.height as i32 - size.height as i32;
    let mover_side = if x > tp.x { "right" } else { "left" };
    let anchor_side = if mover_side == "right" { "left" } else { "right" };
    let token = app.motion.claim(&label);
    std::thread::spawn(move || {
        phys(&w, "walk");
        let from = (pos.x as f64, pos.y as f64);
        let t0 = Instant::now();
        let ms = 1200.0;
        loop {
            if !app.motion.owns(&label, token) {
                return;
            }
            let k = (t0.elapsed().as_secs_f64() * 1000.0 / ms).min(1.0);
            let e = k * k * (3.0 - 2.0 * k);
            let nx = from.0 + (x as f64 - from.0) * e;
            let ny = from.1 + (y as f64 - from.1) * e;
            let _ = w.set_position(PhysicalPosition::new(nx.round() as i32, ny.round() as i32));
            if k >= 1.0 {
                break;
            }
            std::thread::sleep(FRAME);
        }
        phys(&w, "stop");
        let _ = w.emit_to(w.label(), "phys", serde_json::json!({ "kind": kind, "side": mover_side }));
        let _ = aw.emit_to(aw.label(), "phys", serde_json::json!({ "kind": kind, "side": anchor_side }));
        app.motion.release(&label, token);
    });
}

/// Scripted walk: next to `target`'s window, or back home when None.
pub fn walk(app: Arc<App>, h: &AppHandle, pet: &str, target: Option<&str>) {
    let label = format!("pet-{pet}");
    let Some(w) = h.get_webview_window(&label) else { return };
    let (Ok(pos), Ok(size)) = (w.outer_position(), w.outer_size()) else { return };
    let to = match target {
        Some(t) => {
            let Some(tw) = h.get_webview_window(&format!("pet-{t}")) else { return };
            let (Ok(tp), Ok(ts)) = (tw.outer_position(), tw.outer_size()) else { return };
            app.motion.homes.lock().unwrap().entry(label.clone()).or_insert((pos.x, pos.y));
            // Stand beside the target, on the side we come from.
            let side = if pos.x < tp.x { -1 } else { 1 };
            let x = tp.x + ts.width as i32 / 2 + side * (ts.width as i32 / 2 + size.width as i32 / 3) - size.width as i32 / 2;
            let y = tp.y + ts.height as i32 - size.height as i32;
            (x, y)
        }
        None => match app.motion.homes.lock().unwrap().remove(&label) {
            Some(home) => home,
            None => return,
        },
    };
    let token = app.motion.claim(&label);
    let ms = if target.is_some() { 1200.0 } else { 700.0 };
    std::thread::spawn(move || {
        phys(&w, "walk");
        let from = (pos.x as f64, pos.y as f64);
        let t0 = Instant::now();
        loop {
            if !app.motion.owns(&label, token) {
                return;
            }
            let k = (t0.elapsed().as_secs_f64() * 1000.0 / ms).min(1.0);
            let e = k * k * (3.0 - 2.0 * k);
            let x = from.0 + (to.0 as f64 - from.0) * e;
            let y = from.1 + (to.1 as f64 - from.1) * e;
            let _ = w.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
            if k >= 1.0 {
                break;
            }
            std::thread::sleep(FRAME);
        }
        phys(&w, "stop");
        app.motion.release(&label, token);
    });
}
