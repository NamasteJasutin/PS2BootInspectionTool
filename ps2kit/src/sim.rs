//! The opening as functions of the frame number: towers from the play history, the camera
//! timeline, closed-form motion, and the whole boot as one clock.

use crate::bios::OpeningAssets;
use crate::history::PlayHistory;
use glam::Vec3;
use std::f32::consts::PI;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VideoMode {
    Ntsc,
    Pal,
}

impl VideoMode {
    pub fn fps(self) -> f32 { if self == Self::Pal { 50.0 } else { 60.0 } }
    /// The opening integrates with a 1.2 step at 50 Hz so both modes take the same time.
    pub fn time_step(self) -> f32 { if self == Self::Pal { 1.2 } else { 1.0 } }
    pub fn field_height(self) -> f32 { if self == Self::Pal { 256.0 } else { 224.0 } }
    pub fn aspect_y(self) -> f32 { if self == Self::Pal { 0.526271 } else { 0.457627 } }
    pub fn name(self) -> &'static str { if self == Self::Pal { "PAL 50 Hz" } else { "NTSC 60 Hz" } }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SceneKind {
    Full,
    Boot,
    Warning,
    Logo,
}

impl SceneKind {
    pub const ALL: [SceneKind; 4] = [Self::Full, Self::Boot, Self::Warning, Self::Logo];
    pub fn name(self) -> &'static str {
        match self {
            Self::Full => "Full boot: BIOS → disc → hand-off",
            Self::Boot => "Boot (towers)",
            Self::Warning => "Warning (insert disc)",
            Self::Logo => "PlayStation 2 logo (disc boot)",
        }
    }
}

/// One tower of the opening, derived from a history record.
#[derive(Debug, Clone)]
pub struct Tower {
    pub column: usize,
    pub row: usize,
    pub record: usize,
    /// World-space centre (already pushed back so the near cap lines up).
    pub centre: Vec3,
    pub half_length: f32,
    /// Vertex brightness of the near cap in GS units (128 = texture colour unchanged).
    pub brightness: f32,
    /// Still-growing towers sway; finished ones stand still.
    pub growing: bool,
    pub quarter_turns: i32,
    pub uv_offset: f32,
}

pub const TOWER_HALF_WIDTH: f32 = 2.0;

/// Static scene data built once per (assets, history) pair.
#[derive(Debug, Clone, Default)]
pub struct OpeningScene {
    pub towers: Vec<Tower>,
}

impl OpeningScene {
    pub fn new(assets: &OpeningAssets, history: &PlayHistory) -> Self {
        let mut towers = Vec::new();
        for (i, rec) in history.records.iter().enumerate() {
            if rec.is_empty() || i >= assets.slots.len() {
                continue;
            }
            let c = rec.count as usize;
            let step = (if c < 14 { c } else { 4 + (c - 14) % 10 }).min(13);
            for (k, &(col, row)) in assets.slots[i].iter().enumerate() {
                let (fill, size) = if k == rec.index as usize {
                    (assets.fill[step], assets.size[step])
                } else if rec.mask >> k & 1 == 1 {
                    (1.0, 1.0)
                } else {
                    continue;
                };
                let m = assets.tower_positions[col][row];
                let h = (size * 30.0).max(3.0);
                let centre = Vec3::new((m.x + 4.8) * 4.0, (m.y - 6.5) * 4.0, (m.z + 4.0) * 12.0 + 150.0 + fill * 30.0 - h);
                let alpha = if fill >= 1.0 { 0 } else { ((1.0 - fill) * 128.0) as i32 };
                let factor = if alpha == 0 { h / 30.0 } else { alpha as f32 / 128.0 };
                let (a, b) = (col as i32 + 3, row as i32 + 6);
                towers.push(Tower {
                    column: col,
                    row,
                    record: i,
                    centre,
                    half_length: h,
                    brightness: light_map(a, b) * factor,
                    growing: fill != 1.0,
                    quarter_turns: ((a + b) * a / (row as i32 + 7)) % 4,
                    uv_offset: ((a + b) * (col as i32 + 8) / (row as i32 + 7) + (a + b) * (col as i32 + 7) / (row as i32 + 9)) as f32 / 256.0,
                });
            }
        }
        Self { towers }
    }
}

