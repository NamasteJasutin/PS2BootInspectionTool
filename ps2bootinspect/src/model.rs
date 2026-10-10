//! App state: the user's files, the play history in use, the clock, and view settings.

use crate::audio::{Analysis, AudioPlayer, Snapshot, VisualizerMode};
use crate::renderer::{RenderOptions, TowerTint};
use glam::Vec3;
use ps2kit::bios::OpeningAssets;
use ps2kit::disc::{boot_outcome, handoff_steps, DiscImage, HandoffStep};
use ps2kit::history::PlayHistory;
use ps2kit::logo::{DiscLogo, LogoAnimation, LogoAssets, LogoBitmap};
use ps2kit::memcard::MemoryCard;
use ps2kit::ps1::Ps1Shell;
use ps2kit::ps1bios::{Ps1Bios, Ps1SetIssue};
use ps2kit::ps1boot::{ps1_boot_steps, Ps1BootStep, Ps1SystemCnf};
use ps2kit::pup::Pup;
use ps2kit::rom::RomDir;
use ps2kit::sim::{phase_at, BootOutcome, BootPhase, BootPlan, BootSequence, CameraState, OpeningScene, Segment, Timeline, HANDOFF_PHASES, POWER_ON_PHASES, PS1_HANDOFF_PHASES};
use ps2kit::sound::BootSound;
use ps2kit::{Region, VideoMode};
use std::path::{Path, PathBuf};
use std::sync::mpsc;

/// What the viewport shows: one of the console's screens on its own, or the whole boot.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Scene { Full, Boot, Warning, Logo }

impl Scene {
    pub const ALL: [Self; 4] = [Self::Full, Self::Boot, Self::Warning, Self::Logo];
    pub fn name(self) -> &'static str {
        match self {
            Self::Full => "Full boot: BIOS → disc → hand-off",
            Self::Boot => "Boot (towers)",
            Self::Warning => "Warning (insert disc)",
            Self::Logo => "PlayStation 2 logo (disc boot)",
        }
    }
}

pub fn video_mode_name(video: VideoMode) -> &'static str { if video == VideoMode::Pal { "PAL 50 Hz" } else { "NTSC 60 Hz" } }

/// The sidebar's tabs.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab { Boot, SaveData, Disc, Bios }

/// Which console's boot the picture plays.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Console { Ps2, Ps1 }

impl Console {
    pub fn name(self) -> &'static str { match self { Self::Ps2 => "PlayStation 2", Self::Ps1 => "PlayStation" } }
}

/// A BIOS file found in the BIOS folder, offered in the pickers.
#[derive(Clone, Debug)]
pub struct ShelfEntry {
    pub path: PathBuf,
    pub console: Console,
    /// Version and region as read from the file, then the file name.
    pub label: String,
}

impl Tab {
    pub const ALL: [Self; 4] = [Self::Boot, Self::SaveData, Self::Disc, Self::Bios];
    pub fn name(self) -> &'static str {
        match self { Self::Boot => "Boot", Self::SaveData => "Save data", Self::Disc => "Disc", Self::Bios => "BIOS" }
    }
}

/// What the drive is told it found, overriding the loaded image (the scenario bar).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DiscOverride { AsLoaded, NoDisc, Illegal }

impl DiscOverride {
    pub const ALL: [Self; 3] = [Self::AsLoaded, Self::NoDisc, Self::Illegal];
    pub fn name(self) -> &'static str {
        match self { Self::AsLoaded => "the loaded image", Self::NoDisc => "no disc (tray empty)", Self::Illegal => "an illegal disc (state 0x74)" }
    }
}

/// A predetermined camera path (beyond the PS2), or the console's own.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CameraMode { Scripted, Orbit, Tornado, Crane, FigureEight, Zenith }

impl CameraMode {
    pub const ALL: [Self; 6] = [Self::Scripted, Self::Orbit, Self::Tornado, Self::Crane, Self::FigureEight, Self::Zenith];
    pub fn name(self) -> &'static str {
        match self {
            Self::Scripted => "the console's camera",
            Self::Orbit => "orbit around the towers",
            Self::Tornado => "tornado (tightening helix)",
            Self::Crane => "crane: street level to overhead",
            Self::FigureEight => "figure-eight through the field",
            Self::Zenith => "zenith: straight down (Wii-style grid)",
        }
    }
    pub fn describe(self) -> &'static str {
        match self {
            Self::Scripted => "The route OSDSYS integrates every field (notes/opening.md).",
            Self::Orbit => "The console's camera dives down the city's axis (Y+ → Y−). A level circle around that axis at the chosen height, aimed at the focus point up the towers.",
            Self::Tornado => "A funnel around the axis: wide at the start's height, tightening as it descends to the base, looking at the centre.",
            Self::Crane => "Street level at the city's edge, rising onto the axis above the towers — the console's own vantage — and back.",
            Self::FigureEight => "A level lemniscate at the chosen height, looking down at the centre.",
            Self::Zenith => "On the axis above the city, held still: the console's start without the dive.",
        }
    }

    /// The view at `frame` of the scene, turning `degrees_per_frame` around `centre`, at
    /// `radius` (the distance from the centre to where the console's own camera starts) and
    /// `height` (fraction of the radius above the centre, for the paths that stay level).
    /// `vertical` is the scene's down direction: +z for the city (the console starts above the
    /// tower tops and dives down the axis), +y for the logo screens (the console looks at them
    /// from the front, at height 0). `front` is where the lap starts: a horizontal direction.
    pub fn view(self, frame: f32, degrees_per_frame: f32, centre: Vec3, radius: f32, height: f32, vertical: Vec3, front: Vec3, video: VideoMode) -> Option<crate::renderer::ViewCamera> {
        use std::f32::consts::TAU;
        let a = (degrees_per_frame * frame).to_radians();
        let lap = (a / TAU).rem_euclid(1.0);
        let eased = (lap * TAU).cos() * -0.5 + 0.5;                  // 0 → 1 → 0 over a lap
        let down = vertical.normalize_or(Vec3::Y);
        let up = -down;
        let front = (front - down * front.dot(down)).normalize_or(Vec3::X);
        let side = down.cross(front).normalize_or(Vec3::X);
        let ring = |angle: f32, r: f32| (front * angle.cos() + side * angle.sin()) * r;
        let position = match self {
            Self::Scripted => return None,
            // A level circle around the vertical axis at `height`.
            Self::Orbit => centre + up * (height * radius) + ring(a, radius),
            // A funnel: wide at the start's height, tightening as it descends to the base.
            Self::Tornado => centre + up * (radius * (1.0 - eased) * height.max(0.3) * 2.0) + ring(3.0 * a, radius * (1.0 - 0.85 * eased).max(0.1)),
            // Street level at the edge, rising onto the axis above the city, and back.
            Self::Crane => centre + up * (radius * (0.05 + 0.95 * eased)) + ring(0.0, radius * (1.0 - 0.95 * eased)),
            // A level lemniscate at `height`, looking down at the centre.
            Self::FigureEight => centre + up * (height * radius) + front * (radius * a.sin()) + side * (radius * 0.5 * (2.0 * a).sin()),
            // On the axis above the city: the console's own vantage, held still.
            Self::Zenith => centre + up * radius + front * (0.001 * radius),
        };
        let forward = (centre - position).normalize_or(down);
        // `up` is the screen-down direction in this renderer (+Y is down the console's screen):
        // the scene's vertical projected into the view plane; looking along the vertical,
        // fall back to the lap's front so the picture keeps its bearings.
        let proj = down - forward * forward.dot(down);
        let screen_down = if proj.length() < 0.05 { (front - forward * forward.dot(front)).normalize_or(Vec3::Y) } else { proj.normalize() };
        Some(crate::renderer::ViewCamera { position, forward, up: screen_down, video, zoom: 1.0 })
    }
}

