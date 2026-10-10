//! Tests for disc file traversal, game executable probe, and SYSTEM.CNF audit.
//!
//! Synthetic tests run unconditionally on CI.
//! Oracle tests verify real disc dumps in `discs/` and skip silently when absent.

use ps2kit::disc::DiscKind;
use ps2kit::probe::{
    audit_system_cnf, CnfKeyReader, DetectedVideoMode, DiscFiles, DiscProbe, ElfProbe,
    IoprpReport, IrxProbe, Ps1ExeProbe,
};
use std::path::{Path, PathBuf};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ps2kit-probe-test-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn discs_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("discs")
}

// ============================================================================
// Synthetic Test Helpers
// ============================================================================

fn write_iso(dir: &Path, sectors: &[Vec<u8>]) -> PathBuf {
    let p = dir.join("disc.iso");
    std::fs::write(&p, sectors.concat()).unwrap();
    p
}

/// Builds a synthetic multi-level ISO 9660 filesystem image.
fn synthetic_iso_with_subdirs(
    files: &[(&str, &[u8])], // (full_path, data)
) -> Vec<Vec<u8>> {
    let mut sectors: Vec<Vec<u8>> = vec![vec![0u8; 2048]; 40];

    // PVD at 16
    let pvd = &mut sectors[16];
    pvd[0] = 1;
    pvd[1..6].copy_from_slice(b"CD001");
    pvd[6] = 1;
    pvd[40..40 + 8].copy_from_slice(b"TESTDISC");
    pvd[80..84].copy_from_slice(&40u32.to_le_bytes());
    // Root dir at 18
    pvd[156] = 34;
    pvd[158..162].copy_from_slice(&18u32.to_le_bytes());
    pvd[166..170].copy_from_slice(&2048u32.to_le_bytes());

    // Root directory entries:
    // We'll put root files and subdirectories.
    // LBA 18: root dir
    // LBA 19: /IRX dir
    // LBA 20+: files
    let mut current_lba = 20u32;
    let mut file_lbas = Vec::new();
    for (_path, data) in files {
        let lba = current_lba;
        let count = data.len().div_ceil(2048).max(1) as u32;
        current_lba += count;
        file_lbas.push((lba, data.len() as u32));
    }

    // Expand sectors if needed
    while sectors.len() < (current_lba as usize) {
        sectors.push(vec![0u8; 2048]);
    }

    // Populate file contents
    for (i, (_path, data)) in files.iter().enumerate() {
        let (lba, _size) = file_lbas[i];
        let mut off = 0;
        let mut sec = lba as usize;
        while off < data.len() {
            let chunk = (data.len() - off).min(2048);
            sectors[sec][..chunk].copy_from_slice(&data[off..off + chunk]);
            off += chunk;
            sec += 1;
        }
    }

    // Build root dir (LBA 18) and /IRX dir (LBA 19)
    let mut root_entries = Vec::new();
    // Subdir /IRX at LBA 19
    let irx_dir_entry = {
        let name = "IRX";
        let len = 33 + name.len() + (name.len() + 1) % 2;
        let mut e = vec![0u8; len];
        e[0] = len as u8;
        e[2..6].copy_from_slice(&19u32.to_le_bytes());
        e[10..14].copy_from_slice(&2048u32.to_le_bytes());
        e[25] = 2; // Directory flag
        e[32] = name.len() as u8;
        e[33..33 + name.len()].copy_from_slice(name.as_bytes());
        e
    };
    root_entries.extend(irx_dir_entry);

    let mut irx_entries = Vec::new();

    for (i, (path, _)) in files.iter().enumerate() {
        let (lba, size) = file_lbas[i];
        if let Some(sub) = path.strip_prefix("/IRX/") {
            let len = 33 + sub.len() + (sub.len() + 1) % 2;
            let mut e = vec![0u8; len];
            e[0] = len as u8;
            e[2..6].copy_from_slice(&lba.to_le_bytes());
            e[10..14].copy_from_slice(&size.to_le_bytes());
            e[32] = sub.len() as u8;
            e[33..33 + sub.len()].copy_from_slice(sub.as_bytes());
            irx_entries.extend(e);
        } else {
            let name = path.trim_start_matches('/');
            let len = 33 + name.len() + (name.len() + 1) % 2;
            let mut e = vec![0u8; len];
            e[0] = len as u8;
            e[2..6].copy_from_slice(&lba.to_le_bytes());
            e[10..14].copy_from_slice(&size.to_le_bytes());
            e[32] = name.len() as u8;
            e[33..33 + name.len()].copy_from_slice(name.as_bytes());
            root_entries.extend(e);
        }
    }

    sectors[18][..root_entries.len()].copy_from_slice(&root_entries);
    sectors[19][..irx_entries.len()].copy_from_slice(&irx_entries);

    sectors
}