/// Precomputed brightness over the grid: two radial falloffs plus a fixed per-cell jitter.
pub fn light_map(inner: i32, outer: i32) -> f32 {
    let radius = 5202f32.sqrt();
    let cell = |k: i32| (2 * k - 20) as f32 * 5.1 * 0.5 + 2.55;
    let d1 = (-5.1 - cell(inner)).hypot(0.0 - cell(outer));
    let v1 = ((radius - 2.0 * d1) * 255.0 / radius).clamp(32.0, 255.0);
    let d2 = (10.2 - cell(inner)).hypot(5.1 - cell(outer));
    let v2 = ((radius - 4.0 * d2) * 255.0 / radius * 0.5).clamp(32.0, 255.0);
    let jitter = (((inner + outer) * inner / (outer + 1)) % 11 - 5) as f32 * 10.0;
    ((v1 + v2) * 0.85 - jitter).clamp(32.0, 220.0)
}

/// The scripted camera: a point moving along +Z that rolls about its view axis.
#[derive(Debug, Clone, Copy, Default)]
pub struct CameraState {
    pub z: f32,
    pub roll: f32,
    pub stage: i32,
    /// Downward tilt of the view direction (the warning scene looks slightly down).
    pub tilt: f32,
}

impl CameraState {
    pub fn position(&self) -> Vec3 { Vec3::new(0.0, 0.0, self.z) }
    pub fn forward(&self) -> Vec3 { Vec3::new(0.0, self.tilt, 1.0) }
    pub fn up(&self) -> Vec3 { Vec3::new(self.roll.sin(), self.roll.cos(), 0.0) }
}

/// The opening's stage machine and camera kinematics, precomputed for every frame.
#[derive(Debug, Clone)]
pub struct Timeline {
    pub video: VideoMode,
    pub kind: SceneKind,
    pub states: Vec<CameraState>,
    /// Frame at which the dive starts (boot) or the exit fade starts (warning).
    pub dive_frame: usize,
}

impl Timeline {
    pub fn fps(&self) -> f32 { self.video.fps() }
    /// First frame after the scene has ended.
    pub fn end_frame(&self) -> usize { self.states.len() - 1 }

    /// The boot scene. `disc_settled_frame` is when the drive has finished identifying the
    /// disc (0 = already known); the dive needs this and at least two seconds of drift.
    pub fn boot(disc_settled_frame: usize, video: VideoMode) -> Self {
        let dt = video.time_step();
        let fps = video.fps() as usize;
        let thresholds = [16.0f32, 56.0, 104.0];
        let (mut z, mut vz, mut az, mut jz) = (16.0f32, 0.04f32, 0.0f32, 0.0f32);
        let (mut roll, mut vr, mut ar) = (-0.12f32, 0.001f32, 0.0f32);
        let (mut stage, mut diving, mut dive) = (0usize, false, None);
        let mut out = vec![CameraState { z, roll, stage: 0, tilt: 0.0 }];
        let mut frame = 0usize;
        while stage < 3 && frame < 4000 {
            if thresholds[stage] < z {
                stage += 1;
            }
            match stage {
                1 => {
                    jz = 4e-7;
                    if frame >= disc_settled_frame && frame > 2 * fps {
                        diving = true;
                        stage = 2;
                    }
                }
                2 => {
                    if frame > 10 * fps {
                        diving = true;
                    }
                    if diving {
                        dive.get_or_insert(frame);
                        az = 0.0099;
                        ar = 0.000195;
                    }
                }
                _ => {}
            }
            vr += ar * dt;
            vz += (2.0 * az + jz) * 0.5 * dt;
            az += jz * dt;
            roll += (2.0 * vr + ar) * 0.5 * dt;
            z += (2.0 * vz + az) * 0.5 * dt;
            if roll > PI { roll -= 2.0 * PI }
            if roll < -PI { roll += 2.0 * PI }
            frame += 1;
            out.push(CameraState { z, roll, stage: stage as i32, tilt: 0.0 });
        }
        Self { video, kind: SceneKind::Boot, states: out, dive_frame: dive.unwrap_or(0) }
    }

