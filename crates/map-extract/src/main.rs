//! Stitches the Tamriel terrain LOD textures (`Data/textures/terrain/tamriel/tamriel.<level>.<x>.<y>.dds`)
//! into one top-down map image. The tiles are laid out on the cell grid, so the result lines up with
//! world coordinates exactly and needs no landmark calibration.
//!
//! Tiles are read from loose files (xLODGen/DynDOLOD output) or, failing that, straight out of the
//! game's archives (`Skyrim - Textures7.bsa` has the vanilla ones); loose files win, as in the game.
//! With no Data folder given, the game is found through the registry and Steam libraries.
//! If the folder also has an xLODGen terrain heightmap (`Tamriel.Terrain.HeightMap.*.dds`), everything
//! below sea level is painted as water; otherwise the sea shows the LOD seabed texture.

mod bsa;
mod locate;

use image::{imageops, Rgba, RgbaImage};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const USAGE: &str = "\
usage: map-extract [Skyrim Data dir] [options]

  With no Data dir, the Skyrim Special Edition install is found automatically.

  --out <file>          output PNG (default assets/skyrim_map.png)
  --level <n>           LOD level to read: 4, 8, 16 or 32 (default 4, the sharpest)
  --px-per-cell <n>     output resolution (default 40)
  --cells <x0> <x1> <y0> <y1>
                        inclusive cell range to keep (default -57 61 -43 50, which matches
                        the [map] defaults in companion.toml)
  --sea-level <h>       water height in heightmap units (default -250, matched to the
                        Haafingar coastline; only used when a heightmap is present)
  --archives-only       ignore loose LOD files and use only the game's archives";

/// One cell is 4096 world units.
const CELL: i32 = 4096;
/// Drawn where no tile exists (open sea / outside the worldspace).
const BACKGROUND: Rgba<u8> = Rgba([28, 36, 44, 255]);

struct Args {
    data: PathBuf,
    out: PathBuf,
    level: i32,
    px_per_cell: u32,
    cells: [i32; 4],
    sea_level: f32,
    archives_only: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut it = std::env::args().skip(1);
    let mut args = Args {
        data: PathBuf::new(),
        out: PathBuf::from("assets/skyrim_map.png"),
        level: 4,
        px_per_cell: 40,
        cells: [-57, 61, -43, 50],
        sea_level: -250.0,
        archives_only: false,
    };
    let num = |it: &mut dyn Iterator<Item = String>, what: &str| -> Result<i32, String> {
        it.next()
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| format!("{what} needs a number"))
    };
    while let Some(a) = it.next() {
        match a.as_str() {
            "--out" => args.out = it.next().ok_or("--out needs a path")?.into(),
            "--level" => args.level = num(&mut it, "--level")?,
            "--px-per-cell" => args.px_per_cell = num(&mut it, "--px-per-cell")?.max(1) as u32,
            "--cells" => {
                for i in 0..4 {
                    args.cells[i] = num(&mut it, "--cells")?;
                }
            }
            "--sea-level" => args.sea_level = num(&mut it, "--sea-level")? as f32,
            "--archives-only" => args.archives_only = true,
            "-h" | "--help" => return Err(USAGE.into()),
            _ if args.data.as_os_str().is_empty() => args.data = a.into(),
            _ => return Err(format!("unexpected argument {a}\n\n{USAGE}")),
        }
    }
    if args.data.as_os_str().is_empty() {
        args.data = locate::find_data_dir().ok_or_else(|| {
            format!("could not find Skyrim Special Edition; pass its Data folder.\n\n{USAGE}")
        })?;
        println!("found the game: {}", args.data.display());
    }
    Ok(args)
}

