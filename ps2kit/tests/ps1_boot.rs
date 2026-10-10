//! Synthetic tests for PS1 kernel boot path, SYSTEM.CNF grammar, EXE checks, and root directory lookup.

use ps2kit::ps1boot::{
    ps1_boot_steps, CnfLint, ExeOutcome, FieldDerivation, Ps1BootStep, Ps1ExeCheck, Ps1RootLookup,
    Ps1SystemCnf, BIOS_DEFAULT_STACK, ROM_DEFAULT_EVENT, ROM_DEFAULT_TCB,
};

// --- Helper: ISO 9660 Directory Generator ----------------------------------------------

fn make_dir_record(name: &[u8], lba: u32, size: u32) -> Vec<u8> {
    let name_len = name.len();
    let pad = if name_len.is_multiple_of(2) { 1 } else { 0 };
    let rec_len = 33 + name_len + pad;
    let mut rec = vec![0u8; rec_len];
    rec[0] = rec_len as u8;
    rec[1] = 0; // ext attr
    rec[2..6].copy_from_slice(&lba.to_le_bytes());
    rec[6..10].copy_from_slice(&lba.to_be_bytes());
    rec[10..14].copy_from_slice(&size.to_le_bytes());
    rec[14..18].copy_from_slice(&size.to_be_bytes());
    // recording date/time: 7 bytes (bytes 18..25)
    rec[25] = 0; // flags
    rec[28..30].copy_from_slice(&1u16.to_le_bytes()); // vol seq LE
    rec[30..32].copy_from_slice(&1u16.to_be_bytes()); // vol seq BE
    rec[32] = name_len as u8;
    rec[33..33 + name_len].copy_from_slice(name);
    rec
}

fn make_root_dir_sector(entries: &[(&[u8], u32, u32)]) -> Vec<u8> {
    let mut sector = vec![0u8; 2048];
    let mut p = 0;
    for (name, lba, size) in entries {
        let rec = make_dir_record(name, *lba, *size);
        if p + rec.len() > 2048 {
            panic!("make_root_dir_sector: entries exceed one 2048-byte sector");
        }
        sector[p..p + rec.len()].copy_from_slice(&rec);
        p += rec.len();
    }
    sector
}

fn make_exe_header(
    pc0: u32,
    gp0: u32,
    t_addr: u32,
    t_size: u32,
    s_addr: u32,
    s_size: u32,
) -> Vec<u8> {
    let mut hdr = vec![0u8; 2048];
    hdr[0..8].copy_from_slice(b"PS-X EXE");
    hdr[0x10..0x14].copy_from_slice(&pc0.to_le_bytes());
    hdr[0x14..0x18].copy_from_slice(&gp0.to_le_bytes());
    hdr[0x18..0x1C].copy_from_slice(&t_addr.to_le_bytes());
    hdr[0x1C..0x20].copy_from_slice(&t_size.to_le_bytes());
    hdr[0x30..0x34].copy_from_slice(&s_addr.to_le_bytes());
    hdr[0x34..0x38].copy_from_slice(&s_size.to_le_bytes());
    hdr
}

// --- 1. SYSTEM.CNF Grammar Tests --------------------------------------------------------

#[test]
fn system_cnf_grammar_line_prefix_matching() {
    // BOOT2 satisfies BOOT
    let text = b"BOOT2 = cdrom0:\\SLUS_001.23;1\r\nTCB = 4\r\n";
    let cnf = Ps1SystemCnf::parse(text);
    assert_eq!(cnf.boot.as_deref(), Some("cdrom0:\\SLUS_001.23;1"));
    if let FieldDerivation::Parsed { matched_prefix, .. } = &cnf.boot_derivation {
        assert_eq!(matched_prefix, "BOOT2");
    } else {
        panic!("expected parsed derivation");
    }

    // TCBX satisfies TCB
    let text2 = b"TCBX = 10\r\n";
    let cnf2 = Ps1SystemCnf::parse(text2);
    assert_eq!(cnf2.tcb, 0x10);
    if let FieldDerivation::Parsed { matched_prefix, .. } = &cnf2.tcb_derivation {
        assert_eq!(matched_prefix, "TCBX");
    } else {
        panic!("expected parsed derivation");
    }
}

#[test]
fn system_cnf_grammar_hex_without_prefix() {
    // TCB = 10 is hex 16, EVENT = 10 is hex 16
    let text = b"TCB = 10\r\nEVENT = 10\r\nSTACK = 801FFF00\r\n";
    let cnf = Ps1SystemCnf::parse(text);
    assert_eq!(cnf.tcb, 16);
    assert_eq!(cnf.event, 16);
    assert_eq!(cnf.stack, 0x801FFF00);
}

