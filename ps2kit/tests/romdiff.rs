//! Synthetic comparisons and private dump oracles; absent BIOS files skip silently.
use ps2kit::ps1bios::{KernelGeneration, Ps1Bios};
use ps2kit::rom::RomDir;
use ps2kit::romdiff::{cluster_label, ChangeKind, RomSet, Space};
use std::path::{Path, PathBuf};

fn root() -> PathBuf { Path::new(env!("CARGO_MANIFEST_DIR")).join("..") }
fn read(name: &str) -> Option<Vec<u8>> { std::fs::read(root().join("bios").join(name)).ok() }
fn word(d: &mut [u8], p: usize, w: u32) { d[p..p + 4].copy_from_slice(&w.to_le_bytes()); }

// Entirely invented module contents and metadata. Alignment placeholders have EXTINFO
// too, proving that the parser consumes records in physical ROMDIR order.
fn tiny(version: &str, modules: &[(&str, &[u8])], extinfo: bool) -> RomDir {
    let mut items = vec![("RESET", vec![0; 16], 0u16), ("ROMDIR", Vec::new(), 0), ("EXTINFO", Vec::new(), 0),
        ("ROMVER", version.as_bytes().to_vec(), 0), ("-", vec![0; 16], if extinfo { 4 } else { 0 })];
    items.extend(modules.iter().map(|&(n, d)| (n, d.to_vec(), if extinfo { 24 } else { 0 })));
    let directory_size = (items.len() + 1) * 16;
    items[1].1 = vec![0; directory_size];
    let mut ext = Vec::new();
    if extinfo {
        ext.extend_from_slice(&[0, 0, 0, 0x7F]);
        for _ in modules {
            ext.extend_from_slice(&[0x07, 0x02, 0, 2]);
            ext.extend_from_slice(&[0, 0, 4, 1, 0x29, 0x02, 0x20, 0x20]);
            ext.extend_from_slice(&[0, 0, 8, 3]);
            ext.extend_from_slice(b"invented");
        }
    }
    items[2].1 = ext;
    let mut directory = vec![0; directory_size];
    for (i, (name, data, ext)) in items.iter().enumerate() {
        directory[i * 16..i * 16 + name.len()].copy_from_slice(name.as_bytes());
        directory[i * 16 + 10..i * 16 + 12].copy_from_slice(&ext.to_le_bytes());
        word(&mut directory, i * 16 + 12, data.len() as u32);
    }
    items[1].1 = directory;
    let mut dump = Vec::new();
    for (_, data, _) in items { dump.extend_from_slice(&data); dump.resize((dump.len() + 15) & !15, 0); }
    RomDir::new(dump).expect("invented archive")
}

#[test]
fn clusters_presence_sizes_metadata_and_date_order() {
    let a = tiny("0100JC20000117", &[("SAME", b"abc"), ("GONE", b"old"), ("RESIZE", b"one")], true);
    let b = tiny("0200EC20040614", &[("SAME", b"abc"), ("NEW", b"new"), ("RESIZE", b"longer")], true);
    let set = RomSet::from_ps2(&[("later", b), ("earlier", a)]).unwrap();
    assert_eq!(set.dumps.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(), ["earlier", "later"]);
    let row = set.row("SAME").unwrap();
    let (a, b) = (row.cells[0].as_ref().unwrap(), row.cells[1].as_ref().unwrap());
    assert_eq!((a.cluster, b.cluster, a.size), (0, 0, 3));
    assert_eq!(a.extinfo.version, Some(0x0207));
    assert_eq!(a.extinfo.comment.as_deref(), Some("invented"));
    let date = a.extinfo.date.unwrap();
    assert_eq!((date.year, date.month, date.day), (2020, 2, 29));
    assert!(a.evidence.iter().any(|e| e.module == "EXTINFO"));
    for (name, kind) in [("NEW", ChangeKind::Added), ("GONE", ChangeKind::Removed), ("RESIZE", ChangeKind::Resized)] {
        assert!(set.changelog.entries.iter().any(|c| c.subject == name && c.kind == kind));
    }
    assert!(set.row("NEW").unwrap().cells[0].is_none());
    assert!(!set.changelog.entries.iter().any(|c| c.subject == "SAME"));
    assert!(set.dumps.iter().all(|d| !d.diagnostics.is_empty()));
}

