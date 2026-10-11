//! Synthetic and user-dump checks for the asset bench. Dump assertions cite sound.md §3,
//! ps1_shell_scenes.md §2, ps1_version_matrix.md §2.1, and menu_survey.md §4.
use ps2kit::bench::{FntSheet, FontBench, FontmFont, GaijiFont, KromFont, SoundBench};
use ps2kit::ps1::Vab;
use ps2kit::ps1bios::{audit, Ps1Bios};
use ps2kit::rom::{unpack, RomDir};
use ps2kit::sound::{DriverTables, ProgramKind, SoundBank};
use ps2kit::banks::BankRef;
use std::ops::Bound;
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
    // Nonzero first block marks loop start; the second ends with repeat enabled.
    let mut d = vec![0x55; 32];
    d[0] = 0;
    d[1] = 4;
    d[16] = 0;
    d[17] = 3;
    d
}

fn vab_bytes() -> Vec<u8> {
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
    d[tone + 6] = 0;
    d[tone + 7] = 127;
    put16(&mut d, tone + 16, 0x00FF);
    put16(&mut d, tone + 22, 1);
    put16(&mut d, table + 2, 4);
    d[body..].copy_from_slice(&adpcm());
    d
}

fn bank_header() -> Vec<u8> {
    let mut h = vec![0; 0x60];
    put32(&mut h, 0, 0x60);
    put32(&mut h, 4, 32);
    h[0xC..0x10].copy_from_slice(b"SShd");
    put32(&mut h, 0x10, 0x20);
    put32(&mut h, 0x14, u32::MAX);
    put32(&mut h, 0x2C, u32::MAX);
    put16(&mut h, 0x20, 0);
    put16(&mut h, 0x22, 0x10);
    h[0x30] = 0;
    h[0x31] = 127;
    let t = 0x38;
    h[t] = 0;
    h[t + 1] = 127;
    h[t + 2] = 60;
    put16(&mut h, t + 6, 0x00FF);
    h[t + 11] = 127;
    h[t + 12] = 64;
    h[t + 13] = 2;
    h
}

// A literal-only OSD LZ stream, containing wholly synthetic bytes.
fn packed(d: &[u8]) -> Vec<u8> {
    let mut out = (d.len() as u32).to_le_bytes().to_vec();
    for chunk in d.chunks(30) {
        out.extend_from_slice(&[0; 4]);
        out.extend_from_slice(chunk)
    }
    out
}

fn archive(assets: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let count = assets.len() + 2;
    let mut data = vec![0; (count + 1) * 16];
    data[..5].copy_from_slice(b"RESET");
    data[16..22].copy_from_slice(b"ROMDIR");
    let dir_size = data.len() as u32;
    put32(&mut data, 28, dir_size);
    for (i, (name, bytes)) in assets.iter().enumerate() {
        let p = (i + 2) * 16;
        data[p..p + name.len()].copy_from_slice(name.as_bytes());
        put32(&mut data, p + 12, bytes.len() as u32);
        data.extend_from_slice(bytes);
        data.resize(data.len().div_ceil(16) * 16, 0);
    }
    data
}

fn fontm_bytes() -> Vec<u8> {
    let mut d = vec![0; 0x20 + 2 * 338];
    d[..4].copy_from_slice(b"FBJ2");
    put32(&mut d, 12, 0x10);
    put32(&mut d, 16, 2);
    put32(&mut d, 24, 0x10);
    put32(&mut d, 28, 0x10 + 338);
    d[0x20] = 0xF1;
    d[0x20 + 337] = 0x2E;
    d[0x20 + 338] = 0x80;
    d
}

fn assert_pcm(pcm: &[f32], frames: usize) {
    assert_eq!(pcm.len(), frames * 2);
    assert!(pcm.iter().all(|s| s.is_finite()));
    assert!(pcm.iter().any(|s| s.abs() > 1e-5), "silent render");
}

