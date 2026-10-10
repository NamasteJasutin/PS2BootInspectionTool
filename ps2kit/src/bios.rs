//! Everything the opening needs from the user's BIOS dump: data tables and textures of
//! `rom0:OSDSYS` or its opening plug-in, located per ROM version.

use crate::bytes::Bytes;
use crate::history::RECORD_COUNT;
use crate::locate::{OsdLayout, ProgramImage};
use crate::rom::{unpack, RomDir};
use crate::{Error, Format, Result};
use glam::Vec3;
use std::collections::HashMap;

/// A decoded texture, RGBA8 with PS2 alpha rescaled so that 0x80 becomes 255.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Texture {
    /// Asset name as the OSD program calls it (`TEXOWAL0`).
    pub name: String,
    /// Width in pixels.
    pub width: usize,
    /// Height in pixels.
    pub height: usize,
    /// `width * height * 4` bytes, row-major.
    pub rgba: Vec<u8>,
    /// Extra mip levels the console generates for this texture.
    pub mip_levels: usize,
}

/// Where the stub of a compressed OSDSYS places the program.
const COMPRESSED_BASE: usize = 0x200000;

/// Compatibility roles for MOPEN's unnamed descriptors. `research/README.md`, Round two,
/// "PS2 1.00 J" describes the inline layout; `opening.md` §2 gives the later names/order.
/// Verified in 0100JC20000117: slots 0..=12 have the same order, dimensions, source format
/// and decoded RGBA bytes as 0160AC20020207's named assets (including the wall's two mips).
/// Slots 13/14 are Japanese/English "insert disc" masks, identified by decoded lettering;
/// they use 256x256 format 3 instead of the later 512x128 indexed atlas. MOPEN draws the
/// lower 256x128 strip to the right of the upper one (0x502EA8..0x502EDC); we repack those
/// strips into the later atlas shape. Their GS alpha mask becomes grey + alpha, so the later
/// additive text draw uses the mask as brightness (`opening_scene1.md` §8.2), rather than
/// adding an all-white rectangle. No inline texture is indexed, so there is no CLUT.
/// +0x24 skips the 20-byte TIM header on RGB16 sources;
/// four alignment bytes after those sources explain the 0x18-byte inter-pixel gaps.
/// Descriptor +0xC is the load group: 0 both scenes, 1 opening, 2 warning, 3 current language
/// in the warning (MOPEN 0x5007C8..0x5008CC; compare `opening.md` §2).
const INLINE_TEXTURE_NAMES: [&str; 15] = [
    "TEXOSCE", "TEXOFOG0", "TEXOFOG1", "TEXOFOG2", "TEXOFOG3", "TEXOFOG4", "TEXOWAL0",
    "TEXOCRLE", "TEXOCRBL", "TEXOFLAR", "TEXOREF", "TEXOBLP", "TEXOBLPR", "TEXOPNGJ", "TEXOPNGE",
];

/// Resolves the raw-LZ opening plug-in from its ROM descriptor (`osdsys_hooks.md` §0).
fn opening_plugin(bios: &RomDir, descriptor: &[u8]) -> Result<ProgramImage> {
    let invalid = || Error::Corrupt(Format::Bios, "invalid OSOPEN module descriptor".into());
    let text = std::str::from_utf8(descriptor).map_err(|_| invalid())?;
    let mut lines = text.trim_end_matches('\0').lines();
    let version = lines.next().ok_or_else(invalid)?;
    let name = lines.next().ok_or_else(invalid)?;
    let address = lines.next().ok_or_else(invalid)?;
    if version.parse::<u32>().is_err() || name.is_empty() || name.len() > 10 || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
        return Err(invalid());
    }
    let base = u32::from_str_radix(address, 16).map_err(|_| invalid())? as usize;
    ProgramImage::from_module(bios.module(name)?, base)
}

/// Columns of the tower grid.
pub const COLUMNS: usize = 14;
/// Rows of the tower grid.
pub const ROWS: usize = 9;
/// Towers a history record owns.
pub const SLOTS_PER_RECORD: usize = 6;
/// Steps of the growing tower's fill and size tables.
pub const GROWTH_STEPS: usize = 14;

/// A cell of the tower grid: `column < COLUMNS`, `row < ROWS`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Slot {
    /// Column, 0..14.
    pub column: usize,
    /// Row, 0..9.
    pub row: usize,
}

