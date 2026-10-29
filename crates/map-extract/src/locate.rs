//! Finds the Skyrim Special Edition `Data` folder when none is given on the command line:
//! Bethesda's and GOG's registry entries, then every Steam library.

use std::path::{Path, PathBuf};
use std::process::Command;

pub fn find_data_dir() -> Option<PathBuf> {
    candidates()
        .into_iter()
        .map(|game| game.join("Data"))
        .find(|data| data.join("Skyrim.esm").is_file())
}

fn candidates() -> Vec<PathBuf> {
    let mut found = Vec::new();
    for (key, value) in [
        (
            r"HKLM\SOFTWARE\WOW6432Node\Bethesda Softworks\Skyrim Special Edition",
            "Installed Path",
        ),
        (
            r"HKLM\SOFTWARE\WOW6432Node\GOG.com\Games\1711230643",
            "path",
        ),
    ] {
        found.extend(reg_value(key, value).map(PathBuf::from));
    }
    if let Some(steam) = reg_value(r"HKCU\Software\Valve\Steam", "SteamPath") {
        for library in steam_libraries(Path::new(&steam)) {
            found.push(library.join("steamapps/common/Skyrim Special Edition"));
        }
    }
    found
}

/// Reads a string value with `reg query` (no registry crate needed).
fn reg_value(key: &str, value: &str) -> Option<String> {
    let out = Command::new("reg")
        .args(["query", key, "/v", value])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text.lines().find(|l| l.contains("REG_SZ"))?;
    let data = line.split("REG_SZ").nth(1)?.trim();
    (!data.is_empty()).then(|| data.to_string())
}

/// The Steam folder itself plus every library listed in `steamapps/libraryfolders.vdf`.
fn steam_libraries(steam: &Path) -> Vec<PathBuf> {
    let mut libraries = vec![steam.to_path_buf()];
    if let Ok(vdf) = std::fs::read_to_string(steam.join("steamapps/libraryfolders.vdf")) {
        libraries.extend(library_paths(&vdf).into_iter().map(PathBuf::from));
    }
    libraries
}

/// `"path"		"E:\\Games\\SteamRoot\\Steam"` lines from libraryfolders.vdf.
fn library_paths(vdf: &str) -> Vec<String> {
    vdf.lines()
        .filter_map(|line| {
            let quoted: Vec<&str> = line.split('"').collect();
            // ["", "path", "\t\t", "E:\\...", ""]
            (quoted.len() >= 4 && quoted[1].eq_ignore_ascii_case("path"))
                .then(|| quoted[3].replace("\\\\", "\\"))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_library_folders() {
        let vdf = "\"libraryfolders\"\n{\n\t\"0\"\n\t{\n\t\t\"path\"\t\t\"C:\\\\Program Files (x86)\\\\Steam\"\n\t}\n\t\"1\"\n\t{\n\t\t\"path\"\t\t\"E:\\\\Games\\\\SteamRoot\\\\Steam\"\n\t}\n}\n";
        assert_eq!(
            library_paths(vdf),
            [r"C:\Program Files (x86)\Steam", r"E:\Games\SteamRoot\Steam"]
        );
    }
}