#[test]
fn metadata_is_independent_and_padding_is_not_module_identity() {
    let a = tiny("0100JC20000117", &[("SAME", b"abc")], true);
    let mut bytes = a.clone().into_data();
    let ext = a.entries().into_iter().find(|e| e.0 == "EXTINFO").unwrap();
    bytes[ext.1 + 4] = 8; // invented version 2.07 → 2.08
    let module = a.entries().into_iter().find(|e| e.0 == "SAME").unwrap();
    bytes[module.1 + module.2] = 0xAA; // alignment byte is outside the module
    let set = RomSet::from_ps2(&[("a", a), ("b", RomDir::new(bytes).unwrap())]).unwrap();
    let same = set.row("SAME").unwrap();
    assert_eq!(same.cells[0].as_ref().unwrap().cluster, same.cells[1].as_ref().unwrap().cluster);
    assert!(set.changelog.entries.iter().any(|c| c.subject == "SAME" && c.kind == ChangeKind::Metadata));
    assert!(!set.changelog.entries.iter().any(|c| c.subject == "SAME" && c.kind == ChangeKind::Changed));
    let whole = set.row("whole dump").unwrap();
    assert_ne!(whole.cells[0].as_ref().unwrap().cluster, whole.cells[1].as_ref().unwrap().cluster);
}

#[test]
fn malformed_extinfo_and_empty_sets_are_bounded() {
    let a = tiny("0100JC20000117", &[("SAME", b"abc")], true);
    let ext = a.entries().into_iter().find(|e| e.0 == "EXTINFO").unwrap();
    for (offset, value) in [(ext.1 + 6, 255), (ext.1 + 12, 0xFA)] {
        let mut d = a.clone().into_data();
        d[offset] = value;
        assert!(RomSet::from_ps2(&[("bad", RomDir::new(d).unwrap())]).is_err());
    }
    assert!(RomSet::from_ps2(&[]).unwrap().rows.iter().all(|r| r.cells.is_empty()));
    assert!(RomSet::from_ps1(&[]).unwrap().dumps.is_empty());
    assert_eq!([0, 25, 26, 51, 52, 701, 702].map(cluster_label), ["A", "Z", "AA", "AZ", "BA", "ZZ", "AAA"]);
}

// Invented called loader at a deliberately non-reference offset, followed by a tiny
// LZ representation of 64 KiB of zeros. It exercises store decoding without any dump.
fn packed_osd(store: Option<u32>, target: u16) -> Vec<u8> {
    let mut module = vec![0; 0x400];
    module[..4].copy_from_slice(b"\x7FELF");
    word(&mut module, 0x1C, 0x34);
    module[0x2A..0x2E].copy_from_slice(&[32, 0, 1, 0]);
    word(&mut module, 0x34, 1);
    word(&mut module, 0x38, 0x80);
    word(&mut module, 0x3C, 0x100000);
    word(&mut module, 0x100, 0x0C040030); // jal 0x1000C0, wrapper at file +0x140
    word(&mut module, 0x138, 0x03E00008); // previous routine ends before +0x140
    let mut code = Vec::new();
    if let Some(store) = store { code.extend([0x3C020020, 0x34420000 | u32::from(target), 0x3C081000, 0x35080002, store]); }
    code.extend([0x24030007, 0x0000000C, 0x03E00008, 0]);
    for (i, w) in code.into_iter().enumerate() { word(&mut module, 0x140 + i * 4, w); }
    module.extend_from_slice(&0x10000u32.to_le_bytes());
    let mut remaining = 0x10000usize;
    let mut first = true;
    while remaining > 0 {
        let mut descriptor = 3u32;
        let mut tokens = Vec::new();
        for i in 0..30 {
            if remaining == 0 { break }
            if first { tokens.push(0); remaining -= 1; first = false; }
            else {
                descriptor |= 1 << (31 - i);
                tokens.extend_from_slice(&0xF800u16.to_be_bytes()); // offset 1, length 34
                remaining = remaining.saturating_sub(34);
            }
        }
        module.extend_from_slice(&descriptor.to_be_bytes());
        module.extend(tokens);
    }
    let length = (module.len() - 0x80) as u32;
    word(&mut module, 0x44, length);
    word(&mut module, 0x48, length);
    module
}