#[test]
fn synthetic_banks_share_the_mixers() {
    let v = SoundBench::ps1(&Vab::parse(&vab_bytes()).unwrap());
    assert_eq!((v.programs.len(), v.samples.len()), (1, 1));
    let t = &v.programs[0].tones[0];
    assert_eq!((t.root, t.adsr1, t.sample_index), (60, 255, 0));
    assert_eq!((v.samples[0].length, v.samples[0].loop_start), (56, Some(0)));
    assert_pcm(&v.render(0, 60, 127, 0.25), 12000);
    let bank = SoundBank::new(&bank_header(), adpcm()).unwrap();
    let p = SoundBench::from_bank(bank, Some(DriverTables::computed()));
    assert_eq!((p.programs.len(), p.samples.len()), (1, 1));
    assert_eq!(p.programs[0].kind, ProgramKind::Split);
    assert_eq!(p.samples[0].loop_start, Some(0));
    assert_pcm(&p.render(0, 60, 127, 0.25), 12000);
    for bench in [v, p] {
        for duration in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::MAX] {
            assert!(bench.render(0, 60, 127, duration).is_empty());
        }
        for (program, note, velocity) in
            [(999, 60, 127), (-1, 60, 127), (0, i32::MIN, 127), (0, i32::MAX, 127), (0, 60, 0), (0, 60, -1)]
        {
            let pcm = bench.render(program, note, velocity, 0.01);
            assert_eq!(pcm.len(), 960);
            assert!(pcm.iter().all(|&s| s == 0.0));
        }
        assert_eq!(bench.render(0, 60, 127, 1.5 / 48000.0).len(), 4);
    }
}

fn effect_header() -> Vec<u8> {
    let mut d = vec![0xFF; 0x80 + 0x20 + 0x40];
    d[12..16].copy_from_slice(b"SShd");
    put32(&mut d, 0x2C, 0x80);
    put32(&mut d, 0x80, 127);
    put32(&mut d, 0x84, 0);
    let p = 0xA0;
    put16(&mut d, p, 0x2000);
    put16(&mut d, p + 2, 0);
    put16(&mut d, p + 4, 0x1000);
    put16(&mut d, p + 8, 0x00FF);
    put16(&mut d, p + 10, 0);
    put32(&mut d, p + 16, 0);
    d
}

#[test]
fn synthetic_nested_effect_bank_is_audible_and_panned() {
    let snd = archive(&[
        ("SNDBOOTH", packed(&bank_header())),
        ("SNDBOOTB", packed(&adpcm())),
        ("SNDOSDDH", packed(&effect_header())),
        ("SNDOSDDB", packed(&adpcm())),
    ]);
    let rom = RomDir::new(archive(&[("SNDIMAGE", snd)])).unwrap();
    let bench = SoundBench::ps2(&rom).unwrap();
    assert_eq!((bench.effects.len(), bench.samples.len()), (1, 2));
    assert_eq!(bench.effects[0].sample_index, 1);
    let pcm = bench.render_effect(0, 0.2);
    assert_pcm(&pcm, 9600);
    assert!(pcm.chunks_exact(2).all(|f| f[1] == 0.0));
    assert!(bench.render_effect(999, 0.1).iter().all(|&s| s == 0.0));
}

#[test]
fn synthetic_split_layer_and_key_ranges() {
    let mut h = bank_header();
    h.resize(0x80, 0);
    h[0x30] = 1;
    let tone = h[0x38..0x48].to_vec();
    h[0x48..0x58].copy_from_slice(&tone);
    h[0x38] = 40;
    h[0x39] = 80;
    h[0x48] = 40;
    h[0x49] = 80;
    let split = SoundBench::from_bank(SoundBank::new(&h, adpcm()).unwrap(), None);
    h[0x30] |= 0x80;
    let layer = SoundBench::from_bank(SoundBank::new(&h, adpcm()).unwrap(), None);
    let a = split.render(0, 60, 127, 0.1);
    let b = layer.render(0, 60, 127, 0.1);
    assert!(a.iter().zip(b).all(|(&a, b)| (b - a * 2.0).abs() < 1e-6));
    assert!(split.render(0, 30, 127, 0.1).iter().all(|&s| s == 0.0));
    assert_eq!(split.samples.len(), 1, "deduplicate a sample shared by tones");
}

