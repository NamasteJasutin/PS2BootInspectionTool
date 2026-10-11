//! Invented parser fixtures plus optional inventory oracles from the caller's BIOS folder.
//! Facts: notes/research/sound_inventory.md §§Banks, Every stored score, What is truly distinct.
use ps2kit::bench::SoundBench;
use ps2kit::library::{BankKey, BankKind, BankResolver, Evidence, SoundLibrary};
use ps2kit::song::{BankRef, Channel, DrumVoice, Instrument, InstrumentRef, Note, Pattern, SampleRef, Song};
use ps2kit::sound::SAMPLE_RATE;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join("m3-library").join(format!(
            "test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
    fn write(&self, name: &str, data: &[u8]) -> PathBuf {
        let path = self.0.join(name);
        std::fs::write(&path, data).unwrap();
        path
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn put16(data: &mut [u8], offset: usize, value: u16) {
    data[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}
fn put32(data: &mut [u8], offset: usize, value: u32) {
    data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}
fn adpcm(value: u8) -> Vec<u8> {
    let mut data = vec![value; 32];
    data[0] = 0;
    data[1] = 4;
    data[16] = 0;
    data[17] = 3;
    data
}
fn program_header() -> Vec<u8> {
    let mut h = vec![0; 0x60];
    put32(&mut h, 0, 0x60);
    put32(&mut h, 4, 32);
    h[12..16].copy_from_slice(b"SShd");
    put32(&mut h, 0x10, 0x20);
    put32(&mut h, 0x14, u32::MAX);
    put32(&mut h, 0x2c, u32::MAX);
    put16(&mut h, 0x22, 0x10);
    h[0x31] = 100;
    h[0x39] = 127;
    h[0x3a] = 60;
    put16(&mut h, 0x3e, 0xff);
    h[0x43] = 100;
    h[0x44] = 64;
    h[0x45] = 2;
    h
}
fn effects(count: usize) -> Vec<u8> {
    let mut h = vec![0; 0xa0 + count * 64];
    put32(&mut h, 0, u32::MAX);
    put32(&mut h, 4, 32);
    h[12..16].copy_from_slice(b"SShd");
    put32(&mut h, 0x10, u32::MAX);
    put32(&mut h, 0x14, u32::MAX);
    put32(&mut h, 0x2c, 0x80);
    put32(&mut h, 0x80, 100);
    put32(&mut h, 0x84, count as u32 - 1);
    for i in 0..count {
        let p = 0xa0 + i * 64;
        put16(&mut h, p, 4000 + i as u16);
        put16(&mut h, p + 2, 4000);
        put16(&mut h, p + 4, 4096);
        put16(&mut h, p + 8, 0xff);
    }
    h
}
fn inline(header: &[u8], body: &[u8]) -> Vec<u8> {
    let mut data = header.to_vec();
    data.resize((header.len() + 63) & !63, 0xaa);
    data.extend_from_slice(body);
    data
}
fn vab() -> Vec<u8> {
    vab_with_programs(1)
}
fn vab_with_programs(programs: usize) -> Vec<u8> {
    let tone = 32 + 128 * 16;
    let table = tone + programs * 512;
    let header = table + 512;
    let mut data = vec![0; header + 32];
    data[..4].copy_from_slice(b"pBAV");
    put32(&mut data, 4, 6);
    put32(&mut data, 12, (header + 32) as u32);
    put16(&mut data, 18, programs as u16);
    put16(&mut data, 20, programs as u16);
    put16(&mut data, 22, 1);
    data[24] = 100;
    data[25] = 64;
    for i in 0..programs {
        let p = 32 + i * 16;
        let t = tone + i * 512;
        data[p] = 1;
        data[p + 1] = 100;
        data[p + 4] = 64;
        data[t + 2] = 100;
        data[t + 3] = 64;
        data[t + 4] = 60;
        data[t + 7] = 127;
        put16(&mut data, t + 16, 0xff);
        put16(&mut data, t + 22, 1);
    }
    put16(&mut data, table + 2, 4);
    data[header..].copy_from_slice(&adpcm(0x35));
    data
}
fn vag() -> Vec<u8> {
    let mut data = vec![0; 0x30];
    data[..4].copy_from_slice(b"VAGp");
    data[4..8].copy_from_slice(&3u32.to_be_bytes());
    data[12..16].copy_from_slice(&32u32.to_be_bytes());
    data[16..20].copy_from_slice(&48000u32.to_be_bytes());
    data.extend(adpcm(0x52));
    data
}
fn lz(data: &[u8]) -> Vec<u8> {
    let mut out = (data.len() as u32).to_le_bytes().to_vec();
    for group in data.chunks(30) {
        out.extend_from_slice(&[0; 4]);
        out.extend_from_slice(group);
    }
    out
}
fn rom(modules: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let entries = modules.len() + 3;
    let size = (entries * 16 + 15) & !15;
    let mut names = vec![("RESET", Vec::new()), ("ROMDIR", vec![0; size])];
    names.extend_from_slice(modules);
    let mut table = vec![0; size];
    for (i, (name, data)) in names.iter().enumerate() {
        table[i * 16..i * 16 + name.len()].copy_from_slice(name.as_bytes());
        put32(&mut table, i * 16 + 12, data.len() as u32);
    }
    names[1].1 = table;
    let mut data = Vec::new();
    for (_, bytes) in names {
        data.extend(bytes);
        data.resize((data.len() + 15) & !15, 0);
    }
    data
}
fn simple_song(bank: BankRef, key: u8) -> Song {
    let mut song = Song::new("Invented song", bank);
    let mut channel = Channel::new("Track", 0);
    let mut pattern = Pattern::new(1920);
    pattern.notes.push(Note::new(0, 480, key, 100));
    channel.patterns.push(pattern);
    channel.arrangement.push(Some(0));
    song.channels.push(channel);
    song.end_tick = Some(960);
    song
}
fn library_fixture(f: &Fixture) -> SoundLibrary {
    SoundLibrary::scan(&[
        f.write("boot.bin", &inline(&program_header(), &adpcm(0x55))),
        f.write("menu12.bin", &inline(&effects(12), &adpcm(0x46))),
        f.write("menu14.bin", &inline(&effects(14), &adpcm(0x46))),
        f.write("logo.bin", &inline(&effects(5), &adpcm(0x63))),
        f.write("shell.vab", &vab()),
        f.write("test.vag", &vag()),
    ])
}

#[test]
fn exact_content_dedup_and_menu_variant_resolution() {
    let f = Fixture::new();
    let library = library_fixture(&f);
    assert_eq!(library.banks.len(), 5);
    assert_eq!(library.choices().len(), 22);
    assert!(library.diagnostics.is_empty(), "{:?}", library.diagnostics);
    let menu = library.banks.iter().find(|b| b.kind == BankKind::Ps2Menu).unwrap();
    assert_eq!(menu.bench.effects.len(), 14);
    assert_eq!(menu.variants.len(), 1);
    assert_eq!(menu.sources.len(), 2);
    assert_ne!(menu.key, menu.variants[0].key);
    assert_eq!(menu.variants[0].bench.effects.len(), 12);
    assert_eq!(library.resolve(&menu.variants[0].key, BankRef::Ps2).unwrap().bench.effects.len(), 12);
    assert_eq!(menu.variants[0].bench.render_effect(5, 0.1), menu.bench.render_effect(5, 0.1));
    let duplicate = f.write("same-bank-other-file", &inline(&program_header(), &adpcm(0x55)));
    let original = f.0.join("boot.bin");
    let mut padded = std::fs::read(&original).unwrap();
    padded[0x60..0x80].fill(0x7a);
    padded.extend_from_slice(&[0xb7; 80]);
    let padded = f.write("padding-changed", &padded);
    let mut changed = program_header();
    changed[0x31] += 1;
    let changed = f.write("envelope-changed", &inline(&changed, &adpcm(0x55)));
    let dedup = SoundLibrary::scan(&[original, duplicate, padded, changed]);
    assert_eq!(dedup.banks.len(), 2);
    assert_eq!(dedup.banks[0].sources.len(), 3);
    assert_ne!(dedup.banks[0].key, dedup.banks[1].key);
    let mut renamed = dedup.banks[0].key.clone();
    renamed.label = "Renamed by user".into();
    assert_eq!(renamed, dedup.banks[0].key);
    assert!(dedup.resolve(&renamed, BankRef::Ps2).is_some());
    // Input order must not affect the preferred header or shared choices.
    let a = f.0.join("menu12.bin");
    let b = f.0.join("menu14.bin");
    assert_eq!(SoundLibrary::scan(&[a.clone(), b.clone()]).choices(), SoundLibrary::scan(&[b, a]).choices());
}

#[test]
fn synthetic_shell_note_tables_keep_exact_extent_and_clock() {
    let f = Fixture::new();
    let mut image = vab_with_programs(4);
    for (program, count) in [(0u8, 19), (1, 40)] {
        for i in 0..count {
            image.extend_from_slice(&(i as u32 * 3).to_le_bytes());
            image.extend_from_slice(&[program, 60, if i % 2 == 0 { 100 } else { 0 }, 64]);
        }
        image.extend_from_slice(&9999u32.to_le_bytes());
        image.extend_from_slice(&[0; 4]);
    }
    let a = f.write("shell-tables", &image);
    image.extend_from_slice(&[0x37; 70]);
    let b = f.write("same-tables-with-padding", &image);
    let library = SoundLibrary::scan(&[a, b]);
    assert_eq!(library.banks.len(), 1);
    assert_eq!(library.pieces.len(), 2);
    for id in ["Q08", "Q09"] {
        let piece = library.piece(id).unwrap();
        assert_eq!(piece.sources.len(), 2);
        let song = piece.to_song(&library).unwrap();
        assert_eq!(song.ticks_per_beat, 60);
        assert!(song.render(&library).iter().any(|v| v.abs() > 1e-6));
        assert_eq!(piece.length_s, if id == "Q08" { 0.9 } else { 1.95 });
    }
}

#[test]
fn named_scores_hash_through_terminator_and_import_marked_loop() {
    let f = Fixture::new();
    let names = ["SNDBOOTS", "SNDTNNLS", "SNDCLOKS", "SNDTM30S", "SNDTM60S", "SNDLOGOS", "SNDWARNS", "SNDRCLKS"];
    let mut modules = vec![("SNDBOOTH", lz(&program_header())), ("SNDBOOTB", lz(&adpcm(0x55)))];
    let mut alias = Vec::new();
    for (i, name) in names.into_iter().enumerate() {
        let mut seq = simple_song(BankRef::Ps2, 60 + i as u8).to_sssq().unwrap();
        if i == 2 || i == 6 {
            // Invented half-bar loop: markers at 480 and 960 ticks, one held note.
            seq.truncate(0x110);
            seq.extend_from_slice(&[
                0x90, 60, 100, 0x83, 0x60, 0xb0, 99, 20, 0, 0xb0, 6, 127, 0, 0x80, 60, 0, 0x83, 0x60, 0xb0, 99, 30, 0,
                0xff, 0x2f, 0,
            ]);
            if i == 6 {
                seq[4..6].copy_from_slice(&110u16.to_le_bytes());
            }
        }
        if i == 3 {
            alias = seq.clone();
        }
        if i == 5 {
            seq = alias.clone();
            seq.extend_from_slice(&[0xff; 57]);
        }
        modules.push((name, lz(&seq)));
    }
    let file = f.write("synthetic-rom0", &rom(&[("ROMVER", b"0200EC20040614".to_vec()), ("SNDIMAGE", rom(&modules))]));
    let library = SoundLibrary::scan(&[file]);
    assert_eq!(library.pieces.len(), 7);
    assert!(library.diagnostics.is_empty(), "{:?}", library.diagnostics);
    assert_eq!(library.piece("SNDLOGOS").unwrap().key, library.piece("SNDTM30S").unwrap().key);
    assert_eq!(library.piece("Q04").unwrap().sources.len(), 2);
    assert_eq!(library.piece("Q04").unwrap().evidence, Evidence::RegisteredCue);
    for piece in &library.pieces {
        let song = piece.to_song(&library).unwrap();
        assert!(song.missing_banks(&library).is_empty());
        assert!(song.render(&library).iter().any(|v| v.abs() > 1e-6));
    }
    let song = library.piece("Q03").unwrap().to_song(&library).unwrap();
    assert_eq!(song.beats_per_bar, 1);
    assert_eq!(song.loop_bars, Some((1, 2)));
    assert_eq!(song.end_tick, Some(960));
    let text = song.to_text().unwrap();
    assert!(text.starts_with("psxsong 3\n"));
    assert_eq!(Song::parse(&text).unwrap(), song);
}

#[test]
fn mixed_native_programs_drums_samples_and_missing_tracks_round_trip() {
    let f = Fixture::new();
    let library = library_fixture(&f);
    let ps1 = library.banks.iter().find(|b| b.kind == BankKind::Ps1Shell).unwrap();
    let boot = library.banks.iter().find(|b| b.kind == BankKind::Ps2Boot).unwrap();
    let sine = library.banks.iter().find(|b| b.kind == BankKind::TestSine).unwrap();
    let mut song = simple_song(BankRef::Ps2, 60);
    song.channels[0].instrument.bank = ps1.key.clone();
    let mut ps2 = song.channels[0].clone();
    ps2.name = "PS2".into();
    ps2.instrument.bank = boot.key.clone();
    ps2.patterns[0].notes[0].start = 480;
    song.channels.push(ps2);
    let mut drum = song.channels[0].clone();
    drum.name = "PS2 sample drum".into();
    drum.instrument =
        InstrumentRef::new(boot.key.clone(), Instrument::Drum(DrumVoice::new(SampleRef::sample(BankRef::Ps2, 0))));
    song.channels.push(drum);
    let mut loose = song.channels[0].clone();
    loose.instrument = InstrumentRef::new(sine.key.clone(), Instrument::Sample(0));
    song.channels.push(loose);
    let text = song.to_text().unwrap();
    let parsed = Song::parse(&text).unwrap();
    assert_eq!(parsed, song);
    assert_eq!(parsed.to_text().unwrap(), text);
    assert_eq!(parsed.render(&library), song.render(&library));
    assert!(song.to_sssq().is_err());
    let pcm = song.render(&library);
    assert!(pcm[..SAMPLE_RATE].iter().any(|v| v.abs() > 1e-5));
    assert!(pcm[SAMPLE_RATE..].iter().any(|v| v.abs() > 1e-5));
    for i in 0..song.channels.len() {
        let mut single = song.clone();
        single.channels = vec![song.channels[i].clone()];
        assert!(single.render(&library).iter().any(|v| v.abs() > 1e-6), "track {i}");
    }
    let mut absent = library.clone();
    absent.banks.retain(|b| b.key != ps1.key);
    assert_eq!(song.missing_banks(&absent), vec![ps1.key.clone()]);
    let mut remaining = song.clone();
    remaining.channels.remove(0);
    assert_eq!(song.render(&absent), remaining.render(&library));
    let mut missing_song = simple_song(BankRef::Ps2, 60);
    missing_song.channels[0].instrument.bank =
        BankKey::new("1234567890abcdef1234567890abcdef", "PS2 logo bank (from a 1.50+ BIOS)");
    assert_eq!(missing_song.missing_banks(&library)[0].label, "PS2 logo bank (from a 1.50+ BIOS)");
    assert!(missing_song.render(&library).iter().all(|v| *v == 0.0));
    assert_eq!(sine.bench.sample_rate(), 48000);
    assert_eq!(sine.bench.samples[0].length, 56);
    assert!(sine.bench.programs.is_empty());
}

#[test]
fn legacy_files_and_single_bank_audio_remain_identical() {
    let f = Fixture::new();
    let library = library_fixture(&f);
    for (kind, default) in [(BankKind::Ps2Boot, BankRef::Ps2), (BankKind::Ps1Shell, BankRef::Ps1)] {
        let bank = library.banks.iter().find(|b| b.kind == kind).unwrap();
        let mut song = simple_song(default, 60);
        let legacy = song.to_text().unwrap();
        assert!(legacy.starts_with("psxsong 1\n"));
        let migrated = Song::parse(&legacy).unwrap();
        assert!(migrated.channels[0].instrument.bank.is_default());
        assert_eq!(migrated.render(&library), song.render(&bank.bench));
        song.channels[0].instrument.bank = bank.key.clone();
        assert_eq!(song.render(&library), migrated.render(&bank.bench));
        assert_eq!(song.render_from(&library, 480).unwrap(), migrated.render_from(&bank.bench, 480).unwrap());
        assert_eq!(song.render_bar_range(&library, 0..1), migrated.render_bar_range(&bank.bench, 0..1));
        let extended =
            Channel::with_instrument("Recipe", Instrument::Drum(DrumVoice::new(SampleRef::sample(default, 0))));
        let mut old2 = migrated;
        old2.channels.push(extended);
        let text = old2.to_text().unwrap();
        assert!(text.starts_with("psxsong 2\n"));
        assert_eq!(Song::parse(&text).unwrap(), old2);
    }
}

#[test]
fn legacy_ps2_effect_and_appended_menu_sample_references_stay_audible() {
    let f = Fixture::new();
    let archive = rom(&[
        ("SNDBOOTH", lz(&program_header())),
        ("SNDBOOTB", lz(&adpcm(0x55))),
        ("SNDOSDDH", lz(&effects(14))),
        ("SNDOSDDB", lz(&adpcm(0x46))),
    ]);
    let bytes = rom(&[("SNDIMAGE", archive)]);
    let file = f.write("old-combined-bench", &bytes);
    let original = SoundBench::ps2(&ps2kit::rom::RomDir::new(bytes).unwrap()).unwrap();
    let library = SoundLibrary::scan(&[file]);
    assert_eq!(original.samples.len(), 2);
    for instrument in [Instrument::Effect(0), Instrument::Drum(DrumVoice::new(SampleRef::sample(BankRef::Ps2, 1)))] {
        let mut song = simple_song(BankRef::Ps2, 60);
        song.channels[0].instrument = instrument.into();
        let legacy = song.to_text().unwrap();
        assert!(legacy.starts_with("psxsong 2\n"));
        let parsed = Song::parse(&legacy).unwrap();
        let expected = song.render(&original);
        assert!(expected.iter().any(|v| v.abs() > 1e-6));
        assert_eq!(expected, parsed.render(&library));
    }
}

#[test]
fn malformed_extents_and_version_three_records_are_bounded() {
    let f = Fixture::new();
    let mut bad_vab = vab();
    put32(&mut bad_vab, 12, u32::MAX);
    let mut bad_bank = program_header();
    put32(&mut bad_bank, 0x10, u32::MAX - 1);
    let mut bad_effects = effects(12);
    put32(&mut bad_effects, 0x84, u32::MAX);
    let mut bad_vag = vag();
    bad_vag[12..16].copy_from_slice(&u32::MAX.to_be_bytes());
    let library = SoundLibrary::scan(&[
        f.write("bad-vab", &bad_vab),
        f.write("bad-bank", &inline(&bad_bank, &adpcm(0x55))),
        f.write("bad-effects", &inline(&bad_effects, &adpcm(0x55))),
        f.write("bad-vag", &bad_vag),
    ]);
    assert!(library.banks.is_empty());
    assert!(library.diagnostics.iter().any(|d| d.contains("no valid complete")));
    let mut song = simple_song(BankRef::Ps2, 60);
    song.channels[0].instrument.bank = BankKey::new("a".repeat(32), "A \"bank\" 日本語");
    let text = song.to_text().unwrap();
    for end in 0..text.len() {
        if text.is_char_boundary(end) {
            let _ = Song::parse(&text[..end]);
        }
    }
    let binding = text.lines().find(|l| l.starts_with("binding")).unwrap();
    assert!(Song::parse(&text.replace(binding, "")).is_err());
    assert!(Song::parse(&text.replace(binding, &format!("{binding}\n{binding}"))).is_err());
    assert!(Song::parse(&text.replace(&"a".repeat(32), "bad fingerprint")).is_err());
    assert!(Song::parse(&text.replace("psxsong 3", "psxsong 2")).is_err());
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}
fn collect_files(dir: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else { continue };
        if kind.is_dir() {
            collect_files(&entry.path(), files);
        } else if kind.is_file() {
            files.push(entry.path());
        }
    }
}

#[test]
fn user_inventory_counts_dedup_all_imports_and_mixed_song() {
    let folder = root().join("bios");
    let Ok(_) = std::fs::read(folder.join("scph10000.bin")) else { return };
    let Ok(_) = std::fs::read(folder.join("scph39001.bin")) else { return };
    let Ok(_) = std::fs::read(folder.join("scph5501.bin")) else { return };
    let mut files = Vec::new();
    collect_files(&folder, &mut files);
    let library = SoundLibrary::scan(&files);
    assert_eq!(library.banks.len(), 5);
    assert_eq!(library.choices().len(), 41);
    assert_eq!(library.pieces.len(), 10);
    let bank = |kind| library.banks.iter().find(|b| b.kind == kind).unwrap();
    assert_eq!(bank(BankKind::Ps2Boot).bench.programs.len(), 17);
    assert_eq!(bank(BankKind::Ps1Shell).bench.programs.len(), 4);
    assert!(bank(BankKind::Ps1Shell).sources.len() >= 3);
    assert!(bank(BankKind::Ps2Boot).sources.len() >= 2);
    assert_eq!(bank(BankKind::Ps2Menu).bench.effects.len(), 14);
    assert_eq!(bank(BankKind::Ps2Menu).variants.len(), 1);
    assert_eq!(bank(BankKind::Ps2Menu).variants[0].bench.effects.len(), 12);
    let sine = bank(BankKind::TestSine);
    assert_eq!(sine.bench.sample_rate(), 48000);
    assert_eq!(sine.bench.samples[0].length, 2128);
    assert_eq!(library.piece("SNDLOGOS").unwrap().key, library.piece("SNDTM30S").unwrap().key);
    for piece in &library.pieces {
        let song = piece.to_song(&library).unwrap_or_else(|e| panic!("{}: {e}", piece.label));
        assert!(song.missing_banks(&library).is_empty());
        let pcm = song.render_checked(&library).unwrap();
        assert!(pcm.iter().any(|v| v.abs() > 1e-6), "{} silent", piece.label);
        assert_eq!(Song::parse(&song.to_text().unwrap()).unwrap(), song);
        if matches!(piece.aliases[0].as_str(), "Q03" | "Q06") {
            assert_eq!(song.channels.iter().flat_map(|c| &c.patterns).map(|p| p.notes.len()).sum::<usize>(), 126);
            let (start, end) = song.loop_bars.unwrap();
            let ticks = (start as u32 * song.ticks_per_bar(), end as u32 * song.ticks_per_bar());
            assert_eq!(ticks, (if piece.aliases[0] == "Q03" { 18240 } else { 17280 }, 288000));
        }
        if matches!(piece.aliases[0].as_str(), "Q08" | "Q09") {
            assert!(!piece.console_timing.is_empty());
            for index in 0..piece.console_timing.len() {
                let timed = piece.to_song_variant(&library, index).unwrap();
                assert!(timed.render(&library).iter().any(|v| v.abs() > 1e-6));
            }
        }
    }
    let mut song = simple_song(BankRef::Ps2, 60);
    song.channels[0].instrument = InstrumentRef::new(bank(BankKind::Ps1Shell).key.clone(), Instrument::Program(3));
    let mut drone = song.channels[0].clone();
    drone.instrument = InstrumentRef::new(bank(BankKind::Ps2Boot).key.clone(), Instrument::Program(50));
    song.channels.push(drone);
    let boot = bank(BankKind::Ps2Boot);
    let kit = boot.kit.as_ref().expect("boot-bank noise derives a playable kit");
    let mut percussion = song.channels[0].clone();
    percussion.instrument = InstrumentRef::new(boot.key.clone(), Instrument::Kit(kit.clone()));
    percussion.patterns[0].notes[0].key = kit.pieces[0].key;
    song.channels.push(percussion);
    for c in &song.channels {
        let mut single = song.clone();
        single.channels = vec![c.clone()];
        assert!(single.render(&library).iter().any(|v| v.abs() > 1e-6));
    }
    assert_eq!(song.render(&library), Song::parse(&song.to_text().unwrap()).unwrap().render(&library));
    for file in ["scph10000.bin", "scph39001.bin", "SCPH-70004_BIOS_V12_PAL_200.BIN"] {
        let Ok(data) = std::fs::read(folder.join(file)) else { continue };
        let rom = ps2kit::rom::RomDir::new(data).unwrap();
        let song = Song::from_ps2_boot(&rom).unwrap();
        let bench = SoundBench::ps2(&rom).unwrap();
        let direct = song.render(&bench);
        let resolved = song.render(&library);
        assert_eq!(
            direct.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
            resolved.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
            "{file}"
        );
    }
    assert_eq!(library.by_style().iter().map(|(_, p)| p.len()).sum::<usize>(), 41);
}
