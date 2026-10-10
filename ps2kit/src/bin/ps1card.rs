//! CLI tool for inspecting PlayStation 1 memory card images.
//!
//! # Usage
//! ```text
//! ps1card <file.mcr | file.gme> [--icons <dir>]
//! ```
//!
//! Lists active saves, block allocation chains, decoded Shift-JIS titles, icon frame counts,
//! deleted saves evaluated as candidates for the shell's hidden L1+L2+R1+R2 undelete
//! (`notes/research/hidden_input.md` §6), and decoded SoundScope performances
//! (`notes/research/hidden_input.md` §8–§9).

use ps2kit::ps1card::{Ps1Card, GME_MAGIC};
use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Err(e) = run(&args) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn print_usage() {
    println!("usage: ps1card <file.mcr | file.gme> [--icons <dir>]");
    println!("  --icons <dir>   Export icon frames to <dir> as PPM images");
}

fn run(args: &[String]) -> ps2kit::Result<()> {
    if args.len() < 2 {
        print_usage();
        std::process::exit(2);
    }

    let mut file_path: Option<&str> = None;
    let mut icons_dir: Option<&str> = None;

    let mut iter = args.iter().skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--icons" => {
                let Some(dir) = iter.next() else {
                    eprintln!("error: --icons requires a directory path");
                    std::process::exit(2);
                };
                icons_dir = Some(dir.as_str());
            }
            "--help" | "-h" => {
                print_usage();
                return Ok(());
            }
            path if !path.starts_with("--") && file_path.is_none() => {
                file_path = Some(path);
            }
            other => {
                eprintln!("unknown argument: {other}");
                print_usage();
                std::process::exit(2);
            }
        }
    }

    let Some(path_str) = file_path else {
        print_usage();
        std::process::exit(2);
    };

    let path = Path::new(path_str);
    let bytes = std::fs::read(path)?;
    let card = Ps1Card::parse(&bytes)?;

    let is_gme = bytes.starts_with(GME_MAGIC);
    println!("PS1 Memory Card Image: {}", path.display());
    println!(
        "Format:            {}",
        if is_gme { "InterAct DexDrive (.gme, 3904-byte container)" } else { "Raw 128 KiB memory card image" }
    );
    println!("Total file size:   {} bytes", bytes.len());
    println!(
        "Header checksum:   {} (Frame 0)",
        if card.header_checksum_valid { "Valid (OK)" } else { "Mismatch (CORRUPT)" }
    );
    println!(
        "Directory frames:  {}",
        if card.directory_checksums_valid { "All 15 frame checksums valid (OK)" } else { "Some checksums invalid" }
    );
    println!("Broken frames:     {}", card.broken_frames.len());

    // Active saves
    println!("\n# Active Saves ({} saves):", card.saves.len());
    if card.saves.is_empty() {
        println!("  (no active saves on card)");
    } else {
        println!(
            "{:<4}  {:<12}  {:<8}  {:<10}  {:<8}  {:<6}  {:<32}  Filename",
            "#", "Product", "Region", "Blocks", "Size", "Icons", "Title (Shift-JIS)"
        );
        for (i, save) in card.saves.iter().enumerate() {
            let blocks_str = format!("{:?}", save.blocks);
            let region_str = save.region.map(|r| r.to_string()).unwrap_or_else(|| "-".into());
            let product_str = if save.product.is_empty() { "-" } else { &save.product };
            println!(
                "{:<4}  {:<12}  {:<8}  {:<10}  {:<8}  {:<6}  {:<32}  {}",
                i + 1,
                product_str,
                region_str,
                blocks_str,
                format!("{} B", save.file_size),
                format!("{} (flag {:#x})", save.icon_frames.len(), save.icon_flag),
                save.title,
                save.filename
            );
        }
    }

    // Deleted entries & undelete candidates
    let deleted = card.deleted();
    println!("\n# Deleted Entries & Undelete Candidates ({} entries):", deleted.len());
    if deleted.is_empty() {
        println!("  (no deleted entries on card)");
    } else {
        for (i, cand) in deleted.iter().enumerate() {
            let status = if cand.recoverable {
                let trans: Vec<String> = cand
                    .state_transitions
                    .iter()
                    .map(|(b, old_s, new_s)| format!("blk {b} (0x{old_s:02X} -> 0x{new_s:02X})"))
                    .collect();
                format!("Recoverable ({})", trans.join(", "))
            } else {
                format!("Not recoverable ({})", cand.failure_reason.as_deref().unwrap_or("unknown"))
            };

            println!(
                "  [{}] \"{}\" (blocks {:?}, {} bytes): {}",
                i + 1,
                if cand.save.title.is_empty() { &cand.save.filename } else { &cand.save.title },
                cand.save.blocks,
                cand.save.file_size,
                status
            );
        }
    }
    println!("  Note on PS1 shell undelete (hidden_input.md §6):");
    println!("  The retail shell remembers only the single last-deleted filename in that session (RAM 0x80086208/0x80090CF0).");
    println!("  Candidates above are evaluated assuming each was the last deletion.\n");

    // SoundScope performances
    let perfs = card.soundscope_performances();
    println!("# SoundScope Performances ({} files):", perfs.len());
    if perfs.is_empty() {
        println!("  (no BPLAYSTATION performance records found)");
    } else {
        for (i, p) in perfs.iter().enumerate() {
            println!(
                "  [{}] BPLAYSTATION: ver {:#010x} ({}), checksum {:#010x} (calculated {:#010x}, {}), {} events, initial scale {:#x}, variant {:#x}",
                i + 1,
                p.version,
                if p.version == 0x0001_0000 { "valid" } else { "unexpected" },
                p.checksum,
                p.calculated_checksum,
                if p.checksum_valid { "valid" } else { "MISMATCH" },
                p.events.len(),
                p.initial_scale,
                p.initial_variant
            );
            println!(
                "      Duration: {} fields (~{:.2}s at 60 Hz)",
                p.duration_fields(),
                p.duration_fields() as f32 / 60.0
            );
            if !p.events.is_empty() {
                println!("      First events (up to 5):");
                for (ei, e) in p.events.iter().take(5).enumerate() {
                    let preset_str = e.preset.map(|pr| format!("preset {pr}")).unwrap_or_else(|| "-".into());
                    println!(
                        "        {}: field {}, duration {}, buttons {:#06x} ({})",
                        ei + 1, e.field, e.duration, e.buttons, preset_str
                    );
                }
            }
        }
    }

    // Export icons if requested
    if let Some(dir_str) = icons_dir {
        export_icons(&card, Path::new(dir_str))?;
    }

    Ok(())
}