#[test]
fn system_cnf_grammar_missing_keys_default_to_zero() {
    // Empty file or missing keys -> 0, not ROM defaults
    let cnf = Ps1SystemCnf::parse(b"");
    assert_eq!(cnf.tcb, 0);
    assert_eq!(cnf.event, 0);
    assert_eq!(cnf.stack, 0);
    assert_eq!(cnf.boot, None);
    assert!(cnf.argument.is_empty());
    assert_eq!(cnf.tcb_derivation, FieldDerivation::DefaultZero);
    assert_eq!(cnf.event_derivation, FieldDerivation::DefaultZero);
    assert_eq!(cnf.stack_derivation, FieldDerivation::DefaultZero);
    assert_eq!(cnf.boot_derivation, FieldDerivation::NotPresent);
}

#[test]
fn system_cnf_grammar_stack_zero_inherits_bios_stack() {
    // STACK = 0 means inherit BIOS stack
    let cnf = Ps1SystemCnf::parse(b"STACK = 0\r\n");
    assert_eq!(cnf.stack, 0);
    assert!(cnf.stack_derivation.is_parsed());
}

#[test]
fn system_cnf_grammar_argument_copied_to_ram_180() {
    let text = b"BOOT = cdrom:\\GAME.EXE;1 -debug -v\r\n";
    let cnf = Ps1SystemCnf::parse(text);
    assert_eq!(cnf.boot.as_deref(), Some("cdrom:\\GAME.EXE;1"));
    assert_eq!(cnf.argument_str(), "-debug -v");

    // Exact 128-byte clamp
    let long_arg = "A".repeat(200);
    let text_long = format!("BOOT = cdrom:\\GAME.EXE;1 {long_arg}\n");
    let cnf_long = Ps1SystemCnf::parse(text_long.as_bytes());
    assert_eq!(cnf_long.argument.len(), 128);
    assert_eq!(cnf_long.argument_str(), "A".repeat(128));
}

#[test]
fn system_cnf_grammar_cr_tolerated() {
    // Windows CRLF vs UNIX LF
    let crlf = b"BOOT = cdrom:\\GAME.EXE;1\r\nTCB = 4\r\nEVENT = 10\r\nSTACK = 801FFF00\r\n";
    let lf = b"BOOT = cdrom:\\GAME.EXE;1\nTCB = 4\nEVENT = 10\nSTACK = 801FFF00\n";
    let cnf_crlf = Ps1SystemCnf::parse(crlf);
    let cnf_lf = Ps1SystemCnf::parse(lf);
    assert_eq!(cnf_crlf.boot, cnf_lf.boot);
    assert_eq!(cnf_crlf.tcb, cnf_lf.tcb);
    assert_eq!(cnf_crlf.event, cnf_lf.event);
    assert_eq!(cnf_crlf.stack, cnf_lf.stack);
    assert!(cnf_crlf.argument.is_empty());
    assert!(cnf_lf.argument.is_empty());
}

#[test]
fn system_cnf_first_match_wins() {
    let text = b"TCB = 4\r\nTCB = 8\r\nBOOT = cdrom:\\FIRST.EXE;1\r\nBOOT = cdrom:\\SECOND.EXE;1\r\n";
    let cnf = Ps1SystemCnf::parse(text);
    assert_eq!(cnf.tcb, 4);
    assert_eq!(cnf.boot.as_deref(), Some("cdrom:\\FIRST.EXE;1"));
}

#[test]
fn system_cnf_max_2048_bytes() {
    let mut large = vec![b' '; 3000];
    large[0..7].copy_from_slice(b"TCB = 4");
    large[7] = b'\n';
    let cnf = Ps1SystemCnf::parse(&large);
    assert!(cnf.was_truncated);
    assert_eq!(cnf.total_input_bytes, 3000);
    assert_eq!(cnf.tcb, 4);
}

// --- 2. SYSTEM.CNF Linter Tests --------------------------------------------------------

#[test]
fn linter_flags_decimal_interpreted_as_hex() {
    let text = b"TCB = 10\r\nEVENT = 16\r\n";
    let cnf = Ps1SystemCnf::parse(text);
    let lints = cnf.lints();

    assert!(lints.iter().any(|l| matches!(
        l,
        CnfLint::DecimalInterpretedAsHex { key, raw, parsed_hex: 16, decimal_value: 10 }
        if key == "TCB" && raw == "10"
    )));
    assert!(lints.iter().any(|l| matches!(
        l,
        CnfLint::DecimalInterpretedAsHex { key, raw, parsed_hex: 22, decimal_value: 16 }
        if key == "EVENT" && raw == "16"
    )));
}

