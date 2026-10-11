//! Synthetic arrangements plus optional caller-dump audio comparisons. BIOS facts:
//! sound.md §§2.2/3.3, ps1_shell_scenes.md §§1a/2 and ps1_boot.md §6.
use ps2kit::bench::SoundBench;
use ps2kit::ps1::{Ps1Shell, Vab};
use ps2kit::ps1bios::Ps1Bios;
use ps2kit::ps1intro::SceIntro;
use ps2kit::rom::RomDir;
use ps2kit::song::{
    BankRef, Channel, DrumMetrics, DrumVoice, Instrument, Kit, KitPiece, NoiseLayer, Note, Pattern,
    SampleRef, Song,
};
use ps2kit::sound::{
    write_wav, BootSound, DriverTables, EventKind, SoundBank, SoundSequence, Synth, SAMPLE_RATE,
};
use ps2kit::VideoMode;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}
fn put16(d: &mut [u8], p: usize, n: u16) {
    d[p..p + 2].copy_from_slice(&n.to_le_bytes())
}
fn put32(d: &mut [u8], p: usize, n: u32) {
    d[p..p + 4].copy_from_slice(&n.to_le_bytes())
}
fn adpcm() -> Vec<u8> {
    let mut d = vec![0x55; 32];
    d[0] = 0;
    d[1] = 4;
    d[16] = 0;
    d[17] = 3;
    d
}
fn bank() -> SoundBank {
    let mut h = vec![0; 0x60];
    put32(&mut h, 0, 0x60);
    put32(&mut h, 4, 32);
    h[12..16].copy_from_slice(b"SShd");
    put32(&mut h, 0x10, 0x20);
    put32(&mut h, 0x14, u32::MAX);
    put32(&mut h, 0x2c, u32::MAX);
    put16(&mut h, 0x22, 0x10);
    h[0x31] = 127;
    let t = 0x38;
    h[t + 1] = 127;
    h[t + 2] = 60;
    put16(&mut h, t + 6, 0x00ff);
    h[t + 11] = 127;
    h[t + 12] = 64;
    h[t + 13] = 2;
    SoundBank::new(&h, adpcm()).unwrap()
}
fn vab() -> Vab {
    let tone = 32 + 128 * 16;
    let table = tone + 16 * 32;
    let body = table + 512;
    let mut d = vec![0; body + 32];
    d[..4].copy_from_slice(b"pBAV");
    put16(&mut d, 18, 1);
    put16(&mut d, 20, 1);
    put16(&mut d, 22, 1);
    d[24] = 127;
    d[25] = 64;
    d[32] = 1;
    d[33] = 127;
    d[36] = 64;
    d[tone + 2] = 127;
    d[tone + 3] = 64;
    d[tone + 4] = 60;
    d[tone + 7] = 127;
    put16(&mut d, tone + 16, 0xff);
    put16(&mut d, tone + 22, 1);
    put16(&mut d, table + 2, 4);
    d[body..].copy_from_slice(&adpcm());
    Vab::parse(&d).unwrap()
}
fn song(kind: BankRef) -> Song {
    let mut s = Song::new("Test song", kind);
    let mut c = Channel::new("Instrument 0", 0);
    let mut p = Pattern::new(1920);
    p.notes.push(Note::new(0, 480, 60, 100));
    p.notes.push(Note::new(960, 240, 67, 80));
    c.patterns.push(p);
    c.arrangement = vec![Some(0), Some(0)];
    s.channels.push(c);
    s
}
fn non_silent(pcm: &[f32], frames: usize) {
    assert_eq!(pcm.len(), frames * 2);
    assert!(pcm.iter().all(|v| v.is_finite() && v.abs() <= 1.0));
    assert!(pcm.iter().any(|v| v.abs() > 1e-5));
}

