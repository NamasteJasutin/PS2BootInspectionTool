//! Optional dump oracles and synthetic safety checks. Scene evidence: scenes §§1a/2/4.
use ps2kit::ps1::Ps1Shell;
use ps2kit::ps1bios::Ps1Bios;
use ps2kit::ps1intro::{SceIntro, ScePolygon};
use ps2kit::rom::{unpack, RomDir};
use ps2kit::{Error, Format, VideoMode};
use std::path::{Path, PathBuf};

fn root() -> PathBuf { Path::new(env!("CARGO_MANIFEST_DIR")).join("..") }

fn cases() -> Vec<(&'static str, usize, bool, bool)> {
    vec![
        ("Sony PlayStation BIOS (J)[SCPH-1000].bin", 4, false, false),
        ("Sony PlayStation BIOS (J)(v1.1)(1995-01-22)[SCPH-3000].bin", 4, false, false),
        ("Sony PlayStation BIOS (E)(v2.0)(1995-05-10)[SCPH-1002].bin", 8, false, false),
        ("Sony PlayStation BIOS (J)(v2.2)(1995-12-04)[SCPH-5000].bin", 8, false, false),
        ("Sony PlayStation SCPH-1001 - DTLH-3000 BIOS v2.2 (1995-12-04)(Sony)(US).bin", 8, false, false),
        ("Sony PlayStation BIOS (E)(v2.2)(1995-12-04)[DTLH-3002].bin", 8, false, false),
        ("Sony PlayStation BIOS (J)(v3.0)(1996-09-09)[SCPH-5500].bin", 8, false, false),
        ("scph5501.bin", 8, false, false),
        ("Sony PlayStation BIOS (U)(v3.0)(1996-11-18)[SCPH-7003].bin", 8, false, false),
        ("Sony PlayStation BIOS (E)(v3.0)(1997-01-06)[SCPH-5502 + SCPH-5552].bin", 8, false, false),
        ("Sony PlayStation BIOS (J)(v4.0)(1997-08-18)[SCPH-7000].bin", 8, true, false),
        ("Sony PlayStation BIOS (U)(v4.1)(1997-12-16)[SCPH-7001 + SCPH-9001].bin", 8, true, true),
        ("Sony PlayStation BIOS (E)(v4.1)(1997-12-16)[SCPH-7502 + SCPH-9002].bin", 8, true, true),
        ("Sony PSone BIOS (U)(v4.5)(2000-05-25)[SCPH-101].bin", 8, true, true),
    ]
}

