//! Sony's OSD sound data: an "SShd" bank header, a headerless ADPCM body and "SSsq"
//! sequences, rendered the way the IOP driver and the SPU2 play them (`notes/sound.md`).

use crate::bytes::Bytes;
use crate::rom::{unpack, RomDir};
use crate::{Error, Format, Result};
use std::collections::HashMap;
use std::path::Path;

/// Output sample rate of every renderer in this crate, in Hz.
pub const SAMPLE_RATE: usize = 48000;
/// Rate of the IOP driver's sequencer tick, in Hz.
pub const UPDATES_PER_SECOND: f64 = 60.0;

/// One tone (a sample plus its playback parameters) of a program.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Tone {
    /// Lowest MIDI note the tone answers to.
    pub low: i32,
    /// Highest MIDI note the tone answers to.
    pub high: i32,
    /// Note at which the sample plays at its recorded pitch.
    pub root: i32,
    /// Fine tuning in 1/16 semitone steps.
    pub fine: i32,
    /// Start of the sample in [`SoundBank::body`](SoundBank::body).
    pub sample_offset: usize,
    /// SPU ADSR register 1.
    pub adsr1: u16,
    /// SPU ADSR register 2.
    pub adsr2: u16,
    /// Tone volume, 0..=127.
    pub volume: i32,
    /// Tone pan, 0..=127 (64 = centre).
    pub pan: i32,
    /// Pitch bend range in semitones.
    pub bend_range: i32,
    /// Raw flag byte.
    pub flags: u8,
}

/// How a program chooses tones for a note.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProgramKind {
    /// The first tone whose range covers the note plays.
    Split,
    /// Every tone whose range covers the note plays.
    Layer,
    /// A drum kit (not rendered; has no tones here).
    Drum,
}

/// A program (instrument) of a bank.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Program {
    /// Tone selection mode.
    pub kind: ProgramKind,
    /// Program volume, 0..=127.
    pub volume: i32,
    /// The tones, in bank order.
    pub tones: Vec<Tone>,
}

/// An `SShd` bank: programs, the velocity curve and the ADPCM body.
#[derive(Clone)]
#[non_exhaustive]
pub struct SoundBank {
    /// Programs by number.
    pub programs: HashMap<i32, Program>,
    /// 128-entry velocity curve.
    pub velocity: Vec<i32>,
    body: Vec<u8>,
}

impl std::fmt::Debug for SoundBank {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SoundBank").field("programs", &self.programs).field("velocity", &self.velocity).field("body_bytes", &self.body.len()).finish_non_exhaustive()
    }
}

impl SoundBank {
    /// The headerless ADPCM body; [`Tone::sample_offset`] indexes into it.
    #[must_use]
    pub fn body(&self) -> &[u8] { &self.body }

    /// Parses a bank `header` (the `SShd` block) and takes ownership of its ADPCM `body`.
    pub fn new(header: &[u8], body: Vec<u8>) -> Result<Self> {
        if header.len() <= 0x30 || &header[0xC..0x10] != b"SShd" {
            return Err(Error::Corrupt(Format::SoundBank, "signature".into()));
        }
        let mut programs = HashMap::new();
        let base = header.u32(0x10) as usize;
        if base != 0xFFFF_FFFF {
            let max = header.u16(base) as usize;
            for p in 0..=max {
                let off = header.u16(base + 2 + 2 * p) as usize;
                if off == 0xFFFF {
                    continue;
                }
                let h = base + off;
                if h + 8 > header.len() {
                    return Err(Error::Corrupt(Format::SoundBank, format!("program {p} out of range")));
                }
                let mode = header.u8(h);
                let kind = if mode == 0xFF { ProgramKind::Drum } else if mode & 0x80 != 0 { ProgramKind::Layer } else { ProgramKind::Split };
                let count = if kind == ProgramKind::Drum { 0 } else { (mode & 0x7F) as usize + 1 };
                let mut tones = Vec::new();
                for t in 0..count {
                    let o = h + 8 + 16 * t;
                    if o + 16 > header.len() {
                        return Err(Error::Corrupt(Format::SoundBank, "tone out of range".into()));
                    }
                    tones.push(Tone {
                        low: header.u8(o) as i32,
                        high: header.u8(o + 1) as i32,
                        root: header.u8(o + 2) as i32,
                        fine: header.u8(o + 3) as i8 as i32,
                        sample_offset: header.u16(o + 4) as usize * 8,
                        adsr1: header.u16(o + 6),
                        adsr2: header.u16(o + 8),
                        volume: header.u8(o + 11) as i32,
                        pan: header.u8(o + 12) as i32,
                        bend_range: header.u8(o + 13) as i32,
                        flags: header.u8(o + 15),
                    });
                }
                programs.insert(p as i32, Program { kind, volume: header.u8(h + 1) as i32, tones });
            }
        }
        let vel_off = header.u32(0x14) as usize;
        let velocity = if vel_off != 0xFFFF_FFFF && vel_off + 130 <= header.len() {
            (0..128).map(|i| header.u8(vel_off + 2 + i) as i32).collect()
        } else {
            (0..128).collect()
        };
        Ok(Self { programs, velocity, body })
    }

