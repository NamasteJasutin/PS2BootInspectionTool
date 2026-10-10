//! Synthetic file gates and optional, silent-when-absent checks of the user's dumps.

use ps2kit::disc::{DiscImage, DiscKind};
use ps2kit::hooks::{Hook, HookMap, Verdict};
use ps2kit::memcard::MemoryCard;
use ps2kit::nvm::{BiosSet, Nvm};
use ps2kit::rom::RomDir;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn root() -> PathBuf { Path::new(env!("CARGO_MANIFEST_DIR")).join("..") }
fn put16(b: &mut [u8], o: usize, v: u16) { b[o..o + 2].copy_from_slice(&v.to_le_bytes()); }
fn put32(b: &mut [u8], o: usize, v: u32) { b[o..o + 4].copy_from_slice(&v.to_le_bytes()); }

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let p = Path::new("/tmp/b3a-hookmap").join(format!("tests-{}-{stamp}", std::process::id()));
        std::fs::create_dir_all(&p).unwrap();
        Self(p)
    }
    fn write(&self, name: &str, data: impl AsRef<[u8]>) -> PathBuf {
        let p = self.0.join(name);
        std::fs::write(&p, data).unwrap();
        p
    }
    fn card(&self, files: &[(&str, &[u8])]) -> MemoryCard {
        MemoryCard::open(self.write("card.ps2", card_image(files))).unwrap()
    }
}
impl Drop for Scratch { fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); } }

// Minimal formatted raw PS2 card: 512-byte clusters, one indirect/FAT cluster,
// and real directory/file chains. All payloads below are original synthetic text.
fn card_image(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut folders: BTreeMap<&str, Vec<(&str, &[u8])>> = BTreeMap::new();
    for &(path, data) in files {
        let (folder, name) = path.split_once('/').unwrap();
        folders.entry(folder).or_default().push((name, data));
    }
    let root_count = 2 + folders.len();
    let mut clusters = vec![vec![0u8; 512]; root_count];
    let mut fat: Vec<u32> = (0..root_count).map(|i| if i + 1 == root_count { u32::MAX } else { 0x80000000 | (i + 1) as u32 }).collect();
    let entry = |name: &str, directory: bool, cluster: u32, length: usize| {
        let mut e = vec![0; 512];
        put16(&mut e, 0, if directory { 0x8020 } else { 0x8010 });
        put32(&mut e, 4, length as u32); put32(&mut e, 0x10, cluster);
        e[0x40..0x40 + name.len()].copy_from_slice(name.as_bytes());
        e
    };
    clusters[0] = entry(".", true, 0, root_count);
    clusters[1] = entry("..", true, 0, root_count);
    for (i, (folder, files)) in folders.into_iter().enumerate() {
        let first = clusters.len();
        let count = 2 + files.len();
        for j in 0..count {
            clusters.push(vec![0; 512]);
            fat.push(if j + 1 == count { u32::MAX } else { 0x80000000 | (first + j + 1) as u32 });
        }
        clusters[2 + i] = entry(folder, true, first as u32, count);
        clusters[first] = entry(".", true, first as u32, count);
        clusters[first + 1] = entry("..", true, 0, root_count);
        for (j, (name, bytes)) in files.into_iter().enumerate() {
            let start = clusters.len();
            let pages = bytes.len().div_ceil(512).max(1);
            for k in 0..pages {
                let mut cluster = vec![0; 512];
                let slice = &bytes[(k * 512).min(bytes.len())..((k + 1) * 512).min(bytes.len())];
                cluster[..slice.len()].copy_from_slice(slice);
                clusters.push(cluster);
                fat.push(if k + 1 == pages { u32::MAX } else { 0x80000000 | (start + k + 1) as u32 });
            }
            clusters[first + 2 + j] = entry(name, false, start as u32, bytes.len());
        }
    }
    assert!(clusters.len() < 125);
    let mut data = vec![0; 128 * 512];
    let magic = b"Sony PS2 Memory Card Format ";
    data[..magic.len()].copy_from_slice(magic);
    put16(&mut data, 0x28, 512); put16(&mut data, 0x2A, 1);
    put32(&mut data, 0x30, 128); put32(&mut data, 0x34, 3); put32(&mut data, 0x3C, 0);
    put32(&mut data, 0x50, 1); put32(&mut data, 512, 2);
    for (i, value) in fat.into_iter().enumerate() { put32(&mut data, 1024 + i * 4, value); }
    for (i, bytes) in clusters.into_iter().enumerate() { data[(3 + i) * 512..(4 + i) * 512].copy_from_slice(&bytes); }
    data
}

