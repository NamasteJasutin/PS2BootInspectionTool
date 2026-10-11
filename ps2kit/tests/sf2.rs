//! SoundFont 2.04 integration tests and format verification.
//!
//! Includes a clean-room SoundFont 2 reader that walks the RIFF tree and verifies:
//! - RIFF sfbk container structure and sizes
//! - `INFO` chunk text and version tag (`ifil` 2.04, `isng` "EMU8000", `ISFT` "PSX Boot Inspection Tool")
//! - `sdta/smpl` 16-bit PCM pool, sample bounds, 46 zero-padding samples, and loop points
//! - `pdta` Hydra sub-chunks (`phdr`, `pbag`, `pmod`, `pgen`, `inst`, `ibag`, `imod`, `igen`, `shdr`)
//! - Terminal records (`EOP`, `EOI`, `EOS`) and index monotonicity
//! - Zone reference integrity (every preset zone points to a valid instrument; every instrument zone points to a valid sample)
//! - ADSR conversion accuracy compared against `ps2kit::sound::envelope`
//! - Real dumps in `bios/` (skipping silently if absent)

use ps2kit::bench::SoundBench;
use ps2kit::library::SoundLibrary;
use ps2kit::ps1::Vab;
use ps2kit::sf2::{adsr_to_vol_env, sf2_from_bench, Sf2Writer};
use ps2kit::song::{DrumVoice, Kit, KitPiece, SampleRef};
use ps2kit::sound::{envelope, DriverTables, SoundBank, SAMPLE_RATE};
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

// =======================================================================================
// Clean-room SF2 Reader for Verification
// =======================================================================================

#[derive(Debug)]
#[allow(dead_code)]
struct ParsedPreset {
    name: String,
    preset: u16,
    bank: u16,
    bag_ndx: u16,
}

#[derive(Debug)]
#[allow(dead_code)]
struct ParsedInstrument {
    name: String,
    bag_ndx: u16,
}

#[derive(Debug)]
#[allow(dead_code)]
struct ParsedSample {
    name: String,
    start: u32,
    end: u32,
    startloop: u32,
    endloop: u32,
    sample_rate: u32,
    original_pitch: u8,
    pitch_correction: i8,
    sample_type: u16,
}

#[derive(Debug)]
#[allow(dead_code)]
struct ParsedBag {
    gen_ndx: u16,
    mod_ndx: u16,
}

#[derive(Debug)]
#[allow(dead_code)]
struct ParsedGen {
    oper: u16,
    amount: u16,
}

#[allow(dead_code)]
struct ParsedSf2 {
    ifil: (u16, u16),
    isng: String,
    inam: String,
    icmt: String,
    isft: String,
    smpl: Vec<i16>,
    presets: Vec<ParsedPreset>,
    preset_bags: Vec<ParsedBag>,
    preset_gens: Vec<ParsedGen>,
    instruments: Vec<ParsedInstrument>,
    inst_bags: Vec<ParsedBag>,
    inst_gens: Vec<ParsedGen>,
    samples: Vec<ParsedSample>,
}

