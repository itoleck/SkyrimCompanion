//! Minimal reader for Skyrim Special Edition archives (BSA version 105), enough to pull the
//! terrain LOD textures out of `Skyrim - Textures*.bsa` without unpacking anything to disk.

use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

const INCLUDE_DIR_NAMES: u32 = 0x1;
const INCLUDE_FILE_NAMES: u32 = 0x2;
const COMPRESSED: u32 = 0x4;
const EMBED_FILE_NAMES: u32 = 0x100;
/// Set in a file's size field when its compression differs from the archive default.
const TOGGLE_COMPRESSION: u32 = 0x4000_0000;

/// One file inside an archive.
#[derive(Debug, Clone)]
pub struct Entry {
    /// Lower-case file name without the folder.
    pub name: String,
    offset: u64,
    size: u32,
    compressed: bool,
}

pub struct Archive {
    pub path: PathBuf,
    flags: u32,
    pub entries: Vec<Entry>,
}

impl Archive {
    /// Lists the files in `folder` (e.g. `textures\terrain\tamriel`, any case). Returns None for
    /// archives that aren't version 105 or don't have that folder.
    pub fn open(path: &Path, folder: &str) -> std::io::Result<Option<Archive>> {
        let mut f = BufReader::new(File::open(path)?);
        let mut header = [0u8; 36];
        f.read_exact(&mut header)?;
        let u32_at = |o: usize| u32::from_le_bytes(header[o..o + 4].try_into().unwrap());
        if &header[0..4] != b"BSA\0" || u32_at(4) != 105 {
            return Ok(None);
        }
        let (flags, folder_count, total_file_name_len) = (u32_at(12), u32_at(16), u32_at(28));
        if flags & INCLUDE_DIR_NAMES == 0 || flags & INCLUDE_FILE_NAMES == 0 {
            return Ok(None);
        }

        // Folder records: hash u64, file count u32, padding u32, offset u64.
        let mut counts = Vec::with_capacity(folder_count as usize);
        for _ in 0..folder_count {
            let mut rec = [0u8; 24];
            f.read_exact(&mut rec)?;
            counts.push(u32::from_le_bytes(rec[8..12].try_into().unwrap()));
        }
        // File record blocks: the folder's name, then hash u64, size u32, offset u32 per file.
        let mut files = Vec::new(); // (folder matches, size, offset)
        for count in counts {
            let mut len = [0u8; 1];
            f.read_exact(&mut len)?;
            let mut name = vec![0u8; len[0] as usize];
            f.read_exact(&mut name)?;
            let name = String::from_utf8_lossy(&name);
            let wanted = name.trim_end_matches('\0').eq_ignore_ascii_case(folder);
            for _ in 0..count {
                let mut rec = [0u8; 16];
                f.read_exact(&mut rec)?;
                let size = u32::from_le_bytes(rec[8..12].try_into().unwrap());
                let offset = u32::from_le_bytes(rec[12..16].try_into().unwrap());
                files.push((wanted, size, offset));
            }
        }
        if !files.iter().any(|(wanted, ..)| *wanted) {
            return Ok(None);
        }
        // File names, null-terminated, in record order.
        let mut names = vec![0u8; total_file_name_len as usize];
        f.read_exact(&mut names)?;
        let entries = names
            .split(|&b| b == 0)
            .zip(files)
            .filter(|(_, (wanted, ..))| *wanted)
            .map(|(name, (_, size, offset))| Entry {
                name: String::from_utf8_lossy(name).to_ascii_lowercase(),
                offset: offset as u64,
                size: size & !TOGGLE_COMPRESSION,
                compressed: (flags & COMPRESSED != 0) != (size & TOGGLE_COMPRESSION != 0),
            })
            .collect();
        Ok(Some(Archive {
            path: path.to_path_buf(),
            flags,
            entries,
        }))
    }

    /// Reads and, if needed, decompresses (LZ4 frame) one file.
    pub fn read(&self, entry: &Entry) -> std::io::Result<Vec<u8>> {
        let mut f = File::open(&self.path)?;
        f.seek(SeekFrom::Start(entry.offset))?;
        let mut raw = vec![0u8; entry.size as usize];
        f.read_exact(&mut raw)?;
        let mut data = &raw[..];
        if self.flags & EMBED_FILE_NAMES != 0 {
            let skip = 1 + *data.first().unwrap_or(&0) as usize;
            data = data.get(skip..).unwrap_or_default();
        }
        if !entry.compressed {
            return Ok(data.to_vec());
        }
        let original = data
            .get(..4)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap()) as usize)
            .unwrap_or(0);
        let mut out = Vec::with_capacity(original);
        lz4_flex::frame::FrameDecoder::new(&data[4.min(data.len())..]).read_to_end(&mut out)?;
        Ok(out)
    }
}