#[test]
fn every_standalone_intro_assets_timeline_frames_and_sound() {
    for (file, lead, modern, pal_assets) in cases() {
        let Ok(d) = std::fs::read(root().join("bios").join(file)) else { continue };
        let bios = Ps1Bios::new(d).expect(file);
        let shell = bios.shell_image().unwrap();
        let intro = SceIntro::locate(&shell, bios.identity()).expect(file);
        let bank = Ps1Shell::load_ps1(&bios).unwrap().bank;
        assert_eq!(intro.logotypes.len(), if pal_assets { 5 } else { 3 }, "{file}");
        assert_eq!(intro.events.len(), 40);
        assert_eq!(intro.events.last().unwrap().time, 265);
        assert_eq!((intro.constants.size, intro.constants.height, intro.constants.brightness, intro.constants.grey), (256, 256, 70, 180));
        assert_eq!((intro.constants.scale, intro.constants.scale_delta), (4096, 2258));
        assert_eq!(intro.constants.pal_height, if pal_assets { 307 } else { 256 });
        assert!(intro.constants.evidence.iter().filter(|e| matches!(e.name, "size" | "brightness" | "grey" | "text_fade")).all(|e| e.offset.is_some()), "{file}: {:?}", intro.constants.evidence);
        for video in [VideoMode::Ntsc, VideoMode::Pal] {
            let t = intro.timeline(video);
            let pal = video == VideoMode::Pal && lead != 4;
            assert_eq!(t.video, if pal { VideoMode::Pal } else { VideoMode::Ntsc });
            assert_eq!((t.lead_in, t.fade, t.slide, t.hold, t.text_fade), (lead, if pal { 51 } else { 61 }, if pal { 50 } else { 60 }, if pal { 100 } else { 120 }, 17), "{file}");
            assert_eq!(t.total_fields(), if pal { 209 } else if lead == 4 { 245 } else { 249 });
            assert_eq!(t.music_clock, if pal { 74 } else { 89 });
            let first = intro.frame_for_video(0, video);
            assert_eq!(first.grey, 0);
            assert!(first.polygons.is_empty() && first.sprites.is_empty());
            let mut previous = 0;
            for f in 0..t.diamond_start() {
                let frame = intro.frame_for_video(f, video);
                assert!(frame.grey >= previous, "{file} background at {f}");
                previous = frame.grey;
                assert!(frame.polygons.is_empty() && frame.sprites.is_empty());
            }
            assert_eq!(previous, 180);
            let before = intro.frame_for_video(t.text_start() - 1, video);
            assert!(before.sprites.is_empty());
            for f in [t.diamond_start(), t.diamond_start() + 8, t.text_start(), 180, t.total_fields() - 1, usize::MAX] {
                let frame = intro.frame_for_video(f, video);
                assert_eq!(frame.grey, 180);
                assert_eq!(frame.polygons.len(), 3);
                for poly in &frame.polygons {
                    let vertices: &[_] = match poly { ScePolygon::Quad(v) => v, ScePolygon::Triangle(v) => v, _ => unreachable!() };
                    for v in vertices { assert!((0.0..640.0).contains(&v.xy[0]) && (0.0..480.0).contains(&v.xy[1]), "{file} field {f}: {v:?}") }
                }
            }
            let final_frame = intro.frame_for_video(t.total_fields() - 1, video);
            assert_eq!(final_frame, intro.frame_for_video(t.total_fields(), video));
            let expected_centres = if pal && pal_assets { vec![[338.0, 69.0], [338.0, 448.0]] }
                else if modern { vec![[320.0, 84.0], [320.0, 400.0], [436.0, 417.0]] }
                else { vec![[320.0, 80.0], [320.0, 412.0], [352.0, 365.0]] };
            assert_eq!(final_frame.sprites.iter().map(|s| s.centre).collect::<Vec<_>>(), expected_centres, "{file}");
            let expected_sizes = if pal && pal_assets { vec![[240.0, 44.0], [240.0, 54.0]] }
                else if modern { vec![[200.0, 36.0], [200.0, 44.0], [24.0, 10.0]] }
                else { vec![[240.0, 48.0], [240.0, 60.0], [24.0, 12.0]] };
            assert_eq!(final_frame.sprites.iter().map(|s| s.size).collect::<Vec<_>>(), expected_sizes);
            for f in 0..=17 {
                let frame = intro.frame_for_video(t.text_start() + f, video);
                assert!(frame.sprites.iter().all(|s| s.clut_level == (f + 1).min(17)));
            }
            let keys = intro.scheduled_events(video);
            assert_eq!(keys.len(), 44);
            assert_eq!(keys[0].0, 0);
            assert_eq!(keys[1].0, 0);
            assert_eq!(keys[2].0, t.diamond_start());
            assert_eq!(keys[3].0, t.diamond_start());
            assert_eq!(keys[4].0, t.diamond_start() + 1);
            assert_eq!(keys[40].0, t.total_fields());
            let pcm = intro.render_sound(&bank, video);
            assert_eq!(pcm.len(), t.sound_samples() * 2, "{file} sound length");
            assert!(pcm.iter().all(|v| v.is_finite()));
            let peak = pcm.iter().fold(0f32, |a, b| a.max(b.abs()));
            assert!(peak > 0.001, "{file}: silent intro ({peak})");
            assert!(pcm[..(t.diamond_start() * 800).min(pcm.len())].iter().any(|v| v.abs() > 0.001), "{file}: sweep must start under black/fade");
        }
        assert_eq!(intro.frame(180), intro.frame_for_video(180, intro.video));
        eprintln!("{file}: {} TIMs, 40 notes, {}/{} fields; fallback {:?}", intro.logotypes.len(), intro.timeline(VideoMode::Ntsc).total_fields(), intro.timeline(VideoMode::Pal).total_fields(), intro.constants.evidence.iter().filter(|e| e.offset.is_none()).map(|e| e.name).collect::<Vec<_>>());
    }
}

#[test]
fn pops_ps2_have_no_intro_and_corrupt_transfer_is_rejected() {
    if let Ok(d) = std::fs::read(root().join("bios/PSXONPSP660.BIN")) {
        let bios = Ps1Bios::new(d).unwrap();
        assert!(matches!(SceIntro::locate(&bios.shell_image().unwrap(), bios.identity()), Err(Error::NotFound(Format::Bios, _))));
    }
    let identity = synthetic_bios();
    for file in ["scph10000.bin", "scph39001.bin", "SCPH-70004_BIOS_V12_PAL_200.BIN", "DTL-H30101/DTL-H30101_USA_Dev_0150_20001228_v4_[CC645DA1].rom0"] {
        let Ok(d) = std::fs::read(root().join("bios").join(file)) else { continue };
        let rom = RomDir::new(d).unwrap();
        let shell = unpack(rom.module("LOGO").unwrap(), 0x54).unwrap();
        assert!(matches!(SceIntro::locate(&shell, identity.identity()), Err(Error::NotFound(Format::Bios, _))), "{file}");
    }
    let Ok(d) = std::fs::read(root().join("bios/Sony PlayStation BIOS (J)(v2.2)(1995-12-04)[SCPH-5000][h].bin")) else { return };
    assert!(Ps1Bios::new(d).is_err());
}