impl ParsedSf2 {
    fn parse(data: &[u8]) -> Self {
        assert!(data.len() >= 12, "file too small for RIFF header");
        assert_eq!(&data[0..4], b"RIFF", "missing RIFF magic");
        let riff_len = u32::from_le_bytes(data[4..8].try_into().unwrap()) as usize;
        assert_eq!(riff_len + 8, data.len(), "RIFF chunk size mismatch");
        assert_eq!(&data[8..12], b"sfbk", "missing sfbk form type");

        let mut ifil = None;
        let mut isng = None;
        let mut inam = None;
        let mut icmt = None;
        let mut isft = None;
        let mut smpl = None;
        let mut pdta_chunks: std::collections::HashMap<[u8; 4], Vec<u8>> = std::collections::HashMap::new();

        let mut pos = 12;
        while pos + 8 <= data.len() {
            let chunk_id: [u8; 4] = data[pos..pos + 4].try_into().unwrap();
            let chunk_size = u32::from_le_bytes(data[pos + 4..pos + 8].try_into().unwrap()) as usize;
            pos += 8;
            assert!(pos + chunk_size <= data.len(), "chunk extends past file");
            let chunk_data = &data[pos..pos + chunk_size];

            if &chunk_id == b"LIST" {
                assert!(chunk_data.len() >= 4, "LIST chunk too short");
                let list_type: [u8; 4] = chunk_data[..4].try_into().unwrap();
                let list_body = &chunk_data[4..];

                let mut sub_pos = 0;
                while sub_pos + 8 <= list_body.len() {
                    let sub_id: [u8; 4] = list_body[sub_pos..sub_pos + 4].try_into().unwrap();
                    let sub_size = u32::from_le_bytes(list_body[sub_pos + 4..sub_pos + 8].try_into().unwrap()) as usize;
                    sub_pos += 8;
                    assert!(sub_pos + sub_size <= list_body.len(), "subchunk extends past LIST");
                    let sub_data = &list_body[sub_pos..sub_pos + sub_size];

                    match (&list_type, &sub_id) {
                        (b"INFO", b"ifil") => {
                            assert_eq!(sub_data.len(), 4, "ifil must be 4 bytes");
                            let major = u16::from_le_bytes(sub_data[0..2].try_into().unwrap());
                            let minor = u16::from_le_bytes(sub_data[2..4].try_into().unwrap());
                            ifil = Some((major, minor));
                        }
                        (b"INFO", b"isng") => {
                            isng = Some(String::from_utf8_lossy(sub_data).trim_matches('\0').to_string());
                        }
                        (b"INFO", b"INAM") => {
                            inam = Some(String::from_utf8_lossy(sub_data).trim_matches('\0').to_string());
                        }
                        (b"INFO", b"ICMT") => {
                            icmt = Some(String::from_utf8_lossy(sub_data).trim_matches('\0').to_string());
                        }
                        (b"INFO", b"ISFT") => {
                            isft = Some(String::from_utf8_lossy(sub_data).trim_matches('\0').to_string());
                        }
                        (b"sdta", b"smpl") => {
                            assert!(sub_data.len().is_multiple_of(2), "smpl chunk must have even byte length");
                            let samples: Vec<i16> = sub_data
                                .chunks_exact(2)
                                .map(|c| i16::from_le_bytes(c.try_into().unwrap()))
                                .collect();
                            smpl = Some(samples);
                        }
                        (b"pdta", _) => {
                            pdta_chunks.insert(sub_id, sub_data.to_vec());
                        }
                        _ => {}
                    }

                    sub_pos += sub_size;
                    if !sub_size.is_multiple_of(2) {
                        sub_pos += 1; // 1-byte padding
                    }
                }
            }

            pos += chunk_size;
            if !chunk_size.is_multiple_of(2) {
                pos += 1; // 1-byte padding
            }
        }

        let ifil = ifil.expect("missing ifil in INFO chunk");
        assert_eq!(ifil, (2, 4), "expected SoundFont version 2.04");
        let isng = isng.expect("missing isng");
        assert_eq!(isng, "EMU8000");
        let inam = inam.expect("missing INAM");
        let icmt = icmt.expect("missing ICMT");
        let isft = isft.expect("missing ISFT");
        assert_eq!(isft, "PSX Boot Inspection Tool");
        let smpl = smpl.expect("missing smpl in sdta chunk");

        // Verify required pdta subchunks
        let phdr_raw = pdta_chunks.get(b"phdr").expect("missing phdr in pdta");
        let pbag_raw = pdta_chunks.get(b"pbag").expect("missing pbag in pdta");
        let pmod_raw = pdta_chunks.get(b"pmod").expect("missing pmod in pdta");
        let pgen_raw = pdta_chunks.get(b"pgen").expect("missing pgen in pdta");
        let inst_raw = pdta_chunks.get(b"inst").expect("missing inst in pdta");
        let ibag_raw = pdta_chunks.get(b"ibag").expect("missing ibag in pdta");
        let imod_raw = pdta_chunks.get(b"imod").expect("missing imod in pdta");
        let igen_raw = pdta_chunks.get(b"igen").expect("missing igen in pdta");
        let shdr_raw = pdta_chunks.get(b"shdr").expect("missing shdr in pdta");

        assert_eq!(phdr_raw.len() % 38, 0, "phdr record size not multiple of 38");
        assert_eq!(pbag_raw.len() % 4, 0, "pbag record size not multiple of 4");
        assert_eq!(pmod_raw.len() % 10, 0, "pmod record size not multiple of 10");
        assert_eq!(pgen_raw.len() % 4, 0, "pgen record size not multiple of 4");
        assert_eq!(inst_raw.len() % 22, 0, "inst record size not multiple of 22");
        assert_eq!(ibag_raw.len() % 4, 0, "ibag record size not multiple of 4");
        assert_eq!(imod_raw.len() % 10, 0, "imod record size not multiple of 10");
        assert_eq!(igen_raw.len() % 4, 0, "igen record size not multiple of 4");
        assert_eq!(shdr_raw.len() % 46, 0, "shdr record size not multiple of 46");

        // Parse pbag
        let pbag_all: Vec<ParsedBag> = pbag_raw
            .chunks_exact(4)
            .map(|c| ParsedBag {
                gen_ndx: u16::from_le_bytes(c[0..2].try_into().unwrap()),
                mod_ndx: u16::from_le_bytes(c[2..4].try_into().unwrap()),
            })
            .collect();
        assert!(!pbag_all.is_empty(), "pbag must have at least terminal record");

        // Parse pgen
        let pgen_all: Vec<ParsedGen> = pgen_raw
            .chunks_exact(4)
            .map(|c| ParsedGen {
                oper: u16::from_le_bytes(c[0..2].try_into().unwrap()),
                amount: u16::from_le_bytes(c[2..4].try_into().unwrap()),
            })
            .collect();
        assert!(!pgen_all.is_empty(), "pgen must have at least terminal record");
        assert_eq!(pgen_all.last().unwrap().oper, 0, "pgen terminal oper must be 0");
        assert_eq!(pgen_all.last().unwrap().amount, 0, "pgen terminal amount must be 0");

        // Parse ibag
        let ibag_all: Vec<ParsedBag> = ibag_raw
            .chunks_exact(4)
            .map(|c| ParsedBag {
                gen_ndx: u16::from_le_bytes(c[0..2].try_into().unwrap()),
                mod_ndx: u16::from_le_bytes(c[2..4].try_into().unwrap()),
            })
            .collect();
        assert!(!ibag_all.is_empty(), "ibag must have at least terminal record");

        // Parse igen
        let igen_all: Vec<ParsedGen> = igen_raw
            .chunks_exact(4)
            .map(|c| ParsedGen {
                oper: u16::from_le_bytes(c[0..2].try_into().unwrap()),
                amount: u16::from_le_bytes(c[2..4].try_into().unwrap()),
            })
            .collect();
        assert!(!igen_all.is_empty(), "igen must have at least terminal record");
        assert_eq!(igen_all.last().unwrap().oper, 0, "igen terminal oper must be 0");
        assert_eq!(igen_all.last().unwrap().amount, 0, "igen terminal amount must be 0");

        // Parse phdr
        let phdr_all: Vec<ParsedPreset> = phdr_raw
            .chunks_exact(38)
            .map(|c| {
                let name = String::from_utf8_lossy(&c[0..20]).trim_matches('\0').to_string();
                let preset = u16::from_le_bytes(c[20..22].try_into().unwrap());
                let bank = u16::from_le_bytes(c[22..24].try_into().unwrap());
                let bag_ndx = u16::from_le_bytes(c[24..26].try_into().unwrap());
                ParsedPreset { name, preset, bank, bag_ndx }
            })
            .collect();
        assert!(!phdr_all.is_empty(), "phdr must have at least terminal record");
        let eop = phdr_all.last().unwrap();
        assert_eq!(eop.name, "EOP", "phdr terminal name must be EOP");
        assert_eq!(eop.bag_ndx as usize, pbag_all.len() - 1, "EOP bag_ndx must equal terminal pbag index");

        // Parse inst
        let inst_all: Vec<ParsedInstrument> = inst_raw
            .chunks_exact(22)
            .map(|c| {
                let name = String::from_utf8_lossy(&c[0..20]).trim_matches('\0').to_string();
                let bag_ndx = u16::from_le_bytes(c[20..22].try_into().unwrap());
                ParsedInstrument { name, bag_ndx }
            })
            .collect();
        assert!(!inst_all.is_empty(), "inst must have at least terminal record");
        let eoi = inst_all.last().unwrap();
        assert_eq!(eoi.name, "EOI", "inst terminal name must be EOI");
        assert_eq!(eoi.bag_ndx as usize, ibag_all.len() - 1, "EOI bag_ndx must equal terminal ibag index");

        // Parse shdr
        let shdr_all: Vec<ParsedSample> = shdr_raw
            .chunks_exact(46)
            .map(|c| {
                let name = String::from_utf8_lossy(&c[0..20]).trim_matches('\0').to_string();
                let start = u32::from_le_bytes(c[20..24].try_into().unwrap());
                let end = u32::from_le_bytes(c[24..28].try_into().unwrap());
                let startloop = u32::from_le_bytes(c[28..32].try_into().unwrap());
                let endloop = u32::from_le_bytes(c[32..36].try_into().unwrap());
                let sample_rate = u32::from_le_bytes(c[36..40].try_into().unwrap());
                let original_pitch = c[40];
                let pitch_correction = c[41] as i8;
                let sample_type = u16::from_le_bytes(c[44..46].try_into().unwrap());
                ParsedSample {
                    name,
                    start,
                    end,
                    startloop,
                    endloop,
                    sample_rate,
                    original_pitch,
                    pitch_correction,
                    sample_type,
                }
            })
            .collect();
        assert!(!shdr_all.is_empty(), "shdr must have at least terminal record");
        let eos = shdr_all.last().unwrap();
        assert_eq!(eos.name, "EOS", "shdr terminal name must be EOS");

        // Validate sample bounds and padding
        let valid_samples: Vec<ParsedSample> = shdr_all.into_iter().take_while(|s| s.name != "EOS").collect();
        for (i, s) in valid_samples.iter().enumerate() {
            assert!(s.start < s.end, "sample {i} start must be less than end");
            assert!(s.end as usize <= smpl.len(), "sample {i} end out of smpl range");
            // Check 46 zero-padding samples
            if (s.end as usize + 46) <= smpl.len() {
                for (k, &v) in smpl[s.end as usize..s.end as usize + 46].iter().enumerate() {
                    assert_eq!(v, 0, "sample {i} zero padding[{k}] is non-zero: {v}");
                }
            }
            if s.startloop != s.start || s.endloop != s.end {
                assert!(s.start <= s.startloop, "sample {i} startloop < start");
                assert!(s.startloop <= s.endloop, "sample {i} startloop > endloop");
                assert!(s.endloop <= s.end, "sample {i} endloop > end");
            }
        }

        // Validate instrument zone generator linkage
        let valid_insts: Vec<ParsedInstrument> = inst_all.into_iter().take_while(|inst| inst.name != "EOI").collect();
        for (inst_idx, inst) in valid_insts.iter().enumerate() {
            let next_bag = if inst_idx + 1 < valid_insts.len() {
                valid_insts[inst_idx + 1].bag_ndx as usize
            } else {
                ibag_all.len() - 1
            };
            assert!(inst.bag_ndx as usize <= next_bag, "instrument bags not monotonic");
            for bag_idx in inst.bag_ndx as usize..next_bag {
                let next_gen = if bag_idx + 1 < ibag_all.len() {
                    ibag_all[bag_idx + 1].gen_ndx as usize
                } else {
                    igen_all.len() - 1
                };
                let zone_gens = &igen_all[ibag_all[bag_idx].gen_ndx as usize..next_gen];
                assert!(!zone_gens.is_empty(), "instrument zone has no generators");

                // Last generator MUST be sampleID (SoundFont 2.04 §8.1.3)
                let last = zone_gens.last().unwrap();
                assert_eq!(last.oper, 53, "instrument zone last generator must be sampleID (53)");
                assert!((last.amount as usize) < valid_samples.len(), "sampleID out of range");

                // First generator, if keyRange, must have low <= high
                if zone_gens[0].oper == 43 {
                    let low = zone_gens[0].amount & 0xFF;
                    let high = zone_gens[0].amount >> 8;
                    assert!(low <= high, "keyRange low > high: {low} > {high}");
                }
            }
        }

        // Validate preset zone linkage
        let valid_presets: Vec<ParsedPreset> = phdr_all.into_iter().take_while(|p| p.name != "EOP").collect();
        for (p_idx, preset) in valid_presets.iter().enumerate() {
            let next_bag = if p_idx + 1 < valid_presets.len() {
                valid_presets[p_idx + 1].bag_ndx as usize
            } else {
                pbag_all.len() - 1
            };
            assert!(preset.bag_ndx as usize <= next_bag, "preset bags not monotonic");
            for bag_idx in preset.bag_ndx as usize..next_bag {
                let next_gen = if bag_idx + 1 < pbag_all.len() {
                    pbag_all[bag_idx + 1].gen_ndx as usize
                } else {
                    pgen_all.len() - 1
                };
                let zone_gens = &pgen_all[pbag_all[bag_idx].gen_ndx as usize..next_gen];
                assert!(!zone_gens.is_empty(), "preset zone has no generators");
                let last = zone_gens.last().unwrap();
                assert_eq!(last.oper, 41, "preset zone last generator must be instrument (41)");
                assert!((last.amount as usize) < valid_insts.len(), "preset instrument out of range");
            }
        }

        Self {
            ifil,
            isng,
            inam,
            icmt,
            isft,
            smpl,
            presets: valid_presets,
            preset_bags: pbag_all,
            preset_gens: pgen_all,
            instruments: valid_insts,
            inst_bags: ibag_all,
            inst_gens: igen_all,
            samples: valid_samples,
        }
    }
}

