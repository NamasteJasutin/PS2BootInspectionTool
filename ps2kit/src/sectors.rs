//! Reads 2048-byte data sectors from a disc image, whether it is a plain ISO or a raw
//! 2352-byte-sector BIN (MODE1 or MODE2/XA form 1, as CD rips usually are). A `.cue` is
//! resolved to its BIN.

use crate::{Error, Result};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

pub struct SectorReader {
    file: File,
    pub sector_size: u64,
    pub data_offset: u64,
}

impl SectorReader {
    pub fn open(path: &Path) -> Result<Self> {
        let path = Self::resolve_cue(path)?;
        let mut file = File::open(&path)?;
        let mut head = [0u8; 16];
        let n = file.read(&mut head)?;
        const SYNC: [u8; 12] = [0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 0];
        let (sector_size, data_offset) = if n == 16 && head[..12] == SYNC {
            (2352, if head[15] == 2 { 24 } else { 16 })
        } else {
            (2048, 0)
        };
        Ok(Self { file, sector_size, data_offset })
    }

    /// `FILE "name" BINARY` in a cue sheet names the image next to it.
    fn resolve_cue(path: &Path) -> Result<PathBuf> {
        if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("cue")) {
            return Ok(path.to_path_buf());
        }
        let text = std::fs::read_to_string(path)?;
        for line in text.lines() {
            let line = line.trim();
            if let Some(rest) = line.strip_prefix("FILE ") {
                let name = rest.trim().trim_end_matches("BINARY").trim().trim_matches('"');
                let dir = path.parent().unwrap_or(Path::new("."));
                let named = dir.join(name);
                if named.exists() { return Ok(named) }
                // Renamed sets: the cue still names the original files. Take the .bin that
                // shares the cue's name, else the one marked as track 1, else the only one.
                let stem = path.file_stem().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
                let mut bins: Vec<PathBuf> = std::fs::read_dir(dir)?.filter_map(|e| e.ok().map(|e| e.path()))
                    .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("bin") || e.eq_ignore_ascii_case("img"))).collect();
                bins.sort();
                let lower = |p: &PathBuf| p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
                if let Some(b) = bins.iter().find(|b| lower(b) == format!("{stem}.bin") || lower(b) == format!("{stem}.img")) { return Ok(b.clone()) }
                if let Some(b) = bins.iter().find(|b| { let n = lower(b); n.contains("track 01") || n.contains("track 1)") || n.contains("track01") }) { return Ok(b.clone()) }
                if bins.len() == 1 { return Ok(bins.remove(0)) }
                return Err(Error::Corrupt(format!("cue sheet names {name}, which is not next to it")));
            }
        }
        Err(Error::Corrupt("cue sheet has no FILE line".into()))
    }

    pub fn is_raw(&self) -> bool { self.sector_size == 2352 }

    /// Reads `count` sectors starting at `lba`; short reads at the end are returned as is.
    pub fn read(&mut self, lba: usize, count: usize) -> Result<Vec<u8>> {
        let mut out = Vec::with_capacity(count * 2048);
        for i in 0..count {
            self.file.seek(SeekFrom::Start((lba + i) as u64 * self.sector_size + self.data_offset))?;
            let mut buf = vec![0u8; 2048];
            let got = self.file.read(&mut buf)?;
            if got == 0 { break }
            buf.truncate(got);
            out.extend(buf);
        }
        Ok(out)
    }
}

/// True for the file types the readers accept.
pub fn is_disc_image(path: &Path) -> bool {
    path.extension().is_some_and(|e| ["iso", "bin", "cue", "img"].iter().any(|x| e.eq_ignore_ascii_case(x)))
}
