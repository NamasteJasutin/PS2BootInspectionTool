//! Tests for PS1 boot narration (`ps2kit::ps1narrate`).
//!
//! Verifies POST sequence extraction and TTY log generation across both
//! real BIOS dumps in `bios/` (oracle test, skipped silently if absent) and
//! synthetic BIOS images (for offline/CI verification).

use ps2kit::ps1bios::{KernelGeneration, Ps1Bios};
use ps2kit::ps1narrate::{post_sequence, tty_log, BootFacts};
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn read_dump(name: &str) -> Option<Vec<u8>> {
    std::fs::read(root().join("bios").join(name)).ok()
}

struct OracleCase {
    file: &'static str,
    label: &'static str,
    kernel: KernelGeneration,
}

fn oracle_cases() -> Vec<OracleCase> {
    use KernelGeneration::{K1, K2};
    vec![
        OracleCase { file: "Sony PlayStation BIOS (J)[SCPH-1000].bin", label: "1000", kernel: K1 },
        OracleCase { file: "Sony PlayStation BIOS (J)(v1.1)(1995-01-22)[SCPH-3000].bin", label: "3000", kernel: K1 },
        OracleCase { file: "Sony PlayStation BIOS (E)(v2.0)(1995-05-10)[SCPH-1002].bin", label: "1002", kernel: K1 },
        OracleCase { file: "Sony PlayStation BIOS (J)(v2.2)(1995-12-04)[SCPH-5000].bin", label: "5000", kernel: K2 },
        OracleCase { file: "Sony PlayStation SCPH-1001 - DTLH-3000 BIOS v2.2 (1995-12-04)(Sony)(US).bin", label: "1001", kernel: K2 },
        OracleCase { file: "Sony PlayStation BIOS (E)(v2.2)(1995-12-04)[DTLH-3002].bin", label: "DTL3002", kernel: K2 },
        OracleCase { file: "Sony PlayStation BIOS (J)(v3.0)(1996-09-09)[SCPH-5500].bin", label: "5500", kernel: K2 },
        OracleCase { file: "scph5501.bin", label: "5501", kernel: K2 },
        OracleCase { file: "Sony PlayStation BIOS (U)(v3.0)(1996-11-18)[SCPH-7003].bin", label: "7003", kernel: K2 },
        OracleCase { file: "Sony PlayStation BIOS (E)(v3.0)(1997-01-06)[SCPH-5502 + SCPH-5552].bin", label: "5502", kernel: K2 },
        OracleCase { file: "Sony PlayStation BIOS (J)(v4.0)(1997-08-18)[SCPH-7000].bin", label: "7000", kernel: K2 },
        OracleCase { file: "Sony PlayStation BIOS (U)(v4.1)(1997-12-16)[SCPH-7001 + SCPH-9001].bin", label: "7001", kernel: K2 },
        OracleCase { file: "Sony PlayStation BIOS (E)(v4.1)(1997-12-16)[SCPH-7502 + SCPH-9002].bin", label: "7502", kernel: K2 },
        OracleCase { file: "Sony PSone BIOS (U)(v4.5)(2000-05-25)[SCPH-101].bin", label: "101", kernel: K2 },
        OracleCase { file: "PSXONPSP660.BIN", label: "POPS660", kernel: K2 },
    ]
}