// =======================================================================================
// Synthetic Bench Fixtures
// =======================================================================================

fn put16(d: &mut [u8], p: usize, n: u16) {
    d[p..p + 2].copy_from_slice(&n.to_le_bytes());
}

fn put32(d: &mut [u8], p: usize, n: u32) {
    d[p..p + 4].copy_from_slice(&n.to_le_bytes());
}

fn synthetic_adpcm() -> Vec<u8> {
    // 32-byte ADPCM block: block 0 loop start (flag 4), block 1 loop end + repeat (flag 3)
    let mut d = vec![0x55; 32];
    d[0] = 0;
    d[1] = 4;
    d[16] = 0;
    d[17] = 3;
    d
}

fn synthetic_vab() -> Vec<u8> {
    let tone = 32 + 128 * 16;
    let table = tone + 16 * 32;
    let body = table + 512;
    let mut d = vec![0; body + 32];
    d[..4].copy_from_slice(b"pBAV");
    put16(&mut d, 18, 1); // 1 program
    put16(&mut d, 20, 1); // 1 tone
    put16(&mut d, 22, 1); // 1 VAG sample
    d[24] = 127;
    d[25] = 64;
    d[32] = 1;
    d[33] = 127;
    d[36] = 64;
    d[tone + 2] = 127;
    d[tone + 3] = 64;
    d[tone + 4] = 60; // root 60
    d[tone + 6] = 0;
    d[tone + 7] = 127;
    put16(&mut d, tone + 16, 0x00FF);
    put16(&mut d, tone + 22, 1); // vag 1
    put16(&mut d, table + 2, 4);
    d[body..].copy_from_slice(&synthetic_adpcm());
    d
}

