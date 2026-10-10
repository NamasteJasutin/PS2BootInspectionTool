//! The PlayStation 1 licence screen from standalone BIOS dumps or a PS2's `rom0:LOGO`.
//! On a PS2 `rom0:LOGO` is the
//! PS1 shell (packed, R3000), and on a PS2 it shows only the licence screen — the "Sony
//! Computer Entertainment" diamond is not in this build (`notes/ps1_boot.md`). Everything is
//! read from the user's BIOS (the shell's model, bitmaps, font, matrices, note table and sound
//! bank) and the disc (the logo model in sectors 5–11, which a licensed disc carries
//! identically to the shell's own copy).

use crate::bytes::Bytes;
use crate::rom::{unpack, RomDir};
use crate::ps1bios::{region_letter, Ps1Bios};
use crate::sound::{decode_adpcm, envelope, SAMPLE_RATE};
use crate::{Error, Format, Result, VideoMode};
use std::path::Path;

/// One flat triangle of a TMD (mode 0x20): a colour, one normal, three vertices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct Prim {
    /// RGB, 0..=255.
    pub colour: [u8; 3],
    /// Index into [`Tmd::normals`].
    pub normal: u16,
    /// Indexes into [`Tmd::verts`], clockwise on screen.
    pub verts: [u16; 3],
}

/// A libgs TMD with one object, as the shell parses it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Tmd {
    /// Model-space vertices.
    pub verts: Vec<[i16; 3]>,
    /// Normals, 4.12 fixed point.
    pub normals: Vec<[i16; 3]>,
    /// Flat triangles in file order.
    pub prims: Vec<Prim>,
}

/// Bytes of the logo TMD the shell reads and compares.
pub const LOGO_TMD_BYTES: usize = 0x3278;

impl Tmd {
    /// Parses a TMD (one object, flat triangles of mode 0x20 or 0x30). Fails with
    /// [`Error::Corrupt`] on any other layout.
    pub fn parse(d: &[u8]) -> Result<Self> {
        let bad = |m: &str| Error::Corrupt(Format::Tmd, format!("TMD: {m}"));
        if d.len() < 0x28 || d.u32(0) != 0x41 {
            return Err(bad("no header"));
        }
        let (vert_top, n_vert, normal_top, n_normal, _prim_top, n_prim) = (d.u32(0x0C) as usize, d.u32(0x10) as usize, d.u32(0x14) as usize, d.u32(0x18) as usize, d.u32(0x1C) as usize, d.u32(0x20) as usize);
        if n_vert > 4096 || n_normal > 4096 || n_prim > 4096 {
            return Err(bad("implausible counts"));
        }
        let (vb, nb) = (0x0C + vert_top, 0x0C + normal_top);
        if vb + n_vert * 8 > d.len() || nb + n_normal * 8 > d.len() {
            return Err(bad("vertex or normal table past the end"));
        }
        let verts = (0..n_vert).map(|i| [d.i16(vb + i * 8), d.i16(vb + i * 8 + 2), d.i16(vb + i * 8 + 4)]).collect();
        let normals = (0..n_normal).map(|i| [d.i16(nb + i * 8), d.i16(nb + i * 8 + 2), d.i16(nb + i * 8 + 4)]).collect();
        // The shell assumes the primitives start right after the object table (file 0x28).
        let mut prims = Vec::with_capacity(n_prim);
        let mut p = 0x28;
        for _ in 0..n_prim {
            if p + 16 > d.len() { return Err(bad("primitive table past the end")) }
            let mode = d.u8(p + 3);
            let colour = [d.u8(p + 4), d.u8(p + 5), d.u8(p + 6)];
            match mode {
                0x20 => {
                    prims.push(Prim { colour, normal: d.u16(p + 8), verts: [d.u16(p + 10), d.u16(p + 12), d.u16(p + 14)] });
                    p += 16;
                }
                0x30 => {
                    if p + 20 > d.len() { return Err(bad("primitive table past the end")) }
                    // Three normals: the shell averages them and rewrites the record as 0x20.
                    // The first is used here; the logo does not contain any.
                    prims.push(Prim { colour, normal: d.u16(p + 8), verts: [d.u16(p + 10), d.u16(p + 14), d.u16(p + 18)] });
                    p += 20;
                }
                _ => return Err(bad("unsupported primitive mode")),
            }
        }
        Ok(Self { verts, normals, prims })
    }

    /// The logo TMD a PlayStation disc carries in sectors 5–11.
    pub fn from_disc(path: impl AsRef<Path>) -> Result<Self> {
        let raw = crate::sectors::SectorReader::open(path)?.read(5, 7)?;
        if raw.len() < LOGO_TMD_BYTES {
            return Err(Error::Corrupt(Format::DiscImage, "disc image is too short for the licence logo".into()));
        }
        Self::parse(&raw[..LOGO_TMD_BYTES])
    }
}

/// A 4-bit TIM with its CLUT, expanded to RGBA (15-bit colour 0 = transparent).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Tim {
    /// Width in pixels.
    pub width: usize,
    /// Height in pixels.
    pub height: usize,
    /// `width * height * 4` bytes, row-major.
    pub rgba: Vec<u8>,
}

