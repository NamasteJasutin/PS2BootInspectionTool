//! Tests that need no Sony files: every input is built here, so they run on CI.
//! `tests/oracles.rs` holds the checks against a real BIOS, which skip when none is present.

use ps2kit::disc::{handoff_steps, DiscImage, DiscKind, Ps1Verdict};
use ps2kit::history::{PlayHistory, Record, RECORD_SIZE};
use ps2kit::logo::{DiscLogo, LogoMaster};
use ps2kit::memcard::MemoryCard;
use ps2kit::rom::{unpack, RomDir};
use ps2kit::sectors::SectorReader;
use ps2kit::sim::{BootSequence, Segment, Timeline};
use ps2kit::sound::{decode_adpcm, envelope};
use ps2kit::{Region, VideoMode};
use std::path::{Path, PathBuf};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ps2kit-test-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

// --- ROMDIR -----------------------------------------------------------------------------

fn romdir(modules: &[(&str, &[u8])]) -> Vec<u8> {
    // RESET (empty), ROMDIR (the table itself), EXTINFO (empty), then the modules;
    // every module is padded to 16 bytes, like the real image.
    let entries: Vec<(&str, usize)> = [("RESET", 0), ("ROMDIR", (modules.len() + 4) * 16), ("EXTINFO", 0)]
        .into_iter()
        .chain(modules.iter().map(|(n, d)| (*n, d.len())))
        .collect();
    let mut table = Vec::new();
    for (name, size) in &entries {
        let mut e = [0u8; 16];
        e[..name.len()].copy_from_slice(name.as_bytes());
        e[12..].copy_from_slice(&(*size as u32).to_le_bytes());
        table.extend_from_slice(&e);
    }
    table.extend_from_slice(&[0u8; 16]);
    let mut image = table.clone();
    for (_, data) in modules {
        image.extend_from_slice(data);
        while image.len() % 16 != 0 { image.push(0) }
    }
    image
}

#[test]
fn romdir_lists_and_slices_modules() {
    let image = romdir(&[("ROMVER", b"0200EC20040614"), ("OSDSYS", &[7u8; 100]), ("TBIN", b"x")]);
    let rom = RomDir::new(image).unwrap();
    assert_eq!(rom.module("ROMVER").unwrap(), b"0200EC20040614");
    assert_eq!(rom.module("OSDSYS").unwrap(), &[7u8; 100][..]);
    assert_eq!(rom.module("TBIN").unwrap(), b"x");
    assert!(rom.module("PS2LOGO").is_err());
    assert!(RomDir::new(vec![0u8; 4096]).is_err(), "no RESET entry → not a BIOS");
}

// --- OSD LZ -----------------------------------------------------------------------------

/// A straightforward encoder for the OSD scheme: greedy matches, descriptor split n = 0
/// (14-bit offset, 2-bit length → matches of 3..6 bytes).
fn lz_pack(data: &[u8]) -> Vec<u8> {
    enum Tok { Lit(u8), Match(usize, usize) }
    let mut toks = Vec::new();
    let mut i = 0;
    while i < data.len() {
        let mut best = (0usize, 0usize);
        for off in 1..=i.min(0x3FFF) {
            let mut len = 0;
            while len < 6 && i + len < data.len() && data[i + len] == data[i - off + len] { len += 1 }
            if len > best.1 { best = (off, len) }
        }
        if best.1 >= 3 { toks.push(Tok::Match(best.0, best.1)); i += best.1 } else { toks.push(Tok::Lit(data[i])); i += 1 }
    }
    let mut out = (data.len() as u32).to_le_bytes().to_vec();
    for group in toks.chunks(30) {
        let mut desc = 0u32;
        let mut body = Vec::new();
        for (k, t) in group.iter().enumerate() {
            match t {
                Tok::Lit(b) => body.push(*b),
                Tok::Match(off, len) => {
                    desc |= 1 << (31 - k);
                    let h = ((len - 3) << 14 | (off - 1)) as u16;
                    body.extend_from_slice(&h.to_be_bytes());
                }
            }
        }
        out.extend_from_slice(&desc.to_be_bytes());
        out.extend(body);
    }
    out
}

#[test]
fn lz_round_trip() {
    let mut data = Vec::new();
    for i in 0..5000u32 { data.push((i * 7 % 13) as u8) }           // periodic → long matches
    data.extend_from_slice(b"The quick brown fox jumps over the lazy dog, the lazy dog, the lazy dog.");
    data.extend((0..300).map(|i| (i * 31 + 17) as u8));            // little to match
    let packed = lz_pack(&data);
    assert!(packed.len() < data.len() / 2, "the periodic part must compress");
    assert_eq!(unpack(&packed, 0).unwrap(), data);
    // Streams usually sit behind a loader stub: `start` skips it.
    let mut with_stub = vec![0xEEu8; 0xE00];
    with_stub.extend_from_slice(&packed);
    assert_eq!(unpack(&with_stub, 0xE00).unwrap(), data);
}

#[test]
fn lz_rejects_damage() {
    let packed = lz_pack(b"hello hello hello hello hello");
    assert!(unpack(&packed[..packed.len() - 3], 0).is_err(), "truncated stream");
    let mut bad = packed.clone();
    bad[4..8].copy_from_slice(&0x8000_0000u32.to_be_bytes());       // first token: a match with nothing behind it
    assert!(unpack(&bad, 0).is_err());
    let mut huge = packed.clone();
    huge[..4].copy_from_slice(&(1u32 << 30).to_le_bytes());
    assert!(unpack(&huge, 0).is_err(), "implausible size");
}

// --- play history -----------------------------------------------------------------------

fn history_record(name: &str, count: u8, mask: u8, index: u8, year: u16, month: u16, day: u16) -> [u8; RECORD_SIZE] {
    let mut r = [0u8; RECORD_SIZE];
    r[..name.len()].copy_from_slice(name.as_bytes());
    r[16] = count;
    r[17] = mask;
    r[18] = index;
    let date = (year - 2000) << 9 | month << 5 | day;
    r[20..22].copy_from_slice(&date.to_le_bytes());
    r
}

#[test]
fn history_file_and_folder_card() {
    let mut file = Vec::new();
    file.extend_from_slice(&history_record("SLES_530.64", 17, 0b11, 1, 2006, 3, 9));
    file.extend_from_slice(&history_record("SCUS_971.98", 1, 1, 0, 2001, 12, 25));
    file.resize(21 * RECORD_SIZE, 0);

    let h = PlayHistory::from_file(&file, None);
    let r = &h.records[0];
    assert_eq!((r.name.as_str(), r.count, r.mask, r.index), ("SLES_530.64", 17, 0b11, 1));
    assert_eq!((r.year(), r.month(), r.day()), (2006, 3, 9));
    assert_eq!(h.records[1].name, "SCUS_971.98");
    assert!(h.records[2].is_empty());

    // The same file inside a PCSX2 folder card, under a European system folder.
    let dir = scratch("card");
    std::fs::create_dir_all(dir.join("BEDATA-SYSTEM")).unwrap();
    std::fs::write(dir.join("BEDATA-SYSTEM/history"), &file).unwrap();
    std::fs::create_dir_all(dir.join("BESLES-53064")).unwrap();
    std::fs::write(dir.join("_pcsx2_index"), b"{}").unwrap();
    let card = MemoryCard::open(&dir).unwrap();
    let names: Vec<String> = card.list(&[]).unwrap().into_iter().map(|e| e.name).collect();
    assert!(names.contains(&"BEDATA-SYSTEM".to_string()) && !names.iter().any(|n| n.starts_with("_pcsx2_")));
    let from_card = PlayHistory::from_card(&card).unwrap();
    assert_eq!(from_card.records[0].name, "SLES_530.64");
    assert_eq!(PlayHistory::titles_with_saves(&card), vec!["SLES_530.64".to_string()]);
}

#[test]
fn synthetic_history_is_deterministic() {
    let a = PlayHistory::synthetic(&[1, 14, 24, 60], &[], 7);
    let b = PlayHistory::synthetic(&[1, 14, 24, 60], &[], 7);
    assert_eq!(a.records.len(), b.records.len());
    for (x, y) in a.records.iter().zip(&b.records) {
        assert_eq!((x.name.clone(), x.count, x.mask), (y.name.clone(), y.count, y.mask));
    }
    assert_eq!(a.records[0].count, 1);
    assert_eq!(a.records[0].mask.count_ones(), 1, "a first launch plants exactly one tower");
    assert!(a.records[3].mask.count_ones() >= a.records[1].mask.count_ones(), "more launches, no fewer towers");
}

// --- disc images ------------------------------------------------------------------------

