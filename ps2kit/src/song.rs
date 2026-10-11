//! A small, clean-room pattern/arrangement engine using the caller's BIOS instruments.
//!
//! Design inspiration only: Terry Cavanagh's **Bosca Ceoil** (FreeBSD /
//! BSD-2-Clause-Views licence), with instrument patterns on a bar grid and a loop range,
//! and John Nesky's **BeepBox** (MIT licence), with channels, bar cells and timed notes.
//! No code or assets from either project are copied. References:
//! <https://github.com/TerryCavanagh/boscaceoil> and
//! <https://github.com/johnnesky/beepbox/blob/main/LICENSE.md>.
//!
//! PS2 timing and SSsq grammar: `notes/sound.md` §§2.2, 3.3. PS1 note tables:
//! `notes/research/ps1_shell_scenes.md` §2 and `notes/ps1_boot.md` §6. Voices use the
//! existing sequencer/VAB mixers, including their current dry-render approximations.
//!
//! `.psxsong` version 1 is UTF-8, one record per line; blank lines and whole-line `#`
//! comments are ignored. Names/annotations use quoted strings with `\"`, `\\`, `\n`,
//! `\r`, `\t` escapes. Records occur in the order emitted by [`Song::to_text`]:
//! `psxsong 1`, `name`, `bank`, `clock` (BPM, ticks/beat, beats/bar), `mix` (volume,
//! tail ticks, end tick or `-`), `loop` (exclusive bar range or `-`), `annotation`,
//! then `channel` (name, program, volume, pan), `arrangement` (pattern indices or `-`),
//! `pattern` (length), `note` (start, length, key, velocity), `endpattern`, `endchannel`.
//! Version 1 retains numeric programs and its original renderer. Extended songs use
//! `psxsong 2`, add bank `logo`, and allow channel instruments `effect:<id>`, `drum`,
//! or `kit`. Before `arrangement`, each `voice` record contains: key (`-` for a single
//! drum), quoted lane name/reason, bank, source (`s<index>` or `p<program>:<key>`),
//! start ms, pitch, pitch drop, drop ms, decay ms, attack ms, low/high cutoff (`-`
//! for bypass), gain, comma-separated retrigger offsets (`-` for none), then noise
//! bank/source/start/pitch/mix (`- - 0 0 0` for none). Kits can add quoted `omission`
//! records. Version 3 adds a `binding` record (quoted fingerprint, label) to every
//! channel, and a pitched `sample:<index>` instrument for loose VAGs. Legacy files
//! migrate to the default binding. Songs contain references and processing parameters,
//! never samples, BIOS bytes or paths. Sampler parameters are editor choices, not BIOS facts.

use crate::bench::SoundBench;
use crate::library::{BankKey, BankResolver};
use crate::ps1::{NoteEvent, Ps1Shell};
use crate::ps1intro::SceIntro;
use crate::rom::{unpack, RomDir};
use crate::sound::{self, EventKind, SequenceEvent, SoundSequence, SAMPLE_RATE};
use crate::{Error, Format, Result, VideoMode};
use std::fmt::Write as _;
use std::ops::Range;
use std::path::Path;

const MAX_ITEMS: usize = 8192;
const MAX_TEXT: usize = 4 << 20;
const MAX_SECONDS: f64 = 120.0;

/// Which caller-supplied instrument bank the program numbers address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum BankRef {
    /// PS2 SShd boot bank.
    Ps2,
    /// PS2 logo SShd effect bank (`notes/ps2logo.md` §5).
    Ps2Logo,
    /// PS1 shell VAB, including the PS2's embedded PS1 shell.
    Ps1,
}

/// A sample index or a tone selected through a program/key pair; never contains audio.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SampleSource {
    /// Decoded sample index in the referenced bench.
    Sample(usize),
    /// First tone answering this MIDI key, at its tuned playback pitch.
    Program {
        /// Program ID.
        program: u8,
        /// MIDI key.
        key: u8,
    },
}

/// A portable reference resolved against a caller-supplied bench.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct SampleRef {
    /// Bank containing the source.
    pub bank: BankRef,
    /// Sample index or program/key selection.
    pub source: SampleSource,
}

impl SampleRef {
    /// References one decoded sample at its recorded 44.1 kHz pitch.
    pub fn sample(bank: BankRef, index: usize) -> Self {
        Self {
            bank,
            source: SampleSource::Sample(index),
        }
    }

    /// References the first tone selected by a program/key pair.
    pub fn program(bank: BankRef, program: u8, key: u8) -> Self {
        Self {
            bank,
            source: SampleSource::Program { program, key },
        }
    }

    fn resolve(self, bench: &SoundBench) -> Option<(&[f32], Option<usize>, f32)> {
        if self.bank != bench.bank {
            return None;
        }
        let (index, pitch) = match self.source {
            SampleSource::Sample(i) => (i, 0.0),
            SampleSource::Program { program, key } => {
                let t = bench
                    .program(i32::from(program))?
                    .tones
                    .iter()
                    .find(|t| t.low <= i32::from(key) && i32::from(key) <= t.high)?;
                let fine = if self.bank == BankRef::Ps1 {
                    128.0
                } else {
                    16.0
                };
                (
                    t.sample_index,
                    f32::from(key) - t.root as f32 + t.fine as f32 / fine,
                )
            }
        };
        let (pcm, lp) = bench.sample_pcm(index)?;
        Some((pcm, lp, pitch))
    }
}

/// Optional noise layer, also taken from a sample in the user's bank.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct NoiseLayer {
    /// Sample supplying the layer; no generated noise is used.
    pub source: SampleRef,
    /// Skip milliseconds in this layer independently of the main source.
    pub start_ms: f32,
    /// Pitch offset in semitones.
    pub pitch: f32,
    /// Linear layer level, 0..=4.
    pub mix: f32,
}

impl NoiseLayer {
    /// Adds a source-derived noise layer at its recorded pitch.
    pub fn new(source: SampleRef, mix: f32) -> Self {
        Self {
            source,
            start_ms: 0.0,
            pitch: 0.0,
            mix,
        }
    }
}

/// Sampler processing parameters. These are authored DSP choices, not console behaviour.
/// Samples/loops are decoded by the existing SPU decoder (sound.md §2.1; scenes §2).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct DrumVoice {
    /// Source sample, never embedded PCM.
    pub source: SampleRef,
    /// Skip this many milliseconds at the source's recorded rate.
    pub start_ms: f32,
    /// Final pitch offset in semitones.
    pub pitch: f32,
    /// Initial extra semitones, falling linearly to zero over `pitch_drop_ms`.
    pub pitch_drop: f32,
    /// Duration of the pitch drop in milliseconds.
    pub pitch_drop_ms: f32,
    /// Exponential amplitude envelope's time to -60 dB, then a short fade to zero.
    pub decay_ms: f32,
    /// Linear attack in milliseconds.
    pub attack_ms: f32,
    /// Optional one-pole low-pass cutoff, Hz.
    pub low_pass_hz: Option<f32>,
    /// Optional one-pole high-pass cutoff, Hz.
    pub high_pass_hz: Option<f32>,
    /// Optional source-derived noise, high-passed at 2 kHz and low-passed at 18 kHz.
    pub noise_mix: Option<NoiseLayer>,
    /// Linear level before soft clipping, 0..=8.
    pub gain: f32,
    /// Additional attacks at these millisecond offsets (for a sampled clap).
    pub retriggers_ms: Vec<f32>,
}

impl DrumVoice {
    /// A neutral 180 ms sample-derived voice, ready for parameter editing.
    pub fn new(source: SampleRef) -> Self {
        Self {
            source,
            start_ms: 0.0,
            pitch: 0.0,
            pitch_drop: 0.0,
            pitch_drop_ms: 30.0,
            decay_ms: 180.0,
            attack_ms: 1.0,
            low_pass_hz: None,
            high_pass_hz: None,
            noise_mix: None,
            gain: 0.8,
            retriggers_ms: Vec::new(),
        }
    }

    /// Checks finite DSP parameters and a bounded, at-most-two-second one-shot.
    pub fn validate(&self) -> Result<()> {
        let range = |v: f32, a: f32, b: f32| v.is_finite() && (a..=b).contains(&v);
        let source_ok = |s: SampleRef| match s.source {
            SampleSource::Sample(i) => i < MAX_ITEMS,
            SampleSource::Program { program, key } => program <= 127 && key <= 127,
        };
        if !source_ok(self.source)
            || !range(self.start_ms, 0.0, 120000.0)
            || !range(self.pitch, -96.0, 96.0)
            || !range(self.pitch_drop, -48.0, 48.0)
            || !range(self.pitch_drop_ms, 0.0, 1000.0)
            || !range(self.decay_ms, 1.0, 1500.0)
            || !range(self.attack_ms, 0.0, self.decay_ms)
            || !range(self.gain, 0.0, 8.0)
            || [self.low_pass_hz, self.high_pass_hz]
                .into_iter()
                .flatten()
                .any(|f| !range(f, 10.0, 20000.0))
            || self.retriggers_ms.len() > 8
            || self.retriggers_ms.iter().any(|&t| !range(t, 0.0, 400.0))
            || self.noise_mix.as_ref().is_some_and(|n| {
                !source_ok(n.source)
                    || !range(n.start_ms, 0.0, 120000.0)
                    || !range(n.pitch, -96.0, 96.0)
                    || !range(n.mix, 0.0, 4.0)
            })
        {
            return Err(bad("invalid drum source or DSP parameters"));
        }
        Ok(())
    }

    /// Renders 48 kHz centred stereo with resampled pitch, filters, decay and soft clipping.
    /// Missing/incompatible references are silent; invalid parameters return an error.
    pub fn render(&self, bench: &SoundBench) -> Result<Vec<f32>> {
        self.validate()?;
        let last = self.retriggers_ms.iter().copied().fold(0.0f32, f32::max);
        let frames = ((self.decay_ms + last + 10.0) * SAMPLE_RATE as f32 / 1000.0).ceil() as usize;
        let mut mono = vec![0.0; frames];
        for offset in std::iter::once(0.0).chain(self.retriggers_ms.iter().copied()) {
            let at = (offset * SAMPLE_RATE as f32 / 1000.0).round() as usize;
            let body = sample_voice(
                self,
                bench,
                self.source,
                self.start_ms,
                self.pitch,
                self.low_pass_hz,
                self.high_pass_hz,
            );
            let noise = self.noise_mix.as_ref().map(|n| {
                sample_voice(
                    self,
                    bench,
                    n.source,
                    n.start_ms,
                    n.pitch,
                    Some(18000.0),
                    Some(2000.0),
                )
            });
            for (i, &v) in body.iter().enumerate() {
                let layer = noise.as_ref().map_or(0.0, |pcm| {
                    pcm[i] * self.noise_mix.as_ref().map_or(0.0, |n| n.mix)
                });
                if let Some(out) = mono.get_mut(at + i) {
                    *out += (v + layer) * self.gain;
                }
            }
        }
        Ok(mono
            .into_iter()
            .flat_map(|v| {
                let v = v.tanh();
                [v, v]
            })
            .collect())
    }
}

