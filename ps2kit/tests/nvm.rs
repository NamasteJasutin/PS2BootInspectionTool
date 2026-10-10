//! Synthetic checks and silent-when-absent checks of the user's own BIOS sets.

use ps2kit::nvm::{BiosSet, BiosSetPaths, EromInfo, IdState, Mec, Nvm, NvmSource, RomVersion};
use std::path::{Path, PathBuf};

fn root() -> PathBuf { Path::new(env!("CARGO_MANIFEST_DIR")).join("..") }
fn checksum(b: &mut [u8], offset: usize) {
    b[offset + 15] = b[offset..offset + 15].iter().fold(0u8, |s, &v| s.wrapping_add(v));
}

#[test]
fn full_byte_checksum_and_settings() {
    let mut b = [0u8; 1024];
    b[0x300] = 0x91; // Two option nibbles, including the ignored fourth bit.
    checksum(&mut b, 0x300);
    let o = 0x310;
    b[o] = 0x20 | 0x10 | 5 | 8;
    b[o + 1] = 1;
    b[o + 2] = 0x80 | 8 | 0x10 | 0x20 | 6;
    b[o + 3] = 0xD4; // Signed 11-bit -300 minutes.
    b[o + 4] = 1 | 0x10 | 0x20 | 0x40 | 0x80;
    b[o + 5] = 0xFF;
    checksum(&mut b, o);
    let n = Nvm::parse(b, "0160EC20011004").unwrap();
    assert!(n.blocks.iter().all(|b| b.valid()));
    assert!(!n.osd_rejected);
    assert!(n.ps1_fast && n.ps1_smooth);
    assert_eq!(&n.option_nibbles[..2], &[1, 1]);
    let s = &n.osd;
    assert_eq!(s.language_name(), "English");
    assert_eq!(s.screen_name(), "16:9");
    assert!(!s.digital_out && s.component_video);
    assert_eq!(s.timezone_minutes, -300);
    assert_eq!(s.timezone_index, 511);
    assert!(s.summer_time && s.progressive && s.remote_item && s.remote_enabled && s.unknown_option);
    assert_eq!((s.time_format, s.date_format), (1, 1));
    assert!(!s.setup_required());
}

#[test]
fn pcsx2_seven_bit_checksum_rejected_and_defaulted() {
    let mut b = [0; 1024];
    b[0x310] = 0x30;
    b[0x311] = 0x21;
    b[0x315] = 0x70;
    checksum(&mut b, 0x310);
    assert_eq!(b[0x31F], 0xC1);
    b[0x31F] &= 0x7F;
    let n = Nvm::parse(b, "0160AC20020207").unwrap();
    assert!(n.osd_rejected && n.blocks[5].has_seven_bit_checksum());
    assert_eq!(n.stored_osd.language_name(), "English");
    assert_eq!(n.osd.language, None);
    assert!(n.osd.setup_required());
    assert!(n.eeconf_cache().is_none());
    assert!(n.predictions().iter().any(|p| p.message.contains("retry yields zeros")));
    checksum(&mut b, 0x310);
    assert!(!Nvm::parse(b, "0160AC20020207").unwrap().osd_rejected);
    b[0x31F] = b[0x31F].wrapping_add(1); // An unrelated checksum error also rejects the read.
    let n = Nvm::parse(b, "0160AC20020207").unwrap();
    assert!(n.osd_rejected && !n.blocks[5].has_seven_bit_checksum());
}

#[test]
fn checksum_wraps_and_other_areas_are_checked() {
    let mut b = [0; 1024];
    for o in [0x200, 0x280, 0x310] {
        b[o] = 0xFF;
        b[o + 1] = 0xFF;
        checksum(&mut b, o);
        assert_eq!(b[o + 15], 0xFE);
    }
    let n = Nvm::parse(b, "0160EC20011004").unwrap();
    assert!(n.blocks.iter().all(|b| b.valid()));
    b[0x20F] ^= 1;
    b[0x28F] ^= 1;
    let n = Nvm::parse(b, "0160EC20011004").unwrap();
    assert_eq!(n.blocks.iter().filter(|b| !b.valid()).count(), 2);
    assert!(!n.osd_rejected); // Area-2 corruption does not reject the independent OSD read.
    assert!(n.eeconf_cache().is_none());
}

