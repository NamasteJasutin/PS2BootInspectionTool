//! The BIOS container (ROMDIR) and the LZ scheme used by the OSD program and its assets.

use crate::bytes::Bytes;
use crate::{Error, Result};
use std::collections::HashMap;

/// A ROMDIR archive: the BIOS image itself, and the asset bundles nested inside it.
#[derive(Clone)]
pub struct RomDir {
    /// The whole image. Modules are slices of it; see [`module`](Self::module).
    pub data: Vec<u8>,
    entries: HashMap<String, (usize, usize)>,
}

impl std::fmt::Debug for RomDir {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RomDir").field("bytes", &self.data.len()).field("modules", &self.entries.len()).finish()
    }
}

impl RomDir {
    /// Parses the ROMDIR table of a BIOS dump (or of a nested archive such as `TEXIMAGE`).
    /// Fails with [`Error::NotABios`] when there is no `RESET` entry.
    pub fn new(data: Vec<u8>) -> Result<Self> {
        let base = data
            .windows(10)
            .position(|w| w == b"RESET\0\0\0\0\0")
            .ok_or_else(|| Error::NotABios("no ROMDIR table".into()))?;
        let mut entries = HashMap::new();
        let (mut p, mut offset) = (base, 0usize);
        while p + 16 <= data.len() && data[p] != 0 {
            let name = data.cstr(p, 10);
            let size = data.u32(p + 12) as usize;
            if name != "-" && !entries.contains_key(&name) && offset.checked_add(size).is_some_and(|end| end <= data.len()) {
                entries.insert(name, (offset, size));
            }
            offset = offset.saturating_add((size + 15) & !15);
            p += 16;
        }
        Ok(Self { data, entries })
    }

    /// The bytes of the module called `name` (`OSDSYS`, `ROMVER`, ...).
    pub fn module(&self, name: &str) -> Result<&[u8]> {
        let &(o, n) = self.entries.get(name).ok_or_else(|| Error::MissingModule(name.into()))?;
        self.data.get(o..o + n).ok_or_else(|| Error::Corrupt(format!("module {name} lies outside the image")))
    }

    /// Names of every module in the table, in no particular order.
    pub fn names(&self) -> impl Iterator<Item = &String> { self.entries.keys() }
}

/// Decompresses the OSD LZ stream at `src[start..]`.
///
/// `u32` output size, then groups of 30 tokens, each group preceded by a big-endian word
/// whose top 30 bits flag literal (0) / match (1) and whose low 2 bits `n` set the
/// offset/length split of a 16-bit big-endian match word.
pub fn unpack(src: &[u8], start: usize) -> Result<Vec<u8>> {
    let trunc = || Error::Corrupt("truncated LZ stream".into());
    if start + 4 > src.len() {
        return Err(trunc());
    }
    let size = src.u32(start) as usize;
    if size > 64 << 20 {
        return Err(Error::Corrupt("implausible LZ stream size".into()));
    }
    let mut out = Vec::with_capacity(size);
    let mut pos = start + 4;
    let (mut desc, mut left, mut shift, mut mask) = (0u32, 0, 14, 0x3FFF);
    while out.len() < size {
        if left == 0 {
            if pos + 4 > src.len() {
                return Err(trunc());
            }
            desc = u32::from_be_bytes([src[pos], src[pos + 1], src[pos + 2], src[pos + 3]]);
            pos += 4;
            let n = (desc & 3) as usize;
            shift = 14 - n;
            mask = 0x3FFF >> n;
            left = 30;
        }
        if desc & 0x8000_0000 != 0 {
            if pos + 2 > src.len() {
                return Err(trunc());
            }
            let h = (src[pos] as usize) << 8 | src[pos + 1] as usize;
            pos += 2;
            let off = (h & mask) + 1;
            if off > out.len() {
                return Err(Error::Corrupt("LZ match before start of output".into()));
            }
            for _ in 0..(h >> shift) + 3 {
                out.push(out[out.len() - off]);
            }
        } else {
            if pos >= src.len() {
                return Err(trunc());
            }
            out.push(src[pos]);
            pos += 1;
        }
        desc <<= 1;
        left -= 1;
    }
    out.truncate(size);
    Ok(out)
}