/// Builds a synthetic PS2 boot ELF with known segments, stamps, syscalls, and gates.
fn synthetic_boot_elf(vmode_pal: bool) -> Vec<u8> {
    let mut elf = vec![0u8; 0x4000];
    elf[..4].copy_from_slice(&[0x7F, b'E', b'L', b'F']);
    elf[0x12] = 8; // MIPS
    elf[0x18..0x1C].copy_from_slice(&0x0034_0000u32.to_le_bytes()); // entry
    elf[0x1C..0x20].copy_from_slice(&0x34u32.to_le_bytes()); // phoff
    elf[0x2A] = 0x20; // phentsize
    elf[0x2C] = 1; // phnum

    // Segment 0: file 0x1000..0x3000 -> vaddr 0x340000..0x342000
    let ph = 0x34;
    elf[ph..ph + 4].copy_from_slice(&1u32.to_le_bytes()); // PT_LOAD
    elf[ph + 4..ph + 8].copy_from_slice(&0x1000u32.to_le_bytes()); // p_offset
    elf[ph + 8..ph + 12].copy_from_slice(&0x0034_0000u32.to_le_bytes()); // p_vaddr
    elf[ph + 16..ph + 20].copy_from_slice(&0x2000u32.to_le_bytes()); // filesz
    elf[ph + 20..ph + 24].copy_from_slice(&0x2000u32.to_le_bytes()); // memsz
    elf[ph + 24..ph + 28].copy_from_slice(&7u32.to_le_bytes()); // flags RWX

    // In loaded segment (file 0x1000..):
    // Put SDK stamps at 0x1100
    let stamp1 = b"PsIIlibcdvd 1650\0";
    elf[0x1100..0x1100 + stamp1.len()].copy_from_slice(stamp1);
    let stamp2 = b"libdma  1620\0";
    elf[0x1120..0x1120 + stamp2.len()].copy_from_slice(stamp2);

    // Put referenced modules at 0x1200
    let ref1 = b"cdrom:\\IRX\\SIO2MAN.IRX;1\0";
    elf[0x1200..0x1200 + ref1.len()].copy_from_slice(ref1);
    let ref2 = b"cdrom:\\IRX\\IOPRP165.IMG;1\0";
    elf[0x1230..0x1230 + ref2.len()].copy_from_slice(ref2);
    let ref3 = b"rom0:UDNL \0";
    elf[0x1260..0x1260 + ref3.len()].copy_from_slice(ref3);

    // Version gate at 0x1300 (vaddr 0x340300):
    // slti $2, $2, 0x205 (op=0x0A, rt=2, rs=2, imm=0x205)
    let slti_inst = (0x0Au32 << 26) | (2 << 21) | (2 << 16) | 0x205;
    elf[0x1300..0x1304].copy_from_slice(&slti_inst.to_le_bytes());

    // Syscall stubs:
    // Stub 1 at 0x1800 (vaddr 0x340800): SetGsCrt (2)
    // addiu $3, $0, 2; syscall; jr $ra; nop
    elf[0x1800..0x1804].copy_from_slice(&0x24030002u32.to_le_bytes());
    elf[0x1804..0x1808].copy_from_slice(&0x0000000Cu32.to_le_bytes());
    elf[0x1808..0x180C].copy_from_slice(&0x03E00008u32.to_le_bytes());

    // Stub 2 at 0x1810 (vaddr 0x340810): FlushCache (0x64)
    elf[0x1810..0x1814].copy_from_slice(&0x24030064u32.to_le_bytes());
    elf[0x1814..0x1818].copy_from_slice(&0x0000000Cu32.to_le_bytes());

    // Stub 3 at 0x1820 (vaddr 0x340820): GetOsdConfigParam (0x4B) - uncalled
    elf[0x1820..0x1824].copy_from_slice(&0x2403004Bu32.to_le_bytes());
    elf[0x1824..0x1828].copy_from_slice(&0x0000000Cu32.to_le_bytes());

    // Callers in code:
    // Call Stub 1 (SetGsCrt at 0x340800): jal 0x340800
    // jal target word = (0x340800 >> 2) & 0x03FFFFFF = 0x000D0200
    // jal opcode = (3 << 26) | 0x000D0200 = 0x0C0D0200
    let jal_setgscrt = (3u32 << 26) | ((0x0034_0800 >> 2) & 0x03FF_FFFF);
    elf[0x1400..0x1404].copy_from_slice(&jal_setgscrt.to_le_bytes());

    // Call Stub 2 (FlushCache at 0x340810): jal 0x340810
    let jal_flush = (3u32 << 26) | ((0x0034_0810 >> 2) & 0x03FF_FFFF);
    elf[0x1408..0x140C].copy_from_slice(&jal_flush.to_le_bytes());

    // Video mode call at 0x1500 (vaddr 0x398950 inside ResetGraph region):
    // addiu $6, $0, mode (2 for NTSC, 3 for PAL)
    let mode_val = if vmode_pal { 3u32 } else { 2u32 };
    let addiu_a2 = (9u32 << 26) | (6 << 16) | mode_val;
    elf[0x1500..0x1504].copy_from_slice(&addiu_a2.to_le_bytes());
    // jal dummy target
    let jal_dummy = (3u32 << 26) | 0x00010000;
    elf[0x1504..0x1508].copy_from_slice(&jal_dummy.to_le_bytes());

    elf
}