fn archive(mut modules: Vec<(&str, Vec<u8>)>) -> Vec<u8> {
    let size = (modules.len() + 2) * 16;
    modules.insert(1, ("ROMDIR", vec![0; size]));
    let mut directory = vec![0; size];
    for (i, (name, bytes)) in modules.iter().enumerate() {
        directory[i * 16..i * 16 + name.len()].copy_from_slice(name.as_bytes());
        put32(&mut directory, i * 16 + 12, bytes.len() as u32);
    }
    modules[1].1 = directory;
    let mut bytes = Vec::new();
    for (_, module) in modules { bytes.extend(module); bytes.resize(bytes.len().next_multiple_of(16), 0); }
    bytes
}

fn elf(payload: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0; 0x80]; bytes[..4].copy_from_slice(b"\x7fELF");
    put32(&mut bytes, 0x1C, 0x34); put16(&mut bytes, 0x2A, 32); put16(&mut bytes, 0x2C, 1);
    put32(&mut bytes, 0x34, 1); put32(&mut bytes, 0x38, 0x80); put32(&mut bytes, 0x3C, 0x200000);
    put32(&mut bytes, 0x44, payload.len() as u32); put32(&mut bytes, 0x48, payload.len() as u32);
    bytes.extend_from_slice(payload); bytes
}

fn bios(version: &str, words: &str, title_db: bool, illegal: bool, hdd: bool) -> Vec<u8> {
    let mut osd = words.as_bytes().to_vec();
    if version.starts_with("0200") {
        osd.resize(osd.len().next_multiple_of(4), 0);
        // Original synthetic MIPS: load the required input mask, then store it in a record.
        for word in [(9u32 << 26) | (2 << 16) | 0x090F, (43 << 26) | (3 << 21) | (2 << 16)] { osd.extend(word.to_le_bytes()); }
    }
    let mut driver = b"PSD1.0.0\0".to_vec();
    if title_db { driver.extend(b"mc0:/BADATA-SYSTEM/TITLE.DB\0"); }
    let mut modules = vec![("RESET", vec![0; 32]), ("ROMVER", version.as_bytes().to_vec()),
        ("OSDSYS", elf(&osd)), ("PS1DRV", elf(&driver)), ("EELOAD", b"rom0:TESTMODE\0".to_vec()), ("TESTMODE", vec![1])];
    if illegal { modules[4].1.extend(b"BootIllegal\0"); modules.push(("XLOADFILE", b"cdrom0:sce_dev5\0".to_vec())); }
    if hdd { modules.extend([("ATAD", vec![1]), ("HDDLOAD", vec![1]), ("HDDOSD", vec![1])]); }
    if version.starts_with("0200") { modules.push(("XDEV9", vec![1])); }
    archive(modules)
}

const WORDS: &str = "osd170.elf\0osdmain.elf\0dvdplayer.elf\0rom1:DVDID\0BootIllegal\0Initialize\0SkipMc\0SkipHdd\0BootBrowser\0BootClock\0HDDLOAD\0";
fn rom(bytes: Vec<u8>) -> RomDir { RomDir::new(bytes).unwrap() }
fn finding(map: &HookMap, hook: Hook) -> &ps2kit::hooks::HookFinding {
    map.findings.iter().find(|f| f.hook == hook).unwrap_or_else(|| panic!("missing {hook} for {:?}; found {:?}", map.romver, map.findings.iter().map(|f| f.hook).collect::<Vec<_>>()))
}
fn verdict(map: &HookMap, hook: Hook) -> Verdict { finding(map, hook).verdict }

