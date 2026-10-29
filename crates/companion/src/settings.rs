//! User preferences edited in Edit > Settings, saved per user in
//! `%APPDATA%\Skyrim Companion\settings.toml` (next to eframe's own state).
//! Machine setup (server address, window placement, map bounds) stays in `companion.toml`.

use protocol::QuestStatus;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Text size in points for the header and quest list; secondary text scales from it.
    pub font_size: f32,
    /// Font family name as shown in the picker, or None for the built-in font.
    pub font_family: Option<String>,
    /// The font file for `font_family`, so start-up doesn't need to scan the fonts folder.
    pub font_file: Option<PathBuf>,
    /// Face index inside a .ttc collection.
    pub font_index: u32,
    /// Which quests the quest list shows.
    pub quest_filter: Vec<QuestStatus>,
    /// With "Follow player" on: keep the player in the middle of the map and pan the map as they
    /// move. Off (the default), the map only re-centres when the player leaves the visible area.
    pub map_keep_centered: bool,
}

/// Default font family and its file in the Windows fonts folder (ships with Windows/Office).
const DEFAULT_FONT: (&str, &str) = ("Lucida Calligraphy", "LCALLIG.TTF");

impl Default for Settings {
    fn default() -> Self {
        // Use the default font only where it is installed; otherwise the built-in font.
        let file = std::env::var_os("WINDIR")
            .map(|w| PathBuf::from(w).join("Fonts").join(DEFAULT_FONT.1))
            .filter(|p| p.is_file());
        Self {
            font_size: 28.0,
            font_family: file.as_ref().map(|_| DEFAULT_FONT.0.to_string()),
            font_file: file,
            font_index: 0,
            quest_filter: vec![QuestStatus::Tracked, QuestStatus::Current],
            map_keep_centered: false,
        }
    }
}

impl Settings {
    pub fn path() -> Option<PathBuf> {
        let base = std::env::var_os("APPDATA").or_else(|| std::env::var_os("XDG_CONFIG_HOME"))?;
        Some(
            PathBuf::from(base)
                .join("Skyrim Companion")
                .join("settings.toml"),
        )
    }

    pub fn load() -> Self {
        Self::path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|text| toml::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> Result<(), String> {
        let path = Self::path().ok_or("no APPDATA folder")?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let text = toml::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// Installs the chosen font (falling back to egui's fonts for missing glyphs).
    pub fn apply_font(&self, ctx: &egui::Context) {
        let mut fonts = egui::FontDefinitions::default();
        if let Some(file) = &self.font_file {
            match std::fs::read(file) {
                Ok(bytes) => {
                    let mut data = egui::FontData::from_owned(bytes);
                    data.index = self.font_index;
                    fonts
                        .font_data
                        .insert("user".into(), std::sync::Arc::new(data));
                    if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
                        family.insert(0, "user".into());
                    }
                }
                Err(e) => eprintln!("{}: {e}; using the built-in font", file.display()),
            }
        }
        ctx.set_fonts(fonts);
    }
}

/// A font family installed on this PC: the file holding its regular face.
#[derive(Debug, Clone)]
pub struct InstalledFont {
    pub file: PathBuf,
    pub index: u32,
}

/// Scans the system and per-user font folders on a background thread.
pub fn scan_fonts() -> mpsc::Receiver<BTreeMap<String, InstalledFont>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(installed_fonts());
    });
    rx
}

fn font_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(windir) = std::env::var_os("WINDIR") {
        dirs.push(PathBuf::from(windir).join("Fonts"));
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        dirs.push(PathBuf::from(local).join("Microsoft/Windows/Fonts"));
    }
    dirs.push(PathBuf::from("/usr/share/fonts"));
    dirs
}

fn installed_fonts() -> BTreeMap<String, InstalledFont> {
    let mut found: BTreeMap<String, (InstalledFont, bool)> = BTreeMap::new();
    for dir in font_dirs() {
        visit(&dir, &mut |path| {
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .map(str::to_ascii_lowercase);
            if !matches!(ext.as_deref(), Some("ttf" | "otf" | "ttc")) {
                return;
            }
            let Ok(bytes) = std::fs::read(path) else {
                return;
            };
            let count = ttf_parser::fonts_in_collection(&bytes).unwrap_or(1);
            for index in 0..count {
                let Ok(face) = ttf_parser::Face::parse(&bytes, index) else {
                    continue;
                };
                let Some(family) = family_name(&face) else {
                    continue;
                };
                let regular = face.is_regular() && !face.is_bold() && !face.is_italic();
                // Keep the regular face; otherwise whatever face comes first.
                let entry = found.entry(family).or_insert_with(|| {
                    (
                        InstalledFont {
                            file: path.to_path_buf(),
                            index,
                        },
                        regular,
                    )
                });
                if regular && !entry.1 {
                    *entry = (
                        InstalledFont {
                            file: path.to_path_buf(),
                            index,
                        },
                        true,
                    );
                }
            }
        });
    }
    found.into_iter().map(|(k, (f, _))| (k, f)).collect()
}

fn family_name(face: &ttf_parser::Face) -> Option<String> {
    use ttf_parser::name_id;
    let names = face.names();
    [name_id::TYPOGRAPHIC_FAMILY, name_id::FAMILY]
        .iter()
        .find_map(|id| {
            names
                .into_iter()
                .filter(|n| n.name_id == *id && n.is_unicode())
                .find_map(|n| n.to_string())
        })
        .filter(|s| !s.trim().is_empty())
}

fn visit(dir: &Path, f: &mut dyn FnMut(&Path)) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            visit(&path, f);
        } else {
            f(&path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip() {
        let s = Settings {
            font_size: 18.0,
            font_family: Some("Georgia".into()),
            font_file: Some(PathBuf::from("C:/Windows/Fonts/georgia.ttf")),
            font_index: 0,
            quest_filter: vec![QuestStatus::Completed],
            map_keep_centered: true,
        };
        let text = toml::to_string_pretty(&s).unwrap();
        assert_eq!(toml::from_str::<Settings>(&text).unwrap(), s);
        // Missing fields fall back to defaults.
        assert_eq!(toml::from_str::<Settings>("").unwrap(), Settings::default());
    }
}