#[test]
fn synthetic_arrangements_lengths_previews_release_and_pan() {
    for (kind, bench) in [
        (BankRef::Ps2, SoundBench::from_bank(bank(), None)),
        (BankRef::Ps1, SoundBench::ps1(&vab())),
    ] {
        let mut s = song(kind);
        let pcm = s.render(&bench);
        non_silent(&pcm, SAMPLE_RATE * 4);
        // The shared ADSR release finishes just after 0.5 seconds, not at the bench's
        // fixed audition key-off of duration minus 0.25 seconds.
        assert!(pcm[60000..90000].iter().all(|v| *v == 0.0));
        assert_eq!(
            s.render_bar_range(&bench, 1..2),
            pcm[SAMPLE_RATE * 4..].to_vec()
        );
        assert!(s.render_bar_range(&bench, 2..2).is_empty());
        s.channels[0].pan = 0;
        let left = s.render(&bench);
        assert!(left.chunks_exact(2).all(|v| v[1] == 0.0));
        s.channels[0].volume = 0;
        assert!(s.render(&bench).iter().all(|v| *v == 0.0));
        s.channels[0].instrument = ps2kit::song::Instrument::Program(100).into();
        assert!(s.render(&bench).iter().all(|v| *v == 0.0));
    }
}

#[test]
fn polyphony_channels_and_cross_bar_sustain_are_independent() {
    for (kind, bench) in [
        (BankRef::Ps2, SoundBench::from_bank(bank(), None)),
        (BankRef::Ps1, SoundBench::ps1(&vab())),
    ] {
        let mut s = song(kind);
        let mut long = Channel::new("Held", 0);
        let mut p = Pattern::new(3840);
        p.notes.push(Note::new(0, 3840, 60, 127));
        long.patterns.push(p);
        long.arrangement = vec![Some(0)];
        s.channels.push(long);
        let pcm = s.render(&bench);
        non_silent(&pcm, SAMPLE_RATE * 4);
        assert!(pcm[60000..90000].iter().any(|v| v.abs() > 0.001));
        assert_eq!(
            s.render_bar_range(&bench, 1..2),
            pcm[SAMPLE_RATE * 4..].to_vec()
        );
        // Lots of layered tracks trigger the uniform peak limiter.
        let loud = s.channels[1].clone();
        s.channels = vec![loud; 32];
        let pcm = s.render(&bench);
        assert!(pcm.iter().all(|v| v.abs() <= 1.0));
        assert!(pcm.iter().any(|v| v.abs() > 0.99));
    }
}

#[test]
fn noteoff_inside_explicit_tail_and_malformed_duration_are_bounded() {
    for (kind, bench) in [
        (BankRef::Ps2, SoundBench::from_bank(bank(), None)),
        (BankRef::Ps1, SoundBench::ps1(&vab())),
    ] {
        let mut s = song(kind);
        s.end_tick = Some(960);
        s.tail_ticks = 960;
        s.channels[0].arrangement = vec![Some(0)];
        s.channels[0].patterns[0].notes = vec![Note::new(0, 1440, 60, 127)];
        let pcm = s.render(&bench);
        non_silent(&pcm, SAMPLE_RATE * 2);
        assert!(pcm[SAMPLE_RATE * 2..SAMPLE_RATE * 3]
            .iter()
            .any(|v| v.abs() > 0.01));
        assert!(pcm[160000..].iter().all(|v| *v == 0.0));
        if kind == BankRef::Ps2 {
            assert!(s.to_sssq().is_err());
        }
        s.tail_ticks = u32::MAX;
        assert!(s.validate().is_err());
        s.tail_ticks = 0;
        s.bpm = 1;
        assert!(s.validate().is_err());
        let mut wide = song(kind);
        wide.bpm = 960;
        wide.ticks_per_beat = u16::MAX;
        wide.beats_per_bar = 32;
        wide.validate().unwrap();
        assert!(wide.render_bar_range(&bench, 8191..8192).is_empty());
        wide.channels[0].arrangement.resize(8192, None);
        assert!(wide.validate().is_err());
    }
}

