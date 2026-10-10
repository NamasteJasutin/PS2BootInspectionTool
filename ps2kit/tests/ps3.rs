//! Synthetic plaintext PS3 metadata and encrypted-extent refusal; optional env-var oracles.

use ps2kit::ps3::{
    Category, FileReadability, FirmwareVerdict, FirmwareVersion, PfdHeader, Ps3Disc, Ps3HandoffStep, Ps3RegionMap,
    Ps3Save, Sfb, Sfo, SfoValue,
};
use ps2kit::pup::{Pup, SceHeader};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = PathBuf::from("/tmp/p3a-ps3-disc").join(format!(
            "test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn put(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.0.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, bytes).unwrap();
        path
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn sfo(fields: &[(&str, u16, Vec<u8>, u32)]) -> Vec<u8> {
    let mut keys = Vec::new();
    let mut data = Vec::new();
    let mut index = Vec::new();
    for (key, format, value, max) in fields {
        index.extend_from_slice(&(keys.len() as u16).to_le_bytes());
        index.extend_from_slice(&format.to_le_bytes());
        index.extend_from_slice(&(value.len() as u32).to_le_bytes());
        index.extend_from_slice(&max.to_le_bytes());
        index.extend_from_slice(&(data.len() as u32).to_le_bytes());
        keys.extend_from_slice(key.as_bytes());
        keys.push(0);
        data.extend_from_slice(value);
        data.resize(data.len() + *max as usize - value.len(), 0);
    }
    let key_offset = 20 + index.len();
    keys.resize(keys.len().div_ceil(4) * 4, 0);
    let data_offset = key_offset + keys.len();
    let mut bytes = b"\0PSF".to_vec();
    bytes.extend_from_slice(&0x101u32.to_le_bytes());
    bytes.extend_from_slice(&(key_offset as u32).to_le_bytes());
    bytes.extend_from_slice(&(data_offset as u32).to_le_bytes());
    bytes.extend_from_slice(&(fields.len() as u32).to_le_bytes());
    bytes.extend(index);
    bytes.extend(keys);
    bytes.extend(data);
    bytes
}
fn text<'a>(key: &'a str, value: &str) -> (&'a str, u16, Vec<u8>, u32) {
    let mut bytes = value.as_bytes().to_vec();
    bytes.push(0);
    let max = bytes.len().div_ceil(4) as u32 * 4;
    (key, 0x0204, bytes, max)
}
fn int(key: &str, value: u32) -> (&str, u16, Vec<u8>, u32) {
    (key, 0x0404, value.to_le_bytes().to_vec(), 4)
}
fn game_sfo() -> Vec<u8> {
    sfo(&[
        text("TITLE", "Synthetic disc"),
        text("TITLE_ID", "TEST00001"),
        text("CATEGORY", "DG"),
        text("PS3_SYSTEM_VER", "04.8200"),
        text("APP_VER", "01.03"),
        text("VERSION", "01.00"),
        int("ATTRIBUTE", 0x80000140),
        int("RESOLUTION", 0x3f),
        int("SOUND_FORMAT", 0x317),
        int("PARENTAL_LEVEL", 5),
        int("BOOTABLE", 1),
        text("LICENSE", "Synthetic licence"),
    ])
}
fn sfb() -> Vec<u8> {
    let mut bytes = vec![0; 0x600];
    bytes[..4].copy_from_slice(b".SFB");
    bytes[4..8].copy_from_slice(&0x10000u32.to_be_bytes());
    for (i, (key, value, size)) in
        [("HYBRID_FLAG", "gu", 32), ("TITLE_ID", "TEST-00001", 16), ("VERSION", "01.00", 16)].iter().enumerate()
    {
        let p = 0x20 + i * 0x20;
        let offset = match i {
            0 => 0x200,
            1 => 0x220,
            _ => 0x230,
        };
        bytes[p..p + key.len()].copy_from_slice(key.as_bytes());
        bytes[p + 16..p + 20].copy_from_slice(&(offset as u32).to_be_bytes());
        bytes[p + 20..p + 24].copy_from_slice(&(*size as u32).to_be_bytes());
        bytes[offset..offset + value.len()].copy_from_slice(value.as_bytes());
    }
    bytes
}
fn pup(version: &str) -> Vec<u8> {
    let value = format!("{version}\n");
    let mut bytes = vec![0; 0x90]; // 0x30 + 64*1 + 0x20
    bytes[..8].copy_from_slice(b"SCEUF\0\0\0");
    for (p, n) in [
        (8, 1),
        (16, 2),
        (24, 1),
        (32, 0x90),
        (40, value.len() as u64),
        (48, 0x100),
        (56, 0x90),
        (64, value.len() as u64),
    ] {
        bytes[p..p + 8].copy_from_slice(&n.to_be_bytes());
    }
    bytes.extend_from_slice(value.as_bytes());
    bytes
}
fn eboot() -> Vec<u8> {
    let mut bytes = vec![0; 4096];
    bytes[..4].copy_from_slice(b"SCE\0");
    bytes[4..8].copy_from_slice(&2u32.to_be_bytes());
    bytes[8..10].copy_from_slice(&7u16.to_be_bytes());
    bytes[10..12].copy_from_slice(&1u16.to_be_bytes());
    bytes[12..16].copy_from_slice(&0x40u32.to_be_bytes());
    bytes[16..24].copy_from_slice(&0x80u64.to_be_bytes());
    bytes[24..32].copy_from_slice(&0x800u64.to_be_bytes());
    bytes
}
fn disc_folder(dir: &Scratch) {
    dir.put("PS3_DISC.SFB", &sfb());
    dir.put("PS3_GAME/PARAM.SFO", &game_sfo());
    dir.put("PS3_GAME/ICON0.PNG", b"synthetic icon");
    dir.put("PS3_GAME/USRDIR/EBOOT.BIN", &eboot());
    dir.put("PS3_UPDATE/PS3UPDAT.PUP", &pup("4.82"));
}
fn record(name: &[u8], lba: u32, size: u32, directory: bool) -> Vec<u8> {
    let len = (33 + name.len()).div_ceil(2) * 2;
    let mut rec = vec![0; len];
    rec[0] = len as u8;
    rec[2..6].copy_from_slice(&lba.to_le_bytes());
    rec[6..10].copy_from_slice(&lba.to_be_bytes());
    rec[10..14].copy_from_slice(&size.to_le_bytes());
    rec[14..18].copy_from_slice(&size.to_be_bytes());
    rec[25] = if directory { 2 } else { 0 };
    rec[28..30].copy_from_slice(&1u16.to_le_bytes());
    rec[30..32].copy_from_slice(&1u16.to_be_bytes());
    rec[32] = name.len() as u8;
    rec[33..33 + name.len()].copy_from_slice(name);
    rec
}
fn region_table(bytes: &mut [u8], regions: &[(u32, u32)]) {
    bytes[..2048].fill(0);
    bytes[..4].copy_from_slice(&(regions.len() as u32).to_be_bytes());
    for (i, (first, last)) in regions.iter().enumerate() {
        bytes[8 + i * 8..12 + i * 8].copy_from_slice(&first.to_be_bytes());
        bytes[12 + i * 8..16 + i * 8].copy_from_slice(&last.to_be_bytes());
    }
}
fn iso(regions: &[(u32, u32)]) -> Vec<u8> {
    let mut bytes = vec![0; 64 * 2048];
    region_table(&mut bytes, regions);
    bytes[16 * 2048] = 1;
    bytes[16 * 2048 + 1..16 * 2048 + 6].copy_from_slice(b"CD001");
    bytes[16 * 2048 + 6] = 1;
    bytes[16 * 2048 + 128..16 * 2048 + 130].copy_from_slice(&2048u16.to_le_bytes());
    bytes[16 * 2048 + 130..16 * 2048 + 132].copy_from_slice(&2048u16.to_be_bytes());
    let root = record(&[0], 20, 2048, true);
    bytes[16 * 2048 + 156..16 * 2048 + 156 + root.len()].copy_from_slice(&root);
    let sfb = sfb();
    let sfo = game_sfo();
    let eboot = eboot();
    let pup = pup("4.82");
    let dirs = [
        (
            20,
            vec![
                record(b"PS3_GAME", 21, 2048, true),
                record(b"PS3_UPDATE", 23, 2048, true),
                record(b"PS3_DISC.SFB;1", 24, sfb.len() as u32, false),
            ],
        ),
        (
            21,
            vec![
                record(b"USRDIR", 22, 2048, true),
                record(b"PARAM.SFO;1", 25, sfo.len() as u32, false),
                record(b"ICON0.PNG;1", 29, 4, false),
            ],
        ),
        (22, vec![record(b"EBOOT.BIN;1", 26, eboot.len() as u32, false)]),
        (23, vec![record(b"PS3UPDAT.PUP;1", 28, pup.len() as u32, false)]),
    ];
    for (lba, records) in dirs {
        let data = records.concat();
        bytes[lba * 2048..lba * 2048 + data.len()].copy_from_slice(&data);
    }
    for (lba, data) in [(24, sfb), (25, sfo), (26, eboot), (28, pup), (29, b"icon".to_vec())] {
        bytes[lba * 2048..lba * 2048 + data.len()].copy_from_slice(&data);
    }
    bytes
}

#[test]
fn sfo_all_boot_keys_and_formats() {
    let s = Sfo::parse(&game_sfo()).unwrap();
    assert_eq!(s.title(), Some("Synthetic disc"));
    assert_eq!(s.title_id(), Some("TEST00001"));
    assert_eq!(s.category(), Some(Category::DiscGame));
    assert_eq!(s.entries.len(), 12);
    assert_eq!(s.ps3_system_ver().unwrap(), Some(FirmwareVersion::parse("4.82").unwrap()));
    assert_eq!(s.app_ver(), Some("01.03"));
    assert_eq!(s.version(), Some("01.00"));
    assert_eq!(s.parental_level(), Some(5));
    assert_eq!(s.bootable(), Some(1));
    assert_eq!(s.license(), Some("Synthetic licence"));
    let a = s.attribute_flags().unwrap();
    assert_eq!(a.unknown_bits, 0x80000040);
    assert_eq!(a.labels, vec!["Move warning"]);
    assert_eq!(s.resolution_flags().unwrap().labels.len(), 6);
    let sound = s.sound_flags().unwrap();
    assert_eq!(sound.labels.len(), 5);
    assert_eq!(sound.unknown_bits, 2);
    let key = s.entry("PS3_SYSTEM_VER").unwrap();
    assert_eq!(key.format, 0x0204);
    assert_eq!(key.length, 8);
    assert_eq!(key.max_length, 8);
    let raw = Sfo::parse(&sfo(&[
        ("PARAMS", 0x0004, vec![0xff, 0, 0x80], 4),
        ("FUTURE", 0x9999, vec![1, 2], 4),
        ("RESERVED", 0x0404, vec![], 4),
    ]))
    .unwrap();
    assert_eq!(raw.entries[0].value, SfoValue::Raw(vec![0xff, 0, 0x80]));
    assert_eq!(raw.entries[1].value, SfoValue::Raw(vec![1, 2]));
    assert_eq!(raw.entries[2].value, SfoValue::Raw(vec![]));
}
#[test]
fn categories_and_save_attribute_do_not_guess_patch_or_remote_play() {
    assert_eq!(Category::decode("GD"), Category::GameData);
    assert!(Category::decode("GD").to_string().contains("patch"));
    assert_eq!(Category::decode("??"), Category::Unknown("??".into()));
    let s = Sfo::parse(&sfo(&[text("CATEGORY", "SD"), int("ATTRIBUTE", 1)])).unwrap();
    assert_eq!(s.attribute_flags().unwrap().labels, vec!["copy protected"]);
    assert_eq!(s.ps3_system_ver().unwrap(), None);
}
#[test]
fn sfo_rejects_lengths_tables_duplicates_and_bad_text() {
    let base = game_sfo();
    for (offset, value) in [(8, 0u32), (12, 0), (16, u32::MAX), (24, u32::MAX), (28, 1), (32, u32::MAX)] {
        let mut b = base.clone();
        b[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        assert!(Sfo::parse(&b).is_err(), "offset {offset}");
    }
    assert!(Sfo::parse(&sfo(&[text("TITLE", "a"), text("TITLE", "b")])).is_err());
    assert!(Sfo::parse(&sfo(&[("TITLE", 0x0204, vec![0xff, 0], 4)])).is_err());
    assert!(Sfo::parse(&sfo(&[("TITLE", 0x0204, b"abc".to_vec(), 4)])).is_err());
    assert!(Sfo::parse(&sfo(&[("ATTRIBUTE", 0x0404, vec![0], 4)])).is_err());
    let malformed = Sfo::parse(&sfo(&[text("PS3_SYSTEM_VER", "bogus")])).unwrap();
    assert!(malformed.ps3_system_ver().is_err());
}
#[test]
fn sfb_reads_all_records_and_rejects_bad_offsets() {
    let s = Sfb::parse(&sfb()).unwrap();
    assert_eq!(s.version, 0x10000);
    assert_eq!(s.entries.len(), 3);
    assert_eq!(s.title_id(), Some("TEST-00001"));
    assert_eq!(s.hybrid_flags(), Some("gu"));
    assert_eq!(s.text("VERSION"), Some("01.00"));
    for (p, n) in [(0x30, 0u32), (0x34, u32::MAX), (0x50, 0x200)] {
        let mut b = sfb();
        b[p..p + 4].copy_from_slice(&n.to_be_bytes());
        assert!(Sfb::parse(&b).is_err());
    }
}
#[test]
fn firmware_comparison_is_decimal_and_unknown_is_not_satisfied() {
    let v = |s| Some(FirmwareVersion::parse(s).unwrap());
    assert_eq!(v("04.8200"), v("4.82"));
    assert_eq!(v("4.8"), v("4.8000"));
    assert_eq!(FirmwareVerdict::compare(v("4.82"), v("04.8200")), FirmwareVerdict::Satisfied);
    assert_eq!(FirmwareVerdict::compare(v("4.81"), v("4.82")), FirmwareVerdict::UpdateRequired);
    assert_eq!(FirmwareVerdict::compare(v("4.83"), v("4.82")), FirmwareVerdict::Satisfied);
    assert_eq!(FirmwareVerdict::compare(None, v("4.82")), FirmwareVerdict::Unknown);
    assert_eq!(FirmwareVerdict::compare(v("4.82"), None), FirmwareVerdict::Unknown);
    assert!(v("4.9") > v("4.82"));
    assert!(v("4.8201") > v("4.82"));
    assert_eq!(FirmwareVersion::parse("00.0000").unwrap().to_string(), "0.00");
    for invalid in ["", "4", "4.", ".82", "4.82000", "4.8.2", "4.8x", "-4.82", "4294967296.82"] {
        assert!(FirmwareVersion::parse(invalid).is_err(), "{invalid}");
    }
}
#[test]
fn folder_disc_handoff_is_ordered_and_cites_sources() {
    let dir = Scratch::new();
    disc_folder(&dir);
    let d = Ps3Disc::open(&dir.0).unwrap();
    assert_eq!(d.update_version, Some(FirmwareVersion::parse("4.82").unwrap()));
    let h = d.eboot_header.as_ref().unwrap();
    assert_eq!(h.key_revision, 7);
    assert_eq!(h.header_length, 128);
    assert_eq!(*h, SceHeader::parse(&eboot()).unwrap());
    assert_eq!(d.file("ps3_game/icon0.png;1").unwrap().size, Some(14));
    assert_eq!(d.file("PS3_GAME/PIC1.PNG").unwrap().readability, FileReadability::Missing);
    let steps = d.handoff(Some("4.81"));
    assert!(matches!(
        steps.as_slice(),
        [
            Ps3HandoffStep::DiscDetected { .. },
            Ps3HandoffStep::Metadata { .. },
            Ps3HandoffStep::Firmware { .. },
            Ps3HandoffStep::Update { .. },
            Ps3HandoffStep::Parental { .. },
            Ps3HandoffStep::Capabilities { .. },
            Ps3HandoffStep::Executable { .. },
            Ps3HandoffStep::Unknown
        ]
    ));
    for step in &steps {
        assert!(!step.source().is_empty());
        assert!(step.to_string().contains(step.source()));
    }
    assert!(steps[3].to_string().contains("console would offer this update"));
    assert!(steps[7].to_string().contains("BD-drive"));
    for (input, expected) in [
        (Some("4.82"), FirmwareVerdict::Satisfied),
        (Some("4.83"), FirmwareVerdict::Satisfied),
        (Some("4.81"), FirmwareVerdict::UpdateRequired),
        (None, FirmwareVerdict::Unknown),
        (Some("nonsense"), FirmwareVerdict::Unknown),
    ] {
        assert!(matches!(d.handoff(input)[2],Ps3HandoffStep::Firmware{verdict,..} if verdict==expected));
    }
}
#[test]
fn bad_optional_metadata_is_an_explicit_diagnostic() {
    let dir = Scratch::new();
    disc_folder(&dir);
    dir.put("PS3_GAME/PARAM.SFO", b"broken");
    let d = Ps3Disc::open(&dir.0).unwrap();
    assert!(d.sfo.is_none());
    assert!(matches!(d.file("PS3_GAME/PARAM.SFO").unwrap().readability, FileReadability::Unreadable(_)));
    assert!(matches!(d.handoff(Some("4.82"))[2], Ps3HandoffStep::Firmware { verdict: FirmwareVerdict::Unknown, .. }));
}
#[test]
fn disc_update_missing_corrupt_and_too_old_are_not_offered_as_sufficient() {
    let dir = Scratch::new();
    disc_folder(&dir);
    dir.put("PS3_UPDATE/PS3UPDAT.PUP", &pup("4.80"));
    let d = Ps3Disc::open(&dir.0).unwrap();
    assert!(d.handoff(Some("4.81"))[3].to_string().contains("below the game's requirement"));
    dir.put("PS3_UPDATE/PS3UPDAT.PUP", b"bad");
    let d = Ps3Disc::open(&dir.0).unwrap();
    assert!(d.update_version.is_none());
    assert!(matches!(d.file("PS3_UPDATE/PS3UPDAT.PUP").unwrap().readability, FileReadability::Unreadable(_)));
    std::fs::remove_file(dir.0.join("PS3_UPDATE/PS3UPDAT.PUP")).unwrap();
    let d = Ps3Disc::open(&dir.0).unwrap();
    assert_eq!(d.file("PS3_UPDATE/PS3UPDAT.PUP").unwrap().readability, FileReadability::Missing);
}
#[test]
fn iso_plaintext_reads_metadata_and_disc_pup() {
    let dir = Scratch::new();
    let p = dir.put("disc.iso", &iso(&[(0, 63)]));
    let d = Ps3Disc::open(p).unwrap();
    assert_eq!(d.sfo.as_ref().unwrap().title(), Some("Synthetic disc"));
    assert_eq!(d.update_version, Some(FirmwareVersion::parse("4.82").unwrap()));
    assert!(d.eboot_header.is_some());
    assert_eq!(d.read_file("PS3_GAME/ICON0.PNG").unwrap(), b"icon");
}
#[test]
fn encrypted_extents_are_refused_even_when_the_bytes_look_valid() {
    let dir = Scratch::new();
    let p = dir.put("disc.iso", &iso(&[(0, 25), (28, 63)]));
    let d = Ps3Disc::open(p).unwrap();
    assert!(d.sfo.is_some());
    assert!(d.eboot_header.is_none());
    assert_eq!(d.file("PS3_GAME/USRDIR/EBOOT.BIN").unwrap().readability, FileReadability::EncryptedRegion);
    assert!(d.read_file("PS3_GAME/USRDIR/EBOOT.BIN").unwrap_err().to_string().contains("needs the disc key, not read"));
    assert!(d.handoff(Some("4.82"))[6].to_string().contains("not read"));
    // Even a plaintext SCE prefix cannot be read if another part of the file is encrypted.
    let p = dir.put("straddles.iso", &iso(&[(0, 26), (28, 63)]));
    assert!(Ps3Disc::open(p).unwrap().eboot_header.is_none());
    // PARAM.SFO itself is valid, but its extent is marked encrypted.
    let p = dir.put("sfo-encrypted.iso", &iso(&[(0, 24), (26, 63)]));
    let d = Ps3Disc::open(p).unwrap();
    assert!(d.sfo.is_none());
    assert_eq!(d.file("PS3_GAME/PARAM.SFO").unwrap().readability, FileReadability::EncryptedRegion);
    assert!(matches!(d.handoff(Some("4.82"))[2], Ps3HandoffStep::Firmware { verdict: FirmwareVerdict::Unknown, .. }));
    let p = dir.put("pup-encrypted.iso", &iso(&[(0, 27), (29, 63)]));
    let d = Ps3Disc::open(p).unwrap();
    assert!(d.update_version.is_none());
    assert_eq!(d.file("PS3_UPDATE/PS3UPDAT.PUP").unwrap().readability, FileReadability::EncryptedRegion);
}
#[test]
fn encrypted_directory_and_pvd_are_refused_before_indexing() {
    let dir = Scratch::new();
    for regions in [&[(0, 22), (24, 63)][..], &[(0, 15), (17, 63)][..]] {
        let p = dir.put("disc.iso", &iso(regions));
        assert!(Ps3Disc::open(p).unwrap_err().to_string().contains("encrypted region, not read"));
    }
}
#[test]
fn region_boundaries_adjacent_ranges_and_invalid_tables() {
    let bytes = iso(&[(0, 25), (28, 63)]);
    let r = Ps3RegionMap::parse(&bytes[..2048]).unwrap();
    assert!(r.is_plaintext(25, 2048));
    assert!(!r.is_plaintext(25, 2049));
    assert!(!r.is_plaintext(27, 2048));
    assert!(r.is_plaintext(28, 2048));
    assert!(r.is_plaintext(63, 2048));
    assert!(!r.is_plaintext(63, 2049));
    assert!(!r.is_plaintext(u64::MAX, 2048));
    let bytes = iso(&[(0, 25), (26, 63)]);
    assert!(Ps3RegionMap::parse(&bytes[..2048]).unwrap().is_plaintext(25, 4096));
    for ranges in [&[][..], &[(1, 63)][..], &[(0, 30), (30, 63)][..], &[(0, 25), (28, 27)][..]] {
        assert!(Ps3RegionMap::parse(&iso(ranges)[..2048]).is_err());
    }
    let mut bytes = iso(&[(0, 63)]);
    bytes[..4].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(Ps3RegionMap::parse(&bytes[..2048]).is_err());
}
#[test]
fn absent_region_map_reports_unknown_and_udf_only_is_explicit() {
    let dir = Scratch::new();
    let p = dir.put("no-map.iso", &iso(&[]));
    let d = Ps3Disc::open(p).unwrap();
    assert!(d.sfo.is_none());
    assert!(d.eboot_header.is_none());
    assert_eq!(d.file("PS3_GAME/PARAM.SFO").unwrap().readability, FileReadability::UnknownRegionMap);
    assert!(d.read_file("PS3_GAME/PARAM.SFO").is_err());
    let mut bytes = iso(&[(0, 63)]);
    bytes[16 * 2048 + 1..16 * 2048 + 6].copy_from_slice(b"BEA01");
    let p = dir.put("udf.iso", &bytes);
    assert!(Ps3Disc::open(p).unwrap_err().to_string().contains("UDF-only"));
}
#[test]
fn save_reads_only_sfo_pfd_prefix_and_file_sizes() {
    let dir = Scratch::new();
    dir.put(
        "PARAM.SFO",
        &sfo(&[
            text("CATEGORY", "SD"),
            text("TITLE", "Synthetic save"),
            text("SUB_TITLE", "Slot 1"),
            text("DETAIL", "A synthetic record"),
            text("SAVEDATA_DIRECTORY", "TEST00001_SLOT1"),
            text("SAVEDATA_LIST_PARAM", "SLOT1"),
            int("ATTRIBUTE", 1),
        ]),
    );
    dir.put("ICON0.PNG", b"icon");
    let mut pfd = vec![0x5a; 256];
    pfd[..8].copy_from_slice(b"\0\0\0\0PFDB");
    pfd[8..16].copy_from_slice(&3u64.to_be_bytes());
    dir.put("PARAM.PFD", &pfd);
    dir.put("DATA.BIN", &[0xff; 32]);
    dir.put("sub/record", b"opaque");
    let s = Ps3Save::open(&dir.0).unwrap();
    assert_eq!(s.icon_size, Some(4));
    assert_eq!(s.pfd_size, Some(256));
    assert_eq!(s.pfd.unwrap().version, 3);
    assert_eq!(s.data_files.len(), 2);
    assert_eq!(s.data_files[0].size, 32);
    assert_eq!(s.sfo.sub_title(), Some("Slot 1"));
    assert_eq!(s.sfo.detail(), Some("A synthetic record"));
    assert_eq!(s.sfo.savedata_directory(), Some("TEST00001_SLOT1"));
    assert_eq!(s.sfo.savedata_list_param(), Some("SLOT1"));
    assert!(PfdHeader::parse(&pfd[..16]).is_err());
    dir.put("PARAM.PFD", b"bad");
    assert!(Ps3Save::open(&dir.0).is_err());
    std::fs::remove_file(dir.0.join("PARAM.PFD")).unwrap();
    std::fs::remove_file(dir.0.join("ICON0.PNG")).unwrap();
    let s = Ps3Save::open(&dir.0).unwrap();
    assert!(s.pfd.is_none());
    assert!(s.icon_size.is_none());
}
#[test]
fn malformed_parsers_never_panic_under_mutation() {
    for original in [game_sfo(), sfb(), eboot(), iso(&[(0, 25), (28, 63)])[..2048].to_vec()] {
        for length in 0..original.len().min(2049) {
            let bytes = &original[..length];
            let _ = Sfo::parse(bytes);
            let _ = Sfb::parse(bytes);
            let _ = PfdHeader::parse(bytes);
            let _ = Ps3RegionMap::parse(bytes);
            let _ = SceHeader::parse(bytes);
        }
        for offset in 0..original.len().min(2048) {
            for value in [0, 1, 0x7f, 0xff] {
                let mut bytes = original.clone();
                bytes[offset] = value;
                let _ = Sfo::parse(&bytes);
                let _ = Sfb::parse(&bytes);
                let _ = PfdHeader::parse(&bytes);
                let _ = Ps3RegionMap::parse(&bytes);
                let _ = SceHeader::parse(&bytes);
            }
        }
    }
}
#[test]
fn malformed_iso_records_and_mutations_never_panic() {
    let dir = Scratch::new();
    for offset in [
        16 * 2048 + 128,
        16 * 2048 + 158,
        16 * 2048 + 166,
        20 * 2048,
        20 * 2048 + 25,
        20 * 2048 + 32,
        21 * 2048 + 2,
        21 * 2048 + 10,
    ] {
        for value in [0, 1, 0x7f, 0xff] {
            let mut bytes = iso(&[(0, 25), (28, 63)]);
            bytes[offset] = value;
            let p = dir.put("fuzz.iso", &bytes);
            let _ = Ps3Disc::open(p);
        }
    }
    let mut bytes = iso(&[(0, 63)]);
    bytes[20 * 2048] = 2;
    let p = dir.put("bad.iso", &bytes);
    assert!(Ps3Disc::open(p).is_err());
}
#[test]
fn cli_disc_save_and_argument_validation() {
    let dir = Scratch::new();
    disc_folder(&dir);
    let run = |args: &[&str]| {
        std::process::Command::new(env!("CARGO_BIN_EXE_ps3card")).arg(&dir.0).args(args).output().unwrap()
    };
    let out = run(&["--firmware", "4.81"]);
    assert!(out.status.success());
    let report = String::from_utf8(out.stdout).unwrap();
    assert!(report.contains("console would offer this update"));
    let p = dir.put("console.pup", &pup("4.83"));
    let out = run(&["--pup", p.to_str().unwrap()]);
    assert!(out.status.success());
    assert!(String::from_utf8(out.stdout).unwrap().contains("firmware gate permits boot"));
    assert!(!run(&["--firmware"]).status.success());
    assert!(!run(&["--firmware", "bad"]).status.success());
    assert!(!run(&["--firmware", "4.82", "--pup", "anything"]).status.success());
    let save = Scratch::new();
    save.put("PARAM.SFO", &sfo(&[text("CATEGORY", "SD"), text("TITLE", "Save")]));
    save.put("opaque", b"data");
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_ps3card")).arg(&save.0).output().unwrap();
    assert!(out.status.success());
    assert!(String::from_utf8(out.stdout).unwrap().contains("opaque: 4 bytes (data not read)"));
}

#[test]
fn real_disc_from_env() {
    let Some(path) = std::env::var_os("PS3_DISC") else {
        return;
    };
    let d = Ps3Disc::open(PathBuf::from(path)).expect("PS3_DISC must name a supported ISO 9660 image or disc folder");
    assert!(d.files.iter().any(|f| f.path == "PS3_GAME/PARAM.SFO"));
    assert_eq!(d.handoff(None).len(), 8);
    if let Some(s) = &d.sfo {
        assert!(s.title().is_some());
        assert!(s.title_id().is_some());
    }
    for f in &d.files {
        if f.readability == FileReadability::EncryptedRegion {
            assert!(d.read_file(&f.path).is_err());
        }
    }
}
#[test]
fn real_save_from_env() {
    let Some(path) = std::env::var_os("PS3_SAVE") else {
        return;
    };
    let s = Ps3Save::open(PathBuf::from(path)).expect("PS3_SAVE must name a save directory");
    assert!(s.sfo.title().is_some());
    assert!(s.sfo.savedata_directory().is_some());
}
#[test]
fn user_pup_firmware_and_shared_sce_prefix() {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("bios/PS3UPDAT.PUP");
    if !p.is_file() {
        return;
    }
    let pup = Pup::open(p).unwrap();
    assert_eq!(FirmwareVersion::parse(&pup.version().unwrap()).unwrap(), FirmwareVersion::parse("4.82").unwrap());
    for id in [0x200, 0x601] {
        let prefix = pup.read_member_range(id, 0, 32).unwrap();
        assert_eq!(SceHeader::parse(&prefix).unwrap(), pup.self_info(id).unwrap().sce_header);
    }
}