#[test]
fn real_card_chains_update_candidates_and_region_letters() {
    let scratch = Scratch::new();
    let empty = scratch.card(&[]);
    let r = rom(bios("0160AC20990101", WORDS, true, true, true));
    assert_eq!(verdict(&HookMap::inspect(&r, None, None, None), Hook::SystemUpdate), Verdict::Unknown);
    assert_eq!(verdict(&HookMap::inspect(&r, Some(&empty), None, None), Hook::SystemUpdate), Verdict::DoesNotFire);
    let card = scratch.card(&[("BADATA-SYSTEM/history", &[7; 700]), ("BAEXEC-SYSTEM/osdmain.elf", b"synthetic executable")]);
    assert_eq!(card.read_file(&["BADATA-SYSTEM", "history"]).unwrap(), [7; 700]);
    let map = HookMap::inspect(&r, Some(&card), None, None);
    assert_eq!(verdict(&map, Hook::SystemUpdate), Verdict::Fires);
    assert!(finding(&map, Hook::SystemUpdate).because.contains("osdmain.elf opens"));
    assert_eq!(verdict(&map, Hook::HddBoot), Verdict::Unknown);
    assert_eq!(verdict(&map, Hook::SkipMc), Verdict::DoesNotFire);
    let wrong = scratch.card(&[("BEEXEC-SYSTEM/osd170.elf", b"wrong region")]);
    assert_eq!(verdict(&HookMap::inspect(&r, Some(&wrong), None, None), Hook::SystemUpdate), Verdict::DoesNotFire);
    for (version, folder) in [("0160HC20990101", "BAEXEC-SYSTEM"), ("0160JC20990101", "BIEXEC-SYSTEM"), ("0160EC20990101", "BEEXEC-SYSTEM")] {
        let r = rom(bios(version, WORDS, false, false, false));
        let path = format!("{folder}/osd170.elf");
        let card = scratch.card(&[(&path, b"candidate")]);
        assert_eq!(verdict(&HookMap::inspect(&r, Some(&card), None, None), Hook::SystemUpdate), Verdict::Fires);
    }
}

#[test]
fn corrupt_card_read_remains_unknown_instead_of_missing() {
    let scratch = Scratch::new();
    let mut bytes = card_image(&[]);
    put32(&mut bytes, 3 * 512 + 4, 0); // Bad root entry count, despite a valid superblock.
    let card = MemoryCard::open(scratch.write("damaged.ps2", bytes)).unwrap();
    let r = rom(bios("0160AC20990101", WORDS, true, true, true));
    assert_eq!(verdict(&HookMap::inspect(&r, Some(&card), None, None), Hook::SystemUpdate), Verdict::Unknown);
}

fn rom1(id: &str, suffix: &str) -> Vec<u8> {
    let id_name = format!("DVDID{suffix}");
    archive(vec![("RESET", vec![0; 32]), (&id_name, id.as_bytes().to_vec()), ("UDNL", vec![1]), ("DVDCNF", vec![1])])
}

#[test]
fn dvd_override_needs_rom1_newer_same_letter_and_allowed_display() {
    let scratch = Scratch::new();
    let path = scratch.write("bios.bin", bios("0160AC20990101", WORDS, false, true, true));
    let r = rom(std::fs::read(&path).unwrap());
    let files = [("BAEXEC-DVDPLAYER/dvdplayer.elf", b"synthetic".as_slice()),
        ("BAEXEC-DVDPLAYER/dvdplayer.id", b"3.11E\n".as_slice()), ("BAEXEC-DVDPLAYER/dvdplayer-e.ver", b"3.11\n".as_slice())];
    let card = scratch.card(&files);
    assert_eq!(verdict(&HookMap::inspect(&r, Some(&card), None, None), Hook::CardDvdPlayer), Verdict::Unknown);
    let missing = BiosSet::inspect(&path).unwrap();
    assert_eq!(verdict(&HookMap::inspect_with_bios_set(&r, Some(&card), None, None, &missing), Hook::CardDvdPlayer), Verdict::DoesNotFire);
    assert_eq!(verdict(&HookMap::inspect_with_bios_set(&r, None, None, None, &missing), Hook::CardDvdPlayer), Verdict::DoesNotFire);
    scratch.write("bios.ROM1", rom1("3.10E\n", ""));
    let set = BiosSet::inspect(&path).unwrap();
    assert_eq!(verdict(&HookMap::inspect_with_bios_set(&r, Some(&card), None, None, &set), Hook::CardDvdPlayer), Verdict::Fires);
    for (id, display) in [("3.10E\n", "3.11\n"), ("3.09E\n", "3.11\n"), ("3.11A\n", "3.11\n"), ("3.11E\n", "1.00\n"), ("3.11E\n", "1.01\n"), ("bad", "3.11\n")] {
        let card = scratch.card(&[(files[0].0, files[0].1), (files[1].0, id.as_bytes()), (files[2].0, display.as_bytes())]);
        assert_eq!(verdict(&HookMap::inspect_with_bios_set(&r, Some(&card), None, None, &set), Hook::CardDvdPlayer), Verdict::DoesNotFire, "{id}/{display}");
    }
    for omitted in [1, 2] {
        let subset: Vec<_> = files.iter().enumerate().filter(|&(i, _)| i != omitted).map(|(_, f)| *f).collect();
        let card = scratch.card(&subset);
        assert_eq!(verdict(&HookMap::inspect_with_bios_set(&r, Some(&card), None, None, &set), Hook::CardDvdPlayer), Verdict::DoesNotFire);
    }
}

