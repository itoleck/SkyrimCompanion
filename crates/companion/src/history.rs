//! Per-character travel history: the trail walked on the world map and the spots where
//! notable things happened (kills, theft, standing stones, ...). Saved as JSON in
//! `%APPDATA%\Skyrim Companion\history\<character>.json` so it survives restarts.

use crate::icons::Icon;
use crate::runlog;
use protocol::{GameEvent, Snapshot, Vec3};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Only movement and events in this worldspace are kept; it is the one the map shows.
pub const WORLDSPACE: &str = "Tamriel";
/// A new trail point once the player has moved this far (about 3.5 m).
const TRAIL_STEP: f32 = 250.0;
/// Moving further than this between two samples is a fast travel or a loaded save,
/// so the trail is broken there instead of drawing a line across the map.
const TRAIL_JUMP: f32 = 3_000.0;
/// Oldest points are dropped beyond this (about 175 km of walking).
const TRAIL_MAX: usize = 50_000;
/// Events within 5 m of each other share one spot. One game unit is about 1.43 cm.
pub const SPOT_RADIUS: f32 = 5.0 / 0.0143;
const SAVE_EVERY: Duration = Duration::from_secs(15);
/// A history file that takes longer than this to load and is bigger than SLOW_LOAD_BYTES
/// loses the oldest half of its trail.
const SLOW_LOAD: Duration = Duration::from_secs(3);
const SLOW_LOAD_BYTES: u64 = 256 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TrailPoint {
    pub x: f32,
    pub y: f32,
    /// Not connected to the previous point (fast travel, a loaded save, ...).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub gap: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpotEntry {
    pub kind: String,
    pub text: String,
    pub days_passed: f32,
}

/// One or more events close together.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Spot {
    pub position: Vec3,
    pub entries: Vec<SpotEntry>,
}

impl Spot {
    pub fn has_kill(&self) -> bool {
        self.entries.iter().any(|e| e.kind == "kill")
    }

