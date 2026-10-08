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
use ps2kit::sim::{phase_at, BootPhase, BootSequence, CameraState, OpeningScene, SceneKind, Segment, Timeline, VideoMode, HANDOFF_PHASES, POWER_ON_PHASES};
use ps2kit::sound::BootSound;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HistorySource { Card, Saves, Custom, Empty }

impl HistorySource {
    pub const ALL: [Self; 4] = [Self::Card, Self::Saves, Self::Custom, Self::Empty];
    pub fn name(self) -> &'static str {
        match self { Self::Card => "History file on card", Self::Saves => "Simulated from saves on card", Self::Custom => "Simulated", Self::Empty => "Empty (new console)" }
    }
}

pub const LANGUAGES: [(&str, &str); 12] = [("J", "Japanese"), ("E", "English"), ("F", "French"), ("S", "Spanish"), ("G", "German"), ("I", "Italian"), ("D", "Dutch"), ("P", "Portuguese"), ("R", "Russian"), ("K", "Korean"), ("H", "Chinese (traditional)"), ("C", "Chinese (simplified)")];

/// A fly-through camera: position plus yaw/pitch, level horizon.
#[derive(Clone, Copy, Debug)]
pub struct FreeCamera {
    pub position: Vec3,
    pub yaw: f32,
    pub pitch: f32,
}