#[test]
fn linter_flags_hex_prefix_pitfall() {
    let text = b"STACK = 0x801FFF00\r\n";
    let cnf = Ps1SystemCnf::parse(text);
    assert_eq!(cnf.stack, 0); // Stops at 'x'
    let lints = cnf.lints();
    assert!(lints.iter().any(|l| matches!(l, CnfLint::HexPrefix { key, .. } if key == "STACK")));
}

#[test]
fn linter_flags_boot2_taken_as_boot() {
    let text = b"BOOT2 = cdrom0:\\SLUS_123.45;1\r\n";
    let cnf = Ps1SystemCnf::parse(text);
    let lints = cnf.lints();
    assert!(lints.iter().any(|l| matches!(l, CnfLint::Boot2TakenAsBoot { .. })));
}

#[test]
fn linter_flags_missing_keys() {
    let text = b"BOOT = cdrom:\\SLUS_001.23;1\r\n";
    let cnf = Ps1SystemCnf::parse(text);
    let lints = cnf.lints();
    assert!(lints.iter().any(|l| matches!(l, CnfLint::MissingKey { key: "TCB", rom_default: 4 })));
    assert!(lints.iter().any(|l| matches!(l, CnfLint::MissingKey { key: "EVENT", rom_default: 0x10 })));
    assert!(lints.iter().any(|l| matches!(l, CnfLint::MissingKey { key: "STACK", .. })));
}

#[test]
fn linter_flags_explicit_zero_stack() {
    let text = b"STACK = 0\r\n";
    let cnf = Ps1SystemCnf::parse(text);
    let lints = cnf.lints();
    assert!(lints.iter().any(|l| matches!(l, CnfLint::ExplicitZeroStack)));
}

#[test]
fn linter_flags_trailing_argument() {
    let text = b"BOOT = cdrom:\\SLUS_001.23;1 -arg\r\n";
    let cnf = Ps1SystemCnf::parse(text);
    let lints = cnf.lints();
    assert!(lints.iter().any(|l| matches!(l, CnfLint::TrailingArgument { argument } if argument == "-arg")));
}

#[test]
fn linter_flags_missing_version_and_non_cdrom() {
    let text = b"BOOT = host:game.exe\r\n";
    let cnf = Ps1SystemCnf::parse(text);
    let lints = cnf.lints();
    assert!(lints.iter().any(|l| matches!(l, CnfLint::MissingVersionSuffix { .. })));
    assert!(lints.iter().any(|l| matches!(l, CnfLint::NonCdromDevice { .. })));
}

#[test]
fn linter_display_sentences() {
    let text = b"BOOT2 = cdrom0:\\SLUS_123.45\r\nTCB = 10\r\nSTACK = 0x801FFF00\r\n";
    let cnf = Ps1SystemCnf::parse(text);
    for lint in cnf.lints() {
        let disp = lint.to_string();
        assert!(!disp.is_empty());
        assert!(disp.contains("ps1_kernel_boot.md"));
    }
}

// --- 3. PS-X EXE Checks -----------------------------------------------------------------

#[test]
fn exe_check_normal_bootable() {
    let hdr = make_exe_header(0x80010000, 0x80020000, 0x80010000, 0x2000, 0x801FFFF0, 0);
    let check = Ps1ExeCheck::judge(&hdr, 2048 + 0x2000);
    assert!(check.header_readable);
    assert_eq!(check.pc0, 0x80010000);
    assert_eq!(check.gp0, 0x80020000);
    assert_eq!(check.t_addr, 0x80010000);
    assert_eq!(check.t_size, 0x2000);
    assert!(check.t_size_sector_aligned);
    assert!(!check.text_extends_past_file);
    assert_eq!(check.sp, Some(0x801FFFF0));
    assert_eq!(check.outcome, ExeOutcome::Bootable);
}

#[test]
fn exe_check_unaligned_loads_nothing() {
    // t_size % 0x800 != 0
    let hdr = make_exe_header(0x80010000, 0, 0x80010000, 0x1234, 0, 0);
    let check = Ps1ExeCheck::judge(&hdr, 10000);
    assert!(!check.t_size_sector_aligned);
    assert_eq!(check.outcome, ExeOutcome::UnalignedLoadsNothing);
}