fn main() {
    println!("map-extract {}", env!("COMPANION_VERSION"));
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    if let Err(e) = run(&args) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn run(args: &Args) -> Result<(), String> {
    let dir = args.data.join("textures/terrain/tamriel");
    let tiles = Tiles::scan(&args.data, !args.archives_only);
    let [x0, x1, y0, y1] = args.cells;
    let p = args.px_per_cell;
    let width = (x1 - x0 + 1) as u32 * p;
    let height = (y1 - y0 + 1) as u32 * p;
    let mut map = RgbaImage::from_pixel(width, height, BACKGROUND);

    let tile_px = args.level as u32 * p;
    let (mut drawn, mut failed) = (0, 0);
    for name in tiles.files.keys() {
        let Some((tx, ty)) = tile_coords(Path::new(name), args.level) else {
            continue;
        };
        // A tile covers cells tx..tx+level, ty..ty+level. Image rows run north to south.
        let left = (tx - x0) * p as i32;
        let top = (y1 + 1 - (ty + args.level)) * p as i32;
        if left + tile_px as i32 <= 0
            || top + tile_px as i32 <= 0
            || left >= width as i32
            || top >= height as i32
        {
            continue;
        }
        match tiles.read(name).and_then(|b| decode_dds(&b)) {
            Ok(mut tile) => {
                // The matching normal map gives the terrain relief; skip shading if it is missing.
                let normals = format!("tamriel.{}.{tx}.{ty}_n.dds", args.level);
                if let Ok(n) = tiles.read(&normals).and_then(|b| decode_dds(&b)) {
                    hillshade(&mut tile, &n);
                }
                let tile =
                    imageops::resize(&tile, tile_px, tile_px, imageops::FilterType::Triangle);
                imageops::replace(&mut map, &tile, left as i64, top as i64);
                drawn += 1;
            }
            Err(e) => {
                eprintln!("skipping {name}: {e}");
                failed += 1;
            }
        }
    }
    if drawn == 0 {
        return Err(format!(
            "no tamriel.{}.*.dds tiles found in {} or the game's archives",
            args.level,
            dir.display()
        ));
    }

    match HeightMap::find(&dir).filter(|_| !args.archives_only) {
        Some(hm) => {
            paint_water(&mut map, &hm, args);
            println!("water from {}", hm.name);
        }
        None => println!("no terrain heightmap found; sea shows the LOD seabed"),
    }

    if let Some(parent) = args.out.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    map.save(&args.out)
        .map_err(|e| format!("{}: {e}", args.out.display()))?;
    println!(
        "wrote {} ({width}x{height}) from {drawn} tiles, {failed} skipped",
        args.out.display()
    );
    println!("[map] bounds for companion.toml:");
    println!("world_min_x = {:.1}", (x0 * CELL) as f32);
    println!("world_max_x = {:.1}", ((x1 + 1) * CELL) as f32);
    println!("world_min_y = {:.1}", (y0 * CELL) as f32);
    println!("world_max_y = {:.1}", ((y1 + 1) * CELL) as f32);
    Ok(())
}

/// Parses `tamriel.<level>.<x>.<y>.dds`, ignoring normal maps (`_n`) and seasonal variants (`.win`).
fn tile_coords(path: &Path, level: i32) -> Option<(i32, i32)> {
    let name = path.file_name()?.to_str()?.to_ascii_lowercase();
    let rest = name.strip_prefix("tamriel.")?.strip_suffix(".dds")?;
    let mut parts = rest.split('.');
    let (l, x, y) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() || l.parse::<i32>().ok()? != level {
        return None;
    }
    Some((x.parse().ok()?, y.parse().ok()?))
}

/// The terrain LOD textures, by lower-case file name: loose files override archives, and later
/// archives override earlier ones (mods' archives over the game's own).
struct Tiles {
    archives: Vec<bsa::Archive>,
    files: HashMap<String, Source>,
}

enum Source {
    Loose(PathBuf),
    Packed(usize, bsa::Entry),
}

impl Tiles {
    fn scan(data: &Path, loose_files: bool) -> Self {
        let mut tiles = Tiles {
            archives: Vec::new(),
            files: HashMap::new(),
        };
        let mut bsas: Vec<PathBuf> = std::fs::read_dir(data)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("bsa")))
            .collect();
        // The game's own archives first, so mods' archives replace their tiles.
        let vanilla = |p: &PathBuf| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.to_ascii_lowercase().starts_with("skyrim - "))
        };
        bsas.sort_by_key(|p| (!vanilla(p), p.clone()));
        for path in bsas {
            match bsa::Archive::open(&path, r"textures\terrain\tamriel") {
                Ok(Some(archive)) => {
                    let index = tiles.archives.len();
                    for e in &archive.entries {
                        tiles
                            .files
                            .insert(e.name.clone(), Source::Packed(index, e.clone()));
                    }
                    println!(
                        "{} terrain textures in {}",
                        archive.entries.len(),
                        path.display()
                    );
                    tiles.archives.push(archive);
                }
                Ok(None) => {}
                Err(e) => eprintln!("skipping {}: {e}", path.display()),
            }
        }
        let loose = data.join("textures/terrain/tamriel");
        let mut count = 0;
        let listing = std::fs::read_dir(&loose)
            .into_iter()
            .filter(|_| loose_files);
        for entry in listing.flatten().flatten() {
            if let Some(name) = entry.file_name().to_str() {
                tiles
                    .files
                    .insert(name.to_ascii_lowercase(), Source::Loose(entry.path()));
                count += 1;
            }
        }
        if count > 0 {
            println!("{count} loose terrain textures in {}", loose.display());
        }
        tiles
    }

    fn read(&self, name: &str) -> Result<Vec<u8>, String> {
        match self.files.get(name) {
            Some(Source::Loose(path)) => std::fs::read(path).map_err(|e| e.to_string()),
            Some(Source::Packed(i, entry)) => self.archives[*i]
                .read(entry)
                .map_err(|e| format!("{}: {e}", self.archives[*i].path.display())),
            None => Err("not found".into()),
        }
    }
}

