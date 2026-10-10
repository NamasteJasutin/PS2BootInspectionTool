//! SCPH-10000 checks against the user's dumps; skip silently when an input is absent.
use ps2kit::bios::{OpeningAssets, COLUMNS, GROWTH_STEPS, ROWS};
use ps2kit::locate::ProgramImage;
use ps2kit::logo::{LogoAnimation, LogoAssets};
use ps2kit::rom::{unpack, RomDir};
use ps2kit::sound::{BootSound, DriverTables};
use ps2kit::VideoMode;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

fn root() -> PathBuf { Path::new(env!("CARGO_MANIFEST_DIR")).join("..") }
fn bios(name: &str) -> Option<RomDir> {
    let Ok(d) = std::fs::read(root().join("bios").join(name)) else { return None };
    Some(RomDir::new(d).expect("ROMDIR"))
}

#[test]
fn opening_100_has_the_grid_and_inline_textures() {
    let Some(rom) = bios("scph10000.bin") else { return };
    let a = OpeningAssets::load(&rom).expect("1.00 J opening plug-in");
    assert_eq!(a.rom_version(), "0100JC20000117");
    let cells: HashSet<_> = a.slots().iter().flatten().map(|s| (s.column, s.row)).collect();
    assert_eq!(cells.len(), COLUMNS * ROWS);
    for c in 0..COLUMNS {
        for r in 0..ROWS {
            assert!(cells.contains(&(c, r)));
            assert!(a.tower_position(c, r).expect("grid cell").is_finite());
        }
    }
    assert_eq!(a.textures().count(), 15);
    for (name, w, h) in [
        ("TEXOSCE", 256, 64), ("TEXOFOG0", 128, 128),
        ("TEXOFOG1", 64, 64), ("TEXOFOG2", 64, 64), ("TEXOFOG3", 64, 64), ("TEXOFOG4", 64, 64),
        ("TEXOWAL0", 256, 256), ("TEXOCRLE", 64, 64), ("TEXOCRBL", 64, 64), ("TEXOFLAR", 128, 128),
        ("TEXOREF", 128, 128), ("TEXOBLP", 64, 64), ("TEXOBLPR", 64, 64),
        ("TEXOPNGJ", 512, 128), ("TEXOPNGE", 512, 128),
    ] {
        let t = a.texture(name).expect(name);
        assert_eq!((t.width, t.height, t.rgba.len()), (w, h, w * h * 4), "{name}");
        assert!(t.rgba.chunks_exact(4).any(|p| p[3] != 0), "{name} is invisible");
    }
    assert_eq!(a.texture("TEXOWAL0").expect("wall").mip_levels, 2);
    for suffix in ["F", "S", "G", "I", "D", "P", "R", "K", "H", "C"] {
        assert!(a.texture(&format!("TEXOPNG{suffix}")).is_none());
    }
    assert!(a.cube_positions().iter().chain(a.prism_positions()).all(|p| p.is_finite()));
}

#[test]
fn opening_100_matches_160_growth_and_common_pixels() {
    let (Some(early), Some(later)) = (bios("scph10000.bin"), bios("scph39001.bin")) else { return };
    let a = OpeningAssets::load(&early).expect("1.00 opening");
    let b = OpeningAssets::load(&later).expect("1.60 opening");
    for step in 0..GROWTH_STEPS { assert_eq!(a.growth(step), b.growth(step), "growth step {step}") }
    assert_eq!(a.slots(), b.slots());
    assert_eq!(a.orb_colours(), b.orb_colours());
    assert_eq!(a.cube_positions(), b.cube_positions());
    assert_eq!(a.prism_positions(), b.prism_positions());
    for c in 0..COLUMNS {
        for r in 0..ROWS { assert_eq!(a.tower_position(c, r), b.tower_position(c, r)) }
    }
    for t in a.textures().filter(|t| !t.name.starts_with("TEXOPNG")) {
        let other = b.texture(&t.name).expect("named 1.60 counterpart");
        assert_eq!((t.width, t.height, t.mip_levels), (other.width, other.height, other.mip_levels), "{} layout", t.name);
        assert!(t.rgba == other.rgba, "{} decoded pixels", t.name);
    }
}