// Deliberately invented runs, with a different mapping from Sony's font tables.
fn synthetic_krom_context() -> Vec<u8> {
    let mut context = vec![0; 24 + 51 * 4];
    for i in 0..19 {
        let (code, index) = if i == 18 { (0x849F, 492) } else { (0x8140 + i * 4, i * 2) };
        put16(&mut context, 24 + i as usize * 4, code);
        put16(&mut context, 26 + i as usize * 4, index);
    }
    for i in 0..32 {
        let code = if i == 31 { 0x9840 } else { 0x889F + i * 0x80 };
        put16(&mut context, 24 + (19 + i as usize) * 4, code);
        put16(&mut context, 26 + (19 + i as usize) * 4, (u32::from(i) * 2914 / 31) as u16);
    }
    context
}

#[test]
fn synthetic_one_bit_fonts_mapping_and_atlas_ranges() {
    let mut raw = vec![0; 3489 * 30];
    raw[0] = 0x80;
    raw[1] = 1;
    raw[28] = 0;
    raw[29] = 0x80;
    let context = synthetic_krom_context();
    let f = KromFont::new(raw.clone(), Some(&context));
    let g = f.glyph(0).unwrap();
    assert_eq!((g.width, g.height), (16, 16));
    assert_eq!(&g.rgba[0..4], &[255; 4]);
    assert_eq!(&g.rgba[15 * 4..16 * 4], &[255; 4]);
    assert_eq!(&g.rgba[4..8], &[0; 4]);
    assert!(g.rgba[15 * 16 * 4..].iter().all(|&b| b == 0));
    assert_eq!(f.code_to_index(0x8141), Some(1));
    assert_eq!(f.code_to_index(0x8142), None, "gap after a synthetic run");
    assert_eq!(f.krom2rawadd(0x889F), Some(0xBFC6_9D68));
    assert_eq!(f.code_to_index(0x84BE), Some(523));
    assert_eq!(f.code_to_index(0x9872), Some(3488));
    for code in [0, 0x84BF, 0x9873, 0xFFFF] {
        assert!(f.glyph_by_code(code).is_none())
    }
    assert!(KromFont::new(raw.clone(), None).code_to_index(0x8140).is_none());
    assert!(KromFont::new(raw, Some(&context[..50])).code_to_index(0x8140).is_none());
    let (w, h, rgba) = f.atlas(0..2);
    assert_eq!((w, h, rgba.len()), (32, 16, 32 * 16 * 4));
    assert_eq!(f.atlas((Bound::Excluded(usize::MAX), Bound::Unbounded)), (0, 0, Vec::new()));
    let gaiji = GaijiFont::new(vec![0xAA; 60]);
    assert_eq!(gaiji.glyph_count(), 2);
    assert_eq!(gaiji.glyph(1).unwrap().rgba[14 * 16 * 4 + 3], 255);
    assert_eq!(gaiji.atlas(1..=usize::MAX).0, 16);
    assert!(gaiji.glyph(usize::MAX).is_none());
}