#[test]
fn ps1_post_sequence_oracles() {
    let mut tested = 0;
    for c in oracle_cases() {
        let Some(data) = read_dump(c.file) else { continue };
        let bios = Ps1Bios::new(data).expect(c.file);
        let seq = post_sequence(&bios);

        assert!(!seq.is_empty(), "{}: POST sequence must not be empty", c.label);

        // All stages must have ROM addresses in range and non-empty meanings
        for s in &seq {
            assert!(
                (s.rom_address >= 0xBFC0_0000 && s.rom_address < 0xBFC8_0000) || s.rom_address < 0x0001_0000,
                "{}: stage 0x{:02X} has out-of-range address 0x{:08X}",
                c.label, s.value, s.rom_address
            );
            assert!(!s.meaning.is_empty(), "{}: stage meaning must not be empty", c.label);
        }

        // K1 and K2 differ in length and stages
        match c.kernel {
            KernelGeneration::K1 => {
                assert_eq!(seq.len(), 11, "{}: K1 sequence must have 11 stages", c.label);
                let values: Vec<u8> = seq.iter().map(|s| s.value).collect();
                assert_eq!(values, vec![0xF, 0xE, 1, 2, 3, 4, 5, 6, 7, 8, 9], "{}: K1 stage values", c.label);
            }
            KernelGeneration::K2 => {
                // In K2 dumps: 11 bootstrap stages + 6 kernel InstallDevices stages = 17 stages
                assert_eq!(seq.len(), 17, "{}: K2 sequence must have 17 stages", c.label);
                let values: Vec<u8> = seq.iter().map(|s| s.value).collect();
                assert_eq!(
                    values,
                    vec![0xF, 0xE, 1, 2, 3, 4, 1, 3, 4, 5, 6, 2, 5, 6, 7, 8, 9],
                    "{}: K2 stage values mismatch",
                    c.label
                );
            }
            _ => unreachable!(),
        }

        tested += 1;
    }

    // Informational logging during test output
    if tested > 0 {
        println!("ps1_post_sequence_oracles verified {tested} BIOS dumps");
    }
}

#[test]
fn ps1_tty_log_synthetic_boot_facts() {
    let mut facts = BootFacts::default();
    facts.boot_file = Some("cdrom:SLUS_000.01;1".into());
    facts.argument = Some("1".into());
    facts.tcb = Some(4);
    facts.event = Some(16);
    facts.stack = Some(0x801F_FF00);
    facts.ram_size_mb = Some(2);
    facts.pc0 = Some(0x8001_0000);
    facts.t_addr = Some(0x8001_0000);
    facts.t_size = Some(0x0004_0000);
    facts.s_addr = Some(0x801F_FF00);
    facts.s_size = Some(0x0000_8000);

    // Find any available real BIOS dump to test format string filling
    let mut bios_opt = None;
    for c in oracle_cases() {
        if let Some(data) = read_dump(c.file) {
            bios_opt = Some(Ps1Bios::new(data).expect(c.file));
            break;
        }
    }

    let bios = match bios_opt {
        Some(b) => b,
        None => {
            // If no dump is present, build a synthetic 512 KB image containing format strings
            let mut data = vec![0u8; 0x80000];
            data[0x108..0x128].copy_from_slice(b"Sony Computer Entertainment Inc.");
            // Raw shell loader stub at 0x18000 (addiu sp, sp, -16)
            data[0x18000..0x18004].copy_from_slice(&0x27BD_FFF0u32.to_le_bytes());
            let s1 = b"PS-X Realtime Kernel Ver.2.5\n\0";
            data[0x200..0x200 + s1.len()].copy_from_slice(s1);
            let s2 = b"boot file     : %s\n\0";
            data[0x60050..0x60050 + s2.len()].copy_from_slice(s2);
            let s3 = b"EXEC:PC0(%08x)  T_ADDR(%08x)  T_SIZE(%08x)\n\0";
            data[0x60080..0x60080 + s3.len()].copy_from_slice(s3);
            let s4 = b"S_ADDR(%08x)\0";
            data[0x600C0..0x600C0 + s4.len()].copy_from_slice(s4);
            Ps1Bios::new(data).expect("synthetic BIOS")
        }
    };

    let log = tty_log(&bios, &facts);
    assert!(!log.is_empty(), "TTY log must not be empty");

    // Every line starts with a ROM address "0xBFC.....: "
    for line in &log {
        assert!(
            line.starts_with("0xBFC") && line.contains(": "),
            "line must start with ROM address prefix: {line}"
        );
    }

    // Verify boot file name appears in the log
    assert!(
        log.iter().any(|l| l.contains("cdrom:SLUS_000.01;1")),
        "TTY log must contain boot file name"
    );

    // Verify formatted hex values appear in the log
    assert!(
        log.iter().any(|l| l.contains("80010000") || l.contains("8001_0000")),
        "TTY log must contain formatted initial_pc / text_addr"
    );
    assert!(
        log.iter().any(|l| l.contains("801fff00") || l.contains("801FFF00")),
        "TTY log must contain formatted stack address"
    );
}
