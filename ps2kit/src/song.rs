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
//! Instrument references are numbers; songs contain no samples, BIOS bytes or paths.

use crate::bench::SoundBench;
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
    /// PS1 shell VAB, including the PS2's embedded PS1 shell.
    Ps1,
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
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Channel {
    /// Editor-visible name.
    pub name: String,
    /// Program number in the user's bank, 0..=127.
    pub program: u8,
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
            program,
            volume: 127,
            pan: 64,
            patterns: Vec::new(),
            arrangement: Vec::new(),
        }
    }
}

/// A finite arrangement over program numbers, with optional editor playback metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
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
    /// Instrument bank kind; an incompatible bench produces silence.
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
    /// Version 1 bounds songs to 120 s, 128 channels and 8192 stored/placed notes or bars.
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
            if c.program > 127 || c.volume > 127 || c.pan > 127 || c.patterns.len() > MAX_ITEMS {
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
        if frames > (MAX_SECONDS * SAMPLE_RATE as f64) as usize {
            return Err(bad("render exceeds 120 seconds"));
        }
        let text_bytes = self
            .name
            .len()
            .saturating_add(self.channels.iter().map(|c| c.name.len()).sum::<usize>())
            .saturating_add(self.import_notes.iter().map(String::len).sum::<usize>());
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
                program: c.program as i32,
                volume: c.volume as i32,
                pan: c.pan as i32,
                bend: 64,
            })
            .collect();
        let mut events = Vec::new();
        for (ci, c) in self.channels.iter().enumerate() {
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
        if self.bank == BankRef::Ps2 {
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
    pub fn render(&self, bench: &SoundBench) -> Vec<f32> {
        self.render_checked(bench).unwrap_or_default()
    }

    /// Renders like [`Self::render`], reporting invalid models with [`Error::Corrupt`].
    pub fn render_checked(&self, bench: &SoundBench) -> Result<Vec<f32>> {
        self.validate()?;
        let end = self.end();
        let (seq, channels) = self.sequence(end + self.tail_ticks)?;
        let frames = self.sample_tick(end)? + self.nominal_frames(self.tail_ticks);
        let mut mix = bench.render_song(&seq, &channels, frames, self.bank == BankRef::Ps1);
        limit(&mut mix);
        Ok(mix)
    }

    /// Previews `[start, end)` bars, preserving earlier notes/releases but omitting the
    /// final tail. Only audio up to the requested endpoint is synthesised; invalid ranges
    /// return empty. A multi-bar pattern can continue past its channel's last explicit cell.
    pub fn render_bar_range(&self, bench: &SoundBench, bars: Range<usize>) -> Vec<f32> {
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
        let mix = bench.render_song(&seq, &channels, last, self.bank == BankRef::Ps1);
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
    pub fn write_wav(&self, bench: &SoundBench, path: impl AsRef<Path>) -> Result<()> {
        sound::write_wav(path, &self.render_checked(bench)?, SAMPLE_RATE as u32)
    }

    /// Writes a finite PS2 arrangement as SSsq (sound.md §3.3), without audio data.
    /// Requires at most 16 channels; editor loops and holds into the tail are rejected.
    /// Tail, meter,
    /// names and annotations have no SSsq representation and are omitted.
    pub fn to_sssq(&self) -> Result<Vec<u8>> {
        self.validate()?;
        if self.bank != BankRef::Ps2 || self.channels.len() > 16 || self.loop_bars.is_some() {
            return Err(bad(
                "SSsq export requires PS2, at most 16 channels and no editor loop",
            ));
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

    /// Emits canonical, diffable version-1 text, retaining empty patterns and bar cells.
    pub fn to_text(&self) -> Result<String> {
        self.validate()?;
        let mut text = String::from("psxsong 1\n");
        let _ = writeln!(
            text,
            "name {}\nbank {}\nclock {} {} {}\nmix {} {} {}",
            quote(&self.name),
            if self.bank == BankRef::Ps2 {
                "ps2"
            } else {
                "ps1"
            },
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
                c.program,
                c.volume,
                c.pan
            );
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

    /// Parses version-1 text with line-numbered syntax errors and bounded allocation.
    /// Unknown/duplicate/missing records and dangling pattern references are errors.
    pub fn parse(text: &str) -> Result<Self> {
        parse_text(text)
    }
}

fn bad(message: impl Into<String>) -> Error {
    Error::Corrupt(Format::Sequence, format!("psxsong: {}", message.into()))
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
    let mut stage = 0usize;
    let mut channel: Option<Channel> = None;
    let mut pattern: Option<Pattern> = None;
    let mut arranged = false;
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
                (0, ["psxsong", "1"]) => stage = 1,
                (1, ["name", name]) => {
                    song.name = (*name).into();
                    stage = 2
                }
                (2, ["bank", bank]) => {
                    song.bank = match *bank {
                        "ps1" => BankRef::Ps1,
                        "ps2" => BankRef::Ps2,
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
                    let mut c = Channel::new(*name, number(program)?);
                    c.volume = number(volume)?;
                    c.pan = number(pan)?;
                    channel = Some(c);
                    arranged = false;
                }
                (6, ["arrangement", cells @ ..]) if channel.is_some() && !arranged => {
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
