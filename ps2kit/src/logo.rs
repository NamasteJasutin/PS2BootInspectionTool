//! `rom0:PS2LOGO` — the "PlayStation 2" logo program run before a disc boots
//! (`notes/ps2logo.md`), and the lettering bitmap every licensed disc carries.

use crate::bytes::Bytes;
use crate::locate::{Image, LogoLayout};
use crate::rom::RomDir;
use crate::sim::VideoMode;
use crate::sound::{decode_adpcm, envelope};
use crate::{Error, Result};
use glam::Vec2;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Node {
    pub kind: i32, // 0 move, 1 line, 2 cubic, 3 end
    pub f: [f32; 6],
}
pub type Shape = Vec<Vec<Node>>;

#[derive(Debug, Clone)]
pub struct ColourKey {
    pub t: i32,
    pub rgba: [f32; 4],
}

#[derive(Debug, Clone)]
pub struct Object {
    pub kind: i32, // 0 = line strips, 1 = ribbon
    pub layer_a: bool,
    pub layer_b: bool,
    pub keys: Vec<(i32, Shape)>,
    pub colours: Vec<ColourKey>,
}

#[derive(Debug, Clone)]
pub struct SoundEffect {
    pub vol_l: i64,
    pub vol_r: i64,
    pub pitch: i64,
    pub adsr1: u16,
    pub adsr2: u16,
    pub reverb: bool,
    pub sample_offset: usize,
}

pub struct LogoAssets {
    pub objects: HashMap<VideoMode, Vec<Object>>,
    pub ribbon_multipliers: [f32; 5],
    pub ribbon_delta: [f32; 4],
    pub ribbon_rate: HashMap<VideoMode, f32>,
    pub pal_y_scale: f32,
    pub effects: Vec<SoundEffect>,
    pub effect_master_volume: i64,
    pub sample_body: Vec<u8>,
}

impl LogoAssets {
    pub fn load(bios: &RomDir) -> Result<Self> {
        Self::from_image(&Image::from_module(bios.module("PS2LOGO")?, 0x100000)?)
    }

    pub fn from_image(img: &Image) -> Result<Self> {
        let lay = LogoLayout::discover(img)?;
        let d = &img.data[..];
        let at = |a: usize| img.at(a).unwrap_or(0);
        let u32 = |a: usize| d.u32(at(a)) as usize;
        let i32 = |a: usize| d.i32(at(a));
        let f32 = |a: usize| d.f32(at(a));
        let shape = |ptr: usize, count: usize| -> Shape {
            (0..count)
                .map(|j| {
                    let mut a = u32(ptr + 4 * j);
                    let mut nodes = Vec::new();
                    loop {
                        let ty = i32(a + 0x20);
                        let mut f = [0f32; 6];
                        for (i, v) in f.iter_mut().enumerate() {
                            *v = f32(a + 4 * i);
                        }
                        // One-time fix-up at init: authoring canvas -> centred coordinates.
                        if ty == 0 || ty == 1 { f[0] -= 128.0; f[1] += 128.0 }
                        if ty == 2 { for i in (0..6).step_by(2) { f[i] -= 128.0; f[i + 1] += 128.0 } }
                        nodes.push(Node { kind: ty, f });
                        a += 0x30;
                        if ty == 3 || nodes.len() > 4096 || !img.contains(a) {
                            break;
                        }
                    }
                    nodes
                })
                .collect()
        };
        let mut objects = HashMap::new();
        for (mode, shape_table, colour_table) in [(VideoMode::Ntsc, lay.shape_tables.0, lay.colour_tables.0), (VideoMode::Pal, lay.shape_tables.1, lay.colour_tables.1)] {
            let objs = (0..4)
                .map(|o| {
                    let (kp, kn) = (u32(shape_table + o * 8), u32(shape_table + o * 8 + 4).min(16));
                    let keys = (0..kn).map(|k| (i32(kp + k * 12), shape(u32(kp + k * 12 + 4), i32(kp + k * 12 + 8) as usize))).collect();
                    let (cp, cn) = (u32(colour_table + o * 8), u32(colour_table + o * 8 + 4).min(32));
                    let colours = (0..cn)
                        .map(|k| ColourKey { t: i32(cp + k * 20), rgba: [i32(cp + k * 20 + 4) as f32, i32(cp + k * 20 + 8) as f32, i32(cp + k * 20 + 12) as f32, i32(cp + k * 20 + 16) as f32] })
                        .collect();
                    Object { kind: i32(lay.type_table + 4 * (3 - o)), layer_a: u32(lay.type_table + 0x20 + 4 * (3 - o)) != 0, layer_b: u32(lay.type_table + 0x40 + 4 * (3 - o)) != 0, keys, colours }
                })
                .collect();
            objects.insert(mode, objs);
        }
        let hd = lay.bank_header;
        let body_size = u32(hd + 4);
        let se = hd + u32(hd + 0x2C);
        let last = u32(se + 4).min(15);
        let effects = (0..=last)
            .map(|i| {
                let e = at(se + 0x20 + i * 0x40);
                SoundEffect { vol_l: d.u16(e) as i64, vol_r: d.u16(e + 2) as i64, pitch: d.u16(e + 4) as i64, adsr1: d.u16(e + 8), adsr2: d.u16(e + 10), reverb: d.u16(e + 14) & 0x80 != 0, sample_offset: d.u32(e + 16) as usize }
            })
            .collect();
        let body_start = at(hd + 0x200);
        let m = lay.ribbon_multipliers;
        let r = lay.ribbon_delta;
        Ok(Self {
            objects,
            ribbon_multipliers: [f32(m), f32(m + 4), f32(m + 8), f32(m + 12), f32(m + 16)],
            ribbon_delta: [f32(r + 4), f32(r), f32(r), 0.0],
            ribbon_rate: HashMap::from([(VideoMode::Ntsc, f32(r + 0x10)), (VideoMode::Pal, f32(r + 0x14))]),
            pal_y_scale: f32(r + 8) / f32(r + 12),
            effects,
            effect_master_volume: u32(se) as i64,
            sample_body: d[body_start..(body_start + body_size).min(d.len())].to_vec(),
        })
    }