fn sample_voice(
    voice: &DrumVoice,
    bench: &SoundBench,
    source: SampleRef,
    start_ms: f32,
    pitch: f32,
    low: Option<f32>,
    high: Option<f32>,
) -> Vec<f32> {
    let frames = ((voice.decay_ms + 10.0) * SAMPLE_RATE as f32 / 1000.0).ceil() as usize;
    let mut out = vec![0.0; frames];
    let Some((pcm, lp, root)) = source.resolve(bench) else {
        return out;
    };
    if pcm.len() < 2 {
        return out;
    }
    let norm = pcm.iter().fold(0.0f32, |a, b| a.max(b.abs())).max(1e-9);
    let mut position = f64::from(start_ms) * (f64::from(bench.sample_rate()) / 1000.0);
    let mut low_state = 0.0;
    let mut high_state = 0.0;
    let coefficient = |hz: f32| 1.0 - (-std::f32::consts::TAU * hz / SAMPLE_RATE as f32).exp();
    for (i, value) in out.iter_mut().enumerate() {
        let ms = i as f32 * 1000.0 / SAMPLE_RATE as f32;
        if position >= (pcm.len() - 1) as f64 {
            let Some(lp) = lp.filter(|&lp| lp < pcm.len() - 1) else {
                break;
            };
            position = lp as f64 + (position - lp as f64) % (pcm.len() - lp - 1) as f64;
        }
        let n = position as usize;
        let f = (position - n as f64) as f32;
        let mut v = (pcm[n] * (1.0 - f) + pcm[n + 1] * f) / norm;
        if let Some(hz) = high {
            high_state += coefficient(hz) * (v - high_state);
            v -= high_state;
        }
        if let Some(hz) = low {
            low_state += coefficient(hz) * (v - low_state);
            v = low_state;
        }
        let attack = if voice.attack_ms > 0.0 {
            (ms / voice.attack_ms).min(1.0)
        } else {
            1.0
        };
        let fade = ((voice.decay_ms + 10.0 - ms) / 10.0).clamp(0.0, 1.0);
        *value = v * attack * (-std::f32::consts::LN_10 * 3.0 * ms / voice.decay_ms).exp() * fade;
        let drop = if voice.pitch_drop_ms > 0.0 {
            voice.pitch_drop * (1.0 - ms / voice.pitch_drop_ms).max(0.0)
        } else {
            0.0
        };
        position +=
            f64::from(bench.sample_rate()) / SAMPLE_RATE as f64 * 2.0f64.powf(f64::from(pitch + root + drop) / 12.0);
    }
    out
}

/// A named lane with measured source-selection provenance.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct KitPiece {
    /// GM-like MIDI key used by a drum grid.
    pub key: u8,
    /// Editor-visible lane name.
    pub name: String,
    /// Processing recipe referencing the user's sample.
    pub voice: DrumVoice,
    /// Why this sample was selected and which checks passed.
    pub reason: String,
}

impl KitPiece {
    /// Creates an editable lane; song validation checks key uniqueness and DSP bounds.
    pub fn new(
        key: u8,
        name: impl Into<String>,
        voice: DrumVoice,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            key,
            name: name.into(),
            voice,
            reason: reason.into(),
        }
    }
}

/// A sample-derived drum map. Unsupported pieces are omitted with an explanation.
#[derive(Debug, Clone, PartialEq, Default)]
#[non_exhaustive]
pub struct Kit {
    /// Available named MIDI lanes, ordered by key.
    pub pieces: Vec<KitPiece>,
    /// Reasons a requested piece could not be derived honestly.
    pub omissions: Vec<String>,
}

/// Numerical checks for a 48 kHz drum render; energy ratios exclude DC.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct DrumMetrics {
    /// Fraction of spectral energy below 150 Hz.
    pub below_150_hz: f32,
    /// Fraction above 5 kHz.
    pub above_5khz: f32,
    /// Fraction above 2 kHz.
    pub above_2khz: f32,
    /// Strongest spectral bin between 100 and 500 Hz.
    pub body_peak_hz: f32,
    /// Last 5 ms RMS window above -30 dB relative to the peak RMS window.
    pub decay_30db_ms: f32,
    /// Allocated one-shot duration, including the final fade.
    pub duration_ms: f32,
    /// RMS level; zero indicates an unsupported or silent reference.
    pub rms: f32,
}

impl DrumMetrics {
    /// Measures energy with an unwindowed, zero-padded FFT over the complete one-shot.
    pub fn measure(pcm: &[f32]) -> Self {
        let mono: Vec<_> = pcm.chunks_exact(2).map(|c| (c[0] + c[1]) * 0.5).collect();
        let power = spectrum(&mono, 1 << 17);
        let hz = SAMPLE_RATE as f32 / ((power.len() - 1) * 2).max(1) as f32;
        let total = power.iter().skip(1).sum::<f32>().max(1e-20);
        let above = |f: f32| {
            power
                .iter()
                .enumerate()
                .filter(|(i, _)| *i as f32 * hz > f)
                .map(|(_, v)| *v)
                .sum::<f32>()
                / total
        };
        let peak = spectral_peak(&power, hz, 100.0, 500.0);
        let env: Vec<_> = mono
            .chunks(SAMPLE_RATE / 200)
            .map(|c| (c.iter().map(|v| v * v).sum::<f32>() / c.len() as f32).sqrt())
            .collect();
        let max = env.iter().copied().fold(0.0f32, f32::max);
        let end = env
            .iter()
            .rposition(|&v| v > max * 10.0f32.powf(-30.0 / 20.0))
            .map_or(0, |i| i + 1);
        Self {
            below_150_hz: 1.0 - above(150.0),
            above_5khz: above(5000.0),
            above_2khz: above(2000.0),
            body_peak_hz: peak,
            decay_30db_ms: end as f32 * 5.0,
            duration_ms: mono.len() as f32 * 1000.0 / SAMPLE_RATE as f32,
            rms: (mono.iter().map(|v| v * v).sum::<f32>() / mono.len().max(1) as f32).sqrt(),
        }
    }
}

// Iterative radix-2 FFT, used only for source selection and numerical checks, never playback.
fn spectrum(pcm: &[f32], maximum: usize) -> Vec<f32> {
    let n = pcm.len().min(maximum).max(2).next_power_of_two();
    let mut re = vec![0.0f32; n];
    let mut im = vec![0.0f32; n];
    for (a, b) in re.iter_mut().zip(pcm) {
        *a = *b;
    }
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j ^= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut width = 2;
    while width <= n {
        let angle = -std::f32::consts::TAU / width as f32;
        let (wi, wr) = angle.sin_cos();
        for start in (0..n).step_by(width) {
            let (mut ar, mut ai) = (1.0, 0.0);
            for offset in 0..width / 2 {
                let a = start + offset;
                let b = a + width / 2;
                let br = ar * re[b] - ai * im[b];
                let bi = ar * im[b] + ai * re[b];
                re[b] = re[a] - br;
                im[b] = im[a] - bi;
                re[a] += br;
                im[a] += bi;
                (ar, ai) = (ar * wr - ai * wi, ar * wi + ai * wr);
            }
        }
        width *= 2;
    }
    (0..=n / 2).map(|i| re[i] * re[i] + im[i] * im[i]).collect()
}

fn spectral_peak(power: &[f32], hz: f32, low: f32, high: f32) -> f32 {
    power
        .iter()
        .enumerate()
        .filter(|(i, _)| (low..=high).contains(&(*i as f32 * hz)))
        .max_by(|a, b| a.1.total_cmp(b.1))
        .map_or(0.0, |(i, _)| i as f32 * hz)
}

struct Candidate {
    index: usize,
    peak: f32,
    brightness: f32,
    flatness: f32,
    start_ms: f32,
}