#[test]
fn synthetic_four_bit_fonts_and_sheets() {
    let f = FontmFont::new(fontm_bytes()).unwrap();
    assert_eq!((f.glyph_count(), f.available_glyph_count()), (2, 2));
    let g = f.glyph(0).unwrap();
    assert_eq!((g.width, g.height), (26, 26));
    assert_eq!(&g.rgba[..8], &[255, 255, 255, 255, 255, 255, 255, 17]);
    assert_eq!(g.rgba[26 * 26 * 4 - 1], 238);
    let mut bad_offset = fontm_bytes();
    put32(&mut bad_offset, 24, u32::MAX);
    let bad = FontmFont::new(bad_offset).unwrap();
    assert!(bad.glyph(0).is_none());
    assert_eq!(bad.available_glyph_count(), 1);
    let (w, h, rgba) = f.atlas(1..2);
    assert_eq!((w, h, rgba.len()), (26, 26, 26 * 26 * 4));
    let mut extra = vec![0; 64 + 16 * 338];
    for i in 0..5 {
        put32(&mut extra, i * 4, (64 + 16 * 338) as u32)
    }
    extra[64] = 0x0F;
    let mut add_pal = [[0; 4]; 16];
    add_pal[15] = [255; 4];
    assert_eq!(FontmFont::additional(extra, &add_pal).unwrap().glyph(0).unwrap().rgba[3], 255);
    let mut pal = [[0; 4]; 16];
    pal[1] = [10, 20, 30, 40];
    pal[2] = [50, 60, 70, 80];
    let mut raw = vec![0; 64 * 40 / 2];
    raw[0] = 0x21;
    raw[16] = 0x12;
    let sheet = FntSheet::new("fake", &raw, 64, 40, 2, 2, "synthetic", Some(&pal)).unwrap();
    assert_eq!(&sheet.glyph(0).unwrap().rgba[..8], &[10, 20, 30, 40, 50, 60, 70, 80]);
    assert_eq!(&sheet.glyph(1).unwrap().rgba[..4], &[50, 60, 70, 80]);
    assert_eq!(sheet.atlas(..).0, 64);
    for (w, h, per_row, count) in
        [(0, 40, 1, 1), (63, 40, 1, 1), (64, 40, 0, 1), (64, 40, 3, 1), (64, 40, 2, 3), (usize::MAX, usize::MAX, 1, 1)]
    {
        assert!(FntSheet::new("bad", &raw, w, h, per_row, count, "", Some(&pal)).is_err());
    }
    assert!(FntSheet::new("no palette", &raw, 64, 40, 2, 2, "", None).is_err());
}

fn dump_paths(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut paths = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            paths.extend(dump_paths(&path))
        } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("bin") || e == "rom0") {
            paths.push(path)
        }
    }
    paths.sort();
    paths
}

fn check_krom(data: &[u8], fonts: &FontBench) {
    let krom = fonts.krom().expect("KROM");
    assert_eq!(krom.glyph_count(), 3489);
    for (code, index) in
        [(0x8140, 0), (0x824F, 147), (0x8260, 157), (0x8281, 183), (0x829F, 209), (0x889F, 524), (0x9872, 3488)]
    {
        assert_eq!(krom.code_to_index(code), Some(index));
        let addr = krom.krom2rawadd(code).unwrap();
        assert_eq!(addr, 0xBFC6_6000 + index as u32 * 30);
        // Independently compare the decoded bits with the actual font ROM rows.
        let raw = &data[(addr - 0xBFC0_0000) as usize..][..30];
        let glyph = krom.glyph_by_code(code).unwrap();
        for row in 0..15 {
            let bits = u16::from_be_bytes([raw[row * 2], raw[row * 2 + 1]]);
            for col in 0..16 {
                assert_eq!(glyph.rgba[(row * 16 + col) * 4 + 3] != 0, bits & (1 << (15 - col)) != 0)
            }
        }
    }
    assert!(krom.glyph_by_code(0x8260).unwrap().rgba.iter().any(|&b| b != 0));
    for code in [0x817F, 0x81AD, 0x8230, 0x897F, 0x88FD, 0x9873] {
        assert_eq!(krom.code_to_index(code), None)
    }
}

