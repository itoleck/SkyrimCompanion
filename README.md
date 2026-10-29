# Skyrim Companion

A second-monitor companion for Skyrim Special Edition, written in Rust. Shows time of day, level and XP to
the next level, wanted status, a compass, a world map with your position, and your quest journal.

A small SKSE plugin reads the game state and streams it over `127.0.0.1:47311`; the Rust app displays it.
See [ARCHITECTURE.md](ARCHITECTURE.md) for why.

## Try the UI without Skyrim

```powershell
cargo run -p mock-skse          # terminal 1: fake game data
cargo run -p skyrim-companion   # terminal 2: the app
```

## Build and install the SKSE plugin (Windows)

Prerequisites: Visual Studio 2022 (Desktop C++), CMake ≥ 3.21, Ninja, and vcpkg with `VCPKG_ROOT` set.
Address Library for SKSE Plugins must be installed in the game (Nexus mod 32444).

```powershell
cd skse-plugin
cmake --preset release -DSKYRIM_DATA="E:/Games/SteamRoot/Steam/steamapps/common/Skyrim Special Edition/Data"
cmake --build --preset release
```

With `SKYRIM_DATA` set the DLL is copied to `Data/SKSE/Plugins` after each build. Run the build from a
"x64 Native Tools Command Prompt for VS 2022" so Ninja finds MSVC.

## Run

1. Start Skyrim through SKSE:
   `"E:\Games\SteamRoot\Steam\steamapps\common\Skyrim Special Edition\skse64_loader.exe"`
2. Start `skyrim-companion.exe` (`cargo run --release -p skyrim-companion`).
3. The badge in the top right turns **Live** once a save is loaded.

Plugin log: `Documents\My Games\Skyrim Special Edition\SKSE\SkyrimCompanionExporter.log`.

## Map image

The map is built from your game's terrain LOD textures (loose files from xLODGen/DynDOLOD) and lines up with
world coordinates exactly:

```powershell
cargo run --release -p map-extract -- "E:/Games/SteamRoot/Steam/steamapps/common/Skyrim Special Edition/Data"
```

This writes `assets/skyrim_map.png` (git-ignored). Run `map-extract --help` for resolution and crop options;
the default crop matches the default `[map]` bounds. Rerun it after regenerating LOD.

## Configuration (`companion.toml`, created on first run)

```toml
server = "127.0.0.1:47311"

[window]
x = 1920.0          # put the window on a monitor to the right of a 1080p primary
y = 0.0
fullscreen = true   # default; F11 or File → Full screen toggles it
always_on_top = false

[compass]
follow = "camera"   # "camera" = mouse look (like the in-game compass), "character" = body facing

[map]
image = "assets/skyrim_map.png"
follow_player = true
```

Fonts (size and family; default Lucida Calligraphy 28 pt) are set in **Edit → Settings** and saved per user in
`%APPDATA%\Skyrim Companion\settings.toml`.

The map keeps a trail of where each character has walked in Tamriel (dark blue = most recent, fading to
light blue) and small points where notable things happened: red for kills, gold for theft, pickpocketing,
new bounties, standing stones, shrines, discoveries, cleared locations, dragon souls and level-ups. Events
within 5 m share one point; hover it, or stand within 5 m of it in the game, to see what happened. This is
saved per character name in `%APPDATA%\Skyrim Companion\history\<name>.json`; **Edit → Settings** has
buttons to reset the trail or the event points of the character being played.

Right-click the map to add your own marker there: a note and one of the map icons (or a plain point).
Right-click a marker to edit or delete it. Hover a marker, or come within 25 m of it, to read its note.
Markers are saved in the same per-character history file.

## Layout

```
crates/protocol    shared snapshot types (JSON over TCP)
crates/companion   the Windows app (eframe/egui)
crates/mock-skse   fake exporter for UI development
crates/map-extract builds assets/skyrim_map.png from the game's terrain LOD
skse-plugin/       C++ SKSE plugin (CommonLibSSE-NG)
```