impl Tim {
    fn parse_4bit(d: &[u8]) -> Result<Self> {
        if d.len() < 0x14 || d.u32(0) != 0x10 || d.u32(4) != 8 {
            return Err(Error::Corrupt(Format::Bios, "not a 4-bit TIM with CLUT".into()));
        }
        let clut_len = d.u32(8) as usize;
        let (cw, ch) = (d.u16(0x10) as usize, d.u16(0x12) as usize);
        if cw * ch == 0 || cw * ch > 256 || clut_len < 12 + cw * ch * 2 || clut_len > 12 + 512
            || 8 + clut_len + 12 > d.len() {
            return Err(Error::Corrupt(Format::Bios, "TIM CLUT is implausible".into()));
        }
        let clut: Vec<u16> = (0..cw * ch).map(|i| d.u16(0x14 + i * 2)).collect();
        let ib = 8 + clut_len;
        let (iw, ih) = (d.u16(ib + 8) as usize, d.u16(ib + 10) as usize);
        let (width, height) = (iw * 4, ih);
        if width > 1024 || height > 1024 || width == 0 || height == 0 {
            return Err(Error::Corrupt(Format::Bios, "TIM image size is implausible".into()));
        }
        let data = ib + 12;
        if d.u32(ib) as usize != 12 + iw * ih * 2 || data + iw * ih * 2 > d.len() {
            return Err(Error::Corrupt(Format::Bios, "TIM pixels are truncated".into()));
        }
        let mut rgba = vec![0u8; width * height * 4];
        for y in 0..height {
            for x in 0..width {
                let byte = d.u8(data + y * iw * 2 + x / 2);
                let idx = if x & 1 == 0 { byte & 0xF } else { byte >> 4 } as usize;
                let c = clut.get(idx).copied().unwrap_or(0);
                let o = (y * width + x) * 4;
                let five = |v: u16| ((v & 31) * 255 / 31) as u8;
                rgba[o..o + 4].copy_from_slice(&[five(c), five(c >> 5), five(c >> 10), if c == 0 { 0 } else { 255 }]);
            }
        }
        Ok(Self { width, height, rgba })
    }
}

/// The kernel's kanji font (`rom0:KROM`, 30 bytes per 16×15 glyph) with the shell's
/// proportional-width table for digits, letters and the space.
#[derive(Debug, Clone)]
pub struct Font {
    glyphs: Vec<u8>,
    /// (left shift, advance) for glyph indexes 0x93..=0xD1.
    prop: Vec<(u16, u16)>,
}

/// Rows of the mask [`Font::render`] produces (15 glyph rows and one blank).
pub const TEXT_ROW_HEIGHT: usize = 16;

impl Font {
    /// Glyph index of an ASCII character, through the shell's Shift-JIS conversion.
    fn index(c: u8) -> Option<usize> {
        Some(match c {
            b' ' => 0,
            b'.' => 4,
            b'(' => 0x29,
            b')' => 0x2A,
            b'0'..=b'9' => 0x93 + (c - b'0') as usize,
            b'A'..=b'Z' => 0x9D + (c - b'A') as usize,
            b'a'..=b'z' => 0xB7 + (c - b'a') as usize,
            _ => return None,
        })
    }

    fn metrics(&self, c: u8, index: usize) -> (u16, u16) {
        let slot = if c == b' ' { 0xD1 } else { index };
        if (0x93..=0xD1).contains(&slot) { self.prop[slot - 0x93] } else { (0, 16) }
    }

    /// Width in pixels of a line rendered proportionally.
    pub fn width(&self, text: &str) -> usize {
        text.bytes().filter_map(|c| Self::index(c).map(|i| self.metrics(c, i).1 as usize)).sum()
    }

    /// Renders a line as an 8-bit mask (0 or 255), `TEXT_ROW_HEIGHT` rows: rows shifted left
    /// by the glyph's shift, each pixel ORed with its left neighbour (the shell's 1-px bold),
    /// and the next glyph overwriting from its predecessor's advance on.
    pub fn render(&self, text: &str) -> (usize, Vec<u8>) {
        let width = self.width(text).max(1);
        let mut mask = vec![0u8; width * TEXT_ROW_HEIGHT];
        let mut x = 0usize;
        for c in text.bytes() {
            let Some(index) = Self::index(c) else { continue };
            let (shift, advance) = self.metrics(c, index);
            let g = &self.glyphs[index * 30..index * 30 + 30];
            for row in 0..15 {
                let bits = ((g[row * 2] as u32) << 8 | g[row * 2 + 1] as u32) << shift as u32;
                let bold = bits | (bits >> 1);
                for col in 0..16usize {
                    let px = x + col;
                    if px >= width { break }
                    // The next glyph overwrites from `advance`; model that by clipping here.
                    if col >= advance as usize { break }
                    let on = bold >> (15 - col) & 1 != 0;
                    mask[row * width + px] = if on { 255 } else { 0 };
                }
            }
            x += advance as usize;
        }
        (width, mask)
    }
}

/// One entry of the shell's note table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct NoteEvent {
    /// NTSC fields from the first text frame (PAL: × 5/6).
    pub time: u32,
    /// VAB program number.
    pub prog: u8,
    /// MIDI note number.
    pub note: u8,
    /// Velocity, 0..=127; 0 = key off.
    pub vel: u8,
    /// Pan, 0..=127 (64 = centre).
    pub pan: u8,
}

#[derive(Debug, Clone, Copy)]
struct Tone {
    vol: u8,
    pan: u8,
    centre: u8,
    shift: u8,
    adsr1: u16,
    adsr2: u16,
    vag: u8,
}

#[derive(Debug, Clone)]
struct Program {
    vol: u8,
    tones: Vec<Tone>,
}

