//! Standalone PS1 SCE intro, derived from the caller's decoded shell.
//! Evidence: `notes/research/ps1_shell_scenes.md` §§1a, 2, 4, 5 items 6/8.
//! \[V\] Dump corrections: 1.1 shares 1.0's four-field, NTSC-only path (1.1 J
//! shell +0xEC00/+0xEC6C/+0xEC80); 4.0 J already
//! uses the small logotypes (shell +0x576D0/+0x58524/+0x59698), at (320,84),
//! (320,400), (436,417). 4.1 already has 4.5's PAL assets/height adjustment
//! (4.1 A/E +0x557E8/+0x56CC8 and the 307 literal at +0x1E2CC).
//! Older background helpers divide before multiplying (3*t on PAL, then 180
//! on the last field: 3.0 A +0x178DC); 4.1/4.5 divide after multiplying.

use crate::bytes::Bytes;
use crate::ps1::{address_refs, code_constant, memory_address, NoteEvent, Tim, Vab};
use crate::ps1bios::Ps1Identity;
use crate::sound::SAMPLE_RATE;
use crate::{Error, Format, Result, VideoMode};

const BASE: u32 = 0x80030000;

/// A located literal, or an explicitly documented fallback when code shape differs.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct SceLiteral {
    /// Parameter described by this evidence.
    pub name: &'static str,
    /// Decoded shell byte offset of its instruction; `None` means documented fallback.
    pub offset: Option<usize>,
}

/// Diamond and fade parameters; literals are read from instruction patterns where possible.
/// Fallbacks follow `ps1_shell_scenes.md` §§1a/4, and are listed in `evidence`.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct SceConstants {
    /// Horizontal diamond extent, in screen pixels.
    pub size: u16,
    /// Vertical extent in NTSC.
    pub height: u16,
    /// Vertical extent in PAL (4.1/4.5: 307, read from its SH initialization).
    pub pal_height: u16,
    /// Polygon brightness as a percentage.
    pub brightness: u8,
    /// Opaque background grey, 0..=255.
    pub grey: u8,
    /// Initial triangle scale, in 4.12 fixed point.
    pub scale: u16,
    /// Scale removed over the slide, in 4.12 fixed point.
    pub scale_delta: u16,
    /// Older fade helpers truncate grey/fields before multiplication.
    pub divide_before_multiply: bool,
    /// Code locations and documented fallbacks for the exposed parameters.
    pub evidence: Vec<SceLiteral>,
}

/// Durations in VSync fields (`ps1_shell_scenes.md` §§1a/4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct SceTimeline {
    /// Effective mode; early 1.0/1.1 shells have only an NTSC path.
    pub video: VideoMode,
    /// Black fields before the background fade.
    pub lead_in: usize,
    /// Background fields including both endpoints (61 NTSC / 51 PAL).
    pub fade: usize,
    /// Fields in which the triangles move (60 NTSC / 50 PAL).
    pub slide: usize,
    /// Fields after the slide; the text fade overlaps this hold.
    pub hold: usize,
    /// CLUT interpolation steps (17); first visible text field uses step one.
    pub text_fade: usize,
    /// Note-table clock at the first diamond field (89 NTSC / 74 PAL).
    pub music_clock: u32,
}

impl SceTimeline {
    /// Index of the first diamond field, after the inclusive background fade.
    pub fn diamond_start(self) -> usize { self.lead_in + self.fade }
    /// Index of the first text field (the field after the slide completes).
    pub fn text_start(self) -> usize { self.diamond_start() + self.slide }
    /// Fields until the intro returns; the final picture can persist during disc spin-up.
    pub fn total_fields(self) -> usize { self.diamond_start() + self.slide + self.hold }
    /// Stereo PCM sample frames at 48 kHz, without an added release tail.
    pub fn sound_samples(self) -> usize { (self.total_fields() as f64 * SAMPLE_RATE as f64 / self.video.fps() as f64) as usize }
}

/// One Gouraud vertex in the shell's 640×480 interlaced drawing coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct SceVertex {
    /// Screen position, x right and y down.
    pub xy: [f32; 2],
    /// RGB, 0..=255.
    pub colour: [u8; 3],
}