#[test]
fn exe_check_magic_unchecked_by_kernel() {
    // Arbitrary garbage instead of "PS-X EXE" magic
    let mut hdr = make_exe_header(0x80010000, 0, 0x80010000, 0x1000, 0, 0);
    hdr[0..8].copy_from_slice(b"GARBAGE!");
    let check = Ps1ExeCheck::judge(&hdr, 2048 + 0x1000);
    assert!(!check.has_psx_magic);
    assert!(!check.magic_checked_by_kernel);
    assert_eq!(check.outcome, ExeOutcome::Bootable); // Kernel executes it anyway
}

#[test]
fn exe_check_header_shorter_than_2048() {
    let hdr = vec![0u8; 1024];
    let check = Ps1ExeCheck::judge(&hdr, 1024);
    assert!(!check.header_readable);
    assert_eq!(check.outcome, ExeOutcome::HeaderTruncated);
}

#[test]
fn exe_check_text_extends_past_file() {
    let hdr = make_exe_header(0x80010000, 0, 0x80010000, 0x10000, 0, 0);
    let check = Ps1ExeCheck::judge(&hdr, 4096);
    assert!(check.t_size_sector_aligned);
    assert!(check.text_extends_past_file);
    assert_eq!(check.outcome, ExeOutcome::TextExtendsPastFile);
}

#[test]
fn exe_check_stack_handling_zero() {
    // s_addr == 0 -> sp is None (inherited from kernel)
    let hdr = make_exe_header(0x80010000, 0, 0x80010000, 0x800, 0, 0);
    let check = Ps1ExeCheck::judge(&hdr, 2048 + 0x800);
    assert_eq!(check.sp, None);
}

// --- 4. Root Directory Lookup Tests -----------------------------------------------------

#[test]
fn root_lookup_found_within_40_entries() {
    let mut entries = Vec::new();
    entries.push((b".\x00".as_slice(), 20, 2048));
    entries.push((b"..\x01".as_slice(), 20, 2048));
    for i in 2..39 {
        let name = format!("FILE{i:02};1");
        entries.push((Box::leak(name.into_bytes().into_boxed_slice()), 100 + i as u32, 2048));
    }
    entries.push((b"SYSTEM.CNF;1".as_slice(), 200, 512));

    let sector = make_root_dir_sector(&entries);
    let lookup = Ps1RootLookup::judge(&sector, "SYSTEM.CNF");
    assert!(lookup.found_by_kernel);
    assert!(lookup.found_in_full_directory);
    assert_eq!(lookup.entry_index, Some(39));
    assert!(!lookup.exceeds_40_entries);
    assert!(!lookup.exceeds_one_sector);
}

#[test]
fn root_lookup_exceeds_40_entries_in_first_sector() {
    let mut entries = Vec::new();
    entries.push((b".\x00".as_slice(), 20, 2048));
    entries.push((b"..\x01".as_slice(), 20, 2048));
    for i in 2..40 {
        let name = format!("FILE{i:02};1");
        entries.push((Box::leak(name.into_bytes().into_boxed_slice()), 100 + i as u32, 2048));
    }
    // Entry 40 (the 41st entry)
    entries.push((b"SYSTEM.CNF;1".as_slice(), 300, 512));

    let sector = make_root_dir_sector(&entries);
    let lookup = Ps1RootLookup::judge(&sector, "SYSTEM.CNF");
    assert!(!lookup.found_by_kernel);
    assert!(lookup.found_in_full_directory);
    assert_eq!(lookup.entry_index, Some(40));
    assert!(lookup.exceeds_40_entries);
    assert!(!lookup.exceeds_one_sector);
}

#[test]
fn root_lookup_exceeds_one_sector() {
    let sector0_entries: Vec<(&[u8], u32, u32)> = vec![
        (b".\x00", 20, 4096),
        (b"..\x01", 20, 4096),
        (b"OTHER.BIN;1", 30, 2048),
    ];
    let sector0 = make_root_dir_sector(&sector0_entries);

    let sector1_entries: Vec<(&[u8], u32, u32)> = vec![
        (b"SYSTEM.CNF;1", 40, 512),
    ];
    let sector1 = make_root_dir_sector(&sector1_entries);

    let mut two_sectors = sector0;
    two_sectors.extend_from_slice(&sector1);

    let lookup = Ps1RootLookup::judge(&two_sectors, "SYSTEM.CNF");
    assert!(!lookup.found_by_kernel);
    assert!(lookup.found_in_full_directory);
    assert_eq!(lookup.sector_index, Some(1));
    assert!(lookup.exceeds_one_sector);
    assert!(!lookup.exceeds_40_entries);
}

