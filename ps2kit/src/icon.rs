//! The user's PS2 save icons; no geometry, textures or lighting tables are embedded.
//!
//! \[I\] Layout sources: ps2sdk `libmc.h` (`mcIcon`), mymc `ps2save.py`
//! (`unpack_icon_sys`/`icon_sys_title`), PS2 IconSys `ps2_ps2icon.cpp`/`.hpp`
//! (`ReadFile`, `Frame_Data`), and HDLGameInstaller `IconLoader.h`/`.c`.
//! <https://github.com/ticky/ps2iconsys/blob/develop/src/ps2_ps2icon.cpp>
//! <https://github.com/ps2dev/ps2sdk/blob/master/ee/rpc/memorycard/include/libmc.h>
//! <https://github.com/ps2dev/mymc/blob/master/ps2save.py>
//! <https://github.com/ps2homebrew/HDLGameInstaller/blob/main/IconLoader.h>
//!
//! \[V\] Mcd001.ps2: all nine icon.sys files are 964 bytes; model attributes are
//! 3 (no texture), 7 (raw) or 15 (RLE). Animation tracks have an **8-byte**
//! header, then keys, contrary to the 16-byte header in some ICN descriptions.
//! E.g. GTA's animation starts at file offset 0x80E4, texture at 0x8108;
//! DBZ's RLE length at 0x83D8 is 6334 bytes, decoding exactly 16384 pixels.
//! The purported reserved float at model offset 12 is 3.0 in FIFA/SSX; retain
//! it as the back-face parameter, without requiring 1.0. Single-shape files
//! sometimes name shape 1 rather than 0; they still refer to their sole shape.
//! Coordinates use IconSys' /4096 convention (HDLGameInstaller uses /1024
//! for positions); this is a uniform model-unit choice, removed by fitting.

use crate::memcard::MemoryCard;
use crate::{Error, Format, Result, Vec2, Vec3, Vec4};

const PIXELS: usize = 128 * 128;
const MAX_SHAPES: usize = 8;

fn corrupt(message: &str) -> Error { Error::Corrupt(Format::MemoryCard, format!("save icon: {message}")) }

struct Reader<'a> { data: &'a [u8], offset: usize }
impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.offset.checked_add(n).ok_or_else(|| corrupt("length overflow"))?;
        let b = self.data.get(self.offset..end).ok_or_else(|| corrupt("truncated data"))?;
        self.offset = end;
        Ok(b)
    }
    fn u16(&mut self) -> Result<u16> { let b = self.take(2)?; Ok(u16::from_le_bytes([b[0], b[1]])) }
    fn u32(&mut self) -> Result<u32> { let b = self.take(4)?; Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]])) }
    fn float(&mut self) -> Result<f32> {
        let v = f32::from_bits(self.u32()?);
        if !v.is_finite() { return Err(corrupt("non-finite float")) }
        Ok(v)
    }
    fn vector(&mut self) -> Result<Vec4> { Ok(Vec4::new(self.float()?, self.float()?, self.float()?, self.float()?)) }
    fn fixed(&mut self) -> Result<Vec4> {
        Ok(Vec4::new(self.u16()? as i16 as f32, self.u16()? as i16 as f32, self.u16()? as i16 as f32, self.u16()? as i16 as f32) / 4096.0)
    }
}

/// Browser metadata from `icon.sys` (\[I\] ps2sdk `mcIcon`, mymc `unpack_icon_sys`).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct IconSys {
    /// Browser data category; preserve unknown categories.
    pub file_type: u16,
    /// Byte offset of the second Shift-JIS title line (not a character count).
    pub title_offset: u16,
    /// Undocumented word at offset 8.
    pub reserved: u32,
    /// Background transparency in the file's GS units.
    pub background_alpha: u32,
    /// Four corner colours, preserved as four 32-bit components each.
    pub background_colours: [[u32; 4]; 4],
    /// The three directional lights, including their stored fourth component.
    pub light_directions: [Vec4; 3],
    /// The three light colours in floating-point units.
    pub light_colours: [Vec4; 3],
    /// Ambient light colour.
    pub ambient: Vec4,
    /// Unmodified 68-byte Shift-JIS title field, including padding.
    pub title_sjis: Vec<u8>,
    /// Decoded title lines; unsupported Kanji use the shared PS1 decoder's replacement.
    pub title_lines: [String; 2],
    /// File displayed when listing the save.
    pub list_file: String,
    /// File displayed when copying the save (may be empty).
    pub copy_file: String,
    /// File displayed when deleting the save (may be empty).
    pub delete_file: String,
}