    /// Hover text: one line per event, collapsing repeats ("Killed Wolf ×3").
    pub fn summary(&self) -> String {
        let mut lines: Vec<(&str, usize)> = Vec::new();
        for e in &self.entries {
            match lines.iter_mut().find(|(t, _)| *t == e.text) {
                Some((_, n)) => *n += 1,
                None => lines.push((&e.text, 1)),
            }
        }
        lines
            .iter()
            .map(|(t, n)| {
                if *n > 1 {
                    format!("{t} ×{n}")
                } else {
                    (*t).to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// A marker the player put on the map (right-click on the map).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Pin {
    pub x: f32,
    pub y: f32,
    /// A map icon, or None for a plain point.
    pub icon: Option<Icon>,
    pub text: String,
}

impl Pin {
    pub fn position(&self) -> Vec3 {
        Vec3 {
            x: self.x,
            y: self.y,
            z: 0.0,
        }
    }
}

/// An edit to the player's own map markers.
#[derive(Debug, Clone, PartialEq)]
pub enum PinChange {
    Add(Pin),
    Update(usize, Pin),
    Remove(usize),
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Data {
    pub character: String,
    pub trail: Vec<TrailPoint>,
    pub spots: Vec<Spot>,
    pub pins: Vec<Pin>,
    /// (plugin session, seq) of the newest event already recorded.
    pub last_event: (u64, u64),
}

impl Data {
    /// Adds a trail point when the player has moved far enough. Returns true if it changed.
    pub fn track(&mut self, p: Vec3) -> bool {
        let point = |gap| TrailPoint {
            x: p.x,
            y: p.y,
            gap,
        };
        let Some(last) = self.trail.last() else {
            self.trail.push(point(true));
            return true;
        };
        let d2 = (last.x - p.x).powi(2) + (last.y - p.y).powi(2);
        if d2 < TRAIL_STEP * TRAIL_STEP {
            return false;
        }
        self.trail.push(point(d2 > TRAIL_JUMP * TRAIL_JUMP));
        if self.trail.len() > TRAIL_MAX {
            self.trail.drain(..self.trail.len() - TRAIL_MAX);
        }
        true
    }

    /// Drops the oldest half of the trail. Returns how many points were removed.
    pub fn trim_oldest_half(&mut self) -> usize {
        let n = self.trail.len() / 2;
        self.trail.drain(..n);
        // The new first point no longer follows on from anything.
        if let Some(first) = self.trail.first_mut() {
            first.gap = true;
        }
        n
    }

    /// Records the events not seen before. Returns true if anything changed.
    pub fn record(&mut self, session: u64, events: &[GameEvent]) -> bool {
        let mut changed = false;
        for e in events {
            let id = (session, e.seq);
            if id <= self.last_event || e.player != self.character {
                continue;
            }
            self.last_event = id;
            changed = true;
            if e.is_interior || e.worldspace != WORLDSPACE {
                continue;
            }
            let entry = SpotEntry {
                kind: e.kind.clone(),
                text: e.text.clone(),
                days_passed: e.days_passed,
            };
            match self
                .spots
                .iter_mut()
                .find(|s| dist2(s.position, e.position) <= SPOT_RADIUS * SPOT_RADIUS)
            {
                Some(spot) => spot.entries.push(entry),
                None => self.spots.push(Spot {
                    position: e.position,
                    entries: vec![entry],
                }),
            }
        }
        changed
    }
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1_000.0
}

pub fn dist2(a: Vec3, b: Vec3) -> f32 {
    (a.x - b.x).powi(2) + (a.y - b.y).powi(2)
}

/// The history of whichever character is being played, loaded and saved as they change.
#[derive(Default)]
pub struct History {
    pub data: Data,
    dirty: bool,
    last_save: Option<Instant>,
    pub error: Option<String>,
    /// How the last character load went, shown in Settings.
    pub load_stats: Option<LoadStats>,
}

/// Timing of reading and parsing a character's history file.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LoadStats {
    pub bytes: u64,
    pub read: Duration,
    pub parse: Duration,
    /// Trail points dropped because the load was too slow.
    pub trimmed: usize,
}

impl LoadStats {
    pub fn total(&self) -> Duration {
        self.read + self.parse
    }
}

impl History {
    pub fn dir() -> Option<PathBuf> {
        let base = std::env::var_os("APPDATA").or_else(|| std::env::var_os("XDG_CONFIG_HOME"))?;
        Some(PathBuf::from(base).join("Skyrim Companion").join("history"))
    }

    /// File for a character; characters can share a name, so this is per name.
    pub fn path(character: &str) -> Option<PathBuf> {
        let safe: String = character
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let safe = safe.trim();
        Some(Self::dir()?.join(format!("{}.json", if safe.is_empty() { "_" } else { safe })))
    }

    /// Reads a character's file, timing the read and the parse. A file that is both large and
    /// slow to load has the oldest half of its trail dropped (saved on the next save).
    fn load(character: &str) -> (Data, Option<LoadStats>) {
        let mut data = Data {
            character: character.to_string(),
            ..Data::default()
        };
        let Some(path) = Self::path(character) else {
            return (data, None);
        };
        let start = Instant::now();
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) => {
                runlog::line(&format!("history read {}: none yet ({e})", path.display()));
                return (data, None);
            }
        };
        let read = start.elapsed();
        let start = Instant::now();
        match serde_json::from_str::<Data>(&text) {
            Ok(saved) => {
                data = Data {
                    character: character.to_string(),
                    ..saved
                }
            }
            Err(e) => runlog::line(&format!(
                "history read {}: unreadable, starting empty ({e})",
                path.display()
            )),
        }
        let mut stats = LoadStats {
            bytes: text.len() as u64,
            read,
            parse: start.elapsed(),
            trimmed: 0,
        };
        runlog::line(&format!(
            "history read {}: {} bytes, {} trail points, {} event points, read {:.2} ms + parse {:.2} ms = {:.2} ms",
            path.display(),
            stats.bytes,
            data.trail.len(),
            data.spots.len(),
            ms(stats.read),
            ms(stats.parse),
            ms(stats.total()),
        ));
        if stats.total() > SLOW_LOAD && stats.bytes > SLOW_LOAD_BYTES {
            stats.trimmed = data.trim_oldest_half();
            runlog::line(&format!(
                "history trimmed {}: load took over {} s and the file is over {} KB, removed the {} oldest trail points, {} remain",
                path.display(),
                SLOW_LOAD.as_secs(),
                SLOW_LOAD_BYTES / 1024,
                stats.trimmed,
                data.trail.len(),
            ));
        }
        (data, Some(stats))
    }

    /// Feeds one snapshot: switches character files, extends the trail and records events.
    pub fn update(&mut self, snap: &Snapshot) {
        if !snap.in_game || snap.player.name.is_empty() {
            return;
        }
        if snap.player.name != self.data.character {
            self.save();
            let (data, stats) = Self::load(&snap.player.name);
            self.data = data;
            self.load_stats = stats;
            // Write the trimmed trail back so the next load is quicker.
            self.dirty = stats.is_some_and(|s| s.trimmed > 0);
        }
        let p = &snap.player;
        if !p.is_interior && p.worldspace == WORLDSPACE && self.data.track(p.position) {
            self.dirty = true;
        }
        if self.data.record(snap.session, &snap.events) {
            self.dirty = true;
        }
        if self.dirty && self.last_save.is_none_or(|t| t.elapsed() >= SAVE_EVERY) {
            self.save();
        }
    }

    /// Applies a map marker edit and saves straight away.
    pub fn change_pin(&mut self, change: PinChange) {
        let pins = &mut self.data.pins;
        match change {
            PinChange::Add(pin) => pins.push(pin),
            PinChange::Update(i, pin) if i < pins.len() => pins[i] = pin,
            PinChange::Remove(i) if i < pins.len() => {
                pins.remove(i);
            }
            _ => return,
        }
        self.dirty = true;
        self.save();
    }

    pub fn reset_trail(&mut self) {
        self.data.trail.clear();
        self.dirty = true;
        self.save();
    }

    pub fn reset_spots(&mut self) {
        self.data.spots.clear();
        self.dirty = true;
        self.save();
    }

    pub fn save(&mut self) {
        self.last_save = Some(Instant::now());
        if !self.dirty || self.data.character.is_empty() {
            return;
        }
        self.dirty = false;
        self.error = self.write().err();
    }

    fn write(&self) -> Result<(), String> {
        let path = Self::path(&self.data.character).ok_or("no APPDATA folder")?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let text = serde_json::to_string(&self.data).map_err(|e| e.to_string())?;
        // Write then rename, so a crash mid-write never leaves a half file.
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, text).map_err(|e| format!("{}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, &path).map_err(|e| format!("{}: {e}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(seq: u64, kind: &str, x: f32) -> GameEvent {
        GameEvent {
            seq,
            kind: kind.into(),
            text: format!("{kind} {seq}"),
            player: "Lydia".into(),
            position: Vec3 { x, y: 0.0, z: 0.0 },
            worldspace: WORLDSPACE.into(),
            ..Default::default()
        }
    }

    #[test]
    fn nearby_events_share_a_spot_and_repeats_are_skipped() {
        let mut d = Data {
            character: "Lydia".into(),
            ..Default::default()
        };
        let events = [
            event(1, "kill", 0.0),
            event(2, "steal", 100.0),
            event(3, "kill", 2_000.0),
        ];
        assert!(d.record(7, &events));
        assert_eq!(d.spots.len(), 2);
        assert!(d.spots[0].has_kill());
        assert_eq!(d.spots[0].entries.len(), 2);
        // The same events again (every snapshot repeats them) change nothing.
        assert!(!d.record(7, &events));
        // A new plugin session starts over at seq 1.
        assert!(d.record(8, &[event(1, "standing_stone", 5_000.0)]));
        assert_eq!(d.spots.len(), 3);
    }

    #[test]
    fn trail_breaks_on_fast_travel() {
        let mut d = Data::default();
        let at = |x| Vec3 { x, y: 0.0, z: 0.0 };
        assert!(d.track(at(0.0)));
        assert!(!d.track(at(100.0)));
        assert!(d.track(at(300.0)));
        assert!(d.track(at(90_000.0)));
        assert_eq!(
            d.trail.iter().map(|p| p.gap).collect::<Vec<_>>(),
            [true, false, true]
        );
    }

    #[test]
    fn trimming_keeps_the_newest_half() {
        let mut d = Data::default();
        for i in 0..10 {
            d.track(Vec3 {
                x: i as f32 * 1_000.0,
                y: 0.0,
                z: 0.0,
            });
        }
        assert_eq!(d.trim_oldest_half(), 5);
        assert_eq!(d.trail.len(), 5);
        assert_eq!(d.trail[0].x, 5_000.0);
        assert!(d.trail[0].gap);
    }

    /// Times parsing a history file at the trail limit. Run with
    /// `cargo test --release -p skyrim-companion profile_full_history -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn profile_full_history() {
        let mut d = Data {
            character: "Lydia".into(),
            ..Default::default()
        };
        for i in 0..TRAIL_MAX {
            let a = i as f32 * 0.01;
            d.track(Vec3 {
                x: a.cos() * (300.0 + i as f32),
                y: a.sin() * (300.0 + i as f32) * 3.0,
                z: 0.0,
            });
        }
        for i in 0..2_000u64 {
            d.record(1, &[event(i + 1, "kill", i as f32 * 1_000.0)]);
        }
        let start = Instant::now();
        let text = serde_json::to_string(&d).unwrap();
        let write = start.elapsed();
        let start = Instant::now();
        let back: Data = serde_json::from_str(&text).unwrap();
        let parse = start.elapsed();
        println!(
            "{} trail points, {} spots, {} KB: serialize {write:?}, parse {parse:?}",
            back.trail.len(),
            back.spots.len(),
            text.len() / 1024
        );
        // And the real files on this PC, through the same path a character load takes.
        let files = History::dir().and_then(|d| std::fs::read_dir(d).ok());
        for entry in files.into_iter().flatten().flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "json") {
                let name = path.file_stem().unwrap().to_string_lossy().into_owned();
                let (data, stats) = History::load(&name);
                println!("{name}: {} trail points, {stats:?}", data.trail.len());
            }
        }
    }

    #[test]
    fn data_round_trip() {
        let mut d = Data {
            character: "Lydia".into(),
            ..Default::default()
        };
        d.track(Vec3::default());
        d.record(1, &[event(1, "kill", 0.0)]);
        d.pins.push(Pin {
            x: 1.0,
            y: 2.0,
            icon: Some(Icon::Cave),
            text: "Loot later".into(),
        });
        d.pins.push(Pin::default());
        let text = serde_json::to_string(&d).unwrap();
        assert_eq!(serde_json::from_str::<Data>(&text).unwrap(), d);
        assert!(History::path("Lydia: Housecarl")
            .unwrap()
            .ends_with("Lydia_ Housecarl.json"));
    }
}
