//! Playback of the synthesised sounds on the sequence clock (cpal), and the measurements
//! behind the volume / wave / equaliser visualisers.

use crate::model::Scene;
use ps2kit::sound::{BootSound, SAMPLE_RATE};
use rustfft::{num_complex::Complex, FftPlanner};
use std::sync::{Arc, Mutex};

/// A piece of audio placed on the sequence clock.
#[derive(Clone)]
pub struct Clip {
    pub start: i64, // stereo frame
    pub pcm: Arc<Vec<f32>>,
    pub looped: bool,
    pub fade_start: Option<i64>, // stop with release 0xF: 1.37 s linear fade
}

#[derive(Default)]
struct Shared {
    clips: Vec<Clip>,
    position: f64,
    rate: f64,
    playing: bool,
    gain: f32,
}

pub struct AudioPlayer {
    shared: Arc<Mutex<Shared>>,
    _stream: Option<cpal::Stream>,
    pub sounds: Option<Arc<BootSound>>,
    pub logo_chime: Arc<Vec<f32>>,
    pub ready: bool,
    pub device_rate: f64,
}

fn sample_clips(clips: &[Clip], frame: i64, right: bool, gain: f32) -> f32 {
    let c = right as usize;
    let mut v = 0.0;
    for clip in clips {
        let mut k = frame - clip.start;
        if k < 0 || clip.pcm.is_empty() { continue }
        let frames = (clip.pcm.len() / 2) as i64;
        if clip.looped { k %= frames } else if k >= frames { continue }
        let mut s = clip.pcm[(k as usize) * 2 + c];
        if let Some(f) = clip.fade_start {
            if frame > f { s *= (1.0 - (frame - f) as f32 / (SAMPLE_RATE as f32 * 1.37)).max(0.0) }
        }
        v += s;
    }
    v * gain
}

impl AudioPlayer {
    pub fn new() -> Self {
        use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
        let shared = Arc::new(Mutex::new(Shared { rate: 1.0, gain: 1.0, ..Default::default() }));
        let mut stream = None;
        let mut device_rate = SAMPLE_RATE as f64;
        if let Some(device) = cpal::default_host().default_output_device() {
            if let Ok(cfg) = device.default_output_config() {
                let channels = cfg.channels() as usize;
                device_rate = cfg.sample_rate() as f64;
                let resample = SAMPLE_RATE as f64 / device_rate;
                let s = shared.clone();
                let built = device.build_output_stream(
                    cfg.config(),
                    move |out: &mut [f32], _| {
                        let mut sh = s.lock().unwrap();
                        for frame in out.chunks_mut(channels) {
                            if !sh.playing { frame.fill(0.0); continue }
                            let p = sh.position.floor() as i64;
                            let f = (sh.position - p as f64) as f32;
                            let l = sample_clips(&sh.clips, p, false, sh.gain) * (1.0 - f) + sample_clips(&sh.clips, p + 1, false, sh.gain) * f;
                            let r = sample_clips(&sh.clips, p, true, sh.gain) * (1.0 - f) + sample_clips(&sh.clips, p + 1, true, sh.gain) * f;
                            frame[0] = l;
                            if channels > 1 { frame[1] = r }
                            for extra in frame.iter_mut().skip(2) { *extra = 0.0 }
                            sh.position += sh.rate * resample;
                        }
                    },
                    |e| eprintln!("audio: {e}"),
                    None,
                );
                if let Ok(st) = built {
                    let _ = st.play();
                    stream = Some(st);
                }
            }
        }
        Self { shared, _stream: stream, sounds: None, logo_chime: Arc::new(Vec::new()), ready: false, device_rate }
    }

    pub fn load(&mut self, sound: BootSound) { self.sounds = Some(Arc::new(sound)); self.ready = true }
    pub fn load_logo_chime(&mut self, pcm: Vec<f32>) { self.logo_chime = Arc::new(pcm) }

    /// Places the clips for a scene; frames are converted with `fps`.
    /// `ps1_disc`: the opening stops the chime with the fast release instead of starting the
    /// PS2 transition cue (OSDSYS does that for PlayStation discs, audio CDs and DVD-Video).
    pub fn arrange(&self, scene: Scene, fps: f32, dive_frame: usize, logo_start: usize, opening_start: usize, ps1_disc: bool) {
        let at = |frame: usize| (frame as f64 / fps as f64 * SAMPLE_RATE as f64) as i64;
        let mut list = Vec::new();
        let clip = |start, pcm: Arc<Vec<f32>>, looped, fade_start| Clip { start, pcm, looped, fade_start };
        match scene {
            Scene::Boot | Scene::Full => {
                if let Some(s) = &self.sounds {
                    if ps1_disc {
                        list.push(clip(at(opening_start), Arc::new(s.chime.clone()), false, Some(at(dive_frame))));
                    } else {
                        list.push(clip(at(opening_start), Arc::new(s.chime.clone()), false, None));
                        list.push(clip(at(dive_frame), Arc::new(s.cue.clone()), false, None));
                    }
                }
                if scene == Scene::Full { list.push(clip(at(logo_start), self.logo_chime.clone(), false, None)) }
            }
            Scene::Warning => {
                if let Some(s) = &self.sounds { list.push(clip(0, Arc::new(s.warning.clone()), true, Some(at(dive_frame)))) }
            }
            Scene::Logo => list.push(clip(0, self.logo_chime.clone(), false, None)),
        }
        self.shared.lock().unwrap().clips = list;
    }