/// A GPU-order Gouraud quad (strip order) or triangle; drawn in vector order.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum ScePolygon {
    /// POLY_G4: triangles (0,1,2), (1,2,3), underneath both moving halves.
    Quad([SceVertex; 4]),
    /// POLY_G3 over the quad.
    Triangle([SceVertex; 3]),
}

/// A centre-anchored logotype, with the CLUT interpolation step to draw.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct SceSprite {
    /// Index into [`SceIntro::logotypes`].
    pub texture: usize,
    /// Screen-space centre.
    pub centre: [f32; 2],
    /// Screen-space dimensions, in pixels.
    pub size: [f32; 2],
    /// Integer palette interpolation step, 0..=the timeline's text_fade.
    pub clut_level: usize,
}

/// All drawing data for one field, independent of any graphics API.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct SceFrame {
    /// Full-screen opaque background grey.
    pub grey: u8,
    /// Back-to-front order: diamond quad, left triangle, right triangle.
    pub polygons: Vec<ScePolygon>,
    /// Logotype sprites; empty until the slide finishes.
    pub sprites: Vec<SceSprite>,
}

/// The intro assets and parameters located in a supplied standalone shell.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct SceIntro {
    /// Referenced SONY, CE and mark TIMs; PAL variants are included where present.
    pub logotypes: Vec<Tim>,
    /// The intro's 40-event note table, in NTSC ticks, without its 9999 terminator.
    pub events: Vec<NoteEvent>,
    /// Runtime code literals and their provenance.
    pub constants: SceConstants,
    /// Native video mode from the identity letter (missing letter: NTSC).
    pub video: VideoMode,
    timelines: [SceTimeline; 2],
    layouts: [Vec<(usize, [f32; 2])>; 2],
}

fn bad(what: &str) -> Error { Error::Corrupt(Format::Bios, format!("SCE intro: {what}")) }
fn missing(what: &str) -> Error { Error::NotFound(Format::Bios, format!("SCE intro: {what} (absent in PS2 LOGO and POPS)")) }

// Short, bounded function windows, not an emulator/disassembler. The constants are
// initialised by SH and passed to helpers; see scenes §1a at 0x80040574 onwards.
fn function(d: &[u8], from: usize) -> Option<std::ops::Range<usize>> {
    let start = (0..=64).filter_map(|i| from.checked_sub(i * 4)).find(|&o| d.u32(o) >> 16 == 0x27BD)?;
    let end = (start..(start + 4096).min(d.len().saturating_sub(7))).step_by(4).find(|&o| d.u32(o) == 0x03E00008)? + 8;
    Some(start..end)
}

fn target(d: &[u8], o: usize) -> Option<usize> {
    let w = d.u32(o);
    if w >> 26 != 3 { return None }
    let address = 0x80000000 | ((w & 0x03FFFFFF) << 2);
    address.checked_sub(BASE).map(|v| v as usize).filter(|&v| v < d.len())
}

fn literal(d: &[u8], range: std::ops::Range<usize>, reg: Option<u32>, value: u16) -> Option<usize> {
    range.step_by(4).find(|&o| {
        let w = d.u32(o);
        matches!(w >> 26, 9 | 13) && w >> 21 & 31 == 0 && w as u16 == value && reg.is_none_or(|r| w >> 16 & 31 == r)
    })
}