#[test]
fn blank_and_early_rom_have_distinct_wizard_behaviour() {
    let b = [0; 1024];
    let early = Nvm::parse(b, "0100JC20000117").unwrap();
    assert_eq!(early.source, NvmSource::Blank);
    assert_eq!(early.osd.language, Some(0));
    assert!(!early.osd.setup_required());
    assert_eq!(early.testmode(Some(0x10)), Some(false));
    assert!(early.eeconf_cache().is_none());
    let late = Nvm::parse(b, "0200EC20040614").unwrap();
    assert!(late.osd.setup_required());
    assert_eq!(late.testmode(None), None);
    assert_eq!(late.testmode(Some(0x10)), Some(true));
    assert_eq!(late.testmode(Some(0x08)), Some(false));
    assert_eq!(late.console_id.state, IdState::Blank);
    let erased = Nvm::parse([0xFF; 1024], "0200EC20040614").unwrap();
    assert_eq!(erased.source, NvmSource::Blank);
    assert_eq!(erased.console_id.state, IdState::Blank);
    assert!(erased.blocks.iter().all(|b| !b.valid()));
}

#[test]
fn decimal_layout_threshold_and_region_command_gates() {
    for (v, expected) in [("0169EC20010101", 0), ("0170EC20010101", 1), ("0200EC20040614", 1)] {
        assert_eq!(Nvm::parse([0; 1024], v).unwrap().layout.number, expected);
    }
    let mut b = [0; 1024];
    // Deliberately use distinct ROM, PS1 and DVD letters to catch an incorrect selector.
    b[0x180..0x187].copy_from_slice(b"AEengJU");
    let n = Nvm::parse(b, "0200AC20040101").unwrap();
    let p = n.region_params.as_ref().unwrap();
    assert_eq!((p.rom_region, p.ps1_region, p.dvd_region), ('A', 'J', 'U'));
    let default = Mec::parse([3, 6, 2, 0]).unwrap();
    let predictions = n.predictions_with_mec(Some(&default));
    assert!(predictions.iter().any(|p| p.message.contains("PS1VERA") && p.message.contains("DVDIDU")));
    let old = Mec::parse([0, 5, 6, 0]).unwrap();
    let predictions = n.predictions_with_mec(Some(&old));
    assert!(!predictions.iter().any(|p| p.message.contains("DVDIDU")));
    assert!(predictions.iter().any(|p| p.message.contains("closes 0x36")));
    assert!(predictions.iter().all(|p| !p.citation.is_empty()));
    b[0x180] = 0x14;
    let n = Nvm::parse(b, "0200EC20040614").unwrap();
    assert!(n.region_params.is_none() && n.unknown_region_params);
    b[0x1B0..0x1B5].copy_from_slice(b"TEST\0");
    let n = Nvm::parse(b, "0200EC20040614").unwrap();
    let old = Mec::parse([0, 1, 4, 0]).unwrap();
    assert!(n.predictions_with_mec(Some(&old)).iter().any(|p| p.message.contains("Console: Unknown") && p.message.contains("blocks the model read")));
}

