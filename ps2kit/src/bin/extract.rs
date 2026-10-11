//! Extract validated native sounds from caller-owned disc images.
use ps2kit::extract::{AssetKind, Extractor};
use ps2kit::sectors::is_disc_image;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

fn walk(folder: &Path, images: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(folder)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            walk(&entry.path(), images)?;
        } else if entry.file_type()?.is_file() && is_disc_image(entry.path()) {
            images.push(entry.path());
        }
    }
    Ok(())
}
fn images(folder: &Path) -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
    let mut paths = Vec::new();
    walk(folder, &mut paths)?;
    paths.sort();
    let mut covered = HashSet::new();
    let identity = |p: &Path| p.to_string_lossy().to_ascii_lowercase();
    for cue in paths
        .iter()
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("cue")))
    {
        let parent = cue.parent().unwrap_or(Path::new("."));
        let text = std::fs::read_to_string(cue)?;
        for line in text.lines() {
            let line = line.trim();
            if let Some(rest) = line
                .get(..5)
                .filter(|s| s.eq_ignore_ascii_case("FILE "))
                .map(|_| &line[5..])
            {
                let name = if let Some(quoted) = rest.trim().strip_prefix('"') {
                    quoted.split('"').next().unwrap_or("")
                } else {
                    rest.split_whitespace().next().unwrap_or("")
                };
                covered.insert(identity(&parent.join(name)));
            }
        }
        // SectorReader also accepts renamed rips; deduplicate its fallback BIN.
        for path in paths.iter().filter(|p| {
            p.parent() == cue.parent()
                && p.extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("bin") || e.eq_ignore_ascii_case("img"))
        }) {
            let name = path
                .file_name()
                .map(|s| s.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default();
            if path.file_stem() == cue.file_stem() || name.contains("track 01") || name.contains("track01") {
                covered.insert(identity(path));
            }
        }
    }
    paths.retain(|p| {
        if covered.contains(&identity(p)) {
            return false;
        }
        // Multi-file rips may have lost their CUE. Ignore later tracks only
        // when they have no ISO tree; the supported Tekken tracks are CD-DA.
        let name = p
            .file_stem()
            .map(|s| s.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        if p.extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("bin") || e.eq_ignore_ascii_case("img"))
        {
            if let Some(at) = name.find("(track ") {
                let digits: String = name[at + 7..].chars().take_while(|c| c.is_ascii_digit()).collect();
                if digits.parse::<usize>().is_ok_and(|n| n > 1) && ps2kit::probe::DiscFiles::open(p).is_err() {
                    return false;
                }
            }
        }
        true
    });
    Ok(paths)
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let mut all = false;
    let mut input = None;
    let mut out = PathBuf::from("Soundfonts");
    let mut at = 0;
    while at < args.len() {
        if args[at] == "--all" {
            all = true;
        } else if args[at] == "--out" {
            at += 1;
            out = PathBuf::from(args.get(at).ok_or("missing --out directory")?);
        } else if input.is_none() && !args[at].to_string_lossy().starts_with('-') {
            input = Some(PathBuf::from(&args[at]));
        } else {
            return Err("usage: extract <image> [--out <dir>] | extract --all <folder> [--out <dir>]".into());
        }
        at += 1;
    }
    let input = input.ok_or("usage: extract <image> [--out <dir>] | extract --all <folder> [--out <dir>]")?;
    let paths = if all { images(&input)? } else { vec![input] };
    let mut failed = 0;
    for path in &paths {
        let result = (|| -> ps2kit::Result<()> {
            let extraction = Extractor::scan(path)?;
            println!(
                "{} [{}] ({})",
                extraction.title, extraction.title_id, extraction.console
            );
            for kind in AssetKind::ALL {
                let c = extraction.counts(kind);
                println!(
                    "  {:5}: found {} / validated {} / rejected {} / unique {}",
                    kind.name(),
                    c.found,
                    c.validated,
                    c.rejected,
                    c.unique
                );
            }
            for hit in &extraction.rejected {
                eprintln!(
                    "  rejected {} {} +{:#x}: {}",
                    hit.kind.name(),
                    hit.file,
                    hit.offset,
                    hit.reason
                );
            }
            for diagnostic in &extraction.diagnostics {
                eprintln!("  {diagnostic}");
            }
            let set = extraction.write(&out)?;
            println!("  {} assets → {}", set.assets, set.folder.display());
            Ok(())
        })();
        if let Err(e) = result {
            failed += 1;
            eprintln!("{}: {e}", path.display());
        }
    }
    if failed != 0 {
        return Err(format!("{failed}/{} disc extractions failed", paths.len()).into());
    }
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
