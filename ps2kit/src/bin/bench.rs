//! CLI tool for inspecting and rendering sound programs and font glyphs from BIOS dumps.

use ps2kit::bench::{FontBench, SoundBench};
use ps2kit::sound::{write_wav, SAMPLE_RATE};
use std::path::{Path, PathBuf};

const USAGE: &str = "usage: bench <bios> [--sounds] [--render <prog> <note> <out.wav>] [--effect <id> <out.wav>] [--fonts] [--atlas <font> <out.ppm|pam>] [--sf2 <out.sf2> [--bank boot|logo|ps1]]";

enum RenderRequest {
    Program(i32, i32, PathBuf),
    Effect(usize, PathBuf),
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || (args.len() == 1 && (args[0] == "--help" || args[0] == "-h")) {
        println!("{USAGE}");
        return;
    }
    let bios_path = PathBuf::from(&args[0]);
    if bios_path.to_string_lossy().starts_with('-') {
        eprintln!("{USAGE}");
        std::process::exit(2);
    }

    let mut i = 1;
    let mut show_sounds = false;
    let mut render_req = None;
    let mut show_fonts = false;
    let mut atlas_req = None;
    let mut sf2_req = None;
    let mut bank_req = None;

    while i < args.len() {
        match args[i].as_str() {
            "--sounds" => {
                show_sounds = true;
                i += 1;
            }
            "--render" => {
                if i + 3 >= args.len() {
                    eprintln!("{USAGE}");
                    std::process::exit(2);
                }
                let prog: i32 = match args[i + 1].parse() {
                    Ok(p) => p,
                    Err(_) => {
                        eprintln!("invalid program number: {}", args[i + 1]);
                        std::process::exit(2);
                    }
                };
                let note: i32 = match args[i + 2].parse() {
                    Ok(n) => n,
                    Err(_) => {
                        eprintln!("invalid note number: {}", args[i + 2]);
                        std::process::exit(2);
                    }
                };
                let out = PathBuf::from(&args[i + 3]);
                render_req = Some(RenderRequest::Program(prog, note, out));
                i += 4;
            }
            "--effect" => {
                if i + 2 >= args.len() {
                    eprintln!("{USAGE}");
                    std::process::exit(2)
                }
                let id = match args[i + 1].parse() {
                    Ok(id) => id,
                    Err(_) => {
                        eprintln!("invalid effect number: {}", args[i + 1]);
                        std::process::exit(2)
                    }
                };
                render_req = Some(RenderRequest::Effect(id, PathBuf::from(&args[i + 2])));
                i += 3;
            }
            "--fonts" => {
                show_fonts = true;
                i += 1;
            }
            "--atlas" => {
                if i + 2 >= args.len() {
                    eprintln!("{USAGE}");
                    std::process::exit(2);
                }
                let font_name = args[i + 1].clone();
                let out = PathBuf::from(&args[i + 2]);
                atlas_req = Some((font_name, out));
                i += 3;
            }
            "--sf2" => {
                if i + 1 >= args.len() {
                    eprintln!("{USAGE}");
                    std::process::exit(2);
                }
                sf2_req = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            "--bank" => {
                if i + 1 >= args.len() {
                    eprintln!("{USAGE}");
                    std::process::exit(2);
                }
                let bank_name = args[i + 1].to_ascii_lowercase();
                if !["boot", "logo", "ps1"].contains(&bank_name.as_str()) {
                    eprintln!("invalid bank name: {}\n{USAGE}", args[i + 1]);
                    std::process::exit(2);
                }
                bank_req = Some(bank_name);
                i += 2;
            }
            other => {
                eprintln!("unknown option: {other}\n{USAGE}");
                std::process::exit(2);
            }
        }
    }

    if !show_sounds && render_req.is_none() && !show_fonts && atlas_req.is_none() && sf2_req.is_none() {
        eprintln!("{USAGE}");
        std::process::exit(2);
    }

    if let Err(e) = run(bios_path, show_sounds, render_req, show_fonts, atlas_req, sf2_req, bank_req) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn run(
    bios_path: PathBuf,
    show_sounds: bool,
    render_req: Option<RenderRequest>,
    show_fonts: bool,
    atlas_req: Option<(String, PathBuf)>,
    sf2_req: Option<PathBuf>,
    bank_req: Option<String>,
) -> ps2kit::Result<()> {
    if show_sounds || render_req.is_some() {
        let sound_bench = SoundBench::open(&bios_path)?;

        if show_sounds {
            println!(
                "Sound banks: {} programs, {} UI effects, {} samples",
                sound_bench.programs.len(),
                sound_bench.effects.len(),
                sound_bench.samples.len()
            );
            println!("\nPrograms & Tones:");
            for p in &sound_bench.programs {
                println!("  Program {:3} ({:?}, vol: {:3}, tones: {})", p.id, p.kind, p.volume, p.tones.len());
                for (t_idx, t) in p.tones.iter().enumerate() {
                    println!(
                        "    Tone {:2}: key {:3}..={:3}, root {:3}, fine {:+3}, sample {:2}, ADSR {:04X}/{:04X}, vol {:3}, pan {:3}",
                        t_idx, t.low, t.high, t.root, t.fine, t.sample_index, t.adsr1, t.adsr2, t.volume, t.pan
                    );
                }
            }

            println!("\nUI Effects:");
            for e in &sound_bench.effects {
                println!(
                    "  Effect {:2}: sample {:2}, pitch {:04X}, ADSR {:04X}/{:04X}, vol L/R {}/{}",
                    e.id, e.sample_index, e.pitch, e.adsr.0, e.adsr.1, e.volume.0, e.volume.1
                );
            }

            println!("\nSamples:");
            for s in &sound_bench.samples {
                let loop_str = match s.loop_start {
                    Some(lp) => format!("loop @ {lp}"),
                    None => "one-shot".into(),
                };
                println!("  Sample {:2}: {:6} samples ({loop_str})", s.index, s.length);
            }
        }

        if let Some(request) = render_req {
            let (pcm, out, label) = match request {
                RenderRequest::Program(prog, note, out) => {
                    if !(0..=127).contains(&note) || sound_bench.program(prog).is_none() {
                        return Err(ps2kit::Error::NotFound(
                            ps2kit::Format::SoundBank,
                            format!("program {prog} or MIDI note {note} is unavailable"),
                        ));
                    }
                    (sound_bench.render(prog, note, 127, 2.0), out, format!("program {prog} note {note}"))
                }
                RenderRequest::Effect(id, out) => {
                    if sound_bench.effects.get(id).is_none() {
                        return Err(ps2kit::Error::NotFound(ps2kit::Format::SoundBank, format!("UI effect {id}")));
                    }
                    (sound_bench.render_effect(id, 2.0), out, format!("UI effect {id}"))
                }
            };
            write_wav(&out, &pcm, SAMPLE_RATE as u32)?;
            println!("Rendered {label} to {} (2.00 s, {} stereo frames)", out.display(), pcm.len() / 2);
        }
    }

    if show_fonts || atlas_req.is_some() {
        let font_bench = FontBench::open(&bios_path)?;

        if show_fonts {
            println!("Fonts in {}:", bios_path.display());
            for f in font_bench.fonts() {
                let (w, h) = f.dimensions();
                println!("  {:8} (cell: {:2}x{:2}, count: {:4}) | {}", f.name(), w, h, f.glyph_count(), f.summary());
            }
        }

        if let Some((font_name, out)) = atlas_req {
            let font = font_bench.font(&font_name).ok_or_else(|| {
                ps2kit::Error::NotFound(ps2kit::Format::Bios, format!("font '{font_name}' not found"))
            })?;
            let (w, h, rgba) = font.atlas(..);
            write_image(&out, w, h, &rgba)?;
            println!(
                "Exported font '{}' atlas ({}x{}, {} glyphs) to {}",
                font.name(),
                w,
                h,
                font.glyph_count(),
                out.display()
            );
        }
    }

    if let Some(out_path) = sf2_req {
        let bank_kind = bank_req.as_deref().unwrap_or("boot");
        let (bench, name) = match bank_kind {
            "boot" | "ps1" => {
                let bench = SoundBench::open(&bios_path)?;
                if bank_kind == "ps1" && bench.bank != ps2kit::banks::BankRef::Ps1 {
                    return Err(ps2kit::Error::NotFound(ps2kit::Format::SoundBank, format!("PS1 sound bank in {}", bios_path.display())));
                }
                let name = if bench.bank == ps2kit::banks::BankRef::Ps1 { "PS1 shell VAB" } else { "PS2 boot bank" };
                (bench, name.to_string())
            }
            "logo" => {
                let rom = ps2kit::rom::RomDir::new(std::fs::read(&bios_path)?)?;
                (SoundBench::ps2_logo(&rom)?, "PS2 logo bank".to_string())
            }
            _ => unreachable!(),
        };

        let comment = format!("Exported from {}", bios_path.display());
        let sf2_data = ps2kit::sf2::sf2_from_bench(&bench, &[], &name, &comment)?;
        std::fs::write(&out_path, &sf2_data)?;
        println!(
            "Exported SoundFont 2 to {} ({} bytes, {} programs, {} UI effects, {} samples)",
            out_path.display(),
            sf2_data.len(),
            bench.programs.len(),
            bench.effects.len(),
            bench.samples.len()
        );
    }

    Ok(())
}

fn write_image(path: &Path, width: usize, height: usize, rgba: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
    if ext.eq_ignore_ascii_case("pam") {
        let mut f = std::fs::File::create(path)?;
        write!(f, "P7\nWIDTH {width}\nHEIGHT {height}\nDEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\nENDHDR\n")?;
        f.write_all(rgba)?;
    } else if ext.eq_ignore_ascii_case("ppm") {
        let mut f = std::fs::File::create(path)?;
        write!(f, "P6\n{width} {height}\n255\n")?;
        let mut rgb = Vec::with_capacity(width * height * 3);
        for chunk in rgba.chunks_exact(4) {
            for (&c, bg) in chunk[..3].iter().zip([96u32, 96, 160]) {
                let alpha = u32::from(chunk[3]);
                rgb.push(((u32::from(c) * alpha + bg * (255 - alpha)) / 255) as u8);
            }
        }
        f.write_all(&rgb)?;
    } else {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "atlas output must end in .ppm or .pam"));
    }
    Ok(())
}
