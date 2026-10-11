//! Asset benches for auditioning every sound program and inspecting every font glyph
//! in PlayStation BIOS dumps without shipping any proprietary data (`notes/sound.md` §1.3,
//! `notes/research/version_evolution.md` §3.4, `notes/research/ps1_shell_scenes.md` §2,
//! `notes/research/ps1_version_matrix.md` §2.1, `notes/menu_survey.md` §4).

use crate::bytes::Bytes;
use crate::locate::{LogoLayout, ProgramImage};
use crate::banks::BankRef;
use crate::ps1::{NoteEvent, Vab};
use crate::ps1bios::Ps1Bios;
use crate::rom::{unpack, RomDir};
use crate::sound::{embedded_boot_sounds, envelope, mix_voice, DriverTables, ProgramKind, SoundBank, SAMPLE_RATE};
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

/// One UI sound-effect entry from SNDOSDDH or its inline counterpart (sound.md §2.4).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct BenchEffect {
    /// Effect index within the OSD effect table, distinct from a MIDI program ID.
    pub id: usize,
    /// Index into [`SoundBench::samples`].
    pub sample_index: usize,
    /// Left and right SPU volume registers before the bank's master volume.
    pub volume: (u16, u16),
    /// Recorded SPU pitch register (0x1000 = 44.1 kHz).
    pub pitch: u16,
    /// Raw ADSR registers.
    pub adsr: (u16, u16),
    /// Raw effect flags, including bit 7 for reverb send.
    pub flags: u16,
}

#[derive(Clone)]
enum SoundBenchBackend {
    Ps2 { bank: SoundBank, tables: DriverTables },
    Ps1 { vab: Vab },
    Sample { rate: u32 },
}

/// Asset workbench for sound banks: inspect programs, tones, and samples, and render
/// notes to 48 kHz stereo PCM (`notes/sound.md`, `notes/research/ps1_shell_scenes.md` §2).
#[derive(Clone)]
#[non_exhaustive]
pub struct SoundBench {
    /// All programs in the bank, ordered by program ID.
    pub programs: Vec<BenchProgram>,
    /// Bank addressed by sample/song references.
    pub bank: BankRef,
    /// Content identity assigned when this bench belongs to a runtime sound library.
    pub key: Option<crate::banks::BankKey>,
    /// All decoded samples referenced by the programs and UI effects. PS1 VAG zero is absent.
    pub samples: Vec<BenchSample>,
    /// All OSD UI effects, when the PS2 image includes the second bank.
    pub effects: Vec<BenchEffect>,
    sample_data: Vec<(Vec<f32>, Option<usize>)>,
    effect_samples: Vec<(Vec<f32>, Option<usize>)>,
    effect_master: u32,
    backend: SoundBenchBackend,
}

impl std::fmt::Debug for SoundBench {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SoundBench")
            .field("programs", &self.programs.len())
            .field("samples", &self.samples.len())
            .field("effects", &self.effects.len())
            .finish_non_exhaustive()
    }
}

/// An instrument's audible character, measured from one note rendered from the user's bank
/// ([`SoundBench::character`]). The style is a heuristic grouping for browsing, not a name
/// the console uses.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Character {
    /// Time to reach 90 % of the peak level, in milliseconds.
    pub attack_ms: f32,
    /// How long the note stays above 10 % of its peak within the measured window, in ms.
    pub ring_ms: f32,
    /// Zero-crossing rate of the note, in Hz: a coarse brightness measure.
    pub brightness_hz: f32,
    /// Energy of the sample-to-sample difference over the signal's energy: high for noise.
    pub noisiness: f32,
    /// Level three quarters into the window relative to the peak.
    pub sustain: f32,
    /// Any of the program's tones uses a looping sample.
    pub looped: bool,
    /// The grouping the measurements put the program in.
    pub style: Style,
}

/// Browsing groups for instruments, from fast and percussive to slow and textural.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum Style {
    /// Instant, bright attack that rings on.
    BellsAndChimes,
    /// Fast attack on a held, looping tone.
    KeysAndBellPads,
    /// Short-to-medium attack, decays without a loop.
    MalletsAndPlucks,
    /// Medium attack, held: usable as a melody voice.
    SoftLeadsAndStrings,
    /// Slow swell, bright.
    BrightPads,
    /// Slow swell, mid-range.
    WarmPads,
    /// Slow swell, dark and low in content.
    DarkPadsAndDrones,
    /// Swells in and fades out on its own (no loop).
    Swells,
    /// Sustained noise: air, wind, shimmer.
    AirAndNoise,
    /// Short noise bursts.
    PercussiveNoise,
    /// Unmeasured game samples, grouped without spectral classification.
    Samples,
}

impl Style {
    /// Every style in browsing order.
    pub const ALL: [Style; 11] = [Style::BellsAndChimes, Style::KeysAndBellPads, Style::MalletsAndPlucks, Style::SoftLeadsAndStrings, Style::BrightPads, Style::WarmPads, Style::DarkPadsAndDrones, Style::Swells, Style::AirAndNoise, Style::PercussiveNoise, Style::Samples];
    /// A short display name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Style::BellsAndChimes => "Bells & chimes",
            Style::KeysAndBellPads => "Keys & bell pads",
            Style::MalletsAndPlucks => "Mallets & plucks",
            Style::SoftLeadsAndStrings => "Soft leads & strings",
            Style::BrightPads => "Bright pads",
            Style::WarmPads => "Warm pads",
            Style::DarkPadsAndDrones => "Dark pads & drones",
            Style::Swells => "Swells",
            Style::AirAndNoise => "Air & noise",
            Style::PercussiveNoise => "Percussive noise",
            Style::Samples => "Samples",
        }
    }

    /// The grouping rule over the measured character (thresholds chosen on the PS1 VAB and the
    /// PS2 banks of the user's dumps; documented, not authoritative).
    fn classify(attack_ms: f32, ring_ms: f32, brightness_hz: f32, noisiness: f32, looped: bool) -> Style {
        if noisiness > 0.9 { return if ring_ms < 600.0 { Style::PercussiveNoise } else { Style::AirAndNoise } }
        if noisiness > 0.5 && attack_ms > 1000.0 { return Style::AirAndNoise }
        if attack_ms < 100.0 && brightness_hz > 3000.0 { return Style::BellsAndChimes }
        if !looped && ring_ms < 600.0 { return Style::PercussiveNoise }
        if !looped { return if attack_ms < 600.0 { Style::MalletsAndPlucks } else { Style::Swells } }
        if attack_ms < 300.0 { return Style::KeysAndBellPads }
        if attack_ms < 1200.0 { return if brightness_hz > 3000.0 { Style::BrightPads } else { Style::SoftLeadsAndStrings } }
        if brightness_hz > 3000.0 { Style::BrightPads } else if brightness_hz < 800.0 { Style::DarkPadsAndDrones } else { Style::WarmPads }
    }
}