#[test]
fn driver_clock_and_sssq_round_trip() {
    let mut s = song(BankRef::Ps2);
    s.bpm = 90;
    s.tail_ticks = 720;
    s.channels[0].patterns[0].notes = vec![Note::new(4, 480, 60, 127)];
    let data = s.to_sssq().unwrap();
    let seq = SoundSequence::new(&data).unwrap();
    let on = seq
        .events
        .iter()
        .find(|e| matches!(e.kind, EventKind::NoteOn { .. }))
        .unwrap();
    assert_eq!((on.tick, on.update), (4, 1));
    let b = bank();
    let t = DriverTables::computed();
    let mut reference = Vec::new();
    Synth {
        bank: &b,
        tables: &t,
    }
    .render(&seq, 127, 0.0, 1.0, 120.0, &mut reference);
    let bench = SoundBench::from_bank(b, Some(t));
    assert_eq!(s.render(&bench), reference);
    let imported = Song::from_sequence(&seq, "Round trip", 127).unwrap();
    let again = SoundSequence::new(&imported.to_sssq().unwrap()).unwrap();
    let notes = |q: &SoundSequence| {
        q.events
            .iter()
            .filter(|e| matches!(e.kind, EventKind::NoteOn { .. } | EventKind::NoteOff { .. }))
            .copied()
            .collect::<Vec<_>>()
    };
    assert_eq!(notes(&seq), notes(&again));
    let mut more = s.clone();
    more.channels.resize(17, Channel::new("Empty", 0));
    assert!(more.to_sssq().is_err());
    s.loop_bars = Some((0, 2));
    assert!(s.to_sssq().is_err());
}

#[test]
fn text_round_trip_unicode_empty_cells_and_good_errors() {
    let mut s = song(BankRef::Ps1);
    s.name = "A \"song\" \\ 日本語\n\r\t".into();
    s.channels[0].name = "".into();
    s.channels[0].arrangement.push(None);
    s.channels[0].patterns.push(Pattern::new(240));
    s.import_notes
        .push("Tempo flattened\nReason: constant grid".into());
    s.loop_bars = Some((1, 3));
    let text = s.to_text().unwrap();
    assert_eq!(Song::parse(&text).unwrap(), s);
    assert_eq!(
        Song::parse(&format!("# Comment\n\n{text}"))
            .unwrap()
            .to_text()
            .unwrap(),
        text
    );
    assert!(!text.contains("SShd") && !text.contains("pBAV"));
    for broken in [
        text.replace("psxsong 1", "psxsong 4"),
        text.replace("clock 120", "clock 0"),
        text.replace("arrangement 0", "arrangement 99"),
        text.replace("endpattern", "unknown"),
        text.replace("note 0 480", "note 0 0"),
        text.replace("name \"", "name \"\\q"),
        text.replace("loop 1 3", "loop 3 1"),
        text.replace("endchannel", ""),
        format!("{text}name \"duplicate\"\n"),
    ] {
        assert!(
            Song::parse(&broken).is_err(),
            "accepted malformed song: {broken}"
        );
    }
    let e = Song::parse("psxsong 1\nname\n").unwrap_err().to_string();
    assert!(e.contains("line 2"));
    let mut bad = s;
    bad.channels[0].patterns[0].notes[0].length = u32::MAX;
    assert!(bad.validate().is_err());
    assert!(bad.render(&SoundBench::ps1(&vab())).is_empty());
}

#[test]
fn parser_fuzz_truncation_mutation_and_numeric_extremes_never_panics() {
    let text = song(BankRef::Ps2).to_text().unwrap();
    for i in 0..text.len() {
        let _ = Song::parse(&text[..i]);
    }
    let mut state = 0x1234abcd_u32;
    for _ in 0..2000 {
        let mut bytes = text.as_bytes().to_vec();
        for _ in 0..8 {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            let at = state as usize % bytes.len();
            bytes[at] = (state >> 24) as u8;
        }
        let text = String::from_utf8_lossy(&bytes);
        if let Ok(s) = Song::parse(&text) {
            s.validate().unwrap();
            let _ = s.to_text().unwrap();
        }
    }
    for n in [
        "-1",
        "4294967295",
        "18446744073709551615",
        "NaN",
        "99999999999999999999999999",
    ] {
        assert!(
            Song::parse(&text.replace("clock 120 480 4", &format!("clock {n} {n} {n}"))).is_err()
        );
        assert!(
            Song::parse(&text.replace("note 0 480 60 100", &format!("note {n} {n} {n} {n}")))
                .is_err()
        );
    }
    assert!(Song::parse(&" ".repeat(4 * 1024 * 1024 + 1)).is_err());
}