/// A minimal PS2 game disc: logo sectors, PVD, root directory, SYSTEM.CNF and a boot ELF.
fn synthetic_disc(logo_region: &str) -> Vec<Vec<u8>> {
    let mut sectors: Vec<Vec<u8>> = vec![vec![0u8; 2048]; 24];
    // Logo (sectors 0–11): the drive descrambles each byte as rotl(b ^ key, 3) with key =
    // the first raw byte, so the first descrambled byte is always 0 and the checksum is the
    // sum of the 32-bit words. Put the whole sum into word 1.
    let key = 0x5Au8;
    let sum: u32 = if logo_region == "E" { 0x7813_4705 } else { 0x62DB_1E66 };
    let mut plain = vec![0u8; 12 * 2048];
    plain[100] = 0xFF;                                                 // a pixel that must survive the rotation
    plain[4..8].copy_from_slice(&sum.wrapping_sub(0xFF).to_le_bytes());
    for (i, b) in plain.iter().enumerate() {
        let r = b.rotate_right(3);
        sectors[i / 2048][i % 2048] = r ^ key;
    }
    assert_eq!(sectors[0][0], key);
    // PVD at 16.
    let pvd = &mut sectors[16];
    pvd[0] = 1; pvd[1..6].copy_from_slice(b"CD001"); pvd[6] = 1;
    pvd[40..40 + 9].copy_from_slice(b"TESTDISC ");
    pvd[80..84].copy_from_slice(&24u32.to_le_bytes());
    pvd[156] = 34; pvd[158..162].copy_from_slice(&18u32.to_le_bytes()); pvd[166..170].copy_from_slice(&2048u32.to_le_bytes());
    // Root directory at 18 with SYSTEM.CNF (19) and the ELF (20..).
    let cnf = b"BOOT2 = cdrom0:\\SLUS_123.45;1\r\nVER = 1.00\r\nVMODE = NTSC\r\n".to_vec();
    let mut elf = vec![0u8; 0x1000];
    elf[..4].copy_from_slice(&[0x7F, b'E', b'L', b'F']);
    elf[0x12] = 8;                                                     // EM_MIPS
    elf[0x18..0x1C].copy_from_slice(&0x0010_0008u32.to_le_bytes());   // entry
    elf[0x1C..0x20].copy_from_slice(&0x34u32.to_le_bytes());          // phoff
    elf[0x2A] = 0x20; elf[0x2C] = 1;                                   // phentsize, phnum
    let ph = 0x34;
    elf[ph..ph + 4].copy_from_slice(&1u32.to_le_bytes());             // PT_LOAD
    elf[ph + 8..ph + 12].copy_from_slice(&0x0010_0000u32.to_le_bytes());
    elf[ph + 16..ph + 20].copy_from_slice(&0x800u32.to_le_bytes());  // filesz
    elf[ph + 20..ph + 24].copy_from_slice(&0x1800u32.to_le_bytes()); // memsz
    let mut dir = Vec::new();
    for (name, lba, size) in [("SYSTEM.CNF;1", 19u32, cnf.len() as u32), ("SLUS_123.45;1", 20, elf.len() as u32)] {
        let len = 33 + name.len() + (name.len() + 1) % 2;
        let mut e = vec![0u8; len];
        e[0] = len as u8;
        e[2..6].copy_from_slice(&lba.to_le_bytes());
        e[10..14].copy_from_slice(&size.to_le_bytes());
        e[32] = name.len() as u8;
        e[33..33 + name.len()].copy_from_slice(name.as_bytes());
        dir.extend(e);
    }
    sectors[18][..dir.len()].copy_from_slice(&dir);
    sectors[19][..cnf.len()].copy_from_slice(&cnf);
    sectors[20][..2048].copy_from_slice(&elf[..2048]);
    sectors[21][..2048].copy_from_slice(&elf[2048..]);
    sectors
}

fn write_iso(dir: &Path, sectors: &[Vec<u8>]) -> PathBuf {
    let p = dir.join("disc.iso");
    std::fs::write(&p, sectors.concat()).unwrap();
    p
}

fn write_bin_cue(dir: &Path, sectors: &[Vec<u8>]) -> PathBuf {
    // MODE2/2352: sync, header, 8-byte subheader, 2048 data, 280 bytes EDC/ECC.
    let mut raw = Vec::new();
    for (i, s) in sectors.iter().enumerate() {
        raw.extend_from_slice(&[0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 0]);
        raw.extend_from_slice(&[0, 2, (i + 150) as u8, 2]);
        raw.extend_from_slice(&[0, 0, 8, 0, 0, 0, 8, 0]);
        raw.extend_from_slice(s);
        raw.extend_from_slice(&[0xAA; 280]);
    }
    std::fs::write(dir.join("disc.bin"), raw).unwrap();
    let cue = dir.join("disc.cue");
    std::fs::write(&cue, "FILE \"disc.bin\" BINARY\n  TRACK 01 MODE2/2352\n    INDEX 01 00:00:00\n").unwrap();
    cue
}

fn check_disc(path: &Path, raw: bool, region: LogoMaster) {
    let d = DiscImage::open(path).unwrap();
    assert_eq!(d.kind, DiscKind::Ps2);
    assert_eq!(d.raw_sectors, raw);
    assert_eq!(d.volume_id.trim(), "TESTDISC");
    assert_eq!(d.title_id().as_deref(), Some("SLUS_123.45"));
    assert_eq!(d.system_cnf.get("BOOT2").map(String::as_str), Some("cdrom0:\\SLUS_123.45;1"));
    assert_eq!(d.system_cnf.get("VMODE").map(String::as_str), Some("NTSC"));
    assert!(!d.is_dvd());
    assert_eq!(d.disc_state_code(), 0x6C);
    let elf = d.boot_elf.as_ref().expect("boot ELF");
    assert!(elf.is_mips);
    assert_eq!((elf.lba, elf.size, elf.entry), (20, 0x1000, 0x0010_0008));
    assert_eq!(elf.segments.len(), 1);
    assert_eq!((elf.segments[0].vaddr, elf.segments[0].file_size, elf.segments[0].mem_size), (0x0010_0000, 0x800, 0x1800));
    assert_eq!(d.logo_region, Some(region));
    let logo = DiscLogo::read(path).unwrap();
    assert_eq!(logo.region, Some(region));
    assert_eq!(logo.pixels[100], 0xFF);
    assert_eq!(logo.bitmap(VideoMode::Ntsc).width, 384);
}

#[test]
fn disc_image_plain_iso() {
    let dir = scratch("iso");
    let p = write_iso(&dir, &synthetic_disc("E"));
    let mut r = SectorReader::open(&p).unwrap();
    assert!(!r.is_raw());
    assert_eq!((r.sector_size(), r.data_offset()), (2048, 0));
    assert_eq!(r.read(16, 1).unwrap()[1..6], *b"CD001");
    check_disc(&p, false, LogoMaster::Europe);
}

#[test]
fn disc_image_bin_cue() {
    let dir = scratch("bincue");
    let cue = write_bin_cue(&dir, &synthetic_disc("J"));
    let mut r = SectorReader::open(&cue).unwrap();
    assert!(r.is_raw());
    assert_eq!((r.sector_size(), r.data_offset()), (2352, 24));
    assert_eq!(r.read(16, 1).unwrap()[1..6], *b"CD001");
    check_disc(&cue, true, LogoMaster::JapanAmerica);
    check_disc(&dir.join("disc.bin"), true, LogoMaster::JapanAmerica);
}

#[test]
fn disc_image_rejects_non_iso() {
    let dir = scratch("noiso");
    let p = dir.join("junk.iso");
    std::fs::write(&p, vec![0u8; 40 * 2048]).unwrap();
    assert!(DiscImage::open(&p).is_err());
}

// --- simulation ------------------------------------------------------------------------

#[test]
fn timelines_have_the_console_lengths() {
    // The logo program animates fields 17–42 (NTSC) / 14–35 (PAL) and holds for 120 fields.
    assert!(Timeline::logo(VideoMode::Ntsc).end_frame() > Timeline::logo(VideoMode::Pal).end_frame());
    for video in [VideoMode::Ntsc, VideoMode::Pal] {
        let t = Timeline::boot(0, video);
        let (first, last) = (t.camera(0.0), t.camera(t.end_frame() as f32));
        assert!(first.z < 20.0 && last.z > 100.0, "the camera flies from z 16 to z 104 ({video:?}: {} → {})", first.z, last.z);
        let mut prev = first.z;
        for f in 1..=t.end_frame() {
            let z = t.camera(f as f32).z;
            assert!(z >= prev - 1e-4, "camera never moves backwards");
            prev = z;
        }
        assert!(t.frame_passing(60.0) > 0 && t.frame_passing(60.0) < t.end_frame());
    }
    // PAL integrates with a larger step, so it reaches the same place in fewer frames.
    assert!(Timeline::boot(0, VideoMode::Pal).end_frame() < Timeline::boot(0, VideoMode::Ntsc).end_frame());
}

#[test]
fn boot_sequence_segments_are_contiguous() {
    let seq = BootSequence::new(VideoMode::Ntsc, 3.0, 2.0, 1.2, 6.0);
    let mut next = 0;
    for s in seq.spans() {
        assert_eq!(s.start, next);
        next += s.length;
    }
    assert_eq!(seq.total_frames(), next);
    assert_eq!(seq.spans()[0].segment, Segment::PowerOn);
    assert_eq!(seq.spans()[0].length, 180);
    assert_eq!(seq.logo_start(), seq.spans()[3].start);
    let (span, local) = seq.span_at(seq.logo_start() + 5);
    assert_eq!((span.segment, local), (Segment::Logo, 5));
}

// --- sound -----------------------------------------------------------------------------

#[test]
fn adpcm_silence_and_envelope_shape() {
    // Sixteen-byte SPU ADPCM blocks: shift/filter byte, flags byte, 14 nibble bytes.
    // Flags 0x07 = loop end + loop; 0x01 = end without loop.
    let mut body = vec![0u8; 64];
    body[1] = 0x06;                 // block 0: loop start
    body[1 + 48] = 0x03;            // block 3: end
    let (pcm, loop_start) = decode_adpcm(&body, 0);
    assert_eq!(pcm.len(), 4 * 28);
    assert!(pcm.iter().all(|s| *s == 0.0));
    assert_eq!(loop_start, Some(0));

    // Attack-decay-sustain-release: rises while the key is held, then falls to silence.
    let env = envelope(0x00FF, 0x1FC0, 4000, 12000);
    assert_eq!(env.len(), 12000);
    assert!(env[0] <= env[100] && env[100] <= env[3000], "attack rises");
    assert!(env[11999] < env[3000], "release falls");
    assert!(env.iter().all(|v| (0.0..=32767.0).contains(v)), "SPU level scale");
}

// --- PlayStation 1 discs -----------------------------------------------------------------

