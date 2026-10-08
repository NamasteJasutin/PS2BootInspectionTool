//! Everything the opening needs from the user's BIOS dump: data tables and textures of
//! `rom0:OSDSYS`, located per ROM version.

use crate::bytes::Bytes;
use crate::locate::{Image, OsdLayout};
use crate::rom::{unpack, RomDir};
use crate::{Error, Result};
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

/// Columns of the tower grid.
pub const COLUMNS: usize = 14;
/// Rows of the tower grid.
pub const ROWS: usize = 9;

/// The opening's data tables and textures, read from `rom0:OSDSYS` and `rom0:TEXIMAGE`.
#[derive(Clone)]
#[non_exhaustive]
pub struct OpeningAssets {
    /// The 14-character `ROMVER` string (`0200EC20040614`).
    pub rom_version: String,
    /// `slots[record][k]` = (column, row) of the k-th tower owned by a history record.
    pub slots: Vec<[(usize, usize); 6]>,
    /// Indexed by launch-count step: how solid / how long the growing tower is.
    pub fill: Vec<f32>,
    /// Indexed by launch-count step: length factor of the growing tower.
    pub size: Vec<f32>,
    /// Model-space base position per slot, `[column][row]`.
    pub tower_positions: Vec<Vec<Vec3>>,
    /// Colours of the four light orbs, 0..=255 per channel.
    pub orb_colours: Vec<Vec3>,
    /// Model-space positions of the five glass cubes of the boot scene.
    pub cube_positions: Vec<Vec3>,
    /// Glass prisms of the warning scene (model units; z maps as (z - 2.5) * 128 + 788).
    pub prism_positions: Vec<Vec3>,
    /// Every texture the opening draws, by asset name.
    pub textures: HashMap<String, Texture>,
}

impl std::fmt::Debug for OpeningAssets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut names: Vec<&String> = self.textures.keys().collect();
        names.sort();
        f.debug_struct("OpeningAssets").field("rom_version", &self.rom_version).field("slots", &self.slots.len()).field("textures", &names).finish_non_exhaustive()
    }
}

impl OpeningAssets {
    /// Locates and decodes everything from a BIOS dump. Fails with
    /// [`Error::UnsupportedVersion`] when a table cannot be found in this ROM.
    pub fn load(bios: &RomDir) -> Result<Self> {
        let version = bios.module("ROMVER")?.cstr(0, 14);
        let img = Image::from_module(bios.module("OSDSYS")?, COMPRESSED_BASE)?;
        let (lay, names) = OsdLayout::discover(&img).map_err(|e| Error::UnsupportedVersion(format!("{version}: {e}")))?;
        let osd = &img.data[..];
        let v3 = |o: usize| Vec3::new(osd.f32(o), osd.f32(o + 4), osd.f32(o + 8));

        let slots = (0..crate::history::RECORD_COUNT).map(|i| {
            let mut s = [(0usize, 0usize); 6];
            for (k, slot) in s.iter_mut().enumerate() {
                let o = lay.slot_table + i * 0x30 + k * 8;
                *slot = (osd.i32(o) as usize, osd.i32(o + 4) as usize);
            }
            s
        }).collect();
        let fill = img.f32s(lay.fill_table, 14);
        let size = img.f32s(lay.size_table, 14);
        let tower_positions = (0..COLUMNS).map(|c| (0..ROWS).map(|r| v3(lay.tower_positions + c * 0x90 + r * 16)).collect()).collect();
        let orb_colours = (0..4).map(|i| v3(lay.orb_colours + i * 16)).collect();
        let cube_positions = (0..5).map(|i| v3(lay.cube_positions + i * 16)).collect();
        let prism_positions = (0..5).map(|i| v3(lay.prism_positions + i * 16)).collect();

        let archive = RomDir::new(bios.module("TEXIMAGE")?.to_vec())?;
        let mut textures = HashMap::new();
        for i in 0..lay.texture_count {
            let d = lay.texture_table + i * 0xF0;
            let Some(Some(name)) = names.get(osd.i32(d + 4) as usize) else { continue };
            let clut = osd.u32(d + 8) as usize;
            let (w, h) = (osd.i32(d + 24) as usize, osd.i32(d + 28) as usize);
            let (mips, skip, format) = (osd.i32(d + 32) as usize, osd.i32(d + 36) as usize, osd.i32(d + 40));
            let Ok(packed) = archive.module(name) else { continue };
            let raw = unpack(packed, 0)?;
            let palette = match img.at(clut).filter(|_| clut != 0) {
                Some(o) => Some(osd.get(o..o + 64).ok_or_else(|| Error::Corrupt(format!("palette of {name} is truncated")))?.to_vec()),
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
        return Err(Error::Corrupt(format!("implausible texture size {w}x{h}")));
    }
    let mut out = vec![0u8; w * h * 4];
    let need = |n: usize| if src.len() < n { Err(Error::Corrupt("texture data too short".into())) } else { Ok(()) };
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
            let pal = palette.ok_or_else(|| Error::Corrupt("palette missing".into()))?;
            if pal.len() < 64 {
                return Err(Error::Corrupt("palette too short".into()));
            }
            for i in 0..w * h {
                let idx = if i & 1 == 0 { src[i / 2] & 15 } else { src[i / 2] >> 4 } as usize;
                out[i * 4..i * 4 + 3].copy_from_slice(&pal[idx * 4..idx * 4 + 3]);
                out[i * 4 + 3] = scale_alpha(pal[idx * 4 + 3]);
            }
        }
        f => return Err(Error::Corrupt(format!("unknown texture format {f}"))),
    }
    Ok(out)
}
