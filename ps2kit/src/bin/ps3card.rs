//! Prints plaintext PS3 disc/save metadata and a source-cited disc hand-off.
//! Usage: ps3card <disc folder|iso|save folder> [--firmware 4.82 | --pup PS3UPDAT.PUP]

use ps2kit::ps3::{FirmwareVersion, Ps3Disc, Ps3Save, Sfo};
use ps2kit::pup::Pup;
use std::path::PathBuf;

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
fn usage() {
    println!("ps3card <disc folder|iso|save folder> [--firmware 4.82 | --pup PS3UPDAT.PUP]");
}
fn argument_error(message: &str) -> ps2kit::Error {
    ps2kit::Error::Corrupt(ps2kit::Format::DiscImage, message.into())
}
fn run() -> ps2kit::Result<()> {
    let mut args = std::env::args_os().skip(1);
    let mut path = None;
    let mut firmware = None;
    let mut pup_path = None;
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--help" | "-h") => {
                usage();
                return Ok(());
            }
            Some("--firmware") => {
                if firmware.is_some() || pup_path.is_some() {
                    return Err(argument_error("use exactly one of --firmware / --pup"));
                }
                let value = args.next().ok_or_else(|| argument_error("--firmware needs a version"))?;
                let value = value.into_string().map_err(|_| argument_error("firmware version must be text"))?;
                FirmwareVersion::parse(&value)?;
                firmware = Some(value);
            }
            Some("--pup") => {
                if firmware.is_some() || pup_path.is_some() {
                    return Err(argument_error("use exactly one of --firmware / --pup"));
                }
                pup_path = Some(PathBuf::from(args.next().ok_or_else(|| argument_error("--pup needs a path"))?));
            }
            Some(s) if s.starts_with('-') => return Err(argument_error("unknown option (see --help)")),
            _ if path.is_none() => path = Some(PathBuf::from(arg)),
            _ => return Err(argument_error("only one disc/save path is accepted")),
        }
    }
    let Some(path) = path else {
        usage();
        std::process::exit(2);
    };
    if let Some(p) = pup_path {
        let version = Pup::open(&p)?.version()?;
        FirmwareVersion::parse(&version)?;
        println!("Console firmware input: {version} from {} (assumes this matches the console).", p.display());
        firmware = Some(version);
    }
    // A root PARAM.SFO is a save candidate; Ps3Save never reads game data files.
    let is_save = path.is_dir()
        && std::fs::read_dir(&path)?.any(|e| e.ok().is_some_and(|e| e.file_name().eq_ignore_ascii_case("PARAM.SFO")));
    if is_save {
        let save = Ps3Save::open(&path)?;
        println!("PS3 save: {}", path.display());
        print_sfo(&save.sfo);
        println!("ICON0.PNG: {}", save.icon_size.map(|n| format!("{n} bytes")).unwrap_or_else(|| "missing".into()));
        if let Some(h) = &save.pfd {
            println!("PARAM.PFD: {} bytes, PFDB version {}; encrypted header table not read or verified. [PS3 Developer wiki PARAM.PFD §PFD Header, I]", save.pfd_size.unwrap_or(0), h.version);
        } else {
            println!("PARAM.PFD: missing; integrity/authentication unknown.");
        }
        for file in &save.data_files {
            println!("  {}: {} bytes (data not read)", file.name, file.size);
        }
    } else {
        let disc = Ps3Disc::open(&path)?;
        println!("PS3 disc: {}", path.display());
        for step in disc.handoff(firmware.as_deref()) {
            println!("{step}");
        }
        if let Some(sfb) = &disc.sfb {
            println!("SFB version 0x{:08x}:", sfb.version);
            for e in &sfb.entries {
                println!("  {} @ 0x{:x}: {} bytes, text {:?}", e.key, e.offset, e.value.len(), sfb.text(&e.key));
            }
        }
        if let Some(sfo) = &disc.sfo {
            print_sfo(sfo);
        }
        println!("File inventory (sector plaintext does not imply an unencrypted SELF payload):");
        for file in &disc.files {
            let size = file.size.map(|n| format!("{n} bytes")).unwrap_or_else(|| "size unknown".into());
            let location = file.lba.map(|n| format!(", LBA {n}")).unwrap_or_default();
            println!("  {}: {size}{location}, {}", file.path, file.readability);
        }
    }
    Ok(())
}
fn print_sfo(sfo: &Sfo) {
    println!("PARAM.SFO version 0x{:08x}: [PS3 Developer wiki PARAM.SFO §Internal Structure, I]", sfo.version);
    for entry in &sfo.entries {
        println!(
            "  {}: {} (fmt 0x{:04x}, length {}, max {})",
            entry.key, entry.value, entry.format, entry.length, entry.max_length
        );
    }
    if let Some(flags) = sfo.attribute_flags() {
        println!("  ATTRIBUTE decoded: {flags}");
    }
}