fn filename(b: &[u8], required: bool) -> Result<String> {
    let end = b.iter().position(|&v| v == 0).ok_or_else(|| corrupt("unterminated icon filename"))?;
    let name = std::str::from_utf8(&b[..end]).map_err(|_| corrupt("non-UTF8 icon filename"))?;
    // MemoryCard also accepts folder cards: never follow an icon filename out of its save.
    if (required && name.is_empty()) || name == "." || name == ".." || name.bytes().any(|c| c == b'/' || c == b'\\' || c == b':' || c.is_ascii_control()) {
        return Err(corrupt("icon filename is not a single path component"));
    }
    Ok(name.into())
}

impl IconSys {
    /// Parses a complete 964-byte descriptor, tolerating extra trailing bytes.
    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut r = Reader { data, offset: 0 };
        if r.take(4)? != b"PS2D" { return Err(corrupt("icon.sys PS2D signature missing")) }
        if data.len() < 964 { return Err(corrupt("short icon.sys")) }
        let file_type = r.u16()?;
        let title_offset = r.u16()?;
        let reserved = r.u32()?;
        let background_alpha = r.u32()?;
        let mut background_colours = [[0; 4]; 4];
        for c in background_colours.iter_mut().flatten() { *c = r.u32()? }
        let light_directions = [r.vector()?, r.vector()?, r.vector()?];
        let light_colours = [r.vector()?, r.vector()?, r.vector()?];
        let ambient = r.vector()?;
        let title_sjis = r.take(68)?.to_vec();
        let split = title_offset as usize;
        if split > title_sjis.len() { return Err(corrupt("title split outside title field")) }
        let title_lines = [crate::ps1card::decode_sjis(&title_sjis[..split]), crate::ps1card::decode_sjis(&title_sjis[split..])];
        let list_file = filename(r.take(64)?, true)?;
        let copy_file = filename(r.take(64)?, false)?;
        let delete_file = filename(r.take(64)?, false)?;
        Ok(Self { file_type, title_offset, reserved, background_alpha, background_colours, light_directions, light_colours, ambient, title_sjis, title_lines, list_file, copy_file, delete_file })
    }
}

/// Per-vertex attributes shared by all shapes (\[I\] PS2 IconSys `Texture_Data`).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct IconVertex {
    /// Fixed-point normal converted using /4096; fourth component retained.
    pub normal: Vec4,
    /// Signed fixed-point texture coordinates, /4096 (negative UVs occur on real cards).
    pub uv: Vec2,
    /// Original RGBA bytes; the browser ignores vertex alpha (\[I\] `IconLoader.h`).
    pub rgba: [u8; 4],
}

/// A time and shape weight (\[I\] PS2 IconSys `Frame_Key`).
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct IconKey {
    /// Animation frame number.
    pub time: f32,
    /// Shape weight at this time.
    pub weight: f32,
}

/// One animation track, with keys immediately after its 8-byte header.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct IconFrame {
    /// Stored shape ID; sole-shape icons also use 1 for their only shape.
    pub shape_id: u32,
    /// Ordered interpolation keys.
    pub keys: Vec<IconKey>,
}

/// Geometry, morph tracks and a decoded texture (\[I\] PS2 IconSys `ReadFile`).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct IconModel {
    /// Position arrays in shape order; each has one position per triangle-list vertex.
    pub shapes: Vec<Vec<Vec4>>,
    /// Normals, UVs and colours in triangle-list order.
    pub vertices: Vec<IconVertex>,
    /// Attribute bits: Gouraud 1, antialias 2, texture 4, RLE 8 (\[I\] `IconLoader.h`).
    pub texture_type: u32,
    /// Header float at offset 12; some files use 3.0 instead of 1.0.
    pub back_face: f32,
    /// Animation cycle length in frames.
    pub frame_length: u32,
    /// Stored animation speed multiplier.
    pub animation_speed: f32,
    /// Stored starting frame.
    pub play_offset: u32,
    /// Shape-weight tracks.
    pub frames: Vec<IconFrame>,
    /// 128×128 RGBA8 pixels, or empty when texture bit 4 is clear.
    /// RGB5551 bit 15 is retained as alpha; the browser draws icons opaquely.
    pub texture: Vec<u8>,
}