/// A minimal PlayStation disc: licence sector 4, logo data in 5–15, SYSTEM.CNF with `BOOT`,
/// and a PS-X EXE. `with_cnf = false` leaves SYSTEM.CNF out, so the shell falls back to PSX.EXE.
fn synthetic_ps1_disc(region_text: &str, with_cnf: bool) -> Vec<Vec<u8>> {
    let mut sectors: Vec<Vec<u8>> = vec![vec![0u8; 2048]; 24];
    let lic = format!("          Licensed  by          Sony Computer Entertainment {region_text}");
    sectors[4][0..lic.len()].copy_from_slice(lic.as_bytes());
    for (i, s) in sectors[5..16].iter_mut().enumerate() { s[0] = 0x10 + i as u8; s[100] = 0x7F; }
    let pvd = &mut sectors[16];
    pvd[0] = 1; pvd[1..6].copy_from_slice(b"CD001"); pvd[6] = 1;
    pvd[40..40 + 8].copy_from_slice(b"PSXDISC ");
    pvd[80..84].copy_from_slice(&24u32.to_le_bytes());
    pvd[156] = 34; pvd[158..162].copy_from_slice(&18u32.to_le_bytes()); pvd[166..170].copy_from_slice(&2048u32.to_le_bytes());
    let cnf = b"BOOT = cdrom:\\SLUS_005.94;1\r\nTCB = 4\r\nEVENT = 10\r\nSTACK = 801fff00\r\nVER = 1.1\r\n".to_vec();
    let exe_name = if with_cnf { "SLUS_005.94;1" } else { "PSX.EXE;1" };
    let mut exe = vec![0u8; 2048 + 0x800];
    exe[..8].copy_from_slice(b"PS-X EXE");
    exe[0x10..0x14].copy_from_slice(&0x8001_0000u32.to_le_bytes());
    exe[0x14..0x18].copy_from_slice(&0x8001_8000u32.to_le_bytes());
    exe[0x18..0x1C].copy_from_slice(&0x8001_0000u32.to_le_bytes());
    exe[0x1C..0x20].copy_from_slice(&0x800u32.to_le_bytes());
    exe[0x30..0x34].copy_from_slice(&0x801F_FF00u32.to_le_bytes());
    let marker = b"Sony Computer Entertainment Inc. for North America area";
    exe[0x4C..0x4C + marker.len()].copy_from_slice(marker);
    let mut dir = Vec::new();
    let mut files: Vec<(&str, u32, u32)> = vec![(exe_name, 20, exe.len() as u32)];
    if with_cnf { files.insert(0, ("SYSTEM.CNF;1", 19, cnf.len() as u32)) }
    for (name, lba, size) in files {
        let len = 33 + name.len() + (name.len() + 1) % 2;
        let mut e = vec![0u8; len];
        e[0] = len as u8;
        e[2..6].copy_from_slice(&lba.to_le_bytes());
        e[10..14].copy_from_slice(&size.to_le_bytes());
        e[32] = name.len() as u8;
        e[33..33 + name.len()].copy_from_slice(name.as_bytes());
        dir.extend(e);
    }
    sectors[18][..dir.len()].copy_from_slice(&dir);
    if with_cnf { sectors[19][..cnf.len()].copy_from_slice(&cnf) }
    sectors[20][..2048].copy_from_slice(&exe[..2048]);
    sectors[21][..exe.len() - 2048].copy_from_slice(&exe[2048..]);
    sectors
}

#[test]
fn ps1_disc_with_system_cnf() {
    let dir = scratch("ps1");
    let p = write_iso(&dir, &synthetic_ps1_disc("Amer  ica ", true));
    let d = DiscImage::open(&p).unwrap();
    assert_eq!(d.kind, DiscKind::Ps1);
    assert!(d.boot_elf.is_none() && d.logo_region.is_none());
    assert_eq!(d.title_id().as_deref(), Some("SLUS_005.94"));
    assert_eq!(d.version.as_deref(), Some("1.1"));
    assert_eq!((d.disc_type_register(), d.disc_state_code()), (0x10, 0x6A));
    let l = d.ps1_licence.as_ref().expect("licence sector");
    assert_eq!(l.region, Some(Region::America));
    assert!(l.text.starts_with("Licensed by Sony Computer Entertainment"));
    assert!(l.logo_sectors_present);
    let e = d.ps1_exe.as_ref().expect("PS-X EXE");
    assert_eq!((e.lba, e.initial_pc, e.initial_gp, e.text_addr, e.text_size, e.stack), (20, 0x8001_0000, 0x8001_8000, 0x8001_0000, 0x800, 0x801F_FF00));
    assert!(e.marker.contains("North America"));
    let h = PlayHistory::synthetic(&[], &[], 1);
    let steps = handoff_steps(Some(&d), &h, VideoMode::Ntsc, Some(Region::America), None);
    let text: Vec<String> = steps.iter().map(|s| format!("{} {s}", s.who())).collect();
    assert!(text[0].contains("an A console's PS1 shell checks neither"), "{}", text[0]);
    // The rule from the shell: E wants the 70/67-character European line, J the 64-character "Inc." line.
    let lic = d.ps1_licence.as_ref().unwrap();
    assert_eq!(Ps1Verdict::judge(Some(Region::America), Some(lic), None), Some(Ps1Verdict::NotChecked));
    assert_eq!(Ps1Verdict::judge(Some(Region::Europe), Some(lic), None), Some(Ps1Verdict::TextMismatch), "an American line on a European console");
    assert_eq!(Ps1Verdict::judge(Some(Region::Europe), Some(lic), Some(&lic.logo_data)), Some(Ps1Verdict::TextMismatch));
    assert_eq!(Ps1Verdict::judge(Some(Region::Europe), Some(lic), Some(&[0u8; 0x3278])), Some(Ps1Verdict::LogoMismatch), "the logo is compared before the text");
    assert!(text.iter().any(|t| t.contains("LoadExecPS2(\"rom0:PS1DRV\", argc 2, argv {\"SLUS_005.94\", \"1.1\"}")), "{text:?}");
    assert!(text.iter().any(|t| t.contains("rom0:LOGO")));
    assert!(text.iter().any(|t| t.contains("new record for SLUS_005.94")));
    assert!(matches!(&steps[0], ps2kit::disc::HandoffStep::Ps1RegionCheck { verdict: Some(Ps1Verdict::NotChecked), .. }));
    let steps_e = handoff_steps(Some(&d), &h, VideoMode::Pal, Some(Region::Europe), None);
    assert!(steps_e[0].to_string().contains("re-read the disc for ever"), "{}", steps_e[0]);
}

#[test]
fn ps1_disc_without_system_cnf_and_with_cdda() {
    let dir = scratch("ps1cdda");
    let cue = write_bin_cue(&dir, &synthetic_ps1_disc("Euro pe   ", false));
    std::fs::write(&cue, "FILE \"disc.bin\" BINARY\n  TRACK 01 MODE2/2352\n    INDEX 01 00:00:00\n  TRACK 02 AUDIO\n    INDEX 00 01:00:00\n").unwrap();
    let d = DiscImage::open(&cue).unwrap();
    assert_eq!(d.kind, DiscKind::Ps1);
    assert!(d.has_cdda);
    assert_eq!((d.disc_type_register(), d.disc_state_code()), (0x11, 0x6B));
    assert_eq!(d.title_id().as_deref(), Some("???"), "PSX.EXE is recorded as ???");
    assert_eq!(d.ps1_licence.as_ref().and_then(|l| l.region), Some(Region::Europe));
    assert_eq!(d.ps1_exe.as_ref().map(|e| e.file_name.as_str()), Some("PSX.EXE;1"));
    let lic = d.ps1_licence.as_ref().unwrap();
    assert_eq!(lic.line.len(), 70, "the synthetic European line is the 70-character variant");
    assert_eq!(Ps1Verdict::judge(Some(Region::Europe), Some(lic), None), Some(Ps1Verdict::Accepted));
    assert_eq!(Ps1Verdict::judge(Some(Region::Japan), Some(lic), None), Some(Ps1Verdict::TextMismatch));
}

#[test]
fn iso_without_boot_is_unknown_media() {
    let dir = scratch("datadisc");
    let mut sectors = synthetic_ps1_disc("Inc.", true);
    sectors[4] = vec![0u8; 2048];                       // no licence
    sectors[19] = vec![0u8; 2048];                      // SYSTEM.CNF present but empty
    let p = write_iso(&dir, &sectors);
    let d = DiscImage::open(&p).unwrap();
    assert_eq!(d.kind, DiscKind::Unknown, "an empty SYSTEM.CNF falls back to PSX.EXE, which this disc does not have");
    let mut sectors = synthetic_ps1_disc("Inc.", false);
    sectors[4] = vec![0u8; 2048];
    sectors[18] = vec![0u8; 2048];                      // empty root: nothing to boot
    let p = write_iso(&dir, &sectors);
    let d = DiscImage::open(&p).unwrap();
    assert_eq!(d.kind, DiscKind::Unknown);
    assert_eq!(d.disc_state_code(), 0x69);
    let steps = handoff_steps(Some(&d), &PlayHistory::synthetic(&[], &[], 1), VideoMode::Ntsc, Some(Region::Europe), None);
    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0], ps2kit::disc::HandoffStep::UnknownMedia);
}

// --- malformed input ---------------------------------------------------------------------

