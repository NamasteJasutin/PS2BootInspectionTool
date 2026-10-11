//! Invented ISO fixtures and optional caller-disc oracles; no game bytes ship.
use ps2kit::bench::Style;
use ps2kit::extract::{AssetKind, Extractor};
use ps2kit::library::{BankKind, BankResolver, Console, SoundLibrary};
use ps2kit::song::{BankRef, Channel, Instrument, InstrumentRef, Note, Pattern, Song};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join("x1-extract").join(format!(
            "test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn iso(&self, files: &[(&str, Vec<u8>)]) -> PathBuf {
        let path = self.0.join("Invented game.iso");
        std::fs::write(&path, iso(files)).unwrap();
        path
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn put16(d: &mut [u8], at: usize, n: u16) {
    d[at..at + 2].copy_from_slice(&n.to_le_bytes());
}
fn put32(d: &mut [u8], at: usize, n: u32) {
    d[at..at + 4].copy_from_slice(&n.to_le_bytes());
}
fn record(name: &[u8], lba: usize, size: usize, directory: bool) -> Vec<u8> {
    let mut r = vec![0; (33 + name.len()).div_ceil(2) * 2];
    r[0] = r.len() as u8;
    put32(&mut r, 2, lba as u32);
    put32(&mut r, 10, size as u32);
    r[25] = if directory { 2 } else { 0 };
    r[32] = name.len() as u8;
    r[33..33 + name.len()].copy_from_slice(name);
    r
}
fn iso(files: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut sectors = vec![vec![0u8; 2048]; 20];
    let mut root = Vec::new();
    let mut sub = Vec::new();
    root.extend(record(b"SOUNDS", 19, 2048, true));
    for (name, data) in files {
        let lba = sectors.len();
        for part in data.chunks(2048) {
            let mut sector = vec![0; 2048];
            sector[..part.len()].copy_from_slice(part);
            sectors.push(sector);
        }
        if data.is_empty() {
            sectors.push(vec![0; 2048]);
        }
        if let Some(name) = name.strip_prefix("SOUNDS/") {
            sub.extend(record(name.as_bytes(), lba, data.len(), false));
        } else {
            root.extend(record(name.as_bytes(), lba, data.len(), false));
        }
    }
    sectors[18][..root.len()].copy_from_slice(&root);
    sectors[19][..sub.len()].copy_from_slice(&sub);
    sectors[16][0] = 1;
    sectors[16][1..6].copy_from_slice(b"CD001");
    sectors[16][6] = 1;
    sectors[16][40..48].copy_from_slice(b"INVENTED");
    sectors[16][156..190].copy_from_slice(&record(&[0], 18, 2048, true));
    sectors.concat()
}
fn adpcm(value: u8) -> Vec<u8> {
    let mut body = vec![value; 32];
    body[0] = 0x0a;
    body[1] = 6;
    body[16] = 0x0a;
    body[17] = 3;
    body
}
fn vab(value: u8, id: usize) -> Vec<u8> {
    let header = 32 + 128 * 16 + 512 + 512;
    let mut data = vec![0; header];
    data[..4].copy_from_slice(b"pBAV");
    put32(&mut data, 4, 6);
    put32(&mut data, 12, (header + 32) as u32);
    put16(&mut data, 18, 1);
    put16(&mut data, 20, 1);
    put16(&mut data, 22, 1);
    data[24] = 127;
    data[25] = 64;
    data[32 + id * 16] = 1;
    data[33 + id * 16] = 127;
    let t = 32 + 128 * 16;
    data[t + 2] = 127;
    data[t + 3] = 64;
    data[t + 4] = 60;
    data[t + 6] = 48;
    data[t + 7] = 72;
    put16(&mut data, t + 16, 0xff);
    put16(&mut data, t + 22, 1);
    put16(&mut data, header - 512 + 2, 4);
    data.extend(adpcm(value));
    data
}
fn vag(value: u8, rate: u32) -> Vec<u8> {
    let mut data = vec![0; 48];
    data[..4].copy_from_slice(b"VAGp");
    data[4..8].copy_from_slice(&0x20u32.to_be_bytes());
    data[12..16].copy_from_slice(&32u32.to_be_bytes());
    data[16..20].copy_from_slice(&rate.to_be_bytes());
    data.extend(adpcm(value));
    data
}
fn sshd() -> Vec<u8> {
    let mut h = vec![0; 0x60];
    put32(&mut h, 0, 0x60);
    put32(&mut h, 4, 32);
    h[12..16].copy_from_slice(b"SShd");
    put32(&mut h, 0x10, 0x30);
    put32(&mut h, 0x14, u32::MAX);
    put32(&mut h, 0x2c, u32::MAX);
    put16(&mut h, 0x32, 8);
    h[0x39] = 127;
    h[0x41] = 127;
    h[0x42] = 60;
    put16(&mut h, 0x46, 0xff);
    h[0x4b] = 127;
    h[0x4c] = 64;
    h[0x4d] = 2;
    h
}
fn hd() -> Vec<u8> {
    let mut h = vec![0; 0x50];
    h[..8].copy_from_slice(b"IECSsreV");
    put32(&mut h, 8, 16);
    h[14] = 1;
    h[15] = 1;
    h[16..24].copy_from_slice(b"IECSdaeH");
    put32(&mut h, 24, 64);
    put32(&mut h, 32, 32);
    let chunk = |tag: &[u8], param: Vec<u8>| {
        let mut c = vec![0; 20];
        c[..4].copy_from_slice(b"IECS");
        c[4..8].copy_from_slice(tag);
        put32(&mut c, 8, (20 + param.len()) as u32);
        put32(&mut c, 16, 20);
        c.extend(param);
        c
    };
    let mut vagi = vec![0; 8];
    put16(&mut vagi, 4, 22050);
    vagi[6] = 1;
    let mut smpl = vec![0; 44];
    smpl[4] = 127;
    smpl[11] = 60;
    smpl[13] = 64;
    smpl[16] = 127;
    put16(&mut smpl, 18, 0xff);
    let sset = vec![0, 0, 127, 1, 0, 0];
    let mut prog = vec![0; 56];
    put32(&mut prog, 0, 36);
    prog[4] = 1;
    prog[5] = 20;
    prog[6] = 127;
    prog[40] = 127;
    prog[52] = 127;
    for (ptr, tag, param) in [
        (48, b"igaV", vagi),
        (44, b"lpmS", smpl),
        (40, b"tesS", sset),
        (36, b"gorP", prog),
    ] {
        let offset = h.len();
        put32(&mut h, ptr, offset as u32);
        h.extend(chunk(tag, param));
    }
    let size = h.len();
    put32(&mut h, 28, size as u32);
    h
}
fn wave() -> Vec<u8> {
    let mut w = vec![0; 44];
    w[..4].copy_from_slice(b"RIFF");
    put32(&mut w, 4, 44);
    w[8..16].copy_from_slice(b"WAVEfmt ");
    put32(&mut w, 16, 16);
    put16(&mut w, 20, 1);
    put16(&mut w, 22, 1);
    put32(&mut w, 24, 22050);
    put32(&mut w, 28, 22050);
    put16(&mut w, 32, 1);
    put16(&mut w, 34, 8);
    w[36..40].copy_from_slice(b"data");
    put32(&mut w, 40, 8);
    w.extend([128, 255, 128, 0, 128, 255, 128, 0]);
    w
}
fn seq() -> Vec<u8> {
    let mut s = b"pQES".to_vec();
    s.extend(1u32.to_be_bytes());
    s.extend(480u16.to_be_bytes());
    s.extend([7, 0xa1, 0x20, 4, 2]);
    s.extend([0, 0x90, 60, 100, 0x83, 0x60, 60, 0, 0, 0xff, 0x2f, 0]);
    s
}
fn sssq() -> Vec<u8> {
    let mut s = vec![0; 0x110];
    s[0] = 127;
    put16(&mut s, 2, 480);
    put16(&mut s, 4, 120);
    s[12..16].copy_from_slice(b"SSsq");
    s.extend([0x90, 60, 100, 0x83, 0x60, 60, 0, 0, 0xff, 0x2f, 0]);
    s
}
fn fixtures() -> Vec<(&'static str, Vec<u8>)> {
    let split = vab(0x32, 7);
    let header = split.len() - 32;
    let mut archive = vec![0x42; 37];
    archive.extend(vab(0x21, 3));
    archive.extend(vag(0x54, 48000));
    let mut inline = sshd();
    inline.resize(128, 0);
    inline.extend(adpcm(0x65));
    vec![
        ("SYSTEM.CNF;1", b"BOOT2 = cdrom0:\\TEST_001.23;1\r\n".to_vec()),
        ("ARCHIVE.BIN;1", archive),
        ("SOUNDS/SPLIT.VH;1", split[..header].to_vec()),
        ("SOUNDS/SPLIT.VB;1", split[header..].to_vec()),
        ("SOUNDS/DUP.VAG;1", vag(0x54, 48000)),
        ("SOUNDS/OTHER.VAG;1", vag(0x43, 22050)),
        ("SOUNDS/OSD.BIN;1", inline),
        ("SOUNDS/MUSIC.HD;1", hd()),
        ("SOUNDS/MUSIC.BD;1", adpcm(0x32)),
        ("SOUNDS/PCM.WAV;1", wave()),
        ("SOUNDS/SONG.SEQ;1", seq()),
        ("SOUNDS/SONG.SQ;1", sssq()),
        (
            "DECOYS.BIN;1",
            b"pBAV.....VAGp....pQES....SShd....SSsq....RIFF....IECSsreV".to_vec(),
        ),
    ]
}

#[test]
fn native_formats_pair_deduplicate_and_load() {
    let f = Fixture::new();
    let path = f.iso(&fixtures());
    let extraction = Extractor::scan(&path).unwrap();
    assert!(extraction.diagnostics.is_empty());
    assert_eq!(extraction.title_id, "TEST_001.23");
    for (kind, unique, occurrences) in [
        (AssetKind::Vab, 2, 2),
        (AssetKind::Vag, 2, 3),
        (AssetKind::SShd, 1, 1),
        (AssetKind::Hd, 1, 1),
        (AssetKind::Wave, 1, 1),
        (AssetKind::Seq, 1, 1),
        (AssetKind::SSsq, 1, 1),
    ] {
        let c = extraction.counts(kind);
        assert_eq!(
            (c.unique, c.validated, c.rejected),
            (unique, occurrences, 1),
            "{kind:?}: {:?}",
            extraction.rejected
        );
        assert_eq!(c.found, c.validated + c.rejected);
    }
    let split = extraction
        .assets
        .iter()
        .find(|a| a.kind == AssetKind::Vab && a.sources[0].file.ends_with(".VH;1"))
        .unwrap();
    assert!(split.sources[0].body_file.as_ref().unwrap().ends_with(".VB;1"));
    let set = extraction.write(f.0.join("Soundfonts")).unwrap();
    assert_eq!(set.assets, 9);
    let library = SoundLibrary::scan_with_soundfonts(&[], f.0.join("Soundfonts"));
    assert!(library.diagnostics.is_empty(), "{:?}", library.diagnostics);
    assert_eq!(library.banks.len(), 5);
    assert!(library.banks.iter().all(|b| b.console == Console::Game));
    assert_eq!(library.choices().len(), 7);
    let sample_bank = library.banks.iter().find(|b| b.kind == BankKind::GameSamples).unwrap();
    assert_eq!(sample_bank.bench.samples.len(), 3);
    assert_eq!(sample_bank.bench.sample_rate(), 44100);
    assert_eq!(
        sample_bank.bench.samples.iter().map(|s| s.length).collect::<Vec<_>>(),
        [16, 51, 112]
    );
    assert_eq!(sample_bank.bench.samples[1].loop_start, Some(0));
    let groups = library.by_style();
    let samples = &groups.iter().find(|r| r.0 == Style::Samples).unwrap().1;
    assert_eq!(samples.len(), 4); // Three loose samples and one HD Vagi.
    let bank = library
        .banks
        .iter()
        .find(|b| b.kind == BankKind::GameVab && b.bench.program(7).is_some())
        .unwrap();
    assert!(bank.bench.render(7, 60, 127, 0.05).iter().any(|v| v.abs() > 1e-6));
    assert!(bank.bench.render(7, 40, 127, 0.05).iter().all(|v| *v == 0.0));
    assert!(library.resolve(&bank.key, BankRef::Ps2).is_some());
    let mut song = Song::new("Extracted game", BankRef::Ps2);
    let mut channel = Channel::with_instrument("Sample", Instrument::Sample(0));
    channel.instrument = InstrumentRef::new(sample_bank.key.clone(), Instrument::Sample(0));
    let mut pattern = Pattern::new(song.ticks_per_bar());
    pattern.notes.push(Note::new(0, 120, 60, 100));
    channel.patterns.push(pattern);
    channel.arrangement.push(Some(0));
    song.channels.push(channel);
    song.validate().unwrap();
    assert!(song.render_checked(&library).unwrap().iter().any(|v| v.abs() > 1e-6));
}

#[test]
fn rerun_replaces_and_keeps_provenance() {
    let f = Fixture::new();
    let path = f.iso(&fixtures());
    let extraction = Extractor::scan(&path).unwrap();
    let root = f.0.join("Soundfonts");
    let first = extraction.write(&root).unwrap();
    let key = SoundLibrary::scan_with_soundfonts(&[], &root).banks[0].key.clone();
    let index = std::fs::read_to_string(first.folder.join("index.tsv")).unwrap();
    assert!(index.contains("SOUNDS/SPLIT.VB;1"));
    assert!(std::fs::read_to_string(first.folder.join("README.txt"))
        .unwrap()
        .contains("this folder is yours"));
    std::fs::write(first.folder.join("stale.vag"), b"old").unwrap();
    let second = extraction.write(&root).unwrap();
    assert_eq!(first.folder, second.folder);
    assert!(!second.folder.join("stale.vag").exists());
    assert_eq!(std::fs::read_to_string(second.folder.join("index.tsv")).unwrap(), index);
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
    assert!(SoundLibrary::scan_with_soundfonts(&[], &root)
        .resolve(&key, BankRef::Ps2)
        .is_some());
    std::fs::write(second.folder.join("index.tsv"), b"unowned").unwrap();
    assert!(extraction.write(&root).is_err());
}

#[test]
fn raw_cue_and_all_count_once() {
    let f = Fixture::new();
    let plain = iso(&fixtures());
    let mut raw = Vec::new();
    for sector in plain.chunks_exact(2048) {
        let mut r = vec![0; 2352];
        r[..12].copy_from_slice(&[0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 0]);
        r[15] = 2;
        r[24..2072].copy_from_slice(sector);
        raw.extend(r);
    }
    let bin = f.0.join("Invented (Track 01).bin");
    std::fs::write(&bin, raw).unwrap();
    std::fs::write(f.0.join("Invented (Track 02).bin"), vec![0x12; 2352]).unwrap();
    let cue = f.0.join("Invented.cue");
    std::fs::write(&cue, "FILE \"Invented (Track 01).bin\" BINARY\n TRACK 01 MODE2/2352\nFILE \"Invented (Track 02).bin\" BINARY\n TRACK 02 AUDIO\n").unwrap();
    let extraction = Extractor::scan(&cue).unwrap();
    assert_eq!(extraction.assets.len(), 9);
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_extract"))
        .arg("--all")
        .arg(&f.0)
        .arg("--out")
        .arg(f.0.join("Soundfonts"))
        .output()
        .unwrap();
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    assert_eq!(String::from_utf8_lossy(&result.stdout).matches("assets →").count(), 1);
}

#[test]
fn malformed_candidates_never_panic_or_escape() {
    let f = Fixture::new();
    let mut payloads = Vec::new();
    for template in [vab(0x32, 0), vag(0x21, 44100), sshd(), hd(), wave(), seq(), sssq()] {
        for len in [0, 1, 4, 12, 16, 31, template.len() / 2, template.len() - 1] {
            payloads.push(template[..len.min(template.len())].to_vec());
        }
        for value in [0, 0xff, 0x80] {
            let mut damaged = template.clone();
            for at in (4..damaged.len()).step_by(7) {
                damaged[at] = value;
            }
            payloads.push(damaged);
        }
    }
    for (i, payload) in payloads.into_iter().enumerate() {
        let path = f.iso(&[("BAD.BIN;1", payload)]);
        let result = std::panic::catch_unwind(|| Extractor::scan(path));
        assert!(result.is_ok(), "malformed candidate {i} panicked");
    }
    let invalid = f.0.join("invalid.iso");
    std::fs::write(&invalid, vec![0xff; 40000]).unwrap();
    assert!(Extractor::scan(invalid).is_err());
    let path = f.iso(&fixtures());
    let extraction = Extractor::scan(path).unwrap();
    let set = extraction.write(f.0.join("Soundfonts")).unwrap();
    let index_path = set.folder.join("index.tsv");
    let index = std::fs::read_to_string(&index_path).unwrap();
    std::fs::write(&index_path, index.replace("bank-0001.vab", "../escape.vab")).unwrap();
    let library = SoundLibrary::scan_with_soundfonts(&[], set.folder);
    assert!(library.banks.is_empty());
    assert!(!library.diagnostics.is_empty());
}

#[test]
fn aligned_vab_and_separate_sshd_body() {
    let f = Fixture::new();
    let bank = vab(0x21, 0);
    let header = bank.len() - 32;
    let mut archive = vec![0xab; 64];
    archive.extend(&bank[..header]);
    archive.resize(archive.len().div_ceil(2048) * 2048, 0xab);
    archive.extend(&bank[header..]);
    let path = f.iso(&[
        ("BANK.ARC;1", archive),
        ("SOUNDS/OSD.SSHD;1", sshd()),
        ("SOUNDS/OSD.SSBD;1", adpcm(0x32)),
    ]);
    let extraction = Extractor::scan(path).unwrap();
    assert!(extraction.rejected.is_empty(), "{:?}", extraction.rejected);
    assert_eq!(extraction.assets.len(), 2);
    let bank = &extraction.assets[0];
    assert_eq!(bank.sources[0].offset, 64);
    assert_eq!(bank.sources[0].body_offset, Some(4096));
}

#[test]
fn bounded_adpcm_and_hd_graph_reject_corruption() {
    let f = Fixture::new();
    let mut no_end = vag(0x21, 44100);
    no_end[65] = 2;
    let mut invalid_filter = vab(0x32, 0);
    let last = invalid_filter.len() - 32;
    invalid_filter[last] = 0x51;
    let mut bad_ref = hd();
    let smpl = u32::from_le_bytes(bad_ref[44..48].try_into().unwrap()) as usize;
    put16(&mut bad_ref, smpl + 20, 99);
    let mut bad_seq = seq();
    bad_seq.truncate(bad_seq.len() - 1);
    let path = f.iso(&[
        ("BAD.VAG;1", no_end),
        ("BAD.VAB;1", invalid_filter),
        ("BAD.HD;1", bad_ref),
        ("BAD.BD;1", adpcm(0x32)),
        ("BAD.SEQ;1", bad_seq),
    ]);
    let extraction = Extractor::scan(path).unwrap();
    assert!(extraction.assets.is_empty());
    assert_eq!(extraction.rejected.len(), 4);
}

#[cfg(unix)]
#[test]
fn symlink_destination_is_refused() {
    let f = Fixture::new();
    let path = f.iso(&fixtures());
    let extraction = Extractor::scan(path).unwrap();
    let root = f.0.join("Soundfonts");
    let first = extraction.write(&root).unwrap();
    let other = f.0.join("Other");
    std::fs::rename(&first.folder, &other).unwrap();
    std::os::unix::fs::symlink(&other, &first.folder).unwrap();
    assert!(extraction.write(&root).is_err());
    assert!(other.join("index.tsv").is_file());
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

#[test]
fn caller_tekken_europe_and_japan() {
    for region in ["Europe", "Japan"] {
        let p = root()
            .join("discs")
            .join(format!("Tekken ({region})"))
            .join(format!("Tekken ({region}) (Track 01).bin"));
        let Ok(_) = std::fs::File::open(&p) else { continue };
        let extraction = Extractor::scan(p).unwrap();
        let counts = extraction.counts(AssetKind::Vab);
        eprintln!(
            "Tekken {region}: VAB found {}, validated {}, rejected {}, unique {}",
            counts.found, counts.validated, counts.rejected, counts.unique
        );
        assert_eq!(counts.found, 39);
        assert_eq!((counts.validated, counts.rejected, counts.unique), (38, 1, 20));
        let f = Fixture::new();
        let set = extraction.write(f.0.join("Soundfonts")).unwrap();
        let library = SoundLibrary::scan_with_soundfonts(&[], &set.folder);
        assert!(library.diagnostics.is_empty(), "{:?}", library.diagnostics);
        assert_eq!(library.banks.len(), counts.unique);
        assert!(library.banks.iter().any(|b| b.bench.programs.iter().any(|p| {
            let key = p.tones.first().map_or(60, |t| t.root);
            b.bench.render(p.id, key, 127, 0.1).iter().any(|v| v.abs() > 1e-6)
        })));
    }
}

#[test]
fn caller_tekken_tag_usa() {
    let p = root().join("discs/Tekken Tag Tournament (USA) (v2.00)/Tekken Tag Tournament (USA) (v2.00).bin");
    let Ok(_) = std::fs::File::open(&p) else { return };
    let extraction = Extractor::scan(p).unwrap();
    let vag = extraction.counts(AssetKind::Vag);
    assert_eq!(
        (vag.found, vag.validated, vag.rejected, vag.unique),
        (2355, 2350, 5, 647)
    );
    for (kind, found) in [(AssetKind::SShd, 1), (AssetKind::Seq, 1), (AssetKind::Wave, 15)] {
        let c = extraction.counts(kind);
        assert_eq!((c.found, c.validated, c.rejected), (found, 0, found));
    }
    let f = Fixture::new();
    let set = extraction.write(f.0.join("Soundfonts")).unwrap();
    let library = SoundLibrary::scan_with_soundfonts(&[], set.folder);
    assert!(library.diagnostics.is_empty(), "{:?}", library.diagnostics);
    assert_eq!(library.banks.len(), 1);
    assert_eq!(library.banks[0].bench.samples.len(), 647);
    assert_eq!(library.choices().len(), 647);
    assert_eq!(
        library
            .by_style()
            .iter()
            .find(|r| r.0 == Style::Samples)
            .unwrap()
            .1
            .len(),
        647
    );
}

#[test]
fn damaged_native_asset_is_diagnostic_and_other_banks_load() {
    let f = Fixture::new();
    let path = f.iso(&fixtures());
    let set = Extractor::scan(path).unwrap().write(f.0.join("Soundfonts")).unwrap();
    std::fs::remove_file(set.folder.join("bank-0001.vab")).unwrap();
    let library = SoundLibrary::scan_with_soundfonts(&[], set.folder);
    assert_eq!(library.banks.len(), 4);
    assert_eq!(library.diagnostics.len(), 1);
}
