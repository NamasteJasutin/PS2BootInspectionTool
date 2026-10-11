//! SoundFont 2.04 export for sound benches (and caller-rendered percussion).
//!
//! Converts a [`SoundBench`] (and optional rendered percussion, [`Percussion`]) into a
//! standard SoundFont 2.04 (`.sf2`) binary file that any DAW or software sampler
//! (e.g. FluidSynth, Polyphone, Logic, Ableton) can load.
//!
//! Specifications & Research citations:
//! - SoundFont 2.04 Technical Specification:
//!   - §4–5: RIFF `sfbk` container and `INFO` sub-chunks (`ifil`, `isng`, `INAM`, `ICMT`, `ISFT`).
//!   - §6: `sdta` chunk and `smpl` 16-bit signed PCM audio pool with 46 zero-padding samples.
//!   - §7: `pdta` Hydra chunk (`phdr`, `pbag`, `pmod`, `pgen`, `inst`, `ibag`, `imod`, `igen`, `shdr`)
//!     and required terminal records (`EOP`, `EOI`, `EOS`).
//!   - §8.1.2: Generator parameters (`pan`, `initialAttenuation`, `overridingRootKey`, `fineTune`,
//!     `attackVolEnv`, `decayVolEnv`, `sustainVolEnv`, `releaseVolEnv`, `sampleModes`, `sampleID`).
//!   - §8.1.3: Generator zone order constraints (`keyRange` first, `sampleID` / `instrument` last).
//! - `notes/sound.md` §§2, 3: SPU ADSR register decoding and synthesis mathematics \[V\].
//! - `notes/research/sound_inventory.md`: bank identity, roles, and sample structures \[V\].

use crate::bench::SoundBench;
use crate::banks::BankRef;
use crate::{Error, Format, Result};

// SoundFont 2.04 generator operators (§8.1.2)
const GEN_PAN: u16 = 17;
const GEN_ATTACK_VOL_ENV: u16 = 34;
const GEN_DECAY_VOL_ENV: u16 = 36;
const GEN_SUSTAIN_VOL_ENV: u16 = 37;
const GEN_RELEASE_VOL_ENV: u16 = 38;
const GEN_INSTRUMENT: u16 = 41;
const GEN_KEY_RANGE: u16 = 43;
const GEN_INITIAL_ATTENUATION: u16 = 48;
const GEN_FINE_TUNE: u16 = 52;
const GEN_SAMPLE_ID: u16 = 53;
const GEN_SAMPLE_MODES: u16 = 54;
const GEN_OVERRIDING_ROOT_KEY: u16 = 58;

// SFSampleLink sample type flags (§7.10)
const SAMPLE_TYPE_MONO: u16 = 1;

/// SoundFont 2 volume envelope parameters derived from PlayStation SPU ADSR registers.
///
/// Converts the hardware SPU ADSR registers (`adsr1`, `adsr2`) into SF2 volume envelope
/// generators (Spec §8.1.2):
/// - `attack_tc`: Attack time in timecents ($1200 \log_2(\text{seconds})$).
/// - `decay_tc`: Decay time in timecents for a 100 dB drop.
/// - `sustain_cb`: Sustain attenuation in centibels (0.1 dB) below full scale.
/// - `release_tc`: Release time in timecents for a 100 dB drop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct VolEnv {
    /// Attack duration in absolute timecents (`attackVolEnv`, Gen 34).
    pub attack_tc: i16,
    /// Decay duration in absolute timecents (`decayVolEnv`, Gen 36).
    pub decay_tc: i16,
    /// Sustain attenuation in centibels (`sustainVolEnv`, Gen 37). 0 = full level.
    pub sustain_cb: u16,
    /// Release duration in absolute timecents (`releaseVolEnv`, Gen 38).
    pub release_tc: i16,
}

