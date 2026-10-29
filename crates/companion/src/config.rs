use serde::{Deserialize, Serialize};

pub const DEFAULT_PATH: &str = "companion.toml";

/// A relative path that doesn't exist from the working folder is looked up next to the exe,
/// so the app also finds its files when started from a shortcut or another folder.
pub fn resolve(path: &str) -> String {
    let p = std::path::Path::new(path);
    if p.is_absolute() || p.exists() {
        return path.to_string();
    }
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(p)))
        .filter(|candidate| candidate.exists())
        .map_or_else(|| path.to_string(), |c| c.to_string_lossy().into_owned())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Address of the SKSE exporter plugin.
    pub server: String,
    pub window: WindowConfig,
    pub compass: CompassConfig,
    pub map: MapConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: format!("127.0.0.1:{}", protocol::DEFAULT_PORT),
            window: WindowConfig::default(),
            compass: CompassConfig::default(),
            map: MapConfig::default(),
        }
    }
}

/// What the compass and the map's player arrow point along.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompassFollow {
    /// Where the camera (mouse) looks, like the in-game compass.
    #[default]
    Camera,
    /// Where the character's body faces (in third person, usually the move direction).
    Character,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CompassConfig {
    pub follow: CompassFollow,
}

impl CompassConfig {
    pub fn heading(&self, player: &protocol::Player) -> f32 {
        match self.follow {
            CompassFollow::Camera => player.camera_heading.unwrap_or(player.heading),
            CompassFollow::Character => player.heading,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowConfig {
    /// Top-left position in virtual desktop coordinates. Set x to your primary
    /// monitor's width (e.g. 1920) to open on a monitor to its right.
    pub x: Option<f32>,
    pub y: Option<f32>,
    pub width: f32,
    pub height: f32,
    pub fullscreen: bool,
    pub always_on_top: bool,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            x: None,
            y: None,
            width: 1280.0,
            height: 800.0,
            fullscreen: true,
            always_on_top: false,
        }
    }
}

/// Maps Tamriel worldspace coordinates onto the map image. The defaults
/// cover cells -57..61 (x) and -43..50 (y) at 4096 units per cell, which fits
/// the common full-Skyrim map exports; calibrate with two known landmarks if
/// your image is cropped differently.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MapConfig {
    /// PNG or JPEG of the Skyrim world map. Not shipped with the app.
    pub image: String,
    pub world_min_x: f32,
    pub world_max_x: f32,
    pub world_min_y: f32,
    pub world_max_y: f32,
    /// Initial zoom: 1.0 shows the whole map.
    pub zoom: f32,
    pub follow_player: bool,
}

impl Default for MapConfig {
    fn default() -> Self {
        Self {
            image: "assets/skyrim_map.png".into(),
            world_min_x: -57.0 * 4096.0,
            world_max_x: 62.0 * 4096.0,
            world_min_y: -43.0 * 4096.0,
            world_max_y: 51.0 * 4096.0,
            zoom: 4.0,
            follow_player: true,
        }
    }
}

impl Config {
    pub fn load_or_create(path: &str) -> Self {
        match std::fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text).unwrap_or_else(|e| {
                eprintln!("{path}: {e}; using defaults");
                Config::default()
            }),
            Err(_) => {
                let config = Config::default();
                if let Ok(text) = toml::to_string_pretty(&config) {
                    let _ = std::fs::write(path, text);
                }
                config
            }
        }
    }
}
