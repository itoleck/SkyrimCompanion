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

The map is built from your game's terrain LOD textures and lines up with world coordinates exactly:

```powershell
cargo run --release -p map-extract                 # finds the game (Steam, GOG, Bethesda registry)
cargo run --release -p map-extract -- "D:/Games/Skyrim Special Edition/Data"   # or name the Data folder
```

Tiles come from loose xLODGen/DynDOLOD output when present, otherwise straight from the game's archives
(`Skyrim - Textures7.bsa`); an xLODGen terrain heightmap adds water to the sea. This writes
`assets/skyrim_map.png` (git-ignored). Run `map-extract --help` for resolution and crop options; the default
crop matches the default `[map]` bounds. Rerun it after regenerating LOD.

## Package a release

`.\create-release.ps1` builds the apps and writes `SkyrimCompanion.zip` with `App\` (skyrim-companion.exe,
map-extract.exe, create-map.cmd, README.txt) and `Data\SKSE\Plugins\SkyrimCompanionExporter.dll`. Build the
plugin first. The map image is left out unless you pass `-IncludeMap`, since it is made from the game's own
textures; players double-click `create-map.cmd` to build it from their own install.

Each run adds 1 to the patch number in `version.txt` (`0.1.0` → `0.1.1` → …) and stamps it as the file and
product version of `skyrim-companion.exe`, `map-extract.exe` (`build-support/version_resource.rs`, used by both
build scripts) and the SKSE plugin (`skse-plugin/CMakeLists.txt`). A failed run puts the old number back; edit
`version.txt` to change the major or minor number. The plugin is rebuilt by `skse-plugin\build-local.cmd`
(this PC's Visual Studio + vcpkg setup, git-ignored) when it exists; otherwise build it yourself after the bump.
The version shows in the window title, at the bottom of **Edit → Settings** (with the connected plugin's
version), in `run.log`, in the plugin's SKSE log, and in Explorer's file properties.

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

With **Follow player** on, the map re-centres only when the player walks out of view, so you can pan around
freely (the default). **Edit → Settings → Map → Keep the player centred** instead keeps the player in the
middle at all times and moves the map under them.

Right-click the map and pick **Create Path to Here** for an animated path from the player (or, indoors, from
where they were last seen outside) to that spot, or to a marker when you right-click one; **Clear Path**
removes it.

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
