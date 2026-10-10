//! Tests for asset benches: sound banks (PS1 VAB, PS2 OSDSND/SNDIMAGE) and fonts
//! (PS1 KROM, Gaiji block A, PS2 FONTM, FNTIMAGE) (`notes/research/version_evolution.md` §3.4,
//! `notes/research/ps1_shell_scenes.md` §2, `notes/menu_survey.md` §4).

use ps2kit::bench::{FontBench, GaijiFont, KromFont, SoundBench};
use ps2kit::ps1::Vab;
use ps2kit::ps1bios::Ps1Bios;
use ps2kit::rom::RomDir;
use ps2kit::sound::{DriverTables, SoundBank};
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

// Helper to make a 16-byte ADPCM block with non-zero samples
fn make_adpcm_block(shift: u8, filter: u8, flags: u8, nibble: u8) -> Vec<u8> {
    let mut block = vec![0u8; 16];
    block[0] = (filter << 4) | (shift & 0xF);
    block[1] = flags;
    let byte = (nibble << 4) | (nibble & 0xF);
    for b in &mut block[2..16] {
        *b = byte;
    }
    block
}

// --- Synthetic Tests -------------------------------------------------------------------

#[test]
fn synthetic_vab_render_non_silent_and_correct_length() {
    // Construct a synthetic pBAV bank:
    // Header 32 bytes + prog_tab (128 * 16) + tone_tab (1 * 16 * 32) + vag_tab (512) + body
    let n_prog = 1usize;
    let n_vag = 1usize;
    let prog_tab = 32usize;
    let tone_tab = prog_tab + 128 * 16;
    let vag_tab = tone_tab + n_prog * 16 * 32;
    let body = vag_tab + 512;
    let adpcm = make_adpcm_block(0, 0, 1, 0x5); // 16 bytes ADPCM, loop end (flags=1)
    let total_len = body + adpcm.len();

    let mut data = vec![0u8; total_len];
    data[..4].copy_from_slice(b"pBAV");
    data[18..20].copy_from_slice(&(n_prog as u16).to_le_bytes());
    data[22..24].copy_from_slice(&(n_vag as u16).to_le_bytes());
    data[24] = 0x7F; // master volume

    // Program 0 in prog_tab: 1 tone, vol 127
    data[prog_tab] = 1; // 1 tone
    data[prog_tab + 1] = 127; // program volume

    // Tone 0 in tone_tab: vol 127, pan 64, centre 60, shift 0, adsr1 0x00FF (instant attack), vag 1
    let t_off = tone_tab;
    data[t_off + 2] = 127; // tone vol
    data[t_off + 3] = 64;  // tone pan
    data[t_off + 4] = 60;  // centre note
    data[t_off + 5] = 0;   // shift
    data[t_off + 16..t_off + 18].copy_from_slice(&0x00FFu16.to_le_bytes()); // adsr1
    data[t_off + 18..t_off + 20].copy_from_slice(&0x0000u16.to_le_bytes()); // adsr2
    data[t_off + 22] = 1;  // vag 1

    // VAG size table: vag 1 size in 8-byte units (16 bytes = 2 units)
    data[vag_tab + 2..vag_tab + 4].copy_from_slice(&2u16.to_le_bytes());

    // Body: ADPCM block
    data[body..body + adpcm.len()].copy_from_slice(&adpcm);

    let vab = Vab::parse(&data).expect("Vab::parse failed");
    let bench = SoundBench::ps1(&vab);

    assert_eq!(bench.programs.len(), 1);
    assert_eq!(bench.programs[0].tones.len(), 1);
    assert_eq!(bench.samples.len(), 2); // index 0 dummy + index 1

    let duration_sec = 0.5;
    let pcm = bench.render(0, 60, 127, duration_sec);

    let expected_samples = (duration_sec * 48000.0) as usize * 2;
    assert_eq!(pcm.len(), expected_samples, "rendered audio must have exact stereo length");
    assert!(
        pcm.iter().any(|&s| s.abs() > 0.001),
        "rendered audio must be non-silent"
    );
}