    /// The five-voice chime (`cmd 0x5200` for effects 0..=4), 48 kHz interleaved stereo.
    pub fn chime(&self, master_volume: i64) -> Vec<f32> {
        let mut mix: Vec<f32> = Vec::new();
        for e in self.effects.iter().take(5) {
            let (pcm, _) = decode_adpcm(&self.sample_body, e.sample_offset);
            if pcm.len() < 2 {
                continue;
            }
            let step = (e.pitch * 44100 / 48000) as f64 / 4096.0;
            let n = (pcm.len() as f64 / step) as usize + 1;
            let env = envelope(e.adsr1, e.adsr2, n, n);
            let master = 0x3FFF as f32 / 16384.0;
            let gl = ((e.vol_l * master_volume) >> 7) as f32 / 16384.0 * master;
            let gr = ((e.vol_r * master_volume) >> 7) as f32 / 16384.0 * master;
            if mix.len() < n * 2 {
                mix.resize(n * 2, 0.0);
            }
            let mut pos = 0.0f64;
            for (k, &ev) in env.iter().enumerate() {
                let i0 = pos as usize;
                if i0 + 1 >= pcm.len() {
                    break;
                }
                let f = (pos - i0 as f64) as f32;
                let s = (pcm[i0] * (1.0 - f) + pcm[i0 + 1] * f) * ev / 32768.0 / 32768.0;
                mix[k * 2] += s * gl;
                mix[k * 2 + 1] += s * gr;
                pos += step;
            }
        }
        mix
    }
}

/// Evaluates the keyframed vector objects of the logo animation.
pub struct LogoAnimation<'a> {
    pub assets: &'a LogoAssets,
    pub video: VideoMode,
}