fn synthetic_bank_header() -> Vec<u8> {
    let mut h = vec![0; 0x60];
    put32(&mut h, 0, 0x60);
    put32(&mut h, 4, 32);
    h[0xC..0x10].copy_from_slice(b"SShd");
    put32(&mut h, 0x10, 0x20); // program table offset
    put32(&mut h, 0x14, u32::MAX);
    put32(&mut h, 0x2C, u32::MAX);
    put16(&mut h, 0x20, 0); // max program = 0
    put16(&mut h, 0x22, 0x10); // offset of program 0
    h[0x30] = 0; // split mode, 1 tone
    h[0x31] = 127; // program volume
    let t = 0x38;
    h[t] = 0; // low note
    h[t + 1] = 127; // high note
    h[t + 2] = 60; // root 60
    put16(&mut h, t + 6, 0x00FF); // adsr1
    put16(&mut h, t + 8, 0x5FF0); // adsr2
    h[t + 11] = 120; // tone volume
    h[t + 12] = 64; // centre pan
    h[t + 13] = 2; // bend range
    h
}

// =======================================================================================
// Tests
// =======================================================================================

#[test]
fn synthetic_ps2_bank_sf2_export_and_parse() {
    let bank = SoundBank::new(&synthetic_bank_header(), synthetic_adpcm()).unwrap();
    let bench = SoundBench::from_bank(bank, Some(DriverTables::computed()));

    let sf2_bytes = sf2_from_bench(&bench, None, "SyntheticPS2", "Integration Test").unwrap();
    let parsed = ParsedSf2::parse(&sf2_bytes);

    assert_eq!(parsed.inam, "SyntheticPS2");
    assert_eq!(parsed.icmt, "Integration Test");
    assert_eq!(parsed.presets.len(), 1);
    assert_eq!(parsed.presets[0].preset, 0);
    assert_eq!(parsed.presets[0].bank, 0);
    assert_eq!(parsed.instruments.len(), 1);
    assert_eq!(parsed.samples.len(), 1);

    // Verify written file in temp dir
    let temp_file = std::env::temp_dir().join("test_synth_ps2.sf2");
    std::fs::write(&temp_file, &sf2_bytes).unwrap();
    assert!(temp_file.exists());
    let re_read = std::fs::read(&temp_file).unwrap();
    let _ = ParsedSf2::parse(&re_read);
    let _ = std::fs::remove_file(temp_file);
}

