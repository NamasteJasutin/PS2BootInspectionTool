//! Private dump oracles from `ps1_version_matrix.md` §§1–3.4, 5–6; absent inputs skip.
use ps2kit::ps1::{LicencePolicy, Ps1Shell};
use ps2kit::ps1bios::{audit, audit_set, KernelGeneration, Ps1AuditIssue, Ps1Bios, Ps1SetIssue, ShellStorage};
use ps2kit::rom::RomDir;
use std::path::{Path, PathBuf};

fn root() -> PathBuf { Path::new(env!("CARGO_MANIFEST_DIR")).join("..") }
fn read(name: &str) -> Option<Vec<u8>> { std::fs::read(root().join("bios").join(name)).ok() }

// Only expected identity fields and labels, never BIOS bytes or known-image hashes.
struct Case {
    file: &'static str,
    label: &'static str,
    version: &'static str,
    date: (u16, u8, u8),
    kernel: KernelGeneration,
    storage: ShellStorage,
    length: usize,
    policy: LicencePolicy,
    strings: usize,
    logo: bool,
}

fn cases() -> Vec<Case> {
    use KernelGeneration::{K1, K2};
    use LicencePolicy::{Always, ByLetter, Never, Unconditional};
    use ShellStorage::{Packed, Pops, Raw};
    let early = (1994, 9, 22);
    let late = (1995, 12, 4);
    vec![
        Case { file: "Sony PlayStation BIOS (J)[SCPH-1000].bin", label: "1000", version: "", date: early, kernel: K1, storage: Raw, length: 0x67FF0, policy: Unconditional, strings: 1, logo: true },
        Case { file: "Sony PlayStation BIOS (J)(v1.1)(1995-01-22)[SCPH-3000].bin", label: "3000", version: "1.1 01/22/1995", date: early, kernel: K1, storage: Raw, length: 0x67FF0, policy: Unconditional, strings: 1, logo: true },
        Case { file: "Sony PlayStation BIOS (E)(v2.0)(1995-05-10)[SCPH-1002].bin", label: "1002", version: "2.0 05/10/95 E", date: early, kernel: K1, storage: Raw, length: 0x67FF0, policy: Never, strings: 0, logo: false },
        Case { file: "Sony PlayStation BIOS (J)(v2.2)(1995-12-04)[SCPH-5000].bin", label: "5000", version: "2.2 12/04/95 J", date: late, kernel: K2, storage: Raw, length: 0x67FF0, policy: Always, strings: 1, logo: true },
        Case { file: "Sony PlayStation SCPH-1001 - DTLH-3000 BIOS v2.2 (1995-12-04)(Sony)(US).bin", label: "1001", version: "2.2 12/04/95 A", date: late, kernel: K2, storage: Raw, length: 0x67FF0, policy: Never, strings: 1, logo: false },
        Case { file: "Sony PlayStation BIOS (E)(v2.2)(1995-12-04)[DTLH-3002].bin", label: "DTL3002", version: "2.2 12/04/95 E", date: late, kernel: K2, storage: Raw, length: 0x67FF0, policy: Never, strings: 1, logo: false },
        Case { file: "Sony PlayStation BIOS (J)(v3.0)(1996-09-09)[SCPH-5500].bin", label: "5500", version: "3.0 09/09/96 J", date: late, kernel: K2, storage: Raw, length: 0x67FF0, policy: Always, strings: 1, logo: true },
        Case { file: "scph5501.bin", label: "5501", version: "3.0 11/18/96 A", date: late, kernel: K2, storage: Raw, length: 0x67FF0, policy: Never, strings: 1, logo: false },
        Case { file: "Sony PlayStation BIOS (U)(v3.0)(1996-11-18)[SCPH-7003].bin", label: "7003", version: "3.0 11/18/96 A", date: late, kernel: K2, storage: Raw, length: 0x67FF0, policy: Never, strings: 1, logo: false },
        Case { file: "Sony PlayStation BIOS (E)(v3.0)(1997-01-06)[SCPH-5502 + SCPH-5552].bin", label: "5502", version: "3.0 01/06/97 E", date: late, kernel: K2, storage: Raw, length: 0x67FF0, policy: Never, strings: 1, logo: false },
        Case { file: "Sony PlayStation BIOS (J)(v4.0)(1997-08-18)[SCPH-7000].bin", label: "7000", version: "4.0 08/18/97 J", date: (1997, 5, 29), kernel: K2, storage: Packed, length: 0x5BDA0, policy: Always, strings: 1, logo: true },
        Case { file: "Sony PlayStation BIOS (U)(v4.1)(1997-12-16)[SCPH-7001 + SCPH-9001].bin", label: "7001", version: "4.1 12/16/97 A", date: late, kernel: K2, storage: Packed, length: 0x5AD40, policy: Never, strings: 1, logo: false },
        Case { file: "Sony PlayStation BIOS (E)(v4.1)(1997-12-16)[SCPH-7502 + SCPH-9002].bin", label: "7502", version: "4.1 12/16/97 E", date: late, kernel: K2, storage: Packed, length: 0x5AD40, policy: Never, strings: 1, logo: false },
        Case { file: "Sony PSone BIOS (U)(v4.5)(2000-05-25)[SCPH-101].bin", label: "101", version: "4.5 05/25/00 A", date: late, kernel: K2, storage: Packed, length: 0x60990, policy: ByLetter, strings: 3, logo: true },
        Case { file: "PSXONPSP660.BIN", label: "POPS660", version: "4.5 05/25/00 J", date: late, kernel: K2, storage: Pops, length: 0x20F40, policy: Never, strings: 0, logo: true },
    ]
}

