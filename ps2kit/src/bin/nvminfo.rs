//! `nvminfo <bios.bin>`: BIOS companion completeness and private NVM summaries.

use ps2kit::nvm::BiosSet;
use std::path::Path;

fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() == 1 && (args[0] == "--help" || args[0] == "-h") {
        println!("usage: nvminfo <bios.bin>");
        return;
    }
    if args.len() != 1 {
        eprintln!("usage: nvminfo <bios.bin>");
        std::process::exit(2);
    }
    if let Err(e) = run(Path::new(&args[0])) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn run(path: &Path) -> ps2kit::Result<()> {
    let set = BiosSet::inspect(path)?;
    println!("BIOS: {}", set.paths.bios.display());
    println!("ROMVER: {}", set.romver.text);
    let name = |p: &Option<std::path::PathBuf>| p.as_ref().and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "absent".into());
    println!("NVM: {} | MEC: {}", name(&set.paths.nvm), name(&set.paths.mec));
    println!("ROM1: {}", name(&set.paths.rom1));
    println!("ROM2: {} | EROM: {}", name(&set.paths.rom2), name(&set.paths.erom));
    for f in &set.findings { println!("{} [{}]", f.message, f.citation); }
    if let Some(m) = &set.mec {
        let gates = m.gates();
        let open: Vec<_> = gates.iter().filter(|g| g.enabled).map(|g| g.feature).collect();
        let closed: Vec<_> = gates.iter().filter(|g| !g.enabled).map(|g| g.feature).collect();
        println!("2.00 gates open: {} [rom1_erom_nvm.md §4]", open.join("; "));
        println!("2.00 gates closed: {} [rom1_erom_nvm.md §4]", if closed.is_empty() { "none".into() } else { closed.join("; ") });
    }
    let Some(n) = &set.nvm else { return Ok(()) };
    println!("NVM source: {:?} (occupancy heuristic); layout {} [rom1_erom_nvm.md §§3.2–3.4]", n.source, n.layout.number);
    for area in 0..3 {
        let blocks: Vec<_> = n.blocks.iter().filter(|b| b.area == area).collect();
        let bad: Vec<_> = blocks.iter().filter(|b| !b.valid()).map(|b| format!("{:#05X}: stored {:02X}, expected {:02X}{}",
            b.offset, b.stored_checksum, b.expected_checksum, if b.has_seven_bit_checksum() { " (bit 7 dropped, like PCSX2 default)" } else { "" })).collect();
        println!("Config area {area}: {}/{} valid{} [rom1_erom_nvm.md §3.1]", blocks.iter().filter(|b| b.valid()).count(), blocks.len(),
            if bad.is_empty() { String::new() } else { format!("; {}", bad.join("; ")) });
    }
    let s = &n.osd;
    println!("OSD block {:#05X}: {} decode; stored language {}, initialised {:?} [osdsys_hooks.md §4.1]",
        n.layout.config_bases[1] + 16, if n.osd_rejected { "rejected-record/PCSX2-retry" } else { "accepted" }, n.stored_osd.language_name(), n.stored_osd.initialised);
    println!("Language: {} (code {:?}); screen: {}; Digital Out: {}; video: {} [osdsys_hooks.md §4.1]",
        s.language_name(), s.language, s.screen_name(), if s.digital_out { "on" } else { "off" }, if s.component_video { "YPbPr" } else { "RGB" });
    if s.extended {
        println!("Time zone: {:+} min, index {}{}; summer time {}; time/date codes {}/{} [rom1_erom_nvm.md §3.3]",
            s.timezone_minutes, s.timezone_index, if s.timezone_index >= 0x80 { " (custom)" } else { "" }, s.summer_time, s.time_format, s.date_format);
        println!("Remote item/enabled: {}/{}; progressive: {}; unknown bit: {} [osdsys_hooks.md §4.1]",
            s.remote_item, s.remote_enabled, s.progressive, s.unknown_option);
    }
    println!("PS1 Fast/Smooth: {}/{} (bit meanings inferred); option nibbles {:?} [osdsys_hooks.md §§4.1, 4.3]", n.ps1_fast, n.ps1_smooth, n.option_nibbles);
    println!("Region parameters: {} [rom1_erom_nvm.md §3.3]", n.region_params.as_ref().map(|p| p.text.as_str())
        .unwrap_or(if n.unknown_region_params { "present, undecoded older/binary format" } else { "blank" }));
    println!("Console ID: {:?} {}; i.LINK ID: {:?} {} [rom1_erom_nvm.md §3.3]", n.console_id.state, n.console_id.masked, n.ilink_id.state, n.ilink_id.masked);
    println!("EECONF 0x60-byte cache: {} [kernel_eeload_hooks.md §1.5]", if n.eeconf_cache().is_some() { "six valid records; video bit mirrored" }
        else if (n.romver.major, n.romver.minor) < (1, 60) { "early eeconfig implementation not traced" } else { "checksum failure removes cache pointer" });
    for p in n.predictions_with_mec(set.mec.as_ref()) { println!("{} [{}]", p.message, p.citation); }
    Ok(())
}