#[test]
fn synthetic_ps1_vab_sf2_export_and_parse() {
    let vab = Vab::parse(&synthetic_vab()).unwrap();
    let bench = SoundBench::ps1(&vab);

    let sf2_bytes = sf2_from_bench(&bench, None, "SyntheticPS1", "VAB Test").unwrap();
    let parsed = ParsedSf2::parse(&sf2_bytes);

    assert_eq!(parsed.inam, "SyntheticPS1");
    assert_eq!(parsed.presets.len(), 1);
    assert_eq!(parsed.instruments.len(), 1);
    assert_eq!(parsed.samples.len(), 1);
}

#[test]
fn synthetic_kit_export_in_bank_128() {
    let bank = SoundBank::new(&synthetic_bank_header(), synthetic_adpcm()).unwrap();
    let bench = SoundBench::from_bank(bank, Some(DriverTables::computed()));

    let mut voice = DrumVoice::new(SampleRef::sample(bench.bank, 0));
    voice.decay_ms = 50.0;
    voice.attack_ms = 1.0;
    let piece = KitPiece::new(36, "Kick", voice, "Synthetic Kick check");
    let mut kit = Kit::default();
    kit.pieces = vec![piece];

    let sf2_bytes = Sf2Writer::new(&bench)
        .with_kit(&kit)
        .with_name("BankWithKit")
        .with_comment("Drum Kit Test")
        .write()
        .unwrap();

    let parsed = ParsedSf2::parse(&sf2_bytes);

    // 1 program preset in bank 0 + 1 kit preset in bank 128
    assert_eq!(parsed.presets.len(), 2);
    let kit_preset = parsed.presets.iter().find(|p| p.bank == 128).expect("bank 128 preset missing");
    assert_eq!(kit_preset.preset, 0);

    // 1 bench sample + 1 kit piece sample
    assert_eq!(parsed.samples.len(), 2);
    let kit_sample = parsed.samples.iter().find(|s| s.original_pitch == 36).expect("kit piece sample missing");
    assert_eq!(kit_sample.sample_rate, SAMPLE_RATE as u32);
}