/// The opening's data tables and textures, read from the monolithic OSD or its ROM plug-in.
/// The tables have the sizes the OSD program assumes, so the accessors never fail for an
/// index in range.
#[derive(Clone)]
pub struct OpeningAssets {
    rom_version: String,
    slots: Box<[[Slot; SLOTS_PER_RECORD]; RECORD_COUNT]>,
    fill: [f32; GROWTH_STEPS],
    size: [f32; GROWTH_STEPS],
    tower_positions: Box<[[Vec3; ROWS]; COLUMNS]>,
    orb_colours: [Vec3; 4],
    cube_positions: [Vec3; 5],
    prism_positions: [Vec3; 5],
    textures: HashMap<String, Texture>,
}

impl std::fmt::Debug for OpeningAssets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut names: Vec<&String> = self.textures.keys().collect();
        names.sort();
        f.debug_struct("OpeningAssets").field("rom_version", &self.rom_version).field("textures", &names).finish_non_exhaustive()
    }
}

impl OpeningAssets {
    /// The 14-character `ROMVER` string (`0200EC20040614`); its fifth character is the
    /// region letter ([`Region::from_romver_letter`](crate::Region::from_romver_letter)).
    #[must_use]
    pub fn rom_version(&self) -> &str { &self.rom_version }
    /// `slots()[record][k]` is the grid cell of the k-th tower owned by history record `record`.
    #[must_use]
    pub fn slots(&self) -> &[[Slot; SLOTS_PER_RECORD]; RECORD_COUNT] { &self.slots }
    /// `(fill, size)` of the growing tower at launch-count `step` (0..14): how solid and how
    /// long it is, 0..=1 each. `None` past the last step.
    #[must_use]
    pub fn growth(&self, step: usize) -> Option<(f32, f32)> { Some((*self.fill.get(step)?, *self.size.get(step)?)) }
    /// Model-space base position of a grid cell; `None` outside the 14 x 9 grid.
    #[must_use]
    pub fn tower_position(&self, column: usize, row: usize) -> Option<Vec3> { self.tower_positions.get(column)?.get(row).copied() }
    /// Colours of the four light orbs, 0..=255 per channel.
    #[must_use]
    pub fn orb_colours(&self) -> &[Vec3; 4] { &self.orb_colours }
    /// Model-space positions of the five glass cubes of the boot scene.
    #[must_use]
    pub fn cube_positions(&self) -> &[Vec3; 5] { &self.cube_positions }
    /// Glass prisms of the warning scene (model units; z maps as (z - 2.5) * 128 + 788).
    #[must_use]
    pub fn prism_positions(&self) -> &[Vec3; 5] { &self.prism_positions }
    /// The texture called `name` (`TEXOWAL0`), if the ROM has it.
    #[must_use]
    pub fn texture(&self, name: &str) -> Option<&Texture> { self.textures.get(name) }
    /// Every texture the opening draws, in no particular order.
    pub fn textures(&self) -> impl Iterator<Item = &Texture> { self.textures.values() }