#[test]
fn import_reports_tempo_changes_and_missing_noteoffs() {
    let s = song(BankRef::Ps2);
    let mut seq = SoundSequence::new(&s.to_sssq().unwrap()).unwrap();
    seq.events[0].kind = EventKind::Tempo(150);
    seq.events
        .retain(|e| !matches!(e.kind, EventKind::NoteOff { .. }));
    let imported = Song::from_sequence(&seq, "Changed tempo", 66).unwrap();
    assert_eq!(imported.bpm, 120);
    assert!(imported
        .import_notes
        .iter()
        .any(|s| s.contains("tempo 150")));
    assert!(imported
        .import_notes
        .iter()
        .any(|s| s.contains("missing note-off")));
    seq.events[0].tick = -1;
    assert!(Song::from_sequence(&seq, "Bad", 66).is_err());
}

fn rms(x: &[f32]) -> f64 {
    (x.iter().map(|&v| f64::from(v).powi(2)).sum::<f64>() / x.len().max(1) as f64).sqrt()
}
fn metrics(a: &[f32], b: &[f32]) -> (f64, f64, f64, f64) {
    assert_eq!(a.len(), b.len());
    let max_error = a
        .iter()
        .zip(b)
        .map(|(x, y)| f64::from((x - y).abs()))
        .fold(0.0, f64::max);
    let relative = (a
        .iter()
        .zip(b)
        .map(|(x, y)| (f64::from(*x) - f64::from(*y)).powi(2))
        .sum::<f64>()
        / a.len() as f64)
        .sqrt()
        / rms(b).max(1e-12);
    let envelope_db = a
        .chunks(SAMPLE_RATE)
        .zip(b.chunks(SAMPLE_RATE))
        .map(|(x, y)| (20.0 * (rms(x).max(1e-12) / rms(y).max(1e-12)).log10()).abs())
        .fold(0.0, f64::max);
    // Hann-window spectra at sixteen positions, 32 logarithmically spaced frequencies
    // from 40 Hz to 12 kHz, both stereo channels. Relative L2 energy deviation.
    let (mut error, mut power) = (0.0, 0.0);
    for window in 0..16 {
        let at = (a.len() / 2 - 2048) * window / 15;
        for bin in 0..32 {
            let frequency = 40.0 * 300f64.powf(bin as f64 / 31.0);
            for ch in 0..2 {
                let (mut ar, mut ai, mut br, mut bi) = (0.0, 0.0, 0.0, 0.0);
                for j in 0..2048 {
                    let angle = std::f64::consts::TAU * frequency * j as f64 / SAMPLE_RATE as f64;
                    let hann = 0.5 - 0.5 * (std::f64::consts::TAU * j as f64 / 2047.0).cos();
                    let (sn, cs) = angle.sin_cos();
                    let x = f64::from(a[(at + j) * 2 + ch]) * hann;
                    let y = f64::from(b[(at + j) * 2 + ch]) * hann;
                    ar += x * cs;
                    ai += x * sn;
                    br += y * cs;
                    bi += y * sn;
                }
                let ap = ar * ar + ai * ai;
                let bp = br * br + bi * bi;
                error += (ap - bp).powi(2);
                power += bp * bp;
            }
        }
    }
    (
        max_error,
        relative,
        envelope_db,
        (error / power.max(1e-24)).sqrt(),
    )
}

fn artifacts(label: &str, song: &Song, rendered: &[f32], driver: &[f32]) {
    if std::env::var_os("PSXSONG_AUDIO_CHECK").is_none() {
        return;
    }
    let dir = std::env::temp_dir().join("m1-song");
    std::fs::create_dir_all(&dir).unwrap();
    write_wav(
        dir.join(format!("{label}-song.wav")),
        rendered,
        SAMPLE_RATE as u32,
    )
    .unwrap();
    write_wav(
        dir.join(format!("{label}-driver.wav")),
        driver,
        SAMPLE_RATE as u32,
    )
    .unwrap();
    std::fs::write(
        dir.join(format!("{label}.psxsong")),
        song.to_text().unwrap(),
    )
    .unwrap();
}