#[test]
fn identifiers_are_masked_even_in_debug_and_defaults_are_heuristic() {
    let mut b = [0; 1024];
    let id = [0x13, 0x57, 0x24, 0x68, 0xAB, 0xCD, 0x12, 0x34];
    b[0x1F0..0x1F8].copy_from_slice(&id);
    let fabricated = Nvm::parse(b, "0200EC20040614").unwrap();
    assert_eq!(fabricated.console_id.state, IdState::Fabricated);
    assert_eq!(fabricated.console_id.masked, "****************");
    b[0] = 1; // Calibration occupancy supports a populated dump, not proof of authenticity.
    let n = Nvm::parse(b, "0200EC20040614").unwrap();
    assert_eq!(n.console_id.state, IdState::Present);
    assert_eq!(n.console_id.masked, "************1234");
    let debug = format!("{n:?}");
    assert!(!debug.contains("13572468ABCD1234"));
    assert!(!debug.contains("19, 87, 36, 104"));
    // A real dump may share an emulator-default i.LINK ID shape.
    b[0x1E0..0x1E8].copy_from_slice(&[0, 1, 0xFF, 0xFF, 0xFF, 0xFF, 2, 3]);
    assert_eq!(Nvm::parse(b, "0200EC20040614").unwrap().ilink_id.state, IdState::Present);
}

#[test]
fn eeconf_cache_packs_fifteen_byte_records_and_mirrors_video_bit() {
    let mut b = [0; 1024];
    b[0x290] = 0x40;
    checksum(&mut b, 0x290);
    b[0x310] = 8;
    checksum(&mut b, 0x310);
    let n = Nvm::parse(b, "0160EC20011004").unwrap();
    let cache = n.eeconf_cache().unwrap();
    assert_eq!(cache.len(), 0x60);
    assert_eq!(cache[0x0F], 0x48);
    assert_eq!(cache[0x4B], 8);
    assert_eq!(&cache[0x5A..], &[0; 6]);
    assert_eq!(n.blocks[1].data[0], 0x40); // Mirror changes the cache, not the EEPROM.
}

#[test]
fn mec_all_version_gates_and_boundaries() {
    let default = Mec::parse([3, 6, 2, 0]).unwrap();
    assert!(default.pcsx2_default);
    assert_eq!(default.version_string(), "6.02.00");
    assert_eq!(default.version_number(), 0x060200);
    assert_eq!(default.gates().len(), 10);
    assert!(default.gates().iter().all(|g| g.enabled));
    for gate in default.gates() {
        let [a, b, c] = gate.minimum;
        assert!(Mec::parse([0, a, b, c]).unwrap().gates().iter().find(|g| g.minimum == gate.minimum).unwrap().enabled);
        let prior = if b > 0 { [0, a, b - 1, 0xFF] } else { [0, a - 1, 0xFF, 0xFF] };
        assert!(!Mec::parse(prior).unwrap().gates().iter().find(|g| g.minimum == gate.minimum).unwrap().enabled);
    }
}

#[test]
fn malformed_nvm_romver_and_mec_never_panic() {
    for len in [0, 1, 15, 16, 512, 1023, 1025, 2048] {
        assert!(Nvm::parse(vec![0xFF; len], "0200EC20040614").is_err());
    }
    for romver in ["", "0200", "abcdEC20040614", "0200E020040614", "0200EC2004xxxx", "é200EC20040614"] {
        assert!(RomVersion::parse(romver).is_err());
        assert!(Nvm::parse([0; 1024], romver).is_err());
    }
    for len in [0, 1, 3, 5, 1024] { assert!(Mec::parse(vec![0; len]).is_err()); }
    let mut seed = 1u32;
    for _ in 0..128 {
        let mut b = [0; 1024];
        for v in &mut b { seed = seed.wrapping_mul(1664525).wrapping_add(1013904223); *v = (seed >> 24) as u8; }
        for version in ["0100JC20000117", "0150AD20001228", "0200EC20040614"] {
            let n = Nvm::parse(b, version).unwrap();
            let _ = (n.predictions(), n.eeconf_cache(), format!("{n:?}"));
        }
    }
}

fn fake_gzip() -> Vec<u8> {
    // A synthetic empty stored DEFLATE block and zero CRC/ISIZE trailer. No Sony data.
    vec![0x1F, 0x8B, 8, 0, 0, 0, 0, 0, 0, 3, 1, 0, 0, 0xFF, 0xFF, 0, 0, 0, 0, 0, 0, 0, 0]
}