impl Kit {
    /// Chooses sources by measured spectrum, with no fixed BIOS sample/program indices.
    /// Kick/hat/snare recipes are retained only when their rendered numerical checks pass.
    /// All processing is our sampler DSP; source banks are documented in sound.md §2,
    /// ps2logo.md §5 and research/ps1_shell_scenes.md §2.
    pub fn derive(bench: &SoundBench) -> Self {
        let mut kit = Self::default();
        let candidates: Vec<_> = bench
            .samples
            .iter()
            .filter_map(|sample| {
                let (pcm, _) = bench.sample_pcm(sample.index)?;
                let energy = pcm.iter().map(|v| v * v).sum::<f32>();
                if pcm.len() < 32 || energy < 1e-10 {
                    return None;
                }
                // Start at the loudest short window, bypassing silence/slow recorded swells.
                let block = 512;
                let start = pcm
                    .chunks(block)
                    .enumerate()
                    .max_by(|a, b| {
                        let e = |c: &[f32]| c.iter().map(|v| v * v).sum::<f32>() / c.len() as f32;
                        e(a.1).total_cmp(&e(b.1))
                    })?
                    .0
                    * block;
                let power = spectrum(&pcm[start..], 32768);
                let hz = bench.sample_rate() as f32 / ((power.len() - 1) * 2) as f32;
                let total = power.iter().skip(1).sum::<f32>().max(1e-20);
                let brightness = power
                    .iter()
                    .enumerate()
                    .skip(1)
                    .map(|(i, &v)| v * i as f32 * hz)
                    .sum::<f32>()
                    / total;
                let band: Vec<_> = power
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| (2000.0..=16000.0).contains(&(*i as f32 * hz)))
                    .map(|(_, &v)| v.max(1e-20))
                    .collect();
                let mean = band.iter().sum::<f32>() / band.len().max(1) as f32;
                let flatness =
                    (band.iter().map(|v| v.ln()).sum::<f32>() / band.len().max(1) as f32).exp()
                        / mean.max(1e-20);
                Some(Candidate {
                    index: sample.index,
                    peak: spectral_peak(&power, hz, 30.0, 8000.0),
                    brightness,
                    flatness,
                    start_ms: start as f32 / (bench.sample_rate() as f32 / 1000.0),
                })
            })
            .collect();
        let dark = candidates
            .iter()
            .filter(|c| c.peak > 0.0)
            .min_by(|a, b| a.brightness.total_cmp(&b.brightness));
        let mallets: Vec<_> = bench
            .programs
            .iter()
            .filter(|p| {
                bench
                    .character(p.id)
                    .is_some_and(|c| c.style == crate::bench::Style::MalletsAndPlucks)
            })
            .flat_map(|p| p.tones.iter().map(|t| t.sample_index))
            .collect();
        let body = candidates
            .iter()
            .filter(|c| c.peak > 0.0 && mallets.contains(&c.index))
            .min_by(|a, b| a.flatness.total_cmp(&b.flatness))
            .or_else(|| {
                candidates
                    .iter()
                    // High-band flatness can reflect quantization noise in a dark chime;
                    // its low centroid still supplies a body. Final snare checks decide.
                    .filter(|c| c.peak > 0.0 && c.brightness < 1500.0)
                    .min_by(|a, b| a.flatness.total_cmp(&b.flatness))
            });
        let noise = candidates
            .iter()
            .filter(|c| c.flatness > 0.015 && c.brightness > 1500.0)
            .max_by(|a, b| (a.flatness * a.brightness).total_cmp(&(b.flatness * b.brightness)));
        let source = |c: &Candidate| SampleRef::sample(bench.bank, c.index);
        let make = |c: &Candidate| {
            let mut v = DrumVoice::new(source(c));
            v.start_ms = c.start_ms;
            v
        };
        if let Some(c) = dark {
            let mut v = make(c);
            v.pitch = (12.0 * (65.0 / c.peak).log2()).clamp(-96.0, -6.0);
            v.pitch_drop = 18.0;
            v.pitch_drop_ms = 25.0;
            v.decay_ms = 250.0;
            v.low_pass_hz = Some(100.0);
            v.high_pass_hz = Some(20.0);
            v.gain = 2.0;
            kit.offer(
                bench,
                36,
                "Kick",
                v,
                format!(
                    "darkest sample {}, centroid {:.0} Hz, peak {:.0} Hz",
                    c.index, c.brightness, c.peak
                ),
            );
        } else {
            kit.omissions
                .push("Kick: no non-silent tonal source".into());
        }
        if let Some(c) = noise {
            for (key, name, decay) in [
                (42, "Closed hat", 40.0),
                (46, "Open hat", 250.0),
                (39, "Clap", 100.0),
            ] {
                let mut v = make(c);
                v.decay_ms = decay;
                v.high_pass_hz = Some(if key == 39 { 1500.0 } else { 10000.0 });
                v.pitch = if key == 39 { 0.0 } else { 12.0 };
                v.gain = 2.0;
                v.attack_ms = 0.3;
                if key == 39 {
                    v.retriggers_ms = vec![12.0, 24.0];
                }
                kit.offer(
                    bench,
                    key,
                    name,
                    v,
                    format!(
                        "bright noise sample {}, centroid {:.0} Hz, flatness {:.3}",
                        c.index, c.brightness, c.flatness
                    ),
                );
            }
            if let Some(b) = body {
                let mut v = make(b);
                v.pitch = (12.0 * (220.0 / b.peak).log2()).clamp(-96.0, 96.0);
                v.low_pass_hz = Some(450.0);
                v.high_pass_hz = Some(100.0);
                v.decay_ms = 180.0;
                v.gain = 1.5;
                v.noise_mix = Some(NoiseLayer {
                    source: source(c),
                    start_ms: c.start_ms,
                    pitch: 0.0,
                    mix: 1.0,
                });
                kit.offer(
                    bench,
                    38,
                    "Snare",
                    v,
                    format!(
                        "tonal sample {} tuned to 220 Hz plus noise sample {} (flatness {:.3})",
                        b.index, c.index, c.flatness
                    ),
                );
            } else {
                kit.omissions
                    .push("Snare: noise exists but no tonal body".into());
            }
        } else {
            for name in ["Snare", "Closed hat", "Open hat", "Clap"] {
                kit.omissions.push(format!(
                    "{name}: no broadband noise source (flatness >0.015, centroid >1500 Hz)"
                ));
            }
        }
        if let Some(e) = bench.effects.iter().min_by_key(|e| {
            bench
                .sample(e.sample_index)
                .map_or(usize::MAX, |s| s.length)
        }) {
            let mut v = DrumVoice::new(SampleRef::sample(bench.bank, e.sample_index));
            v.start_ms = candidates
                .iter()
                .find(|c| c.index == e.sample_index)
                .map_or(0.0, |c| c.start_ms);
            v.decay_ms = 35.0;
            v.high_pass_hz = Some(500.0);
            kit.offer(
                bench,
                37,
                "Rim / click",
                v,
                format!("shortest effect {}, source sample {}", e.id, e.sample_index),
            );
        } else {
            kit.omissions
                .push("Rim / click: this bank has no OSD one-shot effects".into());
        }
        if let Some(c) = body {
            for (key, name, target) in [
                (45, "Low tom", 110.0),
                (47, "Mid tom", 147.0),
                (50, "High tom", 196.0),
            ] {
                let mut v = make(c);
                v.pitch = (12.0 * (target / c.peak).log2()).clamp(-96.0, 96.0);
                v.decay_ms = 220.0;
                v.low_pass_hz = Some(800.0);
                v.high_pass_hz = Some(40.0);
                kit.offer(
                    bench,
                    key,
                    name,
                    v,
                    format!(
                        "mallet/tonal sample {} (flatness {:.3}), tuned to {target:.0} Hz",
                        c.index, c.flatness
                    ),
                );
            }
        } else {
            for name in ["Low tom", "Mid tom", "High tom"] {
                kit.omissions
                    .push(format!("{name}: no mallet or low-content tonal body"));
            }
        }
        kit.pieces.sort_by_key(|p| p.key);
        kit
    }

    fn offer(&mut self, bench: &SoundBench, key: u8, name: &str, voice: DrumVoice, reason: String) {
        let Ok(pcm) = voice.render(bench) else {
            self.omissions.push(format!("{name}: invalid recipe"));
            return;
        };
        let m = DrumMetrics::measure(&pcm);
        let accepted = m.rms > 1e-5
            && match key {
                36 => m.below_150_hz > 0.5 && m.decay_30db_ms <= 400.0,
                42 => m.above_5khz > 0.5 && m.duration_ms < 80.0,
                38 => {
                    (150.0..=300.0).contains(&m.body_peak_hz)
                        && m.above_2khz > 0.05
                        && m.duration_ms < 300.0
                }
                // An open hat must actually sustain; allocated silence is not a long hat.
                46 => m.decay_30db_ms >= 80.0 && m.duration_ms <= 300.0,
                _ => true,
            };
        let numbers = format!("low150={:.1}%, high5k={:.1}%, high2k={:.1}%, body={:.0} Hz, -30dB={:.0} ms, length={:.0} ms, RMS={:.5}",
            m.below_150_hz * 100.0, m.above_5khz * 100.0, m.above_2khz * 100.0, m.body_peak_hz, m.decay_30db_ms, m.duration_ms, m.rms);
        if accepted {
            self.pieces.push(KitPiece {
                key,
                name: name.into(),
                voice,
                reason: format!("{reason}; {numbers}"),
            });
        } else {
            self.omissions.push(format!(
                "{name}: source failed checks ({reason}; {numbers})"
            ));
        }
    }

    /// Returns the recipe for a MIDI key, or none for an unsupported lane.
    pub fn piece(&self, key: u8) -> Option<&KitPiece> {
        self.pieces.iter().find(|p| p.key == key)
    }

    /// Human-readable choices, measurements and explicit omissions; contains no PCM.
    pub fn explain(&self) -> String {
        let mut text = String::new();
        for p in &self.pieces {
            let _ = writeln!(text, "{} {}: {}", p.key, p.name, p.reason);
        }
        for reason in &self.omissions {
            let _ = writeln!(text, "Omitted {reason}");
        }
        text
    }

    /// Writes each available piece to a separate WAV in the supplied directory.
    pub fn write_wavs(&self, bench: &SoundBench, dir: impl AsRef<Path>) -> Result<()> {
        std::fs::create_dir_all(dir.as_ref())?;
        for p in &self.pieces {
            sound::write_wav(
                dir.as_ref().join(format!("{}.wav", p.key)),
                &p.voice.render(bench)?,
                SAMPLE_RATE as u32,
            )?;
        }
        Ok(())
    }
}

/// Channel instrument reference and, for derived sounds, processing parameters only.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Instrument {
    /// Native MIDI program in the instrument reference's bank.
    Program(u8),
    /// One-shot effect ID in the instrument reference's bank, at its stored pitch.
    Effect(usize),
    /// One decoded sample, pitched around MIDI key 60 with its declared source rate.
    Sample(usize),
    /// One processed source, triggered by every key.
    Drum(DrumVoice),
    /// Named GM-like drum lanes, selected by each note's key.
    Kit(Kit),
}

impl Instrument {
    fn validate(&self) -> Result<()> {
        match self {
            Self::Program(p) if *p <= 127 => Ok(()),
            Self::Effect(i) | Self::Sample(i) if *i < MAX_ITEMS => Ok(()),
            Self::Drum(v) => v.validate(),
            Self::Kit(k) => {
                if k.pieces.len() > 128 || k.omissions.len() > 128 {
                    return Err(bad("kit exceeds lane budget"));
                }
                let mut used = [false; 128];
                for p in &k.pieces {
                    if p.key > 127 || used[usize::from(p.key)] {
                        return Err(bad("invalid or duplicate kit key"));
                    }
                    used[usize::from(p.key)] = true;
                    p.voice.validate()?;
                }
                Ok(())
            }
            _ => Err(bad("instrument reference out of range")),
        }
    }
}

/// A portable instrument binding: local content fingerprint plus a human bank label.
/// Legacy/default bindings resolve using `Song::bank`; no BIOS bytes or file paths are saved.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct InstrumentRef {
    /// Bank containing the program, effect or sample recipe.
    pub bank: BankKey,
    /// Native instrument or caller-created processing recipe within that bank.
    pub instrument: Instrument,
}

impl InstrumentRef {
    /// Binds an instrument to a bank fingerprint computed from the caller's files.
    pub fn new(bank: BankKey, instrument: Instrument) -> Self { Self { bank, instrument } }

    fn validate(&self) -> Result<()> {
        self.bank.validate()?;
        self.instrument.validate()
    }
}

impl From<Instrument> for InstrumentRef {
    fn from(instrument: Instrument) -> Self { Self::new(BankKey::default(), instrument) }
}

/// One note, measured in pattern ticks; note-off starts the bank tone's release.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct Note {
    /// Offset from the pattern's beginning.
    pub start: u32,
    /// Held ticks, at least one; a hold beyond the arrangement sustains into its tail.
    pub length: u32,
    /// MIDI key, 0..=127.
    pub key: u8,
    /// MIDI velocity, 1..=127.
    pub velocity: u8,
}

impl Note {
    /// Creates a note; [`Song::validate`] checks its ranges before rendering/saving.
    pub fn new(start: u32, length: u32, key: u8, velocity: u8) -> Self {
        Self {
            start,
            length,
            key,
            velocity,
        }
    }
}

/// A reusable note pattern. It plays once at each occupied bar cell and may span bars.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Pattern {
    /// Duration including any final silence; notes must end within it.
    pub length_ticks: u32,
    /// Notes in any order; scheduling sorts them by time.
    pub notes: Vec<Note>,
}

impl Pattern {
    /// Creates an empty pattern of the given duration.
    pub fn new(length_ticks: u32) -> Self {
        Self {
            length_ticks,
            notes: Vec::new(),
        }
    }
}

/// One instrument track and its pattern placements on the bar grid.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Channel {
    /// Editor-visible name.
    pub name: String,
    /// Program, one-shot effect, processed sample, or a key-mapped drum kit.
    pub instrument: InstrumentRef,
    /// Channel volume, 0..=127.
    pub volume: u8,
    /// Pan, 0..=127 (64 = centre), combined with each tone's pan.
    pub pan: u8,
    /// Reusable patterns, addressed by arrangement indices.
    pub patterns: Vec<Pattern>,
    /// One cell per bar: a pattern index or silence.
    pub arrangement: Vec<Option<usize>>,
}

impl Channel {
    /// Creates an empty, centred channel at full volume.
    pub fn new(name: impl Into<String>, program: u8) -> Self {
        Self {
            name: name.into(),
            instrument: Instrument::Program(program).into(),
            volume: 127,
            pan: 64,
            patterns: Vec::new(),
            arrangement: Vec::new(),
        }
    }

    /// Creates a track using an effect, drum recipe or key-mapped kit.
    pub fn with_instrument(name: impl Into<String>, instrument: impl Into<InstrumentRef>) -> Self {
        let mut c = Self::new(name, 0);
        c.instrument = instrument.into();
        c
    }
}

