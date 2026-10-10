//! Asset benches for auditioning every sound program and inspecting every font glyph
//! in PlayStation BIOS dumps without shipping any proprietary data (`notes/sound.md` §1.3,
//! `notes/research/version_evolution.md` §3.4, `notes/research/ps1_shell_scenes.md` §2,
//! `notes/research/ps1_version_matrix.md` §2.1, `notes/menu_survey.md` §4).

use crate::bytes::Bytes;
use crate::ps1::Vab;
use crate::ps1bios::Ps1Bios;
use crate::rom::{unpack, RomDir};
use crate::sound::{
    embedded_boot_sounds, envelope, mix_voice, DriverTables, ProgramKind, SoundBank, SAMPLE_RATE,
};
use crate::{Error, Format, Result};
use std::ops::{Bound, RangeBounds};
use std::path::Path;

// =======================================================================================
// Sound Bench
// =======================================================================================

/// One tone (sample reference and SPU synthesis parameters) in a [`BenchProgram`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct BenchTone {
    /// Lowest MIDI note the tone answers to.
    pub low: i32,
    /// Highest MIDI note the tone answers to.
    pub high: i32,
    /// Root/centre note where the sample plays at its recorded pitch.
    pub root: i32,
    /// Fine tuning in 1/16 semitone steps (PS2) or 1/128 semitone (PS1).
    pub fine: i32,
    /// SPU ADSR register 1.
    pub adsr1: u16,
    /// SPU ADSR register 2.
    pub adsr2: u16,
    /// Index into [`SoundBench::samples`].
    pub sample_index: usize,
    /// Tone volume (0..=127).
    pub volume: i32,
    /// Tone pan (0..=127, 64 = centre).
    pub pan: i32,
    /// Pitch bend range in semitones (PS2 only).
    pub bend_range: i32,
}

/// An instrument program containing one or more tones.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct BenchProgram {
    /// Program number (0..=127).
    pub id: i32,
    /// Master program volume (0..=127).
    pub volume: i32,
    /// Tone selection mode (Split, Layer, Drum).
    pub kind: ProgramKind,
    /// The tones making up this program.
    pub tones: Vec<BenchTone>,
}

/// A decoded SPU-ADPCM sample in a [`SoundBench`].
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct BenchSample {
    /// Index of this sample in the bench.
    pub index: usize,
    /// Number of decoded PCM samples.
    pub length: usize,
    /// Loop start index, if the sample loops.
    pub loop_start: Option<usize>,
}

#[derive(Clone)]
enum SoundBenchBackend {
    Ps2 {
        bank: SoundBank,
        tables: DriverTables,
    },
    Ps1 {
        vab: Vab,
    },
}

/// Asset workbench for sound banks: inspect programs, tones, and samples, and render
/// notes to 48 kHz stereo PCM (`notes/sound.md`, `notes/research/ps1_shell_scenes.md` §2).
#[derive(Clone)]
#[non_exhaustive]
pub struct SoundBench {
    /// All programs in the bank, ordered by program ID.
    pub programs: Vec<BenchProgram>,
    /// All decoded samples referenced by the programs.
    pub samples: Vec<BenchSample>,
    backend: SoundBenchBackend,
}

impl std::fmt::Debug for SoundBench {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SoundBench")
            .field("programs", &self.programs.len())
            .field("samples", &self.samples.len())
            .finish_non_exhaustive()
    }
}

impl SoundBench {
    /// Inspects the PS2 boot sound bank from a `RomDir` image (`rom0:SNDIMAGE` or 1.00 J `OSDSYS`).
    /// Locations and driver tables are cited in `notes/sound.md` §1.3 and §3.1.
    pub fn ps2(bios: &RomDir) -> Result<Self> {
        let bank = match bios.module("SNDIMAGE") {
            Ok(module) => {
                let archive = RomDir::new(module.to_vec())?;
                let asset = |n: &str| -> Result<Vec<u8>> { unpack(archive.module(n)?, 0) };
                SoundBank::new(&asset("SNDBOOTH")?, asset("SNDBOOTB")?)?
            }
            Err(Error::NotFound(..)) if bios.module("OSOPEN").is_ok() => {
                embedded_boot_sounds(bios)?.0
            }
            Err(e) => return Err(e),
        };
        let tables = bios
            .module("OSDSND")
            .ok()
            .and_then(DriverTables::from_driver)
            .unwrap_or_else(DriverTables::computed);
        Ok(Self::from_bank_and_tables(bank, tables))
    }

    /// Inspects a PlayStation 1 VAB bank (`notes/research/ps1_shell_scenes.md` §2).
    pub fn ps1(vab: &Vab) -> Self {
        let mut programs = Vec::with_capacity(vab.programs.len());
        for (p_id, p) in vab.programs.iter().enumerate() {
            let mut tones = Vec::with_capacity(p.tones.len());
            for t in &p.tones {
                tones.push(BenchTone {
                    low: 0,
                    high: 127,
                    root: t.centre as i32,
                    fine: t.shift as i32,
                    adsr1: t.adsr1,
                    adsr2: t.adsr2,
                    sample_index: t.vag as usize,
                    volume: t.vol as i32,
                    pan: t.pan as i32,
                    bend_range: 2,
                });
            }
            programs.push(BenchProgram {
                id: p_id as i32,
                volume: p.vol as i32,
                kind: ProgramKind::Layer,
                tones,
            });
        }
        let samples = vab
            .samples
            .iter()
            .enumerate()
            .map(|(i, (pcm, lp))| BenchSample {
                index: i,
                length: pcm.len(),
                loop_start: *lp,
            })
            .collect();
        Self {
            programs,
            samples,
            backend: SoundBenchBackend::Ps1 { vab: vab.clone() },
        }
    }

