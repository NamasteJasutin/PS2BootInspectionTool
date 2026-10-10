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

    /// Unpacks a stub + LZ program, a raw LZ module, or the PT_LOAD segments of a plain ELF.
    /// `compressed_base` is where the loader puts the LZ payload; for 1.00 plug-ins it comes
    /// from the ROM's text descriptor (`research/osdsys_hooks.md` §0).
    pub fn from_module(module: &[u8], compressed_base: usize) -> Result<Self> {
        if !module.starts_with(b"\x7fELF") {
            return Ok(Self { data: unpack(module, 0)?, base: compressed_base });
        }
        if module.len() < 0x34 {
            return Err(Error::Corrupt(Format::Bios, "truncated OSD ELF header".into()));
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
        // Plain ELF: preserve each segment's virtual address, including gaps and bss.
        // 1.00 PS2LOGO keeps code and data in separate PT_LOAD segments (0x200000/0x21B000).
        let (phoff, phentsize, phnum) = (module.u32(0x1C) as usize, module.u16(0x2A) as usize, module.u16(0x2C) as usize);
        if phentsize < 32 || phoff.checked_add(phnum * phentsize).is_none_or(|end| end > module.len()) {
            return Err(Error::Corrupt(Format::Bios, "truncated ELF program headers".into()));
        }
        let mut segments = Vec::new();
        for i in 0..phnum {
            let o = phoff + i * phentsize;
            if module.u32(o) != 1 {
                continue;
            }
            let (off, vaddr, filesz, memsz) = (module.u32(o + 4) as usize, module.u32(o + 8) as usize, module.u32(o + 16) as usize, module.u32(o + 20) as usize);
            let Some(segment) = module.get(off..off.saturating_add(filesz)) else {
                return Err(Error::Corrupt(Format::Bios, "truncated PT_LOAD segment".into()));
            };
            let size = memsz.max(filesz);
            if size > 64 << 20 || vaddr.checked_add(size).is_none() {
                return Err(Error::Corrupt(Format::Bios, "implausible PT_LOAD memory range".into()));
            }
            segments.push((vaddr, size, segment));
        }
        if let Some(base) = segments.iter().map(|s| s.0).min() {
            let end = segments.iter().map(|s| s.0 + s.1).max().unwrap_or(base);
            if end - base > 64 << 20 {
                return Err(Error::Corrupt(Format::Bios, "implausible PT_LOAD memory range".into()));
            }
            let mut data = vec![0; end - base];
            for (vaddr, _, segment) in segments {
                let off = vaddr - base;
                data[off..off + segment.len()].copy_from_slice(segment);
            }
            return Ok(Self { data, base });
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
    /// The texture descriptors.
    pub texture_table: usize,
    /// Record size: 0xF0 for named assets, 0xE0 for inline pixels.
    pub texture_stride: usize,
    /// Whether descriptor +8 points directly to pixels in this program.
    pub inline_pixels: bool,
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
    fn asset_table(img: &ProgramImage) -> Result<usize> {
        let d = img.data();
        let fontm = img.find(b"FONTM\0").ok_or_else(|| missing("asset name strings"))?;
        let ptr = ((img.base + fontm) as u32).to_le_bytes();
        img.find_all(&ptr, 4).into_iter().find(|&i| d.u32(i + 16 + 4) != 0xFFFF_FFFF && img.contains(d.u32(i + 16) as usize)).ok_or_else(|| missing("asset table"))
    }

    /// Whether this program has the named asset table used by the monolithic OSD.
    pub fn has_asset_table(img: &ProgramImage) -> bool { Self::asset_table(img).is_ok() }

    /// Finds every table by content. A miss names the table in [`Error::Corrupt`].
    pub fn discover(img: &ProgramImage) -> Result<Self> {
        let d = &img.data[..];
        let (names, texture_table, texture_count, texture_stride, inline_pixels) = if Self::has_asset_table(img) {
            // Asset table: 16-byte entries {name*, addr, size, flags}; entry 0 names "FONTM".
            let asset_table = Self::asset_table(img)?;
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
            (names, texture_table, texture_count, 0xF0, false)
        } else {
            let (table, count) = inline_textures(img)?;
            (Vec::new(), table, count, 0xE0, true)
        };
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
        Ok(Self { asset_names: names, texture_table, texture_count, texture_stride, inline_pixels, slot_table, fill_table, size_table, tower_positions, orb_colours, cube_positions, prism_positions })
    }
}

/// 1.00's inline descriptors (`research/README.md`, Round two, "PS2 1.00 J"). The scan
/// accepts the SCE lettering followed by fifteen bounded records; it does not use 0x51C678.
fn inline_textures(img: &ProgramImage) -> Result<(usize, usize)> {
    let d = img.data();
    let valid = |q: usize| {
        if q + 0xE0 > d.len() { return false }
        let (w, h, fmt) = (d.i32(q + 0x18), d.i32(q + 0x1C), d.i32(q + 0x28));
        if !(1..=1024).contains(&w) || !(1..=1024).contains(&h) || d.u32(q + 0xC) > 3 || d.u32(q + 0x20) > 10 { return false }
        let bpp = match fmt { 0 => 4, 2 | 5 => 2, 3 | 4 => 1, _ => return false };
        let Some(pixels) = img.at(d.u32(q + 8) as usize) else { return false };
        pixels.checked_add(d.u32(q + 0x24) as usize).and_then(|p| p.checked_add(w as usize * h as usize * bpp)).is_some_and(|end| end <= d.len())
    };
    let table = (0..d.len().saturating_sub(15 * 0xE0)).step_by(4).find(|&q| {
        d.i32(q + 0x18) == 256 && d.i32(q + 0x1C) == 64 && d.i32(q + 0x28) == 5 && (0..15).all(|i| valid(q + i * 0xE0))
    }).ok_or_else(|| missing("inline texture table"))?;
    Ok((table, 15))
}

/// Virtual addresses (not offsets) of the tables inside a PS2LOGO image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LogoLayout {
    /// Whether the program has only one (NTSC) set of keyframes.
    pub ntsc_only: bool,
    /// `(NTSC, PAL)` object tables: 4 x `{keyframes*, count}`.
    pub shape_tables: (usize, usize),
    /// `(NTSC, PAL)` colour keyframe tables: 4 x `{keys*, count}`.
    pub colour_tables: (usize, usize),
    /// Object kinds `{0,1,1,1}`; layer A/B flags follow at one/two `type_flags_stride`s.
    pub type_table: usize,
    /// Distance between the kind and layer-flag arrays (0x10 in the single-mode program).
    pub type_flags_stride: usize,
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
        let ribbon_delta = img.find(&pat(&[2.8, 3.8])).filter(|&q| {
            ((0.5..1.0).contains(&d.f32(q + 0x10)) && (0.5..1.0).contains(&d.f32(q + 0x14))) ||
            ((0.5..1.0).contains(&d.f32(q + 8)) && (0.5..1.0).contains(&d.f32(q + 12)))
        }).ok_or_else(|| missing("ribbon constants"))?;
        // 1.00's single-mode counterpart of `ps2logo.md` §4.3: rates at +8/+12,
        // read by 0x202524/0x202610; one shape table at 0x224790 and colour table at
        // 0x224A40. Types/layer flags at 0x21B000 have a 0x10 stride. Later programs have
        // PAL/NTSC pairs, 0x20-spaced flags and rates at +0x10/+0x14.
        let ntsc_only = !((0.5..1.0).contains(&d.f32(ribbon_delta + 0x10)) && (0.5..1.0).contains(&d.f32(ribbon_delta + 0x14)));
        let type_flags_stride = if ntsc_only { 0x10 } else { 0x20 };
        let type_table = img.find_all(&[0u8, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0], 4).into_iter()
            .find(|&q| (0..4).all(|i| d.u32(q + type_flags_stride + i * 4) <= 1 && d.u32(q + 2 * type_flags_stride + i * 4) <= 1) && d.u32(q + type_flags_stride) == 1 && d.u32(q + 2 * type_flags_stride) == 0)
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
        let shape_tables = shapes.iter().find(|&&q| shapes.contains(&(q + 0x20))).map(|&q| (q + 0x20 + base, q + base))
            .or_else(|| (ntsc_only && shapes.len() == 1).then(|| (shapes[0] + base, shapes[0] + base))).ok_or_else(|| missing("shape keyframe tables"))?;
        let colour_table = |q: usize| (0..4).all(|k| {
            let (p, c) = (d.u32(q + k * 8) as usize, d.u32(q + k * 8 + 4) as usize);
            let Some(o) = img.at(p) else { return false };
            if !(2..=32).contains(&c) { return false }
            if o + 20 * c > d.len() || d.i32(o) != 0 { return false }
            (0..c).all(|i| (1..5).all(|j| (0..=300).contains(&d.i32(o + i * 20 + j * 4)))) && (0..c - 1).all(|i| d.i32(o + i * 20) < d.i32(o + (i + 1) * 20))
        });
        let colours: Vec<usize> = (0..d.len().saturating_sub(64)).step_by(4).filter(|&q| colour_table(q)).collect();
        let colour_tables = colours.iter().find(|&&q| colours.contains(&(q + 0x20))).map(|&q| (q + 0x20 + base, q + base))
            .or_else(|| (ntsc_only && colours.len() == 1).then(|| (colours[0] + base, colours[0] + base))).ok_or_else(|| missing("colour keyframe tables"))?;
        let bank_header = img.find(b"SShd").and_then(|q| q.checked_sub(0xC)).map(|q| q + base).ok_or_else(|| missing("embedded sound bank"))?;
        Ok(Self { ntsc_only, shape_tables, colour_tables, type_table: type_table + base, type_flags_stride, ribbon_multipliers: ribbon_multipliers + base, ribbon_delta: ribbon_delta + base, bank_header })
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
    use super::{driver_tables, inline_textures, ProgramImage};

    #[test]
    fn inline_descriptors_are_found_by_content_and_reject_out_of_range_pixels() {
        for (base, table) in [(0x500000, 24), (0x731000, 0x124)] {
            let pixels = table + 15 * 0xE0;
            let mut d = vec![0; pixels + 256 * 64 * 2];
            for i in 0..15 {
                let q = table + i * 0xE0;
                for (offset, value) in [(8, (base + pixels) as u32), (0x18, if i == 0 { 256 } else { 1 }), (0x1C, if i == 0 { 64 } else { 1 }), (0x28, if i == 0 { 5 } else { 3 })] {
                    d[q + offset..q + offset + 4].copy_from_slice(&value.to_le_bytes());
                }
            }
            assert_eq!(inline_textures(&ProgramImage::new(d.clone(), base)).expect("relocated descriptors"), (table, 15));
            let q = table + 14 * 0xE0;
            let end = (base + d.len()) as u32;
            d[q + 8..q + 12].copy_from_slice(&end.to_le_bytes());
            assert!(inline_textures(&ProgramImage::new(d, base)).is_err());
        }
    }

    #[test]
    fn unison_entry_before_the_table_start_is_rejected() {
        // A driver module whose unison entry sits before where the pitch table would start.
        let mut osdsnd = vec![0u8; 64];
        for (i, v) in [0xFF0u16, 0x1000, 0x1010].iter().enumerate() { osdsnd[i * 2..i * 2 + 2].copy_from_slice(&v.to_le_bytes()) }
        assert_eq!(driver_tables(&osdsnd), None);
    }
}