#[test]
fn dvd_suffix_uses_nvm_dvd_letter_and_mec_gate_with_unsuffixed_fallback() {
    let scratch = Scratch::new();
    let path = scratch.write("bios.bin", bios("0200EC20990101", WORDS, false, true, false));
    scratch.write("bios.ROM1", archive(vec![("RESET", vec![0; 32]), ("DVDIDU", b"3.10U\n".to_vec()),
        ("DVDID", b"2.10A\n".to_vec()), ("UDNL", vec![1]), ("DVDCNF", vec![1])]));
    let mut data = [0; 1024]; data[0x180..0x187].copy_from_slice(b"EEengJU");
    scratch.write("bios.NVM", data);
    let mec_path = scratch.write("bios.MEC", [3, 6, 2, 0]);
    let r = rom(std::fs::read(&path).unwrap());
    let card = scratch.card(&[("BEEXEC-DVDPLAYER/dvdplayer.elf", b"synthetic"),
        ("BEEXEC-DVDPLAYER/dvdplayer.id", b"3.11U\n"), ("BEEXEC-DVDPLAYER/dvdplayer-e.ver", b"3.11\n")]);
    let set = BiosSet::inspect(&path).unwrap();
    assert_eq!(verdict(&HookMap::inspect_with_bios_set(&r, Some(&card), None, None, &set), Hook::CardDvdPlayer), Verdict::Fires);
    scratch.write("bios.MEC", [0, 5, 6, 0]);
    let old = BiosSet::inspect(&path).unwrap();
    assert_eq!(verdict(&HookMap::inspect_with_bios_set(&r, Some(&card), None, None, &old), Hook::CardDvdPlayer), Verdict::DoesNotFire);
    std::fs::remove_file(mec_path).unwrap();
    let unknown = BiosSet::inspect(&path).unwrap();
    assert_eq!(verdict(&HookMap::inspect_with_bios_set(&r, Some(&card), None, None, &unknown), Hook::CardDvdPlayer), Verdict::Unknown);
}

#[test]
fn early_browser_descriptor_and_dvd_have_no_rom1_gate() {
    let scratch = Scratch::new();
    let r = rom(bios("0100JC20990101", "osdsys.elf\0dvdplayer.elf\0mc0:/BIEXEC-SYSTEM/OSBROWS\0BootClock\0", false, false, false));
    let card = scratch.card(&[("BIEXEC-DVDPLAYER/dvdplayer.elf", b"player"), ("BIEXEC-SYSTEM/OSBROWS", b"101\nMBROWS\n007a0000\n"), ("BIEXEC-SYSTEM/MBROWS", b"browser")]);
    let map = HookMap::inspect(&r, Some(&card), None, None);
    assert_eq!(verdict(&map, Hook::CardDvdPlayer), Verdict::Fires);
    assert_eq!(verdict(&map, Hook::CardBrowser), Verdict::Fires);
    assert!(!map.findings.iter().any(|f| matches!(f.hook, Hook::RomDvdPlayer | Hook::FirstSetup | Hook::HiddenRemoteControl)));
    let old = scratch.card(&[("BIEXEC-SYSTEM/OSBROWS", b"100\nMBROWS\n007a0000\n"), ("BIEXEC-SYSTEM/MBROWS", b"browser")]);
    assert_eq!(verdict(&HookMap::inspect(&r, Some(&old), None, None), Hook::CardBrowser), Verdict::DoesNotFire);
    let incomplete = scratch.card(&[("BIEXEC-SYSTEM/OSBROWS", b"101\nMBROWS\n007a0000\n")]);
    assert_eq!(verdict(&HookMap::inspect(&r, Some(&incomplete), None, None), Hook::CardBrowser), Verdict::DoesNotFire);
}