/// Measures a rendered stereo note (48 kHz) into a [`Character`].
fn measure(pcm: &[f32], looped: bool) -> Character {
    let mono: Vec<f32> = pcm.chunks(2).map(|c| (c[0] + c.get(1).copied().unwrap_or(0.0)) * 0.5).collect();
    let window = (SAMPLE_RATE / 100).max(1);
    let env: Vec<f32> = mono.chunks(window).map(|c| (c.iter().map(|v| v * v).sum::<f32>() / c.len() as f32).sqrt()).collect();
    let peak = env.iter().copied().fold(0.0f32, f32::max).max(1e-9);
    let ms = |frames: usize| frames as f32 * 10.0;
    let attack_ms = ms(env.iter().position(|&e| e >= peak * 0.9).unwrap_or(0));
    let ring_ms = ms(env.iter().rposition(|&e| e >= peak * 0.1).unwrap_or(0));
    let crossings = mono.windows(2).filter(|w| (w[0] >= 0.0) != (w[1] >= 0.0)).count();
    let brightness_hz = crossings as f32 / mono.len().max(1) as f32 * SAMPLE_RATE as f32 / 2.0;
    let energy = mono.iter().map(|v| v * v).sum::<f32>().max(1e-12);
    let noisiness = mono.windows(2).map(|w| (w[1] - w[0]).powi(2)).sum::<f32>() / energy;
    let sustain = env.get(env.len() * 3 / 4).copied().unwrap_or(0.0) / peak;
    Character { attack_ms, ring_ms, brightness_hz, noisiness, sustain, looped, style: Style::classify(attack_ms, ring_ms, brightness_hz, noisiness, looped) }
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
            Err(Error::NotFound(..)) if bios.module("OSOPEN").is_ok() => embedded_boot_sounds(bios)?.0,
            Err(e) => return Err(e),
        };
        let tables =
            bios.module("OSDSND").ok().and_then(DriverTables::from_driver).unwrap_or_else(DriverTables::computed);
        let mut bench = Self::from_bank_and_tables(bank, tables);
        if let Ok(module) = bios.module("SNDIMAGE") {
            let archive = RomDir::new(module.to_vec())?;
            if let Ok(header) = archive.module("SNDOSDDH") {
                bench.add_effects(&unpack(header, 0)?, &unpack(archive.module("SNDOSDDB")?, 0)?)?;
            }
        } else {
            // [V] 1.00 J OSDSYS contains an inline effect header at 0x279680;
            // OSDSND is the driver. Locate the SShd/table structure, not that address.
            let image = ProgramImage::from_module(bios.module("OSDSYS")?, 0x200000)?;
            for signature in image.find_all(b"SShd", 4) {
                let Some(start) = signature.checked_sub(12) else { continue };
                let data = &image.data()[start..];
                if data.len() < 0x30 || data.u32(0x10) != u32::MAX || data.u32(0x2C) == u32::MAX {
                    continue;
                }
                let table = data.u32(0x2C) as usize;
                let Some(last) = data.get(table..table.saturating_add(8)) else { continue };
                let Some(count) = (last.u32(4) as usize).checked_add(1) else { continue };
                let Some(header_len) = count.checked_mul(0x40).and_then(|n| table.checked_add(0x20 + n)) else {
                    continue;
                };
                let body_start = header_len.saturating_add(63) & !63;
                let Some(header) = data.get(..header_len) else { continue };
                let Some(body) = data.get(body_start..body_start.saturating_add(data.u32(4) as usize)) else {
                    continue;
                };
                bench.add_effects(header, body)?;
                break;
            }
        }
        Ok(bench)
    }

    /// Inspects the logo's SShd effect bank, located by content (`notes/ps2logo.md` §5).
    /// No MIDI programs are invented for this effect-only bank; use `effects`/`render_effect`.
    pub fn ps2_logo(bios: &RomDir) -> Result<Self> {
        let image = ProgramImage::from_module(bios.module("PS2LOGO")?, 0x100000)?;
        let layout = LogoLayout::discover(&image)?;
        let bad = || Error::Corrupt(Format::SoundBank, "truncated logo sound bank".into());
        let start = image.at(layout.bank_header).ok_or_else(bad)?;
        let data = image.data().get(start..).ok_or_else(bad)?;
        if data.len() < 0x30 { return Err(bad()); }
        // Effect-only headers can use the unused size word's 0xFFFFFFFF sentinel
        // (sound.md §3.1). Derive the extent from the SE table rather than that word.
        let table = data.u32(0x2C) as usize;
        let count_end = table.checked_add(8).ok_or_else(bad)?;
        let prefix = data.get(table..count_end).ok_or_else(bad)?;
        let count = (prefix.u32(4) as usize).checked_add(1).ok_or_else(bad)?;
        let size = count.checked_mul(0x40).and_then(|n| table.checked_add(0x20)?.checked_add(n)).ok_or_else(bad)?;
        let header = data.get(..size).ok_or_else(bad)?;
        // DMA body follows the header on a 64-byte boundary (ps2logo.md §5).
        let body_start = size.checked_add(63).ok_or_else(bad)? & !63;
        let body_end = body_start.checked_add(data.u32(4) as usize).ok_or_else(bad)?;
        let body = data.get(body_start..body_end).ok_or_else(bad)?;
        let tables = bios.module("OSDSND").ok().and_then(DriverTables::from_driver);
        let mut bench = Self::from_bank(SoundBank::new(header, Vec::new())?, tables);
        bench.bank = BankRef::Ps2Logo;
        bench.add_effects(header, body)?;
        Ok(bench)
    }

    /// Inspects a PlayStation 1 shell VAB (`notes/research/ps1_shell_scenes.md` §2).
    /// Reflects [`Vab`]'s shell mixer: contiguous program IDs and full key ranges.
    /// These match all supplied shell banks; arbitrary VAB splits need a richer parser.
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
                    sample_index: (t.vag as usize).checked_sub(1).unwrap_or(usize::MAX),
                    volume: t.vol as i32,
                    pan: t.pan as i32,
                    bend_range: 2,
                });
            }
            programs.push(BenchProgram { id: p_id as i32, volume: p.vol as i32, kind: ProgramKind::Layer, tones });
        }
        let samples = vab
            .samples
            .iter()
            .skip(1)
            .enumerate()
            .map(|(i, (pcm, lp))| BenchSample { index: i, length: pcm.len(), loop_start: *lp })
            .collect();
        Self {
            programs,
            bank: BankRef::Ps1,
            key: None,
            sample_data: vab.samples.iter().skip(1).cloned().collect(),
            samples,
            effects: Vec::new(),
            effect_samples: Vec::new(),
            effect_master: 0,
            backend: SoundBenchBackend::Ps1 { vab: vab.clone() },
        }
    }

    /// Constructs a PS2 sound bench directly from an `SShd` [`SoundBank`] and optional driver tables.
    pub fn from_bank(bank: SoundBank, tables: Option<DriverTables>) -> Self {
        Self::from_bank_and_tables(bank, tables.unwrap_or_else(DriverTables::computed))
    }

    /// Parses one isolated SShd header/body pair, including direct effect entries.
    /// Extents and sentinel handling: `sound_inventory.md` §"SShd boundaries".
    pub fn from_parts(header: &[u8], body: &[u8], tables: Option<DriverTables>) -> Result<Self> {
        let mut bench = Self::from_bank(SoundBank::new(header, body.to_vec())?, tables);
        if header.len() >= 0x30 && header.u32(0x2C) != u32::MAX {
            bench.add_effects(header, body)?;
        }
        Ok(bench)
    }

    /// A bench holding one caller-decoded mono sample (PCM, loop start, declared rate).
    pub fn from_sample(pcm: Vec<f32>, loop_start: Option<usize>, rate: u32) -> Self {
        Self {
            programs: Vec::new(), bank: BankRef::Ps2, key: None,
            samples: vec![BenchSample { index: 0, length: pcm.len(), loop_start }],
            sample_data: vec![(pcm, loop_start)], effects: Vec::new(), effect_samples: Vec::new(),
            effect_master: 0, backend: SoundBenchBackend::Sample { rate },
        }
    }

    /// Builds a sample-only bank from caller-owned mono PCM in SPU scale.
    /// Rates and loop starts are validated; samples are resampled to 44.1 kHz.
    /// This editor normalization is inferred, not a native bank conversion.
    pub fn from_samples(samples: Vec<(Vec<f32>, Option<usize>, u32)>) -> Result<Self> {
        let mut bench = Self::from_sample(Vec::new(), None, 44100);
        bench.samples.clear();
        bench.sample_data.clear();
        for (pcm, loop_start, rate) in samples {
            if !(1000..=192000).contains(&rate) || pcm.len() < 2
                || loop_start.is_some_and(|lp| lp >= pcm.len()) || pcm.iter().any(|s| !s.is_finite())
            {
                return Err(Error::Corrupt(Format::SoundBank, "invalid sample PCM/rate/loop".into()));
            }
            let length = ((pcm.len() as u64 * 44100) / u64::from(rate)) as usize;
            if !(2..=32_000_000).contains(&length) {
                return Err(Error::Corrupt(Format::SoundBank, "sample exceeds audition allocation limit".into()));
            }
            let pcm = if rate == 44100 { pcm } else {
                (0..length).map(|i| {
                    let pos = i as f64 * f64::from(rate) / 44100.0;
                    let at = (pos as usize).min(pcm.len() - 1);
                    let next = (at + 1).min(pcm.len() - 1);
                    pcm[at] + (pcm[next] - pcm[at]) * (pos - at as f64) as f32
                }).collect()
            };
            let loop_start = loop_start.map(|lp| (((lp as u64 * 44100) / u64::from(rate)) as usize).min(length - 1));
            let index = bench.samples.len();
            bench.samples.push(BenchSample { index, length, loop_start });
            bench.sample_data.push((pcm, loop_start));
        }
        Ok(bench)
    }

    /// Recorded rate of decoded sample PCM; ordinary SPU banks use 44.1 kHz.
    /// TESTSPU's VAG declares 48 kHz (`sound_inventory.md` §"VAB and standalone VAG").
    pub fn sample_rate(&self) -> u32 {
        match self.backend { SoundBenchBackend::Sample { rate } => rate, _ => 44100 }
    }

    /// Auditions a loose sample, tuned around MIDI key 60 using its declared rate.
    /// A short fade is an editor processing choice; no native program is invented.
    pub fn render_sample(&self, index: usize, key: u8, velocity: u8, seconds: f64) -> Vec<f32> {
        let Some((total, on)) = render_length(seconds) else { return Vec::new() };
        let mut mix = vec![0.0; total * 2];
        let Some((pcm, lp)) = self.sample_pcm(index) else { return mix };
        if pcm.len() < 2 || key > 127 || velocity > 127 { return mix }
        let env: Vec<_> = (0..total).map(|i| if i < on { 32768.0 } else {
            32768.0 * (total - i) as f32 / (total - on).max(1) as f32
        }).collect();
        let step = f64::from(self.sample_rate()) / SAMPLE_RATE as f64 * 2f64.powf((f64::from(key) - 60.0) / 12.0);
        let gain = f32::from(velocity) / 127.0;
        mix_voice(&mut mix, 0, pcm, lp, &env, step, gain, gain);
        mix
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
                let (pcm, lp) = decode_bank_sample(&bank, off);
                BenchSample { index: i, length: pcm.len(), loop_start: lp }
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
                programs.push(BenchProgram { id: k, volume: p.volume, kind: p.kind, tones });
            }
        }

        Self {
            programs,
            bank: BankRef::Ps2,
            key: None,
            sample_data: offsets.iter().map(|&o| decode_bank_sample(&bank, o)).collect(),
            samples,
            effects: Vec::new(),
            effect_samples: Vec::new(),
            effect_master: 0,
            backend: SoundBenchBackend::Ps2 { bank, tables },
        }
    }

    /// Adds the one-shot effects of an SShd effect bank (`header` + headerless `body`) to this bench.
    pub fn add_effects(&mut self, header: &[u8], body: &[u8]) -> Result<()> {
        let bad = || Error::Corrupt(Format::SoundBank, "truncated OSD effect bank".into());
        if header.len() < 0x30 || &header[0xC..0x10] != b"SShd" {
            return Err(bad());
        }
        let table = header.u32(0x2C) as usize;
        let prefix = header.get(table..table.saturating_add(0x20)).ok_or_else(bad)?;
        let count = (prefix.u32(4) as usize).checked_add(1).ok_or_else(bad)?;
        if count > header.len().saturating_sub(table + 0x20) / 0x40 {
            return Err(bad());
        }
        let mut offsets: Vec<_> = (0..count).map(|i| header.u32(table + 0x20 + i * 0x40 + 16) as usize).collect();
        offsets.sort_unstable();
        offsets.dedup();
        let sample_base = self.samples.len();
        for (i, &offset) in offsets.iter().enumerate() {
            if offset.checked_add(16).is_none_or(|end| end > body.len()) {
                return Err(bad());
            }
            let (pcm, lp) = crate::sound::decode_adpcm(body, offset);
            self.samples.push(BenchSample { index: sample_base + i, length: pcm.len(), loop_start: lp });
            self.sample_data.push((pcm.clone(), lp));
            self.effect_samples.push((pcm, lp));
        }
        self.effect_master = prefix.u32(0);
        for i in 0..count {
            let entry = table + 0x20 + i * 0x40;
            let offset = header.u32(entry + 16) as usize;
            let sample_index = sample_base + offsets.binary_search(&offset).map_err(|_| bad())?;
            self.effects.push(BenchEffect {
                id: i,
                sample_index,
                volume: (header.u16(entry), header.u16(entry + 2)),
                pitch: header.u16(entry + 4),
                adsr: (header.u16(entry + 8), header.u16(entry + 10)),
                flags: header.u16(entry + 14),
            });
        }
        Ok(())
    }

    /// Auditions a PS2 UI effect through the shared ADPCM/ADSR mixer (sound.md §2.4).
    /// Duration rules match [`Self::render`]; pitch and pan come from the effect entry.
    pub fn render_effect(&self, effect_id: usize, seconds: f64) -> Vec<f32> {
        let Some((total, on)) = render_length(seconds) else { return Vec::new() };
        let mut mix = vec![0.0; total * 2];
        let Some(effect) = self.effects.get(effect_id) else { return mix };
        let base = self.samples.len().saturating_sub(self.effect_samples.len());
        let Some((pcm, lp)) = effect.sample_index.checked_sub(base).and_then(|i| self.effect_samples.get(i)) else {
            return mix;
        };
        if pcm.len() < 2 {
            return mix;
        }
        let env = envelope(effect.adsr.0, effect.adsr.1, on, total);
        let gain =
            |v: u16| ((u64::from(v) * u64::from(self.effect_master)) >> 7) as f32 / 16384.0 * (0x3FFF as f32 / 16384.0);
        mix_voice(
            &mut mix,
            0,
            pcm,
            *lp,
            &env,
            effect.pitch as f64 / 4096.0 * 44100.0 / SAMPLE_RATE as f64,
            gain(effect.volume.0),
            gain(effect.volume.1),
        );
        mix
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

    /// Decoded mono PCM at [`Self::sample_rate`] and its optional loop point.
    /// Data comes only from this caller-supplied bank (sound.md §2.1; scenes §2).
    pub fn sample_pcm(&self, index: usize) -> Option<(&[f32], Option<usize>)> {
        self.sample_data.get(index).map(|(pcm, lp)| (pcm.as_slice(), *lp))
    }

    /// How the console plays `tone` of `program` for MIDI `key` at `velocity`: the sample step per
    /// 48 kHz output frame (PS2 includes the driver's integer pitch quantisation, `sound.md` §2.2)
    /// and the left/right gains, with exactly the arithmetic [`SoundBench::render`] uses.
    pub fn voice_parameters(&self, program: &BenchProgram, tone: &BenchTone, key: u8, velocity: u8) -> (f64, f32, f32) {
        match &self.backend {
            SoundBenchBackend::Ps2 { bank, tables } => {
                let synth = crate::sound::Synth { bank, tables };
                let pitch = synth.pitch_register(tone.root.clamp(0, 127), i32::from(key), tone.fine.clamp(-128, 127), 64, tone.bend_range.clamp(0, 127));
                let (pl, pr) = tables.pan.get((tone.pan.clamp(0, 127) >> 2) as usize).copied().unwrap_or((128, 128));
                let vel = i64::from(bank.velocity.get(usize::from(velocity)).copied().unwrap_or(i32::from(velocity)).clamp(0, 127));
                let gain = |pan| crate::sound::Synth::volume_register(127, 127, i64::from(program.volume.clamp(0, 127)), vel, i64::from(tone.volume.clamp(0, 127)), pan) as f32 / 16384.0 * (0x3FFF as f32 / 16384.0);
                (pitch as f64 / 4096.0, gain(pl), gain(pr))
            }
            SoundBenchBackend::Ps1 { vab } => {
                let gain = f32::from(velocity) / 127.0 * tone.volume as f32 / 127.0 * program.volume as f32 / 127.0 * f32::from(vab.master) / 127.0 * (0x3FFF as f32 / 16384.0);
                let pan = (tone.pan as f32 / 127.0).clamp(0.0, 1.0);
                let semis = f32::from(key) - tone.root as f32 + tone.fine as f32 / 128.0;
                (2f64.powf(f64::from(semis) / 12.0) * 44100.0 / SAMPLE_RATE as f64, gain * (1.0 - pan).sqrt(), gain * pan.sqrt())
            }
            SoundBenchBackend::Sample { rate } => (f64::from(*rate) / SAMPLE_RATE as f64 * 2f64.powf((f64::from(key) - 60.0) / 12.0), f32::from(velocity) / 127.0, f32::from(velocity) / 127.0),
        }
    }

    /// Decoded PCM and loop start of any sample index the bench reports, including the samples of
    /// one-shot effects (which live in a separate body but keep their public indices).
    pub fn any_sample_pcm(&self, index: usize) -> Option<(&[f32], Option<usize>)> {
        self.sample_pcm(index).or_else(|| {
            let base = self.samples.len().saturating_sub(self.effect_samples.len());
            self.effect_samples.get(index.checked_sub(base)?).map(|(pcm, lp)| (pcm.as_slice(), *lp))
        })
    }

    /// Measures a loose sample at its declared rate, using MIDI key 60 as the root.
    pub fn sample_character(&self, index: usize) -> Option<Character> {
        let sample = self.sample(index)?;
        Some(measure(&self.render_sample(index, 60, 100, 4.0), sample.loop_start.is_some()))
    }

    /// Measures an effect with its stored pitch, pan and ADSR (`notes/ps2logo.md` §5).
    pub fn effect_character(&self, id: usize) -> Option<Character> {
        let e = self.effects.iter().find(|e| e.id == id)?;
        let looped = self.sample(e.sample_index)?.loop_start.is_some();
        Some(measure(&self.render_effect(id, 4.0), looped))
    }

    /// Renders `frames` stereo frames of a caller's sequence on this bank through the console's own
    /// sequencer and VAB voice paths, including note-off release (`notes/sound.md` §2.2;
    /// `ps1_shell_scenes.md` §2); `ps1` selects the VAB voice path. No audition-length heuristic.
    pub fn render_song(&self, seq: &crate::sound::SoundSequence, channels: &[crate::sound::Channel], frames: usize, ps1: bool) -> Vec<f32> {
        let mut mix = Vec::new();
        match (&self.backend, ps1) {
            (SoundBenchBackend::Ps2 { bank, tables }, false) => {
                let end = seq.events.last().map_or(0, |e| e.update) as f64 / crate::sound::UPDATES_PER_SECOND;
                let tail = (frames as f64 / SAMPLE_RATE as f64 - end).max(0.0);
                crate::sound::Synth { bank, tables }.render_channels(channels, seq, seq.volume as i64, 0.0, tail, (frames as f64 / SAMPLE_RATE as f64).max(120.0), &mut mix);
            }
            (SoundBenchBackend::Ps1 { vab }, true) => {
                let keys: Vec<_> = seq.events.iter().filter_map(|e| {
                    let c = channels.get(e.channel)?;
                    let (note, vel) = match e.kind {
                        crate::sound::EventKind::NoteOn { note, velocity } => (note, velocity),
                        crate::sound::EventKind::NoteOff { note } => (note, 0),
                        _ => return None,
                    };
                    // Song supplies sample-frame timestamps for the VAB clock.
                    Some((e.update as usize, NoteEvent { time: 0, prog: c.program as u8, note: note as u8, vel: vel as u8, pan: c.pan as u8 }, e.channel, c.volume as u8))
                }).collect();
                mix = vab.render_channel_events(&keys, Some(frames), 0);
                let gain = seq.volume as f32 / 127.0;
                for v in &mut mix { *v *= gain }
            }
            _ => {}
        }
        mix.resize(frames * 2, 0.0);
        mix
    }

    /// Measures how `program_id` sounds: one note at its first tone's root (inside the tone's
    /// key range), velocity 100, four seconds, rendered from the user's bank.
    #[must_use]
    pub fn character(&self, program_id: i32) -> Option<Character> {
        let p = self.program(program_id)?;
        let t = p.tones.first()?;
        if t.low > t.high { return None; }
        let note = t.root.clamp(t.low, t.high);
        let looped = p.tones.iter().any(|t| self.samples.get(t.sample_index).is_some_and(|s| s.loop_start.is_some()));
        Some(measure(&self.render(program_id, note, 100, 4.0), looped))
    }

    /// Every program grouped by [`Style`], in browsing order; within a style by program id.
    /// The effect-only logo bank groups effect IDs instead (`notes/ps2logo.md` §5).
    #[must_use]
    pub fn by_style(&self) -> Vec<(Style, Vec<(i32, Character)>)> {
        let mut groups: std::collections::BTreeMap<Style, Vec<(i32, Character)>> = std::collections::BTreeMap::new();
        for p in &self.programs { if let Some(c) = self.character(p.id) { groups.entry(c.style).or_default().push((p.id, c)) } }
        if self.bank == BankRef::Ps2Logo {
            for e in &self.effects { if let Some(c) = self.effect_character(e.id) { groups.entry(c.style).or_default().push((e.id as i32, c)) } }
        }
        groups.into_iter().collect()
    }

    /// Auditions an instrument: synthesises a note at 48 kHz stereo for `seconds`.
    /// Uses the shared ADPCM/ADSR mixers (`notes/sound.md` §2, scenes §2). The key is
    /// released for the last quarter second (last quarter of short clips). Invalid MIDI
    /// notes/programs render silence; nonfinite durations or durations above 120 s return empty.
    pub fn render(&self, program_id: i32, note: i32, velocity: i32, seconds: f64) -> Vec<f32> {
        let Some((total, on_samples)) = render_length(seconds) else { return Vec::new() };
        let mut mix = vec![0.0f32; total * 2];

        if !(0..=127).contains(&note) || velocity <= 0 {
            return mix;
        }

        match &self.backend {
            SoundBenchBackend::Ps2 { bank, tables } => {
                let Some(prog) = bank.programs.get(&program_id) else {
                    return mix;
                };
                let master = 0x3FFF as f32 / 16384.0;
                let synth = crate::sound::Synth { bank, tables };

                for tone in prog.tones.iter().filter(|t| t.low <= note && note <= t.high) {
                    let (pcm, lp) = decode_bank_sample(bank, tone.sample_offset);
                    if pcm.len() < 2 {
                        continue;
                    }
                    let p = tone.pan.clamp(0, 0x7F) >> 2;
                    let (gl, gr) = tables.pan.get(p as usize).copied().unwrap_or((0x80, 0x80));
                    let vel =
                        bank.velocity.get(velocity.clamp(0, 127) as usize).copied().unwrap_or(velocity).clamp(0, 127)
                            as i64;
                    let (seqv, chv, prv, tv) =
                        (0x7Fi64, 0x7Fi64, prog.volume.clamp(0, 127) as i64, tone.volume.clamp(0, 127) as i64);
                    let vol_l = crate::sound::Synth::volume_register(seqv, chv, prv, vel, tv, gl);
                    let vol_r = crate::sound::Synth::volume_register(seqv, chv, prv, vel, tv, gr);
                    let gain_l = vol_l as f32 / 16384.0 * master;
                    let gain_r = vol_r as f32 / 16384.0 * master;

                    let pitch = synth.pitch_register(
                        tone.root.clamp(0, 127),
                        note,
                        tone.fine.clamp(-128, 127),
                        0x40,
                        tone.bend_range,
                    );
                    let step = pitch as f64 / 4096.0;
                    let env = envelope(tone.adsr1, tone.adsr2, on_samples, total);

                    mix_voice(&mut mix, 0, &pcm, lp, &env, step, gain_l, gain_r);

                    if prog.kind == ProgramKind::Split {
                        break;
                    }
                }
            }
            SoundBenchBackend::Sample { .. } => {}
            SoundBenchBackend::Ps1 { vab } => {
                let Ok(prog) = u8::try_from(program_id) else { return mix };
                let key = NoteEvent { time: 0, prog, note: note as u8, vel: velocity.clamp(0, 127) as u8, pan: 64 };
                let off = NoteEvent { vel: 0, ..key };
                return vab.render_events(&[(0, key), (on_samples, off)], Some(total), 0);
            }
        }

        mix
    }
}