/// Uncompressed 24/32-bit RGB(A) DDS (the vanilla terrain LOD textures), using the channel masks.
fn decode_uncompressed(bytes: &[u8], width: usize, height: usize) -> Result<RgbaImage, String> {
    let u32_at = |o: usize| u32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
    let bpp = u32_at(88) as usize / 8;
    if bpp != 3 && bpp != 4 {
        return Err(format!("unsupported {}-bit uncompressed DDS", bpp * 8));
    }
    let masks = [u32_at(92), u32_at(96), u32_at(100), u32_at(104)];
    let data = bytes
        .get(128..128 + width * height * bpp)
        .ok_or("truncated")?;
    let channel = |px: u32, mask: u32| -> u8 {
        if mask == 0 {
            return 255;
        }
        let v = (px & mask) >> mask.trailing_zeros();
        (v * 255 / (mask >> mask.trailing_zeros())) as u8
    };
    let mut img = RgbaImage::new(width as u32, height as u32);
    for (px, chunk) in img.pixels_mut().zip(data.chunks_exact(bpp)) {
        let mut v = [0u8; 4];
        v[..bpp].copy_from_slice(chunk);
        let raw = u32::from_le_bytes(v);
        *px = Rgba(masks.map(|m| channel(raw, m)));
    }
    Ok(img)
}

type Decoder = fn(&[u8], usize, usize, &mut [u32]) -> Result<(), &'static str>;

/// Decodes the top mip level of a block-compressed DDS texture (BC1/BC2/BC3/BC5/BC7).
fn decode_dds(bytes: &[u8]) -> Result<RgbaImage, String> {
    if bytes.len() < 128 || &bytes[0..4] != b"DDS " {
        return Err("not a DDS file".into());
    }
    let u32_at = |o: usize| u32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
    let (height, width) = (u32_at(12) as usize, u32_at(16) as usize);
    let (decode, data, bc5): (Decoder, &[u8], bool) = match &bytes[84..88] {
        b"DXT1" => (texture2ddecoder::decode_bc1, &bytes[128..], false),
        b"DXT3" => (texture2ddecoder::decode_bc2, &bytes[128..], false),
        b"DXT5" => (texture2ddecoder::decode_bc3, &bytes[128..], false),
        b"ATI2" | b"BC5U" => (texture2ddecoder::decode_bc5, &bytes[128..], true),
        b"DX10" if bytes.len() >= 148 => {
            let data = &bytes[148..];
            match u32_at(128) {
                70..=72 => (texture2ddecoder::decode_bc1 as Decoder, data, false),
                73..=75 => (texture2ddecoder::decode_bc2, data, false),
                76..=78 => (texture2ddecoder::decode_bc3, data, false),
                82..=84 => (texture2ddecoder::decode_bc5, data, true),
                97..=99 => (texture2ddecoder::decode_bc7, data, false),
                f => return Err(format!("unsupported DXGI format {f}")),
            }
        }
        _ if u32_at(80) & 0x40 != 0 => return decode_uncompressed(bytes, width, height),
        other => {
            return Err(format!(
                "unsupported format {:?}",
                String::from_utf8_lossy(other)
            ))
        }
    };
    let mut pixels = vec![0u32; width * height];
    decode(data, width, height, &mut pixels)?;
    let mut img = RgbaImage::new(width as u32, height as u32);
    for (px, &c) in img.pixels_mut().zip(&pixels) {
        // texture2ddecoder packs pixels as B, G, R, A bytes.
        let [b, g, r, a] = c.to_le_bytes();
        // BC5 normal maps store only x and y; rebuild z.
        let b = if bc5 {
            let (x, y) = (r as f32 / 127.5 - 1.0, g as f32 / 127.5 - 1.0);
            (((1.0 - x * x - y * y).max(0.0).sqrt() + 1.0) * 127.5) as u8
        } else {
            b
        };
        *px = Rgba([r, g, b, a]);
    }
    Ok(img)
}

/// An xLODGen terrain heightmap: `Tamriel.Terrain.HeightMap.<x0>.<y0>.<x1>.<y1>.<min>.<max>.dds`,
/// 16-bit greyscale covering cells x0..=x1, y0..=y1 with row 0 at the north edge.
struct HeightMap {
    name: String,
    cells: [i32; 4],
    range: [f32; 2],
    width: usize,
    height: usize,
    data: Vec<u16>,
}

