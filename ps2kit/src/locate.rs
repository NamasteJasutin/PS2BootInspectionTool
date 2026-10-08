//! Finds the opening's data tables inside an OSD program by what they contain rather than
//! by address, so that BIOS builds other than the one studied load without a per-version
//! address list. Every search is validated; a miss names the table. The layouts themselves
//! are implementation details of [`bios`](crate::bios), [`logo`](crate::logo) and
//! [`sound`](crate::sound); only [`ProgramImage`] is public.

use crate::bytes::Bytes;
use crate::rom::unpack;
use crate::{Error, Format, Result};

/// A program image (an unpacked EE executable) with the address it is loaded at.
#[derive(Clone)]
pub struct ProgramImage {
    data: Vec<u8>,
    base: usize,
}

impl std::fmt::Debug for ProgramImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProgramImage").field("base", &format_args!("{:#x}", self.base)).field("bytes", &self.data.len()).finish()
    }
}

impl ProgramImage {
    /// Wraps program bytes that would sit in memory from virtual address `base` on.
    #[must_use]
    pub fn new(data: Vec<u8>, base: usize) -> Self { Self { data, base } }
    /// The program bytes.
    #[must_use]
    pub fn data(&self) -> &[u8] { &self.data }
    /// Virtual address of `data()[0]`.
    #[must_use]
    pub fn base(&self) -> usize { self.base }
    /// Offset into `data` of virtual address `vaddr`, if it lies inside the image.
    #[must_use]
    pub fn at(&self, vaddr: usize) -> Option<usize> { vaddr.checked_sub(self.base).filter(|&o| o < self.data.len()) }
    /// Whether `vaddr` lies inside the image.
    #[must_use]
    pub fn contains(&self, vaddr: usize) -> bool { self.at(vaddr).is_some() }
    /// Offset of the first occurrence of `pattern`.
    #[must_use]
    pub fn find(&self, pattern: &[u8]) -> Option<usize> { self.data.windows(pattern.len()).position(|w| w == pattern) }
    /// Offsets of every occurrence of `pattern` that starts on a multiple of `step` bytes.
    #[must_use]
    pub fn find_all(&self, pattern: &[u8], step: usize) -> Vec<usize> {
        (0..self.data.len().saturating_sub(pattern.len())).step_by(step.max(1)).filter(|&i| &self.data[i..i + pattern.len()] == pattern).collect()
    }
    /// `n` little-endian floats from offset `off` (zeros past the end).
    #[must_use]
    pub fn f32s(&self, off: usize, n: usize) -> Vec<f32> { (0..n).map(|i| self.data.f32(off + i * 4)).collect() }

    /// Unpacks a stub + LZ program (the stream is found by trying offsets) or takes the
    /// PT_LOAD segment of a plain ELF. `compressed_base` is where the stub puts the payload.
    pub fn from_module(module: &[u8], compressed_base: usize) -> Result<Self> {
        if module.len() < 0x34 || &module[..4] != b"\x7fELF" {
            return Err(Error::Corrupt(Format::Bios, "OSD module is not an ELF".into()));
        }
        for off in (0x80..module.len().min(0x8000)).step_by(16) {
            let size = module.u32(off) as usize;
            if !(0x10000..=0x200000).contains(&size) {
                continue;
            }
            if let Ok(data) = unpack(module, off) {
                // The stream should account for nearly all of the file after it.
                if data.len() == size && size > (module.len() - off) {
                    return Ok(Self { data, base: compressed_base });
                }
            }
        }
        // Plain ELF: first PT_LOAD, zero-padded to its memory size.
        let (phoff, phentsize, phnum) = (module.u32(0x1C) as usize, module.u16(0x2A) as usize, module.u16(0x2C) as usize);
        for i in 0..phnum {
            let o = phoff + i * phentsize;
            if module.u32(o) != 1 {
                continue;
            }
            let (off, vaddr, filesz, memsz) = (module.u32(o + 4) as usize, module.u32(o + 8) as usize, module.u32(o + 16) as usize, module.u32(o + 20) as usize);
            let Some(segment) = module.get(off..off.saturating_add(filesz)) else { break };
            if memsz > 64 << 20 {
                return Err(Error::Corrupt(Format::Bios, "implausible PT_LOAD memory size".into()));
            }
            let mut data = segment.to_vec();
            data.resize(memsz.max(filesz), 0);
            return Ok(Self { data, base: vaddr });
        }
        Err(Error::Corrupt(Format::Bios, "OSD module has neither an LZ stream nor a PT_LOAD segment".into()))
    }
}