fn raw_driver(label: &str, pcm: &[f32]) {
    if std::env::var_os("PSXSONG_AUDIO_CHECK").is_none() {
        return;
    }
    let path = std::env::temp_dir().join("m1-song");
    std::fs::create_dir_all(&path).unwrap();
    let bytes: Vec<u8> = pcm.iter().flat_map(|v| v.to_le_bytes()).collect();
    std::fs::write(path.join(format!("{label}-driver.f32")), bytes).unwrap();
}

#[test]
fn user_ps2_boot_import_matches_driver_waveform_envelope_and_spectrum() {
    for (file, label) in [
        ("scph39001.bin", "ps2-160a"),
        ("SCPH-70004_BIOS_V12_PAL_200.BIN", "ps2-200e"),
        ("scph10000.bin", "ps2-100j"),
    ] {
        let Ok(data) = std::fs::read(root().join("bios").join(file)) else {
            continue;
        };
        let rom = RomDir::new(data).unwrap();
        let song = Song::from_ps2_boot(&rom).unwrap();
        assert_eq!(
            (song.bpm, song.ticks_per_beat, song.channels.len()),
            (90, 480, 7)
        );
        assert!(
            song.import_notes.is_empty(),
            "{file}: {:?}",
            song.import_notes
        );
        let pcm = song.render(&SoundBench::ps2(&rom).unwrap());
        let boot = BootSound::load(&rom).unwrap();
        raw_driver(label, &boot.chime);
        raw_driver(&format!("{label}-cue"), &boot.cue);
        raw_driver(&format!("{label}-warning"), &boot.warning);
        let driver = boot.chime;
        let (max, relative, db, spectra) = metrics(&pcm, &driver);
        eprintln!("{label}: {} frames; RMS {:.9}; max error {max:.3e}; relative RMS error {relative:.3e}; 0.5 s envelope {db:.3e} dB; spectrum power L2 {spectra:.3e}", pcm.len() / 2, rms(&pcm));
        // Same pitches/envelopes and exact clock. Equal-time notes can accumulate in
        // channel order rather than stream order, causing only float-rounding error.
        assert!(max < 2e-6 && relative < 1e-5 && db < 1e-3 && spectra < 1e-4);
        artifacts(label, &song, &pcm, &driver);
    }
}

#[test]
fn user_ps1_licence_and_intro_imports_keep_scene_clocks() {
    for file in [
        "scph5501.bin",
        "Sony PlayStation BIOS (E)(v3.0)(1997-01-06)[SCPH-5502 + SCPH-5552].bin",
        "Sony PSone BIOS (U)(v4.5)(2000-05-25)[SCPH-101].bin",
        "Sony PlayStation BIOS (J)[SCPH-1000].bin",
    ] {
        let Ok(data) = std::fs::read(root().join("bios").join(file)) else {
            continue;
        };
        let bios = Ps1Bios::new(data).unwrap();
        let shell = Ps1Shell::load_ps1(&bios).unwrap();
        let bench = SoundBench::ps1(&shell.bank);
        let intro = SceIntro::locate(&bios.shell_image().unwrap(), bios.identity()).unwrap();
        for video in [VideoMode::Ntsc, VideoMode::Pal] {
            let licence = Song::from_ps1_licence(&shell, video).unwrap();
            let pcm = licence.render(&bench);
            let mut driver = shell.render_sound(video);
            let mode = if video == VideoMode::Pal {
                "pal"
            } else {
                "ntsc"
            };
            if file == "scph5501.bin" {
                raw_driver(&format!("ps1-licence-{mode}"), &driver);
            }
            normalize(&mut driver);
            non_silent(&pcm, driver.len() / 2);
            let song = Song::from_ps1_intro(&intro, video).unwrap();
            let ipcm = song.render(&bench);
            let mut idriver = intro.render_sound(&shell.bank, video);
            if file == "scph5501.bin" {
                raw_driver(&format!("ps1-intro-{mode}"), &idriver);
            }
            normalize(&mut idriver);
            non_silent(&ipcm, idriver.len() / 2);
            let max = |a: &[f32], b: &[f32]| {
                a.iter()
                    .zip(b)
                    .map(|(x, y)| (x - y).abs())
                    .fold(0.0f32, f32::max)
            };
            eprintln!(
                "{file} {video:?}: licence {} frames max {:.3e}; intro {} frames max {:.3e}",
                pcm.len() / 2,
                max(&pcm, &driver),
                ipcm.len() / 2,
                max(&ipcm, &idriver)
            );
            // PS1 sample-time casts can differ by one frame between equivalent formulas;
            // compare RMS energy as well as requiring exact overall scene lengths.
            assert!((rms(&pcm) / rms(&driver) - 1.0).abs() < 1e-3);
            assert!((rms(&ipcm) / rms(&idriver) - 1.0).abs() < 1e-3);
            if file == "scph5501.bin" {
                let mode = if video == VideoMode::Pal {
                    "pal"
                } else {
                    "ntsc"
                };
                artifacts(&format!("ps1-licence-{mode}"), &licence, &pcm, &driver);
                artifacts(&format!("ps1-intro-{mode}"), &song, &ipcm, &idriver);
            }
        }
    }
}