#[test]
fn erom_scan_checks_headers_without_claiming_inflation_or_crc() {
    let mut b = vec![0x42; 17];
    b.extend(fake_gzip());
    b.extend([0x42; 11]);
    b.extend(fake_gzip());
    b.extend([0x1F, 0x8B, 8]);
    let e = EromInfo::inspect(&b);
    assert_eq!(e.gzip_headers.iter().map(|h| h.offset).collect::<Vec<_>>(), [17, 51]);
    assert_eq!(e.malformed_headers, 1);
    assert!(!e.blank);
    let mut reserved = fake_gzip();
    reserved[3] = 0x20;
    assert_eq!(EromInfo::inspect(&reserved).malformed_headers, 1);
    let mut bad_deflate = fake_gzip();
    bad_deflate[10] = 7;
    assert!(EromInfo::inspect(&bad_deflate).gzip_headers.is_empty());
    let mut extra = fake_gzip();
    extra[3] = 4;
    extra.splice(10..10, [3, 0, 1, 2, 3]);
    assert_eq!(EromInfo::inspect(&extra).gzip_headers[0].header_size, 15);
    let mut truncated_extra = fake_gzip();
    truncated_extra[3] = 4;
    truncated_extra[10..12].copy_from_slice(&[0xFF; 2]);
    assert!(EromInfo::inspect(&truncated_extra).gzip_headers.is_empty());
    for len in 0..fake_gzip().len() { let _ = EromInfo::inspect(&fake_gzip()[..len]); }
}

struct TempDir(PathBuf);
impl TempDir {
    fn new() -> Self {
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("ps2kit-nvm-{}-{stamp}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn put(&self, name: &str, bytes: impl AsRef<[u8]>) -> PathBuf {
        let path = self.0.join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }
}
impl Drop for TempDir { fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); } }

fn fake_rom(mut modules: Vec<(&str, Vec<u8>)>) -> Vec<u8> {
    let directory_size = (modules.len() + 2) * 16;
    modules.insert(1, ("ROMDIR", vec![0; directory_size]));
    let mut table = vec![0; directory_size];
    for (i, (name, bytes)) in modules.iter().enumerate() {
        table[i * 16..i * 16 + name.len()].copy_from_slice(name.as_bytes());
        table[i * 16 + 12..i * 16 + 16].copy_from_slice(&(bytes.len() as u32).to_le_bytes());
    }
    modules[1].1 = table;
    let mut out = Vec::new();
    for (_, bytes) in modules { out.extend(bytes); out.resize(out.len().next_multiple_of(16), 0); }
    out
}
fn fake_bios(version: &str) -> Vec<u8> {
    fake_rom(vec![("RESET", vec![0; 32]), ("ROMVER", version.as_bytes().to_vec())])
}
fn fake_rom1(suffix: &str) -> Vec<u8> {
    let id_name = format!("DVDID{suffix}");
    let ver_name = format!("DVDVER{suffix}");
    let driver_name = format!("EROMDRV{suffix}");
    fake_rom(vec![("RESET", b"20990102-030405\0".to_vec()), (&id_name, b"9.99E\n".to_vec()),
        (&ver_name, b"9.99Z\n".to_vec()), (&driver_name, vec![1]), ("UDNL", vec![1]), ("DVDCNF", vec![1])])
}

