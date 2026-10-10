//! `romdiff <dump> <dump> [...]`: runtime identities and chronological captions.

use ps2kit::ps1bios::Ps1Bios;
use ps2kit::rom::RomDir;
use ps2kit::romdiff::{cluster_label, ChangeKind, Evidence, RomSet, RowKind, Space};
use std::collections::BTreeMap;
use std::path::PathBuf;

const USAGE: &str = "usage: romdiff <dump> <dump> [...] (PS1 or PS2, not mixed)";

fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() == 1 && (args[0] == "--help" || args[0] == "-h") { println!("{USAGE}"); return }
    if args.len() < 2 || args.iter().any(|a| a.to_string_lossy().starts_with('-')) {
        eprintln!("{USAGE}"); std::process::exit(2);
    }
    if let Err(e) = run(args.into_iter().map(PathBuf::from).collect()) { eprintln!("error: {e}"); std::process::exit(1) }
}

fn run(paths: Vec<PathBuf>) -> ps2kit::Result<()> {
    let mut files = Vec::new();
    for path in paths { files.push((path.display().to_string(), std::fs::read(path)?)); }
    let ps1 = Ps1Bios::detect(&files[0].1);
    if files.iter().any(|(_, d)| Ps1Bios::detect(d) != ps1) {
        return Err(ps2kit::Error::NotA(ps2kit::Format::Bios, "cannot mix standalone PS1 and ROMDIR dumps".into()));
    }
    let set = if ps1 {
        let inputs = files.into_iter().map(|(n, d)| Ok((n, Ps1Bios::new(d)?))).collect::<ps2kit::Result<Vec<_>>>()?;
        RomSet::from_ps1(&inputs.iter().map(|(n, d)| (n.as_str(), d.clone())).collect::<Vec<_>>())?
    } else {
        let inputs = files.into_iter().map(|(n, d)| Ok((n, RomDir::new(d)?))).collect::<ps2kit::Result<Vec<_>>>()?;
        RomSet::from_ps2(&inputs.iter().map(|(n, d)| (n.as_str(), d.clone())).collect::<Vec<_>>())?
    };
    println!("{:?} identities; letters are local to each row; '-' means absent/unavailable.", set.family);
    for (i, dump) in set.dumps.iter().enumerate() {
        let date = dump.date.map(|d| format!("{:04}-{:02}-{:02}", d.year, d.month, d.day)).unwrap_or_else(|| "undated".into());
        let kernel = dump.kernel_generation.map(|k| format!(" {k:?}")).unwrap_or_default();
        println!("D{} {date} {}{kernel} {}", i + 1, dump.version, dump.name);
    }
    println!("Matrix columns: {}", (1..=set.dumps.len()).map(|i| format!("D{i}")).collect::<Vec<_>>().join(" "));
    let mut groups: BTreeMap<(u8, String), Vec<&str>> = BTreeMap::new();
    for row in &set.rows {
        let signature = row.cells.iter().map(|c| c.as_ref().map(|c| cluster_label(c.cluster)).unwrap_or_else(|| "-".into())).collect::<Vec<_>>().join(" ");
        if matches!(row.kind, RowKind::Module | RowKind::Table) { groups.entry((u8::from(row.kind == RowKind::Module), signature)).or_default().push(&row.name); }
        else { println!("  {signature:<20} {}", row.name); }
    }
    // Group equal *patterns* to keep all module names in a compact matrix; this does not
    // assert identity between different modules (each row has its own cluster namespace).
    for ((_, pattern), names) in groups { println!("  {pattern:<20} {}", names.join(", ")); }
    for (i, dump) in set.dumps.iter().enumerate() {
        if let Some(osd) = &dump.osdsys {
            let words = osd.patches.iter().map(|p| format!("{:#x}: {:08x}→{:08x} ({}→{}; SW {})", p.target, p.old, p.new, p.old_instruction, p.new_instruction, location(&p.store))).collect::<Vec<_>>();
            println!("D{} loader: {}{} words{}", i + 1, osd.patches.len(), if osd.complete { "" } else { " observed, incomplete;" }, if words.is_empty() { String::new() } else { format!("; {}", words.join("; ")) });
        }
        if !dump.diagnostics.is_empty() { println!("D{} evidence gaps: {}", i + 1, dump.diagnostics.join("; ")); }
    }
    for after in 1..set.dumps.len() {
        let changes: Vec<_> = set.changelog.entries.iter().filter(|c| c.after == after).collect();
        println!("Between D{} and D{}:", after, after + 1);
        let mut modules: BTreeMap<&str, Vec<String>> = BTreeMap::new();
        let mut details = Vec::new();
        let mut metadata = Vec::new();
        for c in changes {
            let row = set.row(&c.subject);
            if row.is_some_and(|r| r.kind == RowKind::Module) {
                let label = match c.kind { ChangeKind::Added => "added", ChangeKind::Removed => "removed", ChangeKind::Resized => "resized", ChangeKind::Metadata => "EXTINFO", _ => "changed" };
                if c.kind == ChangeKind::Metadata { metadata.push(c); continue }
                let item = {
                    let sizes = row.map(|r| (r.cells[c.before].as_ref().map(|c| c.size), r.cells[c.after].as_ref().map(|c| c.size)));
                    let size = if c.kind == ChangeKind::Resized { sizes.map(|(a, b)| format!(" {:#x}→{:#x}", a.unwrap_or(0), b.unwrap_or(0))).unwrap_or_default() } else { String::new() };
                    let span = c.ranges.first().and_then(|r| r.after.as_ref().or(r.before.as_ref())).or(c.evidence.last());
                    format!("{}{size}@{}", c.subject, span.map(location).unwrap_or_else(|| "unknown".into()))
                };
                modules.entry(label).or_default().push(item);
            } else {
                let span = c.ranges.first().and_then(|r| r.after.as_ref().or(r.before.as_ref())).or(c.evidence.last());
                let ranges = if row.is_some_and(|r| r.kind == RowKind::Region) { format!(", {} exact byte runs", c.ranges.len()) } else { String::new() };
                details.push(format!("{}{} [{}]", c.caption, ranges, span.map(location).unwrap_or_else(|| "no located bytes".into())));
            }
        }
        let structural: Vec<_> = modules.iter().filter(|(kind, _)| **kind != "changed").map(|(kind, names)| format!("{kind} ({}): {}", names.len(), names.join(", "))).collect();
        if !structural.is_empty() { println!("  {}", structural.join("; ")); }
        let mut other = modules.get("changed").map(|names| format!("changed ({}): {}", names.len(), names.join(", "))).unwrap_or_default();
        if let Some(first) = metadata.first() {
            if !other.is_empty() { other.push_str("; "); }
            other.push_str(&format!("EXTINFO changed for {} modules, e.g. {} [{}]", metadata.len(), first.caption, first.evidence.last().map(location).unwrap_or_default()));
        }
        if !other.is_empty() { println!("  {other}"); }
        if !details.is_empty() { println!("  {}", details.join("; ")); }
        if !set.changelog.entries.iter().any(|c| c.after == after) { println!("  Identical compared bytes and metadata."); }
    }
    Ok(())
}

fn location(e: &Evidence) -> String {
    let space = if e.space == Space::File { "file" } else { "decoded" };
    format!("{}/{space}+{:#x}", e.module, e.offset)
}