    /// The warning scene: dolly from z = 672 to 800, then hold with a slow roll until the
    /// drive reports a change (`exit_frame`), after which the scene fades for 128 frames.
    pub fn warning(exit_frame: usize, video: VideoMode) -> Self {
        let dt = video.time_step();
        let (mut z, mut vz, mut az) = (672.0f32, 2.16f32, -0.0178f32);
        let mut roll = 0.0f32;
        let vr = 0.00462f32;
        let mut stage = 4usize;
        let thresholds = [16.0f32, 56.0, 104.0, 320.0, 672.0, 800.0, 1160.0];
        let mut out = vec![CameraState { z, roll, stage: 4, tilt: -0.03 }];
        for _ in 0..exit_frame + 129 {
            if stage < 6 && thresholds[stage] < z {
                stage += 1;
            }
            if stage == 6 {
                vz = 0.0;
                az = 0.0;
            }
            vz += az * dt;
            z += (2.0 * vz + az) * 0.5 * dt;
            roll += vr * dt;
            if roll > PI { roll -= 2.0 * PI }
            out.push(CameraState { z, roll, stage: stage as i32, tilt: -0.03 });
        }
        Self { video, kind: SceneKind::Warning, states: out, dive_frame: exit_frame }
    }

    /// The logo program: its animated fields followed by the 120-field hold.
    pub fn logo(video: VideoMode) -> Self {
        let animated = if video == VideoMode::Pal { 35 - 14 } else { 42 - 17 } + 1;
        Self { video, kind: SceneKind::Logo, states: vec![CameraState::default(); animated + 120 + 1], dive_frame: 0 }
    }

    /// The PS1 licence screen: its fields followed by a hold while the game loads.
    pub fn ps1_licence(video: VideoMode, fields: usize) -> Self {
        Self { video, kind: SceneKind::Logo, states: vec![CameraState::default(); fields + 120 + 1], dive_frame: 0 }
    }

    /// Camera at a (possibly fractional) frame; clamps outside the scene.
    pub fn camera(&self, frame: f32) -> CameraState {
        let end = self.end_frame();
        let f = frame.clamp(0.0, end as f32);
        let i = (f as usize).min(end.saturating_sub(1));
        let (a, b) = (self.states[i], self.states[(i + 1).min(end)]);
        let t = f - i as f32;
        CameraState { z: a.z + (b.z - a.z) * t, roll: a.roll + (b.roll - a.roll) * t, stage: a.stage, tilt: a.tilt }
    }

    /// First frame whose camera has passed depth `z`.
    pub fn frame_passing(&self, z: f32) -> usize {
        self.states.iter().position(|s| s.z > z).unwrap_or(self.end_frame())
    }
}

/// Closed-form animation of everything that is not the camera.
pub mod motion {
    use super::*;

    /// Extra rotation (radians) of a still-growing tower: ±10°, 360-frame period.
    pub fn sway(frame: f32) -> f32 { ((frame % 360.0 - 180.0) * PI / 180.0).sin() * 10.0 * PI / 180.0 }

    /// World position of light orb `i` (0..=3).
    pub fn orb_position(i: usize, frame: f32, seed: f32) -> Vec3 {
        let k = (i + 10) as f32;
        let a = (frame + seed + 17.0 * i as f32) * 0.01 * k * 0.1;
        let b = (frame + seed + 15.0 * i as f32) * 0.005 * k * 0.1;
        Vec3::new((10 - i as i32) as f32 * a.cos(), (i + 3) as f32 * b.sin(), a.cos() * 12.0 + 88.0)
    }

    /// Texture scroll of fog layer `i` (0..=5), in texture widths.
    pub fn fog_scroll(i: usize, frame: f32) -> f32 { ((14 - i) as f32 * 0.0001 * (i + 1) as f32 * 0.5 * frame) % 1.0 }

    /// Euler angles of glass cube `i` (0..=4).
    pub fn cube_rotation(i: usize, frame: f32) -> Vec3 {
        let s = if i == 2 { 0.9 } else { (i as f32 - 2.0) * 0.8 };
        let start = s * (i % 3) as f32 * 3.7 + 0.285599;
        Vec3::splat(start) + Vec3::new(0.0031 / s, s * 0.0022, s / 1000.0 + 0.0013) * frame
    }

    /// Opacity (0..=1) of the lettering, `t` frames after the camera passed z = 18.
    pub fn lettering_alpha(t: f32) -> f32 {
        if t <= 0.0 || t >= 120.0 { return 0.0 }
        let counter = if t <= 60.0 { 4.0 * t } else { 480.0 - 4.0 * t };
        counter.min(112.0) / 128.0
    }