    /// Decodes the sample at `offset` in the body; see [`decode_adpcm`].
    #[must_use]
    pub fn decode_sample(&self, offset: usize) -> (Vec<f32>, Option<usize>) { decode_adpcm(&self.body, offset) }
}

/// Decodes one PS-ADPCM sample: 16-byte blocks of a shift/filter byte, a flag byte and 28
/// nibbles. Returns PCM (±32768 scale) and the loop start, if it loops.
#[must_use]
pub fn decode_adpcm(body: &[u8], offset: usize) -> (Vec<f32>, Option<usize>) {
    const FILTERS: [(i32, i32); 5] = [(0, 0), (60, 0), (115, -52), (98, -55), (122, -60)];
    let mut out = Vec::new();
    let (mut s1, mut s2) = (0i32, 0i32);
    let mut loop_start = None;
    let mut loops = false;
    let mut pos = offset;
    while pos + 16 <= body.len() {
        let (shift, filter, flags) = ((body[pos] & 0xF) as i32, (body[pos] >> 4) as usize, body[pos + 1]);
        let (f0, f1) = FILTERS.get(filter).copied().unwrap_or((0, 0));
        if flags & 4 != 0 {
            loop_start = Some(out.len());
        }
        for i in 0..28 {
            let byte = body[pos + 2 + i / 2];
            let mut nib = (if i & 1 == 1 { byte >> 4 } else { byte & 0xF }) as i32;
            if nib >= 8 {
                nib -= 16;
            }
            let mut s = if shift <= 12 { (nib << 12) >> shift } else { (nib << 12) >> 9 };
            s += (s1 * f0 + s2 * f1 + 32) >> 6;
            s = s.clamp(-32768, 32767);
            out.push(s as f32);
            s2 = s1;
            s1 = s;
        }
        pos += 16;
        if flags & 1 != 0 {
            loops = flags & 2 != 0;
            break;
        }
    }
    (out, if loops { loop_start } else { None })
}

/// A decoded sequence event (a subset of MIDI).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum EventKind {
    /// Key on.
    NoteOn {
        /// MIDI note number.
        note: i32,
        /// Velocity, 1..=127.
        velocity: i32,
    },
    /// Key off.
    NoteOff {
        /// MIDI note number.
        note: i32,
    },
    /// Controller change `(controller, value)`; 7 is volume, 10 is pan.
    Control(i32, i32),
    /// Program change.
    Program(i32),
    /// Pitch bend, coarse byte only (64 = none).
    Bend(i32),
    /// Tempo change, in beats per minute.
    Tempo(i32),
    /// End of track.
    End,
}

/// One event of a sequence with its position on both clocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct SequenceEvent {
    /// What happens.
    pub kind: EventKind,
    /// MIDI channel, 0..=15.
    pub channel: usize,
    /// Position in sequence ticks.
    pub tick: i64,
    /// Index of the driver's 60 Hz update at which the event fires.
    pub update: i64,
}

/// Initial state of one MIDI channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct Channel {
    /// Program number.
    pub program: i32,
    /// Volume, 0..=127.
    pub volume: i32,
    /// Pan, 0..=127 (64 = centre).
    pub pan: i32,
    /// Pitch bend, 0..=127 (64 = none).
    pub bend: i32,
}