fn nvm(version: &str, options: u8, initialized: bool, valid: bool) -> Nvm {
    let mut data = [0; 1024];
    let off = if version.starts_with("0200") { 0x2C0 } else { 0x310 };
    data[off + 2] = if initialized { 0x80 } else { 0 };
    data[off + 4] = options;
    data[off + 15] = data[off..off + 15].iter().fold(0u8, |sum, &v| sum.wrapping_add(v));
    if !valid { data[off + 15] ^= 1; }
    Nvm::parse(data, version).unwrap()
}

#[test]
fn hidden_gate_is_bit_seven_not_bit_five_and_bad_checksum_defaults() {
    let version = "0200EC20990101";
    let words = format!("{WORDS}XDEV9\0xfrom:XFROMBOOT\0SkipForbid\0Clear Progressive Setting\0");
    let r = rom(bios(version, &words, false, true, false));
    for (bits, hidden, ordinary, progressive) in [(0x20, Verdict::DoesNotFire, Verdict::Unknown, Verdict::DoesNotFire),
        (0x80, Verdict::Unknown, Verdict::DoesNotFire, Verdict::DoesNotFire),
        (0x40, Verdict::DoesNotFire, Verdict::DoesNotFire, Verdict::DoesNotFire),
        (0x10, Verdict::DoesNotFire, Verdict::DoesNotFire, Verdict::Fires)] {
        let n = nvm(version, bits, true, true);
        let map = HookMap::inspect(&r, None, Some(&n), None);
        assert_eq!(verdict(&map, Hook::HiddenRemoteControl), hidden);
        assert_eq!(verdict(&map, Hook::RemoteControlItem), ordinary);
        assert_eq!(verdict(&map, Hook::ClearProgressive), progressive);
        assert_eq!(verdict(&map, Hook::FirstSetup), Verdict::DoesNotFire);
        assert_eq!(verdict(&map, Hook::HddBoot), Verdict::DoesNotFire);
        assert_eq!(verdict(&map, Hook::FlashBoot), Verdict::DoesNotFire);
    }
    let n = nvm(version, 0xF0, true, false);
    let map = HookMap::inspect(&r, None, Some(&n), None);
    assert_eq!(verdict(&map, Hook::HiddenRemoteControl), Verdict::DoesNotFire);
    assert_eq!(verdict(&map, Hook::FirstSetup), Verdict::Fires);
    assert!(finding(&map, Hook::HiddenRemoteControl).because.contains("checksum rejected"));
    let mismatch = nvm("0160EC20990101", 0x80, true, true);
    let map = HookMap::inspect(&r, None, Some(&mismatch), None);
    assert_eq!(verdict(&map, Hook::HiddenRemoteControl), Verdict::Unknown);
    assert!(!map.diagnostics.is_empty());
}

fn disc(scratch: &Scratch) -> DiscImage {
    let mut image = vec![0; 24 * 2048];
    let pvd = &mut image[16 * 2048..17 * 2048];
    pvd[0] = 1; pvd[1..6].copy_from_slice(b"CD001"); pvd[6] = 1;
    put32(pvd, 80, 24); pvd[156] = 34; put32(pvd, 158, 18); put32(pvd, 166, 2048);
    let cnf = b"BOOT = cdrom:\\SLUS_123.45;1\nVER = 1.00\nPSD1.0.0 = 1,2,3\n";
    let mut offset = 18 * 2048;
    for (name, lba, size) in [("SYSTEM.CNF;1", 19u32, cnf.len() as u32), ("SLUS_123.45;1", 20, 2048)] {
        let length = (33 + name.len()).next_multiple_of(2);
        let e = &mut image[offset..offset + length]; e[0] = length as u8;
        put32(e, 2, lba); put32(e, 10, size); e[32] = name.len() as u8; e[33..33 + name.len()].copy_from_slice(name.as_bytes());
        offset += length;
    }
    image[19 * 2048..19 * 2048 + cnf.len()].copy_from_slice(cnf);
    image[20 * 2048..20 * 2048 + 8].copy_from_slice(b"PS-X EXE");
    DiscImage::open(scratch.write("disc.iso", image)).unwrap()
}