    /// Constructs a PS2 sound bench directly from an `SShd` [`SoundBank`] and optional driver tables.
    pub fn from_bank(bank: SoundBank, tables: Option<DriverTables>) -> Self {
        Self::from_bank_and_tables(bank, tables.unwrap_or_else(DriverTables::computed))
    }

    fn from_bank_and_tables(bank: SoundBank, tables: DriverTables) -> Self {
        // Collect all distinct sample offsets referenced by tones across all programs.
        let mut offsets: Vec<usize> = Vec::new();
        for p in bank.programs.values() {
            for t in &p.tones {
                if !offsets.contains(&t.sample_offset) {
                    offsets.push(t.sample_offset);
                }
            }
        }
        offsets.sort_unstable();

        let samples: Vec<BenchSample> = offsets
            .iter()
            .enumerate()
            .map(|(i, &off)| {
                let (pcm, lp) = bank.decode_sample(off);
                BenchSample {
                    index: i,
                    length: pcm.len(),
                    loop_start: lp,
                }
            })
            .collect();

        let mut p_keys: Vec<i32> = bank.programs.keys().copied().collect();
        p_keys.sort_unstable();

        let mut programs = Vec::with_capacity(p_keys.len());
        for k in p_keys {
            if let Some(p) = bank.programs.get(&k) {
                let tones = p
                    .tones
                    .iter()
                    .map(|t| {
                        let sample_index = offsets.iter().position(|&o| o == t.sample_offset).unwrap_or(0);
                        BenchTone {
                            low: t.low,
                            high: t.high,
                            root: t.root,
                            fine: t.fine,
                            adsr1: t.adsr1,
                            adsr2: t.adsr2,
                            sample_index,
                            volume: t.volume,
                            pan: t.pan,
                            bend_range: t.bend_range,
                        }
                    })
                    .collect();
                programs.push(BenchProgram {
                    id: k,
                    volume: p.volume,
                    kind: p.kind,
                    tones,
                });
            }
        }

        Self {
            programs,
            samples,
            backend: SoundBenchBackend::Ps2 { bank, tables },
        }
    }

    /// Opens a BIOS dump from disk and inspects its sound bank (PS1 or PS2).
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let data = std::fs::read(path)?;
        Self::from_bytes(&data)
    }

    /// Inspects the sound bank from in-memory BIOS dump bytes.
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if Ps1Bios::detect(data) {
            let bios = Ps1Bios::new(data.to_vec())?;
            let shell = bios.shell_image()?;
            let vo = shell
                .windows(4)
                .position(|w| w == b"pBAV")
                .ok_or_else(|| Error::NotFound(Format::SoundBank, "no VAB bank in shell".into()))?;
            let vab = Vab::parse(&shell[vo..])?;
            return Ok(Self::ps1(&vab));
        }
        if let Ok(rom) = RomDir::new(data.to_vec()) {
            return Self::ps2(&rom);
        }
        Err(Error::NotA(Format::Bios, "unrecognised BIOS dump".into()))
    }

    /// Returns the program with the specified ID, if present.
    pub fn program(&self, id: i32) -> Option<&BenchProgram> {
        self.programs.iter().find(|p| p.id == id)
    }

    /// Returns the sample with the specified index, if present.
    pub fn sample(&self, index: usize) -> Option<&BenchSample> {
        self.samples.get(index)
    }

    /// Auditions an instrument: synthesises a note at 48 kHz stereo for `seconds`.
    /// Reuses the existing ADPCM/ADSR mixing path via [`mix_voice`] (`notes/sound.md` §2).
    pub fn render(&self, program_id: i32, note: i32, velocity: i32, seconds: f64) -> Vec<f32> {
        if seconds <= 0.0 {
            return Vec::new();
        }
        let total = (seconds * SAMPLE_RATE as f64).round() as usize;
        let mut mix = vec![0.0f32; total * 2];
        let on_samples = if seconds > 0.5 {
            ((seconds - 0.25) * SAMPLE_RATE as f64) as usize
        } else {
            (total * 3) / 4
        };

        match &self.backend {
            SoundBenchBackend::Ps2 { bank, tables } => {
                let Some(prog) = bank.programs.get(&program_id) else {
                    return mix;
                };
                let master = 0x3FFF as f32 / 16384.0;
                let synth = crate::sound::Synth { bank, tables };

                for tone in prog.tones.iter().filter(|t| t.low <= note && note <= t.high) {
                    let (pcm, lp) = bank.decode_sample(tone.sample_offset);
                    if pcm.len() < 2 {
                        continue;
                    }
                    let p = (64 + tone.pan - 0x40).clamp(0, 0x7F) >> 2;
                    let (gl, gr) = tables.pan.get(p as usize).copied().unwrap_or((0x80, 0x80));
                    let vel = bank
                        .velocity
                        .get(velocity.clamp(0, 127) as usize)
                        .copied()
                        .unwrap_or(velocity) as i64;
                    let (seqv, chv, prv, tv) = (0x7Fi64, 0x7Fi64, prog.volume as i64, tone.volume as i64);
                    let vol_l = crate::sound::Synth::volume_register(seqv, chv, prv, vel, tv, gl);
                    let vol_r = crate::sound::Synth::volume_register(seqv, chv, prv, vel, tv, gr);
                    let gain_l = vol_l as f32 / 16384.0 * master;
                    let gain_r = vol_r as f32 / 16384.0 * master;

                    let pitch = synth.pitch_register(tone.root, note, tone.fine, 0x40, tone.bend_range);
                    let step = pitch as f64 / 4096.0;
                    let env = envelope(tone.adsr1, tone.adsr2, on_samples, total);

                    mix_voice(&mut mix, 0, &pcm, lp, &env, step, gain_l, gain_r);

                    if prog.kind == ProgramKind::Split {
                        break;
                    }
                }
            }
            SoundBenchBackend::Ps1 { vab } => {
                let Some(prog) = vab.programs.get(program_id as usize) else {
                    return mix;
                };
                for tone in &prog.tones {
                    let Some((pcm, lp)) = vab.samples.get(tone.vag as usize) else {
                        continue;
                    };
                    if pcm.len() < 2 {
                        continue;
                    }
                    let env = envelope(tone.adsr1, tone.adsr2, on_samples, total);
                    let gain = (velocity.clamp(0, 127) as f32 / 127.0)
                        * (tone.vol as f32 / 127.0)
                        * (prog.vol as f32 / 127.0)
                        * (vab.master as f32 / 127.0)
                        * (0x3FFF as f32 / 16384.0);
                    let pan = ((tone.pan as f32) / 127.0).clamp(0.0, 1.0);
                    let (gain_l, gain_r) = (gain * (1.0 - pan).sqrt(), gain * pan.sqrt());

                    let semis = note as f32 - tone.centre as f32 + tone.shift as f32 / 128.0;
                    let step = 2f64.powf(semis as f64 / 12.0) * 44100.0 / SAMPLE_RATE as f64;

                    mix_voice(&mut mix, 0, pcm, *lp, &env, step, gain_l, gain_r);
                }
            }
        }

        mix
    }
}