/// Inputs that used to panic (index out of range, overflow, endless loop) must now fail
/// cleanly or degrade to an empty result.
#[test]
fn malformed_input_does_not_panic() {
    use ps2kit::bios::decode_texture;
    use ps2kit::locate::ProgramImage;
    use ps2kit::logo::LogoAssets;
    use ps2kit::sound::{DriverTables, SoundBank, SoundSequence};

    // 4-bit texture with an odd pixel count: the last nibble sits in a half byte.
    assert!(decode_texture(&[0u8; 1], 1, 1, 0x14, Some(&[0u8; 64])).is_ok());
    assert!(decode_texture(&[0u8; 1], 1, 1, 0x14, Some(&[0u8; 8])).is_err(), "short palette");
    assert!(decode_texture(&[], 1 << 20, 1 << 20, 0, None).is_err(), "implausible size");

    // A driver module whose unison entry sits before where the table would start.
    let mut osdsnd = vec![0u8; 64];
    for (i, v) in [0xFF0u16, 0x1000, 0x1010].iter().enumerate() { osdsnd[i * 2..i * 2 + 2].copy_from_slice(&v.to_le_bytes()) }
    assert!(DriverTables::from_driver(&osdsnd).is_none());

    // A sequence with a zero tempo (the driver would spin for ever) and one with a runaway
    // delta time are rejected rather than hung on.
    let mut seq = vec![0u8; 0x120];
    seq[0xC..0x10].copy_from_slice(b"SSsq");
    seq[0x110..0x113].copy_from_slice(&[0x90, 60, 100]);
    seq[0x113] = 0x10;
    seq[0x114..0x117].copy_from_slice(&[0xFF, 0x2F, 0]);
    assert!(SoundSequence::new(&seq).is_err(), "resolution and bpm are 0");
    seq[2..4].copy_from_slice(&480u16.to_le_bytes());
    seq[4..6].copy_from_slice(&120u16.to_le_bytes());
    assert!(SoundSequence::new(&seq).is_ok());
    seq[0x113..0x11A].copy_from_slice(&[0xFF; 7]);
    assert!(SoundSequence::new(&seq).is_err(), "delta time longer than 4 bytes");

    // A bank header that is too short; a PS2LOGO image with none of the tables.
    assert!(SoundBank::new(&[0u8; 16], Vec::new()).is_err());
    assert!(RomDir::new(vec![0u8; 16]).is_err());
    assert!(LogoAssets::from_image(&ProgramImage::new(vec![0u8; 0x100], 0x100000)).is_err());
}

// --- hand-off text --------------------------------------------------------------------------

/// The sentences `handoff_steps` produced in 0.1 for every branch (region verdicts, history
/// update rules, PS1 licence outcomes, unknown media, no disc), captured before the steps
/// became structured. `who()` and `Display` must reproduce them exactly.
const HANDOFF_TEXT_0_1: &str = r#"## ps2 E ntsc h13
Region|disc SLUS_123.45 is USA, console ROM is Europe: a real console would reject it (state 0x74, warning scene). The tool does not enforce region locks.
CDVD (disc thread 0x20F478)|disc type register → state 0x6C (PlayStation 2 CD); Ps2DiscVerifyAndGetId reads the disc key twice → title ID SLUS_123.45
OSDSYS OpeningDecideNext (0x2165A0)|latched state → ctx[0x14] = 1 (launch request: PS2 CD)
OSDSYS Launch (0x203970 → 0x202AB0)|DiscThreadEnable(0); read cdrom0:\SYSTEM.CNF;1 → BOOT2 = cdrom0:\SLUS_123.45;1; file name must match the first 10 characters of the disc ID
OSDSYS HistoryUpdate (0x201E98) + save (0x204AC0)|SLUS_123.45: count 13 → 14, a new random tower bit is added; written to mc0:/B?DATA-SYSTEM/history
OSDSYS|shutdown of subsystems (0x2021E8), then LoadExecPS2("rom0:PS2LOGO", argc 1, argv {"cdrom0:\SLUS_123.45;1"})
KERNEL KLoadExec|HardwareRestart, EELOAD re-copied to 0x82000, loads rom0:PS2LOGO (stub + LZ stream → 0x100000, main 0x102040)
PS2LOGO|rom0:ROMVER → region; load rom0:OSDSND and the embedded one-sample bank; GS 640×512 interlaced FIELD mode
PS2LOGO LoadImage (0x101540)|sceCdDecSet(1,1,5), sceCdRead(lba 0, 12 sectors) → 24,576 bytes; logo data matches the E master; checksum accepted
PS2LOGO|chime (cmd 0x5200 ×5), animation fields 17–42, then the last frame is held for 120 fields
PS2LOGO → KERNEL|SPU quit; LoadExecPS2("cdrom0:\SLUS_123.45;1", …) → KLoadExec → EELOAD LoadElfAll: SLUS_123.45;1 at LBA 20, 4096 bytes
EELOAD / kernel|ELF (MIPS R5900): 1 PT_LOAD segment(s), 6144 bytes in memory, entry 0x00100008 — ExecPS2 jumps there with argv[0] = the boot path
  segment 0|vaddr 0x00100000, 2048 bytes from file, 6144 bytes in memory (bss zeroed)
— stop —|This is where the game takes over the console. The app ends the sequence here.
## ps2 J pal h5
Region|disc SLUS_123.45 is USA, console ROM is Japan: a real console would reject it (state 0x74, warning scene). The tool does not enforce region locks.
CDVD (disc thread 0x20F478)|disc type register → state 0x6C (PlayStation 2 CD); Ps2DiscVerifyAndGetId reads the disc key twice → title ID SLUS_123.45
OSDSYS OpeningDecideNext (0x2165A0)|latched state → ctx[0x14] = 1 (launch request: PS2 CD)
OSDSYS Launch (0x203970 → 0x202AB0)|DiscThreadEnable(0); read cdrom0:\SYSTEM.CNF;1 → BOOT2 = cdrom0:\SLUS_123.45;1; file name must match the first 10 characters of the disc ID
OSDSYS HistoryUpdate (0x201E98) + save (0x204AC0)|SLUS_123.45: count 5 → 6; written to mc0:/B?DATA-SYSTEM/history
OSDSYS|shutdown of subsystems (0x2021E8), then LoadExecPS2("rom0:PS2LOGO", argc 1, argv {"cdrom0:\SLUS_123.45;1"})
KERNEL KLoadExec|HardwareRestart, EELOAD re-copied to 0x82000, loads rom0:PS2LOGO (stub + LZ stream → 0x100000, main 0x102040)
PS2LOGO|rom0:ROMVER → region; load rom0:OSDSND and the embedded one-sample bank; GS 640×512 interlaced FIELD mode
PS2LOGO LoadImage (0x101540)|sceCdDecSet(1,1,5), sceCdRead(lba 0, 12 sectors) → 24,576 bytes; logo data matches the E master; checksum mismatch: the animation is skipped and PS2LOGO exits straight to the game
PS2LOGO|chime (cmd 0x5200 ×5), animation fields 14–35, then the last frame is held for 120 fields
PS2LOGO → KERNEL|SPU quit; LoadExecPS2("cdrom0:\SLUS_123.45;1", …) → KLoadExec → EELOAD LoadElfAll: SLUS_123.45;1 at LBA 20, 4096 bytes
EELOAD / kernel|ELF (MIPS R5900): 1 PT_LOAD segment(s), 6144 bytes in memory, entry 0x00100008 — ExecPS2 jumps there with argv[0] = the boot path
  segment 0|vaddr 0x00100000, 2048 bytes from file, 6144 bytes in memory (bss zeroed)
— stop —|This is where the game takes over the console. The app ends the sequence here.
## ps2 none
Region|disc region USA, console region unknown (not checked)
CDVD (disc thread 0x20F478)|disc type register → state 0x6C (PlayStation 2 CD); Ps2DiscVerifyAndGetId reads the disc key twice → title ID SLUS_123.45
OSDSYS OpeningDecideNext (0x2165A0)|latched state → ctx[0x14] = 1 (launch request: PS2 CD)
OSDSYS Launch (0x203970 → 0x202AB0)|DiscThreadEnable(0); read cdrom0:\SYSTEM.CNF;1 → BOOT2 = cdrom0:\SLUS_123.45;1; file name must match the first 10 characters of the disc ID
OSDSYS HistoryUpdate (0x201E98) + save (0x204AC0)|new record for SLUS_123.45: count 1, mask 0x01 → one tower stub appears on the next boot; written to mc0:/B?DATA-SYSTEM/history
OSDSYS|shutdown of subsystems (0x2021E8), then LoadExecPS2("rom0:PS2LOGO", argc 1, argv {"cdrom0:\SLUS_123.45;1"})
KERNEL KLoadExec|HardwareRestart, EELOAD re-copied to 0x82000, loads rom0:PS2LOGO (stub + LZ stream → 0x100000, main 0x102040)
PS2LOGO|rom0:ROMVER → region; load rom0:OSDSND and the embedded one-sample bank; GS 640×512 interlaced FIELD mode
PS2LOGO LoadImage (0x101540)|sceCdDecSet(1,1,5), sceCdRead(lba 0, 12 sectors) → 24,576 bytes; logo data matches the E master; checksum console region unknown
PS2LOGO|chime (cmd 0x5200 ×5), animation fields 17–42, then the last frame is held for 120 fields
PS2LOGO → KERNEL|SPU quit; LoadExecPS2("cdrom0:\SLUS_123.45;1", …) → KLoadExec → EELOAD LoadElfAll: SLUS_123.45;1 at LBA 20, 4096 bytes
EELOAD / kernel|ELF (MIPS R5900): 1 PT_LOAD segment(s), 6144 bytes in memory, entry 0x00100008 — ExecPS2 jumps there with argv[0] = the boot path
  segment 0|vaddr 0x00100000, 2048 bytes from file, 6144 bytes in memory (bss zeroed)