#[test]
fn ps1_overrides_match_title_and_ps2_keys_remain_live_unknowns() {
    let scratch = Scratch::new();
    let mut d = disc(&scratch);
    assert_eq!(d.kind, DiscKind::Ps1);
    let r = rom(bios("0160AC20990101", WORDS, true, true, true));
    let card = scratch.card(&[("BADATA-SYSTEM/TITLE.DB", b"SLUS_123.45 = 4,5\n")]);
    let map = HookMap::inspect(&r, Some(&card), None, Some(&d));
    assert_eq!(verdict(&map, Hook::TitleDatabase), Verdict::Fires);
    assert_eq!(verdict(&map, Hook::Ps1Parameters), Verdict::Fires);
    assert_eq!(verdict(&map, Hook::BootIllegal), Verdict::DoesNotFire);
    d.system_cnf_text = "BOOT = cdrom:\\SLUS_123.45;1\n\0PSD1.0.0 = 1\n".into();
    assert_eq!(verdict(&HookMap::inspect(&r, Some(&card), None, Some(&d)), Hook::Ps1Parameters), Verdict::DoesNotFire);
    d.system_cnf_text = " ".repeat(1023);
    assert_eq!(verdict(&HookMap::inspect(&r, Some(&card), None, Some(&d)), Hook::Ps1Parameters), Verdict::Unknown);
    let card = scratch.card(&[("BADATA-SYSTEM/TITLE.DB", b"slus_123.45 = 4,5\n")]);
    assert_eq!(verdict(&HookMap::inspect(&r, Some(&card), None, Some(&d)), Hook::TitleDatabase), Verdict::DoesNotFire);
    d.kind = DiscKind::Ps2;
    let map = HookMap::inspect(&r, Some(&card), None, Some(&d));
    assert_eq!(verdict(&map, Hook::Ps1Parameters), Verdict::DoesNotFire);
    assert_eq!(verdict(&map, Hook::BootIllegal), Verdict::Unknown);
    assert!(finding(&map, Hook::BootIllegal).because.contains("Card -x failures always yield BootError"));
    let no_producer = rom(bios("0160EC20990101", WORDS, true, false, true));
    assert_eq!(verdict(&HookMap::inspect(&no_producer, None, None, Some(&d)), Hook::BootIllegal), Verdict::DoesNotFire);
}

#[test]
fn version_alone_does_not_invent_hooks_or_panic_on_bad_modules() {
    let minimal = archive(vec![("RESET", vec![0; 32]), ("ROMVER", b"0200EC20990101".to_vec()), ("OSDSYS", elf(b"original empty program\0"))]);
    let map = HookMap::inspect(&rom(minimal), None, None, None);
    assert!(map.findings.is_empty());
    assert!(!map.diagnostics.is_empty());
    for payload in [vec![], vec![1, 2, 3], b"\x7fELF".to_vec()] {
        let r = rom(archive(vec![("RESET", vec![0; 32]), ("ROMVER", b"0200EC20990101".to_vec()), ("OSDSYS", payload)]));
        assert!(!HookMap::inspect(&r, None, None, None).diagnostics.is_empty());
    }
    let r = rom(archive(vec![("RESET", vec![0; 32]), ("ROMVER", b"broken".to_vec())]));
    assert!(HookMap::inspect(&r, None, None, None).romver.is_none());
    let r = rom(bios("0999EC20990101", WORDS, true, true, true));
    assert!(HookMap::inspect(&r, None, None, None).findings.is_empty());
}

fn local_paths(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() { local_paths(&p, out); }
        else if p.extension().is_some_and(|s| s.eq_ignore_ascii_case("bin") || s.eq_ignore_ascii_case("rom0")) { out.push(p); }
    }
}