/// Song clock position at an output frame; bar, beat and tick-in-beat are zero-based.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct SongPosition {
    /// Absolute tick in the arrangement, including the release tail.
    pub tick: u32,
    /// Bar index.
    pub bar: u32,
    /// Beat within the bar.
    pub beat: u8,
    /// Tick within that beat.
    pub tick_in_beat: u16,
}

/// Position associated with one output block of interleaved stereo PCM.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct PositionBlock {
    /// Absolute frame range within the complete song render.
    pub frames: Range<usize>,
    /// Position at the first frame, respecting the PS2 driver's quantized clock.
    pub position: SongPosition,
}

/// Rendered audio and its playhead positions; frame indices count stereo pairs.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct SongRender {
    /// 48 kHz stereo audio.
    pub pcm: Vec<f32>,
    /// Contiguous blocks covering every output frame.
    pub positions: Vec<PositionBlock>,
}

/// A finite arrangement over program numbers, with optional editor playback metadata.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Song {
    /// Editor-visible song name.
    pub name: String,
    /// Constant tempo in beats per minute, 1..=960.
    pub bpm: u16,
    /// Grid resolution, 1..=65535 ticks per beat.
    pub ticks_per_beat: u16,
    /// Meter numerator, 1..=32 beats per bar.
    pub beats_per_bar: u8,
    /// Default binding and clock for legacy files; each channel carries its own bank key.
    pub bank: BankRef,
    /// Instrument tracks, independent of the SSsq writer's 16-channel limit.
    pub channels: Vec<Channel>,
    /// Editor loop range `[start, end)`; rendering/export plays the arrangement once.
    pub loop_bars: Option<(usize, usize)>,
    /// Sequence master volume, 0..=127; imports can retain OSDSYS's runtime gain.
    pub sequence_volume: u8,
    /// Release tail after the arrangement, in nominal ticks (not driver updates).
    pub tail_ticks: u32,
    /// Optional exact endpoint within the final bar, useful for imported music.
    pub end_tick: Option<u32>,
    /// Import losses/provenance, preserved in the text format for the editor to display.
    pub import_notes: Vec<String>,
}

impl Song {
    /// Creates an empty 120 BPM, 4/4 song on a 480-tick grid, without a release tail.
    pub fn new(name: impl Into<String>, bank: BankRef) -> Self {
        Self {
            name: name.into(),
            bpm: 120,
            ticks_per_beat: 480,
            beats_per_bar: 4,
            bank,
            channels: Vec::new(),
            loop_bars: None,
            sequence_volume: 127,
            tail_ticks: 0,
            end_tick: None,
            import_notes: Vec::new(),
        }
    }

    /// Number of explicit arrangement cells, including trailing silent bars.
    pub fn bars(&self) -> usize {
        self.channels
            .iter()
            .map(|c| c.arrangement.len())
            .max()
            .unwrap_or(0)
    }

    /// Ticks per bar; zero indicates an invalid clock.
    pub fn ticks_per_bar(&self) -> u32 {
        u32::from(self.ticks_per_beat) * u32::from(self.beats_per_bar)
    }

    fn end(&self) -> u32 {
        if let Some(end) = self.end_tick {
            return end;
        }
        let bar = self.ticks_per_bar();
        let mut end = (self.bars() as u32).saturating_mul(bar);
        for c in &self.channels {
            for (i, cell) in c.arrangement.iter().enumerate() {
                if let Some(p) = cell.and_then(|n| c.patterns.get(n)) {
                    end = end.max(
                        (i as u32)
                            .saturating_mul(bar)
                            .saturating_add(p.length_ticks),
                    );
                }
            }
        }
        end
    }

    fn nominal_frames(&self, ticks: u32) -> usize {
        // Integer conversion keeps the shell's 50/60 Hz field grid sample-exact;
        // changing the floating-point division order can otherwise lose one frame.
        (u64::from(ticks) * 60 * SAMPLE_RATE as u64
            / (u64::from(self.bpm) * u64::from(self.ticks_per_beat))) as usize
    }

    fn sample_tick(&self, tick: u32) -> Result<usize> {
        if self.bank == BankRef::Ps1 {
            return Ok(self.nominal_frames(tick));
        }
        let mut e = [event(EventKind::End, 0, tick)];
        sound::schedule_events(&mut e, self.ticks_per_beat as i64, self.bpm as i64)?;
        Ok((e[0].update as f64 / sound::UPDATES_PER_SECOND * SAMPLE_RATE as f64) as usize)
    }

    /// Validates references, MIDI ranges, arithmetic and finite rendering budgets.
    /// Bounds songs to 120 s (600 s for marked loops), 128 channels and 8192 items.
    pub fn validate(&self) -> Result<()> {
        if self.bpm == 0
            || self.bpm > 960
            || self.ticks_per_beat == 0
            || !(1..=32).contains(&self.beats_per_bar)
        {
            return Err(bad(
                "clock requires BPM 1..960, positive ticks/beat and beats/bar 1..32",
            ));
        }
        if self.sequence_volume > 127 || self.channels.len() > 128 || self.bars() > MAX_ITEMS {
            return Err(bad("volume or arrangement size is out of range"));
        }
        let mut items = 0usize;
        let mut placed = 0usize;
        let bar = self.ticks_per_bar();
        let grid_end = (self.bars() as u32)
            .checked_mul(bar)
            .ok_or_else(|| bad("bar grid tick overflow"))?;
        if self.end_tick.is_some_and(|e| e > grid_end) {
            return Err(bad("end tick exceeds the bar grid"));
        }
        let end = self.end();
        let hold_end = end
            .checked_add(self.tail_ticks)
            .ok_or_else(|| bad("tail tick overflow"))?;
        for (ci, c) in self.channels.iter().enumerate() {
            c.instrument.validate()?;
            if c.volume > 127 || c.pan > 127 || c.patterns.len() > MAX_ITEMS {
                return Err(bad(format!(
                    "channel {ci}: program, volume, pan or pattern count out of range"
                )));
            }
            items = items
                .saturating_add(c.arrangement.len())
                .saturating_add(c.patterns.len());
            for p in &c.patterns {
                items = items.saturating_add(p.notes.len());
                if p.length_ticks == 0 {
                    return Err(bad(format!("channel {ci}: zero pattern length")));
                }
                for n in &p.notes {
                    if n.key > 127
                        || n.velocity == 0
                        || n.velocity > 127
                        || n.length == 0
                        || n.start
                            .checked_add(n.length)
                            .is_none_or(|t| t > p.length_ticks)
                    {
                        return Err(bad(format!(
                            "channel {ci}: invalid note or note exceeds pattern"
                        )));
                    }
                }
            }
            for (i, cell) in c.arrangement.iter().enumerate() {
                let Some(id) = cell else { continue };
                let p = c
                    .patterns
                    .get(*id)
                    .ok_or_else(|| bad(format!("channel {ci}, bar {i}: missing pattern {id}")))?;
                placed = placed.saturating_add(p.notes.len());
                for n in &p.notes {
                    if (i as u32 * bar)
                        .checked_add(n.start)
                        .and_then(|t| t.checked_add(n.length))
                        .is_none_or(|t| t > hold_end)
                    {
                        return Err(bad(format!(
                            "channel {ci}, bar {i}: note exceeds song endpoint"
                        )));
                    }
                }
            }
        }
        if items > MAX_ITEMS || placed > MAX_ITEMS {
            return Err(bad("too many patterns, bars or notes"));
        }
        if self
            .loop_bars
            .is_some_and(|(a, b)| a >= b || b > self.bars())
        {
            return Err(bad("invalid exclusive loop bar range"));
        }
        let frames = self
            .sample_tick(end)?
            .checked_add(self.nominal_frames(self.tail_ticks))
            .ok_or_else(|| bad("duration overflow"))?;
        let max_seconds = if self.loop_bars.is_some() { 600.0 } else { MAX_SECONDS };
        if frames > (max_seconds * SAMPLE_RATE as f64) as usize {
            return Err(bad("render exceeds duration budget (120 s; 600 s for marked loops)"));
        }
        let text_bytes = self
            .name
            .len()
            .saturating_add(self.channels.iter().map(|c| c.name.len() + c.instrument.bank.label.len() + c.instrument.bank.fingerprint.len()).sum::<usize>())
            .saturating_add(self.import_notes.iter().map(String::len).sum::<usize>());
        let text_bytes = self.channels.iter().fold(text_bytes, |sum, c| {
            if let Instrument::Kit(k) = &c.instrument.instrument {
                k.pieces
                    .iter()
                    .fold(sum, |n, p| {
                        n.saturating_add(p.name.len())
                            .saturating_add(p.reason.len())
                    })
                    .saturating_add(k.omissions.iter().map(String::len).sum::<usize>())
            } else {
                sum
            }
        });
        if text_bytes > MAX_TEXT / 2 || self.import_notes.len() > MAX_ITEMS {
            return Err(bad("names or annotations exceed text budget"));
        }
        Ok(())
    }

    fn sequence(&self, end: u32) -> Result<(SoundSequence, Vec<sound::Channel>)> {
        let channels: Vec<_> = self
            .channels
            .iter()
            .map(|c| sound::Channel {
                program: match c.instrument.instrument {
                    Instrument::Program(p) => i32::from(p),
                    _ => 255,
                },
                volume: c.volume as i32,
                pan: c.pan as i32,
                bend: 64,
            })
            .collect();
        let mut events = Vec::new();
        for (ci, c) in self.channels.iter().enumerate() {
            if !matches!(c.instrument.instrument, Instrument::Program(_)) {
                continue;
            }
            for (bar, cell) in c.arrangement.iter().enumerate() {
                let Some(p) = cell.and_then(|n| c.patterns.get(n)) else {
                    continue;
                };
                for n in &p.notes {
                    let at = bar as u32 * self.ticks_per_bar() + n.start;
                    if at < end {
                        events.push(event(
                            EventKind::NoteOn {
                                note: n.key as i32,
                                velocity: n.velocity as i32,
                            },
                            ci,
                            at,
                        ))
                    }
                    if at + n.length <= end {
                        events.push(event(
                            EventKind::NoteOff { note: n.key as i32 },
                            ci,
                            at + n.length,
                        ))
                    }
                }
            }
        }
        // Key-offs precede re-triggers of the same key at an equal tick.
        events.sort_by_key(|e| (e.tick, matches!(e.kind, EventKind::NoteOn { .. })));
        events.push(event(EventKind::End, 0, end));
        if self.bank != BankRef::Ps1 {
            sound::schedule_events(&mut events, self.ticks_per_beat as i64, self.bpm as i64)?;
        } else {
            for e in &mut events {
                e.update = self.nominal_frames(e.tick as u32) as i64
            }
        }
        let mut initial = [sound::Channel {
            program: 255,
            volume: 127,
            pan: 64,
            bend: 64,
        }; 16];
        for (a, b) in initial.iter_mut().zip(&channels) {
            *a = *b
        }
        Ok((
            SoundSequence {
                volume: self.sequence_volume as i32,
                resolution: self.ticks_per_beat as i64,
                bpm: self.bpm as i64,
                channels: initial,
                events,
            },
            channels,
        ))
    }

