//! 夜里整理: while you are away in the evening, the pet does what can be undone
//! (puts old files away, boxes the agents' old sessions) and only lists what
//! cannot (installers, copies: those wait for you). Next time you are at the
//! desk it says what it did; the storeroom's 「昨晚」 view has each thing with
//! its "put back".
//!
//! It learns from you: a file you put back is never put away again (see
//! drawer's keep list), and a place whose night put-away you undid is only
//! looked at, not touched, for a month.
//!
//! Every day it also writes down how much of the drive is free, and from the
//! last weeks reckons how long the space will last.

use crate::drawer::{self, Disk, Place, Stowed};
use serde::{Deserialize, Serialize};

const DAY_MS: u64 = 86_400_000;
const MIN_MS: u64 = 60_000;
/// Away at least this long in the evening: the night's work can start.
const AWAY_MS: u64 = 30 * MIN_MS;
/// A place is put away only with at least this many old files.
const STOW_FROM: usize = 5;
/// An agent's old sessions are boxed from this size.
const PACK_FROM: u64 = 256 << 20;
/// A put-away you undid keeps the night's hands off that place this long.
const HANDS_OFF_DAYS: u64 = 30;

/// What one place has waiting for you: installers and copies.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Left {
    pub place: String,
    pub n: u32,
    pub bytes: u64,
    /// Old files it would have put away but did not (you undid it once, or the night only looked).
    pub old: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Boxed {
    pub file: String,
    pub agent: String,
    pub files: u32,
    pub bytes: u64,
    pub packed: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Night {
    pub at: u64,
    pub stowed: Vec<Stowed>,
    pub boxed: Vec<Boxed>,
    pub left: Vec<Left>,
    pub disk: Option<Disk>,
    /// Days until the drive is full at the recent pace; None: it is not filling up.
    pub days_left: Option<u64>,
    /// It only looked (the night's work was off, or it caught up in the morning).
    pub looked_only: bool,
    /// The morning line has been said.
    pub told: bool,
}

impl Night {
    pub fn did_something(&self) -> bool {
        !self.stowed.is_empty() || !self.boxed.is_empty()
    }
    pub fn waiting(&self) -> bool {
        self.left.iter().any(|l| l.n > 0 || l.old > 0)
    }
}

// ---------------------------------------------------------------- the drive, day by day

/// One sample a day: (day number, free bytes).
fn samples(sh: &crate::server::Shared) -> Vec<(u64, u64)> {
    sh.db.kv_get("disk:samples").and_then(|v| serde_json::from_str(&v).ok()).unwrap_or_default()
}

/// Write down today's free space (once a day; the last 60 days are kept).
pub fn sample(sh: &crate::server::Shared, now: u64, d: &Disk) {
    let day = now / DAY_MS;
    let mut s = samples(sh);
    if s.last().is_some_and(|(d0, _)| *d0 == day) {
        s.pop();
    }
    s.push((day, d.free));
    s.retain(|(d0, _)| day.saturating_sub(*d0) <= 60);
    sh.db.kv_set("disk:samples", &serde_json::to_string(&s).unwrap_or_default());
}

/// Days until `free` is gone at the pace of the last four weeks of samples
/// (a least-squares line through them). None with under a week of samples, or
/// when the drive is not filling up (losing under 100 MB a day).
pub fn forecast(samples: &[(u64, u64)], free: u64) -> Option<u64> {
    let last = samples.last()?.0;
    let s: Vec<(f64, f64)> = samples.iter().filter(|(d, _)| last - d <= 28).map(|(d, f)| (*d as f64, *f as f64)).collect();
    if s.len() < 4 || last - samples.iter().find(|(d, _)| last - d <= 28)?.0 < 7 {
        return None;
    }
    let n = s.len() as f64;
    let (mx, my) = (s.iter().map(|p| p.0).sum::<f64>() / n, s.iter().map(|p| p.1).sum::<f64>() / n);
    let var: f64 = s.iter().map(|p| (p.0 - mx).powi(2)).sum();
    if var == 0.0 {
        return None;
    }
    let slope = s.iter().map(|p| (p.0 - mx) * (p.1 - my)).sum::<f64>() / var;
    if slope > -(100.0 * 1_048_576.0) {
        return None;
    }
    Some((free as f64 / -slope).floor() as u64)
}

/// The drive the user's folders are on, and how long it will last.
pub fn drive(sh: &crate::server::Shared) -> (Option<Disk>, Option<u64>) {
    let d = Place::Desktop.dir().and_then(|p| drawer::disk(&p));
    let left = d.as_ref().and_then(|d| forecast(&samples(sh), d.free));
    (d, left)
}

// ---------------------------------------------------------------- the night's work

/// The last night's report.
pub fn last(sh: &crate::server::Shared) -> Option<Night> {
    sh.db.kv_get("night:last").and_then(|v| serde_json::from_str(&v).ok())
}

fn save(sh: &crate::server::Shared, n: &Night) {
    sh.db.kv_set("night:last", &serde_json::to_string(n).unwrap_or_default());
}

/// Hands off `place` at night for a while: you undid what it put away there.
pub fn hands_off(sh: &crate::server::Shared, place: &str) {
    sh.db.kv_set(&format!("night:off:{place}"), &crate::now_ms().to_string());
}

fn hands_off_now(sh: &crate::server::Shared, place: Place, now: u64) -> bool {
    sh.db
        .kv_get(&format!("night:off:{}", place.id()))
        .and_then(|v| v.parse::<u64>().ok())
        .is_some_and(|t| now.saturating_sub(t) < HANDS_OFF_DAYS * DAY_MS)
}

/// Do the night's work (`act`: put away and box; else only look) and keep the report.
pub fn run(sh: &crate::server::Shared, act: bool) -> Night {
    let now = crate::now_ms();
    let home = sh.paths.home.clone();
    let mut night = Night { at: now, looked_only: !act, ..Default::default() };
    for place in [Place::Desktop, Place::Downloads] {
        let Some(dir) = place.dir().filter(|d| d.is_dir()) else { continue };
        let before = drawer::scan(&home, place, &dir, now);
        // Installers and copies stay out where you can see them: whether they go is yours to say.
        // A copy's original stays beside it, or the copy would no longer show as one.
        let originals: std::collections::HashSet<&str> = before.copies.iter().filter_map(|f| f.same.as_deref()).collect();
        let spare: std::collections::HashSet<&str> = before.installers.iter().chain(&before.copies).map(|f| f.path.as_str()).collect();
        let quiet: Vec<String> = before
            .old
            .iter()
            .filter(|f| !spare.contains(f.path.as_str()) && !originals.contains(f.name.as_str()))
            .map(|f| f.path.clone())
            .collect();
        let mut old = quiet.len() as u32;
        if act && quiet.len() >= STOW_FROM && !hands_off_now(sh, place, now) {
            if let Ok(done) = drawer::stow(&home, place, &dir, Some(&quiet), now) {
                if let Some(s) = done.stowed {
                    drawer::noted(sh, place, s.files);
                    night.stowed.push(s);
                }
                old = done.skipped;
            }
        }
        let after = drawer::scan(&home, place, &dir, now);
        let spare: Vec<&drawer::File> = after.installers.iter().chain(&after.copies).collect();
        night.left.push(Left { place: place.id().into(), n: spare.len() as u32, bytes: spare.iter().map(|f| f.bytes).sum(), old });
    }
    if act {
        for (id, _) in crate::storeroom::old_by_agent().into_iter().filter(|(_, b)| *b >= PACK_FROM) {
            if let Ok(b) = crate::storeroom::pack(&home, id, |_, _| {}) {
                night.boxed.push(Boxed { file: b.file, agent: b.agent, files: b.files, bytes: b.bytes, packed: b.packed });
            }
        }
    }
    let (disk, days_left) = drive(sh);
    if let Some(d) = &disk {
        sample(sh, now, d);
    }
    night.disk = disk;
    night.days_left = days_left;
    // Nothing done, nothing waiting, and the drive not running out soon: nothing to tell.
    let tight = night.disk.as_ref().is_some_and(drawer::short) || night.days_left.is_some_and(|d| d < 60);
    night.told = !night.did_something() && !night.waiting() && !tight;
    save(sh, &night);
    night
}

/// Looked at every tick of the think loop. At night, with you away a while:
/// the night's work (once per evening). In the morning, if it never ran (the
/// machine was off): a look only. Then, with you back at the desk: the line.
pub fn tick(sh: &crate::server::Shared, hour: u8, idle_ms: u64, present: bool, quiet: bool) {
    use std::sync::atomic::{AtomicBool, Ordering};
    static BUSY: AtomicBool = AtomicBool::new(false);
    let now = crate::now_ms();
    let on = sh.hub.lock().unwrap().cfg.features.night;
    let last_at = last(sh).map(|n| n.at).unwrap_or(0);
    let evening = !(5..22).contains(&hour);
    let job = if evening && idle_ms >= AWAY_MS && now.saturating_sub(last_at) > 14 * 3_600_000 {
        Some(on)
    } else if present && (6..12).contains(&hour) && now.saturating_sub(last_at) > 30 * 3_600_000 {
        Some(false)
    } else {
        None
    };
    if let Some(act) = job {
        if !BUSY.swap(true, Ordering::SeqCst) {
            let sh = sh.clone();
            std::thread::spawn(move || {
                run(&sh, act);
                BUSY.store(false, Ordering::SeqCst);
            });
        }
        return;
    }
    // The morning line: you are here and active, and there is something to tell.
    if !present || idle_ms > MIN_MS || quiet || BUSY.load(Ordering::SeqCst) {
        return;
    }
    let Some(mut n) = last(sh) else { return };
    if n.told || now.saturating_sub(n.at) < 20 * MIN_MS {
        return;
    }
    let Some((pet, who)) = drawer::asker(sh) else { return };
    n.told = true;
    save(sh, &n);
    let key = if n.did_something() {
        "night_done"
    } else if n.waiting() {
        "night_left"
    } else {
        "night_disk"
    };
    // Yesterday's notes, still in hand, go into the same line.
    let text = if crate::todo::carried(&sh.db, now) > 0 {
        crate::todo::carried_said(&sh.db, now);
        if key == "night_disk" {
            format!("{}{}", crate::voice::line(who, key, 0), crate::voice::todo(who, "carry", "", ""))
        } else {
            crate::voice::todo(who, key, "", "")
        }
    } else {
        crate::voice::line(who, key, 0)
    };
    let _ = sh.ws_tx.send(crate::msg("pet.act", &serde_json::json!({ "pet": pet, "act": "offer", "text": text, "why": "night", "offer": "storeroom:night" })));
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = 1 << 30;

    #[test]
    fn the_drive_lasts_as_long_as_its_pace_allows() {
        // Losing 1 GB a day with 30 GB free: about a month.
        let s: Vec<(u64, u64)> = (0..14).map(|d| (100 + d, (44 - d) * GB)).collect();
        assert_eq!(forecast(&s, 31 * GB), Some(31));
        // Steady (or growing): no end in sight.
        let flat: Vec<(u64, u64)> = (0..14).map(|d| (100 + d, 50 * GB + (d % 2) * 10_000_000)).collect();
        assert_eq!(forecast(&flat, 50 * GB), None);
        // Under a week of samples: too early to say.
        let few: Vec<(u64, u64)> = (0..5).map(|d| (100 + d, (44 - d) * GB)).collect();
        assert_eq!(forecast(&few, 40 * GB), None);
    }
}
