//! The opening as functions of the frame number: towers from the play history, the camera
//! timeline, closed-form motion, and the whole boot as one clock.

use crate::bios::OpeningAssets;
use crate::history::PlayHistory;
use crate::logo::LogoAnimation;
use glam::Vec3;
use std::f32::consts::PI;

/// The video mode lives at the crate root since 0.2; this alias is kept for one release.
#[deprecated(since = "0.2.0", note = "use `ps2kit::VideoMode`")]
pub type VideoMode = crate::VideoMode;

/// What a [`Timeline`] describes: one of the screens the console shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SceneKind {
    /// The opening with the towers (`notes/opening.md`), ending in the dive.
    Boot,
    /// The red "insert a disc" screen.
    Warning,
    /// `rom0:PS2LOGO`: the disc's lettering and the animation, then the hold.
    Logo,
    /// The PS1 shell's licence screen shown for a PlayStation disc.
    Ps1Licence,
}

/// One tower of the opening, derived from a history record.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Tower {
    /// Grid column, 0..14.
    pub column: usize,
    /// Grid row, 0..9.
    pub row: usize,
    /// Index into [`PlayHistory::records`] of the title that owns the tower.
    pub record: usize,
    /// World-space centre (already pushed back so the near cap lines up).
    pub centre: Vec3,
    /// Half the tower's length along z (the near cap is at `centre.z - half_length`).
    pub half_length: f32,
    /// Vertex brightness of the near cap in GS units (128 = texture colour unchanged).
    pub brightness: f32,
    /// Still-growing towers sway; finished ones stand still.
    pub growing: bool,
    /// Rotation about the tower's axis in quarter turns, 0..=3.
    pub quarter_turns: i32,
    /// Offset added to both texture coordinates of the wall texture, in texture widths.
    pub uv_offset: f32,
}

/// Half the width of a tower's square cross-section, in world units.
pub const TOWER_HALF_WIDTH: f32 = 2.0;

/// Static scene data built once per (assets, history) pair.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct OpeningScene {
    /// Every tower the history produces, in record order.
    pub towers: Vec<Tower>,
}