/// The shell's VAB: programs of two stereo tones over three ADPCM samples. Read by
/// [`Ps1Shell::load`] and played by [`Ps1Shell::render_sound`]; it has no public fields.
#[derive(Debug, Clone)]
pub struct Vab {
    programs: Vec<Program>,
    master: u8,
    /// Decoded samples (SPU scale, 44.1 kHz) and their loop starts, index = VAG number.
    samples: Vec<(Vec<f32>, Option<usize>)>,
}

impl Vab {
    fn parse(d: &[u8]) -> Result<Self> {
        if d.len() < 32 || &d[..4] != b"pBAV" {
            return Err(Error::Corrupt(Format::SoundBank, "no pBAV header".into()));
        }
        let (n_prog, n_vag, master) = (d.u16(18) as usize, d.u16(22) as usize, d.u8(24));
        let prog_tab = 32;
        let tone_tab = prog_tab + 128 * 16;
        let vag_tab = tone_tab + n_prog * 16 * 32;
        let body = vag_tab + 512;
        if body > d.len() || n_prog > 128 || n_vag > 254 {
            return Err(Error::Corrupt(Format::SoundBank, "implausible VAB header".into()));
        }
        let mut programs = Vec::new();
        for p in 0..n_prog {
            let o = prog_tab + p * 16;
            let (n_tones, vol) = (d.u8(o) as usize, d.u8(o + 1));
            let tones = (0..n_tones.min(16)).map(|t| {
                let o = tone_tab + (p * 16 + t) * 32;
                Tone { vol: d.u8(o + 2), pan: d.u8(o + 3), centre: d.u8(o + 4), shift: d.u8(o + 5), adsr1: d.u16(o + 16), adsr2: d.u16(o + 18), vag: d.u8(o + 22) }
            }).collect();
            programs.push(Program { vol, tones });
        }
        let mut samples = vec![(Vec::new(), None)];
        let mut off = body;
        for v in 1..=n_vag {
            let size = d.u16(vag_tab + v * 2) as usize * 8;
            if off + size > d.len() { return Err(Error::Corrupt(Format::SoundBank, "VAG past the end of the bank".into())) }
            samples.push(decode_adpcm(&d[off..off + size], 0));
            off += size;
        }
        Ok(Self { programs, master, samples })
    }
}

/// Licence comparison policy read from shell code (`ps1_version_matrix.md` §3.4,
/// `ps1_kernel_boot.md` §3.3). It is independent of the reported version number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum LicencePolicy {
    /// The compare is executed without a check flag (early Japanese shells).
    Unconditional,
    /// The compare's flag is stored as a constant one.
    Always,
    /// The flag is stored as zero, or the shell has no licence comparisons.
    Never,
    /// The region routine sets the flag, then clears it for letter A.
    ByLetter,
}

impl LicencePolicy {
    /// Evaluates the code's policy for a tail/ROMVER letter; ByLetter skips only A.
    pub fn checks(self, letter: Option<char>) -> bool {
        match self { Self::Unconditional | Self::Always => true, Self::Never => false, Self::ByLetter => letter != Some('A') }
    }
}

/// Everything the licence screen needs from a standalone shell or `rom0:LOGO`, by content.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Ps1Shell {
    /// The shell's reference logo; empty when absent, preserving the original PS2 API.
    /// Use [`reference_logo`](Self::reference_logo) to distinguish absent reference data.
    pub logo: Tmd,
    /// The [`LOGO_TMD_BYTES`] bytes of that copy; empty in shells without a reference.
    pub logo_bytes: Vec<u8>,
    /// The selected 200x40 "PlayStation" wordmark for this dump's policy and letter.
    pub wordmark: Tim,
    /// All 200x40 wordmark TIMs in image order: registered first, trademark second if present.
    /// `ps1_shell_scenes.md` §5 item 4; `wordmark` is the one selected for this dump.
    pub wordmarks: Vec<Tim>,
    /// The 20x8 TM mark.
    pub tm: Tim,
    /// The kernel font with the shell's proportional widths.
    pub font: Font,
    /// Light directions, 4.12, one row per light.
    pub light_dirs: [[i16; 3]; 3],
    /// Light colours, rows R, G, B, columns light 1..3, 4.12.
    pub light_colours: [[i16; 3]; 3],
    /// The logo's rotation matrix, 4.12.
    pub rotation: [[i16; 3]; 3],
    /// The logo's translation (model units; z is the viewing distance).
    pub translation: [i32; 3],
    /// The note table, in time order.
    pub events: Vec<NoteEvent>,
    /// The shell's sound bank.
    pub bank: Vab,
    /// The PS2 ROMVER or complete standalone tail version string; empty if absent.
    pub rom_version: String,
    /// Licence policy established from the shell's compare gate and flag stores.
    pub licence_policy: LicencePolicy,
    /// Every stored licence line (zero, one or three); matrix §3.4 includes 2.0 E and POPS.
    pub licence_strings: Vec<String>,
}

fn find(d: &[u8], pat: &[u8], from: usize) -> Option<usize> {
    d.get(from..)?.windows(pat.len()).position(|w| w == pat).map(|p| p + from)
}

fn mat3(d: &[u8], o: usize) -> [[i16; 3]; 3] {
    let m = |i: usize| [d.i16(o + i * 6), d.i16(o + i * 6 + 2), d.i16(o + i * 6 + 4)];
    [m(0), m(1), m(2)]
}

