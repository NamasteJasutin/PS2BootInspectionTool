//! Everything the opening needs from the user's BIOS dump: data tables and textures of
//! `rom0:OSDSYS`, located per ROM version.

use crate::bytes::Bytes;
use crate::rom::{unpack, RomDir};
use crate::{Error, Result};
use glam::Vec3;
use std::collections::HashMap;

/// A decoded texture, RGBA8 with PS2 alpha rescaled so that 0x80 becomes 255.
#[derive(Debug, Clone)]
pub struct Texture {
    pub name: String,
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
    /// Extra mip levels the console generates for this texture.
    pub mip_levels: usize,
}

/// Where the opening's data sits inside one specific OSDSYS build.
struct Layout {
    asset_names: usize,
    texture_table: usize,
    texture_count: usize,
    slot_table: usize,
    fill_table: usize,
    size_table: usize,
    tower_positions: usize,
    orb_colours: usize,
    cube_positions: usize,
    prism_positions: usize,
}

const LOAD_ADDRESS: usize = 0x200000;
/// LZ stream inside the ROM's OSDSYS ELF (stub linked at 0x100000, PT_LOAD at file offset 0x80).
const STUB_STREAM_OFFSET: usize = 0x100D80 - 0x100000 + 0x80;

fn layout(rom_version: &str) -> Option<Layout> {
    match rom_version {
        // SCPH-70004, v2.00 Europe, 2004-06-14
        "0200EC20040614" => Some(Layout {
            asset_names: 0x27B4F8,
            texture_table: 0x287700,
            texture_count: 25,
            slot_table: 0x2891E0,
            fill_table: 0x289E60,
            size_table: 0x289E98,
            tower_positions: 0x2895F0,
            orb_colours: 0x289080,
            cube_positions: 0x289190,
            prism_positions: 0x289ED0,
        }),
        _ => None,
    }
}

pub const COLUMNS: usize = 14;
pub const ROWS: usize = 9;

pub struct OpeningAssets {
    pub rom_version: String,
    /// `slots[record][k]` = (column, row) of the k-th tower owned by a history record.
    pub slots: Vec<[(usize, usize); 6]>,
    /// Indexed by launch-count step: how solid / how long the growing tower is.
    pub fill: Vec<f32>,
    pub size: Vec<f32>,
    /// Model-space base position per slot, `[column][row]`.
    pub tower_positions: Vec<Vec<Vec3>>,
    pub orb_colours: Vec<Vec3>,
    pub cube_positions: Vec<Vec3>,
    /// Glass prisms of the warning scene (model units; z maps as (z - 2.5) * 128 + 788).
    pub prism_positions: Vec<Vec3>,
    pub textures: HashMap<String, Texture>,
}

impl OpeningAssets {
    pub fn load(bios: &RomDir) -> Result<Self> {
        let version = bios.module("ROMVER")?.cstr(0, 14);
        let lay = layout(&version).ok_or_else(|| Error::UnsupportedVersion(version.clone()))?;
        let osd = unpack(bios.module("OSDSYS")?, STUB_STREAM_OFFSET)?;
        let at = |v: usize| v - LOAD_ADDRESS;
        let v3 = |o: usize| Vec3::new(osd.f32(o), osd.f32(o + 4), osd.f32(o + 8));

        let mut slots = Vec::new();
        for i in 0..crate::history::RECORD_COUNT {
            let mut s = [(0usize, 0usize); 6];
            for (k, slot) in s.iter_mut().enumerate() {
                let o = at(lay.slot_table) + i * 0x30 + k * 8;
                let (c, r) = (osd.i32(o), osd.i32(o + 4));
                if !(0..COLUMNS as i32).contains(&c) || !(0..ROWS as i32).contains(&r) {
                    return Err(Error::Corrupt("tower slot table".into()));
                }
                *slot = (c as usize, r as usize);
            }
            slots.push(s);
        }
        let fill = (0..14).map(|i| osd.f32(at(lay.fill_table) + i * 4)).collect();
        let size = (0..14).map(|i| osd.f32(at(lay.size_table) + i * 4)).collect();
        let tower_positions = (0..COLUMNS)
            .map(|c| (0..ROWS).map(|r| v3(at(lay.tower_positions) + c * 0x90 + r * 16)).collect())
            .collect();
        let orb_colours = (0..4).map(|i| v3(at(lay.orb_colours) + i * 16)).collect();
        let cube_positions = (0..5).map(|i| v3(at(lay.cube_positions) + i * 16)).collect();
        let prism_positions = (0..5).map(|i| v3(at(lay.prism_positions) + i * 16)).collect();

        // Asset names, in table order, to resolve the texture descriptors' asset indices.
        let mut names: Vec<Option<String>> = Vec::new();
        let mut p = at(lay.asset_names);
        while osd.u32(p) != 0xFFFF_FFFF && names.len() < 256 {
            let ptr = osd.u32(p) as usize;
            names.push(if ptr == 0 { None } else { Some(osd.cstr(at(ptr), 16)) });
            p += 16;
        }

        let archive = RomDir::new(bios.module("TEXIMAGE")?.to_vec())?;
        let mut textures = HashMap::new();
        for i in 0..lay.texture_count {
            let d = at(lay.texture_table) + i * 0xF0;
            let Some(Some(name)) = names.get(osd.i32(d + 4) as usize) else { continue };
            let clut = osd.u32(d + 8) as usize;
            let (w, h) = (osd.i32(d + 24) as usize, osd.i32(d + 28) as usize);
            let (mips, skip, format) = (osd.i32(d + 32) as usize, osd.i32(d + 36) as usize, osd.i32(d + 40));
            let Ok(packed) = archive.module(name) else { continue };
            let raw = unpack(packed, 0)?;
            let palette = (clut != 0).then(|| osd[at(clut)..at(clut) + 64].to_vec());
            let rgba = decode_texture(&raw[skip.min(raw.len())..], w, h, format, palette.as_deref())?;
            textures.insert(name.clone(), Texture { name: name.clone(), width: w, height: h, rgba, mip_levels: mips });
        }

        Ok(Self { rom_version: version, slots, fill, size, tower_positions, orb_colours, cube_positions, prism_positions, textures })
    }
}

/// Source formats as the console's loader interprets them.
pub fn decode_texture(src: &[u8], w: usize, h: usize, format: i32, palette: Option<&[u8]>) -> Result<Vec<u8>> {
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
            need(w * h / 2)?;
            let pal = palette.ok_or_else(|| Error::Corrupt("palette missing".into()))?;
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