/// Converts PlayStation SPU ADSR register values into SoundFont 2 volume envelope parameters.
///
/// # Conversion Mathematics and Approximation Error
///
/// The PlayStation SPU hardware synthesises envelopes by updating a 16-bit accumulator
/// (`0..=0x7FFF`, full scale = 32767) at 44.1 kHz / 48 kHz (`notes/sound.md` §2, §3):
///
/// 1. **Attack Phase (`adsr1` bits 14..8 `ar`, bit 15 `a_exp`):**
///    - SPU step shift is `shift = ar >> 2` and step value is `step = 7 - (ar & 3)`.
///    - Linear attack steps $N_{\text{linear}} = \frac{32767}{step} \times 2^{\text{shift} - 11}$ samples.
///    - Exponential attack slows to $1/4$ speed for level > 0x6000 (top 25%), taking $\approx 1.75 \times N_{\text{linear}}$.
///    - Attack time $T_{\text{attack}} = N / 48000.0$ s.
///    - Timecents: $tc = \mathrm{clamp}(1200 \log_2(T_{\text{attack}}), -12000, 8000)$.
///    - *Approximation error:* SPU piecewise linear acceleration is mapped to SF2 convex attack;
///      total duration to peak matches within $\sim 10\%$.
///
/// 2. **Decay Phase (`adsr1` bits 7..4 `dr`):**
///    - SPU decay is discrete exponential: level is multiplied by $(1 - 2^{-(1 + dr)})$ per sample.
///    - Continuous rate constant: $k = 48000 \times 2^{-(1 + dr)} \text{ s}^{-1}$.
///    - SF2 defines `decayVolEnv` as time for a 100 dB drop: $T_{\text{decay}} = \frac{5 \ln(10)}{k} \approx \frac{23.02585 \times 2^{dr}}{48000.0}$ s.
///    - Timecents: $tc = \mathrm{clamp}(1200 \times (dr + \log_2(23.02585 / 48000.0)), -12000, 8000) \approx 1200 \times dr - 13231$.
///
/// 3. **Sustain Phase (`adsr1` bits 3..0 `sl`):**
///    - SPU sustain level: $L = (sl + 1) \times 2048$ out of 32768 (full scale when $sl = 15$).
///    - SF2 `sustainVolEnv` specifies attenuation in centibels (0.1 dB):
///      $A = 200 \log_{10}(32768 / L) = 200 \log_{10}(16 / (sl + 1))$.
///      $sl = 15 \implies 0$ cB (no attenuation); $sl = 0 \implies 241$ cB (~24.1 dB attenuation).
///    - *Approximation error:* Exact match to SPU register within $< 0.5$ cB integer rounding.
///
/// 4. **Release Phase (`adsr2` bits 4..0 `rr`, bit 5 `r_exp`):**
///    - Exponential release (`r_exp = true`): same rate as decay:
///      $T_{\text{release}} \approx \frac{23.02585 \times 2^{rr}}{48000.0} \text{ s} \implies tc \approx 1200 \times rr - 13231$.
///    - Linear release (`r_exp = false`): duration to zero is $4096 \times 2^{rr - 11} / 48000.0 = \frac{2 \times 2^{rr}}{48000.0}$ s:
///      $tc \approx 1200 \times rr - 17462$.
///    - Clamped to $[-12000, 8000]$.
#[must_use]
pub fn adsr_to_vol_env(adsr1: u16, adsr2: u16) -> VolEnv {
    let ar = (adsr1 >> 8 & 0x7F) as i32;
    let a_exp = adsr1 & 0x8000 != 0;
    let step = (7 - (ar & 3)).max(1);
    let shift = ar >> 2;
    let factor = if a_exp { 1.75f32 } else { 1.0f32 };
    let attack_samples = factor * (32767.0f32 / step as f32) * 2.0f32.powi(shift - 11);
    let t_attack = (attack_samples / 48000.0).max(0.001);
    let attack_tc = (1200.0 * t_attack.log2()).round() as i32;

    let dr = (adsr1 >> 4 & 0xF) as i32;
    let t_decay = (23.02585f32 * 2.0f32.powi(dr) / 48000.0).max(0.001);
    let decay_tc = (1200.0 * t_decay.log2()).round() as i32;

    let sl = (adsr1 & 0xF) as i32;
    let sustain_cb = if sl >= 15 {
        0u16
    } else {
        (200.0 * (16.0f32 / (sl + 1) as f32).log10()).round().clamp(0.0, 1000.0) as u16
    };

    let r_exp = adsr2 & 0x20 != 0;
    let rr = (adsr2 & 0x1F) as i32;
    let t_release = if r_exp {
        23.02585f32 * 2.0f32.powi(rr) / 48000.0
    } else {
        2.0f32 * 2.0f32.powi(rr) / 48000.0
    }
    .max(0.001);
    let release_tc = (1200.0 * t_release.log2()).round() as i32;

    VolEnv {
        attack_tc: attack_tc.clamp(-12000, 8000) as i16,
        decay_tc: decay_tc.clamp(-12000, 8000) as i16,
        sustain_cb,
        release_tc: release_tc.clamp(-12000, 8000) as i16,
    }
}