/// The dials of a predetermined camera path; each path keeps its own.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct PathParams {
    /// Degrees per frame along the path.
    pub speed: f32,
    /// Distance from the centre, as a multiple of the console's start distance.
    pub distance: f32,
    /// Where the path aims, as a fraction of the way up from the towers' floor to the highest top.
    pub focus: f32,
    /// Height of a level path above the aim point, as a fraction of the radius.
    pub height: f32,
    /// Lens relative to the console's (1 = the same).
    pub zoom: f32,
}

impl CameraMode {
    /// The settings a path starts with (tuned by hand on the 2.00 E opening).
    pub fn default_params(self) -> PathParams {
        match self {
            Self::Tornado => PathParams { speed: 0.55, distance: 2.25, focus: 0.64, height: 0.68, zoom: 0.285 },
            _ => PathParams { speed: 1.25, distance: 0.4, focus: 0.76, height: 0.78, zoom: 0.39 },
        }
    }
    /// Index into [`Model::path_params`].
    pub fn index(self) -> usize { Self::ALL.iter().position(|m| *m == self).unwrap_or(0) }
}

/// The console regions a scenario can pretend to be.
pub const REGIONS: [(Option<Region>, &str); 5] = [(None, "as the BIOS says"), (Some(Region::Japan), "J — Japan"), (Some(Region::America), "A — America"), (Some(Region::Europe), "E — Europe"), (Some(Region::China), "C — China")];

/// Today as the history file encodes a date (`day | month << 5 | (year - 2000) << 9`).
pub fn today_date() -> u16 {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    // Civil-from-days (Howard Hinnant), good for the file's 2000..2127 range.
    let z = (secs / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u16;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u16;
    let year = (yoe + era * 400 + i64::from(month <= 2)) as u16;
    day | month << 5 | year.saturating_sub(2000).min(127) << 9
}

pub fn outcome_name(outcome: BootOutcome) -> &'static str {
    match outcome {
        BootOutcome::Game => "the PlayStation 2 logo, then the game",
        BootOutcome::Ps1Game => "the PS1 licence screen, then the game",
        BootOutcome::Menu => "the clock / main menu (no disc to start)",
        BootOutcome::Warning => "the warning scene (red screen)",
        _ => "?",
    }
}

/// The caption shown while the console is busy with a phase (`notes/boot_sequence.md`).
pub fn phase_caption(phase: BootPhase) -> &'static str {
    match phase {
        BootPhase::IopBoot => "IOP boot #1: IOPBOOT + 29 modules from ROM",
        BootPhase::OsdsysLoad => "EELOAD loads rom0:OSDSYS (363 KB)",
        BootPhase::OsdsysUnpack => "OSDSYS stub decompresses itself to 0x200000",
        BootPhase::IopReboot => "IOP boot #2: rom0:UDNL rom0:OSDCNF, 39 modules",
        BootPhase::CardMount => "Memory card mount (sceMcGetInfo, system folder)",
        BootPhase::AssetArchives => "Asset archives: 1.7 MB read, 1.1 MB LZ-decoded",
        BootPhase::SoundUpload => "Sound bank upload to SPU2 (410 KB)",
        BootPhase::CdvdSetup => "CDVD S-commands, NVRAM config, history read, threads",
        BootPhase::VideoInit => "Video init: GS reset, sync, cleared buffers",
        BootPhase::DiscIdentified => "OSDSYS: disc thread off, disc key read twice, title ID decoded",
        BootPhase::SystemCnfRead => "OSDSYS: cdrom0:\\SYSTEM.CNF;1 read, BOOT2 checked against the disc ID",
        BootPhase::HistorySaved => "OSDSYS: play history updated and saved to the memory card",
        BootPhase::Ps2LogoExec => "OSDSYS: subsystems shut down, LoadExecPS2(\"rom0:PS2LOGO\")",
        BootPhase::Ps2LogoLoad => "KERNEL/EELOAD: PS2LOGO loaded and decompressed to 0x100000",
        BootPhase::Ps2LogoInit => "PS2LOGO: rom0:OSDSND loaded, chime bank uploaded, GS set to 640×512",
        BootPhase::LogoSectorsRead => "PS2LOGO: logo sectors 0–11 read from the disc and checksummed",
        BootPhase::Ps1BootId => "OSDSYS: disc thread off; SYSTEM.CNF read, BOOT line → PS1 title ID",
        BootPhase::Ps1DrvExec => "OSDSYS: subsystems shut down, LoadExecPS2(\"rom0:PS1DRV\", id, ver)",
        BootPhase::Ps1DrvLoad => "KERNEL/EELOAD: PS1DRV loaded; IOP rebooted into PlayStation mode",
        BootPhase::Ps1ShellUnpack => "TBIN: rom0:LOGO (the PS1 shell) decompressed to 0x30000",
        BootPhase::Ps1SystemArea => "PS1 shell: GetID, licence sector 4 and logo sectors 5–11 read and checked",
        BootPhase::Ps1SoundSetup => "PS1 shell: SPU bank uploaded, reverb set, drone notes keyed",
        _ => "…",
    }
}

/// Shown at the end of the full sequence, where the game would start.
const END_CAPTION: &str = "SPU quit; LoadExecPS2(boot ELF) — the game would start here";
const MENU_CAPTION: &str = "OSDSYS: clock / main-menu module woken (ctx[0x5E8] = 2) — not re-created here";
const PS1_MENU_CAPTION: &str = "PS1 shell: no PlayStation disc — the memory-card / CD-player menu opens (not re-created here)";
const PS1_INTRO_CAPTION: &str = "PS1 shell: the SCE intro — grey fade, diamond, logotype (being built)";
const WARNING_END_CAPTION: &str = "the drive reported a change: the warning scene faded and OSDSYS looks at the disc again";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HistorySource { Card, Saves, Custom, Empty }

impl HistorySource {
    pub const ALL: [Self; 4] = [Self::Card, Self::Saves, Self::Custom, Self::Empty];
    pub fn name(self) -> &'static str {
        match self { Self::Card => "History file on card", Self::Saves => "Simulated from saves on card", Self::Custom => "Simulated", Self::Empty => "Empty (new console)" }
    }
}

pub const LANGUAGES: [(&str, &str); 12] = [("J", "Japanese"), ("E", "English"), ("F", "French"), ("S", "Spanish"), ("G", "German"), ("I", "Italian"), ("D", "Dutch"), ("P", "Portuguese"), ("R", "Russian"), ("K", "Korean"), ("H", "Chinese (traditional)"), ("C", "Chinese (simplified)")];

/// Blender-style viewport camera: a pivot point, a distance, and a turntable yaw/pitch.
#[derive(Clone, Copy, Debug)]
pub struct FreeCamera {
    pub pivot: Vec3,
    pub distance: f32,
    pub yaw: f32,
    pub pitch: f32,
}