    /// Renders one pass plus the explicit tail, as 48 kHz interleaved stereo.
    /// PS2 on/off times use the driver's 20.12/60 Hz clock (sound.md §2.2); PS1 uses
    /// the grid's exact sample clock. Invalid songs return empty; absent programs are silent.
    /// Peaks above unity scale the whole mix uniformly, preserving stereo balance.
    pub fn render(&self, bench: &(impl BankResolver + ?Sized)) -> Vec<f32> {
        self.render_checked(bench).unwrap_or_default()
    }

    /// Renders like [`Self::render`], reporting invalid models with [`Error::Corrupt`].
    pub fn render_checked(&self, bench: &(impl BankResolver + ?Sized)) -> Result<Vec<f32>> {
        self.validate()?;
        let end = self.end();
        let (seq, channels) = self.sequence(end + self.tail_ticks)?;
        let frames = self.sample_tick(end)? + self.nominal_frames(self.tail_ticks);
        let mut mix = self.mix_banks(bench, &seq, &channels, frames)?;
        limit(&mut mix);
        Ok(mix)
    }

    /// Unresolved dependencies, preserving the song's human labels for the editor.
    pub fn missing_banks(&self, resolver: &(impl BankResolver + ?Sized)) -> Vec<BankKey> {
        let mut missing = Vec::new();
        for c in &self.channels {
            if resolver.resolve(&c.instrument.bank, self.bank).is_none()
                && !missing.contains(&c.instrument.bank) {
                let mut key = c.instrument.bank.clone();
                if key.is_default() { key.label = BankKey::legacy_label(self.bank).into(); }
                missing.push(key);
            }
        }
        missing
    }

    fn mix_banks(&self, resolver: &(impl BankResolver + ?Sized), seq: &SoundSequence,
        channels: &[sound::Channel], frames: usize) -> Result<Vec<f32>> {
        let mut mix = vec![0.0; frames * 2];
        let mut benches: Vec<&SoundBench> = Vec::new();
        for c in &self.channels {
            if let Some(resolved) = resolver.resolve(&c.instrument.bank, self.bank) {
                if !benches.iter().any(|b| std::ptr::eq(*b, resolved.bench)) { benches.push(resolved.bench); }
            }
        }
        for bench in benches {
            let belongs = |i: usize| self.channels.get(i).and_then(|c| resolver.resolve(&c.instrument.bank, self.bank))
                .is_some_and(|b| std::ptr::eq(b.bench, bench));
            let mut part = seq.clone();
            part.events.retain(|e| e.kind == EventKind::End || belongs(e.channel));
            let ps1 = bench.bank == BankRef::Ps1;
            if ps1 {
                for e in &mut part.events { e.update = self.sample_tick(e.tick as u32)? as i64; }
            } else if self.bank == BankRef::Ps1 {
                sound::schedule_events(&mut part.events, part.resolution, part.bpm)?;
            }
            let selected: Vec<_> = channels.iter().enumerate().map(|(i, c)| {
                if belongs(i) { *c } else { sound::Channel { program: 255, volume: 0, ..*c } }
            }).collect();
            // One native renderer call per bank keeps single-bank accumulation bit-identical.
            let pcm = bench.render_song(&part, &selected, frames, ps1);
            for (a, b) in mix.iter_mut().zip(pcm) { *a += b; }
        }
        self.mix_one_shots(resolver, &mut mix)?;
        Ok(mix)
    }

    fn mix_one_shots(&self, resolver: &(impl BankResolver + ?Sized), mix: &mut [f32]) -> Result<()> {
        for c in &self.channels {
            let Some(resolved) = resolver.resolve(&c.instrument.bank, self.bank) else { continue };
            let bench = resolved.bench;
            if matches!(c.instrument.instrument, Instrument::Program(_)) {
                continue;
            }
            let mut cache = std::collections::BTreeMap::<u8, Vec<f32>>::new();
            for (bar, cell) in c.arrangement.iter().enumerate() {
                let Some(p) = cell.and_then(|i| c.patterns.get(i)) else {
                    continue;
                };
                for n in &p.notes {
                    let tick = bar as u32 * self.ticks_per_bar() + n.start;
                    let at = self.sample_tick(tick)? * 2;
                    if at >= mix.len() {
                        continue;
                    }
                    let key = if matches!(c.instrument.instrument, Instrument::Kit(_) | Instrument::Sample(_)) {
                        n.key
                    } else {
                        0
                    };
                    if matches!(c.instrument.instrument, Instrument::Sample(_)) { cache.remove(&key); }
                    if let std::collections::btree_map::Entry::Vacant(entry) = cache.entry(key) {
                        let pcm = match &c.instrument.instrument {
                            Instrument::Effect(id) => {
                                let seconds = bench
                                    .effects
                                    .get(*id)
                                    .and_then(|e| {
                                        bench.sample(e.sample_index).map(|s| {
                                            (s.length as f64
                                                / 44100.0
                                                / (f64::from(e.pitch).max(1.0) / 4096.0)
                                                + 0.1)
                                                .clamp(0.1, 2.0)
                                        })
                                    })
                                    .unwrap_or(0.1);
                                bench.render_effect(*id, seconds)
                            }
                            Instrument::Drum(v) => v.render(bench)?,
                            Instrument::Kit(k) => k
                                .piece(n.key)
                                .map(|p| p.voice.render(bench))
                                .transpose()?
                                .unwrap_or_default(),
                            Instrument::Sample(id) => {
                                let seconds = f64::from(n.length) * 60.0 / f64::from(self.bpm) / f64::from(self.ticks_per_beat);
                                bench.render_sample(*id, n.key, 127, seconds)
                            }
                            Instrument::Program(_) => Vec::new(),
                        };
                        entry.insert(pcm);
                    }
                    let Some(pcm) = cache.get(&key) else {
                        continue;
                    };
                    let gain = f32::from(n.velocity) / 127.0 * f32::from(c.volume) / 127.0
                        * f32::from(self.sequence_volume)
                        / 127.0;
                    let pan = if c.pan <= 64 {
                        f32::from(c.pan) / 64.0 * std::f32::consts::FRAC_PI_4
                    } else {
                        std::f32::consts::FRAC_PI_4
                            + f32::from(c.pan - 64) / 63.0 * std::f32::consts::FRAC_PI_4
                    };
                    let (right, left) = pan.sin_cos();
                    // Centre preserves the effect's original stereo balance and level.
                    let centre = std::f32::consts::FRAC_1_SQRT_2;
                    for (dst, src) in mix[at..].chunks_exact_mut(2).zip(pcm.chunks_exact(2)) {
                        dst[0] += src[0] * gain * left / centre;
                        dst[1] += src[1] * gain * right / centre;
                    }
                }
            }
        }
        Ok(())
    }

    /// Frame offset of a tick in the finite render, including its nominal release tail.
    /// Uses the same integer/driver clock as note scheduling (sound.md §2.2).
    pub fn frame_at_tick(&self, tick: u32) -> Result<usize> {
        self.validate()?;
        let end = self.end();
        let tick = tick.min(end + self.tail_ticks);
        if tick <= end {
            self.sample_tick(tick)
        } else {
            Ok(self.sample_tick(end)? + self.nominal_frames(tick - end))
        }
    }

    /// Renders from a tick with earlier voices/envelopes and whole-song limiting intact.
    /// This is exactly the tail of `render_checked`; it prerenders the bounded song.
    pub fn render_from(&self, bench: &(impl BankResolver + ?Sized), tick: u32) -> Result<Vec<f32>> {
        let first = self.frame_at_tick(tick)?;
        let pcm = self.render_checked(bench)?;
        Ok(pcm[first.min(pcm.len() / 2) * 2..].to_vec())
    }

    /// Returns bit-identical audio plus contiguous playhead blocks of `block_frames` frames.
    /// PS2 positions invert the driver's clock; the release tail uses nominal ticks.
    pub fn render_with_positions(
        &self,
        bench: &(impl BankResolver + ?Sized),
        block_frames: usize,
    ) -> Result<SongRender> {
        if block_frames == 0 {
            return Err(bad("position block size must be positive"));
        }
        let pcm = self.render_checked(bench)?;
        let frames = pcm.len() / 2;
        let end = self.end();
        let end_frame = self.sample_tick(end)?;
        let frame = |tick: u32| -> Result<usize> {
            if tick <= end {
                self.sample_tick(tick)
            } else {
                Ok(end_frame + self.nominal_frames(tick - end))
            }
        };
        let mut positions = Vec::new();
        for first in (0..frames).step_by(block_frames) {
            let (mut low, mut high) = (0, end + self.tail_ticks);
            while low < high {
                let mid = low + (high - low).div_ceil(2);
                if frame(mid)? <= first {
                    low = mid;
                } else {
                    high = mid - 1;
                }
            }
            positions.push(PositionBlock {
                frames: first..first.saturating_add(block_frames).min(frames),
                position: SongPosition {
                    tick: low,
                    bar: low / self.ticks_per_bar(),
                    beat: ((low / u32::from(self.ticks_per_beat)) % u32::from(self.beats_per_bar))
                        as u8,
                    tick_in_beat: (low % u32::from(self.ticks_per_beat)) as u16,
                },
            });
        }
        Ok(SongRender { pcm, positions })
    }

    /// Previews `[start, end)` bars, preserving earlier notes/releases but omitting the
    /// final tail. Only audio up to the requested endpoint is synthesised; invalid ranges
    /// return empty. A multi-bar pattern can continue past its channel's last explicit cell.
    pub fn render_bar_range(&self, bench: &(impl BankResolver + ?Sized), bars: Range<usize>) -> Vec<f32> {
        if self.validate().is_err() || bars.start >= bars.end || bars.end > MAX_ITEMS {
            return Vec::new();
        }
        let start = (bars.start as u32)
            .saturating_mul(self.ticks_per_bar())
            .min(self.end());
        let end = (bars.end as u32)
            .saturating_mul(self.ticks_per_bar())
            .min(self.end());
        let (Ok(first), Ok(last), Ok((seq, channels))) = (
            self.sample_tick(start),
            self.sample_tick(end),
            self.sequence(end),
        ) else {
            return Vec::new();
        };
        if first >= last {
            return Vec::new();
        }
        let Ok(mix) = self.mix_banks(bench, &seq, &channels, last) else { return Vec::new() };
        let mut preview = mix[first * 2..last * 2].to_vec();
        limit(&mut preview);
        preview
    }