fn missing(what: &str) -> Error { Error::Corrupt(Format::Bios, format!("could not locate the {what} in this BIOS version")) }

/// Offsets (into [`ProgramImage::data`]) of the opening's tables in an OSDSYS image, plus
/// the asset names the texture descriptors refer to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OsdLayout {
    /// Asset names in table order (`None` for a null entry).
    pub asset_names: Vec<Option<String>>,
    /// The texture descriptors, 0xF0 bytes each.
    pub texture_table: usize,
    /// How many descriptors the texture table holds.
    pub texture_count: usize,
    /// 21 records x 6 `(column, row)` pairs.
    pub slot_table: usize,
    /// 14 fill factors of the growing tower.
    pub fill_table: usize,
    /// 14 size factors of the growing tower.
    pub size_table: usize,
    /// 14 x 9 base positions `(x, y, z, 0)`.
    pub tower_positions: usize,
    /// Four orb colours `(r, g, b, 0)`.
    pub orb_colours: usize,
    /// Five glass cube positions.
    pub cube_positions: usize,
    /// Five warning-scene prism positions.
    pub prism_positions: usize,
}

impl OsdLayout {
    /// Finds every table by content. A miss names the table in [`Error::Corrupt`].
    pub fn discover(img: &ProgramImage) -> Result<Self> {
        let d = &img.data[..];
        // Asset table: 16-byte entries {name*, addr, size, flags}; entry 0 names "FONTM".
        let fontm = img.find(b"FONTM\0").ok_or_else(|| missing("asset name strings"))?;
        let ptr = ((img.base + fontm) as u32).to_le_bytes();
        let asset_table = img.find_all(&ptr, 4).into_iter().find(|&i| d.u32(i + 16 + 4) != 0xFFFF_FFFF && img.contains(d.u32(i + 16) as usize)).ok_or_else(|| missing("asset table"))?;
        let mut names: Vec<Option<String>> = Vec::new();
        let mut p = asset_table;
        while d.u32(p) != 0xFFFF_FFFF && names.len() < 256 {
            let s = d.u32(p) as usize;
            names.push(if s == 0 { None } else { img.at(s).map(|o| d.cstr(o, 16)) });
            p += 16;
        }
        let index_of = |n: &str| names.iter().position(|x| x.as_deref() == Some(n));
        let sce = index_of("TEXOSCE").ok_or_else(|| missing("TEXOSCE asset"))?;
        // Texture descriptors: 0xF0-byte records; the first is TEXOSCE, 256x64, format 5.
        let texture_table = (0..d.len().saturating_sub(0xF0)).step_by(16)
            .find(|&q| d.i32(q + 4) as usize == sce && d.i32(q + 24) == 256 && d.i32(q + 28) == 64 && d.i32(q + 40) == 5)
            .ok_or_else(|| missing("texture table"))?;
        let mut texture_count = 0;
        while texture_count < 64 {
            let q = texture_table + texture_count * 0xF0;
            if q + 0xF0 > d.len() { break }
            let (idx, w, h, fmt) = (d.i32(q + 4), d.i32(q + 24), d.i32(q + 28), d.i32(q + 40));
            if idx < 0 || idx as usize >= names.len() || !(1..=1024).contains(&w) || !(1..=1024).contains(&h) || ![0, 2, 3, 4, 5, 0x14].contains(&fmt) { break }
            texture_count += 1;
        }
        // Slot table: 21 x 6 (column,row) pairs covering the 14x9 grid exactly once.
        let slot_table = (0..d.len().saturating_sub(21 * 48)).step_by(4).find(|&q| {
            let mut seen = [false; 126];
            for k in 0..126 {
                let (c, r) = (d.i32(q + k * 8), d.i32(q + k * 8 + 4));
                if !(0..14).contains(&c) || !(0..9).contains(&r) { return false }
                let cell = (c * 9 + r) as usize;
                if seen[cell] { return false }
                seen[cell] = true;
            }
            true
        }).ok_or_else(|| missing("tower slot table"))?;
        // Growth tables: 14 fill factors starting 0.2, 0.4, ..., then 14 size factors.
        let fill_pat: Vec<u8> = [0.2f32, 0.4, 0.6, 0.8, 1.0].iter().flat_map(|v| v.to_le_bytes()).collect();
        let fill_table = img.find_all(&fill_pat, 4).into_iter().find(|&q| {
            let sizes = img.f32s(q + 0x38, 14);
            sizes[0] == 0.1 && sizes[13] == 1.0 && sizes.windows(2).all(|w| w[0] <= w[1])
        }).ok_or_else(|| missing("tower growth tables"))?;
        let size_table = fill_table + 0x38;
        // Base positions: 14 columns x 9 rows of (x, y, z, 0); within a column x is constant
        // and y steps down by 1.3.
        let tower_positions = (0..d.len().saturating_sub(126 * 16)).step_by(16).find(|&q| {
            (0..9).all(|r| {
                let (x0, y0, w) = (d.f32(q), d.f32(q + r * 16 + 4), d.f32(q + r * 16 + 12));
                let x = d.f32(q + r * 16);
                w == 0.0 && (x - x0).abs() < 1e-4 && (r == 0 || (d.f32(q + (r - 1) * 16 + 4) - y0 - 1.3).abs() < 1e-3)
            }) && d.f32(q) < -10.0 && (d.f32(q + 0x90) - d.f32(q) - 1.3).abs() < 1e-3
        }).ok_or_else(|| missing("tower position table"))?;
        // Orb colours: four (r, g, b, 0) rows with 0..=255 values; the cube table follows at +0x110.
        let orb_pat: Vec<u8> = [32f32, 128.0, 0.0, 0.0, 128.0, 32.0, 64.0, 0.0].iter().flat_map(|v| v.to_le_bytes()).collect();
        let orb_colours = img.find(&orb_pat).ok_or_else(|| missing("orb colour table"))?;
        let plausible = |q: usize, limit: f32| (0..5).all(|i| { let v = img.f32s(q + i * 16, 4); v[3] == 0.0 && v[0].abs() < limit && v[1].abs() < limit && (2.0..6.0).contains(&v[2]) });
        let cube_positions = orb_colours + 0x110;
        if !plausible(cube_positions, 6.0) { return Err(missing("glass cube positions")) }
        let prism_positions = fill_table + 0x70;
        if !plausible(prism_positions, 15.0) { return Err(missing("warning-scene prism positions")) }
        Ok(Self { asset_names: names, texture_table, texture_count, slot_table, fill_table, size_table, tower_positions, orb_colours, cube_positions, prism_positions })
    }
}