    pub fn fade_alpha(z: f32) -> f32 { if z > 72.0 { ((z - 72.0) * 4.0).min(128.0) / 128.0 } else { 0.0 } }
    pub fn defocus_passes(z: f32) -> usize { if z > 56.0 { (((z - 56.0) / 12.0) as usize).min(3) } else { 0 } }

    // Warning scene
    pub fn warning_brightness(z: f32) -> f32 { (740.0 - (1160.0 - z)) * 128.0 / 740.0 * 0.6 }
    /// Light-source disc `i` (0..=6): orbit position and the (wildly spinning) rotation vector.
    pub fn warning_disc(i: usize, frame: f32) -> (Vec3, Vec3) {
        let n = (i + 1) as f32;
        let k = ((7 - i) * (7 - i)) as f32 * 2.0 * PI / 64.0;
        let r = k / 2.0;
        let phi = frame % 201.0 / 32.0 - PI + k;
        let base = Vec3::new(0.0, 0.0, i as f32 * 0.925 * 2.0 * PI / 7.0);
        (Vec3::new(r * phi.cos(), r * phi.sin(), 1160.0), base + Vec3::new(0.2, 0.27, 0.35) * n * frame)
    }
    pub fn warning_fade_in(z: f32) -> f32 { if z < 800.0 { ((128.0 - (z - 672.0)) / 128.0).clamp(0.0, 1.0) } else { 0.0 } }
    /// Glass prism `i` (0..=4) of the warning scene: Euler angles at `frame`.
    pub fn prism_rotation(i: usize, frame: f32) -> Vec3 {
        let s = if i == 2 { 0.9 } else { (i as f32 - 2.0) * 0.8 };
        let start = Vec3::new(s * ((2 * i) % 9) as f32 / 4.0, s * ((2 * i) % 8) as f32 / 5.0, s * ((2 * i) % 7) as f32 / 6.0);
        start + Vec3::new(0.004 / s, 0.003 * s, s / 800.0 + 0.002) * frame
    }
}

/// What the console is busy with while the screen is still black after power-on.
/// Durations are the mid-points of the estimates in `notes/boot_sequence.md`.
#[derive(Debug, Clone)]
pub struct BootPhase {
    pub name: &'static str,
    pub seconds: f32,
}

pub const POWER_ON_PHASES: [BootPhase; 9] = [
    BootPhase { name: "IOP boot #1: IOPBOOT + 29 modules from ROM", seconds: 0.55 },
    BootPhase { name: "EELOAD loads rom0:OSDSYS (363 KB)", seconds: 0.20 },
    BootPhase { name: "OSDSYS stub decompresses itself to 0x200000", seconds: 0.06 },
    BootPhase { name: "IOP boot #2: rom0:UDNL rom0:OSDCNF, 39 modules", seconds: 0.85 },
    BootPhase { name: "Memory card mount (sceMcGetInfo, system folder)", seconds: 0.30 },
    BootPhase { name: "Asset archives: 1.7 MB read, 1.1 MB LZ-decoded", seconds: 0.70 },
    BootPhase { name: "Sound bank upload to SPU2 (410 KB)", seconds: 0.27 },
    BootPhase { name: "CDVD S-commands, NVRAM config, history read, threads", seconds: 0.10 },
    BootPhase { name: "Video init: GS reset, sync, cleared buffers", seconds: 0.05 },
];

pub const HANDOFF_PHASES: [BootPhase; 7] = [
    BootPhase { name: "OSDSYS: disc thread off, disc key read twice, title ID decoded", seconds: 0.15 },
    BootPhase { name: "OSDSYS: cdrom0:\\SYSTEM.CNF;1 read, BOOT2 checked against the disc ID", seconds: 0.10 },
    BootPhase { name: "OSDSYS: play history updated and saved to the memory card", seconds: 0.20 },
    BootPhase { name: "OSDSYS: subsystems shut down, LoadExecPS2(\"rom0:PS2LOGO\")", seconds: 0.10 },
    BootPhase { name: "KERNEL/EELOAD: PS2LOGO loaded and decompressed to 0x100000", seconds: 0.15 },
    BootPhase { name: "PS2LOGO: rom0:OSDSND loaded, chime bank uploaded, GS set to 640×512", seconds: 0.30 },
    BootPhase { name: "PS2LOGO: logo sectors 0–11 read from the disc and checksummed", seconds: 0.20 },
];