#[test]
fn every_local_dump_lists_and_renders_its_assets() {
    for path in dump_paths(&root().join("bios")) {
        let Ok(data) = std::fs::read(&path) else { continue };
        if audit(&data).first_shifted_offset.is_some() && data.len() < 0x90000 {
            // The [h] dump's CRLF transfer corruption is identified rather than rendered.
            assert!(SoundBench::from_bytes(&data).is_err());
            assert!(FontBench::from_bytes(&data).is_err());
            continue;
        }
        if Ps1Bios::detect(&data) {
            let bios = Ps1Bios::new(data.clone()).unwrap();
            let sounds = SoundBench::from_bytes(&data).unwrap();
            let shell = bios.shell_image().unwrap();
            let bank = shell.windows(4).position(|w| w == b"pBAV").unwrap();
            // Vab currently models the shell mixer. Verify that its full-range assumption
            // agrees with every tone's actual low/high bytes in every supplied PS1 dump.
            for program in &sounds.programs {
                for (i, tone) in program.tones.iter().enumerate() {
                    let offset = bank + 32 + 128 * 16 + (program.id as usize * 16 + i) * 32;
                    assert_eq!((tone.low, tone.high), (shell[offset + 6] as i32, shell[offset + 7] as i32));
                }
            }
            assert_eq!(sounds.programs.iter().map(|p| p.id).collect::<Vec<_>>(), [0, 1, 2, 3], "{}", path.display());
            assert_eq!(sounds.samples.len(), 3);
            // scenes §2: drones/bass VAG 1, sweep VAG 3, drone VAG 2, chime VAG 1.
            for (id, sample) in [(0, 0), (1, 2), (2, 1), (3, 0)] {
                let program = sounds.program(id).unwrap();
                assert_eq!(program.tones.len(), 2);
                assert!(program.tones.iter().all(|t| t.sample_index == sample));
                assert_pcm(&sounds.render(id, 60, 127, 0.5), 24000);
            }
            let fonts = FontBench::ps1(&data).unwrap();
            check_krom(&data, &fonts);
            if matches!(bios.identity().region_letter, Some('A' | 'E'))
                || path.file_name().is_some_and(|n| n == "PSXONPSP660.BIN")
            {
                assert_eq!(fonts.gaiji().unwrap().glyph_count(), 245);
            } else {
                assert!(fonts.gaiji().is_none(), "Japanese shell bytes are not gaiji: {}", path.display())
            }
        } else if let Ok(rom) = RomDir::new(data.clone()) {
            if rom.module("OSDSYS").is_err() {
                continue;
            }
            let sounds = SoundBench::ps2(&rom).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            assert_eq!(
                sounds.programs.iter().map(|p| p.id).collect::<Vec<_>>(),
                [0, 1, 2, 40, 41, 42, 50, 51, 52, 53, 60, 61, 62, 70, 100, 101, 102]
            );
            let inline = rom.module("SNDIMAGE").is_err();
            assert_eq!((sounds.effects.len(), sounds.samples.len()), (if inline { 12 } else { 14 }, 16));
            for program in &sounds.programs {
                assert!(!program.tones.is_empty());
                assert!(program.tones.iter().all(|t| sounds.sample(t.sample_index).is_some()));
                assert_pcm(&sounds.render(program.id, 60, 127, 0.5), 24000);
            }
            for effect in &sounds.effects {
                assert_pcm(&sounds.render_effect(effect.id, 0.2), 9600)
            }
            assert!(sounds.samples.iter().all(|s| s.length > 0 && s.loop_start.is_none_or(|l| l < s.length)));
            let fonts = FontBench::ps2(&rom).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            check_krom(&data, &fonts);
            assert_eq!(fonts.gaiji().unwrap().glyph_count(), 245);
            let fontm = fonts.fontm().unwrap();
            assert_eq!((fontm.glyph_count(), fontm.available_glyph_count()), (4418, 4416));
            assert!(fontm.glyph(4415).is_some());
            assert!(fontm.glyph(4416).is_none());
            assert!(fontm.glyph(4417).is_none());
            if let Ok(module) = rom.module("FNTIMAGE") {
                let archive = RomDir::new(module.to_vec()).unwrap();
                for (name, count) in [
                    ("FNTASCII", 96),
                    ("FNTASCI2", 96),
                    ("FNTEX000", 304),
                    ("FNTEX001", 304),
                    ("FNTEXOSD", 32),
                    ("FNTADD00", 672),
                ] {
                    if archive.module(name).is_ok() {
                        let font = fonts.font(name).unwrap();
                        assert_eq!(font.glyph_count(), count);
                        assert!(font.glyph(count - 1).is_some());
                    }
                }
            }
        }
    }
}

#[test]
fn named_ps2_100j_and_200e_banks_match() {
    let (Ok(a), Ok(b)) = (
        std::fs::read(root().join("bios/scph10000.bin")),
        std::fs::read(root().join("bios/SCPH-70004_BIOS_V12_PAL_200.BIN")),
    ) else {
        return;
    };
    let a = SoundBench::from_bytes(&a).unwrap();
    let b = SoundBench::from_bytes(&b).unwrap();
    assert_eq!(a.programs, b.programs);
    assert_eq!(a.samples, b.samples);
    assert_eq!(a.effects, b.effects[..12]);
    assert_eq!(a.render(0, 60, 127, 0.5), b.render(0, 60, 127, 0.5));
}