// RotMatrix uses 1/4096-turn angles, Rx·Ry·Rz, stored in 4.12. Scenes §5 item 1.
fn rotation_from_angles(angles: [i16; 3]) -> [[i16; 3]; 3] {
    let [(sx, cx), (sy, cy), (sz, cz)] = angles.map(|a| (f64::from(a) * std::f64::consts::TAU / 4096.0).sin_cos());
    let m = [[cy * cz, -cy * sz, sy],
        [cx * sz + sx * sy * cz, cx * cz - sx * sy * sz, -sx * cy],
        [sx * sz - cx * sy * cz, sx * cz + cx * sy * sz, cx * cy]];
    m.map(|row| row.map(|v| (v * 4096.0).round() as i16))
}

// Resolve only nearby constant construction (LUI/ADDIU/ORI), never execute user code.
fn code_constant(d: &[u8], before: usize, reg: u32, depth: usize) -> Option<u32> {
    if reg == 0 { return Some(0) }
    if depth > 3 { return None }
    for o in (before.saturating_sub(32)..before).step_by(4).rev() {
        let w = d.u32(o);
        let (op, rs, rt) = (w >> 26, w >> 21 & 31, w >> 16 & 31);
        if op == 0 && w >> 11 & 31 == reg && w & 63 != 8 { return None }
        if rt != reg { continue }
        match op {
            15 => return Some((w & 0xFFFF) << 16),
            8 | 9 => return code_constant(d, o, rs, depth + 1).map(|v| v.wrapping_add(w as i16 as i32 as u32)),
            13 => return code_constant(d, o, rs, depth + 1).map(|v| v | (w & 0xFFFF)),
            10..=14 | 32..=38 => return None,
            _ => {}
        }
    }
    None
}

fn memory_address(d: &[u8], o: usize) -> Option<u32> {
    code_constant(d, o, d.u32(o) >> 21 & 31, 0).map(|base| base.wrapping_add(d.i16(o) as i32 as u32))
}

// Dump correction to scenes §5 item 1: 1.0/1.1 leave these vectors zero in data and
// initialise them with SH/SW in the drawing code (1.0 +0x11A3C, 1.1 +0x10920).
// Locate a complete SVECTOR/VECTOR initialization, with code references to both.
fn runtime_transform(d: &[u8]) -> Option<([i16; 3], [i32; 3])> {
    for o in (0..d.len().saturating_sub(3)).step_by(4) {
        let w = d.u32(o);
        if w >> 26 != 41 { continue }
        let Some(base) = memory_address(d, o).filter(|&a| a <= u32::MAX - 16) else { continue };
        let Some(first) = code_constant(d, o, w >> 16 & 31, 0) else { continue };
        if first == 0 || i32::from(first as i16).abs() > 4096 { continue }
        let mut values = [None; 6];
        for p in (o.saturating_sub(16)..(o + 68).min(d.len().saturating_sub(3))).step_by(4) {
            let instruction = d.u32(p);
            let op = instruction >> 26;
            if !matches!(op, 41 | 43) { continue }
            let Some(address) = memory_address(d, p) else { continue };
            let offsets = [0, 2, 4, 8, 12, 16];
            let Some(i) = offsets.iter().position(|&delta| address == base + delta) else { continue };
            if (i < 3 && op == 41) || (i >= 3 && op == 43) {
                values[i] = code_constant(d, p, instruction >> 16 & 31, 0);
            }
        }
        let [Some(x), Some(y), Some(z), Some(tx), Some(ty), Some(tz)] = values else { continue };
        let angles = [x as i16, y as i16, z as i16];
        if angles.iter().any(|&v| i32::from(v).abs() > 4096) || tz as i32 <= 0 { continue }
        if address_refs(d, base).next().is_some() && address_refs(d, base + 8).next().is_some() {
            return Some((angles, [tx as i32, ty as i32, tz as i32]));
        }
    }
    None
}

fn address_refs(d: &[u8], address: u32) -> impl Iterator<Item = usize> + '_ {
    (0..d.len().saturating_sub(3)).step_by(4).filter(move |&o| {
        let w = d.u32(o);
        matches!(w >> 26, 8 | 9 | 13) && code_constant(d, o + 4, w >> 16 & 31, 0) == Some(address)
    })
}

fn tim_offset(d: &[u8], index: usize) -> Option<usize> {
    let (mut p, mut n) = (0, 0);
    while let Some(i) = find(d, &[0x10, 0, 0, 0, 8, 0, 0, 0], p) {
        if let Ok(t) = Tim::parse_4bit(&d[i..]) {
            if (t.width, t.height) == (200, 40) {
                if n == index { return Some(i) }
                n += 1;
            }
        }
        p = i + 8;
    }
    None
}