    /// Imports SSsq ticks onto the same integer grid, one track per used MIDI channel.
    /// Keeps the initial tempo/resolution; subsequent tempo, instrument/controller changes,
    /// bends and unclosed notes are recorded in [`Self::import_notes`]. State at the first
    /// note becomes the track state. Tempo changes are intentionally flattened, not hidden.
    /// `volume` is the runtime sequence gain (boot: 0x42; sound.md §§1.3, 3.3).
    pub fn from_sequence(seq: &SoundSequence, name: impl Into<String>, volume: u8) -> Result<Self> {
        let mut song = Self::new(name, BankRef::Ps2);
        song.bpm = u16::try_from(seq.bpm).map_err(|_| bad("import tempo out of range"))?;
        song.ticks_per_beat =
            u16::try_from(seq.resolution).map_err(|_| bad("import resolution out of range"))?;
        song.sequence_volume = volume;
        if song.bpm == 0 || song.ticks_per_beat == 0 || seq.events.len() > MAX_ITEMS * 4 {
            return Err(bad("invalid import clock or event count"));
        }
        let mut state = seq.channels;
        let mut chosen: [Option<sound::Channel>; 16] = [None; 16];
        let mut raw: [Vec<Note>; 16] = std::array::from_fn(|_| Vec::new());
        let mut active: Vec<(usize, usize)> = Vec::new();
        let mut last = 0u32;
        let mut tempo = seq.bpm;
        for e in &seq.events {
            let at = u32::try_from(e.tick).map_err(|_| bad("negative or oversized import tick"))?;
            if at < last || e.channel >= 16 {
                return Err(bad("unordered ticks or invalid import channel"));
            }
            last = at;
            let c = &mut state[e.channel];
            match e.kind {
                EventKind::Program(p) => {
                    if chosen[e.channel].is_some() && c.program != p {
                        song.import_notes.push(format!(
                            "Tick {at}: channel {} program change to {p} omitted",
                            e.channel
                        ));
                    }
                    c.program = p;
                    c.bend = 64
                }
                EventKind::Control(cc @ (7 | 10), v) => {
                    let value = if cc == 7 { &mut c.volume } else { &mut c.pan };
                    if chosen[e.channel].is_some() && *value != v {
                        song.import_notes.push(format!(
                            "Tick {at}: channel {} controller {cc}={v} change omitted",
                            e.channel
                        ));
                    }
                    *value = v;
                }
                EventKind::Bend(v) => {
                    c.bend = v;
                    if v != 64 {
                        song.import_notes.push(format!(
                            "Tick {at}: channel {} pitch bend {v} omitted",
                            e.channel
                        ))
                    }
                }
                EventKind::Tempo(t) => {
                    if i64::from(t) != tempo {
                        song.import_notes.push(format!(
                            "Tick {at}: tempo {t} BPM flattened to {} BPM",
                            seq.bpm
                        ));
                    }
                    tempo = i64::from(t);
                }
                EventKind::Control(cc, v) => song.import_notes.push(format!(
                    "Tick {at}: channel {} controller {cc}={v} omitted",
                    e.channel
                )),
                EventKind::NoteOn { note, velocity } => {
                    let key = midi(note, "key")?;
                    let velocity = midi(velocity, "velocity")?;
                    if velocity == 0 {
                        return Err(bad("zero-velocity NoteOn in decoded sequence"));
                    }
                    if let Some(first) = chosen[e.channel] {
                        if (first.program, first.volume, first.pan, first.bend)
                            != (c.program, c.volume, c.pan, c.bend)
                        {
                            song.import_notes.push(format!("Tick {at}: channel {} state change omitted (program {}, volume {}, pan {}, bend {})", e.channel, c.program, c.volume, c.pan, c.bend));
                        }
                    } else {
                        if c.bend != 64 {
                            song.import_notes.push(format!(
                                "Tick {at}: channel {} initial pitch bend {} omitted",
                                e.channel, c.bend
                            ));
                        }
                        chosen[e.channel] = Some(*c)
                    }
                    raw[e.channel].push(Note::new(at, 0, key, velocity));
                    active.push((e.channel, raw[e.channel].len() - 1));
                }
                EventKind::NoteOff { note } => {
                    midi(note, "key")?;
                    active.retain(|&(ch, i)| {
                        let n = &mut raw[ch][i];
                        if ch == e.channel && i32::from(n.key) == note {
                            n.length = at.saturating_sub(n.start);
                            false
                        } else {
                            true
                        }
                    });
                }
                EventKind::End => break,
            }
        }
        song.tail_ticks = u32::from(song.ticks_per_beat) * u32::from(song.bpm) / 20; // Three nominal seconds.
        let hold_end = last
            .checked_add(song.tail_ticks)
            .ok_or_else(|| bad("import tail overflow"))?;
        for (ch, i) in active {
            let n = &mut raw[ch][i];
            n.length = hold_end.saturating_sub(n.start);
            song.import_notes.push(format!(
                "Channel {ch} key {}: missing note-off held through render tail",
                n.key
            ));
        }
        song.end_tick = Some(last);
        // [V] NRPN 20/30 bracket the infinite ambience loops; sound_inventory.md,
        // "Every stored score, its bank, and its role". Align the bar grid to both
        // markers rather than discarding the intro or rounding away half a bar.
        let mut loop_start = None;
        let mut loop_end = None;
        for e in &seq.events {
            if let EventKind::Control(99, marker @ (20 | 30)) = e.kind {
                let tick = u32::try_from(e.tick).map_err(|_| bad("invalid loop marker"))?;
                if marker == 20 { loop_start = Some(tick); } else { loop_end = Some(tick); }
            }
        }
        if let (Some(a), Some(b)) = (loop_start, loop_end) {
            if a >= b || b > last { return Err(bad("invalid NRPN loop region")); }
            song.beats_per_bar = (1..=4).rev().find(|&beats| {
                let bar = u32::from(song.ticks_per_beat) * u32::from(beats);
                a % bar == 0 && b % bar == 0
            }).ok_or_else(|| bad("NRPN loop markers do not fit the beat grid"))?;
            let bar = song.ticks_per_bar();
            song.loop_bars = Some(((a / bar) as usize, (b / bar) as usize));
            song.import_notes.push("NRPN loop retained; render plays the intro and one loop traversal".into());
        }
        for (ch, notes) in raw.into_iter().enumerate() {
            let Some(c) = chosen[ch] else { continue };
            let mut channel =
                Channel::new(format!("MIDI channel {ch}"), midi(c.program, "program")?);
            channel.volume = midi(c.volume, "volume")?;
            channel.pan = midi(c.pan, "pan")?;
            song.place_import(channel, notes, last)?;
        }
        if song.channels.is_empty() && last != 0 {
            song.place_import(Channel::new("Silence", 0), Vec::new(), last)?;
        }
        song.validate()?;
        Ok(song)
    }

    fn place_import(&mut self, mut channel: Channel, notes: Vec<Note>, end: u32) -> Result<()> {
        let bar = self.ticks_per_bar();
        let bars = end.div_ceil(bar) as usize;
        if bars > MAX_ITEMS {
            return Err(bad("import has too many bars"));
        }
        channel.arrangement.resize(bars, None);
        for mut n in notes {
            if n.length == 0 {
                self.import_notes.push(format!(
                    "Key {} at tick {}: zero-length note omitted",
                    n.key, n.start
                ));
                continue;
            }
            let b = (n.start / bar) as usize;
            let cell = channel
                .arrangement
                .get_mut(b)
                .ok_or_else(|| bad("import note starts at or after endpoint"))?;
            let id = match *cell {
                Some(id) => id,
                None => {
                    let id = channel.patterns.len();
                    channel.patterns.push(Pattern::new(bar));
                    *cell = Some(id);
                    id
                }
            };
            n.start %= bar;
            let p = &mut channel.patterns[id];
            p.length_ticks = p.length_ticks.max(
                n.start
                    .checked_add(n.length)
                    .ok_or_else(|| bad("import note overflow"))?,
            );
            p.notes.push(n);
        }
        self.channels.push(channel);
        Ok(())
    }

    /// Imports a PS1 note table at its nominal 60 ticks/second (scenes §2). For an
    /// intro's sweep/clock jump or a licence fade, use the scene-specific helpers below.
    /// A channel is created for each `(program, pan)` pair; note-off matches program/key.
    pub fn from_ps1_note_table(events: &[NoteEvent], name: impl Into<String>) -> Result<Self> {
        Self::ps1_at_clock(events, &name.into(), 60, 240)
    }

    fn ps1_at_clock(events: &[NoteEvent], name: &str, clock: u16, tail: u32) -> Result<Self> {
        let mut song = Self::new(name, BankRef::Ps1);
        song.bpm = 60;
        song.ticks_per_beat = clock;
        if events.len() > MAX_ITEMS {
            return Err(bad("too many PS1 events"));
        }
        let mut groups: Vec<(u8, u8, Vec<Note>)> = Vec::new();
        let mut active: Vec<(usize, usize)> = Vec::new();
        let mut ordered = events.to_vec();
        ordered.sort_by_key(|e| e.time);
        let end = ordered.last().map_or(0, |e| e.time);
        for e in ordered {
            if e.prog > 127 || e.note > 127 || e.vel > 127 || e.pan > 127 {
                return Err(bad("PS1 event out of MIDI range"));
            }
            if e.vel == 0 {
                active.retain(|&(g, i)| {
                    let (prog, _, notes) = &mut groups[g];
                    let n = &mut notes[i];
                    if *prog == e.prog && n.key == e.note {
                        n.length = e.time - n.start;
                        false
                    } else {
                        true
                    }
                });
            } else {
                let g = groups
                    .iter()
                    .position(|&(p, pan, _)| (p, pan) == (e.prog, e.pan))
                    .unwrap_or_else(|| {
                        groups.push((e.prog, e.pan, Vec::new()));
                        groups.len() - 1
                    });
                groups[g].2.push(Note::new(e.time, 0, e.note, e.vel));
                active.push((g, groups[g].2.len() - 1));
            }
        }
        let hold_end = end
            .checked_add(tail)
            .ok_or_else(|| bad("PS1 tail overflow"))?;
        for (g, i) in active {
            let prog = groups[g].0;
            let n = &mut groups[g].2[i];
            n.length = hold_end.saturating_sub(n.start);
            song.import_notes.push(format!(
                "Program {prog} key {}: missing note-off held through render tail",
                n.key
            ));
        }
        song.end_tick = Some(end);
        song.tail_ticks = tail;
        for (prog, pan, notes) in groups {
            let mut c = Channel::new(format!("Program {prog}, pan {pan}"), prog);
            c.pan = pan;
            song.place_import(c, notes, end)?;
        }
        song.validate()?;
        Ok(song)
    }

    /// Imports the licence chime including the two fade drones (ps1_boot.md §6).
    /// Timing mirrors [`Ps1Shell::render_sound`], including the PAL 5/6 scaling.
    pub fn from_ps1_licence(shell: &Ps1Shell, video: VideoMode) -> Result<Self> {
        // A 300 Hz grid represents fractional PAL fields. Quantise the existing
        // shell schedule, sharing the fade's setup rather than repeating its literals.
        let events = shell
            .scheduled_sound_events(video)
            .into_iter()
            .map(|(sample, e)| {
                let ticks = (sample as u64)
                    .checked_mul(300)
                    .and_then(|n| n.checked_add(SAMPLE_RATE as u64 / 2))
                    .ok_or_else(|| bad("licence clock overflow"))?
                    / SAMPLE_RATE as u64;
                let time = u32::try_from(ticks).map_err(|_| bad("licence clock overflow"))?;
                Ok(NoteEvent { time, ..e })
            })
            .collect::<Result<Vec<_>>>()?;
        Self::ps1_at_clock(&events, "PS1 licence chime", 300, 1200)
    }