/// An `SSsq` sequence, decoded and scheduled on the driver's 60 Hz clock.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct SoundSequence {
    /// Sequence volume, 0..=127.
    pub volume: i32,
    /// Ticks per quarter note.
    pub resolution: i64,
    /// Initial tempo in beats per minute.
    pub bpm: i64,
    /// Initial channel states.
    pub channels: [Channel; 16],
    /// Events in time order, ending with [`EventKind::End`].
    pub events: Vec<SequenceEvent>,
}

impl SoundSequence {
    /// Parses an `SSsq` block (header, 16 channel records, then the event stream).
    pub fn new(d: &[u8]) -> Result<Self> {
        if d.len() <= 0x110 || &d[0xC..0x10] != b"SSsq" {
            return Err(Error::Corrupt(Format::Sequence, "signature".into()));
        }
        let resolution = d.u16(2) as i64;
        let bpm = d.u16(4) as i64;
        let mut channels = [Channel { program: 0, volume: 0, pan: 0, bend: 0 }; 16];
        for (c, ch) in channels.iter_mut().enumerate() {
            let o = 0x10 + 16 * c;
            *ch = Channel { program: d.u8(o + 2) as i32, volume: d.u8(o + 3) as i32, pan: d.u8(o + 4) as i32, bend: d.u8(o + 10) as i32 };
        }
        let mut events = Vec::new();
        let (mut pos, mut tick, mut status) = (0x110usize, 0i64, 0u8);
        while pos < d.len() {
            if d.u8(pos) & 0x80 != 0 { status = d.u8(pos) } else { pos -= 1 }
            let d1 = d.u8(pos + 1) as i32;
            let d2 = d.u8(pos + 2) as i32;
            let (hi, ch) = (status & 0xF0, (status & 0xF) as usize);
            let kind = match hi {
                0x90 if d2 != 0 => { pos += 3; EventKind::NoteOn { note: d1, velocity: d2 } }
                0x80 | 0x90 => { pos += 3; EventKind::NoteOff { note: d1 } }
                0xB0 => { pos += 3; EventKind::Control(d1, d2) }
                0xC0 => { pos += 2; EventKind::Program(d1) }
                0xE0 => { pos += 2; EventKind::Bend(d1) }
                _ if status == 0xFF && d1 == 0x2F => {
                    events.push(SequenceEvent { kind: EventKind::End, channel: 0, tick, update: 0 });
                    break;
                }
                _ if status == 0xFF && d1 == 0x51 => { let t = d.u16(pos + 2) as i32; pos += 4; EventKind::Tempo(t) }
                _ => return Err(Error::Corrupt(Format::Sequence, format!("unknown event {status:#x} at {pos}"))),
            };
            events.push(SequenceEvent { kind, channel: ch, tick, update: 0 });
            let mut delta = 0i64;
            let mut bytes = 0;
            while pos < d.len() {
                let b = d.u8(pos) as i64;
                pos += 1;
                bytes += 1;
                if bytes > 4 {
                    return Err(Error::Corrupt(Format::Sequence, format!("delta time longer than 4 bytes at {pos}")));
                }
                delta = delta << 7 | (b & 0x7F);
                if b & 0x80 == 0 {
                    break;
                }
            }
            tick += delta;
        }
        // The driver keeps a 20.12 fixed-point countdown and fires events while it is <= 0,
        // subtracting (resolution * bpm << 12) / 60 / 60 per 60 Hz update.
        let (mut bpm_now, mut acc, mut update, mut last) = (bpm, 0i64, 0i64, 0i64);
        for e in events.iter_mut() {
            acc += (e.tick - last) << 12;
            last = e.tick;
            let step = ((resolution * bpm_now) << 12) / 60 / 60;
            if step <= 0 {
                return Err(Error::Corrupt(Format::Sequence, "tempo or resolution is zero".into()));
            }
            // Closed form of `while acc >= 1 { acc -= step; update += 1 }`.
            if acc >= 1 {
                let n = (acc - 1) / step + 1;
                acc -= n * step;
                update += n;
            }
            e.update = update;
            if let EventKind::Tempo(t) = e.kind {
                bpm_now = t as i64;
            }
        }
        Ok(Self { volume: d.u8(0) as i32, resolution, bpm, channels, events })
    }
}