#[test]
fn invented_loader_is_located_and_unknown_stores_are_not_zero_patches() {
    let clean = packed_osd(None, 4);
    let patched = packed_osd(Some(0xAC480000), 4); // sw t0,0(v0)
    let set = RomSet::from_ps2(&[("a", tiny("0100JC20000117", &[("OSDSYS", &clean)], false)),
        ("b", tiny("0200EC20040614", &[("OSDSYS", &patched)], false))]).unwrap();
    let clean = set.dumps[0].osdsys.as_ref().unwrap();
    let patched = set.dumps[1].osdsys.as_ref().unwrap();
    assert!(clean.complete && patched.complete);
    assert_eq!((clean.patches.len(), patched.patches.len()), (0, 1));
    assert_eq!(clean.image_cluster, patched.image_cluster);
    assert_ne!(clean.effective_cluster, patched.effective_cluster);
    let patch = &patched.patches[0];
    assert_eq!((patch.target, patch.old, patch.new), (0x200004, 0, 0x10000002));
    assert_eq!(patch.old_instruction, "nop");
    assert_eq!(patch.new_instruction, "b 0x200010");
    assert_eq!(patch.store.address, Some(0x1000D0));
    for (store, target) in [(0xAC490000, 4), (0xAC480000, 5), (0xAC480000, 0xFFFF)] {
        let osd = packed_osd(Some(store), target);
        let set = RomSet::from_ps2(&[("bad", tiny("0100JC20000117", &[("OSDSYS", &osd)], false))]).unwrap();
        let result = set.dumps[0].osdsys.as_ref().unwrap();
        assert!(!result.complete);
        assert!(result.effective_cluster.is_none());
    }
}

fn invented_ps1() -> Vec<u8> {
    let mut d = vec![0; 0x80000];
    d[0x100..0x104].copy_from_slice(&[0x17, 0x01, 0x00, 0x20]);
    d[0x108..0x128].copy_from_slice(b"Sony Computer Entertainment Inc.");
    d[0x200..0x214].copy_from_slice(b"PS-X Realtime Kernel");
    word(&mut d, 0x18000, 0x27BDFFE0); // invented raw shell entry
    d
}

#[test]
fn ps1_header_exclusion_exact_ranges_and_real_padding_differences() {
    let a = invented_ps1();
    let mut b = a.clone();
    b[0x104] = 7; // header is outside bootstrap identities
    let only_header = RomSet::from_ps1(&[("a", Ps1Bios::new(a.clone()).unwrap()), ("header", Ps1Bios::new(b.clone()).unwrap())]).unwrap();
    assert_eq!(only_header.row("bootstrap + kernel").unwrap().cells.iter().flatten().map(|c| c.cluster).collect::<Vec<_>>(), [0, 0]);
    assert_eq!(only_header.changelog.entries.len(), 1);
    assert_eq!(only_header.changelog.entries[0].subject, "header");
    b[0xFE..0x100].fill(1);
    b[0x180..0x182].fill(1);
    b[0x17000] = 0xFF; // padding stays a byte difference
    b[0x65000] = 1;
    b[0x65D00] = 2;
    let set = RomSet::from_ps1(&[("a", Ps1Bios::new(a).unwrap()), ("b", Ps1Bios::new(b).unwrap())]).unwrap();
    let boot = set.changelog.entries.iter().find(|c| c.subject == "bootstrap").unwrap();
    assert_eq!(boot.changed_bytes, 4);
    assert_eq!(boot.ranges.iter().map(|r| (r.after.as_ref().unwrap().offset, r.after.as_ref().unwrap().length)).collect::<Vec<_>>(), [(0xFE, 2), (0x180, 2)]);
    let kernel = set.changelog.entries.iter().find(|c| c.subject == "kernel").unwrap();
    assert_eq!(kernel.changed_bytes, 1);
    assert_eq!(kernel.ranges[0].after.as_ref().unwrap().offset, 0x17000);
    assert!(kernel.ranges[0].hint.as_ref().unwrap().contains("ROM monitor command tables"));
    for name in ["block A", "block A tail table"] {
        assert_eq!(set.changelog.entries.iter().find(|c| c.subject == name).unwrap().changed_bytes, 1);
    }
    assert_eq!(set.row("block A").unwrap().cells[0].as_ref().unwrap().size, 0x1CB7);
}