// =======================================================================================
// Font Bench
// =======================================================================================

/// A single rendered RGBA glyph bitmap.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Glyph {
    /// Width of the glyph in pixels.
    pub width: usize,
    /// Height of the glyph in pixels.
    pub height: usize,
    /// `width * height * 4` bytes in row-major RGBA order.
    pub rgba: Vec<u8>,
}

trait FontProvider: Send + Sync {
    fn glyph_count(&self) -> usize;
    fn glyph(&self, index: usize) -> Option<Glyph>;
}

/// A font available in the BIOS, exposing per-glyph bitmaps and atlas generation.
pub struct BenchFont {
    name: String,
    width: usize,
    height: usize,
    count: usize,
    summary: String,
    provider: Box<dyn FontProvider>,
}

impl std::fmt::Debug for BenchFont {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BenchFont")
            .field("name", &self.name)
            .field("dimensions", &(self.width, self.height))
            .field("glyph_count", &self.count)
            .finish_non_exhaustive()
    }
}

impl BenchFont {
    /// The font or sheet name (e.g. `"KROM"`, `"KROMG"`, `"FONTM"`, `"FNTASCII"`).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Number of accessible glyphs in this font.
    pub fn glyph_count(&self) -> usize {
        self.provider.glyph_count()
    }

    /// Pixel dimensions `(width, height)` of each glyph cell.
    pub fn dimensions(&self) -> (usize, usize) {
        (self.width, self.height)
    }

    /// Human-readable summary of character coverage and encoding.
    pub fn summary(&self) -> &str {
        &self.summary
    }

    /// Decodes glyph `index` into an RGBA bitmap, or `None` if out of bounds.
    pub fn glyph(&self, index: usize) -> Option<Glyph> {
        self.provider.glyph(index)
    }

    /// Packs a range of glyphs into a 2D atlas image, returning `(width, height, Vec<u8> RGBA)`.
    pub fn atlas(&self, range: impl RangeBounds<usize>) -> (usize, usize, Vec<u8>) {
        let (start, end) = resolve_range(range, self.count);
        let n = end.saturating_sub(start);
        if n == 0 {
            return (0, 0, Vec::new());
        }
        let (gw, gh) = (self.width, self.height);
        let cols = if gw >= 32 && n <= 100 { 8 } else { 16 }.min(n).max(1);
        let rows = n.div_ceil(cols);
        let atlas_w = cols * gw;
        let atlas_h = rows * gh;
        let mut atlas_rgba = vec![0u8; atlas_w * atlas_h * 4];

        for i in 0..n {
            let g_idx = start + i;
            if let Some(g) = self.provider.glyph(g_idx) {
                let col = i % cols;
                let row = i / cols;
                let px0 = col * gw;
                let py0 = row * gh;
                for gy in 0..gh.min(g.height) {
                    for gx in 0..gw.min(g.width) {
                        let src_o = (gy * g.width + gx) * 4;
                        let dst_o = ((py0 + gy) * atlas_w + (px0 + gx)) * 4;
                        if src_o + 4 <= g.rgba.len() && dst_o + 4 <= atlas_rgba.len() {
                            atlas_rgba[dst_o..dst_o + 4].copy_from_slice(&g.rgba[src_o..src_o + 4]);
                        }
                    }
                }
            }
        }
        (atlas_w, atlas_h, atlas_rgba)
    }
}