/// The driver's lookup tables, read from the IOP module when a layout is known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriverTables {
    /// 608 entries, 16 steps per semitone, entry 208 = 0x1000.
    pub pitch: Vec<i64>,
    /// 32 (left, right) gain pairs indexed by pan >> 2.
    pub pan: Vec<(i64, i64)>,
}

impl DriverTables {
    /// The tables recomputed from their formulas (equal-tempered pitch, constant-power pan),
    /// for when `rom0:OSDSND` cannot be located.
    #[must_use]
    pub fn computed() -> Self {
        Self {
            pitch: (0..608).map(|i| (4096.0 * 2f64.powf((i as f64 - 208.0) / 192.0)).round() as i64).collect(),
            pan: (0..32).map(|i| { let a = i as f64 / 31.0 * std::f64::consts::FRAC_PI_2; ((a.cos() * 128.0).round() as i64, (a.sin() * 128.0).round() as i64) }).collect(),
        }
    }

    /// The tables as the driver module `d` (`rom0:OSDSND`) holds them, located by content;
    /// `None` when the module does not contain them.
    #[must_use]
    pub fn from_driver(d: &[u8]) -> Option<Self> {
        let (pitch_offset, pan_offset) = crate::locate::driver_tables(d)?;
        if pitch_offset + 608 * 2 > d.len() || pan_offset + 64 > d.len() {
            return None;
        }
        Some(Self {
            pitch: (0..608).map(|i| d.u16(pitch_offset + i * 2) as i64).collect(),
            pan: (0..32).map(|i| (d.u8(pan_offset + i * 2) as i64, d.u8(pan_offset + i * 2 + 1) as i64)).collect(),
        })
    }
}

/// SPU ADSR level (0..=0x7FFF) per output sample; the key is released after `on_samples`.
#[must_use]
pub fn envelope(a1: u16, a2: u16, on_samples: usize, total: usize) -> Vec<f32> {
    #[derive(PartialEq, Clone, Copy)]
    enum Phase { Attack, Decay, Sustain, Release }
    let mut env = vec![0f32; total];
    let (ar, dr, sl) = ((a1 >> 8 & 0x7F) as i32, (a1 >> 4 & 0xF) as i32, (a1 & 0xF) as i32);
    let a_exp = a1 & 0x8000 != 0;
    let (s_exp, s_dec, sr) = (a2 & 0x8000 != 0, a2 & 0x4000 != 0, (a2 >> 6 & 0x7F) as i32);
    let (r_exp, rr) = (a2 & 0x20 != 0, (a2 & 0x1F) as i32);
    let sustain_level = (sl + 1) * 0x800;
    let (mut level, mut i, mut phase) = (0i32, 0usize, Phase::Attack);
    while i < total {
        if phase != Phase::Release && i >= on_samples {
            phase = Phase::Release;
        }
        let (shift, step, exp, dec) = match phase {
            Phase::Attack => (ar >> 2, 7 - (ar & 3), a_exp, false),
            Phase::Decay => (dr, -8, true, true),
            Phase::Sustain => (sr >> 2, if s_dec { -8 + (sr & 3) } else { 7 - (sr & 3) }, s_exp, s_dec),
            Phase::Release => (rr, -8, r_exp, true),
        };
        let mut cycles = 1usize << (shift - 11).max(0);
        let mut st = step << (11 - shift).max(0);
        if exp && !dec && level > 0x6000 {
            cycles *= 4;
        }
        if exp && dec {
            st = (st * level) >> 15;
        }
        let mut end = (i + cycles).min(total);
        if phase != Phase::Release && i < on_samples && on_samples < end {
            end = on_samples;
        }
        for e in &mut env[i..end] {
            *e = level as f32;
        }
        i = end;
        level = (level + st).clamp(0, 0x7FFF);
        if phase == Phase::Attack && level >= 0x7FFF { phase = Phase::Decay }
        if phase == Phase::Decay && level <= sustain_level { phase = Phase::Sustain }
        if phase == Phase::Release && level == 0 { break }
    }
    env
}

/// Renders a sequence through the bank into 48 kHz interleaved stereo, voice by voice.
#[derive(Debug, Clone, Copy)]
pub struct Synth<'a> {
    /// The bank the sequence's programs refer to.
    pub bank: &'a SoundBank,
    /// The pitch and pan tables.
    pub tables: &'a DriverTables,
}

struct Voice {
    start: usize,
    off: Option<usize>,
    tone: Tone,
    pitch: i64,
    vol_l: i64,
    vol_r: i64,
}

