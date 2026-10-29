//! Wire format between the SKSE exporter plugin and the companion app.
//!
//! The plugin listens on `127.0.0.1:DEFAULT_PORT` and writes one JSON
//! [`Snapshot`] per line, a few times per second. Every field is optional on
//! the wire (`#[serde(default)]`) so either side can add fields without
//! breaking the other.

use serde::{Deserialize, Serialize};

pub const DEFAULT_PORT: u16 = 47_311;
pub const PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Snapshot {
    pub protocol: u32,
    /// Game runtime version, e.g. "1.6.1170".
    pub game_version: String,
    /// The SKSE plugin's own version (from version.txt), e.g. "0.1.4".
    pub plugin_version: String,
    /// False while in the main menu or a loading screen.
    pub in_game: bool,
    pub player: Player,
    pub time: GameTime,
    pub crime: Crime,
    pub quests: Vec<Quest>,
    /// Quest targets and other points shown on the compass and map.
    pub markers: Vec<Marker>,
    /// World map locations (cities, caves, ruins, ...) the player has found, in Tamriel.
    pub places: Vec<Place>,
    /// Identifies one run of the plugin; event `seq` numbers restart when it changes.
    pub session: u64,
    /// The most recent notable events (oldest first), repeated in every snapshot.
    pub events: Vec<GameEvent>,
}

