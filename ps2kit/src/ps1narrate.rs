//! PlayStation 1 kernel boot narration: POST sequence and dummy-TTY replay.
//!
//! Reconstructs the boot progress of the real PS1 kernel as documented in
//! `notes/research/ps1_kernel_boot.md`:
//! - On a good boot, the POST byte at hardware register `0x1F802041` transitions through
//!   a predictable sequence written by bootstrap and kernel (`ps1_kernel_boot.md` §1.3;
//!   `ps1_version_matrix.md` §3.2).
//! - The kernel prints a detailed TTY diagnostic log to a dummy device (`A0:99 AddDummyTtyDevice`,
//!   `InstallDevices(0)`, §1.5). These messages never reach the physical SIO port on retail hardware
//!   but can be replayed from the format strings embedded in ROM filled with known boot facts.

use crate::ps1bios::Ps1Bios;
use crate::ps1boot::{Ps1SystemCnf, BIOS_DEFAULT_STACK, ROM_DEFAULT_EVENT, ROM_DEFAULT_TCB};

/// A stage in the PS1 kernel POST sequence recorded at hardware register `0x1F802041`.
///
/// Sources: `notes/research/ps1_kernel_boot.md` §1.3; `ps1_version_matrix.md` §3.2.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct PostStage {
    /// The POST byte value written (0..=0xFF).
    pub value: u8,
    /// The ROM address of the instruction or call site initiating this write.
    pub rom_address: u32,
    /// One-line explanation of the kernel/bootstrap milestone for this stage.
    pub meaning: &'static str,
}

/// Boot environment facts used to reconstruct the kernel's dummy-TTY boot log (`ps1_kernel_boot.md` §1.5).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct BootFacts {
    /// Boot executable file path (e.g. `"cdrom:SLUS_000.01;1"` or `"cdrom:PSX.EXE;1"`).
    pub boot_file: Option<String>,
    /// Thread Control Blocks count.
    pub tcb: Option<u32>,
    /// Event Control Blocks count.
    pub event: Option<u32>,
    /// Initial stack pointer top address (e.g. 0x801FFF00).
    pub stack: Option<u32>,
    /// Trailing argument copied to RAM 0x00000180.
    pub argument: Option<String>,
    /// System configuration file path (typically `"cdrom:SYSTEM.CNF;1"`).
    pub setup_file: Option<String>,
    /// Initial program counter from EXE header (`pc0`).
    pub pc0: Option<u32>,
    /// Text segment RAM destination from EXE header (`t_addr`).
    pub t_addr: Option<u32>,
    /// Text segment size in bytes from EXE header (`t_size`).
    pub t_size: Option<u32>,
    /// Stack base from EXE header (`s_addr`).
    pub s_addr: Option<u32>,
    /// Stack size from EXE header (`s_size`).
    pub s_size: Option<u32>,
    /// Effective RAM size in MB (typically 2).
    pub ram_size_mb: Option<u32>,
}

impl BootFacts {
    /// Constructs [`BootFacts`] populated from a parsed [`Ps1SystemCnf`].
    #[must_use]
    pub fn from_system_cnf(cnf: &Ps1SystemCnf) -> Self {
        Self {
            boot_file: cnf.boot.clone(),
            tcb: Some(cnf.tcb),
            event: Some(cnf.event),
            stack: Some(cnf.stack),
            argument: if cnf.argument.is_empty() {
                None
            } else {
                Some(cnf.argument_str())
            },
            setup_file: Some("cdrom:SYSTEM.CNF;1".into()),
            ..Default::default()
        }
    }
}