    /// Called once per display frame with the sequence clock.
    pub fn sync(&self, frame: f32, speed: f64, playing: bool, fps: f32, enabled: bool, volume: f32) {
        let target = frame as f64 / fps as f64 * SAMPLE_RATE as f64;
        let mut sh = self.shared.lock().unwrap();
        sh.rate = speed;
        sh.gain = volume;
        let was = sh.playing;
        sh.playing = playing && enabled;
        if !was || (sh.position - target).abs() > SAMPLE_RATE as f64 * 0.06 { sh.position = target }
    }

    pub fn clips(&self) -> Vec<Clip> { self.shared.lock().unwrap().clips.clone() }
    pub fn gain(&self) -> f32 { self.shared.lock().unwrap().gain }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum VisualizerMode { Off, Volume, Wave, Equalizer }

impl VisualizerMode {
    pub const ALL: [Self; 4] = [Self::Off, Self::Volume, Self::Wave, Self::Equalizer];
    pub fn name(self) -> &'static str { match self { Self::Off => "Off", Self::Volume => "Volume", Self::Wave => "Wave", Self::Equalizer => "Equalizer" } }
}

#[derive(Clone, Default)]
pub struct Snapshot {
    pub rms_db: (f32, f32),
    pub peak_db: (f32, f32),
    pub hold_db: (f32, f32),
    pub wave: Vec<f32>,
    pub bands: Vec<f32>,
    pub band_peaks: Vec<f32>,
}

pub const BAND_COUNT: usize = 32;
const FFT_SIZE: usize = 2048;

/// Measures the audio around the current time; keeps a little state for peak-hold decay.
pub struct Analysis {
    fft: Arc<dyn rustfft::Fft<f32>>,
    window: Vec<f32>,
    hold: (f32, f32),
    band_peaks: Vec<f32>,
    band_smooth: Vec<f32>,
    band_edges: Vec<usize>,
}

impl Analysis {
    pub fn new() -> Self {
        let bin_hz = SAMPLE_RATE as f64 / FFT_SIZE as f64;
        Self {
            fft: FftPlanner::new().plan_fft_forward(FFT_SIZE),
            window: (0..FFT_SIZE).map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / FFT_SIZE as f32).cos()).collect(),
            hold: (-90.0, -90.0),
            band_peaks: vec![0.0; BAND_COUNT],
            band_smooth: vec![0.0; BAND_COUNT],
            band_edges: (0..=BAND_COUNT).map(|i| ((40.0 * (16000.0f64 / 40.0).powf(i as f64 / BAND_COUNT as f64)) / bin_hz) as usize).collect(),
        }
    }

    pub fn measure(&mut self, clips: &[Clip], gain: f32, frame: i64, mode: VisualizerMode) -> Snapshot {
        let mut snap = Snapshot::default();
        if mode == VisualizerMode::Off { return snap }
        let n = FFT_SIZE as i64;
        let mut mono = vec![0f32; FFT_SIZE];
        let (mut sum_l, mut sum_r, mut peak_l, mut peak_r) = (0f32, 0f32, 0f32, 0f32);
        for i in 0..FFT_SIZE {
            let at = frame - n + i as i64;
            let (l, r) = (sample_clips(clips, at, false, gain), sample_clips(clips, at, true, gain));
            mono[i] = (l + r) * 0.5;
            {   // meters over the whole 43 ms window
                sum_l += l * l; sum_r += r * r;
                peak_l = peak_l.max(l.abs()); peak_r = peak_r.max(r.abs());
            }
        }
        let db = |v: f32| (20.0 * v.max(1e-6).log10()).max(-90.0);
        snap.rms_db = (db((sum_l / FFT_SIZE as f32).sqrt()), db((sum_r / FFT_SIZE as f32).sqrt()));
        snap.peak_db = (db(peak_l), db(peak_r));
        self.hold = (snap.peak_db.0.max(self.hold.0 - 0.4), snap.peak_db.1.max(self.hold.1 - 0.4));
        snap.hold_db = self.hold;
        if mode == VisualizerMode::Wave { snap.wave = mono[FFT_SIZE - 1024..].to_vec() }
        if mode == VisualizerMode::Equalizer {
            let mut buf: Vec<Complex<f32>> = mono.iter().zip(&self.window).map(|(m, w)| Complex::new(m * w, 0.0)).collect();
            self.fft.process(&mut buf);
            let mags: Vec<f32> = buf[..FFT_SIZE / 2].iter().map(|c| c.norm() / FFT_SIZE as f32).collect();
            for b in 0..BAND_COUNT {
                let lo = self.band_edges[b].max(1);
                let hi = self.band_edges[b + 1].max(lo + 1).min(FFT_SIZE / 2);
                let m = mags[lo..hi].iter().cloned().fold(0.0, f32::max);
                let v = ((db(m) + 72.0) / 72.0).max(0.0);
                self.band_smooth[b] = if v > self.band_smooth[b] { v } else { self.band_smooth[b] * 0.85 };
                self.band_peaks[b] = self.band_smooth[b].max(self.band_peaks[b] - 0.012);
            }
            snap.bands = self.band_smooth.clone();
            snap.band_peaks = self.band_peaks.clone();
        }
        snap
    }
}
