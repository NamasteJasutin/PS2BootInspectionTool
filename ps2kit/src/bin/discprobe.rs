//! `discprobe` CLI: inspects a game disc image (PS2 or PS1) and its boot executable.
//!
//! Usage:
//! ```sh
//! discprobe <image.cue|bin|iso>
//! ```

use ps2kit::probe::DiscProbe;
use std::env;
use std::process;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: discprobe <image.cue|bin|iso>");
        process::exit(1);
    }

    let path = &args[1];
    let probe = match DiscProbe::open(path) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("discprobe error: {e}");
            process::exit(1);
        }
    };

    println!("=== Disc Probe: {path} ===");
    println!("Kind: {:?}", probe.kind);
    println!("Files ({}):", probe.files.len());
    for f in &probe.files {
        let kind = if f.is_directory { "DIR " } else { "FILE" };
        println!("  {kind} {:40} LBA {:7} ({} bytes)", f.path, f.lba, f.size);
    }

    println!("\n=== SYSTEM.CNF Key Audit ===");
    if probe.cnf_audit.is_empty() {
        println!("  (No SYSTEM.CNF present on disc)");
    } else {
        for a in &probe.cnf_audit {
            let readers = a
                .readers
                .iter()
                .map(|r| r.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            println!("  {:10} = {:30} | Readers: {readers}", a.key, a.value);
            println!("    Rationale: {}", a.explanation);
        }
    }

    if let Some(elf) = &probe.elf_probe {
        println!("\n=== PS2 Boot ELF Probe ===");
        println!("Entry Point: {:#010X}", elf.entry);
        if let Some((mode, off)) = elf.video_mode {
            println!("Hard-coded Video Mode: {mode} (at offset {off:#X})");
        } else {
            println!("Hard-coded Video Mode: none detected");
        }

        println!("\nSegments ({}):", elf.segments.len());
        for (i, s) in elf.segments.iter().enumerate() {
            println!(
                "  [{i}] type={:#X} vaddr={:#010X} filesz={:#X} memsz={:#X} flags={:#X}",
                s.seg_type, s.vaddr, s.file_size, s.mem_size, s.flags
            );
        }

        println!("\nSDK Stamps ({}):", elf.sdk_stamps.len());
        for st in &elf.sdk_stamps {
            println!("  offset {:#08X}: {}", st.offset, st.stamp);
        }

        println!("\nReferenced IOP/IRX Modules ({}):", elf.referenced_modules.len());
        for m in &elf.referenced_modules {
            println!("  offset {:#08X}: {}", m.offset, m.path);
        }

        println!("\nVersion Gates ({}):", elf.version_gates.len());
        for g in &elf.version_gates {
            println!(
                "  {} >= {:#06X} (at va {:#010X}, offset {:#X})",
                g.module, g.min_version, g.vaddr, g.offset
            );
        }

        println!("\nEE Syscalls Called ({}):", elf.called_syscalls.len());
        for sc in &elf.called_syscalls {
            let name = sc.name.unwrap_or("unknown");
            println!(
                "  sys {:4} ({:#04X}) {:25} : called {:3} time(s) (stub {:#010X})",
                sc.number, sc.number, name, sc.call_count, sc.stub_vaddr
            );
        }

        println!("\nEE Syscalls Uncalled / Linked only ({}):", elf.uncalled_syscalls.len());
        for sc in &elf.uncalled_syscalls {
            let name = sc.name.unwrap_or("unknown");
            println!(
                "  sys {:4} ({:#04X}) {:25} (stub {:#010X})",
                sc.number, sc.number, name, sc.stub_vaddr
            );
        }

        if !elf.cdvd_usage.is_empty() {
            println!("\nLibcdvd Usage:");
            for u in &elf.cdvd_usage {
                println!("  {:20} : {}", u.name, u.details);
            }
        }
    }

    if let Some(ioprp) = &probe.ioprp_report {
        println!("\n=== IOPRP Image Modules ===");
        if let Some(comment) = &ioprp.romdir_comment {
            println!("ROMDIR Comment: {comment}");
        }
        for m in &ioprp.modules {
            let ver = m.version.as_deref().unwrap_or("-");
            let date = m.date.as_deref().unwrap_or("-");
            let comment = m.comment.as_deref().unwrap_or("");
            println!("  {:12} size={:#8X} ver={:6} date={:10} {comment}", m.name, m.size, ver, date);
        }
    }

    if !probe.irx_modules.is_empty() {
        println!("\n=== Disc IRX Modules ===");
        for irx in &probe.irx_modules {
            println!("  {:20} version {}", irx.module_name, irx.version_string);
        }
    }

    if let Some(ps1) = &probe.ps1_probe {
        println!("\n=== PS1 Executable Probe ===");
        println!("Outcome: {}", ps1.check.outcome);
        println!("PC0: {:#010X}", ps1.check.pc0);
        println!("GP0: {:#010X}", ps1.check.gp0);
        println!("Text Addr: {:#010X}, Text Size: {:#X}", ps1.check.t_addr, ps1.check.t_size);
        if let Some(sp) = ps1.check.sp {
            println!("Initial SP: {sp:#010X}");
        }
        println!("Sector Aligned: {}", ps1.check.t_size_sector_aligned);
        println!("Dead ROM Probe Present: {}", ps1.dead_rom_probe_present);

        println!("\nLibrary Stamps ({}):", ps1.stamps.len());
        for st in &ps1.stamps {
            println!("  offset {:#08X}: {}", st.offset, st.stamp);
        }

        println!("\nBIOS Calls Used ({}):", ps1.bios_calls.len());
        for bc in &ps1.bios_calls {
            let name = bc.name.unwrap_or("unknown");
            println!(
                "  {:02X}:{:02X} {:25} (at offset {:#X})",
                bc.table, bc.function, name, bc.offset
            );
        }
    }
}