impl FreeCamera {
    /// Start behind the scripted camera, looking at the field it looks at.
    pub fn behind(c: &CameraState) -> Self {
        let distance = 60.0;
        Self { pivot: c.position() + c.forward() * distance, distance, yaw: 0.0, pitch: 0.0 }
    }
    pub fn forward(&self) -> Vec3 { Vec3::new(self.yaw.sin() * self.pitch.cos(), self.pitch.sin(), self.yaw.cos() * self.pitch.cos()) }
    pub fn position(&self) -> Vec3 { self.pivot - self.forward() * self.distance }
    pub fn view(&self, video: VideoMode) -> crate::renderer::ViewCamera {
        crate::renderer::ViewCamera { position: self.position(), forward: self.forward(), up: Vec3::Y, video, zoom: 1.0 }
    }
    pub fn orbit(&mut self, dx: f32, dy: f32) { self.yaw += dx * 0.006; self.pitch = (self.pitch - dy * 0.006).clamp(-1.55, 1.55) }
    /// Slides the pivot in the view plane, scaled so that a drag follows the pointer.
    pub fn pan(&mut self, dx: f32, dy: f32, video: VideoMode) {
        let (bx, by, _) = self.view(video).basis();
        let k = self.distance * 0.0025;
        self.pivot -= bx * (dx * k) + by * (dy * k);
    }
    /// Zooms towards the pivot; `steps` > 0 moves closer.
    pub fn dolly(&mut self, steps: f32) { self.distance = (self.distance * 0.9f32.powf(steps)).clamp(1.0, 2000.0) }
    /// Numpad views: 1 front, 3 right, 7 top; `opposite` gives back/left/bottom.
    pub fn snap(&mut self, view: u8, opposite: bool) {
        use std::f32::consts::{FRAC_PI_2, PI};
        match view {
            1 => { self.yaw = if opposite { PI } else { 0.0 }; self.pitch = 0.0 }
            3 => { self.yaw = if opposite { -FRAC_PI_2 } else { FRAC_PI_2 }; self.pitch = 0.0 }
            7 => { self.pitch = if opposite { 1.55 } else { -1.55 }; self.yaw = 0.0 }
            _ => {}
        }
    }
    pub fn frame_all(&mut self) { self.pivot = Vec3::new(0.0, 0.0, 150.0); self.distance = 160.0 }
}

pub struct Model {
    pub bios_path: Option<PathBuf>,
    pub bios_status: String,
    pub card_path: Option<PathBuf>,
    pub card_status: String,
    pub disc_path: Option<PathBuf>,
    pub disc_status: String,
    pub history_source: HistorySource,
    pub custom_titles: u32,
    pub custom_launches: u32,
    pub history: PlayHistory,
    /// Alternate history: the title typed into the Save-data tab, and the launches applied
    /// since the last rebuild (newest last) so they can be undone.
    pub launch_title: String,
    pub launch_log: Vec<(String, PlayHistory)>,
    launch_rng: ps2kit::history::SplitMix,
    pub scene_kind: Scene,
    pub tab: Tab,
    pub disc_override: DiscOverride,
    pub region_override: Option<Region>,
    /// Play the outcome a real console would reach (region lock) rather than the logo regardless.
    pub enforce_checks: bool,
    pub video: VideoMode,
    pub language: &'static str,
    pub power_on_seconds: f32,
    pub disc_seconds: f32,
    pub handoff_seconds: f32,
    pub warning_exit_seconds: f32,
    pub frame: f32,
    pub playing: bool,
    pub looping: bool,
    pub speed: f64,
    pub options: RenderOptions,
    pub free_camera_enabled: bool,
    /// The record under the pointer in the picture (the tower inspector).
    pub hovered_record: Option<usize>,
    pub free_camera: FreeCamera,
    pub camera_mode: CameraMode,
    /// One set of dials per [`CameraMode`] (index by `CameraMode::index`).
    pub path_params: [PathParams; 6],
    /// Lens of the free camera relative to the console's (1 = the same).
    pub free_zoom: f32,
    pub sound_enabled: bool,
    pub sound_volume: f32,
    pub sound_status: String,
    pub visualizer: VisualizerMode,
    pub snapshot: Snapshot,
    pub timeline: Timeline,
    pub sequence: BootSequence,
    pub assets: Option<OpeningAssets>,
    pub rom: Option<RomDir>,
    pub scene: OpeningScene,
    pub logo_assets: Option<LogoAssets>,
    /// The PS1 shell the picture uses: the PS2's `rom0:LOGO` or the standalone dump's shell,
    /// per [`Console`].
    pub ps1_shell: Option<Ps1Shell>,
    /// Which console boots.
    pub console: Console,
    /// The PS2 BIOS's PS1 shell (`rom0:LOGO`).
    ps2_ps1_shell: Option<Ps1Shell>,
    /// A standalone PS1 / PSone / POPS BIOS and its shell.
    pub ps1_bios: Option<Ps1Bios>,
    standalone_ps1_shell: Option<Ps1Shell>,
    pub ps1_bios_path: Option<PathBuf>,
    pub ps1_bios_status: String,
    /// Every BIOS file in the BIOS folder, PS2 and PS1, for the pickers.
    pub shelf: Vec<ShelfEntry>,
    /// Duplicates and file-name/letter disagreements among the PS1 dumps on the shelf.
    pub ps1_set_issues: Vec<Ps1SetIssue>,
    /// A PS3 system update found next to the BIOS files: what its plaintext container says
    /// (read once; the contents are encrypted and never read).
    pub pup_summary: Vec<String>,
    /// The logo model read from a PlayStation disc's sectors 5–11.
    pub ps1_logo: Option<ps2kit::ps1::Tmd>,
    ps2_logo_chime: Vec<f32>,
    pub disc: Option<DiscImage>,
    pub disc_logo: Option<DiscLogo>,
    pub audio: AudioPlayer,
    pub assets_version: u32,
    pub logo_version: u32,
    pub cpu_ms: f64,
    analysis: Analysis,
    card: Option<MemoryCard>,
    card_history: Option<PlayHistory>,
    sound_rx: Option<mpsc::Receiver<Result<(BootSound, Vec<f32>), String>>>,
}

impl Model {
    pub fn new() -> Self {
        let video = VideoMode::Ntsc;
        Self {
            bios_path: None, bios_status: "No BIOS loaded".into(),
            card_path: None, card_status: "No memory card loaded".into(),
            disc_path: None, disc_status: "No disc image — the lettering is filled from the BIOS outline".into(),
            history_source: HistorySource::Empty, custom_titles: 12, custom_launches: 30, history: PlayHistory::default(),
            launch_title: "SLES_000.00".into(), launch_log: Vec::new(), launch_rng: ps2kit::history::SplitMix(0x5EED),
            scene_kind: Scene::Full, tab: Tab::Boot, disc_override: DiscOverride::AsLoaded, region_override: None, enforce_checks: true, video, language: "E",
            power_on_seconds: 3.0, disc_seconds: 0.0, handoff_seconds: 1.2, warning_exit_seconds: 10.0,
            frame: 0.0, playing: true, looping: true, speed: 1.0,
            options: RenderOptions::default(), free_camera_enabled: false, hovered_record: None, camera_mode: CameraMode::Scripted, path_params: CameraMode::ALL.map(CameraMode::default_params), free_zoom: 0.39, free_camera: FreeCamera { pivot: Vec3::new(0.0, 0.0, 120.0), distance: 100.0, yaw: 0.0, pitch: 0.0 },
            sound_enabled: true, sound_volume: 0.8, sound_status: "No sound loaded".into(), visualizer: VisualizerMode::Equalizer, snapshot: Snapshot::default(),
            timeline: Timeline::boot(0, video), sequence: BootSequence::new(video, 3.0, 0.0, 1.2, 6.0),
            assets: None, rom: None, scene: OpeningScene::default(), logo_assets: None, ps1_shell: None, console: Console::Ps2, ps2_ps1_shell: None, ps1_bios: None, standalone_ps1_shell: None, ps1_bios_path: None, ps1_bios_status: "No PS1 BIOS loaded".into(), shelf: Vec::new(), ps1_set_issues: Vec::new(), pup_summary: Vec::new(), ps1_logo: None, ps2_logo_chime: Vec::new(), disc: None, disc_logo: None,
            audio: AudioPlayer::new(), assets_version: 0, logo_version: 0, cpu_ms: 0.0,
            analysis: Analysis::new(), card: None, card_history: None, sound_rx: None,
        }
    }