#[test]
fn logo_effect_bank_matches_the_existing_chime_voices() {
    for file in ["scph10000.bin", "scph39001.bin", "SCPH-70004_BIOS_V12_PAL_200.BIN"] {
        let Ok(data) = std::fs::read(root().join("bios").join(file)) else { continue; };
        let rom = RomDir::new(data).unwrap();
        let assets = ps2kit::logo::LogoAssets::load(&rom).unwrap();
        let bench = SoundBench::ps2_logo(&rom).unwrap();
        assert_eq!(bench.bank, BankRef::Ps2Logo);
        assert!(bench.programs.is_empty());
        assert_eq!(bench.samples.len(), 1);
        assert_eq!(bench.effects.len(), assets.effects.len());
        assert_eq!(bench.samples[0].length, 19936);
        assert_eq!(bench.samples[0].loop_start, None);
        let (pcm, lp) = ps2kit::sound::decode_adpcm(assets.sample_body(), 0);
        assert_eq!(bench.sample_pcm(0), Some((pcm.as_slice(), lp)));
        for (effect, existing) in bench.effects.iter().zip(&assets.effects) {
            assert_eq!(i64::from(effect.pitch), existing.pitch);
            assert_eq!(effect.adsr, (existing.adsr1, existing.adsr2));
            assert_eq!((i64::from(effect.volume.0), i64::from(effect.volume.1)), (existing.vol_l, existing.vol_r));
            assert_pcm(&bench.render_effect(effect.id, 0.5), 24000);
        }
        assert_eq!(bench.by_style().iter().map(|(_, entries)| entries.len()).sum::<usize>(), bench.effects.len());
    }
}

#[test]
fn font_palette_matches_the_users_extracted_oracle() {
    let Ok(data) = std::fs::read(root().join("bios/SCPH-70004_BIOS_V12_PAL_200.BIN")) else { return };
    let fonts = FontBench::from_bytes(&data).unwrap();
    for name in ["FNTASCII", "FNTEX000", "FNTEX001", "FNTEXOSD"] {
        let Ok(file) = std::fs::File::open(root().join(format!("extracted/png/font_{name}.png"))) else { continue };
        let mut reader = png::Decoder::new(file).read_info().unwrap();
        let mut buf = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buf).unwrap();
        assert_eq!(info.color_type, png::ColorType::Rgb);
        let width = info.width as usize;
        let font = fonts.font(name).unwrap();
        let (cw, ch) = font.dimensions();
        let per_row = width / cw;
        for i in 0..font.glyph_count() {
            let g = font.glyph(i).unwrap();
            for y in 0..ch {
                for x in 0..cw {
                    let src = (y * cw + x) * 4;
                    let dst = (((i / per_row) * ch + y) * width + (i % per_row) * cw + x) * 3;
                    let a = u32::from(g.rgba[src + 3]);
                    for (channel, bg) in [96, 96, 160].into_iter().enumerate() {
                        let c = (u32::from(g.rgba[src + channel]) * a + bg * (255 - a)) / 255;
                        assert_eq!(c as u8, buf[dst + channel], "{name} cell {i} ({x},{y})");
                    }
                }
            }
        }
    }
}

// Deterministic mutations, no randomness dependency or BIOS bytes in the repository.
fn mutate<F: FnMut(&[u8])>(seed: &[u8], mut inspect: F) {
    for end in [0, 1, 12, 24, 32, seed.len() / 2, seed.len().saturating_sub(1)] {
        inspect(&seed[..end.min(seed.len())]);
    }
    let mut state = 0x1234_5678u32;
    for _ in 0..96 {
        let mut d = seed.to_vec();
        for _ in 0..4 {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            let pos = state as usize % d.len();
            d[pos] ^= (state >> 24) as u8;
        }
        inspect(&d);
    }
}