impl HeightMap {
    fn find(dir: &Path) -> Option<Self> {
        std::fs::read_dir(dir).ok()?.flatten().find_map(|e| {
            let name = e.file_name().to_str()?.to_string();
            let rest = name
                .to_ascii_lowercase()
                .strip_prefix("tamriel.terrain.heightmap.")?
                .strip_suffix(".dds")?
                .to_string();
            let v: Vec<f32> = rest
                .split('.')
                .map(|s| s.parse().ok())
                .collect::<Option<_>>()?;
            let [x0, y0, x1, y1, lo, hi] = v[..] else {
                return None;
            };
            let bytes = std::fs::read(e.path()).ok()?;
            let u32_at = |o: usize| u32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
            if bytes.len() < 128 || u32_at(88) != 16 {
                return None; // not 16 bits per pixel
            }
            let (height, width) = (u32_at(12) as usize, u32_at(16) as usize);
            let data: Vec<u16> = bytes[128..]
                .chunks_exact(2)
                .take(width * height)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            (data.len() == width * height).then(|| HeightMap {
                name,
                cells: [x0 as i32, x1 as i32, y0 as i32, y1 as i32],
                range: [lo, hi],
                width,
                height,
                data,
            })
        })
    }

    /// Height at a position given in cells (fractional), or None outside the heightmap.
    fn at(&self, cx: f32, cy: f32) -> Option<f32> {
        let [x0, x1, y0, y1] = self.cells.map(|c| c as f32);
        let u = (cx - x0) / (x1 - x0 + 1.0);
        let v = (y1 + 1.0 - cy) / (y1 - y0 + 1.0);
        if !(0.0..1.0).contains(&u) || !(0.0..1.0).contains(&v) {
            return None;
        }
        let raw = self.data
            [(v * self.height as f32) as usize * self.width + (u * self.width as f32) as usize];
        Some(self.range[0] + raw as f32 / 65535.0 * (self.range[1] - self.range[0]))
    }
}

/// Paints everything below sea level as water, darker with depth, keeping a hint of the seabed.
fn paint_water(map: &mut RgbaImage, hm: &HeightMap, args: &Args) {
    const SHALLOW: [f32; 3] = [62.0, 92.0, 108.0];
    const DEEP: [f32; 3] = [22.0, 38.0, 54.0];
    let [x0, _, _, y1] = args.cells;
    let p = args.px_per_cell as f32;
    for (ox, oy, px) in map.enumerate_pixels_mut() {
        let cx = x0 as f32 + (ox as f32 + 0.5) / p;
        let cy = (y1 + 1) as f32 - (oy as f32 + 0.5) / p;
        let Some(h) = hm.at(cx, cy) else { continue };
        if h >= args.sea_level {
            continue;
        }
        let depth = ((args.sea_level - h) / 300.0).clamp(0.0, 1.0);
        for c in 0..3 {
            let water = SHALLOW[c] + (DEEP[c] - SHALLOW[c]) * depth;
            px[c] = (water * 0.85 + px[c] as f32 * 0.15) as u8;
        }
    }
}

/// Darkens slopes facing away from a north-west light and brightens those facing it, like a
/// shaded relief map. Normals are model space: x east, y north, z up.
fn hillshade(tile: &mut RgbaImage, normals: &RgbaImage) {
    const LIGHT: [f32; 3] = [-0.5, 0.5, 0.707];
    if tile.dimensions() != normals.dimensions() {
        return;
    }
    for (px, n) in tile.pixels_mut().zip(normals.pixels()) {
        let v = [0, 1, 2].map(|i| n[i] as f32 / 127.5 - 1.0);
        let shade = (v[0] * LIGHT[0] + v[1] * LIGHT[1] + v[2] * LIGHT[2]) / LIGHT[2];
        // 1.15 lifts the generally dark LOD textures a little.
        let k = (shade * 1.15).clamp(0.35, 1.8);
        for c in 0..3 {
            px[c] = (px[c] as f32 * k).min(255.0) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tile_names() {
        let p = |s: &str| tile_coords(Path::new(s), 4);
        assert_eq!(p("tamriel.4.-12.8.dds"), Some((-12, 8)));
        assert_eq!(p("Tamriel.4.0.-64.DDS"), Some((0, -64)));
        assert_eq!(p("tamriel.4.0.0_n.dds"), None);
        assert_eq!(p("tamriel.4.0.0.win.dds"), None);
        assert_eq!(p("tamriel.8.0.0.dds"), None);
    }
}