#[test]
fn synthetic_soundbank_render_non_silent_and_correct_length() {
    // Construct a synthetic SShd header and body:
    let header_size = 0x60usize;
    let mut header = vec![0u8; header_size];
    header[0xC..0x10].copy_from_slice(b"SShd");

    // Base of program table: 0x20
    header[0x10..0x14].copy_from_slice(&0x20u32.to_le_bytes());
    header[0x14..0x18].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes()); // velocity table default

    // Program table at 0x20: max program index 0, offset 4
    header[0x20..0x22].copy_from_slice(&0u16.to_le_bytes()); // max = 0
    header[0x22..0x24].copy_from_slice(&4u16.to_le_bytes()); // off = 4

    // Program 0 header at 0x24: mode 0 (Split, 1 tone), vol 127
    header[0x24] = 0; // 1 tone
    header[0x25] = 127; // program vol

    // Tone 0 at 0x24 + 8 = 0x2C:
    // low 0, high 127, root 60, fine 0, sample_offset 0, adsr1 0x00FF, adsr2 0, vol 127, pan 64
    let o = 0x2C;
    header[o] = 0;
    header[o + 1] = 127;
    header[o + 2] = 60;
    header[o + 3] = 0;
    header[o + 4..o + 6].copy_from_slice(&0u16.to_le_bytes()); // sample_offset = 0 * 8 = 0
    header[o + 6..o + 8].copy_from_slice(&0x00FFu16.to_le_bytes());
    header[o + 8..o + 10].copy_from_slice(&0x0000u16.to_le_bytes());
    header[o + 11] = 127; // tone vol
    header[o + 12] = 64;  // pan
    header[o + 13] = 2;   // bend_range

    let adpcm = make_adpcm_block(0, 0, 1, 0x6);
    let bank = SoundBank::new(&header, adpcm).expect("SoundBank::new failed");
    let bench = SoundBench::from_bank(bank, Some(DriverTables::computed()));

    assert_eq!(bench.programs.len(), 1);
    assert_eq!(bench.programs[0].tones.len(), 1);
    assert_eq!(bench.samples.len(), 1);

    let duration_sec = 0.5;
    let pcm = bench.render(0, 60, 127, duration_sec);

    let expected_samples = (duration_sec * 48000.0) as usize * 2;
    assert_eq!(pcm.len(), expected_samples);
    assert!(
        pcm.iter().any(|&s| s.abs() > 0.001),
        "synthetic bank render must be non-silent"
    );
}

#[test]
fn synthetic_glyph_decoding_and_atlas() {
    // 1. Synthetic KROM (30 bytes per glyph, 15 active rows)
    let mut krom_bytes = vec![0u8; 30];
    // Row 0: bits 15 and 0 on (0x8001)
    krom_bytes[0] = 0x80;
    krom_bytes[1] = 0x01;
    // Row 14: bit 7 on (0x0080)
    krom_bytes[28] = 0x00;
    krom_bytes[29] = 0x80;

    let kf = KromFont::new(krom_bytes, None);
    let g0 = kf.glyph(0).expect("glyph 0 in synthetic KROM");
    assert_eq!(g0.width, 16);
    assert_eq!(g0.height, 16);
    assert_eq!(g0.rgba.len(), 16 * 16 * 4);

    // Pixel (0, 0) is white, pixel (15, 0) is white, pixel (1, 0) is transparent
    assert_eq!(&g0.rgba[0..4], &[255, 255, 255, 255]);
    assert_eq!(&g0.rgba[15 * 4..16 * 4], &[255, 255, 255, 255]);
    assert_eq!(&g0.rgba[4..8], &[0, 0, 0, 0]);
    // Row 15 (blank)
    assert_eq!(&g0.rgba[(15 * 16) * 4..(15 * 16 + 1) * 4], &[0, 0, 0, 0]);

    // Atlas of KROM
    let (aw, ah, argba) = kf.atlas(0..1);
    assert_eq!((aw, ah), (16, 16));
    assert_eq!(argba.len(), 16 * 16 * 4);
    assert_eq!(&argba[0..4], &[255, 255, 255, 255]);

    // 2. Synthetic Gaiji (32 bytes per glyph, 16 rows)
    let mut gaiji_bytes = vec![0u8; 32];
    gaiji_bytes[0] = 0xFF; // Row 0 all 8 bits on
    gaiji_bytes[31] = 0x01; // Row 15 bit 0 on
    let gf = GaijiFont::new(gaiji_bytes);
    assert_eq!(gf.glyph_count(), 1);
    let gg = gf.glyph(0).expect("glyph 0 in synthetic Gaiji");
    assert_eq!(gg.width, 16);
    assert_eq!(gg.height, 16);
    assert_eq!(&gg.rgba[0..4], &[255, 255, 255, 255]);

    // 3. Atlas multi-glyph test
    let multi_gaiji = vec![0xAA; 64]; // 2 glyphs
    let mgf = GaijiFont::new(multi_gaiji);
    assert_eq!(mgf.glyph_count(), 2);
    let (maw, mah, margba) = mgf.atlas(..);
    assert_eq!(maw, 32); // 2 columns * 16
    assert_eq!(mah, 16); // 1 row * 16
    assert_eq!(margba.len(), 32 * 16 * 4);
}

// --- Oracle Tests (Real Dumps in bios/) ------------------------------------------------