impl IconModel {
    /// Parses bounded triangle/shape/key data and raw or signed-run RLE RGB5551.
    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut r = Reader { data, offset: 0 };
        if r.u32()? != 0x0001_0000 { return Err(corrupt("model signature missing")) }
        let count = r.u32()? as usize;
        let texture_type = r.u32()?;
        let back_face = r.float()?;
        let vertices_count = r.u32()? as usize;
        if !(1..=MAX_SHAPES).contains(&count) || vertices_count == 0 || !vertices_count.is_multiple_of(3) {
            return Err(corrupt("invalid shape or triangle count"));
        }
        let geometry_size = vertices_count.checked_mul(count * 8 + 16).ok_or_else(|| corrupt("geometry size overflow"))?;
        // Validate bytes before any allocation. A generous limit also bounds hostile inputs.
        if geometry_size > 16 * 1024 * 1024 || geometry_size > data.len().saturating_sub(r.offset) {
            return Err(corrupt("geometry exceeds input or size limit"));
        }
        let mut shapes = vec![Vec::with_capacity(vertices_count); count];
        let mut vertices = Vec::with_capacity(vertices_count);
        for _ in 0..vertices_count {
            for shape in &mut shapes { shape.push(r.fixed()?) }
            let normal = r.fixed()?;
            let uv = Vec2::new(r.u16()? as i16 as f32, r.u16()? as i16 as f32) / 4096.0;
            let c = r.take(4)?;
            vertices.push(IconVertex { normal, uv, rgba: [c[0], c[1], c[2], c[3]] });
        }
        if r.u32()? != 1 { return Err(corrupt("unsupported animation sequence count")) }
        let frame_length = r.u32()?;
        let animation_speed = r.float()?;
        let play_offset = r.u32()?;
        let frame_count = r.u32()? as usize;
        if frame_count > MAX_SHAPES || frame_length == 0 || animation_speed < 0.0 { return Err(corrupt("invalid animation header")) }
        let mut frames = Vec::with_capacity(frame_count);
        for _ in 0..frame_count {
            let shape_id = r.u32()?;
            let key_count = r.u32()? as usize;
            if (shape_id as usize >= count && !(count == 1 && shape_id == 1)) || key_count > 4096 || key_count > data.len().saturating_sub(r.offset) / 8 {
                return Err(corrupt("invalid animation shape or key count"));
            }
            let mut keys = Vec::with_capacity(key_count);
            let mut previous = -f32::INFINITY;
            for _ in 0..key_count {
                let time = r.float()?;
                let weight = r.float()?;
                if time < 0.0 || time < previous || weight < 0.0 { return Err(corrupt("invalid or unordered animation key")) }
                previous = time;
                keys.push(IconKey { time, weight });
            }
            frames.push(IconFrame { shape_id, keys });
        }
        let mut texture = Vec::new();
        if texture_type & 4 != 0 {
            let mut pixels = Vec::with_capacity(PIXELS);
            if texture_type & 8 == 0 {
                for _ in 0..PIXELS { pixels.push(r.u16()?) }
            } else {
                let size = r.u32()? as usize;
                let mut packed = Reader { data: r.take(size)?, offset: 0 };
                while packed.offset < packed.data.len() {
                    let run = packed.u16()? as i16 as i32;
                    let n = run.unsigned_abs() as usize;
                    if n == 0 || n > PIXELS - pixels.len() { return Err(corrupt("RLE run exceeds texture or is empty")) }
                    if run > 0 { let p = packed.u16()?; pixels.resize(pixels.len() + n, p) }
                    else { for _ in 0..n { pixels.push(packed.u16()?) } }
                }
                if pixels.len() != PIXELS { return Err(corrupt("RLE texture ends early")) }
            }
            texture.reserve(PIXELS * 4);
            for p in pixels {
                let channel = |shift: u32| { let c = ((p >> shift) & 31u16) as u8; (c << 3) | (c >> 2) };
                texture.extend_from_slice(&[channel(0), channel(5), channel(10), if p & 0x8000 != 0 { 255 } else { 0 }]);
            }
        }
        Ok(Self { shapes, vertices, texture_type, back_face, frame_length, animation_speed, play_offset, frames, texture })
    }

    /// Evaluates piecewise-linear, looping shape weights (\[I\] `IconLoader.h` key semantics).
    /// Playback is inferred; the supplied card contains only static, single-shape icons.
    #[must_use]
    pub fn shape_weights(&self, frame: f32) -> Vec<f32> {
        let mut weights = vec![0.0; self.shapes.len()];
        if weights.is_empty() { return weights }
        let time = (frame.max(0.0) * self.animation_speed + self.play_offset as f32).rem_euclid(self.frame_length.max(1) as f32);
        for track in &self.frames {
            let Some(first) = track.keys.first() else { continue };
            let mut weight = first.weight;
            for pair in track.keys.windows(2) {
                if time < pair[0].time { break }
                let span = pair[1].time - pair[0].time;
                let t = if span > 0.0 { ((time - pair[0].time) / span).clamp(0.0, 1.0) } else { 1.0 };
                weight = pair[0].weight * (1.0 - t) + pair[1].weight * t;
            }
            let i = if weights.len() == 1 { 0 } else { track.shape_id as usize };
            if let Some(w) = weights.get_mut(i) { *w += weight }
        }
        // Normalize so malformed/extreme weights cannot explode the world-space model.
        let total: f64 = weights.iter().map(|&w| w as f64).sum();
        if total > 0.0 && total.is_finite() { for w in &mut weights { *w = (*w as f64 / total) as f32 } }
        else { weights.fill(0.0); weights[0] = 1.0 }
        weights
    }

    /// Bounds across every morph shape, in the parser's model units.
    #[must_use]
    pub fn bounds(&self) -> (Vec3, Vec3) {
        let (mut lo, mut hi) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
        for p in self.shapes.iter().flatten() { lo = lo.min(p.truncate()); hi = hi.max(p.truncate()) }
        if !lo.is_finite() { (Vec3::ZERO, Vec3::ZERO) } else { (lo, hi) }
    }
}