/// Builds a synthetic PS-X EXE with header, stamps, BIOS calls, and dead ROM probe.
fn synthetic_ps1_exe() -> Vec<u8> {
    let mut exe = vec![0u8; 0x1000];
    exe[..8].copy_from_slice(b"PS-X EXE");
    exe[0x10..0x14].copy_from_slice(&0x8016_5000u32.to_le_bytes()); // pc0
    exe[0x14..0x18].copy_from_slice(&0x8016_8000u32.to_le_bytes()); // gp0
    exe[0x18..0x1C].copy_from_slice(&0x8015_0000u32.to_le_bytes()); // t_addr
    exe[0x1C..0x20].copy_from_slice(&0x800u32.to_le_bytes()); // t_size (2048 bytes)
    exe[0x30..0x34].copy_from_slice(&0x801F_FF00u32.to_le_bytes()); // sp
    let marker = b"Sony Computer Entertainment Inc. for Europe area";
    exe[0x4C..0x4C + marker.len()].copy_from_slice(marker);

    // RCS Id stamp at 0x880
    let stamp = b"$Id: intr.c,v 1.61 1995/05/18 06:47:50 suzu Exp $\0";
    exe[0x880..0x880 + stamp.len()].copy_from_slice(stamp);

    // BIOS call sites in text (offset >= 0x800):
    // Site 1 at 0x904: jr $t2 with li $t2, 0xA0 and li $t1, 0x39 (InitHeap)
    // 0x900: addiu $10, $0, 0xA0 -> 0x240A00A0
    // 0x904: jr $t2              -> 0x01400008
    // 0x908: addiu $9, $0, 0x39  -> 0x24090039
    exe[0x900..0x904].copy_from_slice(&0x240A00A0u32.to_le_bytes());
    exe[0x904..0x908].copy_from_slice(&0x01400008u32.to_le_bytes());
    exe[0x908..0x90C].copy_from_slice(&0x24090039u32.to_le_bytes());

    // Site 2 at 0x914: jr $t2 with li $t2, 0xB0 and li $t1, 0x12 (InitPad)
    exe[0x910..0x914].copy_from_slice(&0x240A00B0u32.to_le_bytes());
    exe[0x914..0x918].copy_from_slice(&0x01400008u32.to_le_bytes());
    exe[0x918..0x91C].copy_from_slice(&0x24090012u32.to_le_bytes());

    // Dead ROM probe code at 0x980:
    // lui $8, 0xBFC0 (0x3C08BFC0)
    // lui $1, 0x1994 (0x3C011994)
    // ori $12, $12, 0xE228 (0x358CE228)
    exe[0x980..0x984].copy_from_slice(&0x3C08BFC0u32.to_le_bytes());
    exe[0x984..0x988].copy_from_slice(&0x3C011994u32.to_le_bytes());
    exe[0x988..0x98C].copy_from_slice(&0x358CE228u32.to_le_bytes());

    exe
}