#[test]
fn malformed_and_mutated_inputs_never_panic() {
    mutate(&vab_bytes(), |d| {
        if let Ok(vab) = Vab::parse(d) {
            let bench = SoundBench::ps1(&vab);
            let pcm = bench.render(0, 60, 127, 0.01);
            assert!(pcm.iter().all(|s| s.is_finite()));
        }
    });
    mutate(&bank_header(), |d| {
        if let Ok(bank) = SoundBank::new(d, adpcm()) {
            let bench = SoundBench::from_bank(bank, None);
            assert!(bench.render(0, 60, 127, 0.01).iter().all(|s| s.is_finite()));
        }
    });
    mutate(&fontm_bytes(), |d| {
        if let Ok(f) = FontmFont::new(d.to_vec()) {
            for i in 0..f.glyph_count() {
                let _ = f.glyph(i);
            }
            let _ = f.atlas(..);
        }
    });
    let context = synthetic_krom_context();
    mutate(&context, |d| {
        let f = KromFont::new(vec![0xAA; 120], Some(d));
        for code in [0, 0x8140, 0x8180, 0x889F, u16::MAX] {
            let _ = f.glyph_by_code(code);
        }
    });
    let snd = archive(&[
        ("SNDBOOTH", packed(&bank_header())),
        ("SNDBOOTB", packed(&adpcm())),
        ("SNDOSDDH", packed(&effect_header())),
        ("SNDOSDDB", packed(&adpcm())),
    ]);
    let bios = archive(&[("SNDIMAGE", snd), ("FONTM", packed(&fontm_bytes()))]);
    mutate(&bios, |d| {
        if let Ok(s) = SoundBench::from_bytes(d) {
            let _ = s.render_effect(0, 0.01);
        }
        let _ = FontBench::from_bytes(d);
    });
    for d in [&[][..], &[0xEE; 128][..], &[0; 0x80000][..]] {
        assert!(SoundBench::from_bytes(d).is_err());
        assert!(FontBench::from_bytes(d).is_err());
    }
    let mut bad_font = fontm_bytes();
    put32(&mut bad_font, 16, u32::MAX);
    assert!(FontmFont::new(bad_font).is_err());
}

#[test]
fn corrupt_present_fonts_are_errors() {
    let rom = RomDir::new(archive(&[("KROM", vec![0; 30]), ("FONTM", packed(b"not a font"))])).unwrap();
    assert!(FontBench::ps2(&rom).is_err());
    let rom = RomDir::new(archive(&[("FONTM", packed(&fontm_bytes()))])).unwrap();
    assert_eq!(FontBench::ps2(&rom).unwrap().fontm().unwrap().glyph_count(), 2);
    assert!(unpack(&packed(&[1, 2]), 0).is_ok());
}

#[test]
fn cli_exports_valid_wav_and_pam_and_rejects_unavailable_assets() {
    use std::process::Command;
    let dir = std::env::temp_dir().join("b4c-bench").join(format!("cli-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let input = dir.join("synthetic.bin");
    let wav = dir.join("note.wav");
    let pam = dir.join("font.pam");
    let missing = dir.join("missing.wav");
    let wrong_ext = dir.join("font.bad");
    let snd = archive(&[("SNDBOOTH", packed(&bank_header())), ("SNDBOOTB", packed(&adpcm()))]);
    std::fs::write(&input, archive(&[("SNDIMAGE", snd), ("FONTM", packed(&fontm_bytes()))])).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_bench"))
        .arg(&input)
        .args(["--sounds", "--render", "0", "60"])
        .arg(&wav)
        .args(["--fonts", "--atlas", "FONTM"])
        .arg(&pam)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let bytes = std::fs::read(&wav).unwrap();
    assert_eq!(&bytes[..4], b"RIFF");
    assert_eq!(&bytes[8..16], b"WAVEfmt ");
    assert_eq!(u16::from_le_bytes([bytes[22], bytes[23]]), 2);
    assert_eq!(u32::from_le_bytes(bytes[24..28].try_into().unwrap()), 48000);
    assert_eq!(u16::from_le_bytes([bytes[34], bytes[35]]), 16);
    assert_eq!(bytes.len(), 44 + 2 * 48000 * 2 * 2);
    assert!(bytes[44..].iter().any(|&b| b != 0));
    let bytes = std::fs::read(&pam).unwrap();
    let header = b"P7\nWIDTH 52\nHEIGHT 26\nDEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\nENDHDR\n";
    assert!(bytes.starts_with(header));
    assert_eq!(bytes.len(), header.len() + 52 * 26 * 4);
    let output = Command::new(env!("CARGO_BIN_EXE_bench"))
        .arg(&input)
        .args(["--render", "999", "60"])
        .arg(&missing)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!missing.exists());
    let output = Command::new(env!("CARGO_BIN_EXE_bench"))
        .arg(&input)
        .args(["--atlas", "FONTM"])
        .arg(&wrong_ext)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!wrong_ext.exists());
    for file in [input, wav, pam] {
        std::fs::remove_file(file).unwrap()
    }
    std::fs::remove_dir(dir).unwrap();
}