#[test]
fn oracle_ps2_200e_and_100j_banks_list_programs() {
    let p_200 = root().join("bios/SCPH-70004_BIOS_V12_PAL_200.BIN");
    if let Ok(d) = std::fs::read(&p_200) {
        if let Ok(rom) = RomDir::new(d) {
            let bench = SoundBench::ps2(&rom).expect("SoundBench::ps2 on 2.00 E");
            assert!(
                !bench.programs.is_empty(),
                "2.00 E sound bank must list programs"
            );
            assert!(
                !bench.samples.is_empty(),
                "2.00 E sound bank must list samples"
            );
            // Render first program, note 60
            let prog_id = bench.programs[0].id;
            let pcm = bench.render(prog_id, 60, 127, 1.0);
            assert_eq!(pcm.len(), 48000 * 2);
            assert!(
                pcm.iter().any(|&s| s.abs() > 0.001),
                "2.00 E program {prog_id} render must be non-silent"
            );
        }
    }

    let p_100 = root().join("bios/scph10000.bin");
    if let Ok(d) = std::fs::read(&p_100) {
        if let Ok(rom) = RomDir::new(d) {
            let bench = SoundBench::ps2(&rom).expect("SoundBench::ps2 on 1.00 J");
            assert!(
                !bench.programs.is_empty(),
                "1.00 J sound bank must list programs"
            );
            assert!(
                !bench.samples.is_empty(),
                "1.00 J sound bank must list samples"
            );
            let prog_id = bench.programs[0].id;
            let pcm = bench.render(prog_id, 60, 127, 1.0);
            assert_eq!(pcm.len(), 48000 * 2);
            assert!(
                pcm.iter().any(|&s| s.abs() > 0.001),
                "1.00 J program {prog_id} render must be non-silent"
            );
        }
    }
}

#[test]
fn oracle_ps1_vab_lists_named_programs() {
    // Test on real PS1 dump (e.g. scph5501.bin)
    let p_ps1 = root().join("bios/scph5501.bin");
    let Ok(data) = std::fs::read(&p_ps1) else {
        return;
    };
    let Ok(bios) = Ps1Bios::new(data) else {
        return;
    };
    let shell = bios.shell_image().expect("shell image");
    let vo = shell
        .windows(4)
        .position(|w| w == b"pBAV")
        .expect("VAB bank in PS1 shell");
    let vab = Vab::parse(&shell[vo..]).expect("Vab::parse");
    let bench = SoundBench::ps1(&vab);

    // ps1_shell_scenes.md §2 names 4 programs:
    // Program 0: drone (phase A), bass (sequence t=0) (VAG 1)
    // Program 1: sweep (VAG 3)
    // Program 2: drone (phase A) (VAG 2)
    // Program 3: ascending chime (VAG 1)
    assert!(
        bench.programs.len() >= 4,
        "PS1 VAB must contain at least 4 programs"
    );
    for id in 0..4 {
        assert!(
            bench.program(id).is_some(),
            "Program {id} must exist in PS1 VAB"
        );
    }

    // Verify program 1 is the sweep (referencing sample/VAG 3)
    let p1 = bench.program(1).unwrap();
    assert!(p1.tones.iter().any(|t| t.sample_index == 3));

    // Render program 1 (sweep) at note 36
    let pcm_sweep = bench.render(1, 36, 127, 1.0);
    assert_eq!(pcm_sweep.len(), 48000 * 2);
    assert!(
        pcm_sweep.iter().any(|&s| s.abs() > 0.001),
        "Program 1 (sweep) must render non-silent audio"
    );

    // Render program 3 (chime) at note 60
    let pcm_chime = bench.render(3, 60, 127, 1.0);
    assert_eq!(pcm_chime.len(), 48000 * 2);
    assert!(
        pcm_chime.iter().any(|&s| s.abs() > 0.001),
        "Program 3 (chime) must render non-silent audio"
    );
}

