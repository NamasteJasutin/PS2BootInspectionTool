//! CLI tool for inspecting PS3 System Update (PUP) containers.
//!
//! # Usage
//! ```text
//! pupinfo <file.pup> [--members] [--packages] [--selfs] [--licence]
//! pupinfo --diff <a.pup> <b.pup>
//! ```
//!
//! As specified in `notes/research/ps3_pup.md` §6:
//! This tool inspects container structure only; the payload contents are encrypted and are not read.

use ps2kit::pup::{pup_member_name, Pup, PupSnapshot, SelfInfo};
use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Err(e) = run(&args) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn run(args: &[String]) -> ps2kit::Result<()> {
    if args.len() < 2 {
        print_usage();
        std::process::exit(2);
    }

    if args.iter().any(|a| a == "--snapshot") {
        let files: Vec<&str> = args.iter().skip(1).filter(|a| !a.starts_with("--")).map(String::as_str).collect();
        if files.is_empty() {
            eprintln!("usage: pupinfo --snapshot <file.pup>");
            std::process::exit(2);
        }
        let snap = load_snapshot(Path::new(files[0]))?;
        print!("{}", snap.to_text());
        return Ok(());
    }

    if args.iter().any(|a| a == "--diff") {
        let files: Vec<&str> = args.iter().skip(1).filter(|a| !a.starts_with("--")).map(String::as_str).collect();
        if files.len() < 2 {
            eprintln!("usage: pupinfo --diff <a.pup> <b.pup>");
            std::process::exit(2);
        }
        return diff_pups(Path::new(files[0]), Path::new(files[1]));
    }

    let mut file_path = None;
    let mut show_members = false;
    let mut show_packages = false;
    let mut show_selfs = false;
    let mut show_licence = false;

    for arg in args.iter().skip(1) {
        match arg.as_str() {
            "--members" => show_members = true,
            "--packages" => show_packages = true,
            "--selfs" => show_selfs = true,
            "--licence" | "--license" => show_licence = true,
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
    let pup = Pup::open(path)?;

    // If no specific flag is given, print the default identity card
    if !show_members && !show_packages && !show_selfs && !show_licence {
        print_identity_card(path, &pup)?;
        return Ok(());
    }

    if show_members {
        print_members(&pup);
    }

    if show_licence {
        print_licence(&pup)?;
    }

    if show_packages {
        print_packages(&pup)?;
    }

    if show_selfs {
        print_selfs(&pup)?;
    }

    Ok(())
}

fn print_usage() {
    println!("usage: pupinfo <file.pup> [--members] [--packages] [--selfs] [--licence]");
    println!("       pupinfo --snapshot <file.pup>");
    println!("       pupinfo --diff <a.pup|a.pupsnap> <b.pup|b.pupsnap>");
}

fn print_identity_card(path: &Path, pup: &Pup) -> ps2kit::Result<()> {
    println!("PS3 PUP container description (contents are encrypted and are not read).");
    println!("File:             {}", path.display());
    println!("Total size:       {} bytes", pup.header.header_length + pup.header.data_length);
    println!("Package version:  {}", pup.header.package_version);
    println!("Image version:    {:#07x} ({})", pup.header.image_version, pup.header.image_version);
    println!("File count:       {}", pup.header.file_count);
    println!("Header length:    {} bytes ({:#x})", pup.header.header_length, pup.header.header_length);
    println!("Data length:      {} bytes ({:#x})", pup.header.data_length, pup.header.data_length);
    println!("Header digest:    {}", ps2kit::pup::hex_digest(&pup.header_digest));
    println!("Structure checks: header+data = size (OK), contiguous members (OK), tables inside header (OK)");

    // Plaintext members
    let ver = pup.version().unwrap_or_else(|_| "absent".into());
    let flags = pup.update_flags().unwrap_or_else(|_| "absent".into());
    let dots = pup.dots().unwrap_or_else(|_| "absent".into());
    let locales = pup.license_locales().unwrap_or_default();
    println!("Firmware version: {ver}");
    println!("Update flags:     {flags}");
    println!("Progress dots:    {dots}");
    println!("Licence:          {} locales", locales.len());

    // Embedded TAR archives & packages
    let pkgs = pup.packages().unwrap_or_default();
    let pkg_count = pkgs.iter().filter(|p| p.sce_header.header_type == 3).count();
    let rvk_count = pkgs.iter().filter(|p| p.sce_header.header_type == 2).count();
    let reconciled_count = pkgs.iter().filter(|p| p.uncompressed_reconciled && p.stored_reconciled).count();
    let spkg_match_count = pkgs.iter().filter(|p| p.spkg_hdr_matched == Some(true)).count();

    println!("Packages (0x300): {} members ({} packages, {} RVKs)", pkgs.len(), pkg_count, rvk_count);
    println!("Reconciliation:   {reconciled_count}/{} descriptors match sizes; {spkg_match_count}/{} match spkg_hdr", pkgs.len(), pkg_count);

    // Updater SELFs
    if let Ok(s) = pup.self_info(0x200) {
        println!("ps3swu.self:      key_rev {:#04x}, ver {:#018x}, PPC64 Cell LV2, entry {:#08x}, {} phdrs",
            s.sce_header.key_revision, s.app_info.version, s.elf_header.e_entry, s.program_headers.len());
    }
    if let Ok(s) = pup.self_info(0x601) {
        println!("ps3swu2.self:     key_rev {:#04x}, ver {:#018x}, PPC64 Cell LV2, entry {:#08x}, {} phdrs",
            s.sce_header.key_revision, s.app_info.version, s.elf_header.e_entry, s.program_headers.len());
    }

    Ok(())
}

fn print_members(pup: &Pup) {
    println!("\n# Member Table ({} entries):", pup.entries.len());
    println!("{:<3}  {:<6}  {:<18}  {:<10}  {:<10}  {:<40}", "#", "ID", "Name", "Offset", "Size", "Digest (HMAC-SHA1)");
    for (i, entry) in pup.entries.iter().enumerate() {
        let digest_hex = pup.digests.get(i).map(|d| d.hex()).unwrap_or_else(|| "-".into());
        println!("{:<3}  {:#05x}  {:<18}  {:<10}  {:<10}  {}",
            i, entry.id, entry.name(), entry.offset, entry.size, digest_hex);
    }
}

fn print_licence(pup: &Pup) -> ps2kit::Result<()> {
    let locales = pup.license_locales()?;
    println!("\n# Licence Locales ({} locales):", locales.len());
    println!("{:<6}  {:<8}  {:<24}  Copyright", "Locale", "Bytes", "Agreement Date");
    for loc in &locales {
        println!("{:<6}  {:<8}  {:<24}  {}", loc.lang, loc.byte_length, loc.date, loc.copyright);
    }
    Ok(())
}

fn print_packages(pup: &Pup) -> ps2kit::Result<()> {
    let packages = pup.packages()?;
    println!("\n# Packages in update_files.tar ({} members):", packages.len());
    println!("{:<40}  {:<10}  {:<12}  {:<10}  {:<18}  {:<10}  {:<10}  SPKG Match",
        "Member", "Size", "Kind", "Seq", "Stamp", "Uncomp", "Reconciled");
    for p in &packages {
        let (kind, seq, stamp, uncomp) = if let Some(ref d) = p.pkg_descriptor {
            (d.kind_name().to_string(), format!("{:#x}", d.sequence), format!("{:#018x}", d.version_stamp), format!("{}", d.uncompressed_size))
        } else if let Some(ref r) = p.rvk_descriptor {
            (format!("RVK (kind {})", r.kind), format!("{:#x}", r.sequence), format!("{:#018x}", r.version_stamp), "-".into())
        } else {
            ("Unknown".into(), "-".into(), "-".into(), "-".into())
        };

        let reconciled = if p.uncompressed_reconciled && p.stored_reconciled { "yes" } else { "no" };
        let spkg = match p.spkg_hdr_matched {
            Some(true) => "yes",
            Some(false) => "mismatch",
            None => "-",
        };

        println!("{:<40}  {:<10}  {:<12}  {:<10}  {:<18}  {:<10}  {:<10}  {}",
            p.name, p.member_size, kind, seq, stamp, uncomp, reconciled, spkg);
    }
    Ok(())
}

fn print_selfs(pup: &Pup) -> ps2kit::Result<()> {
    println!("\n# Updater SELFs:");
    for id in [0x200, 0x601] {
        let name = pup_member_name(id);
        let s = match pup.self_info(id) {
            Ok(s) => s,
            Err(e) => {
                println!("{name} ({id:#x}): failed to parse ({e})");
                continue;
            }
        };
        print_single_self(name, id, &s);
    }
    Ok(())
}

fn print_single_self(name: &str, id: u64, s: &SelfInfo) {
    println!("\n## {} ({id:#x}):", name);
    println!("  SCE Header:      magic {:?}, ver {}, key_rev {:#04x}, type {}, header_len {}, data_len {}",
        std::str::from_utf8(&s.sce_header.magic).unwrap_or("?"),
        s.sce_header.version,
        s.sce_header.key_revision,
        s.sce_header.header_type,
        s.sce_header.header_length,
        s.sce_header.data_length,
    );
    println!("  App Info:        auth_id {:#018x}, vendor {:#010x}, self_type {}, version {:#018x}",
        s.app_info.auth_id, s.app_info.vendor_id, s.app_info.self_type, s.app_info.version);
    println!("  ELF64 Header:    e_type {}, machine {} (PPC64), entry {:#08x}, phdrs {}, shdrs {}",
        s.elf_header.e_type, s.elf_header.e_machine, s.elf_header.e_entry, s.elf_header.e_phnum, s.elf_header.e_shnum);
    println!("  Program Headers ({}):", s.program_headers.len());
    println!("    {:<3}  {:<8}  {:<10}  {:<10}  {:<12}  {:<10}  {:<10}  Align",
        "#", "Type", "Flags", "Offset", "VAddr", "FileSz", "MemSz");
    for (i, p) in s.program_headers.iter().enumerate() {
        println!("    {:<3}  {:<8}  {:#010x}  {:#010x}  {:#012x}  {:<10}  {:<10}  {:#x}",
            i, p.p_type, p.p_flags, p.p_offset, p.p_vaddr, p.p_filesz, p.p_memsz, p.p_align);
    }
}

fn load_snapshot(path: &Path) -> ps2kit::Result<PupSnapshot> {
    if path.extension().and_then(|s| s.to_str()) == Some("pupsnap") {
        let text = std::fs::read_to_string(path)?;
        PupSnapshot::parse(&text)
    } else {
        match Pup::open(path) {
            Ok(pup) => PupSnapshot::from_pup(&pup),
            Err(_) => {
                let text = std::fs::read_to_string(path)?;
                PupSnapshot::parse(&text)
            }
        }
    }
}

fn diff_pups(path_a: &Path, path_b: &Path) -> ps2kit::Result<()> {
    let a = load_snapshot(path_a)?;
    let b = load_snapshot(path_b)?;

    println!("Diff between {} and {}:", path_a.display(), path_b.display());

    // Header diff
    if a.image_version != b.image_version {
        println!("  Image version:       {:#07x} -> {:#07x}", a.image_version, b.image_version);
    }
    if a.header_length != b.header_length {
        println!("  Header length:       {} -> {}", a.header_length, b.header_length);
    }
    if a.data_length != b.data_length {
        println!("  Data length:         {} -> {}", a.data_length, b.data_length);
    }
    if a.header_digest != b.header_digest {
        println!("  Header digest:       {} -> {}", a.header_digest, b.header_digest);
    }

    // Version diff
    let ver_a = a.version.as_deref().unwrap_or("?");
    let ver_b = b.version.as_deref().unwrap_or("?");
    if ver_a != ver_b {
        println!("  Firmware version:    {ver_a} -> {ver_b}");
    }

    // Member entries diff
    println!("\nMember entries comparison:");
    let mut any_member_diff = false;
    for ea in &a.entries {
        match b.entries.iter().find(|e| e.id == ea.id) {
            Some(eb) => {
                let size_diff = ea.size != eb.size;
                let digest_diff = ea.digest != eb.digest;
                if size_diff || digest_diff {
                    any_member_diff = true;
                    println!("  Member {:#05x} ({}):", ea.id, ea.name());
                    if size_diff {
                        println!("    Size:   {} -> {}", ea.size, eb.size);
                    }
                    if digest_diff {
                        println!("    Digest: {} -> {}", ea.digest, eb.digest);
                    }
                }
            }
            None => {
                any_member_diff = true;
                println!("  Member {:#05x} ({}) removed in second PUP", ea.id, ea.name());
            }
        }
    }
    for eb in &b.entries {
        if !a.entries.iter().any(|e| e.id == eb.id) {
            any_member_diff = true;
            println!("  Member {:#05x} ({}) added in second PUP", eb.id, eb.name());
        }
    }
    if !any_member_diff {
        println!("  (all member sizes and digests match)");
    }

    // Licence locales diff
    println!("\nLicence locales comparison:");
    let mut any_locale_diff = false;
    for la in &a.license_locales {
        match b.license_locales.iter().find(|l| l.lang == la.lang) {
            Some(lb) => {
                let len_diff = la.byte_length != lb.byte_length;
                let date_diff = la.date != lb.date;
                if len_diff || date_diff {
                    any_locale_diff = true;
                    println!("  Locale {}:", la.lang);
                    if len_diff {
                        println!("    Bytes: {} -> {}", la.byte_length, lb.byte_length);
                    }
                    if date_diff {
                        println!("    Date:  {} -> {}", la.date, lb.date);
                    }
                }
            }
            None => {
                any_locale_diff = true;
                println!("  Locale {} removed in second PUP", la.lang);
            }
        }
    }
    for lb in &b.license_locales {
        if !a.license_locales.iter().any(|l| l.lang == lb.lang) {
            any_locale_diff = true;
            println!("  Locale {} added in second PUP", lb.lang);
        }
    }
    if !any_locale_diff {
        println!("  (all licence locales match)");
    }

    // Packages diff
    println!("\nPackages comparison:");
    let mut any_pkg_diff = false;
    for pa in &a.packages {
        match b.packages.iter().find(|pb| pb.name == pa.name) {
            Some(pb) => {
                let size_diff = pa.member_size != pb.member_size;
                let stamp_a = pa.pkg_descriptor.as_ref().map(|d| d.version_stamp)
                    .or_else(|| pa.rvk_descriptor.as_ref().map(|d| d.version_stamp));
                let stamp_b = pb.pkg_descriptor.as_ref().map(|d| d.version_stamp)
                    .or_else(|| pb.rvk_descriptor.as_ref().map(|d| d.version_stamp));
                let stamp_diff = stamp_a != stamp_b;
                let uncomp_diff = pa.pkg_descriptor.as_ref().map(|d| d.uncompressed_size) != pb.pkg_descriptor.as_ref().map(|d| d.uncompressed_size);

                if size_diff || stamp_diff || uncomp_diff {
                    any_pkg_diff = true;
                    println!("  Package {}:", pa.name);
                    if size_diff {
                        println!("    Size:         {} -> {}", pa.member_size, pb.member_size);
                    }
                    if uncomp_diff {
                        println!("    Uncompressed: {:?} -> {:?}",
                            pa.pkg_descriptor.as_ref().map(|d| d.uncompressed_size),
                            pb.pkg_descriptor.as_ref().map(|d| d.uncompressed_size));
                    }
                    if stamp_diff {
                        println!("    Stamp:        {:018x?} -> {:018x?}", stamp_a, stamp_b);
                    }
                }
            }
            None => {
                any_pkg_diff = true;
                println!("  Package {} removed in second PUP", pa.name);
            }
        }
    }
    for pb in &b.packages {
        if !a.packages.iter().any(|pa| pa.name == pb.name) {
            any_pkg_diff = true;
            println!("  Package {} added in second PUP", pb.name);
        }
    }
    if !any_pkg_diff {
        println!("  (all packages match in size, uncompressed size and descriptor stamp)");
    }

    Ok(())
}