    /// Imports the SCE intro's field schedule, including its sweep and transition releases
    /// (`ps1_shell_scenes.md` §§1a, 2), using the intro's effective video mode.
    pub fn from_ps1_intro(intro: &SceIntro, video: VideoMode) -> Result<Self> {
        let timeline = intro.timeline(video);
        let events: Vec<_> = intro
            .scheduled_events(video)
            .into_iter()
            .map(|(field, e)| {
                u32::try_from(field)
                    .map(|time| NoteEvent { time, ..e })
                    .map_err(|_| bad("intro field overflow"))
            })
            .collect::<Result<_>>()?;
        Self::ps1_at_clock(&events, "PS1 SCE intro", timeline.video.fps() as u16, 0)
    }

    /// Imports the PS2 boot sequence from SNDIMAGE or the content-located 1.00 J bank.
    /// Runtime gain 0x42 follows `sound.md` §1.3; all music bytes come from `bios`.
    pub fn from_ps2_boot(bios: &RomDir) -> Result<Self> {
        let seq = match bios.module("SNDIMAGE") {
            Ok(module) => {
                let archive = RomDir::new(module.to_vec())?;
                SoundSequence::new(&unpack(archive.module("SNDBOOTS")?, 0)?)?
            }
            Err(Error::NotFound(..)) if bios.module("OSOPEN").is_ok() => {
                sound::embedded_boot_sounds(bios)?.1[0].clone()
            }
            Err(e) => return Err(e),
        };
        Self::from_sequence(&seq, "PS2 boot music", 0x42)
    }

    /// Exports this song's finite render as hand-written 16-bit stereo PCM WAV.
    pub fn write_wav(&self, bench: &(impl BankResolver + ?Sized), path: impl AsRef<Path>) -> Result<()> {
        sound::write_wav(path, &self.render_checked(bench)?, SAMPLE_RATE as u32)
    }

    /// Writes a finite PS2 arrangement as SSsq (sound.md §3.3), without audio data.
    /// Requires at most 16 channels; editor loops and holds into the tail are rejected.
    /// Tail, meter,
    /// names and annotations have no SSsq representation and are omitted.
    pub fn to_sssq(&self) -> Result<Vec<u8>> {
        self.validate()?;
        if self.bank != BankRef::Ps2
            || self.channels.len() > 16
            || self.loop_bars.is_some()
            || self
                .channels
                .iter()
                .any(|c| !matches!(c.instrument.instrument, Instrument::Program(_)))
        {
            return Err(bad(
                "SSsq export requires PS2, at most 16 channels and no editor loop",
            ));
        }
        if self.channels.windows(2).any(|c| c[0].instrument.bank != c[1].instrument.bank) {
            return Err(bad("SSsq export cannot retain multiple bank bindings"));
        }
        for c in &self.channels {
            for (bar, cell) in c.arrangement.iter().enumerate() {
                if let Some(p) = cell.and_then(|i| c.patterns.get(i)) {
                    if p.notes.iter().any(|n| {
                        bar as u32 * self.ticks_per_bar() + n.start + n.length > self.end()
                    }) {
                        return Err(bad("SSsq export cannot retain a hold into the render tail"));
                    }
                }
            }
        }
        let (seq, _) = self.sequence(self.end())?;
        let mut data = vec![0u8; 0x110];
        data[0] = self.sequence_volume;
        data[2..4].copy_from_slice(&self.ticks_per_beat.to_le_bytes());
        data[4..6].copy_from_slice(&self.bpm.to_le_bytes());
        data[12..16].copy_from_slice(b"SSsq");
        for (i, c) in seq.channels.iter().enumerate() {
            let p = 0x10 + i * 16;
            data[p + 1] = i as u8;
            data[p + 2] = c.program as u8;
            data[p + 3] = c.volume as u8;
            data[p + 4] = c.pan as u8;
            data[p + 10] = 64;
        }
        // First event has no preceding delta: an inert tempo event supplies tick zero.
        data.extend_from_slice(&[0xFF, 0x51]);
        data.extend_from_slice(&self.bpm.to_le_bytes());
        let mut previous = 0i64;
        for e in &seq.events {
            vlq(&mut data, (e.tick - previous) as u32)?;
            previous = e.tick;
            match e.kind {
                EventKind::NoteOn { note, velocity } => {
                    data.extend_from_slice(&[0x90 | e.channel as u8, note as u8, velocity as u8])
                }
                EventKind::NoteOff { note } => {
                    data.extend_from_slice(&[0x80 | e.channel as u8, note as u8, 0])
                }
                EventKind::End => data.extend_from_slice(&[0xFF, 0x2F, 0]),
                _ => return Err(bad("unexpected arrangement event")),
            }
        }
        Ok(data)
    }

    /// Emits canonical, diffable text, retaining empty patterns and bar cells.
    /// Default program songs use version 1; default effects/drums/kits/logo use version 2.
    /// Explicit bank bindings and loose samples use version 3.
    pub fn to_text(&self) -> Result<String> {
        self.validate()?;
        let extended = self.bank == BankRef::Ps2Logo
            || self
                .channels
                .iter()
                .any(|c| !matches!(c.instrument.instrument, Instrument::Program(_)));
        let multi = self.channels.iter().any(|c| !c.instrument.bank.is_default() || matches!(c.instrument.instrument, Instrument::Sample(_)));
        let mut text = format!("psxsong {}\n", if multi { 3 } else if extended { 2 } else { 1 });
        let _ = writeln!(
            text,
            "name {}\nbank {}\nclock {} {} {}\nmix {} {} {}",
            quote(&self.name),
            bank_tag(self.bank),
            self.bpm,
            self.ticks_per_beat,
            self.beats_per_bar,
            self.sequence_volume,
            self.tail_ticks,
            self.end_tick.map_or_else(|| "-".into(), |v| v.to_string())
        );
        let _ = writeln!(
            text,
            "loop {}",
            self.loop_bars
                .map_or_else(|| "-".into(), |(a, b)| format!("{a} {b}"))
        );
        for note in &self.import_notes {
            let _ = writeln!(text, "annotation {}", quote(note));
        }
        for c in &self.channels {
            let _ = writeln!(
                text,
                "channel {} {} {} {}",
                quote(&c.name),
                instrument_tag(&c.instrument.instrument),
                c.volume,
                c.pan
            );
            if multi {
                let _ = writeln!(text, "binding {} {}", quote(&c.instrument.bank.fingerprint), quote(&c.instrument.bank.label));
            }
            write_instrument(&mut text, &c.instrument.instrument);
            text.push_str("arrangement");
            for cell in &c.arrangement {
                let _ = write!(
                    text,
                    " {}",
                    cell.map_or_else(|| "-".into(), |n| n.to_string())
                );
            }
            text.push('\n');
            for p in &c.patterns {
                let _ = writeln!(text, "pattern {}", p.length_ticks);
                for n in &p.notes {
                    let _ = writeln!(
                        text,
                        "note {} {} {} {}",
                        n.start, n.length, n.key, n.velocity
                    );
                }
                text.push_str("endpattern\n");
            }
            text.push_str("endchannel\n");
        }
        if text.len() > MAX_TEXT {
            return Err(bad("serialized song exceeds text budget"));
        }
        Ok(text)
    }

    /// Parses version-1/2/3 text with line-numbered syntax errors and bounded allocation.
    /// Unknown/duplicate/missing records and dangling pattern references are errors.
    pub fn parse(text: &str) -> Result<Self> {
        parse_text(text)
    }
}

fn bad(message: impl Into<String>) -> Error {
    Error::Corrupt(Format::Sequence, format!("psxsong: {}", message.into()))
}

fn bank_tag(bank: BankRef) -> &'static str {
    match bank {
        BankRef::Ps2 => "ps2",
        BankRef::Ps1 => "ps1",
        BankRef::Ps2Logo => "logo",
    }
}

fn parse_bank(s: &str) -> Result<BankRef> {
    match s {
        "ps2" => Ok(BankRef::Ps2),
        "ps1" => Ok(BankRef::Ps1),
        "logo" => Ok(BankRef::Ps2Logo),
        _ => Err(bad("unknown source bank")),
    }
}

fn source_tag(s: SampleSource) -> String {
    match s {
        SampleSource::Sample(i) => format!("s{i}"),
        SampleSource::Program { program, key } => format!("p{program}:{key}"),
    }
}

fn parse_source(bank: &str, source: &str) -> Result<SampleRef> {
    let source = if let Some(i) = source.strip_prefix('s') {
        SampleSource::Sample(number(i)?)
    } else if let Some(p) = source.strip_prefix('p') {
        let (program, key) = p
            .split_once(':')
            .ok_or_else(|| bad("program source requires program:key"))?;
        SampleSource::Program {
            program: number(program)?,
            key: number(key)?,
        }
    } else {
        return Err(bad("source requires sample index or program:key"));
    };
    Ok(SampleRef {
        bank: parse_bank(bank)?,
        source,
    })
}

fn instrument_tag(instrument: &Instrument) -> String {
    match instrument {
        Instrument::Program(p) => p.to_string(),
        Instrument::Effect(i) => format!("effect:{i}"),
        Instrument::Sample(i) => format!("sample:{i}"),
        Instrument::Drum(_) => "drum".into(),
        Instrument::Kit(_) => "kit".into(),
    }
}

fn parse_instrument(s: &str, version: u8) -> Result<Instrument> {
    if version >= 2 {
        if version >= 3 {
            if let Some(i) = s.strip_prefix("sample:") { return Ok(Instrument::Sample(number(i)?)); }
        }
        if let Some(i) = s.strip_prefix("effect:") {
            return Ok(Instrument::Effect(number(i)?));
        }
        if s == "kit" {
            return Ok(Instrument::Kit(Kit::default()));
        }
        if s == "drum" {
            // An invalid placeholder makes a missing voice record fail final validation.
            return Ok(Instrument::Drum(DrumVoice::new(SampleRef::sample(
                BankRef::Ps2,
                MAX_ITEMS,
            ))));
        }
    }
    Ok(Instrument::Program(number(s)?))
}

fn write_instrument(text: &mut String, instrument: &Instrument) {
    match instrument {
        Instrument::Drum(v) => write_voice(text, "-", "", "", v),
        Instrument::Kit(k) => {
            for p in &k.pieces {
                write_voice(text, &p.key.to_string(), &p.name, &p.reason, &p.voice);
            }
            for s in &k.omissions {
                let _ = writeln!(text, "omission {}", quote(s));
            }
        }
        _ => {}
    }
}

fn write_voice(text: &mut String, key: &str, name: &str, reason: &str, v: &DrumVoice) {
    let optional = |n: Option<f32>| n.map_or_else(|| "-".into(), |n| n.to_string());
    let triggers = if v.retriggers_ms.is_empty() {
        "-".into()
    } else {
        v.retriggers_ms
            .iter()
            .map(f32::to_string)
            .collect::<Vec<_>>()
            .join(",")
    };
    let (bank, source, start, pitch, mix) =
        v.noise_mix
            .as_ref()
            .map_or(("-", "-".into(), 0.0, 0.0, 0.0), |n| {
                (
                    bank_tag(n.source.bank),
                    source_tag(n.source.source),
                    n.start_ms,
                    n.pitch,
                    n.mix,
                )
            });
    let _ = writeln!(text, "voice {key} {} {} {} {} {} {} {} {} {} {} {} {} {} {triggers} {bank} {source} {start} {pitch} {mix}",
        quote(name), quote(reason), bank_tag(v.source.bank), source_tag(v.source.source), v.start_ms,
        v.pitch, v.pitch_drop, v.pitch_drop_ms, v.decay_ms, v.attack_ms,
        optional(v.low_pass_hz), optional(v.high_pass_hz), v.gain);
}