impl OpeningScene {
    /// Places a tower for every bit set in each record's mask, the growing one sized by the
    /// record's launch count, as `OSDSYS` does when it builds the scene.
    #[must_use]
    pub fn new(assets: &OpeningAssets, history: &PlayHistory) -> Self {
        let mut towers = Vec::new();
        for (i, rec) in history.records.iter().enumerate() {
            let Some(slots) = assets.slots().get(i) else { break };
            if rec.is_empty() {
                continue;
            }
            let c = rec.count as usize;
            let step = (if c < 14 { c } else { 4 + (c - 14) % 10 }).min(13);
            for (k, slot) in slots.iter().enumerate() {
                let (fill, size) = if k == rec.index as usize {
                    let Some(growth) = assets.growth(step) else { continue };
                    growth
                } else if rec.mask >> k & 1 == 1 {
                    (1.0, 1.0)
                } else {
                    continue;
                };
                let (col, row) = (slot.column, slot.row);
                let Some(m) = assets.tower_position(col, row) else { continue };
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
#[must_use]
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
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CameraState {
    /// Position along the route (the camera is always on the z axis).
    pub z: f32,
    /// Roll about the view axis, radians.
    pub roll: f32,
    /// The console's stage counter: 0..=2 in the opening, 4..=6 in the warning scene.
    pub stage: i32,
    /// Downward tilt of the view direction (the warning scene looks slightly down).
    pub tilt: f32,
}

impl CameraState {
    /// World position.
    #[must_use]
    pub fn position(&self) -> Vec3 { Vec3::new(0.0, 0.0, self.z) }
    /// View direction (not normalised).
    #[must_use]
    pub fn forward(&self) -> Vec3 { Vec3::new(0.0, self.tilt, 1.0) }
    /// Screen-up vector after the roll.
    #[must_use]
    pub fn up(&self) -> Vec3 { Vec3::new(self.roll.sin(), self.roll.cos(), 0.0) }
}

/// The opening's stage machine and camera kinematics, precomputed for every frame.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Timeline {
    /// The video mode the frames are fields of.
    pub video: crate::VideoMode,
    /// Which screen this is.
    pub kind: SceneKind,
    states: Vec<CameraState>,
    /// Frame at which the dive starts (boot) or the exit fade starts (warning).
    pub dive_frame: usize,
}

impl Timeline {
    /// Fields per second of the video mode.
    #[must_use]
    pub fn fps(&self) -> f32 { self.video.fps() }
    /// The camera at every integer frame, from frame 0 to [`end_frame`](Self::end_frame).
    #[must_use]
    pub fn states(&self) -> &[CameraState] { &self.states }
    /// First frame after the scene has ended.
    #[must_use]
    pub fn end_frame(&self) -> usize { self.states.len().saturating_sub(1) }

    /// The boot scene. `disc_settled_frame` is when the drive has finished identifying the
    /// disc (0 = already known); the dive needs this and at least two seconds of drift.
    #[must_use]
    pub fn boot(disc_settled_frame: usize, video: crate::VideoMode) -> Self {
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
    #[must_use]
    pub fn warning(exit_frame: usize, video: crate::VideoMode) -> Self {
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

    /// The logo program: its animated fields ([`LogoAnimation::animated_fields`]) followed by
    /// the [`LogoAnimation::HOLD_FIELDS`] hold. The camera does not move.
    #[must_use]
    pub fn logo(video: crate::VideoMode) -> Self {
        let fields = LogoAnimation::animated_fields(video);
        let animated = (fields.end() - fields.start() + 1) as usize;
        Self { video, kind: SceneKind::Logo, states: vec![CameraState::default(); animated + LogoAnimation::HOLD_FIELDS + 1], dive_frame: 0 }
    }

    /// The PS1 licence screen: `fields` fields followed by a hold while the game loads
    /// (the same [`LogoAnimation::HOLD_FIELDS`] as the PS2 logo). The camera does not move.
    #[must_use]
    pub fn ps1_licence(video: crate::VideoMode, fields: usize) -> Self {
        Self { video, kind: SceneKind::Ps1Licence, states: vec![CameraState::default(); fields + LogoAnimation::HOLD_FIELDS + 1], dive_frame: 0 }
    }

    /// Camera at a (possibly fractional) frame, interpolated between the two neighbouring
    /// states; clamps outside the scene.
    #[must_use]
    pub fn camera(&self, frame: f32) -> CameraState {
        let end = self.end_frame();
        let f = frame.clamp(0.0, end as f32);
        let i = (f as usize).min(end.saturating_sub(1));
        let (Some(&a), Some(&b)) = (self.states.get(i), self.states.get((i + 1).min(end))) else { return CameraState::default() };
        let t = f - i as f32;
        CameraState { z: a.z + (b.z - a.z) * t, roll: a.roll + (b.roll - a.roll) * t, stage: a.stage, tilt: a.tilt }
    }

    /// First frame whose camera has passed depth `z`.
    #[must_use]
    pub fn frame_passing(&self, z: f32) -> usize {
        self.states.iter().position(|s| s.z > z).unwrap_or(self.end_frame())
    }
}

/// Closed-form animation of everything that is not the camera. Frames are fields of the
/// video mode; a fractional frame interpolates for display and is not something the console
/// ever evaluates.
pub mod motion {
    use super::*;

    /// Extra rotation (radians) of a still-growing tower: ±10°, 360-frame period.
    #[must_use]
    pub fn sway(frame: f32) -> f32 { ((frame % 360.0 - 180.0) * PI / 180.0).sin() * 10.0 * PI / 180.0 }

    /// World position of light orb `i` (0..=3).
    #[must_use]
    pub fn orb_position(i: usize, frame: f32, seed: f32) -> Vec3 {
        let k = (i + 10) as f32;
        let a = (frame + seed + 17.0 * i as f32) * 0.01 * k * 0.1;
        let b = (frame + seed + 15.0 * i as f32) * 0.005 * k * 0.1;
        Vec3::new((10 - i as i32) as f32 * a.cos(), (i + 3) as f32 * b.sin(), a.cos() * 12.0 + 88.0)
    }

    /// Texture scroll of fog layer `i` (0..=5), in texture widths.
    #[must_use]
    pub fn fog_scroll(i: usize, frame: f32) -> f32 { ((14 - i) as f32 * 0.0001 * (i + 1) as f32 * 0.5 * frame) % 1.0 }

    /// Euler angles of glass cube `i` (0..=4).
    #[must_use]
    pub fn cube_rotation(i: usize, frame: f32) -> Vec3 {
        let s = if i == 2 { 0.9 } else { (i as f32 - 2.0) * 0.8 };
        let start = s * (i % 3) as f32 * 3.7 + 0.285599;
        Vec3::splat(start) + Vec3::new(0.0031 / s, s * 0.0022, s / 1000.0 + 0.0013) * frame
    }

    /// Opacity (0..=1) of the lettering, `t` frames after the camera passed z = 18.
    #[must_use]
    pub fn lettering_alpha(t: f32) -> f32 {
        if t <= 0.0 || t >= 120.0 { return 0.0 }
        let counter = if t <= 60.0 { 4.0 * t } else { 480.0 - 4.0 * t };
        counter.min(112.0) / 128.0
    }

    /// Opacity (0..=1) of the fade to black at camera depth `z`: starts at z = 72, full 32 later.
    #[must_use]
    pub fn fade_alpha(z: f32) -> f32 { if z > 72.0 { ((z - 72.0) * 4.0).min(128.0) / 128.0 } else { 0.0 } }
    /// Defocus passes (0..=3) over the picture at camera depth `z`: one more every 12 units past z = 56.
    #[must_use]
    pub fn defocus_passes(z: f32) -> usize { if z > 56.0 { (((z - 56.0) / 12.0) as usize).min(3) } else { 0 } }

    /// Brightness (GS units, 0..=76.8) of the warning scene's light source at camera depth `z`.
    #[must_use]
    pub fn warning_brightness(z: f32) -> f32 { (740.0 - (1160.0 - z)) * 128.0 / 740.0 * 0.6 }
    /// Light-source disc `i` (0..=6): orbit position and the (wildly spinning) rotation vector.
    #[must_use]
    pub fn warning_disc(i: usize, frame: f32) -> (Vec3, Vec3) {
        let n = (i + 1) as f32;
        let k = ((7 - i) * (7 - i)) as f32 * 2.0 * PI / 64.0;
        let r = k / 2.0;
        let phi = frame % 201.0 / 32.0 - PI + k;
        let base = Vec3::new(0.0, 0.0, i as f32 * 0.925 * 2.0 * PI / 7.0);
        (Vec3::new(r * phi.cos(), r * phi.sin(), 1160.0), base + Vec3::new(0.2, 0.27, 0.35) * n * frame)
    }
    /// Opacity (0..=1) of the warning scene's black overlay while the camera dollies in from z = 672.
    #[must_use]
    pub fn warning_fade_in(z: f32) -> f32 { if z < 800.0 { ((128.0 - (z - 672.0)) / 128.0).clamp(0.0, 1.0) } else { 0.0 } }
    /// Glass prism `i` (0..=4) of the warning scene: Euler angles at `frame`.
    #[must_use]
    pub fn prism_rotation(i: usize, frame: f32) -> Vec3 {
        let s = if i == 2 { 0.9 } else { (i as f32 - 2.0) * 0.8 };
        let start = Vec3::new(s * ((2 * i) % 9) as f32 / 4.0, s * ((2 * i) % 8) as f32 / 5.0, s * ((2 * i) % 7) as f32 / 6.0);
        start + Vec3::new(0.004 / s, 0.003 * s, s / 800.0 + 0.002) * frame
    }
}

/// What the console is busy with while the screen is black: after power-on, and during the
/// hand-off from the opening to the disc's program. Durations are the mid-points of the
/// estimates in `notes/boot_sequence.md`; the captions are the caller's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum BootPhase {
    /// IOP boot #1: IOPBOOT and 29 modules from ROM.
    IopBoot,
    /// EELOAD loads `rom0:OSDSYS` (363 KB).
    OsdsysLoad,
    /// The OSDSYS stub decompresses itself to 0x200000.
    OsdsysUnpack,
    /// IOP boot #2: `rom0:UDNL rom0:OSDCNF`, 39 modules.
    IopReboot,
    /// Memory card mount (`sceMcGetInfo`, the system folder).
    CardMount,
    /// Asset archives: 1.7 MB read, 1.1 MB LZ-decoded.
    AssetArchives,
    /// Sound bank upload to SPU2 (410 KB).
    SoundUpload,
    /// CDVD S-commands, NVRAM configuration, history read, thread start.
    CdvdSetup,
    /// Video initialisation: GS reset, sync, cleared buffers.
    VideoInit,

    /// OSDSYS: disc thread off, disc key read twice, title ID decoded.
    DiscIdentified,
    /// OSDSYS: `cdrom0:\SYSTEM.CNF;1` read, `BOOT2` checked against the disc ID.
    SystemCnfRead,
    /// OSDSYS: play history updated and saved to the memory card.
    HistorySaved,
    /// OSDSYS: subsystems shut down, `LoadExecPS2("rom0:PS2LOGO")`.
    Ps2LogoExec,
    /// KERNEL/EELOAD: PS2LOGO loaded and decompressed to 0x100000.
    Ps2LogoLoad,
    /// PS2LOGO: `rom0:OSDSND` loaded, chime bank uploaded, GS set to 640x512.
    Ps2LogoInit,
    /// PS2LOGO: logo sectors 0–11 read from the disc and checksummed.
    LogoSectorsRead,

    /// OSDSYS: disc thread off; `SYSTEM.CNF` read, `BOOT` line → PS1 title ID.
    Ps1BootId,
    /// OSDSYS: subsystems shut down, `LoadExecPS2("rom0:PS1DRV", id, ver)`.
    Ps1DrvExec,
    /// KERNEL/EELOAD: PS1DRV loaded; IOP rebooted into PlayStation mode.
    Ps1DrvLoad,
    /// TBIN: `rom0:LOGO` (the PS1 shell) decompressed to 0x30000.
    Ps1ShellUnpack,
    /// PS1 shell: GetID, licence sector 4 and logo sectors 5–11 read and checked.
    Ps1SystemArea,
    /// PS1 shell: SPU bank uploaded, reverb set, drone notes keyed.
    Ps1SoundSetup,
}

impl BootPhase {
    /// How long the phase takes, in seconds.
    #[must_use]
    pub fn seconds(self) -> f32 {
        match self {
            Self::IopBoot => 0.55,
            Self::OsdsysLoad => 0.20,
            Self::OsdsysUnpack => 0.06,
            Self::IopReboot => 0.85,
            Self::CardMount => 0.30,
            Self::AssetArchives => 0.70,
            Self::SoundUpload => 0.27,
            Self::CdvdSetup => 0.10,
            Self::VideoInit => 0.05,
            Self::DiscIdentified => 0.15,
            Self::SystemCnfRead => 0.10,
            Self::HistorySaved => 0.20,
            Self::Ps2LogoExec => 0.10,
            Self::Ps2LogoLoad => 0.15,
            Self::Ps2LogoInit => 0.30,
            Self::LogoSectorsRead => 0.20,
            Self::Ps1BootId => 0.15,
            Self::Ps1DrvExec => 0.10,
            Self::Ps1DrvLoad => 0.40,
            Self::Ps1ShellUnpack => 0.10,
            Self::Ps1SystemArea => 0.25,
            Self::Ps1SoundSetup => 0.10,
        }
    }
}

/// From power-on to the first frame of the opening, in order.
pub const POWER_ON_PHASES: [BootPhase; 9] = [
    BootPhase::IopBoot,
    BootPhase::OsdsysLoad,
    BootPhase::OsdsysUnpack,
    BootPhase::IopReboot,
    BootPhase::CardMount,
    BootPhase::AssetArchives,
    BootPhase::SoundUpload,
    BootPhase::CdvdSetup,
    BootPhase::VideoInit,
];

/// From the end of the opening to the first frame of PS2LOGO, in order.
pub const HANDOFF_PHASES: [BootPhase; 7] = [
    BootPhase::DiscIdentified,
    BootPhase::SystemCnfRead,
    BootPhase::HistorySaved,
    BootPhase::Ps2LogoExec,
    BootPhase::Ps2LogoLoad,
    BootPhase::Ps2LogoInit,
    BootPhase::LogoSectorsRead,
];

/// The hand-off for a PlayStation disc: OSDSYS to PS1DRV to the PS1 shell's first frame.
pub const PS1_HANDOFF_PHASES: [BootPhase; 7] = [
    BootPhase::Ps1BootId,
    BootPhase::HistorySaved,
    BootPhase::Ps1DrvExec,
    BootPhase::Ps1DrvLoad,
    BootPhase::Ps1ShellUnpack,
    BootPhase::Ps1SystemArea,
    BootPhase::Ps1SoundSetup,
];

/// The phase active `elapsed` seconds into a black period of `total` seconds (the phases'
/// durations are scaled to fill `total`); `None` for an empty list.
#[must_use]
pub fn phase_at(phases: &[BootPhase], elapsed: f32, total: f32) -> Option<BootPhase> {
    let sum: f32 = phases.iter().map(|p| p.seconds()).sum();
    let scaled = elapsed / total.max(0.01) * sum;
    let mut t = 0.0;
    for &p in phases {
        t += p.seconds();
        if scaled < t {
            return Some(p);
        }
    }
    phases.last().copied()
}

/// The parts of a [`BootSequence`], in order; a sequence always has all five.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Segment {
    /// Black screen from power-on ([`POWER_ON_PHASES`]).
    PowerOn,
    /// The opening ([`Timeline::boot`]).
    Opening,
    /// Black screen while OSDSYS hands over ([`HANDOFF_PHASES`] or [`PS1_HANDOFF_PHASES`]).
    Handoff,
    /// The disc's logo or the PS1 licence screen ([`BootSequence::logo`]).
    Logo,
    /// The point where the game would start; the sequence holds here.
    End,
}

/// One segment's place on the sequence clock.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Span {
    /// Which segment.
    pub segment: Segment,
    /// First frame.
    pub start: usize,
    /// Number of frames.
    pub length: usize,
}

/// The whole boot as one clock: power-on black → the opening → the hand-off to PS2LOGO →
/// the logo → the point where the game would start. Frames are fields of the video mode.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct BootSequence {
    /// The video mode the frames are fields of.
    pub video: crate::VideoMode,
    spans: Vec<Span>,
    /// The opening's timeline (frames local to its segment).
    pub opening: Timeline,
    /// The logo's (or the PS1 licence screen's) timeline (frames local to its segment).
    pub logo: Timeline,
}

impl BootSequence {
    /// Builds the five segments from the black periods' lengths in seconds and the frame at
    /// which the drive settles (`disc_settled_seconds`, see [`Timeline::boot`]).
    #[must_use]
    pub fn new(video: crate::VideoMode, power_on_seconds: f32, disc_settled_seconds: f32, handoff_seconds: f32, end_seconds: f32) -> Self {
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
    #[must_use]
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

    /// The five segments in order, each starting where the previous one ends.
    #[must_use]
    pub fn spans(&self) -> &[Span] { &self.spans }
    /// Frames in the whole sequence.
    #[must_use]
    pub fn total_frames(&self) -> usize { self.spans.last().map(|s| s.start + s.length).unwrap_or(0) }
    /// First frame of a segment.
    #[must_use]
    pub fn start_of(&self, segment: Segment) -> usize { self.spans.iter().find(|s| s.segment == segment).map(|s| s.start).unwrap_or(0) }
    /// First frame of the opening.
    #[must_use]
    pub fn opening_start(&self) -> usize { self.start_of(Segment::Opening) }
    /// First frame of the logo (or licence screen).
    #[must_use]
    pub fn logo_start(&self) -> usize { self.start_of(Segment::Logo) }
    /// The frame, on the sequence clock, at which the opening's dive starts.
    #[must_use]
    pub fn dive_frame(&self) -> usize { self.opening_start() + self.opening.dive_frame }

    /// The segment at `frame` and the frame within it; past the end, the last frame of the
    /// last segment.
    #[must_use]
    pub fn span_at(&self, frame: usize) -> (&Span, usize) {
        for s in &self.spans {
            if frame < s.start + s.length {
                return (s, frame - s.start);
            }
        }
        // `spans` is private and `new` always pushes five, so this never fails.
        let last = self.spans.last().expect("a boot sequence always has its five segments");
        (last, last.length.saturating_sub(1))
    }
}