fn ps2_cases() -> Vec<(&'static str, RomDir)> {
    ["scph10000.bin", "DTL-H30101/DTL-H30101_USA_Dev_0150_20001228_v4_[CC645DA1].rom0",
        "PS2 Bios 30004R V6 Pal.bin", "scph39001.bin",
        "Sony PlayStation 2 BIOS (U)(v1.6)(2002-03-19)[SCPH39004].bin", "SCPH-70004_BIOS_V12_PAL_200.BIN"]
        .into_iter().filter_map(|n| read(n).map(|d| (n, RomDir::new(d).expect(n)))).collect()
}

#[test]
fn all_available_ps2_opening_tables_and_sound_are_identical() {
    let inputs = ps2_cases();
    if inputs.is_empty() { return }
    let set = RomSet::from_ps2(&inputs).unwrap();
    for name in ["opening tower grid", "opening slots", "opening growth fill", "opening growth size", "opening orbs", "opening cubes", "opening prisms", "boot sound bank"] {
        let row = set.row(name).expect(name);
        assert!(row.cells.iter().all(|c| c.as_ref().is_some_and(|c| c.cluster == 0)), "{name}: {:?}", set.dumps.iter().map(|d| &d.diagnostics).collect::<Vec<_>>());
    }
    if let Some(row) = set.row("SNDIMAGE") { assert!(row.cells.iter().flatten().all(|c| c.cluster == 0)); }
    assert!(set.dumps.iter().all(|d| d.diagnostics.is_empty()), "{:?}", set.dumps.iter().map(|d| &d.diagnostics).collect::<Vec<_>>());
    if inputs.len() == 6 {
        assert_eq!(set.row("texture list").unwrap().cells.iter().flatten().map(|c| c.cluster).collect::<Vec<_>>(), [0, 1, 1, 1, 1, 2]);
        assert_eq!(set.row("ribbon constants").unwrap().cells.iter().flatten().map(|c| c.cluster).collect::<Vec<_>>(), [0, 1, 1, 1, 1, 1]);
        assert_eq!(set.row("OSDSYS (unpacked)").unwrap().cells[1..].iter().flatten().map(|c| c.size).collect::<Vec<_>>(), [0xA76B4, 0xA7EB4, 0xA7EB4, 0xA7EB4, 0xC8F74]);
    }
}

#[test]
fn loader_store_counts_targets_and_old_words_come_from_dumps() {
    let inputs: Vec<_> = ps2_cases().into_iter().filter(|(n, _)| *n == "PS2 Bios 30004R V6 Pal.bin" || *n == "scph39001.bin" || n.contains("SCPH39004")).collect();
    if inputs.is_empty() { return }
    let set = RomSet::from_ps2(&inputs).unwrap();
    for dump in &set.dumps {
        let osd = dump.osdsys.as_ref().unwrap();
        assert!(osd.complete, "{}: {:?}", dump.name, dump.diagnostics);
        let expected = if dump.name == "scph39001.bin" { 0 } else if dump.name.contains("39004") { 3 } else { 8 };
        assert_eq!(osd.patches.len(), expected, "{}", dump.name);
        assert_eq!(osd.image_cluster, 0);
        for patch in &osd.patches {
            assert_ne!(patch.old, patch.new);
            assert_eq!((patch.store.module.as_str(), patch.store.space), ("OSDSYS loader", Space::File));
            assert!(patch.target >= 0x200000 && patch.target < 0x2A7EB4);
        }
        if expected == 3 {
            assert_eq!(osd.patches.iter().map(|p| p.target).collect::<Vec<_>>(), [0x2024E4, 0x2022F4, 0x20230C]);
            assert_eq!(osd.patches[0].new_instruction, "nop");
            assert!(osd.patches[1..].iter().all(|p| p.new_instruction.starts_with("b ")));
            assert!(osd.patches[2].old_instruction.starts_with("bnel "));
        }
    }
    if inputs.len() == 3 {
        assert_eq!(set.changelog.entries.iter().filter(|c| c.kind == ChangeKind::Loader).count(), 2);
        let effective = set.row("OSDSYS (effective)").unwrap();
        assert_eq!(effective.cells.iter().flatten().map(|c| c.cluster).collect::<Vec<_>>(), [0, 1, 2]);
    }
}