/// Converts volume (0..=127) into SoundFont 2 `initialAttenuation` in centibels (0.1 dB).
#[must_use]
pub fn volume_to_attenuation(vol: i32) -> u16 {
    if vol <= 0 {
        1440
    } else if vol >= 127 {
        0
    } else {
        (200.0 * (127.0f32 / vol.clamp(1, 127) as f32).log10()).round().clamp(0.0, 1440.0) as u16
    }
}

/// Converts pan (0..=127, 64 = centre) into SoundFont 2 `pan` (-500..=500 in 0.1% units).
#[must_use]
pub fn pan_to_sf2(pan: i32) -> i16 {
    let p = (pan.clamp(0, 127) as f32 - 64.0) * (500.0 / 64.0);
    p.round().clamp(-500.0, 500.0) as i16
}

/// Formats a name string into a 20-byte ASCII buffer with at least one null terminator.
fn format_name_20(s: &str) -> [u8; 20] {
    let mut buf = [0u8; 20];
    let bytes = s.as_bytes();
    let len = bytes.len().min(19);
    buf[..len].copy_from_slice(&bytes[..len]);
    buf
}

fn write_chunk(out: &mut Vec<u8>, id: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(id);
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(data);
    if !data.len().is_multiple_of(2) {
        out.push(0);
    }
}

fn write_string_chunk(out: &mut Vec<u8>, id: &[u8; 4], s: &str) {
    let mut data = s.as_bytes().to_vec();
    if data.is_empty() || data.last() != Some(&0) {
        data.push(0);
    }
    write_chunk(out, id, &data);
}

fn write_list_chunk(out: &mut Vec<u8>, list_type: &[u8; 4], contents: &[u8]) {
    out.extend_from_slice(b"LIST");
    let size = 4 + contents.len() as u32;
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(list_type);
    out.extend_from_slice(contents);
    if !size.is_multiple_of(2) {
        out.push(0);
    }
}

fn write_riff(out: &mut Vec<u8>, form_type: &[u8; 4], contents: &[u8]) {
    out.extend_from_slice(b"RIFF");
    let size = 4 + contents.len() as u32;
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(form_type);
    out.extend_from_slice(contents);
}

struct SampleRecord {
    name: String,
    start: u32,
    end: u32,
    startloop: u32,
    endloop: u32,
    sample_rate: u32,
    original_pitch: u8,
    pitch_correction: i8,
}

/// SoundFont 2 exporter for converting a [`SoundBench`] into an `.sf2` file.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Sf2Writer<'a> {
    bench: &'a SoundBench,
    percussion: &'a [Percussion],
    name: String,
    comment: String,
}

impl<'a> Sf2Writer<'a> {
    /// Creates a new exporter for the given sound bench.
    pub fn new(bench: &'a SoundBench) -> Self {
        Self {
            bench,
            percussion: &[],
            name: "SoundBank".to_string(),
            comment: "Exported by PSX Boot Inspection Tool".to_string(),
        }
    }