#[test]
fn discovery_full_name_precedence_case_and_set_findings() {
    let t = TempDir::new();
    let bios = t.put("Example.BIN", fake_bios("0200EC20990101"));
    let mut b = [0; 1024];
    b[0x180..0x187].copy_from_slice(b"EEengEE");
    t.put("eXAMPLE.nVm", b);
    let full = t.put("EXAMPLE.bin.NvM", b);
    t.put("EXAMPLE.MeC", [3, 6, 2, 0]);
    let r = fake_rom1("E");
    t.put("example.rOm1", &r);
    t.put("example.ROM2", &r);
    t.put("example.eRoM", fake_gzip());
    t.put("rom1.bin", fake_rom1("O")); // A stray file is not a named companion.
    let set = BiosSet::inspect(&bios).unwrap();
    assert_eq!(set.paths.nvm, Some(full));
    assert_eq!(set.rom2_mirrors_rom1, Some(true));
    let dvd = &set.rom1.as_ref().unwrap().dvd_versions[0];
    assert_eq!(dvd.id.as_deref(), Some("9.99E"));
    assert_eq!(dvd.display.as_deref(), Some("9.99Z"));
    assert!(set.findings.iter().any(|p| p.message.contains("differ")));
    assert!(set.findings.iter().any(|p| p.message.contains("selected set complete")));
    assert_eq!(BiosSet::inspect(&t.0).unwrap().paths.bios, bios);
    t.put("other.bin", fake_bios("0160EC20990101"));
    assert!(BiosSet::inspect(&t.0).is_err());
}

#[test]
fn missing_and_malformed_companions_keep_report_reviewable() {
    let t = TempDir::new();
    let bios = t.put("test.bin", fake_bios("0200EC20990101"));
    t.put("test.NVM", [1; 5]);
    t.put("test.MEC", [1; 3]);
    t.put("test.ROM1", [0; 32]);
    t.put("test.EROM", [0; 128]);
    let set = BiosSet::inspect(&bios).unwrap();
    assert!(set.nvm.is_none() && set.mec.is_none() && set.rom1.is_none());
    assert!(set.erom.as_ref().unwrap().blank);
    assert!(set.findings.iter().filter(|p| p.message.contains("malformed companion")).count() >= 3);
    assert!(set.findings.iter().any(|p| p.message.contains("cannot accept a memory-card")));
    let mut paths = BiosSetPaths::new(bios);
    paths.nvm = Some(t.put("arbitrary-eeprom", [0; 1024]));
    assert!(BiosSet::inspect_paths(paths).unwrap().nvm.is_some());
}

#[test]
fn suffix_mismatch_and_chinese_missing_fonts_are_reported() {
    let t = TempDir::new();
    let bios = t.put("china.bin", fake_bios("0200CC20990101"));
    let mut n = [0; 1024];
    n[0x180..0x187].copy_from_slice(b"CCschJC");
    t.put("china.NVM", n);
    t.put("china.MEC", [3, 6, 2, 0]);
    t.put("china.ROM1", fake_rom1("E"));
    let set = BiosSet::inspect(bios).unwrap();
    assert!(set.findings.iter().any(|p| p.message.contains("0x186 (C)")));
    assert!(set.findings.iter().any(|p| p.message.contains("Chinese OSD path is incomplete")));
}

