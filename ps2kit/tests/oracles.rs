//! Checks against the user's own files and the outputs of the Python/Swift tooling.
//! Skipped (pass trivially) when the files are not present.
use ps2kit::bios::OpeningAssets;
use ps2kit::disc::DiscImage;
use ps2kit::history::PlayHistory;
use ps2kit::logo::{DiscLogo, LogoAnimation, LogoAssets, LogoMaster};
use ps2kit::rom::{unpack, RomDir};
use ps2kit::sim::{OpeningScene, Timeline};
use ps2kit::sound::BootSound;
use ps2kit::VideoMode;
use std::path::{Path, PathBuf};

fn root() -> PathBuf { Path::new(env!("CARGO_MANIFEST_DIR")).join("..") }
fn bios() -> Option<RomDir> { std::fs::read(root().join("SCPH-70004_BIOS_V12_PAL_200.BIN")).ok().map(|d| RomDir::new(d).unwrap()) }

#[test]
fn lz_unpack_matches_python() {
    let (Some(rom), Ok(expected)) = (bios(), std::fs::read(root().join("extracted/osdsys_200000.bin"))) else { return };
    let got = unpack(rom.module("OSDSYS").unwrap(), 0x100D80 - 0x100000 + 0x80).unwrap();
    assert_eq!(got.len(), expected.len());
    assert!(got == expected, "OSDSYS decompression differs from tools/osd_unpack.py");
}

#[test]
fn textures_match_tex2png() {
    let Some(rom) = bios() else { return };
    let assets = OpeningAssets::load(&rom).unwrap();
    for name in ["TEXOWAL0", "TEXOSCE", "TEXOPNGE", "TEXOBLP", "TEXOCRBL"] {
        let Ok(file) = std::fs::File::open(root().join(format!("extracted/png/{name}.png"))) else { continue };
        let mut reader = png::Decoder::new(file).read_info().unwrap();
        let mut buf = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buf).unwrap();
        let t = assets.texture(name).expect(name);
        assert_eq!((info.width as usize, info.height as usize), (t.width, t.height), "{name} size");
        // tex2png shows 8-bit masks (format 3/4) as grey for viewing; the loader keeps them as alpha.
        let same = if name == "TEXOBLP" {
            buf[..info.buffer_size()].chunks(4).zip(t.rgba.chunks(4)).all(|(a, b)| a[0] == b[3] && b[..3] == [0, 0, 0])
        } else {
            buf[..info.buffer_size()] == t.rgba[..]
        };
        assert!(same, "{name} pixels differ from tools/tex2png.py");
    }
}

#[test]
fn timelines_match_swift() {
    let n = Timeline::boot(0, VideoMode::Ntsc);
    let p = Timeline::boot(0, VideoMode::Pal);
    assert_eq!((n.end_frame(), n.dive_frame), (248, 122));
    assert_eq!((p.end_frame(), p.dive_frame), (207, 102));
    assert!((p.states().last().unwrap().z - 106.387).abs() < 0.01);
    // The console's integrator (velocity updated before the position step) parks the warning
    // camera at z = 800 after 111 NTSC frames; the notes' 103 is the closed-form estimate.
    let w = Timeline::warning(300, VideoMode::Ntsc);
    assert_eq!(w.frame_passing(800.0), 111);
    assert_eq!(Timeline::logo(VideoMode::Pal).end_frame(), 142);
}

#[test]
fn towers_match_swift() {
    let Some(rom) = bios() else { return };
    let assets = OpeningAssets::load(&rom).unwrap();
    for (titles, launches, expected) in [(21, 60, 126), (21, 40, 84), (14, 40, 56)] {
        let h = PlayHistory::synthetic(&vec![launches; titles], &[], 1);
        assert_eq!(OpeningScene::new(&assets, &h).towers.len(), expected);
    }
    let h = PlayHistory::synthetic(&[60; 21], &[], 1);
    let s = OpeningScene::new(&assets, &h);
    assert!(s.towers.iter().all(|t| t.brightness >= 32.0 * 0.1 && t.brightness <= 220.0));
}

#[test]
fn chime_matches_swift_render() {
    let (Some(rom), Ok(wav)) = (bios(), std::fs::read(root().join("extracted/snd_wav/SNDBOOTS_render.wav"))) else { return };
    let sound = BootSound::load(&rom).unwrap();
    // The Python render used a 61.04 Hz update clock and no transition cue; compare loudness
    // per half second within 1 dB over the first four seconds.
    let samples: Vec<f32> = wav[44..].chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0).collect();
    for i in 0..8 {
        let a = &sound.chime[i * 48000..(i + 1) * 48000];
        let b = &samples[i * 48000..(i + 1) * 48000];
        let rms = |x: &[f32]| (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt().max(1e-6);
        let db = 20.0 * (rms(a) / rms(b)).log10();
        assert!(db.abs() < 1.0, "half-second {i}: {db:.2} dB off the reference");
    }
}