#[test]
fn oracle_gaiji_block_and_font_counts() {
    // 1. PS1 Gaiji block count in U/E image
    let p_ps1 = root().join("bios/scph5501.bin");
    if let Ok(data) = std::fs::read(&p_ps1) {
        let fb = FontBench::ps1(&data).expect("FontBench::ps1");
        let gaiji = fb.gaiji().expect("Gaiji block in scph5501");
        assert_eq!(
            gaiji.glyph_count(),
            GaijiFont::GLYPH_COUNT,
            "Gaiji block must decode to exactly 229 glyphs"
        );
        assert_eq!(gaiji.glyph_count(), 229);

        let krom = fb.krom().expect("KROM font in scph5501");
        assert_eq!(
            krom.glyph_count(),
            KromFont::GLYPH_COUNT,
            "KROM font must decode to exactly 3,489 glyphs"
        );

        // Test Shift-JIS mappings through Krom2RawAdd
        // Space 0x8140 -> index 0
        assert_eq!(krom.code_to_index(0x8140), Some(0));
        assert_eq!(krom.krom2rawadd(0x8140), Some(0xBFC6_6000));
        assert!(krom.glyph_by_code(0x8140).is_some());

        // '0' 0x824F -> index 0x93 (147)
        assert_eq!(krom.code_to_index(0x824F), Some(0x93));
        assert!(krom.glyph_by_code(0x824F).is_some());

        // 'A' 0x8260 -> index 0x9D (157)
        assert_eq!(krom.code_to_index(0x8260), Some(0x9D));
        assert!(krom.glyph_by_code(0x8260).is_some());

        // Level-1 Kanji 0x889F -> index 524
        assert_eq!(krom.code_to_index(0x889F), Some(524));
        assert_eq!(krom.krom2rawadd(0x889F), Some(0xBFC6_9D68));
        assert!(krom.glyph_by_code(0x889F).is_some());
    }

    // 2. PS2 FONTM in SCPH-70004 or scph39001
    let p_ps2 = root().join("bios/SCPH-70004_BIOS_V12_PAL_200.BIN");
    if let Ok(data) = std::fs::read(&p_ps2) {
        if let Ok(rom) = RomDir::new(data) {
            let fb = FontBench::ps2(&rom).expect("FontBench::ps2");
            if let Some(fontm) = fb.fontm() {
                assert_eq!(
                    fontm.glyph_count(),
                    4418,
                    "PS2 FONTM must contain exactly 4,418 glyphs"
                );
                assert!(fontm.glyph(0).is_some());
                assert!(fontm.glyph(100).is_some());

                let (aw, ah, rgba) = fontm.atlas(0..16);
                assert_eq!(aw, 16 * 26);
                assert_eq!(ah, 26);
                assert_eq!(rgba.len(), aw * ah * 4);
            }
        }
    }
}

// --- Robustness / Malformed Input Tests ------------------------------------------------

#[test]
#[allow(clippy::reversed_empty_ranges)]
fn malformed_input_never_panics() {
    let empty = b"";
    assert!(FontBench::ps1(empty).is_err());
    assert!(FontBench::from_bytes(empty).is_err());
    assert!(SoundBench::from_bytes(empty).is_err());

    let garbage = vec![0xEE; 128];
    assert!(FontBench::from_bytes(&garbage).is_err());
    assert!(SoundBench::from_bytes(&garbage).is_err());
    assert!(Vab::parse(&garbage).is_err());
    assert!(SoundBank::new(&garbage, Vec::new()).is_err());

    // KromFont on truncated data
    let short_kf = KromFont::new(vec![0xAA; 10], None);
    assert_eq!(short_kf.glyph_count(), 0);
    assert!(short_kf.glyph(0).is_none());
    assert!(short_kf.glyph(9999).is_none());
    assert_eq!(short_kf.atlas(..), (0, 0, Vec::new()));
    assert!(short_kf.glyph_by_code(0x8140).is_none());
    assert!(short_kf.glyph_by_code(0).is_none());

    // GaijiFont on short data
    let short_gf = GaijiFont::new(vec![0xAA; 15]);
    assert_eq!(short_gf.glyph_count(), 0);
    assert!(short_gf.glyph(0).is_none());
    assert!(short_gf.glyph(9999).is_none());
    assert_eq!(short_gf.atlas(50..10), (0, 0, Vec::new()));

    // SoundBench render with out of bounds values
    // Using a synthetic valid bench:
    let n_prog = 1usize;
    let n_vag = 1usize;
    let vag_tab = 32 + 128 * 16 + n_prog * 16 * 32;
    let body = vag_tab + 512;
    let adpcm = make_adpcm_block(0, 0, 1, 0x1);
    let mut data = vec![0u8; body + adpcm.len()];
    data[..4].copy_from_slice(b"pBAV");
    data[18..20].copy_from_slice(&(n_prog as u16).to_le_bytes());
    data[22..24].copy_from_slice(&(n_vag as u16).to_le_bytes());
    data[vag_tab + 2..vag_tab + 4].copy_from_slice(&2u16.to_le_bytes());
    data[body..body + adpcm.len()].copy_from_slice(&adpcm);

    let vab = Vab::parse(&data).unwrap();
    let bench = SoundBench::ps1(&vab);

    // Negative seconds, extreme notes, nonexistent programs
    assert!(bench.render(999, 60, 127, 1.0).iter().all(|&s| s == 0.0));
    assert!(bench.render(0, -50, 127, 1.0).len() == 48000 * 2);
    assert!(bench.render(0, 500, 127, 1.0).len() == 48000 * 2);
    assert!(bench.render(0, 60, -10, 1.0).len() == 48000 * 2);
    assert!(bench.render(0, 60, 127, -5.0).is_empty());
    assert!(bench.render(0, 60, 127, 0.0).is_empty());
}