impl FreeCamera {
    pub fn forward(&self) -> Vec3 { Vec3::new(self.yaw.sin() * self.pitch.cos(), self.pitch.sin(), self.yaw.cos() * self.pitch.cos()) }
    pub fn look(&mut self, dx: f32, dy: f32) { self.yaw += dx * 0.005; self.pitch = (self.pitch + dy * 0.005).clamp(-1.5, 1.5) }
    pub fn view(&self, video: VideoMode) -> crate::renderer::ViewCamera {
        crate::renderer::ViewCamera { position: self.position, forward: self.forward(), up: Vec3::Y, video }
    }
    pub fn slide(&mut self, right: f32, down: f32, forward: f32, video: VideoMode) {
        let (bx, by, bz) = self.view(video).basis();
        self.position += bx * right + by * down + bz * forward;
    }
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
    pub scene_kind: SceneKind,
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
            scene_kind: SceneKind::Full, video, language: "E",
            power_on_seconds: 3.0, disc_seconds: 0.0, handoff_seconds: 1.2, warning_exit_seconds: 10.0,
            frame: 0.0, playing: true, looping: true, speed: 1.0,
            options: RenderOptions::default(), free_camera_enabled: false, free_camera: FreeCamera { position: Vec3::new(0.0, 0.0, 16.0), yaw: 0.0, pitch: 0.0 },
            sound_enabled: true, sound_volume: 0.8, sound_status: "No sound loaded".into(), visualizer: VisualizerMode::Equalizer, snapshot: Snapshot::default(),
            timeline: Timeline::boot(0, video), sequence: BootSequence::new(video, 3.0, 0.0, 1.2, 6.0),
            assets: None, scene: OpeningScene::default(), logo_assets: None, disc: None, disc_logo: None,
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
            for p in Self::files(&home.join("PS2ISO")) {
                if self.disc.is_none() && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("iso")) { self.load_disc(&p, true) }
            }
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
                self.bios_status = format!("{} — ROM {}", path.file_name().unwrap_or_default().to_string_lossy(), assets.rom_version);
                // ROMVER's fifth character is the region: E = Europe (50 Hz).
                self.video = if assets.rom_version.as_bytes().get(4) == Some(&b'E') { VideoMode::Pal } else { VideoMode::Ntsc };
                self.assets = Some(assets);
                self.assets_version += 1;
                self.bios_path = Some(path.to_path_buf());
                if let Some(l) = &logo { self.audio.load_logo_chime(l.chime(0x12)) }
                self.logo_assets = logo;
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
                self.disc_logo = DiscLogo::read(path).ok();
                self.disc_status = format!("{} — {}, {}", path.file_name().unwrap_or_default().to_string_lossy(), d.title_id().unwrap_or_else(|| "no BOOT2".into()), d.logo_region.map(|r| format!("logo checksum region {r}")).unwrap_or_else(|| "logo checksum: no E/J match".into()));
                self.disc = Some(d);
                self.disc_path = Some(path.to_path_buf());
                self.logo_version += 1;
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
            SceneKind::Boot | SceneKind::Full => Timeline::boot((self.disc_seconds * fps) as usize, self.video),
            SceneKind::Warning => Timeline::warning((self.warning_exit_seconds * fps) as usize, self.video),
            SceneKind::Logo => Timeline::logo(self.video),
        };
        self.logo_version += 1;
        self.arrange_audio();
    }

    fn arrange_audio(&self) {
        if self.scene_kind == SceneKind::Full {
            self.audio.arrange(SceneKind::Full, self.timeline.fps(), self.sequence.dive_frame(), self.sequence.logo_start(), self.sequence.opening_start());
        } else {
            self.audio.arrange(self.scene_kind, self.timeline.fps(), self.timeline.dive_frame, 0, 0);
        }
    }

    pub fn set_scene(&mut self, kind: SceneKind) { self.scene_kind = kind; self.rebuild_timeline(); self.frame = self.start_frame() }

    pub fn start_frame(&self) -> f32 { if self.scene_kind == SceneKind::Full { 0.0 } else { -self.power_on_seconds * self.timeline.fps() } }
    pub fn end_frame(&self) -> f32 { if self.scene_kind == SceneKind::Full { (self.sequence.total_frames() - 1) as f32 } else { self.timeline.end_frame() as f32 } }

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
        if self.scene_kind == SceneKind::Full {
            let (span, local) = self.sequence.span_at(self.frame.max(0.0) as usize);
            return if span.segment == Segment::Opening { self.sequence.opening.camera(local as f32) } else { CameraState::default() };
        }
        self.timeline.camera(self.frame)
    }

    pub fn current_segment(&self) -> Option<Segment> {
        (self.scene_kind == SceneKind::Full).then(|| self.sequence.span_at(self.frame.max(0.0) as usize).0.segment)
    }

    pub fn boot_phase(&self) -> Option<&'static BootPhase> {
        let fps = self.timeline.fps();
        if self.scene_kind == SceneKind::Full {
            let (span, local) = self.sequence.span_at(self.frame.max(0.0) as usize);
            return match span.segment {
                Segment::PowerOn => Some(phase_at(&POWER_ON_PHASES, local as f32 / fps, self.power_on_seconds)),
                Segment::Handoff => Some(phase_at(&HANDOFF_PHASES, local as f32 / fps, self.handoff_seconds)),
                Segment::End => Some(&END_PHASE),
                _ => None,
            };
        }
        if self.frame >= 0.0 { return None }
        Some(phase_at(&POWER_ON_PHASES, self.power_on_seconds + self.frame / fps, self.power_on_seconds))
    }

    pub fn handoff_steps(&self) -> Vec<HandoffStep> { handoff_steps(self.disc.as_ref(), &self.history, self.video) }

    pub fn logo_animation(&self) -> Option<LogoAnimation<'_>> { self.logo_assets.as_ref().map(|a| LogoAnimation::new(a, self.video)) }

    pub fn logo_bitmap(&self) -> Option<LogoBitmap> {
        if let Some(d) = &self.disc_logo { return Some(d.bitmap(self.video)) }
        self.logo_animation().map(|a| DiscLogo::synthesised(&a))
    }

    pub fn reset_free_camera(&mut self) {
        let c = self.camera();
        self.free_camera = FreeCamera { position: c.position(), yaw: 0.0, pitch: 0.0 };
    }

    /// Puts the free camera beside the route, looking at its middle.
    pub fn view_path_from_side(&mut self) {
        let position = Vec3::new(130.0, -60.0, 25.0);
        let f = (Vec3::new(0.0, 0.0, 60.0) - position).normalize();
        self.free_camera = FreeCamera { position, yaw: f.x.atan2(f.z), pitch: f.y.asin() };
    }

    pub fn set_camera_path(&mut self, on: bool) {
        self.options.camera_path = on;
        if on && !self.free_camera_enabled { self.free_camera_enabled = true; self.view_path_from_side() }
    }

    pub fn camera_path_csv(&self) -> String {
        let t = if self.scene_kind == SceneKind::Full { &self.sequence.opening } else { &self.timeline };
        let mut out = String::from("frame,seconds,x,y,z,roll_rad,up_x,up_y,up_z,stage\n");
        for (f, s) in t.states.iter().enumerate() {
            let up = s.up();
            out += &format!("{f},{:.4},0,0,{:.5},{:.6},{:.6},{:.6},0,{}\n", f as f32 / t.fps(), s.z, s.roll, -up.x, -up.y, s.stage);
        }
        out
    }
}

static END_PHASE: BootPhase = BootPhase { name: "SPU quit; LoadExecPS2(boot ELF) — the game would start here", seconds: 0.0 };