fn resolve_range(range: impl RangeBounds<usize>, len: usize) -> (usize, usize) {
    let start = match range.start_bound() {
        Bound::Included(&s) => s,
        Bound::Excluded(&s) => s.saturating_add(1),
        Bound::Unbounded => 0,
    };
    let end = match range.end_bound() {
        Bound::Included(&e) => e.saturating_add(1).min(len),
        Bound::Excluded(&e) => e.min(len),
        Bound::Unbounded => len,
    };
    (start.min(len), end.min(len).max(start.min(len)))
}

// ---------------------------------------------------------------------------------------
// PS1 KROM Font
// ---------------------------------------------------------------------------------------

/// The 51 canonical interval table entries verified in every retail PS1 kernel and PS2 rom0
/// (`notes/research/ps1_version_matrix.md` §2.1, ROM `0x16E00` / `0xA2E0`).
const DEFAULT_KROM_INTERVALS: [(u16, u16); 51] = [
    (0x8140, 0x0), (0x8180, 0x3f), (0x81b8, 0x6c), (0x81c8, 0x74), (0x81da, 0x7b),
    (0x81f0, 0x8a), (0x81fc, 0x92), (0x824f, 0x93), (0x8260, 0x9d), (0x8281, 0xb7),
    (0x829f, 0xd1), (0x8340, 0x124), (0x8380, 0x163), (0x839f, 0x17a), (0x83bf, 0x192),
    (0x8440, 0x1aa), (0x8470, 0x1cb), (0x8480, 0x1da), (0x849f, 0x1ec),
    (0x889f, 0x0), (0x8940, 0x5e), (0x899f, 0xbc), (0x8a40, 0x11a), (0x8a9f, 0x178),
    (0x8b40, 0x1d6), (0x8b9f, 0x234), (0x8c40, 0x292), (0x8c9f, 0x2f0), (0x8d40, 0x34e),
    (0x8d9f, 0x3ac), (0x8e40, 0x40a), (0x8e9f, 0x468), (0x8f40, 0x4c6), (0x8f9f, 0x524),
    (0x9040, 0x582), (0x909f, 0x5e0), (0x9140, 0x63e), (0x919f, 0x69c), (0x9240, 0x6fa),
    (0x929f, 0x758), (0x9340, 0x7b6), (0x939f, 0x814), (0x9440, 0x872), (0x949f, 0x8d0),
    (0x9540, 0x92e), (0x959f, 0x98c), (0x9640, 0x9ea), (0x969f, 0xa48), (0x9740, 0xaa6),
    (0x979f, 0xb04), (0x9840, 0xb62),
];

/// The PS1/PS2 kernel kanji font (`rom0:KROM`, 16×15 in 16×16 cell, 30 bytes per glyph),
/// addressable by index or by Shift-JIS code via kernel `Krom2RawAdd` (`notes/ps1_boot.md` §4.3).
#[derive(Clone)]
pub struct KromFont {
    data: Vec<u8>,
    intervals: Vec<(u16, u16)>,
}

impl KromFont {
    /// Total glyphs stored across Charset 2 (524) and Charset 3 (2,965).
    pub const GLYPH_COUNT: usize = 3489;

    /// Parses KROM bytes, locating the kernel interval table by content from `bios_context`
    /// if provided, or falling back to [`DEFAULT_KROM_INTERVALS`].
    pub fn new(krom_data: Vec<u8>, bios_context: Option<&[u8]>) -> Self {
        let mut intervals = Vec::new();
        if let Some(bios) = bios_context {
            // Find interval table signature: 0x8140, 0x0000, 0x8180, 0x003F
            let pat = [0x40, 0x81, 0x00, 0x00, 0x80, 0x81, 0x3f, 0x00];
            if let Some(pos) = bios.windows(pat.len()).position(|w| w == pat) {
                let mut p = pos;
                while p + 4 <= bios.len() && intervals.len() < 70 {
                    let code = bios.u16(p);
                    let idx = bios.u16(p + 2);
                    if code == 0 && idx == 0 {
                        break;
                    }
                    intervals.push((code, idx));
                    p += 4;
                }
            }
        }
        if intervals.len() < 51 {
            intervals = DEFAULT_KROM_INTERVALS.to_vec();
        }
        Self {
            data: krom_data,
            intervals,
        }
    }

    /// Number of glyphs in this font.
    pub fn glyph_count(&self) -> usize {
        Self::GLYPH_COUNT.min(self.data.len() / 30)
    }

    /// Character coverage summary.
    pub fn summary(&self) -> &str {
        "PS1 KROM: 3,489 glyphs (16x16, 15 active rows; 524 Charset 2 symbols/alphabets, 2,965 Charset 3 Level-1 Kanji; SJIS 0x8140-0x84BE, 0x889F-0x9872)"
    }

    /// Translates a 16-bit Shift-JIS character code to its glyph index in KROM.
    pub fn code_to_index(&self, sjis: u16) -> Option<usize> {
        if (0x8140..=0x84BE).contains(&sjis) {
            let limit = 19.min(self.intervals.len());
            for &(base_code, base_idx) in self.intervals[..limit].iter().rev() {
                if sjis >= base_code {
                    let idx = base_idx as usize + (sjis - base_code) as usize;
                    return if idx < 524 { Some(idx) } else { None };
                }
            }
        } else if (0x889F..=0x9872).contains(&sjis) {
            let start = 19.min(self.intervals.len());
            let end = 51.min(self.intervals.len());
            for &(base_code, base_idx) in self.intervals[start..end].iter().rev() {
                if sjis >= base_code {
                    let idx = base_idx as usize + (sjis - base_code) as usize;
                    return if idx < 2965 { Some(524 + idx) } else { None };
                }
            }
        }
        None
    }