    /// Adds rendered percussion sounds, exported as the percussion preset (bank 128, preset 0).
    #[must_use]
    pub fn with_percussion(mut self, percussion: &'a [Percussion]) -> Self {
        self.percussion = percussion;
        self
    }

    /// Sets the SoundFont bank name (`INAM` chunk).
    #[must_use]
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Sets the provenance/comment string (`ICMT` chunk).
    #[must_use]
    pub fn with_comment(mut self, comment: impl Into<String>) -> Self {
        self.comment = comment.into();
        self
    }

    /// Generates the complete SoundFont 2 binary byte buffer.
    pub fn write(&self) -> Result<Vec<u8>> {
        self.to_bytes()
    }

    /// Generates the complete SoundFont 2 binary byte buffer.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        // 1. Build Sample Pool (sdta / smpl) and sample records (pdta / shdr)
        let mut smpl_pcm: Vec<i16> = Vec::new();
        let mut sample_records: Vec<SampleRecord> = Vec::new();

        // 1a. Bench samples
        for s in &self.bench.samples {
            let (pcm_data, lp) = self.bench.sample_pcm(s.index).unwrap_or((&[], None));
            let mut pcm: Vec<i16> = pcm_data
                .iter()
                .map(|&v| v.clamp(-32768.0, 32767.0).round() as i16)
                .collect();
            if pcm.len() < 48 {
                pcm.resize(48, 0);
            }

            let start = smpl_pcm.len() as u32;
            let (end, startloop, endloop) = if let Some(loop_start) = lp {
                let pcm_len = pcm.len();
                // Append 8 proximal loop-wrap points from loop_start to satisfy dwEndloop < dwEnd - 7
                let wrap_start = loop_start.min(pcm_len.saturating_sub(1));
                let wrap: Vec<i16> = (0..8)
                    .map(|k| pcm.get(wrap_start + (k % (pcm_len - wrap_start).max(1))).copied().unwrap_or(0))
                    .collect();
                pcm.extend_from_slice(&wrap);

                let actual_end = (start as usize + pcm_len + 8) as u32;
                let s_loop = (start as usize + loop_start).max(start as usize + 8) as u32;
                let e_loop = (start as usize + pcm_len) as u32;
                (actual_end, s_loop, e_loop)
            } else {
                let actual_end = (start as usize + pcm.len()) as u32;
                (actual_end, start, actual_end)
            };

            smpl_pcm.extend_from_slice(&pcm);
            // 46 zero samples after dwEnd (SoundFont 2.04 §6.1 / §7.10)
            smpl_pcm.extend(std::iter::repeat_n(0i16, 46));

            sample_records.push(SampleRecord {
                name: format!("sample_{:03}", s.index),
                start,
                end,
                startloop,
                endloop,
                sample_rate: self.bench.sample_rate(),
                original_pitch: 60,
                pitch_correction: 0,
            });
        }

        // 1b. Percussion sounds (already rendered by the caller, mono)
        {
            for piece in self.percussion {
                if piece.rate == 0 {
                    return Err(Error::Corrupt(Format::SoundBank, format!("percussion {} has no sample rate", piece.name)));
                }
                let mut pcm: Vec<i16> = piece
                    .pcm
                    .iter()
                    .map(|v| (v * 32767.0).clamp(-32768.0, 32767.0).round() as i16)
                    .collect();
                if pcm.len() < 48 {
                    pcm.resize(48, 0);
                }

                let start = smpl_pcm.len() as u32;
                let end = (start as usize + pcm.len()) as u32;
                smpl_pcm.extend_from_slice(&pcm);
                smpl_pcm.extend(std::iter::repeat_n(0i16, 46));

                let safe_name = piece.name.replace(|c: char| !c.is_ascii_alphanumeric() && c != '_', "");
                sample_records.push(SampleRecord {
                    name: format!("kit_{:02}_{}", piece.key, safe_name),
                    start,
                    end,
                    startloop: start,
                    endloop: end,
                    sample_rate: piece.rate,
                    original_pitch: piece.key,
                    pitch_correction: 0,
                });
            }
        }