fn read_voice(instrument: &mut Instrument, fields: &[&str]) -> Result<()> {
    let [key, name, reason, bank, source, start, pitch, drop, drop_ms, decay, attack, low, high, gain, triggers, noise_bank, noise_source, noise_start, noise_pitch, noise_mix] =
        fields
    else {
        return Err(bad("voice field count"));
    };
    let optional = |s: &str| -> Result<Option<f32>> {
        if s == "-" {
            Ok(None)
        } else {
            Ok(Some(number(s)?))
        }
    };
    let mut v = DrumVoice::new(parse_source(bank, source)?);
    v.start_ms = number(start)?;
    v.pitch = number(pitch)?;
    v.pitch_drop = number(drop)?;
    v.pitch_drop_ms = number(drop_ms)?;
    v.decay_ms = number(decay)?;
    v.attack_ms = number(attack)?;
    v.low_pass_hz = optional(low)?;
    v.high_pass_hz = optional(high)?;
    v.gain = number(gain)?;
    if *triggers != "-" {
        for t in triggers.split(',') {
            if v.retriggers_ms.len() >= 8 {
                return Err(bad("too many retriggers"));
            }
            v.retriggers_ms.push(number(t)?);
        }
    }
    if *noise_bank != "-" {
        v.noise_mix = Some(NoiseLayer {
            source: parse_source(noise_bank, noise_source)?,
            start_ms: number(noise_start)?,
            pitch: number(noise_pitch)?,
            mix: number(noise_mix)?,
        });
    } else if *noise_source != "-"
        || *noise_start != "0"
        || *noise_pitch != "0"
        || *noise_mix != "0"
    {
        return Err(bad("incomplete noise layer"));
    }
    v.validate()?;
    match instrument {
        Instrument::Drum(previous)
            if *key == "-"
                && name.is_empty()
                && reason.is_empty()
                && previous.source.source == SampleSource::Sample(MAX_ITEMS) =>
        {
            *previous = v
        }
        Instrument::Kit(k) if *key != "-" && k.pieces.len() < 128 => {
            k.pieces.push(KitPiece {
                key: number(key)?,
                name: (*name).into(),
                voice: v,
                reason: (*reason).into(),
            });
        }
        _ => return Err(bad("unexpected or duplicate drum voice")),
    }
    Ok(())
}
fn midi(v: i32, field: &str) -> Result<u8> {
    u8::try_from(v)
        .ok()
        .filter(|&n| n <= 127)
        .ok_or_else(|| bad(format!("{field} out of MIDI range")))
}
fn event(kind: EventKind, channel: usize, tick: u32) -> SequenceEvent {
    SequenceEvent {
        kind,
        channel,
        tick: i64::from(tick),
        update: 0,
    }
}
fn limit(mix: &mut [f32]) {
    let peak = mix.iter().fold(1.0f32, |a, b| a.max(b.abs()));
    if peak > 1.0 {
        for v in mix {
            *v /= peak
        }
    }
}
fn vlq(data: &mut Vec<u8>, mut n: u32) -> Result<()> {
    if n > 0x0FFF_FFFF {
        return Err(bad("SSsq delta exceeds four bytes"));
    }
    let mut bytes = [0u8; 4];
    let mut p = 3;
    bytes[p] = (n & 127) as u8;
    while n > 127 {
        n >>= 7;
        p -= 1;
        bytes[p] = (n as u8 & 127) | 128;
    }
    data.extend_from_slice(&bytes[p..]);
    Ok(())
}

fn quote(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '\"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

fn tokens(line: &str) -> Result<Vec<String>> {
    let mut chars = line.chars().peekable();
    let mut out = Vec::new();
    while let Some(c) = chars.next() {
        if c.is_whitespace() {
            continue;
        }
        let mut value = String::new();
        if c == '"' {
            let mut closed = false;
            while let Some(c) = chars.next() {
                match c {
                    '"' => {
                        closed = true;
                        break;
                    }
                    '\\' => value.push(match chars.next() {
                        Some('n') => '\n',
                        Some('r') => '\r',
                        Some('t') => '\t',
                        Some('"') => '"',
                        Some('\\') => '\\',
                        _ => return Err(bad("unknown or incomplete string escape")),
                    }),
                    _ => value.push(c),
                }
            }
            if !closed || chars.peek().is_some_and(|c| !c.is_whitespace()) {
                return Err(bad("unterminated string or missing separator"));
            }
        } else {
            value.push(c);
            while let Some(&c) = chars.peek() {
                if c.is_whitespace() {
                    break;
                }
                value.push(c);
                chars.next();
            }
        }
        out.push(value);
        if out.len() > MAX_ITEMS + 1 {
            return Err(bad("too many fields"));
        }
    }
    Ok(out)
}

fn number<T: std::str::FromStr>(s: &str) -> Result<T> {
    s.parse().map_err(|_| bad(format!("invalid number '{s}'")))
}

fn parse_text(text: &str) -> Result<Song> {
    if text.len() > MAX_TEXT {
        return Err(bad("text exceeds 4 MiB"));
    }
    let mut song = Song::new("", BankRef::Ps2);
    let mut version = 1;
    let mut stage = 0usize;
    let mut channel: Option<Channel> = None;
    let mut pattern: Option<Pattern> = None;
    let mut arranged = false;
    let mut bound = false;
    let mut count = 0usize;
    for (line_no, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let at = |e: Error| bad(format!("line {}: {e}", line_no + 1));
        let t = tokens(line).map_err(at)?;
        let fields: Vec<_> = t.iter().map(String::as_str).collect();
        count += 1;
        if count > MAX_ITEMS * 4 {
            return Err(at(bad("too many records")));
        }
        let result: Result<()> = (|| {
            match (stage, fields.as_slice()) {
                (0, ["psxsong", v @ ("1" | "2" | "3")]) => {
                    version = number(v)?;
                    stage = 1;
                }
                (1, ["name", name]) => {
                    song.name = (*name).into();
                    stage = 2
                }
                (2, ["bank", bank]) => {
                    song.bank = match *bank {
                        "ps1" => BankRef::Ps1,
                        "ps2" => BankRef::Ps2,
                        "logo" if version >= 2 => BankRef::Ps2Logo,
                        _ => return Err(bad("expected ps1 or ps2 bank")),
                    };
                    stage = 3
                }
                (3, ["clock", bpm, ticks, beats]) => {
                    song.bpm = number(bpm)?;
                    song.ticks_per_beat = number(ticks)?;
                    song.beats_per_bar = number(beats)?;
                    stage = 4
                }
                (4, ["mix", volume, tail, end]) => {
                    song.sequence_volume = number(volume)?;
                    song.tail_ticks = number(tail)?;
                    song.end_tick = if *end == "-" {
                        None
                    } else {
                        Some(number(end)?)
                    };
                    stage = 5
                }
                (5, ["loop", "-"]) => stage = 6,
                (5, ["loop", a, b]) => {
                    song.loop_bars = Some((number(a)?, number(b)?));
                    stage = 6
                }
                (6, ["annotation", note]) if channel.is_none() && song.channels.is_empty() => {
                    song.import_notes.push((*note).into())
                }
                (6, ["channel", name, program, volume, pan]) if channel.is_none() => {
                    let mut c = Channel::new(*name, 0);
                    c.instrument = parse_instrument(program, version)?.into();
                    c.volume = number(volume)?;
                    c.pan = number(pan)?;
                    channel = Some(c);
                    arranged = false;
                    bound = version < 3;
                }
                (6, ["binding", fingerprint, label]) if version == 3 && channel.is_some() && !bound && !arranged => {
                    let c = channel.as_mut().ok_or_else(|| bad("binding without channel"))?;
                    c.instrument.bank = BankKey::new(*fingerprint, *label);
                    c.instrument.bank.validate()?;
                    bound = true;
                }
                (6, ["voice", values @ ..])
                    if version >= 2 && channel.is_some() && !arranged && pattern.is_none() =>
                {
                    let c = channel
                        .as_mut()
                        .ok_or_else(|| bad("voice without channel"))?;
                    read_voice(&mut c.instrument.instrument, values)?;
                }
                (6, ["omission", reason]) if version >= 2 && !arranged && pattern.is_none() => {
                    if let Some(InstrumentRef { instrument: Instrument::Kit(k), .. }) = channel.as_mut().map(|c| &mut c.instrument) {
                        if k.omissions.len() >= 128 {
                            return Err(bad("too many kit omissions"));
                        }
                        k.omissions.push((*reason).into());
                    } else {
                        return Err(bad("omission requires a kit channel"));
                    }
                }
                (6, ["arrangement", cells @ ..]) if channel.is_some() && bound && !arranged => {
                    let c = channel
                        .as_mut()
                        .ok_or_else(|| bad("arrangement without channel"))?;
                    for cell in cells {
                        c.arrangement.push(if *cell == "-" {
                            None
                        } else {
                            Some(number(cell)?)
                        });
                    }
                    arranged = true;
                }
                (6, ["pattern", length]) if channel.is_some() && arranged && pattern.is_none() => {
                    pattern = Some(Pattern::new(number(length)?))
                }
                (6, ["note", start, length, key, velocity]) if pattern.is_some() => {
                    let p = pattern
                        .as_mut()
                        .ok_or_else(|| bad("note without pattern"))?;
                    p.notes.push(Note::new(
                        number(start)?,
                        number(length)?,
                        number(key)?,
                        number(velocity)?,
                    ));
                }
                (6, ["endpattern"]) if pattern.is_some() => {
                    let p = pattern.take().ok_or_else(|| bad("missing pattern"))?;
                    channel
                        .as_mut()
                        .ok_or_else(|| bad("pattern without channel"))?
                        .patterns
                        .push(p);
                }
                (6, ["endchannel"]) if channel.is_some() && arranged && pattern.is_none() => song
                    .channels
                    .push(channel.take().ok_or_else(|| bad("missing channel"))?),
                _ => return Err(bad("unexpected record, field count or version")),
            }
            Ok(())
        })();
        result.map_err(at)?;
    }
    if stage != 6 || channel.is_some() || pattern.is_some() {
        return Err(bad("incomplete header, channel or pattern at end of file"));
    }
    song.validate()?;
    Ok(song)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthetic_ps1_pairing_preserves_pan_and_unreleased_voices() {
        let key = NoteEvent {
            time: 0,
            prog: 0,
            note: 60,
            vel: 100,
            pan: 32,
        };
        let other = NoteEvent {
            time: 300,
            note: 64,
            pan: 96,
            ..key
        };
        let off = NoteEvent {
            time: 600,
            vel: 0,
            pan: 0,
            ..key
        };
        let song = Song::ps1_at_clock(&[key, other, off], "Table", 300, 1200).unwrap();
        assert_eq!(song.end_tick, Some(600));
        assert_eq!(song.channels.len(), 2);
        assert_eq!(song.channels[0].patterns[0].notes[0].length, 600);
        assert_eq!(song.channels[1].patterns[0].notes[0].length, 1500);
        assert!(song.import_notes[0].contains("missing note-off"));
        let mut invalid = off;
        invalid.time = u32::MAX;
        assert!(Song::from_ps1_note_table(&[key, invalid], "Overflow").is_err());
    }
}