    /// Re-implements PS1 kernel syscall `B(51h) Krom2RawAdd(shiftjis_code)`
    /// (`notes/research/ps1_version_matrix.md` §2.1, problemkaputt PSXSPX).
    /// Returns the kernel virtual address in ROM (`0xBFC66000` / `0xBFC69D68`), or `None`.
    pub fn krom2rawadd(&self, sjis: u16) -> Option<u32> {
        if (0x8140..=0x84BE).contains(&sjis) {
            let limit = 19.min(self.intervals.len());
            for &(base_code, base_idx) in self.intervals[..limit].iter().rev() {
                if sjis >= base_code {
                    let idx = base_idx as usize + (sjis - base_code) as usize;
                    return if idx < 524 { Some(0xBFC6_6000 + (idx * 30) as u32) } else { None };
                }
            }
        } else if (0x889F..=0x9872).contains(&sjis) {
            let start = 19.min(self.intervals.len());
            let end = 51.min(self.intervals.len());
            for &(base_code, base_idx) in self.intervals[start..end].iter().rev() {
                if sjis >= base_code {
                    let idx = base_idx as usize + (sjis - base_code) as usize;
                    return if idx < 2965 { Some(0xBFC6_9D68 + (idx * 30) as u32) } else { None };
                }
            }
        }
        None
    }

    /// Decodes the 16×16 RGBA glyph for `sjis`, or `None` if not mapped.
    pub fn glyph_by_code(&self, sjis: u16) -> Option<Glyph> {
        let idx = self.code_to_index(sjis)?;
        self.glyph(idx)
    }

    /// Decodes the 16×16 RGBA glyph at sequential index `index`.
    pub fn glyph(&self, index: usize) -> Option<Glyph> {
        if index >= self.glyph_count() {
            return None;
        }
        let offset = if index < 524 {
            index * 30
        } else {
            0x3D68 + (index - 524) * 30
        };
        if offset + 30 > self.data.len() {
            return None;
        }
        let raw = &self.data[offset..offset + 30];
        let mut rgba = vec![0u8; 16 * 16 * 4];
        for row in 0..15 {
            let bits = u16::from_be_bytes([raw[row * 2], raw[row * 2 + 1]]);
            for col in 0..16 {
                if (bits >> (15 - col)) & 1 != 0 {
                    let o = (row * 16 + col) * 4;
                    rgba[o..o + 4].copy_from_slice(&[255, 255, 255, 255]);
                }
            }
        }
        Some(Glyph {
            width: 16,
            height: 16,
            rgba,
        })
    }

    /// Packs a range of KROM glyphs into an atlas image.
    pub fn atlas(&self, range: impl RangeBounds<usize>) -> (usize, usize, Vec<u8>) {
        BenchFont {
            name: "KROM".into(),
            width: 16,
            height: 16,
            count: self.glyph_count(),
            summary: self.summary().into(),
            provider: Box::new(self.clone()),
        }
        .atlas(range)
    }
}

impl FontProvider for KromFont {
    fn glyph_count(&self) -> usize {
        self.glyph_count()
    }
    fn glyph(&self, index: usize) -> Option<Glyph> {
        self.glyph(index)
    }
}

// ---------------------------------------------------------------------------------------
// PS1 / PS2 Gaiji Block
// ---------------------------------------------------------------------------------------

/// The 16×16 1-bit Gaiji (外字) font glyphs at PS1 `0x64000` / PS2 `rom0:KROMG`
/// (`notes/research/version_evolution.md` §3.4: 32 bytes per glyph, 229 glyphs).
#[derive(Clone)]
pub struct GaijiFont {
    data: Vec<u8>,
}

impl GaijiFont {
    /// Number of glyphs expected in retail dumps (7,351 bytes / 32 = 229).
    pub const GLYPH_COUNT: usize = 229;

    /// Parses the Gaiji block bytes.
    pub fn new(data: Vec<u8>) -> Self {
        Self { data }
    }

    /// Number of glyphs in this block.
    pub fn glyph_count(&self) -> usize {
        self.data.len() / 32
    }

    /// Character coverage summary.
    pub fn summary(&self) -> &str {
        "Gaiji block: 229 glyphs (16x16 1-bit, European accented characters and graphical symbols)"
    }

    /// Decodes the 16×16 RGBA glyph at index `index`.
    pub fn glyph(&self, index: usize) -> Option<Glyph> {
        if index >= self.glyph_count() {
            return None;
        }
        let offset = index * 32;
        if offset + 32 > self.data.len() {
            return None;
        }
        let raw = &self.data[offset..offset + 32];
        let mut rgba = vec![0u8; 16 * 16 * 4];
        for row in 0..16 {
            let bits = u16::from_be_bytes([raw[row * 2], raw[row * 2 + 1]]);
            for col in 0..16 {
                if (bits >> (15 - col)) & 1 != 0 {
                    let o = (row * 16 + col) * 4;
                    rgba[o..o + 4].copy_from_slice(&[255, 255, 255, 255]);
                }
            }
        }
        Some(Glyph {
            width: 16,
            height: 16,
            rgba,
        })
    }

    /// Packs a range of Gaiji glyphs into an atlas image.
    pub fn atlas(&self, range: impl RangeBounds<usize>) -> (usize, usize, Vec<u8>) {
        BenchFont {
            name: "KROMG".into(),
            width: 16,
            height: 16,
            count: self.glyph_count(),
            summary: self.summary().into(),
            provider: Box::new(self.clone()),
        }
        .atlas(range)
    }
}