fn normalize(pcm: &mut [f32]) {
    let peak = pcm.iter().map(|v| v.abs()).fold(1.0f32, f32::max);
    for v in pcm {
        *v /= peak
    }
}

#[test]
fn derived_instruments_round_trip_and_extended_parser_stays_bounded() {
    let bench = SoundBench::from_bank(bank(), None);
    let mut s = song(BankRef::Ps2);
    let mut v = DrumVoice::new(SampleRef::program(BankRef::Ps2, 0, 60));
    v.pitch = -12.5;
    v.pitch_drop = 18.0;
    v.decay_ms = 250.0;
    v.low_pass_hz = Some(120.0);
    v.high_pass_hz = Some(20.0);
    v.retriggers_ms = vec![12.0, 24.0];
    v.noise_mix = Some(NoiseLayer::new(SampleRef::sample(BankRef::Ps2, 0), 0.2));
    let mut kit = Kit::default();
    kit.pieces.push(KitPiece::new(
        36,
        "Kick 日本語",
        v.clone(),
        "Measured from sample\nNo PCM",
    ));
    kit.omissions.push("Hat: no noise sample".into());
    for (name, instrument) in [
        ("One drum", Instrument::Drum(v)),
        ("Effects", Instrument::Effect(0)),
        ("Kit", Instrument::Kit(kit)),
    ] {
        let mut c = Channel::with_instrument(name, instrument);
        let mut p = Pattern::new(1920);
        p.notes = vec![Note::new(0, 1, 36, 110), Note::new(960, 1, 42, 100)];
        c.patterns.push(p);
        c.arrangement = vec![Some(0), Some(0)];
        s.channels.push(c);
    }
    let text = s.to_text().unwrap();
    assert!(text.starts_with("psxsong 2\n"));
    assert_eq!(Song::parse(&text).unwrap(), s);
    assert_eq!(Song::parse(&text).unwrap().to_text().unwrap(), text);
    assert_eq!(s.render(&bench), Song::parse(&text).unwrap().render(&bench));
    assert!(s.to_sssq().is_err());
    for i in 0..text.len() {
        if text.is_char_boundary(i) {
            let _ = Song::parse(&text[..i]);
        }
    }
    let mut state = 7u32;
    for _ in 0..1000 {
        let mut d = text.as_bytes().to_vec();
        for _ in 0..4 {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            let i = state as usize % d.len();
            d[i] = (state >> 24) as u8;
        }
        if let Ok(s) = Song::parse(&String::from_utf8_lossy(&d)) {
            s.validate().unwrap();
            Song::parse(&s.to_text().unwrap()).unwrap();
        }
    }
    assert!(Song::parse(&text.replace("psxsong 2", "psxsong 1")).is_err());
    for value in ["NaN", "inf", "-1", "100000000000000000000000000000"] {
        assert!(Song::parse(&text.replace(" 250 ", &format!(" {value} "))).is_err());
    }
    let voice = text.lines().find(|l| l.starts_with("voice - ")).unwrap();
    assert!(Song::parse(&text.replace(voice, "")).is_err());
    assert!(Song::parse(&text.replacen(voice, &format!("{voice}\n{voice}"), 1)).is_err());
    for value in [f32::NAN, f32::INFINITY, -1.0, 2000.0] {
        let mut v = DrumVoice::new(SampleRef::sample(BankRef::Ps2, 0));
        v.decay_ms = value;
        assert!(v.render(&bench).is_err());
    }
    let missing = DrumVoice::new(SampleRef::sample(BankRef::Ps1, 0));
    assert!(missing.render(&bench).unwrap().iter().all(|&v| v == 0.0));
    assert_eq!(
        DrumVoice::new(SampleRef::sample(BankRef::Ps2, 0))
            .render(&bench)
            .unwrap(),
        DrumVoice::new(SampleRef::program(BankRef::Ps2, 0, 60))
            .render(&bench)
            .unwrap()
    );
}

