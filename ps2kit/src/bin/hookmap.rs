//! `hookmap <bios> [--card f.ps2] [--nvm f.nvm] [--disc f]`.

use ps2kit::disc::DiscImage;
use ps2kit::hooks::{HookMap, Verdict};
use ps2kit::memcard::MemoryCard;
use ps2kit::nvm::{BiosSet, BiosSetPaths};
use ps2kit::rom::RomDir;
use std::path::PathBuf;

const USAGE: &str = "usage: hookmap <bios> [--card f.ps2] [--nvm f.nvm] [--disc f]";

fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() == 1 && (args[0] == "--help" || args[0] == "-h") { println!("{USAGE}"); return }
    let Some(bios) = args.first().filter(|s| !s.to_string_lossy().starts_with('-')) else {
        eprintln!("{USAGE}"); std::process::exit(2);
    };
    let (mut card, mut nvm, mut disc) = (None, None, None);
    for pair in args[1..].chunks(2) {
        if pair.len() != 2 { eprintln!("{USAGE}"); std::process::exit(2) }
        let target = match pair[0].to_str() { Some("--card") => &mut card, Some("--nvm") => &mut nvm,
            Some("--disc") => &mut disc, _ => { eprintln!("{USAGE}"); std::process::exit(2) } };
        if target.is_some() { eprintln!("duplicate option: {}", pair[0].to_string_lossy()); std::process::exit(2) }
        *target = Some(PathBuf::from(&pair[1]));
    }
    if let Err(e) = run(PathBuf::from(bios), card, nvm, disc) { eprintln!("error: {e}"); std::process::exit(1) }
}

fn run(bios: PathBuf, card: Option<PathBuf>, nvm: Option<PathBuf>, disc: Option<PathBuf>) -> ps2kit::Result<()> {
    let mut paths = BiosSetPaths::discover(bios)?;
    if let Some(nvm) = nvm { paths.nvm = Some(nvm); }
    let set = BiosSet::inspect_paths(paths)?;
    let rom = RomDir::new(std::fs::read(&set.paths.bios)?)?;
    let card = card.map(MemoryCard::open).transpose()?;
    let disc = disc.map(DiscImage::open).transpose()?;
    let map = HookMap::inspect_with_bios_set(&rom, card.as_ref(), None, disc.as_ref(), &set);
    println!("ROMVER: {} | BIOS: {}", set.romver.text, set.paths.bios.display());
    println!("NVM: {} | ROM1: {}", set.paths.nvm.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "not supplied".into()),
        set.paths.rom1.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "absent from companion set".into()));
    println!("Ordinary cold boot; Fires means file probe/setting gate satisfied, with load success unverified. One supplied card; other port unknown.");
    println!("Source addresses name research reference builds; evidence addresses come from this ROM.");
    for diagnostic in &map.diagnostics { println!("Evidence gap: {diagnostic}"); }
    for finding in &set.findings {
        if finding.message.contains("cannot read companion") || finding.message.contains("malformed companion") {
            println!("Companion: {} [{}]", finding.message, finding.citation);
        }
    }
    for verdict in [Verdict::Fires, Verdict::DoesNotFire, Verdict::Unknown] {
        println!("\n{verdict}:");
        for f in map.findings.iter().filter(|f| f.verdict == verdict) {
            println!("  {}: {} [{}]", f.hook, f.because, f.source);
        }
    }
    Ok(())
}
