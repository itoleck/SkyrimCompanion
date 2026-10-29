# Architecture

## Decision: SKSE plugin exporter + Rust viewer

```
┌──────────── Skyrim SE process ────────────┐        ┌──── skyrim-companion.exe (Rust) ────┐
│ SkyrimCompanionExporter.dll (SKSE, C++)    │  TCP   │ client thread: reconnect + parse     │
│  game thread task: read PlayerCharacter,   │ ─────▶ │ egui UI: clock, XP, wanted, compass, │
│  Calendar, factions, quests → JSON         │ 127.0.0.1:47311, one JSON line / 250 ms │ map, quest journal (second monitor) │
└────────────────────────────────────────────┘        └──────────────────────────────────────┘
```

### Options considered

**A. Raw external memory reading from Rust** (`OpenProcess` + `ReadProcessMemory`, pointer chains)

- Pro: no game-side install, pure Rust.
- Con: offsets change with every runtime (1.5.97, 1.6.640, 1.6.1170 …) and Steam updates silently break it.
  Quests, objectives and crime gold live in linked lists, hash maps and `BSFixedString` pools, so each
  field needs a hand-maintained pointer chain. Reads race with the game thread and can see torn data.
  Some anti-tamper and overlay tools also flag handle-opening processes.

**B. Small SKSE plugin that exports data (chosen)**

- Pro: CommonLibSSE-NG + Address Library resolve addresses across runtimes, and give typed access to
  `PlayerCharacter`, `Calendar`, `TESFaction` crime gold and `TESQuest` objectives. Data is read on the game
  thread via the SKSE task queue, so it is consistent. The Rust app stays a simple, safe viewer.
- Con: a C++ plugin to build (MSVC + vcpkg) and install into `Data/SKSE/Plugins`. You already launch
  through `skse64_loader.exe`, so the runtime side is free.

**Transport:** localhost TCP with newline-delimited JSON. Easier to debug than shared memory
(`telnet 127.0.0.1 47311` shows the stream), lets the app restart independently of the game, and costs
nothing at 4 updates/second. Shared memory can replace it later behind the same `protocol` crate.

## Components

| Path | What |
|---|---|
| `crates/protocol` | Serde types for the snapshot. The single source of truth for field names. |
| `crates/companion` | The Windows app (eframe/egui). Config in `companion.toml`. |
| `crates/mock-skse` | Fake exporter on the same port, so the UI runs without Skyrim. |
| `skse-plugin/` | The C++ SKSE plugin (CommonLibSSE-NG via vcpkg). |

## Data sources in the plugin

| UI | Game source |
|---|---|
| Time of day / date | `RE::Calendar` (hour, day, month, year, weekday) |
| Level, XP to next level | `PlayerCharacter::GetLevel()`, `PlayerSkills::Data::{xp, levelThreshold}` |
| Wanted status | Every crime-tracking `TESFaction`: `GetCrimeGoldViolent/NonViolent()` |
| Quests | Every `TESQuest` with a status: tracked (`IsActive()`) / current (running, objectives displayed) / completed (`IsCompleted()`, or stopped with displayed objectives) / unknown (named, typed, objectives, never shown). `<Global=…>` and `<Alias=…>` tokens are replaced with the global's value and the alias's reference name; the app strips any it still finds (`crates/companion/src/quests.rs`). |
| Map, compass heading | Player position, `data.angle.z`, worldspace (`Tamriel` = 0x3C) |
| Compass/map quest markers | `PlayerCharacter::GetQuestTargets()` (the game's own compass list) → `TESQuestTarget::GetTargetRef()`; targets in another cell/worldspace use the first door on the target's `teleportPath`. Player's custom map marker too. |
| Map places (cities, caves, ruins, …) | Map marker refs (`ExtraMapMarker`) in Tamriel's persistent cell, visible ones only; type → icon in `crates/companion/src/icons.rs`. Rebuilt every 5 s. |
| Map event points | `skse-plugin/src/events.cpp`: `ActorKill` (killer = player), `TESContainerChangedEvent` into the player from something `WouldBeStealing` (not while bartering), `ItemsPickpocketed`, `TESActivateEvent` on activators named "… Stone"/"Shrine…", `LocationDiscovery`, `LocationCleared`, `DragonSoulsGained`, `LevelIncrease`, and bounty increases. The last 100 are repeated in every snapshot with `session` + `seq` so the app saves each once. |
| Compass direction | Camera root node world rotation (mouse look), or `data.angle.z` with `[compass] follow = "character"` |

## Map image

`crates/map-extract` builds `assets/skyrim_map.png` from the game's own terrain LOD textures
(`Data/textures/terrain/tamriel/tamriel.4.<x>.<y>.dds`, plus `_n` normal maps for hill shading). Tiles sit on
the cell grid, so the image matches world coordinates with no calibration. If an xLODGen terrain heightmap is
present, the sea is painted from it. The image is derived from game files, so it is generated locally and
git-ignored, never shipped.

## Known gaps / next steps

1. Objective text with `<Alias=…>` tokens is sent raw; needs alias name substitution.
2. `map-extract` reads loose LOD files only (xLODGen/DynDOLOD output). With vanilla LOD inside the BSAs, unpack
   `textures/terrain/tamriel` first, or add BSA reading.
3. Lakes and rivers are not tinted as water (only the sea, from the heightmap).