impl FontProvider for GaijiFont {
    fn glyph_count(&self) -> usize {
        self.glyph_count()
    }
    fn glyph(&self, index: usize) -> Option<Glyph> {
        self.glyph(index)
    }
}

// ---------------------------------------------------------------------------------------
// PS2 FONTM Font
// ---------------------------------------------------------------------------------------

/// The 26×26 4-bit JIS kanji/kana font from `rom0:FONTM`
/// (`notes/menu_survey.md` §4: 4,418 glyphs, 338 bytes each).
#[derive(Clone)]
pub struct FontmFont {
    data: Vec<u8>,
    count: usize,
    table_offset: usize,
}

impl FontmFont {
    /// Number of glyphs in retail FONTM builds (4,418).
    pub const GLYPH_COUNT: usize = 4418;

    /// Parses an unpacked FONTM container (`"FBJ2"` header).
    pub fn new(unpacked: Vec<u8>) -> Result<Self> {
        if unpacked.len() < 0x20 || &unpacked[..4] != b"FBJ2" {
            return Err(Error::Corrupt(Format::Bios, "invalid FONTM header".into()));
        }
        let block_off = unpacked.u32(0x0C) as usize;
        if block_off + 8 > unpacked.len() {
            return Err(Error::Corrupt(Format::Bios, "truncated FONTM block table".into()));
        }
        let count = unpacked.u32(block_off) as usize;
        let table_offset = block_off + 8;
        if table_offset + count * 4 > unpacked.len() {
            return Err(Error::Corrupt(Format::Bios, "truncated FONTM offset table".into()));
        }
        Ok(Self {
            data: unpacked,
            count,
            table_offset,
        })
    }

    /// Number of glyphs in this font.
    pub fn glyph_count(&self) -> usize {
        self.count
    }

    /// Character coverage summary.
    pub fn summary(&self) -> &str {
        "PS2 FONTM: 4,418 glyphs (26x26 4-bit, JIS kanji and kana)"
    }

    /// Decodes the 26×26 RGBA glyph at index `index`.
    pub fn glyph(&self, index: usize) -> Option<Glyph> {
        if index >= self.count {
            return None;
        }
        let g_off = self.data.u32(self.table_offset + index * 4) as usize;
        if g_off + 338 > self.data.len() {
            return None;
        }
        let raw = &self.data[g_off..g_off + 338];
        let mut rgba = vec![0u8; 26 * 26 * 4];
        for y in 0..26 {
            for x in 0..26 {
                let byte = raw[y * 13 + x / 2];
                let nib = if x % 2 == 0 { byte >> 4 } else { byte & 0x0F };
                if nib > 0 {
                    let a = nib * 17;
                    let o = (y * 26 + x) * 4;
                    rgba[o..o + 4].copy_from_slice(&[255, 255, 255, a]);
                }
            }
        }
        Some(Glyph {
            width: 26,
            height: 26,
            rgba,
        })
    }

    /// Packs a range of FONTM glyphs into an atlas image.
    pub fn atlas(&self, range: impl RangeBounds<usize>) -> (usize, usize, Vec<u8>) {
        BenchFont {
            name: "FONTM".into(),
            width: 26,
            height: 26,
            count: self.count,
            summary: self.summary().into(),
            provider: Box::new(self.clone()),
        }
        .atlas(range)
    }
}

impl FontProvider for FontmFont {
    fn glyph_count(&self) -> usize {
        self.glyph_count()
    }
    fn glyph(&self, index: usize) -> Option<Glyph> {
        self.glyph(index)
    }
}

// ---------------------------------------------------------------------------------------
// PS2 FNTIMAGE Sheets
// ---------------------------------------------------------------------------------------

/// The 16-entry RGBA CLUT from `tools/font2png.py` and `notes/menu_survey.md` §4.
const DEFAULT_FNT_CLUT: [[u8; 4]; 16] = [
    [0, 0, 0, 0],
    [0, 0, 0, 50],
    [0, 0, 0, 100],
    [0, 0, 0, 150],
    [0, 0, 0, 200],
    [51, 51, 51, 100],
    [51, 51, 51, 150],
    [51, 51, 51, 200],
    [51, 51, 51, 250],
    [102, 102, 102, 150],
    [102, 102, 102, 200],
    [102, 102, 102, 250],
    [153, 153, 153, 200],
    [153, 153, 153, 250],
    [204, 204, 204, 250],
    [255, 255, 255, 250],
];

/// A 4-bit bitmap font sheet unpacked from `rom0:FNTIMAGE` (`tools/font2png.py`).
#[derive(Clone)]
pub struct FntSheet {
    name: String,
    sheet_w: usize,
    sheet_h: usize,
    glyphs_per_row: usize,
    glyph_count: usize,
    summary: String,
    rgba: Vec<u8>,
}

impl FntSheet {
    /// Width of each glyph cell in pixels.
    pub const CELL_W: usize = 32;
    /// Height of each glyph cell in pixels.
    pub const CELL_H: usize = 40;