    pub fn pcsx2_dir() -> Option<PathBuf> {
        let home = dirs::home_dir()?;
        let candidates = [home.join("Library/Application Support/PCSX2"), home.join("Documents/PCSX2"), home.join(".config/PCSX2")];
        candidates.into_iter().find(|p| p.is_dir())
    }

    fn files(dir: &Path) -> Vec<PathBuf> {
        let mut v: Vec<PathBuf> = std::fs::read_dir(dir).map(|d| d.filter_map(Result::ok).map(|e| e.path()).collect()).unwrap_or_default();
        v.sort();
        v
    }

    pub fn load_defaults(&mut self) {
        let pcsx2 = Self::pcsx2_dir();
        let bios_dir = pcsx2.as_ref().map(|p| p.join("bios"));
        let files = bios_dir.map(|d| Self::files(&d)).unwrap_or_default();
        self.scan_shelf(&files);
        for p in &files {
            if self.assets.is_none() && self.shelf.iter().any(|e| &e.path == p && e.console == Console::Ps2) { self.load_bios(p, true) }
        }
        // A retail PS1 BIOS by default; POPS only when it is the only one.
        let ps1: Vec<PathBuf> = self.shelf.iter().filter(|e| e.console == Console::Ps1).map(|e| e.path.clone()).collect();
        for p in ps1.iter().filter(|p| !p.to_string_lossy().to_ascii_uppercase().contains("PSXONPSP")).chain(ps1.iter()) {
            if self.ps1_bios.is_none() { self.load_bios(p, true) }
        }
        if let Some(p) = files.iter().find(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pup"))) { self.pup_summary = Pup::open(p).map(|pup| Self::pup_summary(&pup, p)).unwrap_or_default() }
        for p in pcsx2.as_ref().map(|p| Self::files(&p.join("memcards"))).unwrap_or_default() {
            if self.card.is_none() { self.load_card(&p, true) }
        }
        if let Some(home) = dirs::home_dir() {
            // Images directly in ~/PS2ISO or up to two folders down (how rips usually arrive).
            let mut dirs = vec![home.join("PS2ISO")];
            for depth in 0..2 {
                let subs: Vec<PathBuf> = dirs.iter().flat_map(|d| Self::files(d)).filter(|p| p.is_dir()).collect();
                if depth == 0 { dirs.extend(subs.clone()) } else { dirs.extend(subs) }
            }
            let mut images: Vec<PathBuf> = dirs.iter().flat_map(|d| Self::files(d)).filter(|p| p.is_file() && ps2kit::sectors::is_disc_image(p)).collect();
            // Prefer .cue/.iso over the .bin they describe.
            images.sort_by_key(|p| match p.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref() { Some("cue") => 0, Some("iso") => 1, _ => 2 });
            for p in images { if self.disc.is_none() { self.load_disc(&p, true) } }
        }
        self.frame = self.start_frame();
    }

    /// Reads every file in the BIOS folder once: PS2 dumps (a ROMDIR) and PS1 dumps go on the
    /// shelf with what they say about themselves; the PS1 ones are cross-checked as a set.
    fn scan_shelf(&mut self, files: &[PathBuf]) {
        let mut ps1: Vec<(String, Vec<u8>)> = Vec::new();
        for p in files {
            let name = p.file_name().unwrap_or_default().to_string_lossy().into_owned();
            if p.is_dir() || std::fs::metadata(p).map_or(true, |m| m.len() > 8 << 20) { continue }
            let Ok(d) = std::fs::read(p) else { continue };
            if Self::looks_ps1(&d) {
                let label = match Ps1Bios::new(d.clone()) {
                    Ok(b) => { let id = b.identity(); format!("{} {} — {name}", if id.version.is_empty() { "1.0".into() } else { id.version.split_whitespace().next().unwrap_or("").to_string() }, id.region_letter.map(String::from).unwrap_or_default()) }
                    Err(_) => format!("damaged — {name}"),
                };
                self.shelf.push(ShelfEntry { path: p.clone(), console: Console::Ps1, label });
                ps1.push((name, d));
            } else if let Ok(rom) = RomDir::new(d) {
                let ver = rom.module("ROMVER").map(|v| String::from_utf8_lossy(&v[..v.len().min(14)]).into_owned()).unwrap_or_default();
                self.shelf.push(ShelfEntry { path: p.clone(), console: Console::Ps2, label: format!("{} {} — {name}", ver.get(..4).map(|v| format!("{}.{}", &v[..2].trim_start_matches('0'), &v[2..])).unwrap_or_default(), ver.get(4..5).unwrap_or("")) });
            }
        }
        let set: Vec<(&str, &[u8])> = ps1.iter().map(|(n, d)| (n.as_str(), d.as_slice())).collect();
        self.ps1_set_issues = ps2kit::ps1bios::audit_set(&set);
    }

    /// The PUP identity card in a few lines (`ps2kit::pup`, `notes/research/ps3_pup.md`).
    fn pup_summary(pup: &Pup, path: &Path) -> Vec<String> {
        let h = &pup.header;
        let mut out = vec![format!("{} — PS3 system update {}", path.file_name().unwrap_or_default().to_string_lossy(), pup.version().unwrap_or_else(|_| "?".into()))];
        out.push(format!("{} members, image version 0x{:X}, {} + {} bytes of header and data", h.file_count, h.image_version, h.header_length, h.data_length));
        if let Ok(l) = pup.license_locales() { out.push(format!("Licence text in {} locales{}", l.len(), l.first().map(|x| format!(" (dated {})", x.date)).unwrap_or_default())) }
        if let Ok(pk) = pup.packages() {
            let ok = pk.iter().filter(|p| p.uncompressed_reconciled && p.stored_reconciled).count();
            let spkg = pk.iter().filter(|p| p.spkg_hdr_matched == Some(true)).count();
            out.push(format!("{} update packages: {ok} reconcile their sizes, {spkg} match their spkg_hdr copy", pk.len()));
        }
        out.push("Container structure only: the packages are encrypted and are not read.".into());
        out
    }

    /// A standalone PS1 dump, or something that was meant to be one (a damaged copy).
    fn looks_ps1(d: &[u8]) -> bool {
        Ps1Bios::detect(d) || (d.len() < 0x100000 && d.windows(20).any(|w| w == b"PS-X Realtime Kernel") && !d.windows(10).any(|w| w == b"RESET\0\0\0\0\0"))
    }

    pub fn load_bios(&mut self, path: &Path, quiet: bool) {
        let data = match std::fs::read(path) {
            Ok(d) => d,
            Err(e) => { if !quiet { self.bios_status = format!("{}: {e}", path.file_name().unwrap_or_default().to_string_lossy()) } return }
        };
        if Self::looks_ps1(&data) { return self.load_ps1_bios(path, data, quiet) }
        let result = RomDir::new(data).map_err(|e| e.to_string()).and_then(|rom| {
            let assets = OpeningAssets::load(&rom).map_err(|e| e.to_string())?;
            let logo = LogoAssets::load(&rom).ok();
            Ok((rom, assets, logo))
        });
        match result {
            Ok((rom, assets, logo)) => {
                self.bios_status = format!("{} — ROM {}", path.file_name().unwrap_or_default().to_string_lossy(), assets.rom_version());
                // ROMVER's fifth character is the region: E = Europe (50 Hz).
                self.video = if assets.rom_version().as_bytes().get(4) == Some(&b'E') { VideoMode::Pal } else { VideoMode::Ntsc };
                self.assets = Some(assets);
                self.assets_version += 1;
                self.bios_path = Some(path.to_path_buf());
                if let Some(l) = &logo { self.ps2_logo_chime = l.chime(0x12) }
                self.ps2_ps1_shell = Ps1Shell::load(&rom).map_err(|e| eprintln!("rom0:LOGO: {e}")).ok();
                if self.console == Console::Ps2 || !quiet { self.console = Console::Ps2; self.ps1_shell = self.ps2_ps1_shell.clone() }
                self.logo_assets = logo;
                self.rom = Some(rom.clone());
                self.update_logo_sound();
                self.logo_version += 1;
                self.rebuild_scene();
                self.rebuild_timeline();
                self.sound_status = "Synthesising the chime from the BIOS…".into();
                let (tx, rx) = mpsc::channel();
                self.sound_rx = Some(rx);
                std::thread::spawn(move || {
                    let r = BootSound::load(&rom).map(|s| (s, Vec::new())).map_err(|e| e.to_string());
                    let _ = tx.send(r);
                });
            }
            Err(e) => if !quiet { self.bios_status = format!("{}: {e}", path.file_name().unwrap_or_default().to_string_lossy()) },
        }
    }

    /// A standalone PS1 / PSone / POPS dump: identity, shell, licence screen.
    fn load_ps1_bios(&mut self, path: &Path, data: Vec<u8>, quiet: bool) {
        let name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
        match Ps1Bios::new(data).and_then(|b| Ps1Shell::load_ps1(&b).map(|s| (b, s))) {
            Ok((bios, shell)) => {
                let id = bios.identity();
                self.ps1_bios_status = format!("{name} — {}", if id.version.is_empty() { "no version string (1.0)".to_string() } else { id.version.clone() });
                self.standalone_ps1_shell = Some(shell);
                self.ps1_bios = Some(bios);
                self.ps1_bios_path = Some(path.to_path_buf());
                if !quiet || self.assets.is_none() { self.set_console(Console::Ps1) } else if self.console == Console::Ps1 { self.set_console(Console::Ps1) }
            }
            Err(e) => if !quiet { self.ps1_bios_status = format!("{name}: {e}") },
        }
    }

    /// Switches the console the picture boots; the PS1 console uses the standalone dump's shell
    /// and video mode (letter E = PAL).
    pub fn set_console(&mut self, console: Console) {
        if console == Console::Ps1 && self.standalone_ps1_shell.is_none() { return }
        if console == Console::Ps2 && self.assets.is_none() { return }
        self.console = console;
        self.ps1_shell = match console { Console::Ps2 => self.ps2_ps1_shell.clone(), Console::Ps1 => self.standalone_ps1_shell.clone() };
        self.video = match console {
            Console::Ps1 => if self.ps1_bios.as_ref().and_then(|b| b.identity().region_letter) == Some('E') { VideoMode::Pal } else { VideoMode::Ntsc },
            Console::Ps2 => if self.assets.as_ref().is_some_and(|a| a.rom_version().as_bytes().get(4) == Some(&b'E')) { VideoMode::Pal } else { VideoMode::Ntsc },
        };
        if console == Console::Ps1 { self.scene_kind = Scene::Full }
        self.update_logo_sound();
        self.logo_version += 1;
        self.rebuild_timeline();
        self.frame = self.start_frame();
    }

    pub fn load_card(&mut self, path: &Path, quiet: bool) {
        match MemoryCard::open(path).and_then(|c| c.list(&[]).map(|_| c)) {
            Ok(c) => {
                self.card_history = PlayHistory::from_card(&c).ok();
                let name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
                if let Some(h) = &self.card_history {
                    self.card_status = format!("{name} — history in {}", h.source.as_deref().unwrap_or("?"));
                    self.history_source = HistorySource::Card;
                } else {
                    let n = PlayHistory::titles_with_saves(&c).len();
                    self.card_status = format!("{name} — no history file; {n} titles have saves");
                    self.history_source = if n > 0 { HistorySource::Saves } else { HistorySource::Empty };
                }
                self.card = Some(c);
                self.card_path = Some(path.to_path_buf());
                self.rebuild_history();
            }
            Err(e) => if !quiet { self.card_status = format!("{}: {e}", path.file_name().unwrap_or_default().to_string_lossy()) },
        }
    }

    pub fn load_disc(&mut self, path: &Path, quiet: bool) {
        match DiscImage::open(path) {
            Ok(d) => {
                self.disc_logo = if d.kind == ps2kit::disc::DiscKind::Ps2 { DiscLogo::read(path).ok() } else { None };
                self.ps1_logo = if d.kind == ps2kit::disc::DiscKind::Ps1 { ps2kit::ps1::Tmd::from_disc(path).ok() } else { None };
                self.disc_status = match d.kind {
                    ps2kit::disc::DiscKind::Ps2 => format!("{} — {}, {}", path.file_name().unwrap_or_default().to_string_lossy(), d.title_id().unwrap_or_else(|| "no BOOT2".into()), d.logo_region.map(|r| format!("logo: {r} master")).unwrap_or_else(|| "logo: unknown master".into())),
                    ps2kit::disc::DiscKind::Ps1 => format!("{} — PlayStation disc {}, licence {}", path.file_name().unwrap_or_default().to_string_lossy(), d.title_id().unwrap_or_else(|| "???".into()), d.ps1_licence.as_ref().and_then(|l| l.region).map(|r| r.to_string()).unwrap_or_else(|| "unknown".into())),
                    _ => format!("{} — not a PlayStation disc", path.file_name().unwrap_or_default().to_string_lossy()),
                };
                self.disc = Some(d);
                self.disc_path = Some(path.to_path_buf());
                self.update_logo_sound();
                self.rebuild_timeline();
            }
            Err(e) => {
                eprintln!("disc {}: {e}", path.display());
                if !quiet { self.disc_status = format!("{}: {e}", path.file_name().unwrap_or_default().to_string_lossy()) }
            }
        }
    }

    pub fn poll_sound(&mut self) {
        if let Some(rx) = &self.sound_rx {
            if let Ok(r) = rx.try_recv() {
                match r {
                    Ok((s, _)) => {
                        self.sound_status = format!("Chime: {:.1} s from SNDBOOTH/B/S, cue from SNDTNNLS", s.chime.len() as f32 / 2.0 / ps2kit::sound::SAMPLE_RATE as f32);
                        self.audio.load(s);
                        self.arrange_audio();
                    }
                    Err(e) => self.sound_status = format!("Sound: {e}"),
                }
                self.sound_rx = None;
            }
        }
    }

    pub fn rebuild_history(&mut self) {
        self.history = match self.history_source {
            HistorySource::Card => self.card_history.clone().unwrap_or_default(),
            HistorySource::Saves => {
                let titles = self.card.as_ref().map(PlayHistory::titles_with_saves).unwrap_or_default();
                let launches: Vec<u32> = titles.iter().map(|t| 3 + t.bytes().fold(7u32, |a, b| a.wrapping_mul(31).wrapping_add(b as u32)) % 58).collect();
                PlayHistory::synthetic(&launches, &titles, 1)
            }
            HistorySource::Custom => PlayHistory::synthetic(&vec![self.custom_launches; self.custom_titles as usize], &[], 1),
            HistorySource::Empty => PlayHistory::default(),
        };
        self.launch_log.clear();
        self.rebuild_scene();
    }

    /// Launches `title_id` once in the alternate history (today's date) and rebuilds the
    /// skyline; returns what the console would have done.
    pub fn launch(&mut self, title_id: &str) -> ps2kit::history::LaunchEffect {
        let before = self.history.clone();
        let effect = self.history.launch(title_id, today_date(), &mut self.launch_rng);
        let what = match &effect {
            e if e.dropped => format!("{title_id}: every record is maxed out — not recorded"),
            e => {
                let r = &self.history.records[e.slot];
                let mut t = format!("{}: count {}", r.name, r.count);
                if let Some(b) = e.new_tower { t += &format!(", tower bit {b} planted") }
                if let Some(v) = &e.evicted { t += &format!(", evicted {} ({}×) → history.old", v.name, v.count) }
                t
            }
        };
        self.launch_log.push((what, before));
        self.rebuild_scene();
        effect
    }

    /// The history as CSV: slot, title, count, mask, index, last launch.
    pub fn history_csv(&self) -> String {
        let mut out = String::from("slot,title_id,count,mask,index,last_launch\n");
        for (i, r) in self.history.records.iter().enumerate() {
            if r.is_empty() { continue }
            let date = if r.date == 0 { String::new() } else { format!("{:04}-{:02}-{:02}", r.year(), r.month(), r.day()) };
            out += &format!("{i},{},{},0x{:02X},{},{date}\n", r.name, r.count, r.mask, r.index);
        }
        out
    }

    /// Takes the last alternate-history launch back.
    pub fn undo_launch(&mut self) {
        if let Some((_, before)) = self.launch_log.pop() { self.history = before; self.rebuild_scene() }
    }

    fn rebuild_scene(&mut self) {
        if let Some(a) = &self.assets { self.scene = OpeningScene::new(a, &self.history) }
    }

    pub fn rebuild_timeline(&mut self) {
        let fps = self.video.fps();
        let mut plan = BootPlan::new(self.video, self.outcome());
        plan.power_on_seconds = self.power_on_seconds;
        plan.disc_settled_seconds = self.disc_seconds;
        plan.handoff_seconds = self.handoff_seconds;
        plan.warning_seconds = self.warning_exit_seconds;
        self.sequence = BootSequence::from_plan(&plan);
        if self.console == Console::Ps1 {
            self.sequence = BootSequence::ps1(&plan, 0, self.ps1_active().then(|| self.logo_timeline()));
            self.timeline = self.logo_timeline();
            self.logo_version += 1;
            self.arrange_audio();
            return;
        }
        self.timeline = match self.scene_kind {
            Scene::Boot | Scene::Full => Timeline::boot((self.disc_seconds * fps) as usize, self.video),
            Scene::Warning => Timeline::warning((self.warning_exit_seconds * fps) as usize, self.video),
            Scene::Logo => self.logo_timeline(),
        };
        if self.ps1_active() { self.sequence = self.sequence.clone().with_logo(self.logo_timeline()) }
        self.logo_version += 1;
        self.arrange_audio();
    }

    fn arrange_audio(&self) {
        if self.scene_kind == Scene::Full {
            let seq = &self.sequence;
            let logo = matches!(seq.outcome, BootOutcome::Game | BootOutcome::Ps1Game).then(|| seq.logo_start());
            let warning = seq.warning_start().map(|s| (s, s + seq.warning.dive_frame));
            self.audio.arrange(Scene::Full, self.timeline.fps(), seq.dive_frame(), logo, seq.opening_start(), self.ps1_active(), warning);
        } else {
            self.audio.arrange(self.scene_kind, self.timeline.fps(), self.timeline.dive_frame, Some(0), 0, self.ps1_active(), None);
        }
    }

    /// Where the boot leads under the current scenario.
    pub fn outcome(&self) -> BootOutcome {
        if self.console == Console::Ps1 { return if self.ps1_active() { BootOutcome::Ps1Game } else { BootOutcome::Menu } }
        boot_outcome(&self.handoff_steps(), self.enforce_checks)
    }

    pub fn set_scene(&mut self, kind: Scene) { self.scene_kind = kind; self.rebuild_timeline(); self.frame = self.start_frame() }

    pub fn start_frame(&self) -> f32 { if self.scene_kind == Scene::Full { 0.0 } else { -self.power_on_seconds * self.timeline.fps() } }
    pub fn end_frame(&self) -> f32 { if self.scene_kind == Scene::Full { (self.sequence.total_frames() - 1) as f32 } else { self.timeline.end_frame() as f32 } }

    /// Advances the clock by `dt` seconds of wall time.
    pub fn tick(&mut self, dt: f64) {
        if !self.playing { return }
        let mut f = self.frame + (dt * self.speed) as f32 * self.timeline.fps();
        let end = self.end_frame();
        if f >= end {
            if self.looping { f = self.start_frame() } else { f = end; self.playing = false }
        }
        self.frame = f;
    }

    pub fn sync_audio(&mut self) {
        self.audio.sync(self.frame, self.speed, self.playing, self.timeline.fps(), self.sound_enabled && self.audio.ready, self.sound_volume);
        if self.visualizer != VisualizerMode::Off {
            let at = (self.frame as f64 / self.timeline.fps() as f64 * ps2kit::sound::SAMPLE_RATE as f64) as i64;
            let clips = self.audio.clips();
            self.snapshot = self.analysis.measure(&clips, self.audio.gain(), at, self.visualizer);
        }
    }

    pub fn camera(&self) -> CameraState {
        if self.scene_kind == Scene::Full {
            let (span, local) = self.sequence.span_at(self.frame.max(0.0) as usize);
            return match span.segment {
                Segment::Opening => self.sequence.opening.camera(local as f32),
                Segment::Warning => self.sequence.warning.camera(local as f32),
                _ => CameraState::default(),
            };
        }
        self.timeline.camera(self.frame)
    }

    pub fn current_segment(&self) -> Option<Segment> {
        (self.scene_kind == Scene::Full).then(|| self.sequence.span_at(self.frame.max(0.0) as usize).0.segment)
    }

    /// The caption of what the console is busy with while the screen is black, if it is.
    pub fn boot_phase(&self) -> Option<&'static str> {
        let fps = self.timeline.fps();
        if self.scene_kind == Scene::Full {
            let (span, local) = self.sequence.span_at(self.frame.max(0.0) as usize);
            return match span.segment {
                Segment::PowerOn => phase_at(&POWER_ON_PHASES, local as f32 / fps, self.power_on_seconds).map(phase_caption),
                Segment::Handoff => phase_at(if self.ps1_active() { &PS1_HANDOFF_PHASES } else { &HANDOFF_PHASES }, local as f32 / fps, self.handoff_seconds).map(phase_caption),
                Segment::End => Some(if self.sequence.outcome == BootOutcome::Warning { WARNING_END_CAPTION } else { END_CAPTION }),
                Segment::Menu => Some(if self.console == Console::Ps1 { PS1_MENU_CAPTION } else { MENU_CAPTION }),
                Segment::Intro => Some(PS1_INTRO_CAPTION),
                _ => None,
            };
        }
        if self.frame >= 0.0 { return None }
        phase_at(&POWER_ON_PHASES, self.power_on_seconds + self.frame / fps, self.power_on_seconds).map(phase_caption)
    }