        // 2. Build Hydra structures (pdta)
        let mut phdr_bytes: Vec<u8> = Vec::new();
        let mut pbag_bytes: Vec<u8> = Vec::new();
        let mut pmod_bytes: Vec<u8> = Vec::new();
        let mut pgen_bytes: Vec<u8> = Vec::new();
        let mut inst_bytes: Vec<u8> = Vec::new();
        let mut ibag_bytes: Vec<u8> = Vec::new();
        let mut imod_bytes: Vec<u8> = Vec::new();
        let mut igen_bytes: Vec<u8> = Vec::new();
        let mut shdr_bytes: Vec<u8> = Vec::new();

        let mut pbag_count: u16 = 0;
        let mut pgen_count: u16 = 0;
        let mut inst_count: u16 = 0;
        let mut ibag_count: u16 = 0;
        let mut igen_count: u16 = 0;

        let push_gen = |buf: &mut Vec<u8>, count: &mut u16, oper: u16, amount: u16| {
            buf.extend_from_slice(&oper.to_le_bytes());
            buf.extend_from_slice(&amount.to_le_bytes());
            *count += 1;
        };

        let push_bag = |buf: &mut Vec<u8>, count: &mut u16, gen_ndx: u16, mod_ndx: u16| {
            buf.extend_from_slice(&gen_ndx.to_le_bytes());
            buf.extend_from_slice(&mod_ndx.to_le_bytes());
            *count += 1;
        };

        let push_inst = |buf: &mut Vec<u8>, count: &mut u16, name: &str, bag_ndx: u16| {
            buf.extend_from_slice(&format_name_20(name));
            buf.extend_from_slice(&bag_ndx.to_le_bytes());
            *count += 1;
        };

        let push_preset = |buf: &mut Vec<u8>, name: &str, preset: u16, bank: u16, bag_ndx: u16| {
            buf.extend_from_slice(&format_name_20(name));
            buf.extend_from_slice(&preset.to_le_bytes());
            buf.extend_from_slice(&bank.to_le_bytes());
            buf.extend_from_slice(&bag_ndx.to_le_bytes());
            buf.extend_from_slice(&[0u8; 12]); // dwLibrary (4), dwGenre (4), dwMorphology (4)
        };

        // 2a. Programs -> Instruments and Presets in Bank 0
        for p in self.bench.programs.iter().filter(|p| (0..=127).contains(&p.id)) {
            let inst_index = inst_count;
            push_inst(&mut inst_bytes, &mut inst_count, &format!("Prog {:03}", p.id), ibag_count);

            for t in &p.tones {
                if t.sample_index >= self.bench.samples.len() {
                    continue;
                }
                push_bag(&mut ibag_bytes, &mut ibag_count, igen_count, 0);

                // Zone generators: keyRange MUST be first (SoundFont 2.04 §8.1.3)
                let low = t.low.clamp(0, 127) as u8;
                let high = t.high.clamp(0, 127) as u8;
                let (low, high) = if low <= high { (low, high) } else { (0, 127) };
                push_gen(&mut igen_bytes, &mut igen_count, GEN_KEY_RANGE, (low as u16) | ((high as u16) << 8));

                push_gen(&mut igen_bytes, &mut igen_count, GEN_OVERRIDING_ROOT_KEY, t.root.clamp(0, 127) as u16);

                let fine_cents = if self.bench.bank == BankRef::Ps1 {
                    (t.fine as f32 * 100.0 / 128.0).round() as i16
                } else {
                    (t.fine as f32 * 100.0 / 16.0).round() as i16
                };
                push_gen(&mut igen_bytes, &mut igen_count, GEN_FINE_TUNE, fine_cents.clamp(-99, 99) as u16);

                push_gen(&mut igen_bytes, &mut igen_count, GEN_PAN, pan_to_sf2(t.pan) as u16);
                push_gen(&mut igen_bytes, &mut igen_count, GEN_INITIAL_ATTENUATION, volume_to_attenuation(t.volume));

                let env = adsr_to_vol_env(t.adsr1, t.adsr2);
                push_gen(&mut igen_bytes, &mut igen_count, GEN_ATTACK_VOL_ENV, env.attack_tc as u16);
                push_gen(&mut igen_bytes, &mut igen_count, GEN_DECAY_VOL_ENV, env.decay_tc as u16);
                push_gen(&mut igen_bytes, &mut igen_count, GEN_SUSTAIN_VOL_ENV, env.sustain_cb);
                push_gen(&mut igen_bytes, &mut igen_count, GEN_RELEASE_VOL_ENV, env.release_tc as u16);

                let looped = self.bench.samples.get(t.sample_index).is_some_and(|s| s.loop_start.is_some());
                push_gen(&mut igen_bytes, &mut igen_count, GEN_SAMPLE_MODES, if looped { 1 } else { 0 });

                // sampleID MUST be the last generator in an instrument zone (SoundFont 2.04 §8.1.3)
                push_gen(&mut igen_bytes, &mut igen_count, GEN_SAMPLE_ID, t.sample_index as u16);
            }

            // Preset in Bank 0
            push_preset(&mut phdr_bytes, &format!("Prog {:03}", p.id), p.id as u16, 0, pbag_count);
            push_bag(&mut pbag_bytes, &mut pbag_count, pgen_count, 0);
            push_gen(&mut pgen_bytes, &mut pgen_count, GEN_INSTRUMENT, inst_index);
        }