— stop —|This is where the game takes over the console. The app ends the sequence here.
## ps2 A
Region|disc SLUS_123.45 is USA, console ROM is USA: a real console accepts it
CDVD (disc thread 0x20F478)|disc type register → state 0x6C (PlayStation 2 CD); Ps2DiscVerifyAndGetId reads the disc key twice → title ID SLUS_123.45
OSDSYS OpeningDecideNext (0x2165A0)|latched state → ctx[0x14] = 1 (launch request: PS2 CD)
OSDSYS Launch (0x203970 → 0x202AB0)|DiscThreadEnable(0); read cdrom0:\SYSTEM.CNF;1 → BOOT2 = cdrom0:\SLUS_123.45;1; file name must match the first 10 characters of the disc ID
OSDSYS HistoryUpdate (0x201E98) + save (0x204AC0)|new record for SLUS_123.45: count 1, mask 0x01 → one tower stub appears on the next boot; written to mc0:/B?DATA-SYSTEM/history
OSDSYS|shutdown of subsystems (0x2021E8), then LoadExecPS2("rom0:PS2LOGO", argc 1, argv {"cdrom0:\SLUS_123.45;1"})
KERNEL KLoadExec|HardwareRestart, EELOAD re-copied to 0x82000, loads rom0:PS2LOGO (stub + LZ stream → 0x100000, main 0x102040)
PS2LOGO|rom0:ROMVER → region; load rom0:OSDSND and the embedded one-sample bank; GS 640×512 interlaced FIELD mode
PS2LOGO LoadImage (0x101540)|sceCdDecSet(1,1,5), sceCdRead(lba 0, 12 sectors) → 24,576 bytes; logo data matches the E master; checksum this console never checks it
PS2LOGO|chime (cmd 0x5200 ×5), animation fields 17–42, then the last frame is held for 120 fields
PS2LOGO → KERNEL|SPU quit; LoadExecPS2("cdrom0:\SLUS_123.45;1", …) → KLoadExec → EELOAD LoadElfAll: SLUS_123.45;1 at LBA 20, 4096 bytes
EELOAD / kernel|ELF (MIPS R5900): 1 PT_LOAD segment(s), 6144 bytes in memory, entry 0x00100008 — ExecPS2 jumps there with argv[0] = the boot path
  segment 0|vaddr 0x00100000, 2048 bytes from file, 6144 bytes in memory (bss zeroed)
— stop —|This is where the game takes over the console. The app ends the sequence here.
## ps2 H h63
Region|disc SLUS_123.45 is USA, console ROM is Japan: a real console would reject it (state 0x74, warning scene). The tool does not enforce region locks.
CDVD (disc thread 0x20F478)|disc type register → state 0x6C (PlayStation 2 CD); Ps2DiscVerifyAndGetId reads the disc key twice → title ID SLUS_123.45
OSDSYS OpeningDecideNext (0x2165A0)|latched state → ctx[0x14] = 1 (launch request: PS2 CD)
OSDSYS Launch (0x203970 → 0x202AB0)|DiscThreadEnable(0); read cdrom0:\SYSTEM.CNF;1 → BOOT2 = cdrom0:\SLUS_123.45;1; file name must match the first 10 characters of the disc ID
OSDSYS HistoryUpdate (0x201E98) + save (0x204AC0)|SLUS_123.45: count 63 → 64, a new random tower bit is added; written to mc0:/B?DATA-SYSTEM/history
OSDSYS|shutdown of subsystems (0x2021E8), then LoadExecPS2("rom0:PS2LOGO", argc 1, argv {"cdrom0:\SLUS_123.45;1"})
KERNEL KLoadExec|HardwareRestart, EELOAD re-copied to 0x82000, loads rom0:PS2LOGO (stub + LZ stream → 0x100000, main 0x102040)
PS2LOGO|rom0:ROMVER → region; load rom0:OSDSND and the embedded one-sample bank; GS 640×512 interlaced FIELD mode
PS2LOGO LoadImage (0x101540)|sceCdDecSet(1,1,5), sceCdRead(lba 0, 12 sectors) → 24,576 bytes; logo data matches the E master; checksum mismatch: the animation is skipped and PS2LOGO exits straight to the game
PS2LOGO|chime (cmd 0x5200 ×5), animation fields 17–42, then the last frame is held for 120 fields
PS2LOGO → KERNEL|SPU quit; LoadExecPS2("cdrom0:\SLUS_123.45;1", …) → KLoadExec → EELOAD LoadElfAll: SLUS_123.45;1 at LBA 20, 4096 bytes
EELOAD / kernel|ELF (MIPS R5900): 1 PT_LOAD segment(s), 6144 bytes in memory, entry 0x00100008 — ExecPS2 jumps there with argv[0] = the boot path
  segment 0|vaddr 0x00100000, 2048 bytes from file, 6144 bytes in memory (bss zeroed)
— stop —|This is where the game takes over the console. The app ends the sequence here.
## ps2 C
Region|disc SLUS_123.45 is USA, console ROM is China: a real console would reject it (state 0x74, warning scene). The tool does not enforce region locks.
CDVD (disc thread 0x20F478)|disc type register → state 0x6C (PlayStation 2 CD); Ps2DiscVerifyAndGetId reads the disc key twice → title ID SLUS_123.45
OSDSYS OpeningDecideNext (0x2165A0)|latched state → ctx[0x14] = 1 (launch request: PS2 CD)
OSDSYS Launch (0x203970 → 0x202AB0)|DiscThreadEnable(0); read cdrom0:\SYSTEM.CNF;1 → BOOT2 = cdrom0:\SLUS_123.45;1; file name must match the first 10 characters of the disc ID
OSDSYS HistoryUpdate (0x201E98) + save (0x204AC0)|new record for SLUS_123.45: count 1, mask 0x01 → one tower stub appears on the next boot; written to mc0:/B?DATA-SYSTEM/history
OSDSYS|shutdown of subsystems (0x2021E8), then LoadExecPS2("rom0:PS2LOGO", argc 1, argv {"cdrom0:\SLUS_123.45;1"})
KERNEL KLoadExec|HardwareRestart, EELOAD re-copied to 0x82000, loads rom0:PS2LOGO (stub + LZ stream → 0x100000, main 0x102040)
PS2LOGO|rom0:ROMVER → region; load rom0:OSDSND and the embedded one-sample bank; GS 640×512 interlaced FIELD mode
PS2LOGO LoadImage (0x101540)|sceCdDecSet(1,1,5), sceCdRead(lba 0, 12 sectors) → 24,576 bytes; logo data matches the E master; checksum this console never checks it
PS2LOGO|chime (cmd 0x5200 ×5), animation fields 17–42, then the last frame is held for 120 fields
PS2LOGO → KERNEL|SPU quit; LoadExecPS2("cdrom0:\SLUS_123.45;1", …) → KLoadExec → EELOAD LoadElfAll: SLUS_123.45;1 at LBA 20, 4096 bytes
EELOAD / kernel|ELF (MIPS R5900): 1 PT_LOAD segment(s), 6144 bytes in memory, entry 0x00100008 — ExecPS2 jumps there with argv[0] = the boot path
  segment 0|vaddr 0x00100000, 2048 bytes from file, 6144 bytes in memory (bss zeroed)
— stop —|This is where the game takes over the console. The app ends the sequence here.
## ps2 X
Region|disc SLUS_123.45 is USA, console ROM is unknown: a real console would reject it (state 0x74, warning scene). The tool does not enforce region locks.
CDVD (disc thread 0x20F478)|disc type register → state 0x6C (PlayStation 2 CD); Ps2DiscVerifyAndGetId reads the disc key twice → title ID SLUS_123.45
OSDSYS OpeningDecideNext (0x2165A0)|latched state → ctx[0x14] = 1 (launch request: PS2 CD)
OSDSYS Launch (0x203970 → 0x202AB0)|DiscThreadEnable(0); read cdrom0:\SYSTEM.CNF;1 → BOOT2 = cdrom0:\SLUS_123.45;1; file name must match the first 10 characters of the disc ID
OSDSYS HistoryUpdate (0x201E98) + save (0x204AC0)|new record for SLUS_123.45: count 1, mask 0x01 → one tower stub appears on the next boot; written to mc0:/B?DATA-SYSTEM/history
OSDSYS|shutdown of subsystems (0x2021E8), then LoadExecPS2("rom0:PS2LOGO", argc 1, argv {"cdrom0:\SLUS_123.45;1"})
KERNEL KLoadExec|HardwareRestart, EELOAD re-copied to 0x82000, loads rom0:PS2LOGO (stub + LZ stream → 0x100000, main 0x102040)
PS2LOGO|rom0:ROMVER → region; load rom0:OSDSND and the embedded one-sample bank; GS 640×512 interlaced FIELD mode
PS2LOGO LoadImage (0x101540)|sceCdDecSet(1,1,5), sceCdRead(lba 0, 12 sectors) → 24,576 bytes; logo data matches the E master; checksum mismatch: the animation is skipped and PS2LOGO exits straight to the game
PS2LOGO|chime (cmd 0x5200 ×5), animation fields 17–42, then the last frame is held for 120 fields
PS2LOGO → KERNEL|SPU quit; LoadExecPS2("cdrom0:\SLUS_123.45;1", …) → KLoadExec → EELOAD LoadElfAll: SLUS_123.45;1 at LBA 20, 4096 bytes
EELOAD / kernel|ELF (MIPS R5900): 1 PT_LOAD segment(s), 6144 bytes in memory, entry 0x00100008 — ExecPS2 jumps there with argv[0] = the boot path
  segment 0|vaddr 0x00100000, 2048 bytes from file, 6144 bytes in memory (bss zeroed)