#[test]
fn adsr_envelope_approximation_verification() {
    // Test known SPU register combinations from notes/sound.md §3.1
    let cases = [
        // (adsr1, adsr2, label)
        (0xAAFFu16, 0xD130u16, "AAFF: exponential attack ~0.12 s"),
        (0xB9FFu16, 0xD130u16, "B9FF: exponential attack ~1.6 s"),
        (0x3FFFu16, 0x5FF0u16, "3FFF: linear attack ~2.7 s"),
        (0x3CFFu16, 0x5FF0u16, "3CFF: linear attack ~1.6 s"),
        (0xAFE0u16, 0xCFAFu16, "AFE0: decay to lowest sustain, fast release"),
    ];

    for (a1, a2, label) in cases {
        let env_params = adsr_to_vol_env(a1, a2);

        // Compute actual simulated SPU envelope over 48000 frames (1 second) or 144000 frames (3 seconds)
        let total_frames = 144000;
        let on_frames = 96000; // key released after 2 seconds
        let spu_env = envelope(a1, a2, on_frames, total_frames);

        // 1. Verify attack rise time
        let actual_peak_frame = spu_env.iter().position(|&v| v >= 32700.0);
        if let Some(peak_frame) = actual_peak_frame {
            let actual_attack_s = peak_frame as f32 / 48000.0;
            let pred_attack_s = 2.0f32.powf(env_params.attack_tc as f32 / 1200.0);
            let diff = (actual_attack_s - pred_attack_s).abs();
            assert!(
                diff <= 0.25 || diff / actual_attack_s <= 0.25,
                "{label}: attack mismatch: actual {actual_attack_s:.3}s vs pred {pred_attack_s:.3}s"
            );
        }

        // 2. Verify sustain level
        let sl = (a1 & 0xF) as i32;
        let expected_sustain = (sl + 1) * 2048;
        let pred_sustain = 32768.0 * 10.0f32.powf(-(env_params.sustain_cb as f32) / 200.0);
        let sustain_err = (expected_sustain as f32 - pred_sustain).abs() / 32768.0;
        assert!(sustain_err < 0.05, "{label}: sustain error {sustain_err:.3}");
    }
}