    /// Locates and decodes everything from a BIOS dump. Fails with
    /// [`Error::UnsupportedVersion`] when a table cannot be found in this ROM.
    pub fn load(bios: &RomDir) -> Result<Self> {
        let version = bios.module("ROMVER")?.cstr(0, 14);
        let mut img = ProgramImage::from_module(bios.module("OSDSYS")?, COMPRESSED_BASE)?;
        if !OsdLayout::has_asset_table(&img) {
            match bios.module("OSOPEN") {
                Ok(descriptor) => img = opening_plugin(bios, descriptor)?,
                Err(Error::NotFound(..)) => {},
                Err(e) => return Err(e),
            }
        }
        let lay = OsdLayout::discover(&img).map_err(|e| Error::UnsupportedVersion(format!("{version}: {e}")))?;
        let names = &lay.asset_names;
        let osd = img.data();
        let v3 = |o: usize| Vec3::new(osd.f32(o), osd.f32(o + 4), osd.f32(o + 8));

        let mut slots = Box::new([[Slot { column: 0, row: 0 }; SLOTS_PER_RECORD]; RECORD_COUNT]);
        for (i, record) in slots.iter_mut().enumerate() {
            for (k, slot) in record.iter_mut().enumerate() {
                let o = lay.slot_table + i * 0x30 + k * 8;
                // The layout search accepted this table only with every pair inside the grid.
                *slot = Slot { column: osd.i32(o) as usize, row: osd.i32(o + 4) as usize };
            }
        }
        let mut fill = [0f32; GROWTH_STEPS];
        let mut size = [0f32; GROWTH_STEPS];
        fill.copy_from_slice(&img.f32s(lay.fill_table, GROWTH_STEPS));
        size.copy_from_slice(&img.f32s(lay.size_table, GROWTH_STEPS));
        let mut tower_positions = Box::new([[Vec3::ZERO; ROWS]; COLUMNS]);
        for (c, column) in tower_positions.iter_mut().enumerate() {
            for (r, p) in column.iter_mut().enumerate() { *p = v3(lay.tower_positions + c * 0x90 + r * 16) }
        }
        let orb_colours = std::array::from_fn(|i| v3(lay.orb_colours + i * 16));
        let cube_positions = std::array::from_fn(|i| v3(lay.cube_positions + i * 16));
        let prism_positions = std::array::from_fn(|i| v3(lay.prism_positions + i * 16));

        let archive = if lay.inline_pixels { None } else { Some(RomDir::new(bios.module("TEXIMAGE")?.to_vec())?) };
        let mut textures = HashMap::new();
        for i in 0..lay.texture_count {
            let d = lay.texture_table + i * lay.texture_stride;
            let (w, h) = (osd.i32(d + 24) as usize, osd.i32(d + 28) as usize);
            let (mips, skip, format) = (osd.i32(d + 32) as usize, osd.i32(d + 36) as usize, osd.i32(d + 40));
            if lay.inline_pixels {
                let Some(&name) = INLINE_TEXTURE_NAMES.get(i) else { continue };
                let start = img.at(osd.u32(d + 8) as usize).and_then(|p| p.checked_add(skip)).ok_or_else(|| Error::Corrupt(Format::Bios, format!("pixels of {name} lie outside the image")))?;
                let raw = osd.get(start..).ok_or_else(|| Error::Corrupt(Format::Bios, format!("pixels of {name} are truncated")))?;
                let mut rgba = decode_texture(raw, w, h, format, None)?;
                let (w, h) = if i >= 13 {
                    if !h.is_multiple_of(2) { return Err(Error::Corrupt(Format::Bios, format!("odd-height language atlas {name}"))) }
                    for pixel in rgba.chunks_exact_mut(4) {
                        let mask = (pixel[3] as u16 * 2).min(255) as u8;
                        pixel.fill(mask);
                    }
                    let mut atlas = Vec::with_capacity(rgba.len());
                    for y in 0..h / 2 {
                        atlas.extend_from_slice(&rgba[y * w * 4..(y + 1) * w * 4]);
                        atlas.extend_from_slice(&rgba[(y + h / 2) * w * 4..(y + h / 2 + 1) * w * 4]);
                    }
                    rgba = atlas;
                    (w * 2, h / 2)
                } else { (w, h) };
                textures.insert(name.into(), Texture { name: name.into(), width: w, height: h, rgba, mip_levels: mips });
                continue;
            }
            let Some(Some(name)) = names.get(osd.i32(d + 4) as usize) else { continue };
            let clut = osd.u32(d + 8) as usize;
            let Some(archive) = &archive else { continue };
            let Ok(packed) = archive.module(name) else { continue };
            let raw = unpack(packed, 0)?;
            let palette = match img.at(clut).filter(|_| clut != 0) {
                Some(o) => Some(osd.get(o..o + 64).ok_or_else(|| Error::Corrupt(Format::Bios, format!("palette of {name} is truncated")))?.to_vec()),
                None => None,
            };
            let rgba = decode_texture(&raw[skip.min(raw.len())..], w, h, format, palette.as_deref())?;
            textures.insert(name.clone(), Texture { name: name.clone(), width: w, height: h, rgba, mip_levels: mips });
        }
        Ok(Self { rom_version: version, slots, fill, size, tower_positions, orb_colours, cube_positions, prism_positions, textures })
    }
}