        // 2b. One-shot effects -> Presets in Bank 1
        for e in self.bench.effects.iter().filter(|e| e.id <= 127) {
            if e.sample_index >= self.bench.samples.len() {
                continue;
            }
            let inst_index = inst_count;
            push_inst(&mut inst_bytes, &mut inst_count, &format!("Effect {:03}", e.id), ibag_count);
            push_bag(&mut ibag_bytes, &mut ibag_count, igen_count, 0);

            push_gen(&mut igen_bytes, &mut igen_count, GEN_KEY_RANGE, 127u16 << 8);

            // Derive root key and fine tuning from SPU recorded pitch (0x1000 = 44.1 kHz = Middle C 60)
            let semitones_total = 12.0f32 * (e.pitch.max(1) as f32 / 4096.0).log2();
            let semitones_round = semitones_total.round();
            let root = (60.0 + semitones_round).clamp(0.0, 127.0) as u16;
            let fine_cents = ((semitones_total - semitones_round) * 100.0).round() as i16;
            push_gen(&mut igen_bytes, &mut igen_count, GEN_OVERRIDING_ROOT_KEY, root);
            push_gen(&mut igen_bytes, &mut igen_count, GEN_FINE_TUNE, fine_cents.clamp(-99, 99) as u16);

            let vl = e.volume.0 as f32;
            let vr = e.volume.1 as f32;
            let pan_val = if vl + vr > 0.0 {
                ((vr - vl) / (vr + vl) * 500.0).round().clamp(-500.0, 500.0) as i16
            } else {
                0
            };
            push_gen(&mut igen_bytes, &mut igen_count, GEN_PAN, pan_val as u16);

            let max_v = e.volume.0.max(e.volume.1).max(1) as f32;
            let atten_cb = (200.0 * (32767.0f32 / max_v).log10()).round().clamp(0.0, 1440.0) as u16;
            push_gen(&mut igen_bytes, &mut igen_count, GEN_INITIAL_ATTENUATION, atten_cb);

            let env = adsr_to_vol_env(e.adsr.0, e.adsr.1);
            push_gen(&mut igen_bytes, &mut igen_count, GEN_ATTACK_VOL_ENV, env.attack_tc as u16);
            push_gen(&mut igen_bytes, &mut igen_count, GEN_DECAY_VOL_ENV, env.decay_tc as u16);
            push_gen(&mut igen_bytes, &mut igen_count, GEN_SUSTAIN_VOL_ENV, env.sustain_cb);
            push_gen(&mut igen_bytes, &mut igen_count, GEN_RELEASE_VOL_ENV, env.release_tc as u16);

            let looped = self.bench.samples.get(e.sample_index).is_some_and(|s| s.loop_start.is_some());
            push_gen(&mut igen_bytes, &mut igen_count, GEN_SAMPLE_MODES, if looped { 1 } else { 0 });

            push_gen(&mut igen_bytes, &mut igen_count, GEN_SAMPLE_ID, e.sample_index as u16);

            // Preset in Bank 1
            push_preset(&mut phdr_bytes, &format!("Effect {:03}", e.id), e.id as u16, 1, pbag_count);
            push_bag(&mut pbag_bytes, &mut pbag_count, pgen_count, 0);
            push_gen(&mut pgen_bytes, &mut pgen_count, GEN_INSTRUMENT, inst_index);
        }