fn evidence(out: &mut Vec<SceLiteral>, name: &'static str, offset: Option<usize>) { out.push(SceLiteral { name, offset }); }

impl SceIntro {
    /// Locates referenced TIMs, the intro table and code literals by content.
    /// Accepts `Ps1Bios::shell_image()` and `Ps1Bios::identity()`; no ROM offsets/hashes.
    /// PS2 LOGO/POPS return `NotFound`; damaged tables/assets return `Corrupt`.
    pub fn locate(d: &[u8], revision: &Ps1Identity) -> Result<Self> {
        let eo = d.windows(8).position(|w| w == [0, 0, 0, 0, 1, 36, 127, 80]).ok_or_else(|| missing("intro note table"))?;
        let mut events: Vec<NoteEvent> = Vec::new();
        for i in 0..=64 {
            let o = eo + i * 8;
            if o + 8 > d.len() { return Err(bad("truncated note table")) }
            let time = d.u32(o);
            if time == 9999 { break }
            let e = NoteEvent { time, prog: d.u8(o + 4), note: d.u8(o + 5), vel: d.u8(o + 6), pan: d.u8(o + 7) };
            if i == 64 || time > 9999 || e.prog > 3 || e.note > 127 || e.vel > 127 || e.pan > 127
                || events.last().is_some_and(|p| p.time > time) { return Err(bad("invalid note table or missing terminator")) }
            events.push(e);
        }
        if events.len() != 40 { return Err(bad("expected 40 intro events")) }

        // Tie the brightness store to an adjacent size initialization and three TIM refs.
        let mut anchor = None;
        for o in (0..d.len().saturating_sub(3)).step_by(4) {
            let w = d.u32(o);
            if w >> 26 != 41 { continue }
            let Some(bright) = code_constant(d, o, w >> 16 & 31, 0).filter(|&v| (1..=100).contains(&v)) else { continue };
            let Some(address) = memory_address(d, o) else { continue };
            let Some(main) = function(d, o) else { continue };
            let size_store = (o + 4..(o + 96).min(main.end)).step_by(4).find(|&p| {
                let v = d.u32(p);
                v >> 26 == 41 && memory_address(d, p).is_some_and(|a| a.checked_add(4) == Some(address) || a.checked_add(8) == Some(address))
                    && code_constant(d, p, v >> 16 & 31, 0) == Some(256)
            });
            let Some(size_store) = size_store else { continue };
            let call = if target(d, size_store.saturating_sub(4)).is_some() { size_store - 4 } else {
                let Some(p) = (size_store + 4..main.end).step_by(4).find(|&p| target(d, p).is_some()) else { continue };
                p
            };
            let Some(setup) = target(d, call).and_then(|p| function(d, p)) else { continue };
            let mut assets = Vec::new();
            for p in (0..d.len().saturating_sub(7)).step_by(4) {
                if d.get(p..p + 8) != Some(&[0x10, 0, 0, 0, 8, 0, 0, 0]) { continue }
                let referenced = u32::try_from(p).ok().and_then(|p| BASE.checked_add(p))
                    .is_some_and(|a| address_refs(&d[setup.clone()], a).next().is_some());
                if !referenced { continue }
                let t = Tim::parse_4bit(&d[p..]).map_err(|_| bad("referenced logotype TIM is damaged"))?;
                if matches!((t.width, t.height), (240, 48 | 60 | 44 | 54) | (200, 36 | 44) | (24, 12 | 10)) { assets.push((p, t)) }
            }
            if assets.len() >= 3 { anchor = Some((o, bright as u8, size_store, main, setup, assets)); break }
        }
        let (store, brightness, size_store, main, setup, assets) = anchor.ok_or_else(|| bad("diamond initialization / referenced logotypes not found"))?;
        let mut proof = Vec::new();
        let bright_literal = (store.saturating_sub(32)..store).step_by(4).rev().find(|&o| literal(d, o..o + 4, None, brightness as u16).is_some());
        evidence(&mut proof, "brightness", bright_literal);
        evidence(&mut proof, "size", literal(d, store..size_store + 4, None, 256));
        let modern = assets.iter().any(|(_, t)| t.width == 200 && t.height == 44);
        let pal_assets = assets.iter().any(|(_, t)| t.width == 240 && t.height == 54);
        let find_asset = |w, h| assets.iter().position(|(_, t)| t.width == w && t.height == h).ok_or_else(|| bad("logotype set incomplete"));
        let n = if modern { [find_asset(200, 36)?, find_asset(200, 44)?, find_asset(24, 10)?] }
            else { [find_asset(240, 48)?, find_asset(240, 60)?, find_asset(24, 12)?] };
        let native_pal = revision.region_letter == Some('E');
        let sony_y = if modern { 84 } else { 80 };
        let ce_y = if modern { 400 } else { 412 };
        let mark_xy = if modern { [436.0, 417.0] } else { [352.0, 365.0] };
        let ntsc_layout = vec![(n[0], [320.0, sony_y as f32]), (n[1], [320.0, ce_y as f32]), (n[2], mark_xy)];
        // Position literals are tied to the same setup function, not a version string.
        evidence(&mut proof, "sony_y", literal(d, setup.clone(), None, sony_y));
        evidence(&mut proof, "ce_y", literal(d, setup.clone(), None, ce_y));
        evidence(&mut proof, "mark_y", literal(d, setup.clone(), None, mark_xy[1] as u16));
        let pal_layout = if pal_assets {
            evidence(&mut proof, "pal_sony_y", literal(d, setup.clone(), None, 69));
            evidence(&mut proof, "pal_ce_y", literal(d, setup.clone(), None, 448));
            vec![(find_asset(240, 44)?, [338.0, 69.0]), (find_asset(240, 54)?, [338.0, 448.0])]
        } else { ntsc_layout.clone() };

        let budget = literal(d, main.clone(), Some(2), 240);
        let pal_budget = literal(d, main.clone(), Some(2), 200);
        let supports_pal = budget.is_some() && pal_budget.is_some();
        let slide_literal = literal(d, main.clone(), Some(6), 60);
        let fade_literal = literal(d, main.clone(), Some(4), 60);
        let hold_literal = main.clone().step_by(4).find(|&o| d.u32(o) >> 26 == 10 && d.u16(o) == 120);
        evidence(&mut proof, "ntsc_fade_slide", budget.or(fade_literal).or(slide_literal));
        evidence(&mut proof, "ntsc_hold", budget.or(hold_literal));
        if supports_pal { evidence(&mut proof, "pal_fade_slide_hold", pal_budget) }
        let mut grey_source = None;
        let mut text_source = None;
        for p in main.clone().step_by(4) {
            let Some(f) = target(d, p).and_then(|p| function(d, p)) else { continue };
            // Background setters take equal RGB arguments, before or after their delay slot.
            let reg = if pal_assets { 5 } else { 4 };
            if let [Some(r), Some(g), Some(b)] = [reg, reg + 1, reg + 2].map(|r| code_constant(d, p + 8, r, 0)) {
                if r == g && g == b && (1..=255).contains(&r) && grey_source.is_none() {
                    grey_source = literal(d, p.saturating_sub(32)..p + 8, Some(reg), r as u16).map(|o| (r as u8, o));
                }
            }
            // CLUT helper: 16 colours and 5-bit component masks. Read its a0 argument,
            // rather than matching the documented 17, so changed inputs remain inspectable.
            let palette = literal(d, f.clone(), None, 16).is_some()
                && f.step_by(4).any(|o| d.u32(o) >> 26 == 12 && d.u16(o) == 31);
            if palette && text_source.is_none() {
                if let Some(v) = code_constant(d, p + 8, 4, 0).filter(|v| (1..=64).contains(v)) {
                    text_source = literal(d, p.saturating_sub(32)..p + 8, Some(4), v as u16).map(|o| (v as usize, o));
                }
            }
        }
        evidence(&mut proof, "grey", grey_source.map(|(_, o)| o));
        let grey = grey_source.map(|(v, _)| v).unwrap_or(180);
        evidence(&mut proof, "text_fade", text_source.map(|(_, o)| o));
        let text_fade = text_source.map(|(v, _)| v).unwrap_or(17);

        // Old scenes call a four-VSync clear helper once (1.0/1.1) or twice.
        // New libgs scenes have a different setup; the documented eight-field lead-in
        // is retained there, explicitly marked as fallback (scenes §§1a/4).
        let mut lead = None;
        for p in (main.start..store).step_by(4) {
            let Some(f) = target(d, p).and_then(|p| function(d, p)) else { continue };
            let Some(at) = literal(d, f.clone(), Some(17), 4) else { continue };
            if f.len() > 128 { continue }
            let count = (main.start..store).step_by(4).filter(|&q| target(d, q) == Some(f.start)).count();
            lead = Some((count * d.u16(at) as usize, at)); break;
        }
        evidence(&mut proof, "lead_in", lead.map(|(_, o)| o));
        let lead_in = lead.map(|(v, _)| v).unwrap_or(8);
        let mut clocks = [None; 2];
        for p in address_refs(d, BASE + eo as u32) {
            let Some(f) = function(d, p) else { continue };
            if let Some(o) = literal(d, f.clone(), None, 89) {
                clocks[0] = Some(o); clocks[1] = literal(d, f, None, 74); break;
            }
        }
        evidence(&mut proof, "ntsc_music_clock", clocks[0]);
        if supports_pal { evidence(&mut proof, "pal_music_clock", clocks[1]) }
        let times = |video: VideoMode| {
            let video = if supports_pal { video } else { VideoMode::Ntsc };
            let pal = video == VideoMode::Pal;
            let fields = if pal { pal_budget.map(|o| d.u16(o) as usize).unwrap_or(200) }
                else { budget.map(|o| d.u16(o) as usize).unwrap_or(240) };
            SceTimeline { video, lead_in, fade: fields / 4 + 1, slide: fields / 4, hold: fields / 2, text_fade,
                music_clock: clocks[usize::from(pal)].map(|o| d.u16(o) as u32).unwrap_or(if pal { 74 } else { 89 }) }
        };
        let scale_function = (store..main.end).step_by(4).filter_map(|p| target(d, p).and_then(|p| function(d, p)))
            .find(|f| literal(d, f.clone(), None, 2258).is_some());
        let scale_at = scale_function.clone().and_then(|f| literal(d, f, None, 4096));
        let delta_at = scale_function.and_then(|f| literal(d, f, None, 2258));
        evidence(&mut proof, "scale", scale_at);
        evidence(&mut proof, "scale_delta", delta_at);
        let pal_height_at = literal(d, store..(store + 128).min(main.end), None, 307);
        evidence(&mut proof, "pal_height", pal_height_at.or_else(|| literal(d, store..size_store + 4, None, 256)));
        let size = code_constant(d, size_store, d.u32(size_store) >> 16 & 31, 0).ok_or_else(|| bad("size source"))? as u16;
        let constants = SceConstants { size, height: size,
            pal_height: pal_height_at.map(|o| d.u16(o)).unwrap_or(256), brightness, grey,
            scale: scale_at.map(|o| d.u16(o)).unwrap_or(4096), scale_delta: delta_at.map(|o| d.u16(o)).unwrap_or(2258),
            divide_before_multiply: !pal_assets, evidence: proof };
        let timelines = [times(VideoMode::Ntsc), times(VideoMode::Pal)];
        let video = if native_pal && supports_pal { VideoMode::Pal } else { VideoMode::Ntsc };
        Ok(Self { logotypes: assets.into_iter().map(|(_, t)| t).collect(), events, constants, video, timelines, layouts: [ntsc_layout, pal_layout] })
    }

    /// Timeline for a requested standard; early shells normalize PAL to NTSC.
    pub fn timeline(&self, video: VideoMode) -> SceTimeline { self.timelines[usize::from(video == VideoMode::Pal)] }

    /// One native-mode field; fields beyond the end retain the final intro picture.
    pub fn frame(&self, field: usize) -> SceFrame { self.frame_for_video(field, self.video) }

    /// One field in a requested mode, for inspecting the shell's alternate video path.
    /// The slide increments before drawing: first diamond uses t=1, first text uses t=1.
    /// Evidence: scenes §1a, 3.0 A shell +0x11374/+0x118A8.
    pub fn frame_for_video(&self, field: usize, video: VideoMode) -> SceFrame {
        let timeline = self.timeline(video);
        let c = &self.constants;
        let mut out = SceFrame { grey: 0, polygons: Vec::new(), sprites: Vec::new() };
        if field < timeline.lead_in { return out }
        let f = field - timeline.lead_in;
        if f < timeline.fade {
            let steps = timeline.fade - 1;
            out.grey = if f == steps { c.grey } else if c.divide_before_multiply {
                (c.grey as usize / steps * f) as u8
            } else { (c.grey as usize * f / steps) as u8 };
            return out;
        }
        out.grey = c.grey;
        let t = (field.saturating_sub(timeline.diamond_start()) + 1).min(timeline.slide) as i32;
        let duration = timeline.slide as i32;
        let half = i32::from(c.size) / 2;
        let height = if timeline.video == VideoMode::Pal { c.pal_height } else { c.height };
        let hy = i32::from(height) / 2;
        let red = [(u16::from(c.brightness) * 255 / 100) as u8, 0, 0];
        let orange = [red[0], (u16::from(c.brightness) * 200 / 100) as u8, 0];
        let vertex = |x, y, colour| SceVertex { xy: [320.0 + x as f32, 240.0 + y as f32], colour };
        out.polygons.push(ScePolygon::Quad([vertex(-half, 0, red), vertex(0, -hy, orange), vertex(0, hy, orange), vertex(half, 0, red)]));
        let scale = i32::from(c.scale) - i32::from(c.scale_delta) * t / duration;
        let dx = (i32::from(c.size) / 4 + i32::from(c.size) / 16) * 100 * t / duration;
        let dy = (i32::from(height) / 4 - i32::from(height) / 16) * 100 * t / duration;
        let x = (-half * 100 + dx) / 100;
        let y = -dy / 100;
        let scaled = |v: i32| (v * scale) >> 12;
        out.polygons.push(ScePolygon::Triangle([vertex(x, y, red), vertex(x + scaled(half), y + scaled(-hy), orange), vertex(x + scaled(half), y + scaled(hy), orange)]));
        out.polygons.push(ScePolygon::Triangle([vertex(-x, -y, red), vertex(-x + scaled(-half), -y + scaled(-hy), orange), vertex(-x + scaled(-half), -y + scaled(hy), orange)]));
        if field >= timeline.text_start() {
            let clut_level = (field.saturating_sub(timeline.text_start()) + 1).min(timeline.text_fade);
            out.sprites = self.layouts[usize::from(timeline.video == VideoMode::Pal)].iter().map(|&(texture, centre)| {
                let t = &self.logotypes[texture];
                SceSprite { texture, centre, size: [t.width as f32, t.height as f32], clut_level }
            }).collect();
        }
        out
    }

    /// Decodes a logotype's CLUT fade using separate integer divisions per component.
    /// Transparent entries remain transparent; scenes §1a, 3.0 A shell +0x1191C.
    /// This permits uploading all palette steps once, without per-field texture changes.
    pub fn faded_logotype(&self, texture: usize, level: usize) -> Option<Tim> {
        let mut tim = self.logotypes.get(texture)?.clone();
        let steps = self.timelines[0].text_fade;
        let level = level.min(steps);
        for pixel in tim.rgba.chunks_exact_mut(4) {
            if pixel[3] == 0 { continue }
            for channel in &mut pixel[..3] {
                let five = (*channel as usize * 31).div_ceil(255);
                *channel = ((five * level / steps + 22 * (steps - level) / steps) * 255 / 31) as u8;
            }
        }
        Some(tim)
    }

    /// Maps table ticks to intro fields, including the initial sweep and clock jump.
    /// PAL table times truncate ×5/6 before the comparison; scenes §§1a/2.
    pub fn scheduled_events(&self, video: VideoMode) -> Vec<(usize, NoteEvent)> {
        let t = self.timeline(video);
        let mut keys = Vec::new();
        for &e in &self.events {
            let time = if t.video == VideoMode::Pal { e.time * 5 / 6 } else { e.time };
            let field = if time == 0 { 0 } else { t.diamond_start() + time.saturating_sub(t.music_clock) as usize };
            if field < t.total_fields() { keys.push((field, e)) }
        }
        // Shell transition keys off these surviving voices (scenes §1a, 0x8003FB54).
        for (prog, note) in [(3, 62), (3, 64), (1, 29), (1, 41)] {
            keys.push((t.total_fields(), NoteEvent { time: 0, prog, note, vel: 0, pan: 0 }));
        }
        keys
    }

    /// Intro PCM from the caller's shared VAB, 48 kHz interleaved stereo.
    /// Ends at `timeline(video).sound_samples()`; releases can continue in the next scene.
    /// Uses the same approximate libsnd gain/pan/ADSR mixer as `Ps1Shell::render_sound`.
    pub fn render_sound(&self, bank: &Vab, video: VideoMode) -> Vec<f32> {
        let t = self.timeline(video);
        let samples = |field: usize| (field as f64 * SAMPLE_RATE as f64 / t.video.fps() as f64) as usize;
        let keys: Vec<_> = self.scheduled_events(video).into_iter().map(|(field, event)| (samples(field), event)).collect();
        bank.render_events(&keys, Some(t.sound_samples()), 0)
    }
}