    /// A PlayStation disc is loaded (and not overridden) and the BIOS's PS1 shell could be read.
    pub fn ps1_active(&self) -> bool { self.ps1_shell.is_some() && self.disc_override == DiscOverride::AsLoaded && self.disc.as_ref().is_some_and(|d| d.kind == ps2kit::disc::DiscKind::Ps1) }
    /// The logo the licence screen shows: the disc's, else the shell's own copy.
    pub fn ps1_logo_model(&self) -> Option<&ps2kit::ps1::Tmd> { self.ps1_logo.as_ref().or(self.ps1_shell.as_ref().map(|s| &s.logo)) }
    /// The licence line exactly as the shell draws it (spaces included).
    pub fn ps1_licence_text(&self) -> String { self.disc.as_ref().and_then(|d| d.ps1_licence.as_ref()).map(|l| l.line.clone()).unwrap_or_default() }
    fn logo_timeline(&self) -> Timeline {
        match (&self.ps1_shell, self.ps1_active()) {
            (Some(shell), true) => Timeline::ps1_licence(self.video, ps2kit::ps1::LicenceTimeline::new(self.video).total_fields(shell)),
            _ => Timeline::logo(self.video),
        }
    }
    fn update_logo_sound(&mut self) {
        let pcm = match (&self.ps1_shell, self.ps1_active()) {
            (Some(shell), true) => shell.render_sound(self.video),
            _ => self.ps2_logo_chime.clone(),
        };
        self.audio.load_logo_chime(pcm);
    }