#[test]
fn mutations_of_real_dumps_and_real_font_containers_do_not_panic() {
    for name in ["scph5501.bin", "scph10000.bin", "SCPH-70004_BIOS_V12_PAL_200.BIN"] {
        let Ok(data) = std::fs::read(root().join("bios").join(name)) else { continue };
        for pos in [0, 0x108, 0x10000, 0x18000, 0x64000, 0x66000, data.len() - 16] {
            let mut mutated = data.clone();
            mutated[pos..pos + 16].fill(0xFF);
            if let Ok(s) = SoundBench::from_bytes(&mutated) {
                let _ = s.render(0, 60, 127, 0.01);
            }
            if let Ok(f) = FontBench::from_bytes(&mutated) {
                let _ = f.fonts()[0].atlas(0..2);
            }
        }
        if let Ok(rom) = RomDir::new(data) {
            let Ok(font) = rom.module("FONTM").and_then(|m| unpack(m, 0)) else { continue };
            mutate(&font, |bytes| {
                if let Ok(f) = FontmFont::new(bytes.to_vec()) {
                    let _ = f.glyph(0);
                    let _ = f.glyph(4417);
                    let _ = f.atlas(0..2);
                }
            });
        }
    }
}

#[test]
fn styles_group_every_program_once() {
    let Ok(d) = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../bios/scph5501.bin")) else { return };
    let shell = ps2kit::ps1::Ps1Shell::load_ps1(&ps2kit::ps1bios::Ps1Bios::new(d).unwrap()).unwrap();
    let bench = ps2kit::bench::SoundBench::ps1(&shell.bank);
    let groups = bench.by_style();
    let ids: usize = groups.iter().map(|(_, v)| v.len()).sum();
    assert_eq!(ids, bench.programs.len());
    // The shell's chime (program 3) strikes instantly and bright.
    assert!(groups.iter().any(|(s, v)| *s == ps2kit::bench::Style::BellsAndChimes && v.iter().any(|(id, _)| *id == 3)));
}

#[test]
fn voice_parameters_follow_key_and_velocity() {
    let bank = SoundBank::new(&bank_header(), adpcm()).unwrap();
    let bench = SoundBench::from_bank(bank, Some(DriverTables::computed()));
    let program = &bench.programs[0];
    let tone = &program.tones[0];
    let (step, l, r) = bench.voice_parameters(program, tone, tone.root as u8, 100);
    assert!(step.is_finite() && step > 0.0 && l.is_finite() && r.is_finite() && l + r > 0.0);
    let (up, ..) = bench.voice_parameters(program, tone, tone.root as u8 + 12, 100);
    assert!((up / step - 2.0).abs() < 0.02, "an octave up doubles the step: {step} -> {up}");
    let (_, quiet_l, quiet_r) = bench.voice_parameters(program, tone, tone.root as u8, 20);
    assert!(quiet_l + quiet_r < l + r);
    assert!(bench.any_sample_pcm(0).is_some());
    assert!(bench.any_sample_pcm(999).is_none());
}