#[test]
fn logo_and_disc() {
    let Some(rom) = bios() else { return };
    let logo = LogoAssets::load(&rom).unwrap();
    let anim = LogoAnimation::new(&logo, VideoMode::Pal);
    assert_eq!(anim.objects().len(), 4);
    assert_eq!(anim.objects()[3].keys.last().unwrap().0, 24);
    assert_eq!(anim.ribbon_offsets(0), [10, 7, 5, 2, 0]);
    let bitmap = DiscLogo::synthesised(&anim);
    assert!(bitmap.grey.iter().filter(|&&v| v > 128).count() > 2000);
    assert_eq!(logo.chime(0x12).len() % 2, 0);
    let home = std::env::var("HOME").unwrap();
    let Some(iso) = std::fs::read_dir(format!("{home}/PS2ISO")).ok().and_then(|d| d.filter_map(Result::ok).map(|e| e.path()).find(|p| p.extension().is_some_and(|e| e == "iso"))) else { return };
    let disc = DiscImage::open(&iso).unwrap();
    assert_eq!(disc.title_id().as_deref(), Some("SLES_530.64"));
    assert_eq!(disc.boot_elf.as_ref().unwrap().entry, 0x100008);
    assert_eq!(disc.logo_region, Some(LogoMaster::Europe));
    let l = DiscLogo::read(&iso).unwrap().bitmap(VideoMode::Pal);
    assert_eq!((l.width, l.height), (384, 77));
}

/// Every BIOS image in the local PCSX2 folder must load (or fail with a clear message).
#[test]
fn local_bios_versions_load() {
    let Some(home) = std::env::var_os("HOME") else { return };
    let dir = Path::new(&home).join("Library/Application Support/PCSX2/bios");
    let mut found = Vec::new();
    for entry in walk(&dir, 1) {
        let ext = entry.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase());
        if !matches!(ext.as_deref(), Some("bin") | Some("rom0")) { continue }
        let Ok(rom) = RomDir::new(std::fs::read(&entry).unwrap()) else { continue };
        let Ok(version) = rom.module("ROMVER").map(|v| String::from_utf8_lossy(&v[..14]).into_owned()) else { continue };
        // 1.00 (SCPH-10000) has a different OSD generation and is expected to be rejected clearly.
        match OpeningAssets::load(&rom) {
            Ok(a) => {
                assert!(a.texture("TEXOWAL0").is_some() && a.texture("TEXOSCE").is_some(), "{version}: textures");
                assert!(BootSound::load(&rom).is_ok(), "{version}: sound");
                assert!(LogoAssets::load(&rom).is_ok(), "{version}: logo");
                found.push(version);
            }
            Err(e) => assert!(version.starts_with("0100"), "{version}: {e}"),
        }
    }
    eprintln!("loaded: {found:?}");
}

fn walk(dir: &Path, depth: usize) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = e.path();
        if p.is_dir() { if depth > 0 { out.extend(walk(&p, depth - 1)) } } else { out.push(p) }
    }
    out
}

#[test]
fn ps1_shell_from_local_bios_and_discs() {
    let Some(rom) = bios() else { return };
    let shell = ps2kit::ps1::Ps1Shell::load(&rom).expect("rom0:LOGO assets");
    assert_eq!((shell.logo.verts.len(), shell.logo.normals.len(), shell.logo.prims.len()), (337, 153, 560));
    assert_eq!((shell.wordmark.width, shell.wordmark.height, shell.tm.width, shell.tm.height), (200, 40, 20, 8));
    assert_eq!(shell.events.len(), 19);
    assert_eq!(shell.translation, [0, -340, 5888]);
    assert_eq!(shell.font.width("Sony Computer En"), 188, "the width the shell's piece positions imply");
    let tris = shell.project(&shell.logo, 30);
    assert!(tris.len() > 200 && tris.len() < 560, "{} front faces", tris.len());
    let (xs, ys): (Vec<f32>, Vec<f32>) = tris.iter().flat_map(|t| t.xy).map(|p| (p[0], p[1])).unzip();
    let (x0, x1) = (xs.iter().cloned().fold(f32::MAX, f32::min), xs.iter().cloned().fold(f32::MIN, f32::max));
    let (y0, y1) = (ys.iter().cloned().fold(f32::MAX, f32::min), ys.iter().cloned().fold(f32::MIN, f32::max));
    assert!(x0 > 180.0 && x1 < 460.0 && y0 > 20.0 && y1 < 280.0, "logo spans {x0}..{x1} × {y0}..{y1} (the wordmark starts at y 280)");
    let black = shell.project(&shell.logo, 0);
    assert!(black.iter().all(|t| t.colour.iter().all(|c| *c < 0.02)), "first field is black");
    let pcm = shell.render_sound(VideoMode::Ntsc);
    assert!(pcm.len() > 48000 * 2 * 3 && pcm.iter().any(|s| s.abs() > 0.01));
    // Every PS1 disc in ~/PS2ISO must carry the shell's own logo.
    let Some(home) = std::env::var_os("HOME") else { return };
    for dir in std::fs::read_dir(Path::new(&home).join("PS2ISO")).into_iter().flatten().flatten() {
        let Some(bin) = std::fs::read_dir(dir.path()).into_iter().flatten().flatten().map(|e| e.path()).find(|p| p.file_name().is_some_and(|n| n.to_string_lossy().contains("Track 01"))) else { continue };
        if let Ok(d) = ps2kit::disc::DiscImage::open(&bin) {
            if d.kind == ps2kit::disc::DiscKind::Ps1 {
                let raw = ps2kit::sectors::SectorReader::open(&bin).unwrap().read(5, 7).unwrap();
                assert_eq!(&raw[..ps2kit::ps1::LOGO_TMD_BYTES], &shell.logo_bytes[..], "{}", bin.display());
            }
        }
    }
}