fn export_icons(card: &Ps1Card, dir: &Path) -> ps2kit::Result<()> {
    std::fs::create_dir_all(dir)?;
    println!(
        "\nWriting icons as PPM (Netpbm P6) because ps2kit does not depend on the image crate."
    );

    let mut total_exported = 0;
    for save in &card.saves {
        let base_name = if !save.product.is_empty() {
            &save.product
        } else if !save.filename.is_empty() {
            &save.filename
        } else {
            "save"
        };
        let sanitized: String = base_name
            .chars()
            .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
            .collect();

        for (fi, frame) in save.icon_frames.iter().enumerate() {
            let out_name = format!("{sanitized}_frame{fi}.ppm");
            let out_path = dir.join(out_name);
            write_ppm(&out_path, frame)?;
            total_exported += 1;
        }
    }

    println!("Exported {} icon frame(s) to {}.", total_exported, dir.display());
    Ok(())
}

fn write_ppm(path: &Path, rgba: &[u8; 16 * 16 * 4]) -> std::io::Result<()> {
    use std::io::Write;
    let mut data = Vec::with_capacity(16 * 16 * 3 + 32);
    write!(&mut data, "P6\n16 16\n255\n")?;
    for pixel in rgba.chunks_exact(4) {
        data.push(pixel[0]);
        data.push(pixel[1]);
        data.push(pixel[2]);
    }
    std::fs::write(path, data)
}