#[test]
fn real_bios_dumps_oracle_tests() {
    let bios_dir = root().join("bios");
    if !bios_dir.exists() {
        return;
    }

    let files: Vec<PathBuf> = match std::fs::read_dir(&bios_dir) {
        Ok(entries) => entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("bin")))
            .collect(),
        Err(_) => return,
    };

    if files.is_empty() {
        return;
    }

    for file in &files {
        let library = SoundLibrary::scan(std::slice::from_ref(file));
        for bank in &library.banks {
            let kit = bank.kit.as_ref();
            let sf2_bytes = match sf2_from_bench(&bank.bench, kit, &bank.key.label, "Real Dump Oracle") {
                Ok(b) => b,
                Err(e) => panic!("failed to export bank {} from {}: {e}", bank.key.label, file.display()),
            };

            let parsed = ParsedSf2::parse(&sf2_bytes);

            // Check preset count = programs (+ effects) (+ 1 if kit present)
            let mut expected_presets = bank.bench.programs.len() + bank.bench.effects.len();
            if kit.is_some_and(|k| !k.pieces.is_empty()) {
                expected_presets += 1;
            }
            assert_eq!(
                parsed.presets.len(),
                expected_presets,
                "preset count mismatch for {} in {}",
                bank.key.label,
                file.display()
            );

            // Check sample count = bench samples (+ kit pieces)
            let mut expected_samples = bank.bench.samples.len();
            if let Some(k) = kit {
                expected_samples += k.pieces.len();
            }
            assert_eq!(
                parsed.samples.len(),
                expected_samples,
                "sample count mismatch for {} in {}",
                bank.key.label,
                file.display()
            );

            // File size sanity: under 30 MB
            assert!(
                sf2_bytes.len() < 30 * 1024 * 1024,
                "SF2 file size too large for {}: {} bytes",
                bank.key.label,
                sf2_bytes.len()
            );

            // Write test output to temp_dir
            let safe_bank = bank.key.label.replace(|c: char| !c.is_ascii_alphanumeric() && c != '_', "");
            let temp_out = std::env::temp_dir().join(format!("test_oracle_{safe_bank}.sf2"));
            std::fs::write(&temp_out, &sf2_bytes).unwrap();
            assert!(temp_out.exists());
            let _ = std::fs::remove_file(temp_out);
        }
    }
}
