//! List the runtime BIOS sound catalogue and export an imported arrangement.
use ps2kit::library::SoundLibrary;
use std::path::{Path, PathBuf};

fn files_in(dir: &Path, files: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_dir() {
            files_in(&entry.path(), files)?;
        } else if kind.is_file() {
            files.push(entry.path());
        }
    }
    Ok(())
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let Some(dir) = args.first().filter(|d| !d.starts_with('-')) else {
        return Err("usage: library <bios dir> [--piece <name> --out <file.psxsong>]".into());
    };
    let (mut piece, mut out) = (None, None);
    let mut at = 1;
    while at < args.len() {
        let value = args.get(at + 1).ok_or("missing option value")?;
        match args[at].as_str() {
            "--piece" => piece = Some(value),
            "--out" => out = Some(value),
            _ => return Err(format!("unknown option {}", args[at]).into()),
        }
        at += 2;
    }
    if piece.is_some() != out.is_some() {
        return Err("--piece and --out must be supplied together".into());
    }
    let mut files = Vec::new();
    files_in(Path::new(dir), &mut files)?;
    let library = SoundLibrary::scan(&files);
    println!(
        "{} banks; {} playable choices; {} pieces",
        library.banks.len(),
        library.choices().len(),
        library.pieces.len()
    );
    for bank in &library.banks {
        println!(
            "{}: {} programs, {} effects, {} samples; {} sources",
            bank.key.label,
            bank.bench.programs.len(),
            bank.bench.effects.len(),
            bank.bench.samples.len(),
            bank.sources.len()
        );
        for source in &bank.sources {
            println!(
                "  {} | {}{} +{:#x} | {}",
                source.file.display(),
                source.module,
                if source.decoded { " (decoded view)" } else { "" },
                source.offset,
                source.version
            );
        }
        for variant in &bank.variants {
            println!(
                "  variant: {} ({} effects; {} sources)",
                variant.key.label,
                variant.bench.effects.len(),
                variant.sources.len()
            );
        }
    }
    for piece in &library.pieces {
        println!(
            "{}: {} | {:?}: {} | {:.6} s{} | {}",
            piece.aliases.join(" / "),
            piece.label,
            piece.evidence,
            piece.role,
            piece.length_s,
            if piece.loops { " per traversal; infinite native loop" } else { "" },
            piece.bank.label
        );
        for variant in &piece.console_timing {
            println!("  {}", variant.label);
        }
    }
    for (style, programs) in library.by_style() {
        if !programs.is_empty() {
            println!("{}:", style.name());
            for (key, program, character) in programs {
                println!(
                    "  {} entry {}: attack {:.1} ms; brightness {:.0} Hz; noise {:.3}",
                    key.label, program, character.attack_ms, character.brightness_hz, character.noisiness
                );
            }
        }
    }
    for diagnostic in &library.diagnostics {
        eprintln!("{diagnostic}");
    }
    if let (Some(name), Some(path)) = (piece, out) {
        let piece = library.piece(name).ok_or_else(|| format!("piece {name:?} not found"))?;
        std::fs::write(path, piece.to_song(&library)?.to_text()?)?;
        println!("Wrote {path}");
    }
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