fn synthetic_bios() -> Ps1Bios {
    let mut d = vec![0; 0x80000];
    d[0x108..0x128].copy_from_slice(b"Sony Computer Entertainment Inc.");
    d[0x200..0x214].copy_from_slice(b"PS-X Realtime Kernel");
    d[0x18000..0x18004].copy_from_slice(&0x27BDFFE0u32.to_le_bytes());
    Ps1Bios::new(d).unwrap()
}

// Generated pixels and instructions, not a dump-derived fixture. Constants in scenes §1a.
fn synthetic_intro_shell() -> Vec<u8> {
    let mut d = vec![0; 0x3000];
    let put = |d: &mut [u8], at: usize, words: &[u32]| {
        for (i, w) in words.iter().enumerate() { d[at + i * 4..at + i * 4 + 4].copy_from_slice(&w.to_le_bytes()) }
    };
    let li = |reg: u32, value: u16| 0x24000000 | reg << 16 | u32::from(value);
    let jal = |o: u32| 0x0C000000 | ((0x80030000 + o) >> 2 & 0x03FFFFFF);
    let end = [0x03E00008, 0];
    put(&mut d, 0, &[0x27BDFFE0, jal(0x200), 0, jal(0x200), 0,
        li(14, 70), 0x3C018003, 0xA42E2804, 0x3C018003, li(15, 256), jal(0x400), 0xA42F2800,
        jal(0x600), 0, li(4, 180), li(5, 180), jal(0x800), li(6, 180),
        li(2, 240), li(2, 200), jal(0xA00), li(4, 17)]);
    put(&mut d, 88, &end);
    put(&mut d, 0x200, &[0x27BDFFE0, li(17, 4)]);
    put(&mut d, 0x210, &end);
    put(&mut d, 0x400, &[0x27BDFFE0, 0x3C058003, 0x24A51000,
        0x3C068003, 0x24C63000, 0x3C058003, 0x24A55100,
        li(14, 80), li(15, 412), li(24, 365)]);
    put(&mut d, 0x428, &end);
    put(&mut d, 0x600, &[0x27BDFFE0, li(14, 4096), li(15, 2258)]);
    put(&mut d, 0x610, &end);
    put(&mut d, 0x800, &[0x27BDFFE0]);
    put(&mut d, 0x804, &end);
    put(&mut d, 0xA00, &[0x27BDFFE0, li(18, 16), 0x3108001F]);
    put(&mut d, 0xA0C, &end);
    for i in 0..40 {
        let o = 0xB00 + i * 8;
        put(&mut d, o, &[i as u32 * 7]);
        d[o + 4..o + 8].copy_from_slice(&[1, 36, 127, 80]);
    }
    put(&mut d, 0xB00 + 40 * 8, &[9999, 0]);
    // Sparse 4-bit TIMs: alpha-zero pixels plus a synthetic solid green palette entry.
    for (o, w, h) in [(0x1000, 240usize, 48usize), (0x3000, 240, 60), (0x5100, 24, 12)] {
        let required = o + 64 + w * h / 2;
        d.resize(d.len().max(required), 0);
        put(&mut d, o, &[0x10, 8, 44, 0, 0x00010010]);
        d[o + 22..o + 24].copy_from_slice(&0x03E0u16.to_le_bytes());
        put(&mut d, o + 52, &[(12 + w * h / 2) as u32, 0, ((h as u32) << 16) | (w as u32 / 4)]);
        d[o + 64..required].fill(0x10);
    }
    d
}

#[test]
fn synthetic_intro_exercises_field_boundaries_pal_rounding_and_asset_validation() {
    let bios = synthetic_bios();
    let mut d = synthetic_intro_shell();
    let intro = SceIntro::locate(&d, bios.identity()).unwrap();
    let t = intro.timeline(VideoMode::Ntsc);
    assert_eq!(t.total_fields(), 249);
    assert_eq!(intro.frame(0).grey, 0);
    assert_eq!(intro.frame(40).grey, 96);
    assert_eq!(intro.frame(68).grey, 180);
    let moving = intro.frame(77);
    assert_eq!(moving.polygons.len(), 3);
    let ScePolygon::Triangle(v) = &moving.polygons[1] else { panic!("left half") };
    assert_eq!(v[0].xy, [204.0, 233.0]);
    assert!(moving.sprites.is_empty());
    assert_eq!(intro.frame(129).sprites[0].clut_level, 1);
    assert_eq!(intro.frame(145).sprites[0].clut_level, 17);
    assert_eq!(intro.frame(180), intro.frame(usize::MAX));
    let pal = intro.timeline(VideoMode::Pal);
    assert_eq!(pal.total_fields(), 209);
    assert_eq!(intro.frame_for_video(57, VideoMode::Pal).grey, 147);
    assert_eq!(intro.frame_for_video(58, VideoMode::Pal).grey, 180);
    let flat = intro.faded_logotype(0, 0).unwrap();
    assert_eq!(&flat.rgba[..8], &[0, 0, 0, 0, 180, 180, 180, 255]);
    assert_eq!(intro.faded_logotype(0, 17).as_ref(), Some(&intro.logotypes[0]));
    // A code-referenced TIM with a truncated pixel block must fail, even when others exist.
    d.truncate(0x5100 + 64 + 100);
    assert!(matches!(SceIntro::locate(&d, bios.identity()), Err(Error::Corrupt(Format::Bios, _))));
}