impl<'a> Synth<'a> {
    fn pitch_register(&self, root: i32, note: i32, fine: i32, bend: i32, bend_range: i32) -> i64 {
        let b = ((bend - 0x40) * bend_range) >> 2;
        // The tables are public data: a short one must not panic.
        let tab = |i: i32| self.tables.pitch.get(i.clamp(0, 607) as usize).copied().unwrap_or(0x1000);
        let value = if note < root {
            let d = root - note;
            (tab((12 - d % 12) * 16 + b + 0xD0 + fine) * 44100) >> (d / 12 + 1)
        } else {
            let d = note - root;
            (tab((d % 12) * 16 + b + 0xD0 + fine) * 44100) << (d / 12)
        };
        (value / 48000) & 0xFFFF
    }

    fn volume_register(seq: i64, channel: i64, program: i64, velocity: i64, tone: i64, pan: i64) -> i64 {
        ((((seq * channel * program * velocity) >> 14) * tone * pan) >> 14) & 0x7FFF
    }

    /// Mixes `sequence` into `mix` starting `start_seconds` in, growing the buffer as needed.
    pub fn render(&self, seq: &SoundSequence, sequence_volume: i64, start_seconds: f64, tail: f64, max_seconds: f64, mix: &mut Vec<f32>) {
        let rate = SAMPLE_RATE;
        let base = (start_seconds * rate as f64) as usize;
        let mut channels = seq.channels;
        let mut voices: Vec<Voice> = Vec::new();
        let mut active: HashMap<i32, Vec<usize>> = HashMap::new();
        for e in seq.events.iter().filter(|e| e.update as f64 / UPDATES_PER_SECOND <= max_seconds) {
            let t = base + (e.update as f64 / UPDATES_PER_SECOND * rate as f64) as usize;
            let Some(c) = channels.get_mut(e.channel) else { continue };
            match e.kind {
                EventKind::Program(p) => { c.program = p; c.bend = 0x40 }
                EventKind::Control(7, v) => c.volume = v,
                EventKind::Control(10, v) => c.pan = v,
                EventKind::Bend(v) => c.bend = v,
                EventKind::NoteOn { note, velocity } => {
                    let Some(program) = self.bank.programs.get(&c.program) else { continue };
                    for tone in program.tones.iter().filter(|t| t.low <= note && note <= t.high) {
                        let p = (c.pan + tone.pan - 0x40).clamp(0, 0x7F) >> 2;
                        let (gl, gr) = self.tables.pan.get(p as usize).copied().unwrap_or((0x80, 0x80));
                        let vel = self.bank.velocity.get(velocity.clamp(0, 127) as usize).copied().unwrap_or(velocity) as i64;
                        let (seqv, chv, prv, tv) = (sequence_volume, c.volume as i64, program.volume as i64, tone.volume as i64);
                        voices.push(Voice {
                            start: t,
                            off: None,
                            tone: tone.clone(),
                            pitch: self.pitch_register(tone.root, note, tone.fine, c.bend, tone.bend_range),
                            vol_l: Self::volume_register(seqv, chv, prv, vel, tv, gl),
                            vol_r: Self::volume_register(seqv, chv, prv, vel, tv, gr),
                        });
                        active.entry((e.channel as i32) << 8 | note).or_default().push(voices.len() - 1);
                        if program.kind == ProgramKind::Split {
                            break;
                        }
                    }
                }
                EventKind::NoteOff { note } => {
                    for v in active.remove(&((e.channel as i32) << 8 | note)).unwrap_or_default() {
                        voices[v].off = Some(t);
                    }
                }
                _ => {}
            }
        }
        let end_update = seq.events.last().map(|e| e.update).unwrap_or(0) as f64;
        let seconds = (end_update / UPDATES_PER_SECOND).min(max_seconds);
        let total = base + ((seconds + tail) * rate as f64) as usize;
        if mix.len() < total * 2 {
            mix.resize(total * 2, 0.0);
        }
        let mut samples: HashMap<usize, (Vec<f32>, Option<usize>)> = HashMap::new();
        let master = 0x3FFF as f32 / 16384.0;
        for v in &voices {
            let (pcm, lp) = samples.entry(v.tone.sample_offset).or_insert_with(|| self.bank.decode_sample(v.tone.sample_offset));
            let n = total.saturating_sub(v.start);
            if n == 0 || pcm.len() < 2 {
                continue;
            }
            let env = envelope(v.tone.adsr1, v.tone.adsr2, v.off.map(|o| o - v.start).unwrap_or(n), n);
            let (gain_l, gain_r) = (v.vol_l as f32 / 16384.0 * master, v.vol_r as f32 / 16384.0 * master);
            let step = v.pitch as f64 / 4096.0;
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
    }
}

/// The boot sounds as OSDSYS schedules them, synthesised from `rom0:SNDIMAGE` with the
/// driver tables from `rom0:OSDSND`. All three are [`SAMPLE_RATE`] interleaved stereo.
#[derive(Clone)]
#[non_exhaustive]
pub struct BootSound {
    /// `SNDBOOTS`, interleaved stereo, starting at opening frame 0.
    pub chime: Vec<f32>,
    /// `SNDTNNLS`, the cue the opening starts when the dive begins.
    pub cue: Vec<f32>,
    /// `SNDWARNS`, the ambient loop under the warning scene (first minute).
    pub warning: Vec<f32>,
}

impl std::fmt::Debug for BootSound {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let secs = |v: &Vec<f32>| v.len() as f64 / 2.0 / SAMPLE_RATE as f64;
        f.debug_struct("BootSound").field("chime_s", &secs(&self.chime)).field("cue_s", &secs(&self.cue)).field("warning_s", &secs(&self.warning)).finish_non_exhaustive()
    }
}