    /// Decodes an unpacked 4-bit sheet (`w * h / 2` bytes) with the given glyph layout.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: impl Into<String>,
        raw_4bit: &[u8],
        w: usize,
        h: usize,
        glyphs_per_row: usize,
        glyph_count: usize,
        summary: impl Into<String>,
        clut: Option<&[[u8; 4]; 16]>,
    ) -> Result<Self> {
        let name = name.into();
        let expected = w * h / 2;
        if raw_4bit.len() < expected {
            return Err(Error::Corrupt(Format::Bios, format!("sheet {name} is truncated")));
        }
        let pal = clut.unwrap_or(&DEFAULT_FNT_CLUT);
        let mut rgba = vec![0u8; w * h * 4];
        for y in 0..h {
            for x in 0..w {
                let byte = raw_4bit[y * (w / 2) + x / 2];
                let idx = if x % 2 == 0 { byte & 0x0F } else { byte >> 4 } as usize;
                let c = pal.get(idx).copied().unwrap_or([0, 0, 0, 0]);
                let o = (y * w + x) * 4;
                rgba[o..o + 4].copy_from_slice(&c);
            }
        }
        Ok(Self {
            name,
            sheet_w: w,
            sheet_h: h,
            glyphs_per_row,
            glyph_count,
            summary: summary.into(),
            rgba,
        })
    }

    /// Sheet name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Number of glyphs in this sheet.
    pub fn glyph_count(&self) -> usize {
        self.glyph_count
    }

    /// Character coverage summary.
    pub fn summary(&self) -> &str {
        &self.summary
    }

    /// Decodes the 32×40 RGBA cell of glyph `index`.
    pub fn glyph(&self, index: usize) -> Option<Glyph> {
        if index >= self.glyph_count {
            return None;
        }
        let col = index % self.glyphs_per_row;
        let row = index / self.glyphs_per_row;
        let x0 = col * Self::CELL_W;
        let y0 = row * Self::CELL_H;
        if x0 + Self::CELL_W > self.sheet_w || y0 + Self::CELL_H > self.sheet_h {
            return None;
        }
        let mut g_rgba = vec![0u8; Self::CELL_W * Self::CELL_H * 4];
        for cy in 0..Self::CELL_H {
            for cx in 0..Self::CELL_W {
                let src_o = ((y0 + cy) * self.sheet_w + (x0 + cx)) * 4;
                let dst_o = (cy * Self::CELL_W + cx) * 4;
                g_rgba[dst_o..dst_o + 4].copy_from_slice(&self.rgba[src_o..src_o + 4]);
            }
        }
        Some(Glyph {
            width: Self::CELL_W,
            height: Self::CELL_H,
            rgba: g_rgba,
        })
    }

    /// Full sheet image `(width, height, Vec<u8> RGBA)`.
    pub fn sheet_rgba(&self) -> (usize, usize, &[u8]) {
        (self.sheet_w, self.sheet_h, &self.rgba)
    }

    /// Packs a range of glyphs into an atlas image.
    pub fn atlas(&self, range: impl RangeBounds<usize>) -> (usize, usize, Vec<u8>) {
        BenchFont {
            name: self.name.clone(),
            width: Self::CELL_W,
            height: Self::CELL_H,
            count: self.glyph_count,
            summary: self.summary.clone(),
            provider: Box::new(self.clone()),
        }
        .atlas(range)
    }
}

impl FontProvider for FntSheet {
    fn glyph_count(&self) -> usize {
        self.glyph_count
    }
    fn glyph(&self, index: usize) -> Option<Glyph> {
        self.glyph(index)
    }
}

// ---------------------------------------------------------------------------------------
// Font Bench Top-Level
// ---------------------------------------------------------------------------------------

/// Complete font atelier: inspects and renders every font glyph in the user's BIOS
/// (PS1 KROM, Gaiji, PS2 `FONTM`, and `FNTIMAGE` sheets).
pub struct FontBench {
    fonts: Vec<BenchFont>,
    krom: Option<KromFont>,
    gaiji: Option<GaijiFont>,
    fontm: Option<FontmFont>,
}

impl std::fmt::Debug for FontBench {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let names: Vec<&str> = self.fonts.iter().map(|f| f.name()).collect();
        f.debug_struct("FontBench")
            .field("fonts", &names)
            .finish_non_exhaustive()
    }
}

impl FontBench {
    /// Inspects fonts in a PS1 BIOS dump (KROM and optional Gaiji block A).
    pub fn ps1(bios_data: &[u8]) -> Result<Self> {
        let mut fonts = Vec::new();
        let mut krom_opt = None;
        let mut gaiji_opt = None;

        // KROM at 0x66000..0x7FE70 (notes/research/ps1_version_matrix.md §2.1)
        if bios_data.len() >= 0x7FE70 {
            let krom_bytes = bios_data[0x66000..0x7FE70].to_vec();
            let kf = KromFont::new(krom_bytes, Some(bios_data));
            fonts.push(BenchFont {
                name: "KROM".into(),
                width: 16,
                height: 16,
                count: kf.glyph_count(),
                summary: kf.summary().into(),
                provider: Box::new(kf.clone()),
            });
            krom_opt = Some(kf);
        }

        // Gaiji Block A at 0x64000..0x65CB7 (7,351 bytes, U/E only)
        if bios_data.len() >= 0x65CB7 {
            let gaiji_bytes = &bios_data[0x64000..0x65CB7];
            if gaiji_bytes.iter().any(|&b| b != 0) {
                let gf = GaijiFont::new(gaiji_bytes.to_vec());
                fonts.push(BenchFont {
                    name: "KROMG".into(),
                    width: 16,
                    height: 16,
                    count: gf.glyph_count(),
                    summary: gf.summary().into(),
                    provider: Box::new(gf.clone()),
                });
                gaiji_opt = Some(gf);
            }
        }

        if fonts.is_empty() {
            return Err(Error::NotFound(Format::Bios, "no fonts located in PS1 dump".into()));
        }

        Ok(Self {
            fonts,
            krom: krom_opt,
            gaiji: gaiji_opt,
            fontm: None,
        })
    }