// Bind a compare gate to a referenced licence string, then inspect stores to that same
// word. Matrix §3.4 and kernel §3.3 give the evidence; no version-to-policy mapping.
fn locate_policy(d: &[u8], strings: &[usize]) -> Result<LicencePolicy> {
    if strings.is_empty() { return Ok(LicencePolicy::Never) }
    let bad = || Error::Corrupt(Format::Bios, "PS1 shell: unrecognised licence flag code".into());
    let mut references: Vec<_> = strings.iter().flat_map(|&s| address_refs(d, 0x80030000 + s as u32)).filter(|&o| d.u32(o) >> 16 & 31 == 5).collect();
    references.sort_unstable();
    let reference = *references.first().ok_or_else(bad)?;
    let mut flag = None;
    for o in (reference.saturating_sub(192)..reference).step_by(4).rev() {
        let w = d.u32(o);
        let (rs, rt) = (w >> 21 & 31, w >> 16 & 31);
        if w >> 26 != 4 || rs == rt || (rs != 0 && rt != 0) { continue }
        let target = o as i64 + 4 + i64::from(w as i16) * 4;
        if target <= reference as i64 || target > reference as i64 + 256 { continue }
        let reg = rs.max(rt);
        for p in (o.saturating_sub(16)..o).step_by(4).rev() {
            if d.u32(p) >> 26 == 35 && d.u32(p) >> 16 & 31 == reg {
                flag = memory_address(d, p);
                break;
            }
        }
        if flag.is_some() { break }
    }
    let Some(flag) = flag else { return Ok(LicencePolicy::Unconditional) };
    let (mut zero, mut one, mut by_letter) = (false, false, false);
    for o in (0..d.len().min(0x2000).saturating_sub(3)).step_by(4) {
        let w = d.u32(o);
        if w >> 26 != 43 || memory_address(d, o) != Some(flag) { continue }
        match code_constant(d, o, w >> 16 & 31, 0) {
            Some(0) => {
                zero = true;
                // A BNE against literal 'A' skips this exact zero store.
                by_letter |= (o.saturating_sub(24)..o).step_by(4).any(|p| {
                    let b = d.u32(p);
                    b >> 26 == 5 && (code_constant(d, p, b >> 16 & 31, 0) == Some(u32::from(b'A'))
                        || code_constant(d, p, b >> 21 & 31, 0) == Some(u32::from(b'A')))
                        && p as i64 + 4 + i64::from(b as i16) * 4 > o as i64
                });
            }
            Some(1) => one = true,
            _ => return Err(bad()),
        }
    }
    match (zero, one, by_letter) {
        (true, true, true) => Ok(LicencePolicy::ByLetter),
        (true, false, _) => Ok(LicencePolicy::Never),
        (false, true, _) => Ok(LicencePolicy::Always),
        (false, false, _) => {
            // PS2 1.00 J has an initialized constant flag in its image, with no stores.
            let offset = flag.checked_sub(0x80030000).map(|o| o as usize).ok_or_else(bad)?;
            if offset.checked_add(4).is_none_or(|end| end > d.len()) { return Err(bad()) }
            match d.u32(offset) { 0 => Ok(LicencePolicy::Never), 1 => Ok(LicencePolicy::Always), _ => Err(bad()) }
        }
        _ => Err(bad()),
    }
}

impl Ps1Shell {
    /// Unpacks `rom0:LOGO` from a BIOS dump and locates its tables; also reads `rom0:KROM`
    /// and `rom0:ROMVER`. Fails with [`Error::Corrupt`] naming the first table not found.
    pub fn load(rom: &RomDir) -> Result<Self> {
        let module = rom.module("LOGO")?;
        // {load address, size}, a 0x44-byte copy loader, then the LZ stream.
        let image = unpack(module, 0x54).map_err(|_| Error::Corrupt(Format::Bios, "rom0:LOGO: no LZ stream at the expected offset".into()))?;
        let version = String::from_utf8_lossy(rom.module("ROMVER").unwrap_or(b"")).trim_end_matches('\0').to_string();
        Self::locate(&image, rom.module("KROM")?, &version)
    }

    /// Loads a standalone shell and its ROM font (`ps1_shell_scenes.md` §5 items 1–5).
    pub fn load_ps1(bios: &Ps1Bios) -> Result<Self> {
        Self::locate(&bios.shell_image()?, bios.krom(), &bios.identity().version)
    }