fn decode_bank_sample(bank: &SoundBank, offset: usize) -> (Vec<f32>, Option<usize>) {
    if offset.checked_add(16).is_none_or(|end| end > bank.body().len()) {
        return (Vec::new(), None);
    }
    bank.decode_sample(offset)
}

fn render_length(seconds: f64) -> Option<(usize, usize)> {
    if !seconds.is_finite() || seconds <= 0.0 || seconds > 120.0 {
        return None;
    }
    let total = (seconds * SAMPLE_RATE as f64).round() as usize;
    let on = if seconds > 0.5 { ((seconds - 0.25) * SAMPLE_RATE as f64).round() as usize } else { total * 3 / 4 };
    Some((total, on))
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
#[non_exhaustive]
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

    /// Number of indexed slots in this font; malformed/reserved slots may be unavailable.
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

/// The PS1/PS2 kernel kanji font (`rom0:KROM`, 16×15 in 16×16 cell, 30 bytes per glyph),
/// addressable by index or by Shift-JIS code via kernel `Krom2RawAdd` (`notes/ps1_boot.md` §4.3).
#[derive(Clone)]
#[non_exhaustive]
pub struct KromFont {
    data: Vec<u8>,
    intervals: Option<Vec<(u16, u16)>>,
}

impl KromFont {
    /// Total glyphs stored across Charset 2 (524) and Charset 3 (2,965).
    pub const GLYPH_COUNT: usize = 3489;

    /// Keeps raw glyph bytes and reads the two kernel interval tables by content.
    /// Without a valid BIOS table, sequential glyph access remains available, but
    /// Shift-JIS lookup returns `None` (matrix §2.1; no built-in Sony mapping table).
    pub fn new(krom_data: Vec<u8>, bios_context: Option<&[u8]>) -> Self {
        let intervals = bios_context.and_then(|bios| {
            // Krom2RawAdd's two adjacent arrays: 19 symbol runs, then 32 kanji runs.
            // Validate increasing codes/indices within each array, including the reset.
            bios.windows(51 * 4).step_by(4).find_map(|raw| {
                let entries: Vec<_> = raw.chunks_exact(4).map(|e| (e.u16(0), e.u16(2))).collect();
                if entries[0] != (0x8140, 0) || entries[19] != (0x889F, 0) {
                    return None;
                }
                let valid = |runs: &[(u16, u16)], end_code: u16, count: u16| {
                    runs.iter().all(|&(c, i)| c < end_code && i < count)
                        && runs
                            .windows(2)
                            .all(|w| w[1].0 > w[0].0 && w[1].1 > w[0].1 && w[1].1 - w[0].1 <= w[1].0 - w[0].0)
                        && runs.last().is_some_and(|&(c, i)| count - i == end_code - c)
                };
                (valid(&entries[..19], 0x84BF, 524) && valid(&entries[19..], 0x9873, 2965)).then_some(entries)
            })
        });
        Self { data: krom_data, intervals }
    }

    /// Number of glyphs in this font.
    pub fn glyph_count(&self) -> usize {
        Self::GLYPH_COUNT.min(self.data.len() / 30)
    }

    /// Character coverage summary.
    pub fn summary(&self) -> &str {
        "PS1 KROM: 3,489 glyphs (16x16, 15 active rows; 524 Charset 2 symbols/alphabets, 2,965 Charset 3 Level-1 Kanji; SJIS 0x8140-0x84BE, 0x889F-0x9872)"
    }

    /// Translates a Shift-JIS code using the supplied kernel's interval tables.
    /// Undefined gaps return `None`, as does a missing/truncated mapping or glyph.
    pub fn code_to_index(&self, sjis: u16) -> Option<usize> {
        let tables = self.intervals.as_ref()?;
        let (runs, count, offset) = if (0x8140..0x84BF).contains(&sjis) {
            (&tables[..19], 524usize, 0)
        } else if (0x889F..0x9873).contains(&sjis) {
            (&tables[19..], 2965, 524)
        } else {
            return None;
        };
        let run = runs.iter().rposition(|&(code, _)| code <= sjis)?;
        let (code, index) = runs[run];
        let next = runs.get(run + 1).map_or(count, |&(_, i)| i as usize);
        if sjis & 0xFF == 0x7F {
            return None;
        }
        let skip = usize::from(code & 0xFF <= 0x7F && sjis & 0xFF >= 0x80);
        let index = index as usize + (sjis - code) as usize - skip;
        (index < next && offset + index < self.glyph_count()).then_some(offset + index)
    }

    /// Re-implements `B(51h) Krom2RawAdd` for valid characters using the dump's tables.
    /// ROM font bases and 30-byte stride: `ps1_version_matrix.md` §2.1.
    pub fn krom2rawadd(&self, sjis: u16) -> Option<u32> {
        let index = self.code_to_index(sjis)?;
        Some(0xBFC6_6000 + (index * 30) as u32)
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
        let offset = if index < 524 { index * 30 } else { 0x3D68 + (index - 524) * 30 };
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
        Some(Glyph { width: 16, height: 16, rgba })
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

/// Gaiji (外字) at PS1 `0x64000` / PS2 `rom0:KROMG`, in 16x16 cells.
/// Dump correction to `version_evolution.md` §3.4: 245 glyphs of 15 big-endian
/// u16 rows (30 bytes), plus one trailing byte. A 32-byte stride splits the glyphs.
#[derive(Clone)]
#[non_exhaustive]
pub struct GaijiFont {
    data: Vec<u8>,
}

impl GaijiFont {
    /// Complete glyphs in the 7,351-byte retail block (245 × 30, plus a tail byte).
    pub const GLYPH_COUNT: usize = 245;

    /// Parses the Gaiji block bytes.
    pub fn new(data: Vec<u8>) -> Self {
        Self { data }
    }

    /// Number of glyphs in this block.
    pub fn glyph_count(&self) -> usize {
        self.data.len() / 30
    }

    /// Character coverage summary.
    pub fn summary(&self) -> &str {
        "16x16 cells, 15 active rows, 1-bit gaiji; European accented characters and graphical symbols"
    }

    /// Decodes the 16×16 RGBA glyph at index `index`.
    pub fn glyph(&self, index: usize) -> Option<Glyph> {
        if index >= self.glyph_count() {
            return None;
        }
        let offset = index * 30;
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
        Some(Glyph { width: 16, height: 16, rgba })
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
#[non_exhaustive]
pub struct FontmFont {
    data: Vec<u8>,
    count: usize,
    offsets: Vec<usize>,
    row_stride: usize,
    palette: Option<[[u8; 4]; 16]>,
}

impl FontmFont {
    /// Number of slots advertised by retail FONTM builds (4,418; two unavailable).
    pub const GLYPH_COUNT: usize = 4418;

    /// Parses an unpacked FONTM container (`"FBJ2"` header).
    /// Offset entries are relative to the block at header +0x0C, as confirmed by
    /// the OSD font init at 0x20DCF4/0x20DD00 (menu_survey §4). Out-of-file
    /// slots remain unavailable: retail tables have two such entries at their tail.
    pub fn new(unpacked: Vec<u8>) -> Result<Self> {
        if unpacked.len() < 0x20 || &unpacked[..4] != b"FBJ2" {
            return Err(Error::Corrupt(Format::Bios, "invalid FONTM header".into()));
        }
        let block_off = unpacked.u32(0x0C) as usize;
        if block_off.checked_add(8).is_none_or(|end| end > unpacked.len()) {
            return Err(Error::Corrupt(Format::Bios, "truncated FONTM block table".into()));
        }
        let count = unpacked.u32(block_off) as usize;
        let table_offset = block_off + 8;
        if count > unpacked.len().saturating_sub(table_offset) / 4 {
            return Err(Error::Corrupt(Format::Bios, "truncated FONTM offset table".into()));
        }
        Ok(Self {
            count,
            offsets: (0..count)
                .map(|i| block_off.saturating_add(unpacked.u32(table_offset + i * 4) as usize))
                .collect(),
            row_stride: 13,
            palette: None,
            data: unpacked,
        })
    }

    /// Parses the 26x26 glyph sheet in `FNTADD00` using the dump's RGBA palette.
    /// `menu_survey.md` §4 open question: \[V\] 2.00 E's asset has a 64-byte header,
    /// five byte endpoints and a 416-pixel-wide PSMT4 sheet (16 cells per row).
    /// The OSD cache at 0x20C438 copies each glyph row with a 208-byte stride.
    /// \[I\] Header endpoints delimit character groups; code mapping remains unknown.
    pub fn additional(data: Vec<u8>, palette: &[[u8; 4]; 16]) -> Result<Self> {
        let bad = || Error::Corrupt(Format::Bios, "invalid FNTADD00 glyph sheet".into());
        if data.len() < 64 || !data.len().saturating_sub(64).is_multiple_of(16 * 338) {
            return Err(bad());
        }
        let mut previous = 64;
        for i in 0..5 {
            let end = data.u32(i * 4) as usize;
            if end < previous || end > data.len() || !(end - 64).is_multiple_of(338) {
                return Err(bad());
            }
            previous = end;
        }
        if previous != data.len() {
            return Err(bad());
        }
        let count = (data.len() - 64) / 338;
        Ok(Self {
            count,
            offsets: (0..count).map(|i| 64 + (i / 16) * 26 * 208 + (i % 16) * 13).collect(),
            row_stride: 208,
            palette: Some(*palette),
            data,
        })
    }

    /// Number of slots whose full glyph data lies in this container.
    pub fn available_glyph_count(&self) -> usize {
        self.offsets
            .iter()
            .filter(|&&o| o.checked_add(self.row_stride * 25 + 13).is_some_and(|end| end <= self.data.len()))
            .count()
    }

    /// Number of advertised glyph slots; unavailable entries return `None`.
    pub fn glyph_count(&self) -> usize {
        self.count
    }

    /// Character coverage summary.
    pub fn summary(&self) -> &str {
        "26x26 4-bit JIS kanji/kana; header slots may include unavailable glyphs"
    }

    /// Decodes the 26×26 RGBA glyph at index `index`.
    pub fn glyph(&self, index: usize) -> Option<Glyph> {
        if index >= self.count {
            return None;
        }
        let g_off = *self.offsets.get(index)?;
        if g_off.checked_add(self.row_stride * 25 + 13).is_none_or(|end| end > self.data.len()) {
            return None;
        }
        let raw = &self.data[g_off..];
        let mut rgba = vec![0u8; 26 * 26 * 4];
        for y in 0..26 {
            for x in 0..26 {
                let byte = raw[y * self.row_stride + x / 2];
                let low_left = self.palette.is_some();
                let nib = if (x % 2 == 0) == low_left { byte & 15 } else { byte >> 4 };
                let colour = if let Some(pal) = &self.palette {
                    pal[nib as usize]
                } else if nib > 0 {
                    [255, 255, 255, nib * 17]
                } else {
                    [0; 4]
                };
                let o = (y * 26 + x) * 4;
                rgba[o..o + 4].copy_from_slice(&colour);
            }
        }
        Some(Glyph { width: 26, height: 26, rgba })
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

// The uploader's grayscale CLUT, located structurally in the executable rather than
// copied from Sony's data. `menu_survey.md` §4; GS alpha 0x80 represents full opacity.
fn font_clut(image: &[u8]) -> Option<[[u8; 4]; 16]> {
    image.windows(64).step_by(4).find_map(|raw| {
        let mut pal = [[0; 4]; 16];
        for (c, bytes) in pal.iter_mut().zip(raw.chunks_exact(4)) {
            c.copy_from_slice(bytes)
        }
        if pal[0] != [0; 4]
            || pal[15][0] != 255
            || pal.iter().any(|c| c[0] != c[1] || c[1] != c[2] || c[3] > 128)
            || pal[1..5].iter().any(|c| c[0] != 0 || c[3] == 0)
            || pal[1..5].windows(2).any(|w| w[0][3] >= w[1][3])
            || pal[5][0] == 0
            || pal.windows(2).any(|w| w[0][0] > w[1][0])
            || pal[5..].iter().any(|c| c[3] == 0)
        {
            return None;
        }
        for c in &mut pal {
            c[3] = c[3].saturating_mul(2)
        }
        Some(pal)
    })
}

/// A 4-bit bitmap font sheet unpacked from `rom0:FNTIMAGE` (`tools/font2png.py`).
#[derive(Clone)]
#[non_exhaustive]
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
    /// Supply a palette extracted from the OSD; `None` is an error, without a built-in fallback.
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
        let pixels = w
            .checked_mul(h)
            .filter(|&n| n <= 64 << 20)
            .ok_or_else(|| Error::Corrupt(Format::Bios, "invalid font sheet dimensions".into()))?;
        if w == 0
            || h == 0
            || !w.is_multiple_of(2)
            || glyphs_per_row == 0
            || glyphs_per_row > w / Self::CELL_W
            || glyph_count > (h / Self::CELL_H) * glyphs_per_row
        {
            return Err(Error::Corrupt(Format::Bios, "invalid font sheet layout".into()));
        }
        let expected = pixels / 2;
        if raw_4bit.len() < expected {
            return Err(Error::Corrupt(Format::Bios, format!("sheet {name} is truncated")));
        }
        let pal = clut.ok_or_else(|| {
            Error::NotFound(Format::Bios, "FNTIMAGE needs a palette supplied from the user's OSD image".into())
        })?;
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
        Ok(Self { name, sheet_w: w, sheet_h: h, glyphs_per_row, glyph_count, summary: summary.into(), rgba })
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
        Some(Glyph { width: Self::CELL_W, height: Self::CELL_H, rgba: g_rgba })
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
#[non_exhaustive]
pub struct FontBench {
    fonts: Vec<BenchFont>,
    krom: Option<KromFont>,
    gaiji: Option<GaijiFont>,
    fontm: Option<FontmFont>,
}

impl std::fmt::Debug for FontBench {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let names: Vec<&str> = self.fonts.iter().map(|f| f.name()).collect();
        f.debug_struct("FontBench").field("fonts", &names).finish_non_exhaustive()
    }
}

impl FontBench {
    /// Inspects fonts in a PS1 BIOS dump (KROM and optional Gaiji block A).
    pub fn ps1(bios_data: &[u8]) -> Result<Self> {
        let bios = Ps1Bios::new(bios_data.to_vec())?;
        let mut fonts = Vec::new();
        let krom_opt;
        let mut gaiji_opt = None;

        // KROM at 0x66000..0x7FE70 (notes/research/ps1_version_matrix.md §2.1)
        {
            let krom_bytes = bios.krom().to_vec();
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
        // Matrix §2.1: early Japanese images overlap this window with shell data.
        // The supplied 4.0 J dump has a zero-filled block, despite the note's label.
        if matches!(bios.identity().region_letter, Some('A' | 'E'))
            || bios.identity().shell_storage == crate::ps1bios::ShellStorage::Pops
        {
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

        Ok(Self { fonts, krom: krom_opt, gaiji: gaiji_opt, fontm: None })
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
            let fm = FontmFont::new(unpack(m, 0)?)?;
            fonts.push(BenchFont {
                name: "FONTM".into(),
                width: 26,
                height: 26,
                count: fm.glyph_count(),
                summary: format!(
                    "{}; {} of {} slots available",
                    fm.summary(),
                    fm.available_glyph_count(),
                    fm.glyph_count()
                ),
                provider: Box::new(fm.clone()),
            });
            fontm_opt = Some(fm);
        }

        if let Ok(m) = bios.module("FNTIMAGE") {
            let fnt_dir = RomDir::new(m.to_vec())?;
            let image = ProgramImage::from_module(bios.module("OSDSYS")?, 0x200000)?;
            let clut = font_clut(image.data())
                .ok_or_else(|| Error::NotFound(Format::Bios, "FNTIMAGE grayscale CLUT in OSDSYS".into()))?;
            // Sheet layouts from menu_survey §4; pixels and colours come from this dump.
            let sheets_meta = [
                ("FNTASCII", 256, 480, 8, 96, "ASCII 0x20..0x7F"),
                ("FNTASCI2", 256, 480, 8, 96, "China ASCII variant, 0x20..0x7F"),
                ("FNTEX000", 512, 760, 16, 304, "extended Latin/Cyrillic/Hangul"),
                ("FNTEX001", 512, 760, 16, 304, "extended characters"),
                ("FNTEXOSD", 512, 80, 16, 32, "OSD symbols"),
            ];
            for (name, w, h, per_row, count, coverage) in sheets_meta {
                let Ok(module) = fnt_dir.module(name) else { continue };
                let raw = unpack(module, 0)?;
                let sheet = FntSheet::new(name, &raw, w, h, per_row, count, coverage, Some(&clut))?;
                fonts.push(BenchFont {
                    name: name.into(),
                    width: FntSheet::CELL_W,
                    height: FntSheet::CELL_H,
                    count: sheet.glyph_count(),
                    summary: sheet.summary().into(),
                    provider: Box::new(sheet),
                });
            }
            if let Ok(module) = fnt_dir.module("FNTADD00") {
                let font = FontmFont::additional(unpack(module, 0)?, &clut)?;
                fonts.push(BenchFont {
                    name: "FNTADD00".into(),
                    width: 26,
                    height: 26,
                    count: font.glyph_count(),
                    summary: "extra 26x26 4-bit glyphs; character mapping unknown".into(),
                    provider: Box::new(font),
                });
            }
        }

        if fonts.is_empty() {
            return Err(Error::NotFound(Format::Bios, "no fonts located in PS2 dump".into()));
        }

        Ok(Self { fonts, krom: krom_opt, gaiji: gaiji_opt, fontm: fontm_opt })
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
            fn_lower == lower || (lower == "gaiji" && fn_lower == "kromg") || (lower == "kromg" && fn_lower == "gaiji")
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