    /// Inspects all fonts in a PS2 BIOS image (`KROM`, `KROMG`, `FONTM`, `FNTIMAGE`).
    pub fn ps2(bios: &RomDir) -> Result<Self> {
        let mut fonts = Vec::new();
        let mut krom_opt = None;
        let mut gaiji_opt = None;
        let mut fontm_opt = None;

        if let Ok(m) = bios.module("KROM") {
            let kf = KromFont::new(m.to_vec(), Some(bios.data()));
            fonts.push(BenchFont {
                name: "KROM".into(),
                width: 16,
                height: 16,
                count: kf.glyph_count(),
                summary: kf.summary().into(),
                provider: Box::new(kf.clone()),
            });
            krom_opt = Some(kf);
        }

        if let Ok(m) = bios.module("KROMG") {
            let gf = GaijiFont::new(m.to_vec());
            fonts.push(BenchFont {
                name: "KROMG".into(),
                width: 16,
                height: 16,
                count: gf.glyph_count(),
                summary: gf.summary().into(),
                provider: Box::new(gf.clone()),
            });
            gaiji_opt = Some(gf);
        }

        if let Ok(m) = bios.module("FONTM") {
            if let Ok(unpacked) = unpack(m, 0) {
                if let Ok(fm) = FontmFont::new(unpacked) {
                    fonts.push(BenchFont {
                        name: "FONTM".into(),
                        width: 26,
                        height: 26,
                        count: fm.glyph_count(),
                        summary: fm.summary().into(),
                        provider: Box::new(fm.clone()),
                    });
                    fontm_opt = Some(fm);
                }
            }
        }

        if let Ok(m) = bios.module("FNTIMAGE") {
            if let Ok(fnt_dir) = RomDir::new(m.to_vec()) {
                let sheets_meta = [
                    ("FNTASCII", 256, 480, 8, 97, "FNTASCII: 97 glyphs (32x40 4-bit, ASCII 0x20..0x80)"),
                    ("FNTEX000", 512, 760, 16, 304, "FNTEX000: 304 glyphs (32x40 4-bit, Latin/Cyrillic/Hangul)"),
                    ("FNTEX001", 512, 760, 16, 304, "FNTEX001: 304 glyphs (32x40 4-bit, extended characters)"),
                    ("FNTEXOSD", 512, 80, 16, 32, "FNTEXOSD: 32 glyphs (32x40 4-bit, OSD symbols)"),
                ];
                for (sname, w, h, per_row, count, sum) in sheets_meta {
                    if let Ok(mod_bytes) = fnt_dir.module(sname) {
                        if let Ok(unp) = unpack(mod_bytes, 0) {
                            if let Ok(sheet) = FntSheet::new(sname, &unp, w, h, per_row, count, sum, None) {
                                fonts.push(BenchFont {
                                    name: sname.into(),
                                    width: FntSheet::CELL_W,
                                    height: FntSheet::CELL_H,
                                    count: sheet.glyph_count(),
                                    summary: sheet.summary().into(),
                                    provider: Box::new(sheet),
                                });
                            }
                        }
                    }
                }
            }
        }

        if fonts.is_empty() {
            return Err(Error::NotFound(Format::Bios, "no fonts located in PS2 dump".into()));
        }

        Ok(Self {
            fonts,
            krom: krom_opt,
            gaiji: gaiji_opt,
            fontm: fontm_opt,
        })
    }

    /// Opens a BIOS dump from disk and inspects its fonts (PS1 or PS2).
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let data = std::fs::read(path)?;
        Self::from_bytes(&data)
    }

    /// Inspects fonts from in-memory BIOS dump bytes.
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if Ps1Bios::detect(data) {
            return Self::ps1(data);
        }
        if let Ok(rom) = RomDir::new(data.to_vec()) {
            return Self::ps2(&rom);
        }
        Err(Error::NotA(Format::Bios, "unrecognised BIOS dump".into()))
    }

    /// Slice of all available fonts.
    pub fn fonts(&self) -> &[BenchFont] {
        &self.fonts
    }

    /// Looks up a font by name (case-insensitive, e.g. `"krom"`, `"gaiji"`, `"fontm"`, `"fntascii"`).
    pub fn font(&self, name: &str) -> Option<&BenchFont> {
        let lower = name.to_ascii_lowercase();
        self.fonts.iter().find(|f| {
            let fn_lower = f.name().to_ascii_lowercase();
            fn_lower == lower
                || (lower == "gaiji" && fn_lower == "kromg")
                || (lower == "kromg" && fn_lower == "gaiji")
        })
    }

    /// List of font names present in this bench.
    pub fn font_names(&self) -> Vec<&str> {
        self.fonts.iter().map(|f| f.name()).collect()
    }

    /// Direct access to the KROM font, if present.
    pub fn krom(&self) -> Option<&KromFont> {
        self.krom.as_ref()
    }

    /// Direct access to the Gaiji block font, if present.
    pub fn gaiji(&self) -> Option<&GaijiFont> {
        self.gaiji.as_ref()
    }

    /// Direct access to the FONTM font, if present.
    pub fn fontm(&self) -> Option<&FontmFont> {
        self.fontm.as_ref()
    }
}