#[test]
fn every_available_ps2_rom_has_its_own_hook_list() {
    let mut paths = Vec::new(); local_paths(&root().join("bios"), &mut paths);
    for p in paths {
        let Ok(d) = std::fs::read(&p) else { continue };
        let Ok(r) = RomDir::new(d) else { continue };
        if r.module("ROMVER").is_err() { continue }
        let map = HookMap::inspect(&r, None, None, None);
        let v = map.romver.as_ref().expect("PS2 ROMVER");
        assert_eq!(verdict(&map, Hook::SystemUpdate), Verdict::Unknown, "{}", p.display());
        assert_eq!(verdict(&map, Hook::CardDvdPlayer), Verdict::Unknown, "{}", p.display());
        assert!(map.findings.iter().all(|f| f.source.contains(".md §") && f.source.contains("0x")));
        assert_eq!(map.findings.iter().map(|f| f.hook).collect::<std::collections::HashSet<_>>().len(), map.findings.len());
        match (v.major, v.minor) {
            (1, 0) => {
                assert!(finding(&map, Hook::SystemUpdate).because.contains("BIEXEC-SYSTEM/osdsys.elf"));
                assert_eq!(verdict(&map, Hook::CardBrowser), Verdict::Unknown);
                assert!(!map.findings.iter().any(|f| matches!(f.hook, Hook::RomDvdPlayer | Hook::TestMode | Hook::FirstSetup | Hook::SkipMc)));
            }
            (1, 50) => {
                assert!(finding(&map, Hook::SystemUpdate).because.contains("osd160.elf"));
                assert_eq!(verdict(&map, Hook::HddBoot), Verdict::Unknown);
                assert!(!map.findings.iter().any(|f| matches!(f.hook, Hook::SkipMc | Hook::BootIllegal | Hook::HiddenRemoteControl)));
            }
            (1, 60) => {
                assert!(finding(&map, Hook::SystemUpdate).because.contains("osd170.elf"));
                assert_eq!(verdict(&map, Hook::HddBoot), Verdict::Unknown);
                assert_eq!(verdict(&map, Hook::BootIllegal), if v.region == 'E' { Verdict::DoesNotFire } else { Verdict::Unknown });
                if v.region == 'E' { assert_eq!(verdict(&map, Hook::LaunchRelaxations), Verdict::Fires); }
                if v.date == "20020319" {
                    assert_eq!(verdict(&map, Hook::LaunchRelaxations), Verdict::Fires);
                    assert!(finding(&map, Hook::LaunchRelaxations).because.contains("Confirmed 3 stub stores"));
                }
            }
            (2, 0) => {
                assert!(finding(&map, Hook::SystemUpdate).because.contains("osd210.elf"));
                assert_eq!(verdict(&map, Hook::HddBoot), Verdict::DoesNotFire);
                assert_eq!(verdict(&map, Hook::HiddenRemoteControl), Verdict::Unknown);
                assert_eq!(verdict(&map, Hook::HostModules), Verdict::DoesNotFire);
                assert!(!map.findings.iter().any(|f| matches!(f.hook, Hook::TitleDatabase | Hook::CardBrowser)));
            }
            _ => panic!("unresearched local ROM {}", v.text),
        }
    }
}

#[test]
fn local_70004_nvm_closes_hidden_page_but_keeps_ordinary_item() {
    let p = root().join("bios/SCPH-70004_BIOS_V12_PAL_200.BIN");
    let Ok(d) = std::fs::read(&p) else { return };
    let Ok(n) = std::fs::read(root().join("bios/SCPH-70004_BIOS_V12_PAL_200.NVM")) else { return };
    let r = rom(d); let n = Nvm::parse(n, r.module("ROMVER").unwrap()).unwrap();
    assert_eq!(n.blocks[5].offset, 0x2C0);
    assert_eq!(n.blocks[5].data[4], 0x20);
    assert_eq!(n.blocks[5].expected_checksum, 0x97);
    let set = BiosSet::inspect(&p).unwrap();
    let map = HookMap::inspect_with_bios_set(&r, None, Some(&n), None, &set);
    assert_eq!(verdict(&map, Hook::HiddenRemoteControl), Verdict::DoesNotFire);
    assert_eq!(verdict(&map, Hook::RemoteControlItem), Verdict::Unknown);
    assert_eq!(verdict(&map, Hook::FirstSetup), Verdict::DoesNotFire);
    assert_eq!(verdict(&map, Hook::ClearProgressive), Verdict::DoesNotFire);
}

#[test]
fn optional_user_card_is_read_only_and_never_copied() {
    let Some(p) = std::env::var_os("HOOKMAP_CARD") else { return };
    let Ok(_) = std::fs::metadata(&p) else { return };
    let card = MemoryCard::open(p).expect("HOOKMAP_CARD");
    for name in ["scph10000.bin", "scph39001.bin", "SCPH-70004_BIOS_V12_PAL_200.BIN"] {
        let Ok(d) = std::fs::read(root().join("bios").join(name)) else { continue };
        let r = rom(d); let map = HookMap::inspect(&r, Some(&card), None, None);
        assert!(map.diagnostics.is_empty());
        assert!(finding(&map, Hook::SystemUpdate).because.contains("SYSTEM/"));
    }
}