        // 2c. Percussion -> Preset in Bank 128 Preset 0
        {
            if !self.percussion.is_empty() {
                let inst_index = inst_count;
                push_inst(&mut inst_bytes, &mut inst_count, "Standard Kit", ibag_count);

                let base_sample = self.bench.samples.len();
                for (p_idx, piece) in self.percussion.iter().enumerate() {
                    push_bag(&mut ibag_bytes, &mut ibag_count, igen_count, 0);

                    // Active strictly on this piece's GM key
                    let k = piece.key.clamp(0, 127);
                    push_gen(&mut igen_bytes, &mut igen_count, GEN_KEY_RANGE, (k as u16) | ((k as u16) << 8));
                    push_gen(&mut igen_bytes, &mut igen_count, GEN_OVERRIDING_ROOT_KEY, k as u16);
                    push_gen(&mut igen_bytes, &mut igen_count, GEN_PAN, 0);
                    push_gen(&mut igen_bytes, &mut igen_count, GEN_INITIAL_ATTENUATION, 0);
                    push_gen(&mut igen_bytes, &mut igen_count, GEN_SAMPLE_MODES, 0);
                    push_gen(&mut igen_bytes, &mut igen_count, GEN_SAMPLE_ID, (base_sample + p_idx) as u16);
                }

                // Preset in Bank 128 Preset 0
                push_preset(&mut phdr_bytes, "Standard Kit", 0, 128, pbag_count);
                push_bag(&mut pbag_bytes, &mut pbag_count, pgen_count, 0);
                push_gen(&mut pgen_bytes, &mut pgen_count, GEN_INSTRUMENT, inst_index);
            }
        }

        // Terminal records (SoundFont 2.04 §7.2–7.10)
        push_preset(&mut phdr_bytes, "EOP", 0, 0, pbag_count);
        push_bag(&mut pbag_bytes, &mut pbag_count, pgen_count, 0);
        pmod_bytes.extend_from_slice(&[0u8; 10]); // 1 terminal modulator
        push_gen(&mut pgen_bytes, &mut pgen_count, 0, 0);

        push_inst(&mut inst_bytes, &mut inst_count, "EOI", ibag_count);
        push_bag(&mut ibag_bytes, &mut ibag_count, igen_count, 0);
        imod_bytes.extend_from_slice(&[0u8; 10]); // 1 terminal modulator
        push_gen(&mut igen_bytes, &mut igen_count, 0, 0);

        // shdr records
        for s in &sample_records {
            shdr_bytes.extend_from_slice(&format_name_20(&s.name));
            shdr_bytes.extend_from_slice(&s.start.to_le_bytes());
            shdr_bytes.extend_from_slice(&s.end.to_le_bytes());
            shdr_bytes.extend_from_slice(&s.startloop.to_le_bytes());
            shdr_bytes.extend_from_slice(&s.endloop.to_le_bytes());
            shdr_bytes.extend_from_slice(&s.sample_rate.to_le_bytes());
            shdr_bytes.push(s.original_pitch);
            shdr_bytes.push(s.pitch_correction as u8);
            shdr_bytes.extend_from_slice(&0u16.to_le_bytes()); // wSampleLink
            shdr_bytes.extend_from_slice(&SAMPLE_TYPE_MONO.to_le_bytes()); // sfSampleType
        }
        // Terminal sample header: EOS (SoundFont 2.04 §7.10)
        shdr_bytes.extend_from_slice(&format_name_20("EOS"));
        shdr_bytes.extend_from_slice(&[0u8; 26]);