    /// Locates tables in a shell image linked at 0x80030000 with its KROM and version text.
    /// Standalone matrix/asset layouts: `ps1_shell_scenes.md` §5 items 1–5.
    /// Policy follows matrix §3.4, correcting the version-based rule in scenes §5 item 3.
    pub fn locate(d: &[u8], krom: &[u8], version: &str) -> Result<Self> {
        let miss = |what: &str| Error::Corrupt(Format::Bios, format!("PS1 shell: {what} not found or truncated"));
        let mut logo_bytes = Vec::new();
        let mut logo = Tmd { verts: Vec::new(), normals: Vec::new(), prims: Vec::new() };
        let mut p = 0;
        while let Some(t) = find(d, &[0x41, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0], p) {
            if let Some(bytes) = d.get(t..t + LOGO_TMD_BYTES) {
                if let Ok(model) = Tmd::parse(bytes) {
                    if !model.verts.is_empty() && !model.normals.is_empty() && !model.prims.is_empty() {
                        logo = model; logo_bytes = bytes.to_vec(); break;
                    }
                }
            }
            p = t + 4;
        }

        let mut licence_strings = Vec::new();
        let mut string_offsets = Vec::new();
        let mut p = 0;
        while let Some(i) = find(d, b"          Licensed  by", p) {
            let text = d.cstr(i, 73);
            if !matches!(text.len(), 64 | 67 | 70) || d.get(i + text.len()) != Some(&0) { return Err(miss("licence string")) }
            string_offsets.push(i);
            licence_strings.push(text);
            p = i + 1;
        }
        let licence_policy = locate_policy(d, &string_offsets)?;

        let mut tims = Vec::new();
        let mut p = 0;
        while let Some(i) = find(d, &[0x10, 0, 0, 0, 8, 0, 0, 0], p) {
            if let Ok(t) = Tim::parse_4bit(&d[i..]) { tims.push(t) }
            p = i + 8;
        }
        let wordmarks: Vec<Tim> = tims.iter().filter(|t| t.width == 200 && t.height == 40).cloned().collect();
        let wordmark = wordmarks.first().cloned().ok_or_else(|| miss("wordmark TIM"))?;
        let tm = tims.iter().find(|t| t.width == 20 && t.height == 8).cloned().ok_or_else(|| miss("TM TIM"))?;

        // Proportional table: '0' = {0,16}, '1' = {2,8}, '2' = {0,16}.
        let pp = find(d, &[0, 0, 16, 0, 2, 0, 8, 0, 0, 0, 16, 0], 0).ok_or_else(|| miss("font width table"))?;
        if pp + (0xD2 - 0x93) * 4 > d.len() { return Err(miss("font width table")) }
        let prop: Vec<_> = (0..=0xD1 - 0x93).map(|i| (d.u16(pp + i * 4), d.u16(pp + i * 4 + 2))).collect();
        if prop.iter().any(|&(shift, advance)| shift > 15 || advance > 16) { return Err(miss("font width table")) }
        if krom.len() < 0xD2 * 30 { return Err(Error::Corrupt(Format::Bios, "rom0:KROM is too short".into())) }
        let font = Font { glyphs: krom.to_vec(), prop };

        let mut ld = Vec::new();
        for v in [0i16, 0, 4000, 0, -4000, 0, -4000, 0, 0] { ld.extend_from_slice(&v.to_le_bytes()) }
        let lo = find(d, &ld, 0).ok_or_else(|| miss("light matrix"))?;
        if lo + 0x68 > d.len() { return Err(miss("matrix block")) }
        let light_dirs = mat3(d, lo);
        // Colour matrix 0x20 bytes later, rotation 0x40 bytes later, translation after it.
        let light_colours = mat3(d, lo + 0x20);
        let mut rotation = mat3(d, lo + 0x40);
        let mut translation = [d.i32(lo + 0x54), d.i32(lo + 0x58), d.i32(lo + 0x5C)];
        if rotation.iter().flatten().any(|&v| i32::from(v).abs() > 4096) {
            let mut angles = [d.i16(lo + 0x54), d.i16(lo + 0x56), d.i16(lo + 0x58)];
            translation = [d.i32(lo + 0x5C), d.i32(lo + 0x60), d.i32(lo + 0x64)];
            if angles == [0; 3] {
                (angles, translation) = runtime_transform(d).ok_or_else(|| miss("runtime transform stores"))?;
            }
            rotation = rotation_from_angles(angles);
        }
        if translation[2] <= 0 {
            return Err(miss("rotation matrix"));
        }

        // Note table: {u32 time, prog, note, vel, pan}, first event at time 0 with velocity ≥ 1,
        // terminated by time 9999.
        let eo = find(d, &[0, 0, 0, 0, 0, 31, 80, 64], 0).ok_or_else(|| miss("note table"))?;
        let mut events = Vec::new();
        let mut q = eo;
        loop {
            if q + 8 > d.len() || events.len() > 64 { return Err(miss("note table terminator")) }
            let time = d.u32(q);
            if time == 9999 { break }
            events.push(NoteEvent { time, prog: d.u8(q + 4), note: d.u8(q + 5), vel: d.u8(q + 6), pan: d.u8(q + 7) });
            q += 8;
        }
        let vo = find(d, b"pBAV", 0).ok_or_else(|| miss("VAB"))?;
        let bank = Vab::parse(&d[vo..])?;
        // Older shells choose the second TIM when their flag is clear. This includes 4.1
        // (+0x1D218 flag load, +0x1D228 branch), correcting scenes §5's "4.x always ®".
        // 4.5's second TIM is unused: require a code reference, independent of the version.
        let trademark_used = !licence_policy.checks(region_letter(version)) && wordmarks.len() > 1
            && tim_offset(d, 1).is_some_and(|o| address_refs(d, 0x80030000 + o as u32).next().is_some());
        let wordmark = if trademark_used { wordmarks[1].clone() } else { wordmark };
        Ok(Self { logo, logo_bytes, wordmark, wordmarks, tm, font, light_dirs, light_colours, rotation, translation, events, bank,
            rom_version: version.into(), licence_policy, licence_strings })
    }

    /// The optional ROM reference model; absent shells display the disc's own model.
    pub fn reference_logo(&self) -> Option<&Tmd> { (!self.logo_bytes.is_empty()).then_some(&self.logo) }

    /// ROM logo bytes to compare, only when this shell's code enables comparison.
    pub fn comparison_logo_bytes(&self) -> Option<&[u8]> {
        (self.checks_licence() && !self.logo_bytes.is_empty()).then_some(self.logo_bytes.as_slice())
    }

    /// Whether this shell's policy enables licence comparison for this dump's letter.
    pub fn checks_licence(&self) -> bool { self.licence_policy.checks(region_letter(&self.rom_version)) }

    /// The four letters the licence screen shows under the text: "SCE" + the drive's region
    /// letter (E/A/I). The letters a PS2 drive reports were not verified; this follows the ROM.
    pub fn sce_id(&self) -> String {
        let r = match region_letter(&self.rom_version) { Some('E') => 'E', Some('A') => 'A', _ => 'I' };
        format!("SCE{r}")
    }
}

/// Fields of each phase of the licence screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LicenceTimeline {
    /// The video mode, which scales the note table's times.
    pub video: VideoMode,
}