// ============================================================================
// Synthetic Tests
// ============================================================================

#[test]
fn synthetic_iso_directory_traversal_and_read() {
    let dir = scratch("syn_iso");
    let cnf = b"BOOT2 = cdrom0:\\SLUS_200.01;1\r\nVER = 2.00\r\nVMODE = NTSC\r\n";
    let elf = synthetic_boot_elf(false);
    let iso_sectors = synthetic_iso_with_subdirs(&[
        ("/SYSTEM.CNF;1", cnf),
        ("/SLUS_200.01;1", &elf),
        ("/IRX/SIO2MAN.IRX;1", b"sio2man fake data"),
    ]);
    let path = write_iso(&dir, &iso_sectors);

    let mut disc_files = DiscFiles::open(&path).expect("open synthetic ISO");
    let list = disc_files.list();
    assert!(list.iter().any(|f| f.path == "/SYSTEM.CNF;1"));
    assert!(list.iter().any(|f| f.path == "/SLUS_200.01;1"));
    assert!(list.iter().any(|f| f.path == "/IRX/SIO2MAN.IRX;1"));
    assert!(list.iter().any(|f| f.is_directory && f.path == "/IRX"));

    // Loose path reads
    assert_eq!(disc_files.read("SYSTEM.CNF").unwrap(), cnf);
    assert_eq!(disc_files.read("cdrom0:\\SYSTEM.CNF;1").unwrap(), cnf);
    assert_eq!(disc_files.read("/IRX/SIO2MAN.IRX").unwrap(), b"sio2man fake data");
    assert_eq!(disc_files.read("IRX\\SIO2MAN.IRX;1").unwrap(), b"sio2man fake data");
    assert!(disc_files.read("NONEXISTENT.FILE").is_err());
}

#[test]
fn synthetic_ps2_elf_probe() {
    let elf_data = synthetic_boot_elf(false);
    let probe = ElfProbe::inspect(&elf_data).expect("inspect synthetic ELF");

    assert_eq!(probe.entry, 0x0034_0000);
    assert_eq!(probe.segments.len(), 1);
    assert_eq!(probe.segments[0].vaddr, 0x0034_0000);

    // SDK stamps
    assert!(probe.sdk_stamps.iter().any(|s| s.stamp == "PsIIlibcdvd 1650"));
    assert!(probe.sdk_stamps.iter().any(|s| s.stamp == "libdma 1620"));

    // Referenced modules
    assert!(probe.referenced_modules.iter().any(|m| m.path.contains("SIO2MAN.IRX")));
    assert!(probe.referenced_modules.iter().any(|m| m.path.contains("IOPRP165.IMG")));
    assert!(probe.referenced_modules.iter().any(|m| m.path.contains("rom0:UDNL")));

    // Version gates
    assert_eq!(probe.version_gates.len(), 1);
    assert_eq!(probe.version_gates[0].module, "mcserv");
    assert_eq!(probe.version_gates[0].min_version, 0x205);

    // Syscalls
    let called: Vec<i32> = probe.called_syscalls.iter().map(|s| s.number).collect();
    assert!(called.contains(&2), "SetGsCrt called");
    assert!(called.contains(&0x64), "FlushCache called");

    let uncalled: Vec<i32> = probe.uncalled_syscalls.iter().map(|s| s.number).collect();
    assert!(uncalled.contains(&0x4B), "GetOsdConfigParam uncalled");
}