#[test]
fn root_lookup_normalizes_path_and_case() {
    let entries: Vec<(&[u8], u32, u32)> = vec![
        (b".\x00", 20, 2048),
        (b"SLUS_001.23;1", 50, 4096),
    ];
    let sector = make_root_dir_sector(&entries);

    // Look up with mixed case, no semicolon, and device prefix
    let lookup = Ps1RootLookup::judge(&sector, "cdrom:\\slus_001.23");
    assert!(lookup.found_by_kernel);
    assert_eq!(lookup.target_name, "SLUS_001.23;1");
}

// --- 5. Boot Steps Order and Execution Flow Tests ---------------------------------------

#[test]
fn ps1_boot_steps_full_normal_order() {
    let cnf_text = b"BOOT = cdrom:\\SLUS_001.23;1 -v\r\nTCB = 4\r\nEVENT = 10\r\nSTACK = 801FFF00\r\n";
    let exe_hdr = make_exe_header(0x80010000, 0x80020000, 0x80010000, 0x2000, 0x801FFFF0, 0);
    let root_entries: Vec<(&[u8], u32, u32)> = vec![
        (b".\x00", 20, 2048),
        (b"SYSTEM.CNF;1", 21, cnf_text.len() as u32),
        (b"SLUS_001.23;1", 22, 2048 + 0x2000),
    ];
    let root_dir = make_root_dir_sector(&root_entries);

    let steps = ps1_boot_steps(
        Some(cnf_text),
        Some((&exe_hdr, 2048 + 0x2000)),
        Some(&root_dir),
        Some(false),
    );

    // Verify ordering
    assert!(matches!(steps[0], Ps1BootStep::DriveAssumption { region: "SCEA" }));
    assert!(matches!(steps[1], Ps1BootStep::ShellExit { checks_licence: Some(false) }));
    assert!(matches!(steps[2], Ps1BootStep::CdInit));
    assert!(matches!(steps[3], Ps1BootStep::SystemCnfLookup { lookup: Some(_) }));
    assert!(matches!(steps[4], Ps1BootStep::SystemCnf { .. }));
    assert!(matches!(steps[5], Ps1BootStep::KernelSetup2 { .. }));
    assert!(matches!(steps[6], Ps1BootStep::BootFileLookup { .. }));
    assert!(matches!(steps[7], Ps1BootStep::ExeHeader { .. }));
    assert!(matches!(steps[8], Ps1BootStep::ExeTextLoad { sector_aligned: true, .. }));
    assert!(matches!(steps[9], Ps1BootStep::PreExecCheck { enabled: true }));
    assert!(matches!(steps[10], Ps1BootStep::DoExecute { pc0: 0x80010000, .. }));

    // Verify Display and who()
    for step in &steps {
        assert!(!step.who().is_empty());
        assert!(!step.to_string().is_empty());
    }
}

#[test]
fn ps1_boot_steps_missing_system_cnf_fallback() {
    let steps = ps1_boot_steps(None, None, None, None);

    if let Ps1BootStep::SystemCnf { cnf, boot_path, tcb, event, stack, .. } = &steps[4] {
        assert!(cnf.is_none());
        assert_eq!(boot_path, "cdrom:PSX.EXE;1");
        assert_eq!(*tcb, ROM_DEFAULT_TCB);
        assert_eq!(*event, ROM_DEFAULT_EVENT);
        assert_eq!(*stack, BIOS_DEFAULT_STACK);
    } else {
        panic!("expected SystemCnf step");
    }
}

#[test]
fn ps1_boot_steps_missing_boot_file_fails() {
    let cnf_text = b"BOOT = cdrom:\\MISSING.EXE;1\r\n";
    let root_entries: Vec<(&[u8], u32, u32)> = vec![
        (b".\x00", 20, 2048),
        (b"SYSTEM.CNF;1", 21, 100),
    ];
    let root_dir = make_root_dir_sector(&root_entries);

    let steps = ps1_boot_steps(Some(cnf_text), None, Some(&root_dir), Some(true));

    // The last step should be BootFailed
    let last = steps.last().unwrap();
    assert!(matches!(last, Ps1BootStep::BootFailed { code: ('B', 0x38A), .. }));
}

#[test]
fn ps1_boot_steps_truncated_exe_header_fails() {
    let cnf_text = b"BOOT = cdrom:\\TINY.EXE;1\r\n";
    let tiny_hdr = vec![0u8; 512]; // Shorter than 2048
    let steps = ps1_boot_steps(Some(cnf_text), Some((&tiny_hdr, 512)), None, None);

    let last = steps.last().unwrap();
    assert!(matches!(last, Ps1BootStep::BootFailed { code: ('B', 0x38A), .. }));
}