impl LicenceTimeline {
    /// Fields of the logo fade (the GTE depth cue), before the text appears.
    pub const FADE_FIELDS: usize = 31;
    /// Fields over which the wordmark ramps up while the text is shown.
    pub const TEXT_FIELDS: usize = 30;
    /// The timeline for a video mode.
    #[must_use]
    pub fn new(video: VideoMode) -> Self { Self { video } }
    /// Ticks after the text phase until the note table's terminator (NTSC 51, PAL 42 ticks in
    /// all, so 22 / 13 more), plus the final VSync.
    #[must_use]
    pub fn tail_fields(&self, events_end: u32) -> usize {
        let last = if self.video == VideoMode::Pal { events_end * 5 / 6 } else { events_end };
        (last as usize + 1).saturating_sub(Self::TEXT_FIELDS) + 1
    }
    /// Fields from the first logo frame until the shell's last note-table tick: the fade,
    /// the text phase and the tail (83 NTSC, 74 PAL for the studied shell).
    #[must_use]
    pub fn total_fields(&self, shell: &Ps1Shell) -> usize {
        let end = shell.events.iter().map(|e| e.time).max().unwrap_or(0) + 1;
        Self::FADE_FIELDS + Self::TEXT_FIELDS + self.tail_fields(end)
    }
    /// The GTE fog "near" value during the fade: frame × 133 − 6980 for frames 60..=90.
    pub fn fog_near(field: usize) -> i32 { (60 + field.min(30) as i32) * 133 - 6980 }
    /// Brightness of the wordmark sprite over 128 during the text phase: 0, 5, … 145.
    pub fn wordmark_level(field_in_text: usize) -> f32 { (field_in_text.min(29) * 5) as f32 / 128.0 }
}

/// A triangle projected the way the shell's GTE path does: screen position (640×480 frame),
/// depth, and the lit colour with the depth cue applied.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct ScreenTri {
    /// Screen positions of the three corners in the 640x480 frame.
    pub xy: [[f32; 2]; 3],
    /// Mean depth, used for sorting and the depth cue.
    pub sz: f32,
    /// RGB, 0..=1.
    pub colour: [f32; 3],
}

/// One lit, flat triangle of the logo in the GTE's camera space (x right, y down, z into
/// the screen, in the shell's units), before projection: what another camera would see.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct LitTri {
    /// The three corners in camera space.
    pub xyz: [[f32; 3]; 3],
    /// RGB, 0..=1, with the lighting and the field's depth cue applied.
    pub colour: [f32; 3],
}

impl Ps1Shell {
    /// Transforms and lights the logo for one field of the fade (`field` is clamped to 30),
    /// as the shell's GTE would, but stops before projection: every triangle, in TMD order,
    /// in camera space. [`project`](Self::project) finishes the shell's own picture.
    #[must_use]
    pub fn lit_triangles(&self, tmd: &Tmd, field: usize) -> Vec<LitTri> {
        let a = LicenceTimeline::fog_near(field) as f32;
        let r = &self.rotation;
        let t = &self.translation;
        let xf = |v: [i16; 3]| -> [f32; 3] {
            let (x, y, z) = (v[0] as i64, v[1] as i64, v[2] as i64);
            let row = |i: usize| ((r[i][0] as i64 * x + r[i][1] as i64 * y + r[i][2] as i64 * z) >> 12) as f32 + t[i] as f32;
            [row(0), row(1), row(2)]
        };
        let mut out = Vec::with_capacity(tmd.prims.len());
        for p in &tmd.prims {
            let Some(v) = p.verts.iter().map(|&i| tmd.verts.get(i as usize).copied()).collect::<Option<Vec<_>>>() else { continue };
            let w: Vec<[f32; 3]> = v.into_iter().map(xf).collect();
            if w.len() != 3 { continue }
            let sz = (w[0][2] + w[1][2] + w[2][2]) / 3.0;
            // NCDS: three directional lights plus the back colour, then the TMD colour.
            let n = tmd.normals.get(p.normal as usize).copied().unwrap_or([0, 0, 4096]);
            let ll: Vec<f32> = (0..3).map(|i| {
                let d = self.light_dirs[i];
                (((d[0] as i64 * n[0] as i64 + d[1] as i64 * n[1] as i64 + d[2] as i64 * n[2] as i64) >> 12) as f32).max(0.0)
            }).collect();
            let lit = |c: usize| {
                let sum: f32 = (0..3).map(|i| self.light_colours[c][i] as f32 * ll[i]).sum();
                ((sum / 4096.0 + 100.0 * 16.0) / 4096.0).clamp(0.0, 1.0)         // back colour 100 → 1600 (4.12 of 0..255 × 16)
            };
            let cue = if sz != 0.0 { (1.25 * (1.0 - a / sz)).clamp(0.0, 1.0) } else { 1.0 };
            let colour = [0, 1, 2].map(|c| p.colour[c] as f32 / 255.0 * lit(c) * (1.0 - cue));
            out.push(LitTri { xyz: [w[0], w[1], w[2]], colour });
        }
        out
    }