/// Derives the predictable POST stage sequence written to `0x1F802041` during a good boot.
///
/// Rather than returning a static hard-coded list, the sequence is derived from the user's
/// BIOS dump by locating the stores to `0x1F802041` in bootstrap and kernel
/// (`ps1_kernel_boot.md` §1.3).
///
/// In K1 kernels (1.0J, 1.1J, 2.0E), the kernel does not write POST stages during device setup;
/// in K2 kernels (2.2 through 4.5 and POPS), the kernel writes stages 1, 3, 4, 5, 6, 2 during
/// `InstallDevices` (`ps1_version_matrix.md` §3.2).
#[must_use]
pub fn post_sequence(bios: &Ps1Bios) -> Vec<PostStage> {
    let data = bios.data();
    let mut stages = Vec::new();

    // 1. Locate bootstrap post writing function
    let mut b_post_fn = None;
    for off in (0x1000..0x3000.min(data.len() - 4)).step_by(4) {
        let w = u32::from_le_bytes(data[off..off + 4].try_into().unwrap_or([0; 4]));
        // sb rt, 0x2041(rs)
        if (w >> 26) == 0x28 && (w & 0xFFFF) == 0x2041 {
            // Find function start by walking back to addiu sp, sp, -imm
            for back in (off.saturating_sub(40)..=off).rev().step_by(4) {
                let bw = u32::from_le_bytes(data[back..back + 4].try_into().unwrap_or([0; 4]));
                if (bw >> 16) == 0x27BD && (bw & 0x8000) != 0 {
                    b_post_fn = Some(0xBFC00000 + back as u32);
                    break;
                }
            }
            if b_post_fn.is_some() {
                break;
            }
        }
    }

    // 2. Locate kernel post writing function (in ROM window 0x10000..0x18000)
    let mut k_post_fn = None;
    if data.len() >= 0x18000 {
        for off in (0x10000..0x18000).step_by(4) {
            let w = u32::from_le_bytes(data[off..off + 4].try_into().unwrap_or([0; 4]));
            if (w >> 26) == 0x28 && (w & 0xFFFF) == 0x2041 {
                for back in (off.saturating_sub(40)..=off).rev().step_by(4) {
                    let bw = u32::from_le_bytes(data[back..back + 4].try_into().unwrap_or([0; 4]));
                    if (bw >> 16) == 0x27BD && (bw & 0x8000) != 0 {
                        k_post_fn = Some(0x00000500 + (back as u32 - 0x10000));
                        break;
                    }
                }
                if k_post_fn.is_some() {
                    break;
                }
            }
        }
    }

    // Collect bootstrap calls
    let mut b_calls = Vec::new();
    if let Some(target) = b_post_fn {
        let jal_target = (target & 0x0FFFFFFF) >> 2;
        let jal_w = 0x0C000000 | jal_target;
        for off in (0..0x10000.min(data.len() - 8)).step_by(4) {
            let w = u32::from_le_bytes(data[off..off + 4].try_into().unwrap_or([0; 4]));
            if w == jal_w {
                let delay = u32::from_le_bytes(data[off + 4..off + 8].try_into().unwrap_or([0; 4]));
                let prev = if off >= 4 {
                    u32::from_le_bytes(data[off - 4..off].try_into().unwrap_or([0; 4]))
                } else {
                    0
                };
                let mut val = None;
                // Delay slot takes precedence in MIPS
                if (delay >> 16) == 0x2404 || (delay >> 16) == 0x3404 {
                    val = Some((delay & 0xFFFF) as u8);
                } else if (prev >> 16) == 0x2404 || (prev >> 16) == 0x3404 {
                    val = Some((prev & 0xFFFF) as u8);
                }
                if let Some(v) = val {
                    b_calls.push((v, 0xBFC00000 + off as u32));
                }
            }
        }
    }

    // Collect kernel calls in InstallDevices
    let mut k_calls = Vec::new();
    if let Some(target) = k_post_fn {
        let jal_target = (target & 0x0FFFFFFF) >> 2;
        let jal_w = 0x0C000000 | jal_target;
        for off in (0x10000..0x18000.min(data.len() - 8)).step_by(4) {
            let w = u32::from_le_bytes(data[off..off + 4].try_into().unwrap_or([0; 4]));
            if w == jal_w {
                let delay = u32::from_le_bytes(data[off + 4..off + 8].try_into().unwrap_or([0; 4]));
                let prev = if off >= 4 {
                    u32::from_le_bytes(data[off - 4..off].try_into().unwrap_or([0; 4]))
                } else {
                    0
                };
                let mut val = None;
                if (delay >> 16) == 0x2404 || (delay >> 16) == 0x3404 {
                    val = Some((delay & 0xFFFF) as u8);
                } else if (prev >> 16) == 0x2404 || (prev >> 16) == 0x3404 {
                    val = Some((prev & 0xFFFF) as u8);
                }
                if let Some(v) = val {
                    k_calls.push((v, 0xBFC00000 + off as u32));
                }
            }
        }
    }

    // Assemble the sequence following execution order (ps1_kernel_boot.md §1.2, §1.3)
    // 1. rom_main: F, E
    if let Some(&(_, addr)) = b_calls.iter().find(|(v, a)| *v == 0x0F && *a >= 0xBFC06E00 && *a < 0xBFC06F00) {
        stages.push(PostStage {
            value: 0x0F,
            rom_address: addr,
            meaning: "rom_main entry: expansion-ROM pre-boot hook",
        });
    }
    if let Some(&(_, addr)) = b_calls.iter().find(|(v, a)| *v == 0x0E && *a >= 0xBFC06E00 && *a < 0xBFC07000) {
        stages.push(PostStage {
            value: 0x0E,
            rom_address: addr,
            meaning: "Expansion-ROM pre-boot hook complete, ttyflag = 0",
        });
    }

    // 2. boot_main: 1, 2, 3, 4
    if let Some(&(_, addr)) = b_calls.iter().find(|(v, a)| *v == 1 && *a >= 0xBFC06700 && *a < 0xBFC06810) {
        stages.push(PostStage {
            value: 1,
            rom_address: addr,
            meaning: "SR interrupts disabled, SPU volume zeroed",
        });
    }
    if let Some(&(_, addr)) = b_calls.iter().find(|(v, a)| *v == 2 && *a >= 0xBFC067A0 && *a < 0xBFC06840) {
        stages.push(PostStage {
            value: 2,
            rom_address: addr,
            meaning: "Copy kernel to RAM (0xBFC10000 -> 0xA0000500)",
        });
    }
    if let Some(&(_, addr)) = b_calls.iter().find(|(v, a)| *v == 3 && *a >= 0xBFC067B0 && *a < 0xBFC06850) {
        stages.push(PostStage {
            value: 3,
            rom_address: addr,
            meaning: "Install A0 table to 0x200 and exception vectors",
        });
    }
    if let Some(&(_, addr)) = b_calls.iter().find(|(v, a)| *v == 4 && *a >= 0xBFC06800 && *a < 0xBFC06880) {
        stages.push(PostStage {
            value: 4,
            rom_address: addr,
            meaning: "Kernel device installation (InstallDevices)",
        });
    }

    // 3. Kernel InstallDevices calls (K2 only: 1, 3, 4, 5, 6, 2)
    // In K2, k:27C0 calls post(1), then k:2870 which does 3, 4, 5, 6, then post(2)
    if !k_calls.is_empty() {
        if let Some(&(_, addr)) = k_calls.iter().find(|(v, _)| *v == 1) {
            stages.push(PostStage {
                value: 1,
                rom_address: addr,
                meaning: "Kernel InstallDevices entry",
            });
        }
        if let Some(&(_, addr)) = k_calls.iter().find(|(v, _)| *v == 3) {
            stages.push(PostStage {
                value: 3,
                rom_address: addr,
                meaning: "Add built-in kernel device",
            });
        }
        if let Some(&(_, addr)) = k_calls.iter().find(|(v, _)| *v == 4) {
            stages.push(PostStage {
                value: 4,
                rom_address: addr,
                meaning: "Add dummy/DUART TTY device",
            });
        }
        if let Some(&(_, addr)) = k_calls.iter().find(|(v, _)| *v == 5) {
            stages.push(PostStage {
                value: 5,
                rom_address: addr,
                meaning: "Flush standard I/O streams",
            });
        }
        if let Some(&(_, addr)) = k_calls.iter().find(|(v, _)| *v == 6) {
            stages.push(PostStage {
                value: 6,
                rom_address: addr,
                meaning: "Complete TTY device setup",
            });
        }
        if let Some(&(_, addr)) = k_calls.iter().find(|(v, _)| *v == 2) {
            stages.push(PostStage {
                value: 2,
                rom_address: addr,
                meaning: "Add CD-ROM and memory card devices",
            });
        }
    }

    // 4. Bootstrap resumes: 5, 6, 7, 8, 9
    if let Some(&(_, addr)) = b_calls.iter().find(|(v, a)| *v == 5 && *a >= 0xBFC06840 && *a < 0xBFC068E0) {
        stages.push(PostStage {
            value: 5,
            rom_address: addr,
            meaning: "Print kernel banner",
        });
    }
    if let Some(&(_, addr)) = b_calls.iter().find(|(v, a)| *v == 6 && *a >= 0xBFC06850 && *a < 0xBFC06900) {
        stages.push(PostStage {
            value: 6,
            rom_address: addr,
            meaning: "Kernel setup 1 (default TCB/EvCB configuration)",
        });
    }
    if let Some(&(_, addr)) = b_calls.iter().find(|(v, a)| *v == 7 && *a >= 0xBFC06900 && *a < 0xBFC069B0) {
        stages.push(PostStage {
            value: 7,
            rom_address: addr,
            meaning: "Hand-off to shell at 0x80030000",
        });
    }
    if let Some(&(_, addr)) = b_calls.iter().find(|(v, a)| *v == 8 && *a >= 0xBFC06910 && *a < 0xBFC069C0) {
        stages.push(PostStage {
            value: 8,
            rom_address: addr,
            meaning: "CD-ROM subsystem initialization (CdInit) and bootstrap loader banner",
        });
    }
    if let Some(&(_, addr)) = b_calls.iter().find(|(v, a)| *v == 9 && *a >= 0xBFC06990 && *a < 0xBFC06A30) {
        stages.push(PostStage {
            value: 9,
            rom_address: addr,
            meaning: "Open and parse cdrom:SYSTEM.CNF;1",
        });
    }

    stages
}