#[test]
fn synthetic_ps1_exe_probe() {
    let exe_data = synthetic_ps1_exe();
    let probe = Ps1ExeProbe::inspect(&exe_data).expect("inspect synthetic PS1 exe");

    assert_eq!(probe.check.pc0, 0x8016_5000);
    assert_eq!(probe.check.t_addr, 0x8015_0000);
    assert_eq!(probe.check.t_size, 0x800);
    assert_eq!(probe.check.sp, Some(0x801F_FF00));
    assert!(probe.dead_rom_probe_present);

    // Stamps
    assert!(probe.stamps.iter().any(|s| s.stamp.contains("intr.c")));

    // BIOS calls
    let calls: Vec<(&'static str, usize)> = probe
        .bios_calls
        .iter()
        .map(|c| (c.name.unwrap_or("?"), c.offset))
        .collect();
    assert!(calls.iter().any(|(n, _)| *n == "InitHeap"));
    assert!(calls.iter().any(|(n, _)| *n == "InitPad"));
}

#[test]
fn system_cnf_key_audit_rules() {
    let text = r#"
BOOT2 = cdrom0:\SLUS_200.01;1
VER = 2.00
VMODE = NTSC
BOOT = cdrom:SCES_000.05;1
TCB = 4
EVENT = 10
STACK = 801FFF00
PSD1.0.0 = 0,0,160,0,0,0,0,0
UNKNOWN_KEY = 123
"#;
    let audits = audit_system_cnf(text);
    let find = |k: &str| audits.iter().find(|a| a.key == k).unwrap();

    assert_eq!(find("BOOT2").readers, vec![CnfKeyReader::Osdsys]);
    assert_eq!(find("VER").readers, vec![CnfKeyReader::Osdsys]);
    assert_eq!(find("VMODE").readers, vec![CnfKeyReader::Nobody]);
    assert_eq!(
        find("BOOT").readers,
        vec![CnfKeyReader::Osdsys, CnfKeyReader::Ps1Kernel]
    );
    assert_eq!(find("TCB").readers, vec![CnfKeyReader::Ps1Kernel]);
    assert_eq!(find("EVENT").readers, vec![CnfKeyReader::Ps1Kernel]);
    assert_eq!(find("STACK").readers, vec![CnfKeyReader::Ps1Kernel]);
    assert_eq!(find("PSD1.0.0").readers, vec![CnfKeyReader::Ps1Drv]);
    assert_eq!(find("UNKNOWN_KEY").readers, vec![CnfKeyReader::Unknown]);
}

#[test]
fn malformed_inputs_never_panic() {
    // DiscFiles on empty/corrupted files
    assert!(DiscFiles::open("nonexistent-file.iso").is_err());

    let dir = scratch("malformed");
    let bad_file = dir.join("bad.iso");
    std::fs::write(&bad_file, [0u8; 100]).unwrap();
    assert!(DiscFiles::open(&bad_file).is_err());

    // ElfProbe on corrupt data
    assert!(ElfProbe::inspect(&[]).is_err());
    assert!(ElfProbe::inspect(&[0u8; 10]).is_err());
    assert!(ElfProbe::inspect(&[0x7F, b'E', b'L', b'F', 0, 0, 0, 0]).is_err());

    // Ps1ExeProbe on corrupt data
    assert!(Ps1ExeProbe::inspect(&[]).is_ok()); // Degrades to truncated header check
    assert!(Ps1ExeProbe::inspect(&[0u8; 100]).is_ok());

    // IoprpReport on corrupt data
    assert!(IoprpReport::inspect(&[]).is_err());
    assert!(IoprpReport::inspect(&[0u8; 100]).is_err());

    // IrxProbe on corrupt data
    assert!(IrxProbe::inspect(&[]).is_err());

    // audit_system_cnf on malformed lines
    let _ = audit_system_cnf(";;;===;;;\n====\n   \n\n\n===abc");
}

// ============================================================================
// Real Disc Oracle Tests (Skip silently when dumps absent)
// ============================================================================

#[test]
fn oracle_tekken_tag_usa() {
    let cue = discs_dir()
        .join("Tekken Tag Tournament (USA) (v2.00)")
        .join("Tekken Tag Tournament (USA) (v2.00).cue");
    let Ok(probe) = DiscProbe::open(&cue) else {
        return;
    };

    assert_eq!(probe.kind, DiscKind::Ps2);

    // SYSTEM.CNF audit
    let boot2 = probe.cnf_audit.iter().find(|a| a.key == "BOOT2").unwrap();
    assert_eq!(boot2.value, "cdrom0:\\SLUS_200.01;1");
    assert_eq!(boot2.readers, vec![CnfKeyReader::Osdsys]);

    let ver = probe.cnf_audit.iter().find(|a| a.key == "VER").unwrap();
    assert_eq!(ver.value, "2.00");

    let vmode = probe.cnf_audit.iter().find(|a| a.key == "VMODE").unwrap();
    assert_eq!(vmode.value, "NTSC");
    assert_eq!(vmode.readers, vec![CnfKeyReader::Nobody]);

    // Boot ELF probe
    let elf = probe.elf_probe.as_ref().expect("boot ELF");
    assert_eq!(elf.entry, 0x0035_72A0);
    assert_eq!(elf.segments.len(), 3);
    assert_eq!(elf.segments[0].vaddr, 0x0010_0000);
    assert_eq!(elf.segments[1].vaddr, 0x0020_0000);
    assert_eq!(elf.segments[2].vaddr, 0x0034_0000);
    assert_eq!(elf.segments[2].file_size, 0x2B6AE4);

    // Hard-coded video mode: NTSC
    assert_eq!(elf.video_mode.map(|(m, _)| m), Some(DetectedVideoMode::Ntsc));

    // SDK stamps
    assert!(elf.sdk_stamps.iter().any(|s| s.stamp.contains("PsIIlibcdvd 1650")));
    assert!(elf.sdk_stamps.iter().any(|s| s.stamp.contains("libdma")));
    assert!(elf.sdk_stamps.iter().any(|s| s.stamp.contains("libpad")));
    assert!(elf.sdk_stamps.iter().any(|s| s.stamp.contains("libmc")));

    // Version gates: mcserv >= 0x205, mcman >= 0x206, padman >= 0x204
    assert!(elf.version_gates.iter().any(|g| g.module == "mcserv" && g.min_version == 0x205));
    assert!(elf.version_gates.iter().any(|g| g.module == "mcman" && g.min_version == 0x206));
    assert!(elf.version_gates.iter().any(|g| g.module == "padman" && g.min_version == 0x204));

    // Syscalls: Deci2Call (0x7C), SetGsCrt (2), Exit (4) called
    assert!(elf.called_syscalls.iter().any(|s| s.number == 0x7C && s.call_count == 10));
    assert!(elf.called_syscalls.iter().any(|s| s.number == 2 && s.call_count == 1));
    assert!(elf.called_syscalls.iter().any(|s| s.number == 4 && s.call_count == 3));

    // Syscalls uncalled: GetOsdConfigParam (0x4B), GetGsVParam (0x4D), PSMode (0x7D), MachineType (0x7E), GetMemorySize (0x7F)
    assert!(elf.uncalled_syscalls.iter().any(|s| s.number == 0x4B));
    assert!(elf.uncalled_syscalls.iter().any(|s| s.number == 0x4D));
    assert!(elf.uncalled_syscalls.iter().any(|s| s.number == 0x7D));
    assert!(elf.uncalled_syscalls.iter().any(|s| s.number == 0x7E));
    assert!(elf.uncalled_syscalls.iter().any(|s| s.number == 0x7F));

    // IOPRP image modules
    let ioprp = probe.ioprp_report.as_ref().expect("IOPRP report");
    assert!(ioprp.romdir_comment.as_deref().unwrap_or("").contains("xokano@rel-linux"));
    assert!(ioprp.modules.iter().any(|m| m.name == "SIFCMD" && m.version.as_deref() == Some("2.03")));
    assert!(ioprp.modules.iter().any(|m| m.name == "FILEIO" && m.version.as_deref() == Some("2.04")));
    assert!(ioprp.modules.iter().any(|m| m.name == "CDVDMAN" && m.version.as_deref() == Some("2.11")));
    assert!(ioprp.modules.iter().any(|m| m.name == "CDVDFSV" && m.version.as_deref() == Some("2.11")));

    // Disc IRX modules
    assert!(probe.irx_modules.iter().any(|m| m.module_name == "sio2man" && m.version_string == "2.01"));
    assert!(probe.irx_modules.iter().any(|m| m.module_name == "mcman_tool" && m.version_string == "2.11"));
    assert!(probe.irx_modules.iter().any(|m| m.module_name == "mcserv" && m.version_string == "2.08"));
    assert!(probe.irx_modules.iter().any(|m| m.module_name == "padman" && m.version_string == "2.06"));
}

#[test]
fn oracle_tekken_tag_europe() {
    let cue = discs_dir()
        .join("Tekken Tag Tournament (Europe) (En,Fr,De,Es,It) (v2.00)")
        .join("Tekken Tag Tournament (Europe) (En,Fr,De,Es,It) (v2.00)")
        .join("Tekken Tag Tournament (Europe) (En,Fr,De,Es,It) (v2.00).cue");
    let Ok(probe) = DiscProbe::open(&cue) else {
        return;
    };

    assert_eq!(probe.kind, DiscKind::Ps2);

    let boot2 = probe.cnf_audit.iter().find(|a| a.key == "BOOT2").unwrap();
    assert_eq!(boot2.value, "cdrom0:\\SCES_500.01;1");

    let vmode = probe.cnf_audit.iter().find(|a| a.key == "VMODE").unwrap();
    assert_eq!(vmode.value, "PAL");

    let elf = probe.elf_probe.as_ref().expect("boot ELF");
    assert_eq!(elf.video_mode.map(|(m, _)| m), Some(DetectedVideoMode::Pal));
    assert_eq!(elf.segments[2].file_size, 0x2BA9DC);
}

#[test]
fn oracle_tekken_europe_ps1() {
    let bin = discs_dir()
        .join("Tekken (Europe)")
        .join("Tekken (Europe) (Track 01).bin");
    let Ok(probe) = DiscProbe::open(&bin) else {
        return;
    };

    assert_eq!(probe.kind, DiscKind::Ps1);

    // SYSTEM.CNF audit
    let boot = probe.cnf_audit.iter().find(|a| a.key == "BOOT").unwrap();
    assert_eq!(boot.value, "cdrom:SCES_000.05;1");
    assert_eq!(
        boot.readers,
        vec![CnfKeyReader::Osdsys, CnfKeyReader::Ps1Kernel]
    );

    let ps1 = probe.ps1_probe.as_ref().expect("PS1 probe");
    assert_eq!(ps1.check.pc0, 0x8016_5398);
    assert_eq!(ps1.check.t_addr, 0x8015_0000);
    assert_eq!(ps1.check.t_size, 0x7B800);
    assert_eq!(ps1.check.sp, Some(0x801F_FFF0));
    assert!(ps1.dead_rom_probe_present);

    // Stamps
    assert!(ps1.stamps.iter().any(|s| s.stamp.contains("intr.c")));
    assert!(ps1.stamps.iter().any(|s| s.stamp.contains("sys.c")));
    assert!(ps1.stamps.iter().any(|s| s.stamp.contains("vsync.c")));

    // BIOS calls: 29 calls
    assert_eq!(ps1.bios_calls.len(), 29);
    assert!(ps1.bios_calls.iter().any(|c| c.name == Some("WarmBoot")));
    assert!(ps1.bios_calls.iter().any(|c| c.name == Some("strcat")));
    assert!(ps1.bios_calls.iter().any(|c| c.name == Some("strcpy")));
    assert!(ps1.bios_calls.iter().any(|c| c.name == Some("InitHeap")));
    assert!(ps1.bios_calls.iter().any(|c| c.name == Some("InitPad")));
}

#[test]
fn oracle_tekken_japan_ps1() {
    let cue = discs_dir().join("Tekken (Japan)").join("Tekken (Japan).cue");
    let Ok(probe) = DiscProbe::open(&cue) else {
        return;
    };

    assert_eq!(probe.kind, DiscKind::Ps1);
    assert!(probe.cnf_audit.is_empty(), "Tekken Japan has no SYSTEM.CNF");

    let ps1 = probe.ps1_probe.as_ref().expect("PS1 probe");
    assert_eq!(ps1.check.pc0, 0x8015_A760);
    assert_eq!(ps1.check.t_addr, 0x8015_0000);
    assert_eq!(ps1.check.t_size, 0x70800);
    assert!(!ps1.dead_rom_probe_present, "Japan loader has no sys.c ROM probe");

    // BIOS calls: 26 calls (WarmBoot, strcat, strcpy absent)
    assert_eq!(ps1.bios_calls.len(), 26);
    assert!(!ps1.bios_calls.iter().any(|c| c.name == Some("WarmBoot")));
    assert!(!ps1.bios_calls.iter().any(|c| c.name == Some("strcat")));
    assert!(!ps1.bios_calls.iter().any(|c| c.name == Some("strcpy")));
    assert!(ps1.bios_calls.iter().any(|c| c.name == Some("InitHeap")));
}