/// The hand-off for a PlayStation disc: OSDSYS to PS1DRV to the PS1 shell's first frame.
pub const PS1_HANDOFF_PHASES: [BootPhase; 7] = [
    BootPhase { name: "OSDSYS: disc thread off; SYSTEM.CNF read, BOOT line → PS1 title ID", seconds: 0.15 },
    BootPhase { name: "OSDSYS: play history updated and saved to the memory card", seconds: 0.20 },
    BootPhase { name: "OSDSYS: subsystems shut down, LoadExecPS2(\"rom0:PS1DRV\", id, ver)", seconds: 0.10 },
    BootPhase { name: "KERNEL/EELOAD: PS1DRV loaded; IOP rebooted into PlayStation mode", seconds: 0.40 },
    BootPhase { name: "TBIN: rom0:LOGO (the PS1 shell) decompressed to 0x30000", seconds: 0.10 },
    BootPhase { name: "PS1 shell: GetID, licence sector 4 and logo sectors 5–11 read and checked", seconds: 0.25 },
    BootPhase { name: "PS1 shell: SPU bank uploaded, reverb set, drone notes keyed", seconds: 0.10 },
];

/// The phase active `elapsed` seconds into a black period of `total` seconds.
pub fn phase_at(phases: &[BootPhase], elapsed: f32, total: f32) -> &BootPhase {
    let sum: f32 = phases.iter().map(|p| p.seconds).sum();
    let scaled = elapsed / total.max(0.01) * sum;
    let mut t = 0.0;
    for p in phases {
        t += p.seconds;
        if scaled < t {
            return p;
        }
    }
    phases.last().unwrap()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Segment {
    PowerOn,
    Opening,
    Handoff,
    Logo,
    End,
}

#[derive(Debug, Clone)]
pub struct Span {
    pub segment: Segment,
    pub start: usize,
    pub length: usize,
}

/// The whole boot as one clock: power-on black → the opening → the hand-off to PS2LOGO →
/// the logo → the point where the game would start. Frames are fields of the video mode.
#[derive(Debug, Clone)]
pub struct BootSequence {
    pub video: VideoMode,
    pub spans: Vec<Span>,
    pub opening: Timeline,
    pub logo: Timeline,
}

impl BootSequence {
    pub fn new(video: VideoMode, power_on_seconds: f32, disc_settled_seconds: f32, handoff_seconds: f32, end_seconds: f32) -> Self {
        let fps = video.fps();
        let opening = Timeline::boot((disc_settled_seconds * fps) as usize, video);
        let logo = Timeline::logo(video);
        let mut spans = Vec::new();
        let mut t = 0;
        let mut add = |segment, length: usize| {
            spans.push(Span { segment, start: t, length });
            t += length;
        };
        add(Segment::PowerOn, (power_on_seconds * fps) as usize);
        add(Segment::Opening, opening.end_frame());
        add(Segment::Handoff, (handoff_seconds * fps) as usize);
        add(Segment::Logo, logo.end_frame());
        add(Segment::End, (end_seconds * fps) as usize);
        Self { video, spans, opening, logo }
    }

    /// The same sequence with the disc-logo segment replaced by `logo` (a PS1 disc's licence screen).
    pub fn with_logo(mut self, logo: Timeline) -> Self {
        let mut t = 0;
        for s in &mut self.spans {
            if s.segment == Segment::Logo { s.length = logo.end_frame() }
            s.start = t;
            t += s.length;
        }
        self.logo = logo;
        self
    }

    pub fn total_frames(&self) -> usize { self.spans.last().map(|s| s.start + s.length).unwrap_or(0) }
    pub fn opening_start(&self) -> usize { self.spans[1].start }
    pub fn logo_start(&self) -> usize { self.spans[3].start }
    pub fn dive_frame(&self) -> usize { self.opening_start() + self.opening.dive_frame }

    pub fn span_at(&self, frame: usize) -> (&Span, usize) {
        for s in &self.spans {
            if frame < s.start + s.length {
                return (s, frame - s.start);
            }
        }
        let last = self.spans.last().unwrap();
        (last, last.length.saturating_sub(1))
    }
}