#[test]
fn every_standalone_dump_identity_and_licence() {
    let mut rows = Vec::new();
    let ps2 = std::fs::read(root().join("SCPH-70004_BIOS_V12_PAL_200.BIN")).ok().map(|d| RomDir::new(d).unwrap());
    for c in cases() {
        let Some(data) = read(c.file) else { continue };
        assert!(Ps1Bios::detect(&data), "{}: detection", c.label);
        let bios = Ps1Bios::new(data).expect(c.file);
        let id = bios.identity();
        if let Some(rom) = &ps2 { assert_eq!(bios.krom(), rom.module("KROM").unwrap(), "{} KROM", c.label) }
        let date = id.header_date.expect("valid BCD date");
        assert_eq!((date.year, date.month, date.day), c.date, "{} date", c.label);
        assert_eq!(id.maker, "Sony Computer Entertainment Inc.");
        let model = if c.label == "7000" { "CEX-7000/-7001 by K.S.    " } else if c.kernel == KernelGeneration::K1 { "CEX-1000 KT-3  by S.O." } else { "CEX-3000/1001/1002 by K.S." };
        assert_eq!(id.model, model, "{} model", c.label);
        let version = if c.version.is_empty() { String::new() } else { format!("System ROM Version {}", c.version) };
        assert_eq!(id.version, version, "{} version", c.label);
        assert_eq!(audit(bios.data()).issues.contains(&Ps1AuditIssue::MissingVersion), c.version.is_empty());
        let letter = c.version.chars().last().filter(char::is_ascii_uppercase);
        assert_eq!(id.region_letter, letter);
        assert_eq!(id.kernel_generation, Some(c.kernel));
        assert_eq!(id.a0_entries, Some(if c.kernel == KernelGeneration::K1 { 0xBF } else { 0xC1 }));
        assert_eq!(id.a0_implemented_entries, Some(if c.kernel == KernelGeneration::K1 { 0xAF } else { 0xB5 }));
        assert_eq!(id.get_system_info.is_some(), c.kernel == KernelGeneration::K2);
        assert!(id.kernel_banner.starts_with("PS-X Realtime Kernel Ver.2.5"));
        assert_eq!((id.shell_storage, id.shell_length), (c.storage, c.length));
        let shell = Ps1Shell::load_ps1(&bios).expect(c.file);
        assert_eq!(shell.licence_policy, c.policy, "{} policy", c.label);
        assert_eq!(shell.checks_licence(), matches!(c.policy, LicencePolicy::Always | LicencePolicy::Unconditional));
        assert_eq!(shell.licence_strings.len(), c.strings);
        assert_eq!(shell.reference_logo().is_some(), c.logo);
        if let Some(logo) = shell.reference_logo() { assert_eq!((logo.verts.len(), logo.normals.len(), logo.prims.len()), (337, 153, 560)) }
        assert_eq!(shell.comparison_logo_bytes().is_some(), c.logo && shell.checks_licence());
        assert_eq!(shell.events.len(), 19);
        assert_eq!(shell.translation, [0, -340, 5888], "{} translation", c.label);
        assert_eq!((shell.wordmark.width, shell.wordmark.height, shell.tm.width, shell.tm.height), (200, 40, 20, 8));
        assert_eq!(shell.wordmarks.len(), if matches!(c.label, "1000" | "3000" | "POPS660") { 1 } else { 2 });
        let selected = if c.policy == LicencePolicy::Never && c.label != "POPS660" { 1 } else { 0 };
        assert_eq!(shell.wordmark, shell.wordmarks[selected], "{} selected wordmark", c.label);
        rows.push(format!("{:<8} {:<4} {:<2} {:?} {:05X} {:<13?} {} {} {}", c.label, c.version.split_whitespace().next().unwrap_or("—"), letter.map(|c| c.to_string()).unwrap_or_else(|| "—".into()), c.kernel, c.length, c.policy, u8::from(shell.checks_licence()), c.strings, u8::from(c.logo)));
    }
    if !rows.is_empty() {
        eprintln!("dump     ver  R  K  bytes policy        on strings logo");
        for row in rows { eprintln!("{row}") }
    }
}