/// Virtual addresses (not offsets) of the tables inside a PS2LOGO image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LogoLayout {
    /// `(NTSC, PAL)` object tables: 4 x `{keyframes*, count}`.
    pub shape_tables: (usize, usize),
    /// `(NTSC, PAL)` colour keyframe tables: 4 x `{keys*, count}`.
    pub colour_tables: (usize, usize),
    /// Object kinds `{0,1,1,1}`; layer A flags at +0x20, layer B flags at +0x40.
    pub type_table: usize,
    /// Five ribbon copy multipliers.
    pub ribbon_multipliers: usize,
    /// 2.8 then 3.8; the PAL y scale pair at +8/+12; the NTSC/PAL rates at +0x10/+0x14.
    pub ribbon_delta: usize,
    /// The embedded `SShd` sound bank header (12 bytes before the signature).
    pub bank_header: usize,
}

impl LogoLayout {
    /// Finds every table by content. A miss names the table in [`Error::Corrupt`].
    pub fn discover(img: &ProgramImage) -> Result<Self> {
        let d = &img.data[..];
        let base = img.base;
        let pat = |vals: &[f32]| vals.iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<u8>>();
        let ribbon_multipliers = img.find(&pat(&[0.0, 0.3, 1.0, 0.3, 0.0, 0.0])).ok_or_else(|| missing("ribbon multipliers"))?;
        let ribbon_delta = img.find(&pat(&[2.8, 3.8])).filter(|&q| (0.5..1.0).contains(&d.f32(q + 0x10)) && (0.5..1.0).contains(&d.f32(q + 0x14))).ok_or_else(|| missing("ribbon constants"))?;
        let type_table = img.find_all(&[0u8, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0], 4).into_iter()
            .find(|&q| (0..4).all(|i| d.u32(q + 0x20 + i * 4) <= 1 && d.u32(q + 0x40 + i * 4) <= 1) && d.u32(q + 0x20) == 1 && d.u32(q + 0x40) == 0)
            .ok_or_else(|| missing("object type tables"))?;
        // Object tables: 4 x {keyframes*, count}; each keyframe {t, shape*, polylines}.
        let object_table = |q: usize| (0..4).all(|k| {
            let (p, c) = (d.u32(q + k * 8) as usize, d.u32(q + k * 8 + 4));
            let Some(o) = img.at(p) else { return false };
            if !(1..=16).contains(&c) { return false }
            d.u32(o) == 0 && img.contains(d.u32(o + 4) as usize) && (1..=32).contains(&d.u32(o + 8))
        });
        let shapes: Vec<usize> = (0..d.len().saturating_sub(64)).step_by(4).filter(|&q| object_table(q)).collect();
        // Two tables 0x20 apart: PAL first, then NTSC (as in the studied build).
        let shape_tables = shapes.iter().find(|&&q| shapes.contains(&(q + 0x20))).map(|&q| (q + 0x20 + base, q + base)).ok_or_else(|| missing("shape keyframe tables"))?;
        let colour_table = |q: usize| (0..4).all(|k| {
            let (p, c) = (d.u32(q + k * 8) as usize, d.u32(q + k * 8 + 4) as usize);
            let Some(o) = img.at(p) else { return false };
            if !(2..=32).contains(&c) { return false }
            if o + 20 * c > d.len() || d.i32(o) != 0 { return false }
            (0..c).all(|i| (1..5).all(|j| (0..=300).contains(&d.i32(o + i * 20 + j * 4)))) && (0..c - 1).all(|i| d.i32(o + i * 20) < d.i32(o + (i + 1) * 20))
        });
        let colours: Vec<usize> = (0..d.len().saturating_sub(64)).step_by(4).filter(|&q| colour_table(q)).collect();
        let colour_tables = colours.iter().find(|&&q| colours.contains(&(q + 0x20))).map(|&q| (q + 0x20 + base, q + base)).ok_or_else(|| missing("colour keyframe tables"))?;
        let bank_header = img.find(b"SShd").and_then(|q| q.checked_sub(0xC)).map(|q| q + base).ok_or_else(|| missing("embedded sound bank"))?;
        Ok(Self { shape_tables, colour_tables, type_table: type_table + base, ribbon_multipliers: ribbon_multipliers + base, ribbon_delta: ribbon_delta + base, bank_header })
    }
}