/// Something notable the player did, e.g. a kill or a theft.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct GameEvent {
    /// Increases by one per event within a plugin `session`.
    pub seq: u64,
    /// "kill", "steal", "pickpocket", "crime", "standing_stone", "shrine", "discovered",
    /// "cleared", "dragon_soul", "level_up". A string so new kinds don't break older viewers.
    pub kind: String,
    /// Human-readable description, e.g. "Killed Bandit Marauder".
    pub text: String,
    /// The character it happened to.
    pub player: String,
    pub position: Vec3,
    pub worldspace: String,
    pub is_interior: bool,
    pub days_passed: f32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Place {
    pub name: String,
    /// The game's map marker type in snake case, e.g. "city", "cave", "nordic_ruin".
    /// Kept as a string so new types from mods or DLC don't break older viewers.
    pub kind: String,
    pub position: Vec3,
    /// Visited (fast-travel enabled), as opposed to only seen on the map.
    pub discovered: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Player {
    pub name: String,
    pub level: u16,
    /// XP earned towards the next level.
    pub xp: f32,
    /// XP needed for the next level (the game's `levelThreshold`).
    pub xp_next: f32,
    pub position: Vec3,
    /// Facing angle in radians, game convention: 0 = north, clockwise.
    pub heading: f32,
    /// Camera (mouse look) direction, same convention as `heading`. Differs from
    /// `heading` in third person. `None` from older plugins.
    pub camera_heading: Option<f32>,
    pub gold: i32,
    /// Current inventory weight and carry capacity; over capacity means encumbered.
    pub carry_weight: f32,
    pub max_carry_weight: f32,
    /// Worldspace editor ID, e.g. "Tamriel". Empty when in an interior.
    pub worldspace: String,
    pub cell: String,
    pub location: String,
    pub is_interior: bool,
}

impl Player {
    /// Load as a fraction of carry capacity (above 1.0 = encumbered).
    pub fn load_fraction(&self) -> f32 {
        if self.max_carry_weight > 0.0 {
            self.carry_weight / self.max_carry_weight
        } else {
            0.0
        }
    }

    pub fn xp_fraction(&self) -> f32 {
        if self.xp_next > 0.0 {
            (self.xp / self.xp_next).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct GameTime {
    /// 0.0..24.0
    pub hour: f32,
    /// 1-based day of month.
    pub day: u32,
    /// 0-based month index, 0 = Morning Star.
    pub month: u32,
    /// Year of the 4th Era.
    pub year: u32,
    /// 0 = Sundas.
    pub day_of_week: u32,
    pub days_passed: f32,
    pub timescale: f32,
}

pub const MONTHS: [&str; 12] = [
    "Morning Star",
    "Sun's Dawn",
    "First Seed",
    "Rain's Hand",
    "Second Seed",
    "Midyear",
    "Sun's Height",
    "Last Seed",
    "Hearthfire",
    "Frostfall",
    "Sun's Dusk",
    "Evening Star",
];

pub const WEEKDAYS: [&str; 7] = [
    "Sundas", "Morndas", "Tirdas", "Middas", "Turdas", "Fredas", "Loredas",
];

impl GameTime {
    pub fn clock(&self) -> String {
        let total_minutes = (self.hour.rem_euclid(24.0) * 60.0) as u32;
        let (h, m) = (total_minutes / 60, total_minutes % 60);
        let (h12, ampm) = match h {
            0 => (12, "AM"),
            1..=11 => (h, "AM"),
            12 => (12, "PM"),
            _ => (h - 12, "PM"),
        };
        format!("{h12}:{m:02} {ampm}")
    }

    pub fn date(&self) -> String {
        let weekday = WEEKDAYS
            .get(self.day_of_week as usize)
            .copied()
            .unwrap_or("?");
        let month = MONTHS.get(self.month as usize).copied().unwrap_or("?");
        format!(
            "{weekday}, {} of {month}, 4E {}",
            ordinal(self.day),
            self.year
        )
    }

    pub fn period(&self) -> &'static str {
        match self.hour as u32 {
            5..=7 => "Dawn",
            8..=11 => "Morning",
            12..=16 => "Afternoon",
            17..=19 => "Dusk",
            20..=23 => "Evening",
            _ => "Night",
        }
    }
}

fn ordinal(n: u32) -> String {
    let suffix = match (n % 10, n % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Crime {
    pub bounties: Vec<Bounty>,
}

impl Crime {
    pub fn is_wanted(&self) -> bool {
        self.bounties.iter().any(|b| b.total() > 0)
    }

    pub fn total(&self) -> i32 {
        self.bounties.iter().map(Bounty::total).sum()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Bounty {
    /// Hold or faction display name, e.g. "Whiterun".
    pub faction: String,
    pub violent: i32,
    pub non_violent: i32,
}

impl Bounty {
    pub fn total(&self) -> i32 {
        self.violent + self.non_violent
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Quest {
    pub form_id: u32,
    pub editor_id: String,
    pub name: String,
    pub kind: QuestKind,
    /// The quest is selected (tracked) in the journal.
    pub tracked: bool,
    pub status: QuestStatus,
    pub objectives: Vec<Objective>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum QuestKind {
    Main,
    Faction,
    Daedric,
    Side,
    #[default]
    Misc,
    Dlc,
}

impl QuestKind {
    pub fn label(self) -> &'static str {
        match self {
            QuestKind::Main => "Main Quest",
            QuestKind::Faction => "Faction",
            QuestKind::Daedric => "Daedric",
            QuestKind::Side => "Side Quest",
            QuestKind::Misc => "Miscellaneous",
            QuestKind::Dlc => "DLC",
        }
    }
}

/// Where a quest stands for the player, in the order the quest list shows them.
#[derive(
    Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
#[serde(rename_all = "snake_case")]
pub enum QuestStatus {
    /// Running and selected in the journal.
    Tracked,
    /// Running and in the journal, but not selected.
    #[default]
    Current,
    /// Finished (or failed).
    Completed,
    /// A named quest the player has not been given yet.
    Unknown,
}

impl QuestStatus {
    pub const ALL: [QuestStatus; 4] = [
        QuestStatus::Tracked,
        QuestStatus::Current,
        QuestStatus::Completed,
        QuestStatus::Unknown,
    ];

    pub fn label(self) -> &'static str {
        match self {
            QuestStatus::Tracked => "Tracked",
            QuestStatus::Current => "Current",
            QuestStatus::Completed => "Completed",
            QuestStatus::Unknown => "Unknown",
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Objective {
    pub index: u16,
    pub text: String,
    pub state: ObjectiveState,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ObjectiveState {
    #[default]
    Active,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Marker {
    pub name: String,
    pub kind: MarkerKind,
    pub position: Vec3,
    /// Form ID of the quest this marker belongs to, if any.
    pub quest: Option<u32>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MarkerKind {
    #[default]
    QuestTarget,
    Location,
    Custom,
}

pub fn encode_line(s: &Snapshot) -> serde_json::Result<String> {
    let mut line = serde_json::to_string(s)?;
    line.push('\n');
    Ok(line)
}

pub fn decode_line(line: &str) -> serde_json::Result<Snapshot> {
    serde_json::from_str(line.trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let s = Snapshot {
            protocol: PROTOCOL_VERSION,
            in_game: true,
            player: Player {
                name: "Dovahkiin".into(),
                level: 12,
                xp: 50.0,
                xp_next: 200.0,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_eq!(decode_line(&encode_line(&s).unwrap()).unwrap(), s);
    }

    #[test]
    fn places_round_trip() {
        let s = decode_line(
            r#"{"places":[{"name":"Bleak Falls Barrow","kind":"nordic_ruin","position":{"x":1.0,"y":2.0,"z":3.0},"discovered":true}]}"#,
        )
        .unwrap();
        assert_eq!(s.places[0].kind, "nordic_ruin");
        assert!(s.places[0].discovered);
        assert_eq!(decode_line(&encode_line(&s).unwrap()).unwrap(), s);
    }

    #[test]
    fn tolerates_missing_and_extra_fields() {
        let s = decode_line(r#"{"player":{"level":3},"future_field":1}"#).unwrap();
        assert_eq!(s.player.level, 3);
    }

    #[test]
    fn clock_and_date() {
        let t = GameTime {
            hour: 13.5,
            day: 17,
            month: 7,
            year: 201,
            day_of_week: 3,
            ..Default::default()
        };
        assert_eq!(t.clock(), "1:30 PM");
        assert_eq!(t.date(), "Middas, 17th of Last Seed, 4E 201");
    }

    #[test]
    fn wanted() {
        let c = Crime {
            bounties: vec![Bounty {
                faction: "Whiterun".into(),
                violent: 0,
                non_violent: 40,
            }],
        };
        assert!(c.is_wanted());
        assert_eq!(c.total(), 40);
    }
}