#[test]
fn transport_is_an_exact_tail_including_sustain_limiter_and_release() {
    for (kind, bench) in [
        (BankRef::Ps2, SoundBench::from_bank(bank(), None)),
        (BankRef::Ps1, SoundBench::ps1(&vab())),
    ] {
        let mut s = song(kind);
        s.bpm = 91;
        s.tail_ticks = 960;
        s.channels[0].patterns[0]
            .notes
            .push(Note::new(0, 1920, 60, 127));
        s.channels = vec![s.channels[0].clone(); 8];
        let pcm = s.render(&bench);
        for tick in [0, 4, 479, 480, 1920, 3839, 4000, 4800, u32::MAX] {
            let frame = s.frame_at_tick(tick).unwrap();
            assert_eq!(s.render_from(&bench, tick).unwrap(), pcm[frame * 2..]);
        }
        let rendered = s.render_with_positions(&bench, 257).unwrap();
        assert_eq!(rendered.pcm, pcm);
        assert!(s.render_with_positions(&bench, 0).is_err());
        assert_eq!(rendered.positions.first().unwrap().frames.start, 0);
        assert_eq!(rendered.positions.last().unwrap().frames.end, pcm.len() / 2);
        for blocks in rendered.positions.windows(2) {
            assert_eq!(blocks[0].frames.end, blocks[1].frames.start);
            assert!(blocks[0].position.tick <= blocks[1].position.tick);
        }
        for b in &rendered.positions {
            let p = b.position;
            assert!(s.frame_at_tick(p.tick).unwrap() <= b.frames.start);
            if p.tick < 4800 {
                assert!(s.frame_at_tick(p.tick + 1).unwrap() > b.frames.start);
            }
            assert_eq!(p.bar, p.tick / 1920);
            assert_eq!(u32::from(p.beat), p.tick / 480 % 4);
            assert_eq!(u32::from(p.tick_in_beat), p.tick % 480);
        }
    }
}

#[test]
fn numerical_spectrum_checks_have_known_frequency_references() {
    // Invented sine waves test the measuring code; they never enter kit derivation.
    for (frequency, low, high) in [(100.0, true, false), (8000.0, false, true)] {
        let pcm: Vec<_> = (0..24000)
            .flat_map(|i| {
                let v = (std::f32::consts::TAU * frequency * i as f32 / 48000.0).sin() * 0.2;
                [v, v]
            })
            .collect();
        let m = DrumMetrics::measure(&pcm);
        assert_eq!(m.below_150_hz > 0.95, low);
        assert_eq!(m.above_5khz > 0.95, high);
        assert!((m.duration_ms - 500.0).abs() < 0.01);
    }
    let m = DrumMetrics::measure(&[]);
    assert_eq!(m.rms, 0.0);
}