impl<'a> LogoAnimation<'a> {
    pub fn new(assets: &'a LogoAssets, video: VideoMode) -> Self { Self { assets, video } }
    pub fn objects(&self) -> &[Object] { self.assets.objects.get(&self.video).map(Vec::as_slice).unwrap_or(&[]) }
    pub fn first_field(&self) -> i32 { if self.video == VideoMode::Pal { 14 } else { 17 } }
    pub fn last_field(&self) -> i32 { if self.video == VideoMode::Pal { 35 } else { 42 } }
    pub const HOLD_FIELDS: usize = 120;

    pub fn logo_blur_iterations(&self, t: i32) -> i32 {
        if self.video == VideoMode::Pal { ((28 - t) * 240 / 28).clamp(0, 240) } else { ((33 - t) * 240 / 33).clamp(0, 240) }
    }
    pub fn screen_blur_active(&self, t: i32) -> bool { if self.video == VideoMode::Pal { t < 29 } else { t < 34 } }
    pub fn feedback_alpha(&self, t: i32) -> i32 {
        if self.video == VideoMode::Pal {
            if t <= 21 { 112 } else { ((34 - t) * 112 / 12).clamp(0, 112) }
        } else if t <= 25 { 112 } else { ((41 - t) * 112 / 15).clamp(0, 112) }
    }
    /// (x, y, w, h) of the logo quad in 640x512 buffer coordinates.
    pub fn logo_rect(&self) -> (f32, f32, f32, f32) { if self.video == VideoMode::Pal { (149.5, 216.5, 384.0, 77.0) } else { (149.5, 223.5, 384.0, 64.0) } }

    fn key_pair<T>(keys: &[T], time: impl Fn(&T) -> i32, t: i32) -> (usize, usize, f32) {
        let mut i = 0;
        for (k, key) in keys.iter().enumerate() {
            if time(key) <= t { i = k }
        }
        if i + 1 >= keys.len() {
            return (i, i, 1.0);
        }
        let (t0, t1) = (time(&keys[i]), time(&keys[i + 1]));
        (i, i + 1, (t1 - t) as f32 / (t1 - t0) as f32)
    }

    pub fn shape(&self, obj: &Object, t: i32) -> Shape {
        let (i, j, w) = Self::key_pair(&obj.keys, |k| k.0, t.max(0));
        if i == j {
            return obj.keys[i].1.clone();
        }
        obj.keys[i].1.iter().zip(&obj.keys[j].1).map(|(pa, pb)| {
            pa.iter().zip(pb).map(|(na, nb)| {
                let mut f = [0f32; 6];
                for k in 0..6 { f[k] = na.f[k] * w + nb.f[k] * (1.0 - w) }
                Node { kind: na.kind, f }
            }).collect()
        }).collect()
    }

    /// Drawn colour (0..=255 per channel) of an object at field `t`: rgb · min(a, 255) / 128.
    pub fn colour(&self, obj: &Object, t: i32) -> [f32; 3] {
        let (i, j, w) = Self::key_pair(&obj.colours, |k| k.t, t);
        let (ci, cj) = (&obj.colours[i].rgba, &obj.colours[j].rgba);
        let a = (ci[3] * w + cj[3] * (1.0 - w)).min(255.0);
        let mut out = [0f32; 3];
        for k in 0..3 { out[k] = ((ci[k] * w + cj[k] * (1.0 - w)) * a / 128.0).min(255.0) }
        out
    }

    /// Subdivision count of one cubic: 4 steps, or a straight line for very bent curves.
    fn subdivisions(p0: Vec2, f: &[f32; 6]) -> usize {
        let (c1, c2, p3) = (Vec2::new(f[0], f[1]), Vec2::new(f[2], f[3]), Vec2::new(f[4], f[5]));
        let chord = (p3 - p0).length();
        if chord == 0.0 { return 0 }
        let (m01, m12, m23) = ((p0 + c1) / 2.0, (c1 + c2) / 2.0, (c2 + p3) / 2.0);
        let (m012, m123) = ((m01 + m12) / 2.0, (m12 + m23) / 2.0);
        let mid = (m012 + m123) / 2.0;
        let pts = [p0, m01, m012, mid, m123, m23, p3];
        let polygon = (0..6).map(|i| (pts[i + 1] - pts[i]).length()).sum::<f32>().trunc();
        if chord * 3.5 < polygon || chord > 2000.0 { 1 } else { 4 }
    }

    /// Flattens a shape into point lists. `counts` reuses subdivision counts (ribbons);
    /// without it, samples closer than 1 px are dropped (line strips).
    pub fn flatten(&self, shape: &Shape, counts: Option<&[usize]>) -> Vec<Vec<Vec2>> {
        let mut ci = 0;
        shape.iter().map(|nodes| {
            let mut pts = Vec::new();
            let mut prev = Vec2::ZERO;
            for n in nodes {
                match n.kind {
                    0 => { prev = Vec2::new(n.f[0], n.f[1]); pts.push(prev) }
                    1 => { pts.push(prev); prev = Vec2::new(n.f[0], n.f[1]) }
                    2 => {
                        pts.push(prev);
                        let steps = match counts { Some(c) => { let s = c.get(ci).copied().unwrap_or(4); ci += 1; s } None => Self::subdivisions(prev, &n.f) };
                        let (c1, c2, p3) = (Vec2::new(n.f[0], n.f[1]), Vec2::new(n.f[2], n.f[3]), Vec2::new(n.f[4], n.f[5]));
                        let (mut last, mut acc) = (prev, 0.0f32);
                        for i in 1..steps {
                            let s = i as f32 / steps as f32;
                            let u = 1.0 - s;
                            let p = prev * (u * u * u) + c1 * (3.0 * u * u * s) + c2 * (3.0 * u * s * s) + p3 * (s * s * s);
                            if counts.is_some() { pts.push(p) } else {
                                acc += (p - last).length();
                                if acc > 1.0 { pts.push(p); acc = 0.0 }
                            }
                            last = p;
                        }
                        prev = p3;
                    }
                    3 => pts.push(prev),
                    _ => {}
                }
            }
            pts
        }).collect()
    }

    pub fn segment_counts(&self, obj: &Object, t: i32) -> Vec<usize> {
        let mut counts = Vec::new();
        for pl in self.shape(obj, t) {
            let mut prev = Vec2::ZERO;
            for n in pl {
                if n.kind == 0 || n.kind == 1 { prev = Vec2::new(n.f[0], n.f[1]) }
                if n.kind == 2 { counts.push(Self::subdivisions(prev, &n.f)); prev = Vec2::new(n.f[4], n.f[5]) }
            }
        }
        counts
    }

    /// Centred coordinates -> 640x512 buffer coordinates.
    pub fn to_screen(&self, p: Vec2) -> Vec2 {
        Vec2::new(p.x + 320.0, 256.0 - if self.video == VideoMode::Pal { p.y * self.assets.pal_y_scale } else { p.y })
    }

    /// Field offsets of the five ribbon copies for object `index` (oldest first).
    pub fn ribbon_offsets(&self, index: usize) -> [i32; 5] {
        let d = self.assets.ribbon_delta[index];
        let r = self.assets.ribbon_rate.get(&self.video).copied().unwrap_or(1.0);
        let mut out = [0; 5];
        for (k, o) in out.iter_mut().enumerate() { *o = (d * (4 - k) as f32 * r) as i32 }
        out
    }
}

/// A grey bitmap of the lettering as the logo program draws it.
#[derive(Debug, Clone)]
pub struct LogoBitmap {
    pub width: usize,
    pub height: usize,
    pub grey: Vec<u8>,
}

/// The lettering bitmap every licensed disc carries in its first 12 sectors.
pub struct DiscLogo {
    pub pixels: Vec<u8>,
    /// "E" or "J" when the checksum matched.
    pub region: Option<&'static str>,
}

impl DiscLogo {
    /// Reads sectors 0-11 of a plain ISO image and descrambles them as the drive does.
    pub fn read(path: &std::path::Path) -> Result<Self> {
        let raw = crate::sectors::SectorReader::open(path)?.read(0, 12)?;
        if raw.len() != 12 * 2048 {
            return Err(Error::Corrupt("disc image is too short for the logo sectors".into()));
        }
        let key = raw[0];
        let out: Vec<u8> = raw.iter().map(|&b| { let x = b ^ key; (x << 3) | (x >> 5) }).collect();
        let sum = out.chunks_exact(4).fold(0u32, |s, c| s.wrapping_add(u32::from_le_bytes([c[0], c[1], c[2], c[3]])));
        let region = if sum == 0x7813_4705 { Some("E") } else if sum == 0x62DB_1E66 { Some("J") } else { None };
        Ok(Self { pixels: out, region })
    }