    /// The console's region from the fifth `ROMVER` character.
    pub fn detected_region(&self) -> Option<Region> {
        if self.console == Console::Ps1 { return self.ps1_bios.as_ref().and_then(|b| b.identity().region_letter).and_then(Region::from_romver_letter) }
        self.assets.as_ref().and_then(|a| a.rom_version().chars().nth(4)).and_then(Region::from_romver_letter)
    }
    /// The region the scenario pretends the console has, else the detected one.
    pub fn rom_region(&self) -> Option<Region> { self.region_override.or_else(|| self.detected_region()) }
    /// The facts of the hand-off under the current scenario.
    pub fn handoff_steps(&self) -> Vec<HandoffStep> {
        let disc = match self.disc_override {
            DiscOverride::AsLoaded => self.disc.as_ref(),
            DiscOverride::NoDisc => None,
            DiscOverride::Illegal => return vec![HandoffStep::IllegalDisc],
        };
        handoff_steps(disc, &self.history, self.video, self.rom_region(), self.ps1_shell.as_ref().map(|s| s.logo_bytes.as_slice()))
    }
    /// Whether this console's PS1 shell would accept the loaded PlayStation disc.
    pub fn ps1_verdict(&self) -> Option<ps2kit::disc::Ps1Verdict> {
        let d = self.disc.as_ref()?;
        if self.console == Console::Ps1 && !self.ps1_checks_licence()? { return Some(ps2kit::disc::Ps1Verdict::NotChecked) }
        ps2kit::disc::Ps1Verdict::judge(self.rom_region(), d.ps1_licence.as_ref(), self.ps1_shell.as_ref().map(|s| s.logo_bytes.as_slice()))
    }