/// Converts a texture to RGBA8 the way the console's loader interprets `format`: 0 = RGBA32,
/// 2 = RGB16, 3/4 = 8-bit alpha mask over white/black, 5 = grey + alpha, 0x14 = 4-bit indexed
/// through a 16-entry RGBA `palette`. PS2 alpha (0x80 = opaque) is rescaled to 255.
pub fn decode_texture(src: &[u8], w: usize, h: usize, format: i32, palette: Option<&[u8]>) -> Result<Vec<u8>> {
    if w > 4096 || h > 4096 {
        return Err(Error::Corrupt(Format::Bios, format!("implausible texture size {w}x{h}")));
    }
    let mut out = vec![0u8; w * h * 4];
    let need = |n: usize| if src.len() < n { Err(Error::Corrupt(Format::Bios, "texture data too short".into())) } else { Ok(()) };
    let scale_alpha = |a: u8| ((a as u32 * 2).min(255)) as u8;
    match format {
        0 => {
            need(w * h * 4)?;
            for i in 0..w * h {
                out[i * 4..i * 4 + 3].copy_from_slice(&src[i * 4..i * 4 + 3]);
                out[i * 4 + 3] = scale_alpha(src[i * 4 + 3]);
            }
        }
        2 => {
            need(w * h * 2)?;
            for i in 0..w * h {
                let v = src[i * 2] as u32 | (src[i * 2 + 1] as u32) << 8;
                out[i * 4] = ((v & 31) << 3) as u8;
                out[i * 4 + 1] = ((v >> 5 & 31) << 3) as u8;
                out[i * 4 + 2] = ((v >> 10 & 31) << 3) as u8;
                out[i * 4 + 3] = 255;
            }
        }
        3 | 4 => {
            need(w * h)?;
            let rgb = if format == 3 { 255 } else { 0 };
            for i in 0..w * h {
                out[i * 4..i * 4 + 3].fill(rgb);
                out[i * 4 + 3] = src[i]; // used as a 0..=255 mask, not rescaled
            }
        }
        5 => {
            need(w * h * 2)?;
            for i in 0..w * h {
                out[i * 4..i * 4 + 3].fill(src[i * 2]);
                out[i * 4 + 3] = scale_alpha(src[i * 2 + 1]);
            }
        }
        0x14 => {
            need((w * h).div_ceil(2))?;
            let pal = palette.ok_or_else(|| Error::Corrupt(Format::Bios, "palette missing".into()))?;
            if pal.len() < 64 {
                return Err(Error::Corrupt(Format::Bios, "palette too short".into()));
            }
            for i in 0..w * h {
                let idx = if i & 1 == 0 { src[i / 2] & 15 } else { src[i / 2] >> 4 } as usize;
                out[i * 4..i * 4 + 3].copy_from_slice(&pal[idx * 4..idx * 4 + 3]);
                out[i * 4 + 3] = scale_alpha(pal[idx * 4 + 3]);
            }
        }
        f => return Err(Error::Corrupt(Format::Bios, format!("unknown texture format {f}"))),
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::opening_plugin;
    use crate::rom::RomDir;

    #[test]
    fn opening_descriptor_selects_module_and_base_and_rejects_bad_text() {
        let mut d = vec![0; 16];
        for (name, size) in [("RESET", 16u32), ("ROMDIR", 64), ("CUSTOM", 11)] {
            let p = d.len();
            d.resize(p + 16, 0);
            d[p..p + name.len()].copy_from_slice(name.as_bytes());
            d[p + 12..p + 16].copy_from_slice(&size.to_le_bytes());
        }
        d.extend_from_slice(&[0; 16]);
        d.extend_from_slice(&3u32.to_le_bytes());
        d.extend_from_slice(&[0; 4]);
        d.extend_from_slice(b"abc");
        let rom = RomDir::new(d).expect("synthetic ROMDIR");
        let img = opening_plugin(&rom, b"123\nCUSTOM\n00731000\n").expect("descriptor");
        assert_eq!((img.base(), img.data()), (0x731000, &b"abc"[..]));
        for descriptor in [&b""[..], b"123\nCUSTOM", b"123\n../CUSTOM\n00731000", b"bad\nCUSTOM\n00731000", b"123\nCUSTOM\nxyz", b"123\nCUSTOM\n100000000", b"\xFF"] {
            assert!(opening_plugin(&rom, descriptor).is_err());
        }
    }
}