    /// The image laid out for a video mode: 384x64 (NTSC) or the 344x71 PAL picture in a
    /// 384x77 frame starting at row 3.
    pub fn bitmap(&self, video: VideoMode) -> LogoBitmap {
        if video == VideoMode::Pal {
            let mut grey = vec![0u8; 384 * 77];
            for r in 0..71 { for c in 0..344 { grey[(r + 3) * 384 + c] = self.pixels[r * 344 + c] } }
            LogoBitmap { width: 384, height: 77, grey }
        } else {
            LogoBitmap { width: 384, height: 64, grey: self.pixels[..384 * 64].to_vec() }
        }
    }

    /// Fallback when no disc is available: the lettering outline from the BIOS, filled.
    pub fn synthesised(anim: &LogoAnimation) -> LogoBitmap {
        let (rx, ry, _, rh) = anim.logo_rect();
        let (w, h) = (384usize, rh as usize);
        let mut grey = vec![0u8; w * h];
        let Some(outline) = anim.objects().get(3).and_then(|o| o.keys.last()).map(|k| &k.1) else { return LogoBitmap { width: w, height: h, grey } };
        let polys: Vec<Vec<Vec2>> = anim.flatten(outline, None).iter().map(|pl| pl.iter().map(|&p| anim.to_screen(p) - Vec2::new(rx, ry)).collect()).collect();
        let sub = 4;
        for py in 0..h {
            let mut coverage = vec![0f32; w];
            for s in 0..sub {
                let y = py as f32 + (s as f32 + 0.5) / sub as f32;
                let mut xs: Vec<f32> = Vec::new();
                for poly in polys.iter().filter(|p| p.len() > 2) {
                    for i in 0..poly.len() {
                        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
                        if (a.y <= y) != (b.y <= y) { xs.push(a.x + (y - a.y) / (b.y - a.y) * (b.x - a.x)) }
                    }
                }
                xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
                for k in (0..xs.len().saturating_sub(1)).step_by(2) {
                    let (x0, x1) = (xs[k].max(0.0), xs[k + 1].min(w as f32));
                    if x1 <= x0 { continue }
                    for px in x0 as usize..=(x1 as usize).min(w - 1) {
                        coverage[px] += ((px as f32 + 1.0).min(x1) - (px as f32).max(x0)).max(0.0) / sub as f32;
                    }
                }
            }
            for px in 0..w { grey[py * w + px] = (coverage[px] * 255.0).min(255.0) as u8 }
        }
        LogoBitmap { width: w, height: h, grey }
    }
}