    /// Projects and lights the logo for one field of the fade (`field` is clamped to 30):
    /// flat triangles, back faces dropped, sorted far to near, with the depth cue applied.
    #[must_use]
    pub fn project(&self, tmd: &Tmd, field: usize) -> Vec<ScreenTri> {
        let mut out = Vec::with_capacity(tmd.prims.len());
        for tri in self.lit_triangles(tmd, field) {
            let w = &tri.xyz;
            if w.iter().any(|q| q[2] <= 1.0) { continue }
            let s: Vec<[f32; 2]> = w.iter().map(|q| [320.0 + q[0] * 1024.0 / q[2], 240.0 + q[1] * 1024.0 / q[2]]).collect();
            // NCLIP: the signed area; the GTE keeps clockwise triangles (positive here).
            let area = (s[1][0] - s[0][0]) * (s[2][1] - s[0][1]) - (s[2][0] - s[0][0]) * (s[1][1] - s[0][1]);
            if area <= 0.0 { continue }
            let sz = (w[0][2] + w[1][2] + w[2][2]) / 3.0;
            out.push(ScreenTri { xy: [s[0], s[1], s[2]], sz, colour: tri.colour });
        }
        out.sort_by(|a, b| b.sz.total_cmp(&a.sz));
        out
    }

    /// Where the text pieces go: `((x of line 1, y), (x of line 2, y))` in the 640x480 frame,
    /// from the licence line's length as the shell switches on it.
    #[must_use]
    pub fn text_layout(licence_len: usize) -> ((f32, f32), (f32, f32)) {
        let x2 = match licence_len { 64 => 154.0, 67 => 128.0, _ => 118.0 };
        ((221.0, 336.0), (x2, 356.0))
    }

    /// The licence screen's sound: the two drone notes under the fade, then the note table
    /// from the first text frame, as 48 kHz interleaved stereo starting at field 0.
    #[must_use]
    pub fn render_sound(&self, video: VideoMode) -> Vec<f32> {
        let fps = video.fps() as f64;
        let fields_to_samples = |f: f64| (f / fps * SAMPLE_RATE as f64) as usize;
        struct Voice { start: usize, off: Option<usize>, tone: Tone, prog_vol: u8, vel: u8, pan: u8, prog: u8, note: u8 }
        let mut voices: Vec<Voice> = Vec::new();
        let key = |voices: &mut Vec<Voice>, field: f64, prog: u8, note: u8, vel: u8, pan: u8| {
            let at = fields_to_samples(field);
            if vel == 0 {
                for v in voices.iter_mut().filter(|v| v.prog == prog && v.note == note && v.off.is_none()) { v.off = Some(at) }
            } else if let Some(p) = self.bank.programs.get(prog as usize) {
                for t in &p.tones {
                    voices.push(Voice { start: at, off: None, tone: *t, prog_vol: p.vol, vel, pan, prog, note });
                }
            }
        };
        // Phase A: the drones, keyed before the first frame and off after the 31st.
        key(&mut voices, 0.0, 2, 31, 0x7F, 0x40);
        key(&mut voices, 0.0, 0, 36, 0x3C, 0x40);
        key(&mut voices, LicenceTimeline::FADE_FIELDS as f64, 2, 31, 0, 0);
        key(&mut voices, LicenceTimeline::FADE_FIELDS as f64, 0, 36, 0, 0);
        let scale = if video == VideoMode::Pal { 5.0 / 6.0 } else { 1.0 };
        for e in &self.events {
            key(&mut voices, LicenceTimeline::FADE_FIELDS as f64 + e.time as f64 * scale, e.prog, e.note, e.vel, e.pan);
        }
        let tail = fields_to_samples(fps * 4.0);
        let total = voices.iter().map(|v| v.off.unwrap_or(v.start)).max().unwrap_or(0) + tail;
        let mut mix = vec![0f32; total * 2];
        for v in &voices {
            let Some((pcm, lp)) = self.bank.samples.get(v.tone.vag as usize) else { continue };
            if pcm.len() < 2 { continue }
            let n = total.saturating_sub(v.start);
            let env = envelope(v.tone.adsr1, v.tone.adsr2, v.off.map(|o| o - v.start).unwrap_or(n), n);
            // libsnd: velocity × tone volume × program volume × bank master, then the tone's
            // pan combined with the note's pan (a stereo pair: one tone hard left, one hard right).
            let gain = v.vel as f32 / 127.0 * v.tone.vol as f32 / 127.0 * v.prog_vol as f32 / 127.0 * self.bank.master as f32 / 127.0 * (0x3FFF as f32 / 16384.0);
            let pan = ((v.tone.pan as f32 + v.pan as f32 - 64.0) / 127.0).clamp(0.0, 1.0);
            let (gain_l, gain_r) = (gain * (1.0 - pan).sqrt(), gain * pan.sqrt());
            let semis = v.note as f32 - v.tone.centre as f32 + v.tone.shift as f32 / 128.0;
            let step = 2f64.powf(semis as f64 / 12.0) * 44100.0 / SAMPLE_RATE as f64;
            let length = pcm.len();
            let mut pos = 0.0f64;
            for (k, &e) in env.iter().enumerate() {
                let mut i0 = pos as usize;
                let frac = (pos - i0 as f64) as f32;
                let mut i1 = i0 + 1;
                if let Some(l) = *lp {
                    let span = length - l;
                    if i0 >= length { i0 = l + (i0 - l) % span }
                    if i1 >= length { i1 = l + (i1 - l) % span }
                } else if i1 >= length {
                    break;
                }
                let s = (pcm[i0] * (1.0 - frac) + pcm[i1] * frac) * e / 32768.0 / 32768.0;
                mix[(v.start + k) * 2] += s * gain_l;
                mix[(v.start + k) * 2 + 1] += s * gain_r;
                pos += step;
            }
        }
        mix
    }
}