#[test]
fn ps2_front_end_policies() {
    for (name, policy) in [
        ("scph10000.bin", LicencePolicy::Always),
        ("DTL-H30101/DTL-H30101_USA_Dev_0150_20001228_v4_[CC645DA1].rom0", LicencePolicy::Never),
        ("PS2 Bios 30004R V6 Pal.bin", LicencePolicy::ByLetter),
        ("scph39001.bin", LicencePolicy::ByLetter),
        ("Sony PlayStation 2 BIOS (U)(v1.6)(2002-03-19)[SCPH39004].bin", LicencePolicy::ByLetter),
        ("SCPH-70004_BIOS_V12_PAL_200.BIN", LicencePolicy::ByLetter),
    ] {
        let Some(data) = read(name) else { continue };
        let shell = Ps1Shell::load(&RomDir::new(data).unwrap()).expect(name);
        assert_eq!(shell.licence_policy, policy, "{name}");
        assert!(shell.reference_logo().is_some());
        assert_eq!(shell.wordmarks.len(), 1);
    }
}

#[test]
fn corrupted_transfer_and_duplicate_audit() {
    let bad = "Sony PlayStation BIOS (J)(v2.2)(1995-12-04)[SCPH-5000][h].bin";
    if let Some(data) = read(bad) {
        let a = audit(&data);
        assert_eq!((a.size, a.lf_count, a.crlf_count), (526083, 1794, 1794));
        assert_eq!(a.first_shifted_offset, Some(0x24A));
        assert_eq!(a.version_offset, Some(0x80632));
        assert!(a.is_corrupt());
        assert!(a.issues.contains(&Ps1AuditIssue::MisplacedVersion { offset: 0x80632 }));
        assert!(!Ps1Bios::detect(&data));
        assert!(Ps1Bios::new(data).is_err());
    }
    let images: Vec<_> = cases().into_iter().filter_map(|c| read(c.file).map(|d| (c.file, d))).collect();
    let refs: Vec<_> = images.iter().map(|(n, d)| (*n, d.as_slice())).collect();
    let issues = audit_set(&refs);
    if refs.iter().any(|&(n, _)| n == "scph5501.bin") && refs.iter().any(|&(n, _)| n.contains("[SCPH-7003]")) {
        assert!(issues.iter().any(|i| matches!(i, Ps1SetIssue::Duplicate { first, second } if (first == "scph5501.bin" && second.contains("[SCPH-7003]")) || (second == "scph5501.bin" && first.contains("[SCPH-7003]")))));
    }
    assert!(!issues.iter().any(|i| matches!(i, Ps1SetIssue::RegionMismatch { .. })));
    let ps2 = "Sony PlayStation 2 BIOS (U)(v1.6)(2002-03-19)[SCPH39004].bin";
    if let Some(data) = read(ps2) {
        assert!(!Ps1Bios::detect(&data));
        assert!(audit_set(&[(ps2, &data)]).iter().any(|i| matches!(i, Ps1SetIssue::RegionMismatch { filename_letter: 'A', version_letter: 'E', .. })));
    }
}

#[test]
fn psone_projection_matches_ps2_baked_matrix() {
    let Some(data) = read("Sony PSone BIOS (U)(v4.5)(2000-05-25)[SCPH-101].bin") else { return };
    let Ok(ps2) = std::fs::read(root().join("SCPH-70004_BIOS_V12_PAL_200.BIN")) else { return };
    let psone = Ps1Shell::load_ps1(&Ps1Bios::new(data).unwrap()).unwrap();
    let ps2 = Ps1Shell::load(&RomDir::new(ps2).unwrap()).unwrap();
    assert_eq!(psone.logo_bytes, ps2.logo_bytes);
    for (a, b) in psone.rotation.iter().flatten().zip(ps2.rotation.iter().flatten()) { assert!((i32::from(*a) - i32::from(*b)).abs() <= 1, "{a} versus {b}") }
    // Compare vertices in TMD order, then match projected triangles by depth (small rotation
    // rounding can permute nearly-equal depths). Every visible point must be within one px.
    let a = psone.project(&psone.logo, 30);
    let b = ps2.project(&ps2.logo, 30);
    assert_eq!(a.len(), b.len());
    let mut maximum = 0f32;
    for tri in &a {
        let distance = b.iter().map(|other| tri.xy.iter().zip(other.xy.iter()).flat_map(|(p, q)| p.iter().zip(q.iter()).map(|(x, y)| (x - y).abs())).fold(0f32, f32::max)).fold(f32::INFINITY, f32::min);
        maximum = maximum.max(distance);
    }
    assert!(maximum <= 1.0, "projection differs by {maximum} px");
    eprintln!("PSone/PS2: {} visible triangles; max projection difference {maximum:.6} px", a.len());
}

