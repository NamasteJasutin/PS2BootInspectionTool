//! CLI tool for inspecting and rendering sound programs and font glyphs from BIOS dumps.

use ps2kit::bench::{FontBench, SoundBench};
use ps2kit::sound::{write_wav, SAMPLE_RATE};
use std::path::{Path, PathBuf};

const USAGE: &str = "usage: bench <bios> [--sounds] [--render <prog> <note> <out.wav>] [--fonts] [--atlas <font> <out.ppm|pam>]";

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
                render_req = Some((prog, note, out));
                i += 4;
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
            other => {
                eprintln!("unknown option: {other}\n{USAGE}");
                std::process::exit(2);
            }
        }
    }

    if !show_sounds && render_req.is_none() && !show_fonts && atlas_req.is_none() {
        eprintln!("{USAGE}");
        std::process::exit(2);
    }

    if let Err(e) = run(bios_path, show_sounds, render_req, show_fonts, atlas_req) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn run(
    bios_path: PathBuf,
    show_sounds: bool,
    render_req: Option<(i32, i32, PathBuf)>,
    show_fonts: bool,
    atlas_req: Option<(String, PathBuf)>,
) -> ps2kit::Result<()> {
    if show_sounds || render_req.is_some() {
        let sound_bench = SoundBench::open(&bios_path)?;

        if show_sounds {
            println!("Sound bank: {} programs, {} samples", sound_bench.programs.len(), sound_bench.samples.len());
            println!("\nPrograms & Tones:");
            for p in &sound_bench.programs {
                println!("  Program {:3} (vol: {:3}, tones: {})", p.id, p.volume, p.tones.len());
                for (t_idx, t) in p.tones.iter().enumerate() {
                    println!(
                        "    Tone {:2}: key {:3}..={:3}, root {:3}, fine {:+3}, sample {:2}, ADSR {:04X}/{:04X}, vol {:3}, pan {:3}",
                        t_idx, t.low, t.high, t.root, t.fine, t.sample_index, t.adsr1, t.adsr2, t.volume, t.pan
                    );
                }
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

        if let Some((prog, note, out)) = render_req {
            let pcm = sound_bench.render(prog, note, 127, 2.0);
            write_wav(&out, &pcm, SAMPLE_RATE as u32)?;
            println!(
                "Rendered program {prog} note {note} to {} ({:.2} s, {} samples)",
                out.display(),
                pcm.len() as f64 / 2.0 / SAMPLE_RATE as f64,
                pcm.len() / 2
            );
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
            let font = font_bench
                .font(&font_name)
                .ok_or_else(|| ps2kit::Error::NotFound(ps2kit::Format::Bios, format!("font '{font_name}' not found")))?;
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

    Ok(())
}

fn write_image(path: &Path, width: usize, height: usize, rgba: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
    if ext.eq_ignore_ascii_case("pam") {
        let mut f = std::fs::File::create(path)?;
        write!(
            f,
            "P7\nWIDTH {width}\nHEIGHT {height}\nDEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\nENDHDR\n"
        )?;
        f.write_all(rgba)?;
    } else {
        let mut f = std::fs::File::create(path)?;
        write!(f, "P6\n{width} {height}\n255\n")?;
        let mut rgb = Vec::with_capacity(width * height * 3);
        for chunk in rgba.chunks_exact(4) {
            rgb.extend_from_slice(&chunk[..3]);
        }
        f.write_all(&rgb)?;
    }
    Ok(())
}