— stop —|This is where the game takes over the console. The app ends the sequence here.
## ps1 A
Region|console ROM is America: an A console's PS1 shell checks neither the licence line nor the logo — the disc is shown as it is
CDVD (disc thread 0x20F478)|disc type register 0x10 → state 0x6A (PlayStation CD)
OSDSYS OpeningDecideNext (0x2165A0)|latched state → ctx[0x14] = 2 (launch request: PlayStation disc)
OSDSYS LaunchPs1Disc (0x202D50 → Ps1GetBootId 0x203390)|SYSTEM.CNF BOOT = cdrom:\SLUS_005.94;1; VER = 1.1; title ID = the file name between the last \ or : and ; → SLUS_005.94
OSDSYS HistoryUpdate (0x201E98) + save (0x204AC0)|new record for SLUS_005.94: count 1, mask 0x01 → one tower stub appears on the next boot; written to mc0:/B?DATA-SYSTEM/history
OSDSYS|shutdown of subsystems (0x2021E8), then LoadExecPS2("rom0:PS1DRV", argc 2, argv {"SLUS_005.94", "1.1"})
KERNEL KLoadExec|HardwareRestart, EELOAD re-copied to 0x82000, loads rom0:PS1DRV
PS1DRV / TBIN|the EE side of PlayStation compatibility: the IOP is rebooted in PS1 mode and TBIN loads rom0:LOGO — the PS1 BIOS shell (stub + LZ stream → 0x30000, i.e. the PS1's 0x80030000)
PS1 shell: system area|sectors 4–15 read; licence line 70 characters: "Licensed by Sony Computer Entertainment Amer ica"; logo TMD in sectors 5–11 present
PS1 shell: licence screen|no Sony Computer Entertainment screen in this shell; the logo fades in over 31 fields through the GTE depth cue under two drone notes, then the wordmark ramps up over 30 fields with the licence line, the TM mark and the drive's SCE letters while the ascending chime plays, 22 more fields until the note table ends: 83 fields ≈ 1.38 s, then the screen stays while the game loads
PS1 shell → PS-X EXE|SLUS_005.94;1 at LBA 20, 4096 bytes: text 2048 bytes to 0x80010000, initial PC 0x80010000, GP 0x80018000, SP 0x801FFF00; header marker "Sony Computer Entertainment Inc. for North America area"
— stop —|This is where the PlayStation game takes over. The app ends the sequence here.
## ps1 E pal h23
Region|licence sector says America, console ROM is Europe: the PS1 shell would re-read the disc for ever on a black screen (the tool does not enforce region locks)
CDVD (disc thread 0x20F478)|disc type register 0x10 → state 0x6A (PlayStation CD)
OSDSYS OpeningDecideNext (0x2165A0)|latched state → ctx[0x14] = 2 (launch request: PlayStation disc)
OSDSYS LaunchPs1Disc (0x202D50 → Ps1GetBootId 0x203390)|SYSTEM.CNF BOOT = cdrom:\SLUS_005.94;1; VER = 1.1; title ID = the file name between the last \ or : and ; → SLUS_005.94
OSDSYS HistoryUpdate (0x201E98) + save (0x204AC0)|SLUS_005.94: count 23 → 24, a new random tower bit is added; written to mc0:/B?DATA-SYSTEM/history
OSDSYS|shutdown of subsystems (0x2021E8), then LoadExecPS2("rom0:PS1DRV", argc 2, argv {"SLUS_005.94", "1.1"})
KERNEL KLoadExec|HardwareRestart, EELOAD re-copied to 0x82000, loads rom0:PS1DRV
PS1DRV / TBIN|the EE side of PlayStation compatibility: the IOP is rebooted in PS1 mode and TBIN loads rom0:LOGO — the PS1 BIOS shell (stub + LZ stream → 0x30000, i.e. the PS1's 0x80030000)
PS1 shell: system area|sectors 4–15 read; licence line 70 characters: "Licensed by Sony Computer Entertainment Amer ica"; logo TMD in sectors 5–11 present
PS1 shell: licence screen|no Sony Computer Entertainment screen in this shell; the logo fades in over 31 fields through the GTE depth cue under two drone notes, then the wordmark ramps up over 30 fields with the licence line, the TM mark and the drive's SCE letters while the ascending chime plays, 13 more fields until the note table ends: 74 fields ≈ 1.48 s, then the screen stays while the game loads
PS1 shell → PS-X EXE|SLUS_005.94;1 at LBA 20, 4096 bytes: text 2048 bytes to 0x80010000, initial PC 0x80010000, GP 0x80018000, SP 0x801FFF00; header marker "Sony Computer Entertainment Inc. for North America area"
— stop —|This is where the PlayStation game takes over. The app ends the sequence here.
## ps1 J logo ok
Region|licence sector says America, console ROM is Japan: the PS1 shell would re-read the disc for ever on a black screen (the tool does not enforce region locks)
CDVD (disc thread 0x20F478)|disc type register 0x10 → state 0x6A (PlayStation CD)
OSDSYS OpeningDecideNext (0x2165A0)|latched state → ctx[0x14] = 2 (launch request: PlayStation disc)
OSDSYS LaunchPs1Disc (0x202D50 → Ps1GetBootId 0x203390)|SYSTEM.CNF BOOT = cdrom:\SLUS_005.94;1; VER = 1.1; title ID = the file name between the last \ or : and ; → SLUS_005.94
OSDSYS HistoryUpdate (0x201E98) + save (0x204AC0)|new record for SLUS_005.94: count 1, mask 0x01 → one tower stub appears on the next boot; written to mc0:/B?DATA-SYSTEM/history
OSDSYS|shutdown of subsystems (0x2021E8), then LoadExecPS2("rom0:PS1DRV", argc 2, argv {"SLUS_005.94", "1.1"})
KERNEL KLoadExec|HardwareRestart, EELOAD re-copied to 0x82000, loads rom0:PS1DRV
PS1DRV / TBIN|the EE side of PlayStation compatibility: the IOP is rebooted in PS1 mode and TBIN loads rom0:LOGO — the PS1 BIOS shell (stub + LZ stream → 0x30000, i.e. the PS1's 0x80030000)
PS1 shell: system area|sectors 4–15 read; licence line 70 characters: "Licensed by Sony Computer Entertainment Amer ica"; logo TMD in sectors 5–11 identical to the shell's own copy
PS1 shell: licence screen|no Sony Computer Entertainment screen in this shell; the logo fades in over 31 fields through the GTE depth cue under two drone notes, then the wordmark ramps up over 30 fields with the licence line, the TM mark and the drive's SCE letters while the ascending chime plays, 22 more fields until the note table ends: 83 fields ≈ 1.38 s, then the screen stays while the game loads
PS1 shell → PS-X EXE|SLUS_005.94;1 at LBA 20, 4096 bytes: text 2048 bytes to 0x80010000, initial PC 0x80010000, GP 0x80018000, SP 0x801FFF00; header marker "Sony Computer Entertainment Inc. for North America area"
— stop —|This is where the PlayStation game takes over. The app ends the sequence here.
## ps1 E logo bad
Region|console ROM is Europe: the logo in sectors 5–11 differs from the shell's copy — SystemErrorBootOrDiskFailure, black screen (the tool draws the disc's logo anyway)
CDVD (disc thread 0x20F478)|disc type register 0x10 → state 0x6A (PlayStation CD)
OSDSYS OpeningDecideNext (0x2165A0)|latched state → ctx[0x14] = 2 (launch request: PlayStation disc)
OSDSYS LaunchPs1Disc (0x202D50 → Ps1GetBootId 0x203390)|SYSTEM.CNF BOOT = cdrom:\SLUS_005.94;1; VER = 1.1; title ID = the file name between the last \ or : and ; → SLUS_005.94
OSDSYS HistoryUpdate (0x201E98) + save (0x204AC0)|new record for SLUS_005.94: count 1, mask 0x01 → one tower stub appears on the next boot; written to mc0:/B?DATA-SYSTEM/history
OSDSYS|shutdown of subsystems (0x2021E8), then LoadExecPS2("rom0:PS1DRV", argc 2, argv {"SLUS_005.94", "1.1"})
KERNEL KLoadExec|HardwareRestart, EELOAD re-copied to 0x82000, loads rom0:PS1DRV
PS1DRV / TBIN|the EE side of PlayStation compatibility: the IOP is rebooted in PS1 mode and TBIN loads rom0:LOGO — the PS1 BIOS shell (stub + LZ stream → 0x30000, i.e. the PS1's 0x80030000)
PS1 shell: system area|sectors 4–15 read; licence line 70 characters: "Licensed by Sony Computer Entertainment Amer ica"; logo TMD in sectors 5–11 differs from the shell's copy
PS1 shell: licence screen|no Sony Computer Entertainment screen in this shell; the logo fades in over 31 fields through the GTE depth cue under two drone notes, then the wordmark ramps up over 30 fields with the licence line, the TM mark and the drive's SCE letters while the ascending chime plays, 22 more fields until the note table ends: 83 fields ≈ 1.38 s, then the screen stays while the game loads
PS1 shell → PS-X EXE|SLUS_005.94;1 at LBA 20, 4096 bytes: text 2048 bytes to 0x80010000, initial PC 0x80010000, GP 0x80018000, SP 0x801FFF00; header marker "Sony Computer Entertainment Inc. for North America area"
— stop —|This is where the PlayStation game takes over. The app ends the sequence here.
## ps1 none
Region|licence region America, console region unknown (not checked)
CDVD (disc thread 0x20F478)|disc type register 0x10 → state 0x6A (PlayStation CD)
OSDSYS OpeningDecideNext (0x2165A0)|latched state → ctx[0x14] = 2 (launch request: PlayStation disc)
OSDSYS LaunchPs1Disc (0x202D50 → Ps1GetBootId 0x203390)|SYSTEM.CNF BOOT = cdrom:\SLUS_005.94;1; VER = 1.1; title ID = the file name between the last \ or : and ; → SLUS_005.94
OSDSYS HistoryUpdate (0x201E98) + save (0x204AC0)|new record for SLUS_005.94: count 1, mask 0x01 → one tower stub appears on the next boot; written to mc0:/B?DATA-SYSTEM/history
OSDSYS|shutdown of subsystems (0x2021E8), then LoadExecPS2("rom0:PS1DRV", argc 2, argv {"SLUS_005.94", "1.1"})
KERNEL KLoadExec|HardwareRestart, EELOAD re-copied to 0x82000, loads rom0:PS1DRV
PS1DRV / TBIN|the EE side of PlayStation compatibility: the IOP is rebooted in PS1 mode and TBIN loads rom0:LOGO — the PS1 BIOS shell (stub + LZ stream → 0x30000, i.e. the PS1's 0x80030000)
PS1 shell: system area|sectors 4–15 read; licence line 70 characters: "Licensed by Sony Computer Entertainment Amer ica"; logo TMD in sectors 5–11 present
PS1 shell: licence screen|no Sony Computer Entertainment screen in this shell; the logo fades in over 31 fields through the GTE depth cue under two drone notes, then the wordmark ramps up over 30 fields with the licence line, the TM mark and the drive's SCE letters while the ascending chime plays, 22 more fields until the note table ends: 83 fields ≈ 1.38 s, then the screen stays while the game loads
PS1 shell → PS-X EXE|SLUS_005.94;1 at LBA 20, 4096 bytes: text 2048 bytes to 0x80010000, initial PC 0x80010000, GP 0x80018000, SP 0x801FFF00; header marker "Sony Computer Entertainment Inc. for North America area"
— stop —|This is where the PlayStation game takes over. The app ends the sequence here.
## ps1 H
Region|licence sector says America, console ROM is Japan: the PS1 shell would re-read the disc for ever on a black screen (the tool does not enforce region locks)
CDVD (disc thread 0x20F478)|disc type register 0x10 → state 0x6A (PlayStation CD)
OSDSYS OpeningDecideNext (0x2165A0)|latched state → ctx[0x14] = 2 (launch request: PlayStation disc)
OSDSYS LaunchPs1Disc (0x202D50 → Ps1GetBootId 0x203390)|SYSTEM.CNF BOOT = cdrom:\SLUS_005.94;1; VER = 1.1; title ID = the file name between the last \ or : and ; → SLUS_005.94
OSDSYS HistoryUpdate (0x201E98) + save (0x204AC0)|new record for SLUS_005.94: count 1, mask 0x01 → one tower stub appears on the next boot; written to mc0:/B?DATA-SYSTEM/history
OSDSYS|shutdown of subsystems (0x2021E8), then LoadExecPS2("rom0:PS1DRV", argc 2, argv {"SLUS_005.94", "1.1"})
KERNEL KLoadExec|HardwareRestart, EELOAD re-copied to 0x82000, loads rom0:PS1DRV
PS1DRV / TBIN|the EE side of PlayStation compatibility: the IOP is rebooted in PS1 mode and TBIN loads rom0:LOGO — the PS1 BIOS shell (stub + LZ stream → 0x30000, i.e. the PS1's 0x80030000)
PS1 shell: system area|sectors 4–15 read; licence line 70 characters: "Licensed by Sony Computer Entertainment Amer ica"; logo TMD in sectors 5–11 present
PS1 shell: licence screen|no Sony Computer Entertainment screen in this shell; the logo fades in over 31 fields through the GTE depth cue under two drone notes, then the wordmark ramps up over 30 fields with the licence line, the TM mark and the drive's SCE letters while the ascending chime plays, 22 more fields until the note table ends: 83 fields ≈ 1.38 s, then the screen stays while the game loads
PS1 shell → PS-X EXE|SLUS_005.94;1 at LBA 20, 4096 bytes: text 2048 bytes to 0x80010000, initial PC 0x80010000, GP 0x80018000, SP 0x801FFF00; header marker "Sony Computer Entertainment Inc. for North America area"
— stop —|This is where the PlayStation game takes over. The app ends the sequence here.
## ps1 C h4
Region|licence sector says America, console ROM is China: the PS1 shell would re-read the disc for ever on a black screen (the tool does not enforce region locks)
CDVD (disc thread 0x20F478)|disc type register 0x10 → state 0x6A (PlayStation CD)
OSDSYS OpeningDecideNext (0x2165A0)|latched state → ctx[0x14] = 2 (launch request: PlayStation disc)
OSDSYS LaunchPs1Disc (0x202D50 → Ps1GetBootId 0x203390)|SYSTEM.CNF BOOT = cdrom:\SLUS_005.94;1; VER = 1.1; title ID = the file name between the last \ or : and ; → SLUS_005.94
OSDSYS HistoryUpdate (0x201E98) + save (0x204AC0)|SLUS_005.94: count 4 → 5; written to mc0:/B?DATA-SYSTEM/history
OSDSYS|shutdown of subsystems (0x2021E8), then LoadExecPS2("rom0:PS1DRV", argc 2, argv {"SLUS_005.94", "1.1"})
KERNEL KLoadExec|HardwareRestart, EELOAD re-copied to 0x82000, loads rom0:PS1DRV
PS1DRV / TBIN|the EE side of PlayStation compatibility: the IOP is rebooted in PS1 mode and TBIN loads rom0:LOGO — the PS1 BIOS shell (stub + LZ stream → 0x30000, i.e. the PS1's 0x80030000)
PS1 shell: system area|sectors 4–15 read; licence line 70 characters: "Licensed by Sony Computer Entertainment Amer ica"; logo TMD in sectors 5–11 present
PS1 shell: licence screen|no Sony Computer Entertainment screen in this shell; the logo fades in over 31 fields through the GTE depth cue under two drone notes, then the wordmark ramps up over 30 fields with the licence line, the TM mark and the drive's SCE letters while the ascending chime plays, 22 more fields until the note table ends: 83 fields ≈ 1.38 s, then the screen stays while the game loads
PS1 shell → PS-X EXE|SLUS_005.94;1 at LBA 20, 4096 bytes: text 2048 bytes to 0x80010000, initial PC 0x80010000, GP 0x80018000, SP 0x801FFF00; header marker "Sony Computer Entertainment Inc. for North America area"
— stop —|This is where the PlayStation game takes over. The app ends the sequence here.
## ps1b E
Region|licence sector says Europe, console ROM is Europe: the PS1 shell accepts the line and the logo
CDVD (disc thread 0x20F478)|disc type register 0x11 → state 0x6B (PlayStation CD with CD-DA)
OSDSYS OpeningDecideNext (0x2165A0)|latched state → ctx[0x14] = 2 (launch request: PlayStation disc)
OSDSYS LaunchPs1Disc (0x202D50 → Ps1GetBootId 0x203390)|no SYSTEM.CNF on the disc: the shell boots cdrom:\PSX.EXE;1 and OSDSYS records the title as "???"
OSDSYS HistoryUpdate (0x201E98) + save (0x204AC0)|new record for ???: count 1, mask 0x01 → one tower stub appears on the next boot; written to mc0:/B?DATA-SYSTEM/history
OSDSYS|shutdown of subsystems (0x2021E8), then LoadExecPS2("rom0:PS1DRV", argc 2, argv {"???", ""})
KERNEL KLoadExec|HardwareRestart, EELOAD re-copied to 0x82000, loads rom0:PS1DRV
PS1DRV / TBIN|the EE side of PlayStation compatibility: the IOP is rebooted in PS1 mode and TBIN loads rom0:LOGO — the PS1 BIOS shell (stub + LZ stream → 0x30000, i.e. the PS1's 0x80030000)
PS1 shell: system area|sectors 4–15 read; licence line 70 characters: "Licensed by Sony Computer Entertainment Euro pe"; logo TMD in sectors 5–11 present
PS1 shell: licence screen|no Sony Computer Entertainment screen in this shell; the logo fades in over 31 fields through the GTE depth cue under two drone notes, then the wordmark ramps up over 30 fields with the licence line, the TM mark and the drive's SCE letters while the ascending chime plays, 13 more fields until the note table ends: 74 fields ≈ 1.48 s, then the screen stays while the game loads
PS1 shell → PS-X EXE|PSX.EXE;1 at LBA 20, 4096 bytes: text 2048 bytes to 0x80010000, initial PC 0x80010000, GP 0x80018000, SP 0x801FFF00; header marker "Sony Computer Entertainment Inc. for North America area"
— stop —|This is where the PlayStation game takes over. The app ends the sequence here.
## ps1b A logo ok
Region|console ROM is America: an A console's PS1 shell checks neither the licence line nor the logo — the disc is shown as it is
CDVD (disc thread 0x20F478)|disc type register 0x11 → state 0x6B (PlayStation CD with CD-DA)
OSDSYS OpeningDecideNext (0x2165A0)|latched state → ctx[0x14] = 2 (launch request: PlayStation disc)
OSDSYS LaunchPs1Disc (0x202D50 → Ps1GetBootId 0x203390)|no SYSTEM.CNF on the disc: the shell boots cdrom:\PSX.EXE;1 and OSDSYS records the title as "???"
OSDSYS HistoryUpdate (0x201E98) + save (0x204AC0)|new record for ???: count 1, mask 0x01 → one tower stub appears on the next boot; written to mc0:/B?DATA-SYSTEM/history
OSDSYS|shutdown of subsystems (0x2021E8), then LoadExecPS2("rom0:PS1DRV", argc 2, argv {"???", ""})
KERNEL KLoadExec|HardwareRestart, EELOAD re-copied to 0x82000, loads rom0:PS1DRV
PS1DRV / TBIN|the EE side of PlayStation compatibility: the IOP is rebooted in PS1 mode and TBIN loads rom0:LOGO — the PS1 BIOS shell (stub + LZ stream → 0x30000, i.e. the PS1's 0x80030000)
PS1 shell: system area|sectors 4–15 read; licence line 70 characters: "Licensed by Sony Computer Entertainment Euro pe"; logo TMD in sectors 5–11 identical to the shell's own copy
PS1 shell: licence screen|no Sony Computer Entertainment screen in this shell; the logo fades in over 31 fields through the GTE depth cue under two drone notes, then the wordmark ramps up over 30 fields with the licence line, the TM mark and the drive's SCE letters while the ascending chime plays, 22 more fields until the note table ends: 83 fields ≈ 1.38 s, then the screen stays while the game loads
PS1 shell → PS-X EXE|PSX.EXE;1 at LBA 20, 4096 bytes: text 2048 bytes to 0x80010000, initial PC 0x80010000, GP 0x80018000, SP 0x801FFF00; header marker "Sony Computer Entertainment Inc. for North America area"
— stop —|This is where the PlayStation game takes over. The app ends the sequence here.
## ps1b J
Region|licence sector says Europe, console ROM is Japan: the PS1 shell would re-read the disc for ever on a black screen (the tool does not enforce region locks)
CDVD (disc thread 0x20F478)|disc type register 0x11 → state 0x6B (PlayStation CD with CD-DA)
OSDSYS OpeningDecideNext (0x2165A0)|latched state → ctx[0x14] = 2 (launch request: PlayStation disc)
OSDSYS LaunchPs1Disc (0x202D50 → Ps1GetBootId 0x203390)|no SYSTEM.CNF on the disc: the shell boots cdrom:\PSX.EXE;1 and OSDSYS records the title as "???"
OSDSYS HistoryUpdate (0x201E98) + save (0x204AC0)|???: count 2 → 3; written to mc0:/B?DATA-SYSTEM/history
OSDSYS|shutdown of subsystems (0x2021E8), then LoadExecPS2("rom0:PS1DRV", argc 2, argv {"???", ""})
KERNEL KLoadExec|HardwareRestart, EELOAD re-copied to 0x82000, loads rom0:PS1DRV
PS1DRV / TBIN|the EE side of PlayStation compatibility: the IOP is rebooted in PS1 mode and TBIN loads rom0:LOGO — the PS1 BIOS shell (stub + LZ stream → 0x30000, i.e. the PS1's 0x80030000)
PS1 shell: system area|sectors 4–15 read; licence line 70 characters: "Licensed by Sony Computer Entertainment Euro pe"; logo TMD in sectors 5–11 present
PS1 shell: licence screen|no Sony Computer Entertainment screen in this shell; the logo fades in over 31 fields through the GTE depth cue under two drone notes, then the wordmark ramps up over 30 fields with the licence line, the TM mark and the drive's SCE letters while the ascending chime plays, 22 more fields until the note table ends: 83 fields ≈ 1.38 s, then the screen stays while the game loads
PS1 shell → PS-X EXE|PSX.EXE;1 at LBA 20, 4096 bytes: text 2048 bytes to 0x80010000, initial PC 0x80010000, GP 0x80018000, SP 0x801FFF00; header marker "Sony Computer Entertainment Inc. for North America area"
— stop —|This is where the PlayStation game takes over. The app ends the sequence here.
## unk E
CDVD (disc thread 0x20F478)|the disc has no SYSTEM.CNF and no PlayStation licence sector: the drive reports it as unknown media (register 0x05, state 0x69); OpeningDecideNext leaves it to the browser, which shows it as a data disc
## none
OSDSYS|No disc: OpeningDecideNext → ctx[0x5E8] = 2, the clock/main-menu module is woken (not re-created here).
"#;

#[test]
fn handoff_sentences_are_unchanged_since_0_1() {
    use ps2kit::disc::HandoffStep;
    let ps2 = DiscImage::open(write_iso(&scratch("handoff-ps2"), &synthetic_disc("E"))).unwrap();
    let ps1 = DiscImage::open(write_iso(&scratch("handoff-ps1"), &synthetic_ps1_disc("Amer  ica ", true))).unwrap();
    let cue = write_bin_cue(&scratch("handoff-ps1b"), &synthetic_ps1_disc("Euro pe   ", false));
    std::fs::write(&cue, "FILE \"disc.bin\" BINARY\n  TRACK 01 MODE2/2352\n    INDEX 01 00:00:00\n  TRACK 02 AUDIO\n    INDEX 00 01:00:00\n").unwrap();
    let ps1b = DiscImage::open(&cue).unwrap();
    let mut sectors = synthetic_ps1_disc("Inc.", false);
    sectors[4] = vec![0u8; 2048];
    sectors[18] = vec![0u8; 2048];
    let unk = DiscImage::open(write_iso(&scratch("handoff-unk"), &sectors)).unwrap();
    let hist = |name: &str, count: u8| PlayHistory::new(vec![Record { name: name.into(), count, mask: 0x3, index: 1, date: 0 }], None);
    let empty = PlayHistory::synthetic(&[], &[], 1);
    let (h13, h5, h63, h23, h4, h2) = (hist("SLUS_123.45", 13), hist("SLUS_123.45", 5), hist("SLUS_123.45", 63), hist("SLUS_005.94", 23), hist("SLUS_005.94", 4), hist("???", 2));
    let lic = ps1.ps1_licence.as_ref().unwrap().logo_data.clone();
    let other = vec![0u8; 0x3278];
    let (j, a, e, c) = (Some(Region::Japan), Some(Region::America), Some(Region::Europe), Some(Region::China));
    // `X` was an unrecognised ROMVER letter in 0.1 (a console "of unknown region"); it now
    // decodes to `None`, so its sentences are the "not checked" ones.
    let x = Region::from_romver_letter('X');
    assert_eq!(x, None);
    type Case<'a> = (&'a str, Option<&'a DiscImage>, &'a PlayHistory, VideoMode, Option<Region>, Option<&'a [u8]>);
    let cases: Vec<Case> = vec![
        ("ps2 E ntsc h13", Some(&ps2), &h13, VideoMode::Ntsc, e, None),
        ("ps2 J pal h5", Some(&ps2), &h5, VideoMode::Pal, j, None),
        ("ps2 none", Some(&ps2), &empty, VideoMode::Ntsc, None, None),
        ("ps2 A", Some(&ps2), &empty, VideoMode::Ntsc, a, None),
        ("ps2 H h63", Some(&ps2), &h63, VideoMode::Ntsc, Region::from_romver_letter('H'), None),
        ("ps2 C", Some(&ps2), &empty, VideoMode::Ntsc, c, None),
        ("ps1 A", Some(&ps1), &empty, VideoMode::Ntsc, a, None),
        ("ps1 E pal h23", Some(&ps1), &h23, VideoMode::Pal, e, None),
        ("ps1 J logo ok", Some(&ps1), &empty, VideoMode::Ntsc, j, Some(&lic)),
        ("ps1 E logo bad", Some(&ps1), &empty, VideoMode::Ntsc, e, Some(&other)),
        ("ps1 none", Some(&ps1), &empty, VideoMode::Ntsc, None, None),
        ("ps1 H", Some(&ps1), &empty, VideoMode::Ntsc, Region::from_romver_letter('H'), None),
        ("ps1 C h4", Some(&ps1), &h4, VideoMode::Ntsc, c, None),
        ("ps1b E", Some(&ps1b), &empty, VideoMode::Pal, e, None),
        ("ps1b A logo ok", Some(&ps1b), &empty, VideoMode::Ntsc, a, Some(&lic)),
        ("ps1b J", Some(&ps1b), &h2, VideoMode::Ntsc, j, None),
        ("unk E", Some(&unk), &empty, VideoMode::Ntsc, e, None),
        ("none", None, &empty, VideoMode::Ntsc, e, None),
    ];
    let mut got: Vec<String> = Vec::new();
    for (name, d, h, v, r, l) in cases {
        got.push(format!("## {name}"));
        for s in handoff_steps(d, h, v, r, l) { got.push(format!("{}|{s}", s.who())) }
    }
    let expected: Vec<&str> = HANDOFF_TEXT_0_1.lines().filter(|l| !l.is_empty()).collect();
    // The `ps2 X` case of the capture is the one whose text changed (see above); skip it.
    let expected: Vec<&str> = {
        let mut out = Vec::new();
        let mut skip = false;
        for l in expected {
            if l.starts_with("## ") { skip = l == "## ps2 X" }
            if !skip { out.push(l) }
        }
        out
    };
    for (i, (g, e)) in got.iter().zip(&expected).enumerate() {
        assert_eq!(g, e, "line {i}");
    }
    assert_eq!(got.len(), expected.len());

    // The facts behind the sentences are now data.
    let steps = handoff_steps(Some(&ps2), &h13, VideoMode::Ntsc, e, None);
    assert!(matches!(&steps[0], HandoffStep::RegionCheck { disc: Some(Region::America), console: Some(Region::Europe), verdict: ps2kit::disc::RegionVerdict::Rejected, .. }));
    assert!(matches!(&steps[4], HandoffStep::HistoryUpdate { before: Some(Record { count: 13, .. }), after: Record { count: 14, .. }, new_tower: true, .. }));
    assert!(matches!(&steps[8], HandoffStep::LogoCheck { master: Some(LogoMaster::Europe), verdict: Some(ps2kit::disc::LogoVerdict::Accepted) }));
    assert!(matches!(&steps[10], HandoffStep::LoadElf { elf } if elf.lba == 20));
    assert_eq!(h13.next_record("SLUS_123.45").0.count, 14);
    assert_eq!(empty.next_record("SLUS_123.45"), (Record { name: "SLUS_123.45".into(), count: 1, mask: 1, index: 0, date: 0 }, false));
}