/// Offsets of the pitch and pan tables of the IOP sound driver (`rom0:OSDSND`), located by
/// their entries around the unison step; `None` when the module does not contain them.
#[must_use]
pub(crate) fn driver_tables(module: &[u8]) -> Option<(usize, usize)> {
    let unison = (0..module.len().saturating_sub(6)).step_by(2).find(|&i| {
        let (a, b, c) = (module.u16(i), module.u16(i + 2), module.u16(i + 4));
        b == 0x1000 && (0xFEE..=0xFF3).contains(&a) && (0x100D..=0x1012).contains(&c)
    })?;
    let pitch = (unison + 2).checked_sub(208 * 2)?;
    let pan = module.windows(6).position(|w| w == [0x80, 0, 0x80, 8, 0x80, 0x10])?;
    Some((pitch, pan))
}

#[cfg(test)]
mod tests {
    use super::driver_tables;

    #[test]
    fn unison_entry_before_the_table_start_is_rejected() {
        // A driver module whose unison entry sits before where the pitch table would start.
        let mut osdsnd = vec![0u8; 64];
        for (i, v) in [0xFF0u16, 0x1000, 0x1010].iter().enumerate() { osdsnd[i * 2..i * 2 + 2].copy_from_slice(&v.to_le_bytes()) }
        assert_eq!(driver_tables(&osdsnd), None);
    }
}