impl BootSound {
    /// Renders the three sequences from a BIOS dump. Falls back to
    /// [`DriverTables::computed`] when `rom0:OSDSND` is missing or not recognised.
    pub fn load(bios: &RomDir) -> Result<Self> {
        let archive = RomDir::new(bios.module("SNDIMAGE")?.to_vec())?;
        let asset = |n: &str| -> Result<Vec<u8>> { unpack(archive.module(n)?, 0) };
        let bank = SoundBank::new(&asset("SNDBOOTH")?, asset("SNDBOOTB")?)?;
        let tables = bios.module("OSDSND").ok().and_then(DriverTables::from_driver).unwrap_or_else(DriverTables::computed);
        let synth = Synth { bank: &bank, tables: &tables };
        let (mut a, mut b, mut w) = (Vec::new(), Vec::new(), Vec::new());
        // The chime and the cue are a few seconds; the cap keeps a damaged sequence from
        // asking for an unbounded buffer.
        synth.render(&SoundSequence::new(&asset("SNDBOOTS")?)?, 0x42, 0.0, 3.0, 120.0, &mut a);
        synth.render(&SoundSequence::new(&asset("SNDTNNLS")?)?, 0x2A, 0.0, 3.0, 120.0, &mut b);
        synth.render(&SoundSequence::new(&asset("SNDWARNS")?)?, 0x36, 0.0, 0.0, 60.0, &mut w);
        Ok(Self { chime: a, cue: b, warning: w })
    }
}

/// Writes interleaved stereo float PCM as a 16-bit WAV file.
pub fn write_wav(path: impl AsRef<Path>, pcm: &[f32], rate: u32) -> Result<()> {
    let mut data = Vec::with_capacity(44 + pcm.len() * 2);
    let bytes = (pcm.len() * 2) as u32;
    data.extend_from_slice(b"RIFF");
    data.extend_from_slice(&(36 + bytes).to_le_bytes());
    data.extend_from_slice(b"WAVEfmt ");
    data.extend_from_slice(&16u32.to_le_bytes());
    data.extend_from_slice(&1u16.to_le_bytes());
    data.extend_from_slice(&2u16.to_le_bytes());
    data.extend_from_slice(&rate.to_le_bytes());
    data.extend_from_slice(&(rate * 4).to_le_bytes());
    data.extend_from_slice(&4u16.to_le_bytes());
    data.extend_from_slice(&16u16.to_le_bytes());
    data.extend_from_slice(b"data");
    data.extend_from_slice(&bytes.to_le_bytes());
    for &v in pcm {
        data.extend_from_slice(&((v * 32767.0).round().clamp(-32768.0, 32767.0) as i16).to_le_bytes());
    }
    Ok(std::fs::write(path, data)?)
}