#[test]
fn policy_follows_code_even_when_version_changes() {
    let Some(data) = read("Sony PlayStation BIOS (J)(v2.2)(1995-12-04)[SCPH-5000].bin") else { return };
    let bios = Ps1Bios::new(data).unwrap();
    let mut image = bios.shell_image().unwrap();
    // Verified flag store in kernel §3.3/matrix §3.4 at shell +0x58: change source to zero.
    let mut instruction = u32::from_le_bytes(image[0x58..0x5C].try_into().unwrap());
    assert_eq!(instruction >> 26, 43);
    instruction &= !(31 << 16);
    image[0x58..0x5C].copy_from_slice(&instruction.to_le_bytes());
    let shell = Ps1Shell::locate(&image, bios.krom(), "System ROM Version 4.5 05/25/00 J").unwrap();
    assert_eq!(shell.licence_policy, LicencePolicy::Never);
    assert!(!shell.checks_licence());
    let shell = Ps1Shell::locate(&bios.shell_image().unwrap(), bios.krom(), "System ROM Version 3.0 11/18/96 A").unwrap();
    assert_eq!(shell.licence_policy, LicencePolicy::Always);
    assert!(shell.checks_licence());
}

#[test]
fn synthetic_detection_audit_and_malformed_images() {
    assert!(!Ps1Bios::detect(&[]));
    assert!(Ps1Bios::new(Vec::new()).is_err());
    let mut d = vec![0; 0x80000];
    d[0x108..0x128].copy_from_slice(b"Sony Computer Entertainment Inc.");
    d[0x200..0x214].copy_from_slice(b"PS-X Realtime Kernel");
    assert!(Ps1Bios::detect(&d));
    d[0x400..0x40A].copy_from_slice(b"RESET\0\0\0\0\0");
    assert!(!Ps1Bios::detect(&d));
    let text = b"System ROM Version 9.9 01/01/00 E\0";
    d[0x7FF80..0x7FF80 + text.len()].copy_from_slice(text);
    assert!(audit(&d).issues.contains(&Ps1AuditIssue::MisplacedVersion { offset: 0x7FF80 }));
    assert!(audit_set(&[("a (U).bin", &d), ("b (E).bin", &d)]).iter().any(|i| matches!(i, Ps1SetIssue::Duplicate { .. })));
    assert!(audit_set(&[("a (U).bin", &d)]).iter().any(|i| matches!(i, Ps1SetIssue::RegionMismatch { filename_letter: 'A', version_letter: 'E', .. })));
    for n in [0, 4, 12, 64, 1024] { assert!(Ps1Shell::locate(&d[..n], &[], "").is_err()) }
    assert!(LicencePolicy::ByLetter.checks(Some('E')));
    assert!(!LicencePolicy::ByLetter.checks(Some('A')));
}

#[test]
fn malformed_input_is_an_error_not_a_panic() {
    let mut rng = ps2kit::history::SplitMix(5);
    let noise: Vec<u8> = (0..0x80000).map(|_| rng.next_u64() as u8).collect();
    for d in [vec![], vec![0u8; 100], vec![0u8; 0x80000], noise.clone(), noise[..0x7FFFF].to_vec()] {
        let _ = ps2kit::ps1bios::audit(&d);
        assert!(ps2kit::ps1bios::Ps1Bios::new(d).is_err());
    }
    // A real dump cut short or with its shell zeroed must fail cleanly too.
    let Ok(real) = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../bios/scph5501.bin")) else { return };
    let mut zeroed = real.clone();
    zeroed[0x18000..0x7FFF0].fill(0);
    for d in [real[..0x40000].to_vec(), zeroed] { let _ = ps2kit::ps1bios::Ps1Bios::new(d).and_then(|b| b.shell_image()); }
}