        // 3. Assemble RIFF structure
        // 3a. LIST INFO
        let mut info_bytes = Vec::new();
        write_chunk(&mut info_bytes, b"ifil", &[2, 0, 4, 0]); // v2.04
        write_string_chunk(&mut info_bytes, b"isng", "EMU8000");
        write_string_chunk(&mut info_bytes, b"INAM", if self.name.is_empty() { "SoundBank" } else { &self.name });
        write_string_chunk(&mut info_bytes, b"ICMT", if self.comment.is_empty() { "Exported by PSX Boot Inspection Tool" } else { &self.comment });
        write_string_chunk(&mut info_bytes, b"ISFT", "PSX Boot Inspection Tool");

        // 3b. LIST sdta
        let mut sdta_bytes = Vec::new();
        let mut smpl_raw = Vec::with_capacity(smpl_pcm.len() * 2);
        for &s in &smpl_pcm {
            smpl_raw.extend_from_slice(&s.to_le_bytes());
        }
        write_chunk(&mut sdta_bytes, b"smpl", &smpl_raw);

        // 3c. LIST pdta
        let mut pdta_bytes = Vec::new();
        write_chunk(&mut pdta_bytes, b"phdr", &phdr_bytes);
        write_chunk(&mut pdta_bytes, b"pbag", &pbag_bytes);
        write_chunk(&mut pdta_bytes, b"pmod", &pmod_bytes);
        write_chunk(&mut pdta_bytes, b"pgen", &pgen_bytes);
        write_chunk(&mut pdta_bytes, b"inst", &inst_bytes);
        write_chunk(&mut pdta_bytes, b"ibag", &ibag_bytes);
        write_chunk(&mut pdta_bytes, b"imod", &imod_bytes);
        write_chunk(&mut pdta_bytes, b"igen", &igen_bytes);
        write_chunk(&mut pdta_bytes, b"shdr", &shdr_bytes);

        // 3d. Top-level RIFF sfbk
        let mut sfbk_contents = Vec::new();
        write_list_chunk(&mut sfbk_contents, b"INFO", &info_bytes);
        write_list_chunk(&mut sfbk_contents, b"sdta", &sdta_bytes);
        write_list_chunk(&mut sfbk_contents, b"pdta", &pdta_bytes);

        let mut out = Vec::new();
        write_riff(&mut out, b"sfbk", &sfbk_contents);
        Ok(out)
    }
}

/// One rendered percussion sound for the percussion preset: mono PCM in -1..1 at `rate`, played on
/// MIDI key `key` (General MIDI percussion numbering).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Percussion {
    /// MIDI key the sound answers on.
    pub key: u8,
    /// Short name (written into the sample header, at most 20 bytes with its key prefix).
    pub name: String,
    /// Mono samples, -1..1.
    pub pcm: Vec<f32>,
    /// Sample rate in Hz.
    pub rate: u32,
}

impl Percussion {
    /// A percussion sound on `key`.
    pub fn new(key: u8, name: impl Into<String>, pcm: Vec<f32>, rate: u32) -> Self {
        Self { key, name: name.into(), pcm, rate }
    }
}

/// Converts a [`SoundBench`] (and optional rendered percussion) into a SoundFont 2 (`.sf2`) file.
///
/// # Arguments
/// - `bench`: The source sound bench containing programs, tones, samples and/or UI effects.
/// - `percussion`: Rendered percussion sounds, mapped to their keys in bank 128 (may be empty).
/// - `name`: The bank name recorded in the `INAM` chunk.
/// - `comment`: The provenance or user description recorded in the `ICMT` chunk.
pub fn sf2_from_bench(bench: &SoundBench, percussion: &[Percussion], name: &str, comment: &str) -> Result<Vec<u8>> {
    Sf2Writer::new(bench).with_name(name).with_comment(comment).with_percussion(percussion).write()
}