#[test]
fn raw_mopen_uses_the_descriptor_address() {
    let Some(rom) = bios("scph10000.bin") else { return };
    let descriptor = std::str::from_utf8(rom.module("OSOPEN").expect("OSOPEN")).expect("text");
    let mut lines = descriptor.lines();
    assert_eq!(lines.next(), Some("100"));
    let name = lines.next().expect("module name");
    let base = usize::from_str_radix(lines.next().expect("address"), 16).expect("hex address");
    let module = rom.module(name).expect("opening module");
    assert!(!module.starts_with(b"\x7fELF"));
    let img = ProgramImage::from_module(module, base).expect("raw LZ");
    assert_eq!((module.len(), img.data().len(), img.base()), (0x2F761, 0x92C8C, 0x500000));
    assert!(img.data() == unpack(module, 0).expect("LZ stream"));
}

#[test]
fn sound_and_logo_100_load() {
    let Some(rom) = bios("scph10000.bin") else { return };
    assert!(DriverTables::from_driver(rom.module("OSDSND").expect("OSDSND")).is_some());
    let sound = BootSound::load(&rom).expect("inline boot sound");
    for pcm in [&sound.chime, &sound.cue, &sound.warning] {
        assert_eq!(pcm.len() % 2, 0);
        assert!(pcm.len() > 48000);
        assert!(pcm.iter().all(|s| s.is_finite()));
        assert!(pcm.iter().any(|s| s.abs() > 0.001));
    }
    let logo = LogoAssets::load(&rom).expect("two-segment PS2LOGO");
    let anim = LogoAnimation::new(&logo, VideoMode::Ntsc);
    assert_eq!(anim.objects().len(), 4);
    assert!(!logo.objects.contains_key(&VideoMode::Pal), "no invented PAL keyframes");
    assert!(!logo.sample_body().is_empty());
    assert!(logo.chime(0x12).iter().any(|s| s.abs() > 0.001));

    let Some(later) = bios("scph39001.bin") else { return };
    let other = BootSound::load(&later).expect("1.60 archived sound");
    assert!(sound.chime == other.chime, "inline and archived chime PCM differ");
    assert!(sound.cue == other.cue, "inline and archived transition PCM differ");
    assert!(sound.warning == other.warning, "inline and archived warning PCM differ");
}

fn word(d: &mut [u8], p: usize, v: u32) { d[p..p + 4].copy_from_slice(&v.to_le_bytes()) }

#[test]
fn raw_lz_and_multiple_elf_segments_are_bounded() {
    let mut raw = 3u32.to_le_bytes().to_vec();
    raw.extend_from_slice(&[0; 4]);
    raw.extend_from_slice(b"abc");
    let img = ProgramImage::from_module(&raw, 0x765400).expect("literal raw stream");
    assert_eq!((img.base(), img.data()), (0x765400, &b"abc"[..]));
    for len in 0..raw.len() { assert!(ProgramImage::from_module(&raw[..len], 0).is_err()) }
    assert!(unpack(&raw, usize::MAX).is_err());

    let mut elf = vec![0; 0x105];
    elf[..4].copy_from_slice(b"\x7fELF");
    word(&mut elf, 0x1C, 0x34);
    elf[0x2A..0x2E].copy_from_slice(&[32, 0, 2, 0]);
    for (i, vaddr, file, filesz, memsz) in [(0, 0x3000, 0x100, 3, 8), (1, 0x3020, 0x103, 2, 4)] {
        let p = 0x34 + i * 32;
        for (offset, value) in [(0, 1), (4, file), (8, vaddr), (16, filesz), (20, memsz)] { word(&mut elf, p + offset, value) }
    }
    elf[0x100..].copy_from_slice(b"abcde");
    let img = ProgramImage::from_module(&elf, 0).expect("two PT_LOAD segments");
    assert_eq!((img.base(), img.data().len()), (0x3000, 0x24));
    assert_eq!(&img.data()[..3], b"abc");
    assert!(img.data()[3..0x20].iter().all(|&b| b == 0));
    assert_eq!(&img.data()[0x20..], b"de\0\0");
    assert!(ProgramImage::from_module(&elf[..0x104], 0).is_err());
    word(&mut elf, 0x34 + 32 + 8, 0x8000_0000);
    assert!(ProgramImage::from_module(&elf, 0).is_err(), "oversized address span");
}