/// Helper locating a format string in BIOS bytes by subslice search.
fn find_string<'a>(data: &'a [u8], needle: &[u8]) -> Option<(u32, &'a str)> {
    let pos = data.windows(needle.len()).position(|w| w == needle)?;
    // Walk back to previous NUL byte
    let start = data[..pos].iter().rposition(|&b| b == 0).map(|p| p + 1).unwrap_or(pos);
    // Walk forward to terminating NUL byte
    let end = data[pos..].iter().position(|&b| b == 0).map(|p| pos + p).unwrap_or(data.len());
    let s = std::str::from_utf8(&data[start..end]).ok()?;
    Some((0xBFC00000 + start as u32, s))
}

/// Reconstructs the lines the PS1 kernel would have printed to its dummy TTY port during boot.
///
/// Log lines are rebuilt from the format strings located in the BIOS dump by content
/// (`ps1_kernel_boot.md` §1.5) filled with values from [`BootFacts`]. Each line is prefixed
/// with the ROM address of the corresponding format string (`0x{rom_address:08X}: `).
/// Unsupplied or unknown values are marked with placeholder `<unknown>`.
#[must_use]
pub fn tty_log(bios: &Ps1Bios, facts: &BootFacts) -> Vec<String> {
    let data = bios.data();
    let mut log = Vec::new();

    // 1. Kernel banner (printed at POST 5)
    if let Some((addr, banner)) = find_string(data, b"PS-X Realtime Kernel") {
        for line in banner.lines() {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                log.push(format!("0x{addr:08X}: {trimmed}"));
            }
        }
    }

    // 2. Initial KERNEL SETUP! with ROM defaults
    if let Some((addr, _)) = find_string(data, b"KERNEL SETUP") {
        log.push(format!("0x{addr:08X}: KERNEL SETUP!"));
    }
    if let Some((addr, _)) = find_string(data, b"Configuration : EvCB") {
        log.push(format!("0x{addr:08X}: Configuration : EvCB\t0x{:02x}\t\t", ROM_DEFAULT_EVENT));
    }
    if let Some((addr, _)) = find_string(data, b"TCB\t0x%02x").or_else(|| find_string(data, b"TCB 0x%02x")) {
        log.push(format!("0x{addr:08X}: TCB\t0x{:02x}", ROM_DEFAULT_TCB));
    }

    // 3. Bootstrap loader banner (printed at POST 8)
    if let Some((addr, banner)) = find_string(data, b"BOOTSTRAP LOADER") {
        for line in banner.lines() {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                log.push(format!("0x{addr:08X}: {trimmed}"));
            }
        }
    }

    // 4. Setup file name
    let setup_file = facts.setup_file.as_deref().unwrap_or("cdrom:SYSTEM.CNF;1");
    if let Some((addr, _)) = find_string(data, b"setup file    : %s") {
        log.push(format!("0x{addr:08X}: setup file    : {setup_file}"));
    }

    // 5. Parsed SYSTEM.CNF configuration lines
    if let Some((addr, _)) = find_string(data, b"%s\t%08x") {
        let tcb_str = facts.tcb.map(|v| format!("{v:08x}")).unwrap_or_else(|| "<unknown>".into());
        let ev_str = facts.event.map(|v| format!("{v:08x}")).unwrap_or_else(|| "<unknown>".into());
        let st_str = facts.stack.map(|v| format!("{v:08x}")).unwrap_or_else(|| "<unknown>".into());
        log.push(format!("0x{addr:08X}: TCB\t{tcb_str}"));
        log.push(format!("0x{addr:08X}: EVENT\t{ev_str}"));
        log.push(format!("0x{addr:08X}: STACK\t{st_str}"));
    }

    let boot_file_str = facts.boot_file.as_deref().unwrap_or("<unknown>");
    if let Some((addr, _)) = find_string(data, b"BOOT =") {
        log.push(format!("0x{addr:08X}: BOOT =\t{boot_file_str}"));
    }
    if let Some(arg) = &facts.argument {
        if let Some((addr, _)) = find_string(data, b"argument =") {
            log.push(format!("0x{addr:08X}: argument =\t{arg}"));
        }
    }

    // 6. Second KERNEL SETUP! with parsed parameters
    if let Some((addr, _)) = find_string(data, b"KERNEL SETUP") {
        log.push(format!("0x{addr:08X}: KERNEL SETUP!"));
    }
    if let Some((addr, _)) = find_string(data, b"Configuration : EvCB") {
        let ev_str = facts.event.map(|v| format!("0x{v:02x}")).unwrap_or_else(|| "<unknown>".into());
        log.push(format!("0x{addr:08X}: Configuration : EvCB\t{ev_str}\t\t"));
    }
    if let Some((addr, _)) = find_string(data, b"TCB\t0x%02x").or_else(|| find_string(data, b"TCB 0x%02x")) {
        let tcb_str = facts.tcb.map(|v| format!("0x{v:02x}")).unwrap_or_else(|| "<unknown>".into());
        log.push(format!("0x{addr:08X}: TCB\t{tcb_str}"));
    }

    // 7. Boot file confirmation
    if let Some((addr, _)) = find_string(data, b"boot file     : %s") {
        log.push(format!("0x{addr:08X}: boot file     : {boot_file_str}"));
    }

    // 8. Executable header fields
    if let Some((addr, _)) = find_string(data, b"EXEC:PC0") {
        match (facts.pc0, facts.t_addr, facts.t_size) {
            (Some(pc0), Some(t_addr), Some(t_size)) => {
                log.push(format!("0x{addr:08X}: EXEC:PC0({pc0:08x})  T_ADDR({t_addr:08x})  T_SIZE({t_size:08x})"));
            }
            _ => {
                let pc0 = facts.pc0.map(|v| format!("{v:08x}")).unwrap_or_else(|| "<unknown>".into());
                let t_addr = facts.t_addr.map(|v| format!("{v:08x}")).unwrap_or_else(|| "<unknown>".into());
                let t_size = facts.t_size.map(|v| format!("{v:08x}")).unwrap_or_else(|| "<unknown>".into());
                log.push(format!("0x{addr:08X}: EXEC:PC0({pc0})  T_ADDR({t_addr})  T_SIZE({t_size})"));
            }
        }
    }

    // 9. Boot address and execution banner
    if let Some((addr, _)) = find_string(data, b"boot address") {
        let pc0 = facts.pc0.map(|v| format!("{v:08x}")).unwrap_or_else(|| "<unknown>".into());
        let stack = facts.stack.unwrap_or(BIOS_DEFAULT_STACK);
        log.push(format!("0x{addr:08X}: boot address  : {pc0} {stack:08x}"));
        log.push(format!("0x{addr:08X}: Execute !"));
    }

    // 10. Stack line
    if let Some((addr, _)) = find_string(data, b"S_ADDR(") {
        let stack = facts.stack.unwrap_or(BIOS_DEFAULT_STACK);
        log.push(format!("0x{addr:08X}:                 S_ADDR({stack:08x})  S_SIZE(00000000)"));
    }

    // 11. End of Main
    if let Some((addr, _)) = find_string(data, b"End of Main") {
        log.push(format!("0x{addr:08X}: End of Main"));
    }

    log
}