/// One save folder and its own browser icon, read at runtime from the user's card.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct SaveIcon {
    /// History-compatible ID (`BESLES-52541…` → `SLES_525.41`), empty for non-title folders.
    pub title_id: String,
    /// Browser title, joining nonempty lines with a newline.
    pub title: String,
    /// Root save-folder name; multiple saves may share a title ID.
    pub folder: String,
    /// Browser lighting, background and icon filenames.
    pub icon_sys: IconSys,
    /// The model referenced by the descriptor's list filename.
    pub list_icon: IconModel,
}

impl SaveIcon {
    /// Reads one icon per readable save folder, skipping missing/corrupt descriptors/models.
    /// Unlistable cards yield an empty list, as `PlayHistory::titles_with_saves` does.
    #[must_use]
    pub fn from_card(card: &MemoryCard) -> Vec<Self> {
        card.list(&[]).unwrap_or_default().into_iter().filter(|e| e.is_directory).filter_map(|e| Self::read_folder(card, &e.name).ok()).collect()
    }

    /// Reads a particular save, reporting descriptor/model errors instead of skipping it.
    pub fn read_folder(card: &MemoryCard, folder: &str) -> Result<Self> {
        filename(format!("{folder}\0").as_bytes(), true)?;
        let icon_sys = IconSys::parse(&card.read_file(&[folder, "icon.sys"])?)?;
        let list_icon = IconModel::parse(&card.read_file(&[folder, &icon_sys.list_file])?)?;
        // Same byte-prefix rule as history.rs, including its ASCII guard.
        let n = folder.as_bytes();
        let title_id = if n.len() >= 12 && n[..12].is_ascii() && n[0] == b'B' && n[6] == b'-' && n[7..12].iter().all(u8::is_ascii_digit) {
            format!("{}_{}.{}", &folder[2..6], &folder[7..10], &folder[10..12])
        } else { String::new() };
        let title = icon_sys.title_lines.iter().filter(|s| !s.is_empty()).cloned().collect::<Vec<_>>().join("\n");
        Ok(Self { title_id, title, folder: folder.into(), icon_sys, list_icon })
    }
}