#[test]
fn malformed_tables_and_noise_fail_without_panicking() {
    let bios = synthetic_bios();
    let mut rng = ps2kit::history::SplitMix(97);
    for n in [0, 4, 8, 100, 1024, 0x80000] {
        let d: Vec<_> = (0..n).map(|_| rng.next_u64() as u8).collect();
        assert!(SceIntro::locate(&d, bios.identity()).is_err());
    }
    for tail in [vec![], vec![0; 400], vec![255; 400]] {
        let mut d = vec![0, 0, 0, 0, 1, 36, 127, 80];
        d.extend(tail);
        assert!(matches!(SceIntro::locate(&d, bios.identity()), Err(Error::Corrupt(Format::Bios, _))));
    }
}

#[test]
fn clut_interpolation_preserves_alpha_and_integer_rounding() {
    let Ok(d) = std::fs::read(root().join("bios/scph5501.bin")) else { return };
    let bios = Ps1Bios::new(d).unwrap();
    let intro = SceIntro::locate(&bios.shell_image().unwrap(), bios.identity()).unwrap();
    for (i, source) in intro.logotypes.iter().enumerate() {
        assert_eq!(intro.faded_logotype(i, 17).as_ref(), Some(source));
        assert_eq!(intro.faded_logotype(i, usize::MAX).as_ref(), Some(source));
        for level in 0..=17 {
            let faded = intro.faded_logotype(i, level).unwrap();
            for (a, b) in source.rgba.chunks_exact(4).zip(faded.rgba.chunks_exact(4)) {
                assert_eq!(a[3], b[3]);
                if a[3] == 0 { assert_eq!(a, b); continue }
                for c in 0..3 {
                    let five = (a[c] as usize * 31).div_ceil(255);
                    let expected = (five * level / 17 + 22 * (17 - level) / 17) * 255 / 31;
                    assert_eq!(b[c] as usize, expected);
                }
            }
        }
    }
    assert!(intro.faded_logotype(intro.logotypes.len(), 0).is_none());
}

#[test]
fn code_literals_and_note_table_survive_relocation_and_identity_changes() {
    let Ok(d) = std::fs::read(root().join("bios/scph5501.bin")) else { return };
    let bios = Ps1Bios::new(d).unwrap();
    let mut shell = bios.shell_image().unwrap();
    let original = SceIntro::locate(&shell, bios.identity()).unwrap();
    let at = original.constants.evidence.iter().find(|e| e.name == "brightness").unwrap().offset.unwrap();
    // Change the user input's brightness immediate; no Sony instructions/assets in fixtures.
    shell[at..at + 2].copy_from_slice(&55u16.to_le_bytes());
    let mut identity = bios.identity().clone();
    identity.version = "System ROM Version 4.5 05/25/00 A".into();
    let changed = SceIntro::locate(&shell, &identity).unwrap();
    assert_eq!(changed.constants.brightness, 55);
    assert_eq!(changed.logotypes, original.logotypes);
    let frame = changed.frame(77);
    let ScePolygon::Quad(v) = &frame.polygons[0] else { panic!("diamond quad first") };
    assert_eq!(v[0].colour, [140, 0, 0]);
    assert_eq!(v[1].colour, [140, 110, 0]);
    // The palette helper's argument is read from code even when it differs from 17.
    let at = original.constants.evidence.iter().find(|e| e.name == "text_fade").unwrap().offset.unwrap();
    shell[at..at + 2].copy_from_slice(&18u16.to_le_bytes());
    assert_eq!(SceIntro::locate(&shell, &identity).unwrap().timeline(VideoMode::Ntsc).text_fade, 18);
    // Move the note table: content locating still works without a fixed-address lookup.
    let at = shell.windows(8).position(|w| w == [0, 0, 0, 0, 1, 36, 127, 80]).unwrap();
    let table = shell[at..at + 41 * 8].to_vec();
    shell[at..at + 41 * 8].fill(0);
    shell.extend(table);
    let moved = SceIntro::locate(&shell, &identity).unwrap();
    assert_eq!(moved.events, original.events);
}