#[test]
fn ps1_kernels_shell_clusters_font_and_duplicate_file() {
    let inputs: Vec<_> = ["Sony PlayStation BIOS (J)[SCPH-1000].bin", "Sony PlayStation BIOS (J)(v1.1)(1995-01-22)[SCPH-3000].bin",
        "Sony PlayStation BIOS (E)(v2.0)(1995-05-10)[SCPH-1002].bin", "Sony PlayStation BIOS (J)(v2.2)(1995-12-04)[SCPH-5000].bin",
        "Sony PlayStation SCPH-1001 - DTLH-3000 BIOS v2.2 (1995-12-04)(Sony)(US).bin",
        "Sony PlayStation BIOS (E)(v2.2)(1995-12-04)[DTLH-3002].bin", "Sony PlayStation BIOS (J)(v3.0)(1996-09-09)[SCPH-5500].bin",
        "scph5501.bin", "Sony PlayStation BIOS (U)(v3.0)(1996-11-18)[SCPH-7003].bin",
        "Sony PlayStation BIOS (E)(v3.0)(1997-01-06)[SCPH-5502 + SCPH-5552].bin", "Sony PlayStation BIOS (J)(v4.0)(1997-08-18)[SCPH-7000].bin",
        "Sony PlayStation BIOS (U)(v4.1)(1997-12-16)[SCPH-7001 + SCPH-9001].bin",
        "Sony PlayStation BIOS (E)(v4.1)(1997-12-16)[SCPH-7502 + SCPH-9002].bin",
        "Sony PSone BIOS (U)(v4.5)(2000-05-25)[SCPH-101].bin", "PSXONPSP660.BIN"]
        .into_iter().filter_map(|n| read(n).map(|d| (n, Ps1Bios::new(d).expect(n)))).collect();
    if inputs.is_empty() { return }
    let set = RomSet::from_ps1(&inputs).unwrap();
    assert!(set.dumps.iter().all(|d| d.date.is_some_and(|date| (1994..=2000).contains(&date.year))));
    let kernel = set.row("kernel").unwrap();
    let mut generations = std::collections::HashMap::new();
    for (i, dump) in set.dumps.iter().enumerate().filter(|(_, d)| d.name != "PSXONPSP660.BIN") {
        let generation = if dump.kernel_generation == Some(KernelGeneration::K1) { 1 } else { 2 };
        let cluster = kernel.cells[i].as_ref().unwrap().cluster;
        assert_eq!(*generations.entry(generation).or_insert(cluster), cluster);
    }
    if let (Some(a), Some(b)) = (generations.get(&1), generations.get(&2)) { assert_ne!(a, b); }
    assert!(set.row("font").unwrap().cells.iter().flatten().all(|c| c.cluster == 0));
    let duplicate = ["scph5501.bin", "Sony PlayStation BIOS (U)(v3.0)(1996-11-18)[SCPH-7003].bin"].map(|n| set.dumps.iter().position(|d| d.name == n));
    if let [Some(a), Some(b)] = duplicate {
        for row in &set.rows { assert_eq!(row.cells[a].as_ref().unwrap().cluster, row.cells[b].as_ref().unwrap().cluster, "{}", row.name); }
        assert!(!set.changelog.entries.iter().any(|c| c.before == a.min(b) && c.after == a.max(b)));
    }
    let four = set.dumps.iter().enumerate().filter(|(_, d)| d.version.contains("Version 4.1")).map(|(i, _)| i).collect::<Vec<_>>();
    if four.len() == 2 {
        let shell = set.row("shell").unwrap();
        assert_eq!(shell.cells[four[0]].as_ref().unwrap().cluster, shell.cells[four[1]].as_ref().unwrap().cluster);
    }
    for change in set.changelog.entries.iter().filter(|c| c.subject == "bootstrap" || c.subject == "kernel") {
        assert!(!change.ranges.is_empty());
        assert!(change.ranges.iter().all(|r| r.before.as_ref().is_some_and(|e| e.address == Some(0xBFC00000 + e.offset))));
    }
}