    /// Whether the active PS1 shell compares the disc's licence text and logo: the policy read
    /// from the shell's code, applied to the console's letter (the scenario's region if forced).
    pub fn ps1_checks_licence(&self) -> Option<bool> {
        let shell = self.ps1_shell.as_ref()?;
        let letter = match self.console {
            Console::Ps1 => self.region_override.and_then(Region::romver_letter).or_else(|| self.ps1_bios.as_ref().and_then(|b| b.identity().region_letter)),
            Console::Ps2 => self.rom_region().and_then(Region::romver_letter),
        };
        Some(shell.licence_policy.checks(letter))
    }

    /// The PS1 kernel's own boot steps for the loaded PlayStation disc (`ps2kit::ps1boot`).
    pub fn ps1_boot_steps(&self) -> Vec<Ps1BootStep> {
        let cnf = self.disc.as_ref().filter(|d| d.kind == ps2kit::disc::DiscKind::Ps1 && self.disc_override == DiscOverride::AsLoaded && !d.system_cnf_text.is_empty()).map(|d| d.system_cnf_text.as_bytes());
        ps1_boot_steps(cnf, None, None, self.ps1_checks_licence())
    }

    /// The loaded PlayStation disc's `SYSTEM.CNF` as the PS1 kernel reads it.
    pub fn ps1_system_cnf(&self) -> Option<Ps1SystemCnf> {
        self.disc.as_ref().filter(|d| d.kind == ps2kit::disc::DiscKind::Ps1 && !d.system_cnf_text.is_empty()).map(|d| Ps1SystemCnf::parse(d.system_cnf_text.as_bytes()))
    }

    pub fn logo_animation(&self) -> Option<LogoAnimation<'_>> { self.logo_assets.as_ref().map(|a| LogoAnimation::new(a, self.video)) }

    pub fn logo_bitmap(&self) -> Option<LogoBitmap> {
        if let Some(d) = &self.disc_logo { return Some(d.bitmap(self.video)) }
        self.logo_animation().map(|a| DiscLogo::synthesised(&a))
    }

    pub fn reset_free_camera(&mut self) { self.free_camera = FreeCamera::behind(&self.camera()) }

    /// The opening's frame on its own clock, if the picture is showing the opening.
    pub fn opening_frame(&self) -> Option<f32> {
        match self.scene_kind {
            Scene::Boot => (self.frame >= 0.0).then_some(self.frame),
            Scene::Full => {
                let (span, local) = self.sequence.span_at(self.frame.max(0.0) as usize);
                (span.segment == Segment::Opening).then_some(local as f32 + self.frame.fract())
            }
            _ => None,
        }
    }

    /// The picture is showing the second phase's logo or licence screen.
    pub fn in_logo_phase(&self) -> bool {
        match self.scene_kind {
            Scene::Logo => true,
            Scene::Full => self.sequence.span_at(self.frame.max(0.0) as usize).0.segment == Segment::Logo,
            _ => false,
        }
    }

    /// The view the picture is drawn with this frame.
    pub fn current_view(&self) -> crate::renderer::ViewCamera {
        self.view_override().unwrap_or_else(|| crate::renderer::ViewCamera::scripted(self.camera(), self.video))
    }

    /// The record whose tower is nearest to a point of the picture (`x`, `y` in 0..1 of the
    /// 4:3 frame), within `radius` of it, with the tower's name.
    pub fn pick_tower(&self, x: f32, y: f32, radius: f32) -> Option<usize> {
        let frame = self.opening_frame()?;
        let view = self.current_view();
        let fps = self.timeline.fps();
        let n = self.scene.towers.len();
        let mut best: Option<(f32, usize)> = None;
        for (i, t) in self.scene.towers.iter().enumerate() {
            let p = crate::scenes::opening::tower_place(&self.options, i, n, t, frame, fps);
            let near = p - Vec3::new(0.0, 0.0, t.half_length);
            let c = view.project(near);
            if c.w <= 0.5 { continue }
            let (sx, sy) = ((c.x / c.w + 1.0) / 2.0, (1.0 - c.y / c.w) / 2.0);
            let d = ((sx - x).powi(2) + (sy - y).powi(2)).sqrt();
            if d < radius && best.is_none_or(|(bd, _)| d < bd) { best = Some((d, t.record)) }
        }
        best.map(|(_, r)| r)
    }

    /// The dials of the current path.
    pub fn path_params_mut(&mut self) -> &mut PathParams { let i = self.camera_mode.index(); &mut self.path_params[i] }

