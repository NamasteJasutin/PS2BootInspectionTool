//! usage: ps2history <card.ps2 | folder-card> [ls [dir] | cat <file>]
//!        ps2history --bios <bios image>          (check that the opening's data can be read)
use ps2kit::history::PlayHistory;
use ps2kit::memcard::MemoryCard;
use std::io::Write;
use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Err(e) = run(&args) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn run(args: &[String]) -> ps2kit::Result<()> {
    let usage = "usage: ps2history <memory card image or folder> [ls [dir] | cat <file>]\n       ps2history --bios <bios image>";
    let Some(first) = args.get(1) else {
        eprintln!("{usage}");
        std::process::exit(2);
    };
    if first == "--bios" {
        let path = args.get(2).unwrap_or_else(|| { eprintln!("missing BIOS path"); std::process::exit(2) });
        let rom = ps2kit::rom::RomDir::new(std::fs::read(path)?)?;
        let assets = ps2kit::bios::OpeningAssets::load(&rom)?;
        println!("ROM {}: {} textures, {} history records x 6 slots", assets.rom_version, assets.textures.len(), assets.slots.len());
        let mut names: Vec<_> = assets.textures.values().collect();
        names.sort_by(|a, b| a.name.cmp(&b.name));
        for t in names {
            println!("  {:9} {}x{}", t.name, t.width, t.height);
        }
        return Ok(());
    }
    let card = MemoryCard::open(Path::new(first))?;
    match args.get(2).map(String::as_str) {
        Some("ls") => {
            let path: Vec<&str> = args.get(3).map(|p| p.split('/').filter(|s| !s.is_empty()).collect()).unwrap_or_default();
            for e in card.list(&path)? {
                println!("{} {:<8} {}", if e.is_directory { "d" } else { "-" }, e.length, e.name);
            }
        }
        Some("cat") => {
            let path: Vec<&str> = args[3].split('/').filter(|s| !s.is_empty()).collect();
            std::io::stdout().write_all(&card.read_file(&path)?)?;
        }
        _ => {
            let history = PlayHistory::from_card(&card)?;
            println!("history from {}", history.source.as_deref().unwrap_or("?"));
            println!(" #  title             count  towers  growing  last played");
            for (i, r) in history.records.iter().enumerate().filter(|(_, r)| !r.is_empty()) {
                let towers: String = (0..6).map(|k| if r.mask >> k & 1 == 1 { '#' } else { '.' }).collect();
                let growing = if r.index == 7 { "-".into() } else { r.index.to_string() };
                println!("{i:2}  {:<16}  {:5}  {towers}  {growing:>7}  {:04}-{:02}-{:02}", r.name, r.count, r.year(), r.month(), r.day());
            }
            if history.records.iter().all(Record::is_empty) {
                println!("(all 21 records are empty)");
            }
        }
    }
    Ok(())
}

use ps2kit::history::Record;
