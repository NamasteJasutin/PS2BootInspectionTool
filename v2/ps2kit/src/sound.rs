//! Sony's OSD sound data: an "SShd" bank header, a headerless ADPCM body and "SSsq"
//! sequences, rendered the way the IOP driver and the SPU2 play them (`notes/sound.md`).

use crate::bytes::Bytes;
use crate::rom::{unpack, RomDir};
use crate::{Error, Result};
use std::collections::HashMap;

pub const SAMPLE_RATE: usize = 48000;
pub const UPDATES_PER_SECOND: f64 = 60.0;

#[derive(Debug, Clone)]
pub struct Tone {
    pub low: i32,
    pub high: i32,
    pub root: i32,
    pub fine: i32,
    pub sample_offset: usize,
    pub adsr1: u16,
    pub adsr2: u16,
    pub volume: i32,
    pub pan: i32,
    pub bend_range: i32,
    pub flags: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgramKind {
    Split,
    Layer,
    Drum,
}

#[derive(Debug, Clone)]
pub struct Program {
    pub kind: ProgramKind,
    pub volume: i32,
    pub tones: Vec<Tone>,
}

pub struct SoundBank {
    pub programs: HashMap<i32, Program>,
    pub velocity: Vec<i32>,
    pub body: Vec<u8>,
}

impl SoundBank {
    pub fn new(header: &[u8], body: Vec<u8>) -> Result<Self> {
        if header.len() <= 0x30 || &header[0xC..0x10] != b"SShd" {
            return Err(Error::Bank("signature".into()));
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
                    return Err(Error::Bank(format!("program {p} out of range")));
                }
                let mode = header.u8(h);
                let kind = if mode == 0xFF { ProgramKind::Drum } else if mode & 0x80 != 0 { ProgramKind::Layer } else { ProgramKind::Split };
                let count = if kind == ProgramKind::Drum { 0 } else { (mode & 0x7F) as usize + 1 };
                let mut tones = Vec::new();
                for t in 0..count {
                    let o = h + 8 + 16 * t;
                    if o + 16 > header.len() {
                        return Err(Error::Bank("tone out of range".into()));
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

    pub fn decode_sample(&self, offset: usize) -> (Vec<f32>, Option<usize>) { decode_adpcm(&self.body, offset) }
}

/// Decodes one PS-ADPCM sample: 16-byte blocks of a shift/filter byte, a flag byte and 28
/// nibbles. Returns PCM (±32768 scale) and the loop start, if it loops.
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

#[derive(Debug, Clone, Copy)]
pub enum EventKind {
    NoteOn { note: i32, velocity: i32 },
    NoteOff { note: i32 },
    Control(i32, i32),
    Program(i32),
    Bend(i32),
    Tempo(i32),
    End,
}

#[derive(Debug, Clone, Copy)]
pub struct SequenceEvent {
    pub kind: EventKind,
    pub channel: usize,
    pub tick: i64,
    /// Index of the driver's 60 Hz update at which the event fires.
    pub update: i64,
}

#[derive(Debug, Clone, Copy)]
pub struct Channel {
    pub program: i32,
    pub volume: i32,
    pub pan: i32,
    pub bend: i32,
}

pub struct SoundSequence {
    pub volume: i32,
    pub resolution: i64,
    pub bpm: i64,
    pub channels: [Channel; 16],
    pub events: Vec<SequenceEvent>,
}

impl SoundSequence {
    pub fn new(d: &[u8]) -> Result<Self> {
        if d.len() <= 0x110 || &d[0xC..0x10] != b"SSsq" {
            return Err(Error::Sequence("signature".into()));
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
                _ => return Err(Error::Sequence(format!("unknown event {status:#x} at {pos}"))),
            };
            events.push(SequenceEvent { kind, channel: ch, tick, update: 0 });
            let mut delta = 0i64;
            while pos < d.len() {
                let b = d.u8(pos) as i64;
                pos += 1;
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
            let step = (resolution * bpm_now << 12) / 60 / 60;
            while acc >= 1 {
                acc -= step;
                update += 1;
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
pub struct DriverTables {
    /// 608 entries, 16 steps per semitone, [208] = 0x1000.
    pub pitch: Vec<i64>,
    /// 32 (left, right) gain pairs indexed by pan >> 2.
    pub pan: Vec<(i64, i64)>,
}

impl DriverTables {
    pub fn computed() -> Self {
        Self {
            pitch: (0..608).map(|i| (4096.0 * 2f64.powf((i as f64 - 208.0) / 192.0)).round() as i64).collect(),
            pan: (0..32).map(|i| { let a = i as f64 / 31.0 * std::f64::consts::FRAC_PI_2; ((a.cos() * 128.0).round() as i64, (a.sin() * 128.0).round() as i64) }).collect(),
        }
    }

    pub fn from_driver(d: &[u8], pitch_offset: usize, pan_offset: usize) -> Option<Self> {
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
pub struct Synth<'a> {
    pub bank: &'a SoundBank,
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
        let tab = |i: i32| self.tables.pitch[i.clamp(0, 607) as usize];
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
            let c = &mut channels[e.channel];
            match e.kind {
                EventKind::Program(p) => { c.program = p; c.bend = 0x40 }
                EventKind::Control(7, v) => c.volume = v,
                EventKind::Control(10, v) => c.pan = v,
                EventKind::Bend(v) => c.bend = v,
                EventKind::NoteOn { note, velocity } => {
                    let Some(program) = self.bank.programs.get(&c.program) else { continue };
                    for tone in program.tones.iter().filter(|t| t.low <= note && note <= t.high) {
                        let p = (c.pan + tone.pan - 0x40).clamp(0, 0x7F) >> 2;
                        let (gl, gr) = self.tables.pan[p as usize];
                        let vel = self.bank.velocity[velocity.min(127) as usize] as i64;
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
/// driver tables from `rom0:OSDSND`.
pub struct BootSound {
    /// `SNDBOOTS`, interleaved stereo, starting at opening frame 0.
    pub chime: Vec<f32>,
    /// `SNDTNNLS`, the cue the opening starts when the dive begins.
    pub cue: Vec<f32>,
    /// `SNDWARNS`, the ambient loop under the warning scene (first minute).
    pub warning: Vec<f32>,
}

impl BootSound {
    pub fn load(bios: &RomDir) -> Result<Self> {
        let archive = RomDir::new(bios.module("SNDIMAGE")?.to_vec())?;
        let asset = |n: &str| -> Result<Vec<u8>> { unpack(archive.module(n)?, 0) };
        let bank = SoundBank::new(&asset("SNDBOOTH")?, asset("SNDBOOTB")?)?;
        let tables = bios
            .module("OSDSND")
            .ok()
            .and_then(|d| crate::locate::driver_tables(d).and_then(|(p, q)| DriverTables::from_driver(d, p, q)))
            .unwrap_or_else(DriverTables::computed);
        let synth = Synth { bank: &bank, tables: &tables };
        let (mut a, mut b, mut w) = (Vec::new(), Vec::new(), Vec::new());
        synth.render(&SoundSequence::new(&asset("SNDBOOTS")?)?, 0x42, 0.0, 3.0, f64::INFINITY, &mut a);
        synth.render(&SoundSequence::new(&asset("SNDTNNLS")?)?, 0x2A, 0.0, 3.0, f64::INFINITY, &mut b);
        synth.render(&SoundSequence::new(&asset("SNDWARNS")?)?, 0x36, 0.0, 0.0, 60.0, &mut w);
        Ok(Self { chime: a, cue: b, warning: w })
    }
}

/// Writes interleaved stereo float PCM as a 16-bit WAV file.
pub fn write_wav(path: &std::path::Path, pcm: &[f32], rate: u32) -> std::io::Result<()> {
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
    std::fs::write(path, data)
}
