`skyrim_map.png` goes here (not included; it is made from the game's own textures).
Build it with `cargo run --release -p map-extract`: it finds the game by itself and reads the terrain
LOD textures from its archives, or from loose xLODGen/DynDOLOD output when present.
In a release, double-click `create-map.cmd` instead.