#[test]
fn local_70004_set() {
    let dir = root().join("bios");
    let path = dir.join("SCPH-70004_BIOS_V12_PAL_200.BIN");
    let Ok(_) = std::fs::read(&path) else { return };
    let Ok(_) = std::fs::read(dir.join("SCPH-70004_BIOS_V12_PAL_200.NVM")) else { return };
    let set = BiosSet::inspect(path).unwrap();
    let n = set.nvm.as_ref().unwrap();
    assert_eq!(n.layout.number, 1);
    assert_eq!(n.source, NvmSource::Populated);
    assert_eq!(n.model.as_deref(), Some("SCPH-70004"));
    assert_eq!(n.region_params.as_ref().unwrap().text, "EEengEE");
    assert_eq!(n.osd.timezone_minutes, 60);
    assert_eq!(n.osd.timezone_index, 37);
    assert_eq!(n.osd.language, Some(1));
    assert_eq!(n.osd.screen_type, 2);
    assert!(!n.osd.digital_out && !n.osd.component_video);
    assert!(n.osd.remote_item && !n.osd.setup_required());
    assert_eq!(n.blocks[5].expected_checksum, 0x97);
    assert!(n.blocks.iter().all(|b| b.valid()));
    assert_eq!(n.blocks.iter().filter(|b| b.area == 2 && b.data.iter().any(|&v| v != 0)).count(), 4);
    if let Some(r) = &set.rom1 {
        assert_eq!(r.stamp.as_deref(), Some("20040729-164054"));
        assert_eq!(r.dvd_versions.len(), 8);
        assert!(set.findings.iter().any(|p| p.message.contains("differ")));
    }
    if set.paths.rom2.is_some() && set.paths.rom1.is_some() { assert_eq!(set.rom2_mirrors_rom1, Some(true)); }
    if let Some(e) = &set.erom {
        assert_eq!(e.size, 3 * 1024 * 1024);
        assert_eq!(e.gzip_headers.len(), 40);
        assert_eq!(e.malformed_headers, 0);
        assert_eq!(e.gzip_headers.first().unwrap().offset, 0x44EB4);
        assert_eq!(e.gzip_headers.last().unwrap().offset, 0x2FFC4C);
    }
    if let Some(m) = &set.mec { assert!(m.pcsx2_default); }
}

#[test]
fn local_pcsx2_sets() {
    let dir = root().join("bios");
    for (stem, offset, index, sum, wizard) in [("scph39001", 270, 0, 0xE0, false),
        ("PS2 Bios 30004R V6 Pal", -300, 20, 0xBF, false), ("scph10000", 0, 0, 0, false)] {
        let path = dir.join(format!("{stem}.bin"));
        let Ok(_) = std::fs::read(&path) else { continue };
        let Ok(_) = std::fs::read(dir.join(format!("{stem}.NVM"))) else { continue };
        let set = BiosSet::inspect(path).unwrap();
        let n = set.nvm.as_ref().unwrap();
        assert_eq!(n.layout.number, 0);
        assert_eq!(n.source, if stem == "scph10000" { NvmSource::Blank } else { NvmSource::Fabricated });
        assert_eq!(n.osd.timezone_minutes, offset);
        assert_eq!(n.osd.timezone_index, index);
        assert_eq!(n.blocks[5].expected_checksum, sum);
        assert_eq!(n.osd.setup_required(), wizard);
        assert!(n.blocks.iter().all(|b| b.valid()));
        assert_eq!(n.console_id.state, IdState::Blank);
        assert_eq!(n.ilink_id.state, IdState::Blank);
        assert!(n.model.is_none());
        if let Some(m) = &set.mec { assert!(m.pcsx2_default); }
    }
}

#[test]
fn local_devkit_explicit_paths() {
    let dir = root().join("bios/DTL-H30101");
    let bios = dir.join("DTL-H30101_USA_Dev_0150_20001228_v4_[CC645DA1].rom0");
    let nvm = dir.join("DTL-H30101_USA_Dev_0150_20001228_v4_[6A001573].nvm");
    let Ok(_) = std::fs::read(&bios) else { return };
    let Ok(_) = std::fs::read(&nvm) else { return };
    let mut paths = BiosSetPaths::new(bios);
    paths.nvm = Some(nvm);
    let set = BiosSet::inspect_paths(paths).unwrap();
    let n = set.nvm.as_ref().unwrap();
    assert_eq!(n.source, NvmSource::Populated);
    assert_eq!(n.model.as_deref(), Some("DTL-H30101"));
    assert_eq!(n.osd.timezone_minutes, -480);
    assert_eq!(n.osd.timezone_index, 115);
    assert!(n.osd.component_video && n.osd.summer_time);
    assert_eq!(n.osd.date_format, 1);
    assert_eq!(n.blocks[5].expected_checksum, 0x9F);
    assert!(n.blocks.iter().all(|b| b.valid()));
    assert!(n.region_params.is_none() && n.unknown_region_params);
    assert_eq!(n.console_id.state, IdState::Present);
}
