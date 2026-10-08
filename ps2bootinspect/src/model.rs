//! App state: the user's files, the play history in use, the clock, and view settings.

use crate::audio::{Analysis, AudioPlayer, Snapshot, VisualizerMode};
use crate::renderer::RenderOptions;
use glam::Vec3;
use ps2kit::bios::OpeningAssets;
use ps2kit::disc::{handoff_steps, DiscImage, HandoffStep};
use ps2kit::history::PlayHistory;
use ps2kit::logo::{DiscLogo, LogoAnimation, LogoAssets, LogoBitmap};
use ps2kit::memcard::MemoryCard;
use ps2kit::rom::RomDir;
use ps2kit::sim::{phase_at, BootPhase, BootSequence, CameraState, OpeningScene, Segment, Timeline, HANDOFF_PHASES, POWER_ON_PHASES, PS1_HANDOFF_PHASES};
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
        crate::renderer::ViewCamera { position: self.position(), forward: self.forward(), up: Vec3::Y, video }
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
    pub scene_kind: Scene,
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
    pub free_camera: FreeCamera,
    pub sound_enabled: bool,
    pub sound_volume: f32,
    pub sound_status: String,
    pub visualizer: VisualizerMode,
    pub snapshot: Snapshot,
    pub timeline: Timeline,
    pub sequence: BootSequence,
    pub assets: Option<OpeningAssets>,
    pub scene: OpeningScene,
    pub logo_assets: Option<LogoAssets>,
    /// The PS1 shell inside the BIOS (`rom0:LOGO`), for PlayStation discs.
    pub ps1_shell: Option<ps2kit::ps1::Ps1Shell>,
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
            scene_kind: Scene::Full, video, language: "E",
            power_on_seconds: 3.0, disc_seconds: 0.0, handoff_seconds: 1.2, warning_exit_seconds: 10.0,
            frame: 0.0, playing: true, looping: true, speed: 1.0,
            options: RenderOptions::default(), free_camera_enabled: false, free_camera: FreeCamera { pivot: Vec3::new(0.0, 0.0, 120.0), distance: 100.0, yaw: 0.0, pitch: 0.0 },
            sound_enabled: true, sound_volume: 0.8, sound_status: "No sound loaded".into(), visualizer: VisualizerMode::Equalizer, snapshot: Snapshot::default(),
            timeline: Timeline::boot(0, video), sequence: BootSequence::new(video, 3.0, 0.0, 1.2, 6.0),
            assets: None, scene: OpeningScene::default(), logo_assets: None, ps1_shell: None, ps1_logo: None, ps2_logo_chime: Vec::new(), disc: None, disc_logo: None,
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
        for p in bios_dir.map(|d| Self::files(&d)).unwrap_or_default() {
            if self.assets.is_none() { self.load_bios(&p, true) }
        }
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

    pub fn load_bios(&mut self, path: &Path, quiet: bool) {
        let result = std::fs::read(path).map_err(|e| e.to_string()).and_then(|d| RomDir::new(d).map_err(|e| e.to_string())).and_then(|rom| {
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
                self.ps1_shell = ps2kit::ps1::Ps1Shell::load(&rom).map_err(|e| eprintln!("rom0:LOGO: {e}")).ok();
                self.logo_assets = logo;
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
        self.rebuild_scene();
    }

    fn rebuild_scene(&mut self) {
        if let Some(a) = &self.assets { self.scene = OpeningScene::new(a, &self.history) }
    }

    pub fn rebuild_timeline(&mut self) {
        let fps = self.video.fps();
        self.sequence = BootSequence::new(self.video, self.power_on_seconds, self.disc_seconds, self.handoff_seconds, 6.0);
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
            self.audio.arrange(Scene::Full, self.timeline.fps(), self.sequence.dive_frame(), self.sequence.logo_start(), self.sequence.opening_start(), self.ps1_active());
        } else {
            self.audio.arrange(self.scene_kind, self.timeline.fps(), self.timeline.dive_frame, 0, 0, self.ps1_active());
        }
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
            return if span.segment == Segment::Opening { self.sequence.opening.camera(local as f32) } else { CameraState::default() };
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
                Segment::End => Some(END_CAPTION),
                _ => None,
            };
        }
        if self.frame >= 0.0 { return None }
        phase_at(&POWER_ON_PHASES, self.power_on_seconds + self.frame / fps, self.power_on_seconds).map(phase_caption)
    }

    /// A PlayStation disc is loaded and the BIOS's PS1 shell could be read.
    pub fn ps1_active(&self) -> bool { self.ps1_shell.is_some() && self.disc.as_ref().is_some_and(|d| d.kind == ps2kit::disc::DiscKind::Ps1) }
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
    pub fn rom_region(&self) -> Option<Region> { self.assets.as_ref().and_then(|a| a.rom_version().chars().nth(4)).and_then(Region::from_romver_letter) }
    pub fn handoff_steps(&self) -> Vec<HandoffStep> { handoff_steps(self.disc.as_ref(), &self.history, self.video, self.rom_region(), self.ps1_shell.as_ref().map(|s| s.logo_bytes.as_slice())) }
    /// Whether this console's PS1 shell would accept the loaded PlayStation disc.
    pub fn ps1_verdict(&self) -> Option<ps2kit::disc::Ps1Verdict> {
        let d = self.disc.as_ref()?;
        ps2kit::disc::Ps1Verdict::judge(self.rom_region(), d.ps1_licence.as_ref(), self.ps1_shell.as_ref().map(|s| s.logo_bytes.as_slice()))
    }

    pub fn logo_animation(&self) -> Option<LogoAnimation<'_>> { self.logo_assets.as_ref().map(|a| LogoAnimation::new(a, self.video)) }

    pub fn logo_bitmap(&self) -> Option<LogoBitmap> {
        if let Some(d) = &self.disc_logo { return Some(d.bitmap(self.video)) }
        self.logo_animation().map(|a| DiscLogo::synthesised(&a))
    }

    pub fn reset_free_camera(&mut self) { self.free_camera = FreeCamera::behind(&self.camera()) }

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