    /// The view that replaces the console's camera this frame, if any: the free camera, or a
    /// predetermined path.
    pub fn view_override(&self) -> Option<crate::renderer::ViewCamera> {
        if self.free_camera_enabled { let mut v = self.free_camera.view(self.video); v.zoom = self.free_zoom; return Some(v) }
        let p = self.path_params[self.camera_mode.index()];
        let frame = (self.frame - self.start_frame()).max(0.0);
        // The paths circle the city's base (the towers' far end, vertical = z; the console's
        // camera starts at z = 16) — or, in the second phase, the logo (vertical = y): the
        // PS1 model's centre, or the picture plane the PS2 logo hangs on.
        // In the city the console starts on the axis, so a level lap starts beside it, towards
        // the console's screen-top (-y); for the logo screens the lap starts at the console's
        // own front (-z), at height 0.
        let (centre, radius, height, vertical, front) = if self.in_logo_phase() {
            if self.ps1_active() {
                let k = crate::scenes::ps1::PS1_WORLD_SCALE;
                (Vec3::new(0.0, -340.0 * k, 5888.0 * k), 5888.0 * k * 0.7, 0.0, Vec3::Y, -Vec3::Z)
            } else {
                (Vec3::new(0.0, 0.0, crate::renderer::PICTURE_PLANE_Z), crate::renderer::PICTURE_PLANE_Z, 0.0, Vec3::Y, -Vec3::Z)
            }
        } else {
            // The city's extent: tops are the near caps (small z), the floor the far ends.
            let (floor, top) = self.scene.towers.iter().fold((0.0f32, f32::MAX), |(f, t), tw| (f.max(tw.centre.z + tw.half_length), t.min(tw.centre.z - tw.half_length)));
            let (floor, top) = if top == f32::MAX { (200.0, 150.0) } else { (floor.max(200.0), top) };
            let (xs, ys) = self.scene.towers.iter().fold(((f32::MAX, f32::MIN), (f32::MAX, f32::MIN)), |((x0, x1), (y0, y1)), t| ((x0.min(t.centre.x), x1.max(t.centre.x)), (y0.min(t.centre.y), y1.max(t.centre.y))));
            let (cx, cy) = if xs.0 == f32::MAX { (0.0, 0.0) } else { ((xs.0 + xs.1) / 2.0, (ys.0 + ys.1) / 2.0) };
            // Aim at `camera_focus` of the way up from the floor to the highest top.
            let aim_z = floor - (floor - top) * p.focus;
            (Vec3::new(cx, cy, aim_z), floor - 16.0, p.height, Vec3::Z, -Vec3::Y)
        };
        let radius = radius * p.distance;
        let v = self.camera_mode.view(frame, p.speed, centre, radius, height, vertical, front, self.video).map(|mut v| { v.zoom = p.zoom; v });
        if std::env::var_os("PS2_DEBUG_PLANE").is_some() {
            let (lo, hi) = self.scene.towers.iter().fold((f32::MAX, f32::MIN), |(lo, hi), t| (lo.min(t.centre.z - t.half_length), hi.max(t.centre.z + t.half_length)));
            let (xlo, xhi) = self.scene.towers.iter().fold((f32::MAX, f32::MIN), |(lo, hi), t| (lo.min(t.centre.x), hi.max(t.centre.x)));
            let (ylo, yhi) = self.scene.towers.iter().fold((f32::MAX, f32::MIN), |(lo, hi), t| (lo.min(t.centre.y), hi.max(t.centre.y)));
            eprintln!("path centre {centre:?} radius {radius} height {height} tower x {xlo}..{xhi} y {ylo}..{yhi} z {lo}..{hi} view {v:?}");
        }
        v
    }

    /// One colour multiplier per tower of the scene for the chosen tint (beyond the PS2:
    /// the console draws every tower grey).
    pub fn tower_tints(&self) -> Vec<Vec3> {
        let records = &self.history.records;
        let newest = records.iter().map(|r| r.date).max().unwrap_or(0);
        let oldest = records.iter().filter(|r| !r.is_empty() && r.date != 0).map(|r| r.date).min().unwrap_or(newest);
        let days = |d: u16| (d >> 9) as f32 * 365.0 + (d >> 5 & 15) as f32 * 30.4 + (d & 31) as f32;
        let lerp = |a: Vec3, b: Vec3, t: f32| a + (b - a) * t.clamp(0.0, 1.0);
        self.scene.towers.iter().map(|t| {
            let Some(r) = records.get(t.record) else { return Vec3::ONE };
            match self.options.tint {
                TowerTint::Console => Vec3::ONE,
                // Few launches: steel blue; many: gold; maxed out (index 7): white-gold.
                TowerTint::Count => if r.index == 7 { Vec3::new(1.3, 1.15, 0.8) } else { lerp(Vec3::new(0.55, 0.75, 1.25), Vec3::new(1.3, 1.0, 0.45), r.count as f32 / 63.0) },
                // Recent: warm; long ago: cold; no date: grey.
                TowerTint::Age => if r.date == 0 || newest == oldest { Vec3::splat(0.8) } else { lerp(Vec3::new(0.5, 0.7, 1.3), Vec3::new(1.3, 0.8, 0.5), (days(r.date) - days(oldest)) / (days(newest) - days(oldest)).max(1.0)) },
                // The third letter of the title ID: E Europe, U USA, P Japan, K Korea, A Asia, C China.
                TowerTint::Region => match r.name.as_bytes().get(2) {
                    Some(b'E') => Vec3::new(0.6, 0.8, 1.3),
                    Some(b'U') => Vec3::new(1.3, 0.65, 0.6),
                    Some(b'P') => Vec3::new(1.3, 1.2, 0.6),
                    Some(b'K') | Some(b'A') | Some(b'C') => Vec3::new(0.7, 1.25, 0.7),
                    _ => Vec3::splat(0.9),
                },
                // SC = Sony first party, SL = licensed, anything else (homebrew, PSX.EXE) grey.
                TowerTint::Publisher => match r.name.as_bytes().get(0..2) {
                    Some(b"SC") => Vec3::new(1.3, 1.1, 0.6),
                    Some(b"SL") => Vec3::new(0.6, 0.85, 1.3),
                    _ => Vec3::splat(0.9),
                },
            }
        }).collect()
    }

    /// Puts the free camera beside the route, looking at its middle.
    pub fn view_path_from_side(&mut self) {
        let (position, target) = (Vec3::new(130.0, -60.0, 25.0), Vec3::new(0.0, 0.0, 60.0));
        let f = (target - position).normalize();
        self.free_camera = FreeCamera { pivot: target, distance: (target - position).length(), yaw: f.x.atan2(f.z), pitch: f.y.asin() };
    }

    pub fn set_camera_path(&mut self, on: bool) {
        self.options.camera_path = on;
        if on && !self.free_camera_enabled { self.free_camera_enabled = true; self.view_path_from_side() }
    }

    pub fn camera_path_csv(&self) -> String {
        let t = if self.scene_kind == Scene::Full { &self.sequence.opening } else { &self.timeline };
        let mut out = String::from("frame,seconds,x,y,z,roll_rad,up_x,up_y,up_z,stage\n");
        for (f, s) in t.states().iter().enumerate() {
            let up = s.up();
            out += &format!("{f},{:.4},0,0,{:.5},{:.6},{:.6},{:.6},0,{}\n", f as f32 / t.fps(), s.z, s.roll, -up.x, -up.y, s.stage);
        }
        out
    }
}

