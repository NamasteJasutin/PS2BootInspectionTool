//! Imports BIOS boot music into .psxsong and renders songs on caller-supplied instruments.
use ps2kit::bench::SoundBench;
use ps2kit::ps1::Ps1Shell;
use ps2kit::ps1bios::Ps1Bios;
use ps2kit::ps1intro::SceIntro;
use ps2kit::rom::RomDir;
use ps2kit::song::{BankRef, Kit, Song};
use ps2kit::{Error, Format, Result, VideoMode};

const USAGE: &str = "usage: song <bios> --import boot|licence|intro --out x.psxsong\n       song <bios> --render x.psxsong out.wav\n       song <bios> --kit [--bank boot|logo|ps1] --out <directory>";

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.is_empty() || matches!(args.as_slice(), [s] if s == "--help" || s == "-h") {
        println!("{USAGE}");
        return;
    }
    if let Err(e) = run(&args) {
        eprintln!("{e}");
        std::process::exit(1)
    }
}

fn shell(data: &[u8]) -> Result<(Ps1Shell, VideoMode)> {
    if Ps1Bios::detect(data) {
        let bios = Ps1Bios::new(data.to_vec())?;
        let video = if bios.identity().region_letter == Some('E') {
            VideoMode::Pal
        } else {
            VideoMode::Ntsc
        };
        Ok((Ps1Shell::load_ps1(&bios)?, video))
    } else {
        let rom = RomDir::new(data.to_vec())?;
        let video = if rom.module("ROMVER")?.get(4) == Some(&b'E') {
            VideoMode::Pal
        } else {
            VideoMode::Ntsc
        };
        Ok((Ps1Shell::load(&rom)?, video))
    }
}

fn run(args: &[String]) -> Result<()> {
    match args {
        [bios, option, rest @ ..] if option == "--kit" => {
            let (bank, out) = match rest {
                [out_option, out] if out_option == "--out" => (None, out),
                [bank_option, bank, out_option, out]
                    if bank_option == "--bank" && out_option == "--out" =>
                {
                    (Some(bank.as_str()), out)
                }
                [out] => (None, out),
                _ => return Err(Error::Corrupt(Format::Sequence, USAGE.into())),
            };
            let data = std::fs::read(bios)?;
            let bench = match bank {
                None => SoundBench::from_bytes(&data)?,
                Some("boot") => SoundBench::ps2(&RomDir::new(data)?)?,
                Some("logo") => SoundBench::ps2_logo(&RomDir::new(data)?)?,
                Some("ps1") => SoundBench::ps1(&shell(&data)?.0.bank),
                _ => return Err(Error::Corrupt(Format::Sequence, USAGE.into())),
            };
            println!(
                "{:?}: {} programs, {} samples, {} effects",
                bench.bank,
                bench.programs.len(),
                bench.samples.len(),
                bench.effects.len()
            );
            for sample in &bench.samples {
                println!(
                    "Sample {}: {} frames at 44.1 kHz, loop {:?}",
                    sample.index, sample.length, sample.loop_start
                );
            }
            for (style, members) in bench.by_style() {
                println!(
                    "{}: {:?}",
                    style.name(),
                    members.iter().map(|(id, _)| id).collect::<Vec<_>>()
                );
            }
            let kit = Kit::derive(&bench);
            print!("{}", kit.explain());
            kit.write_wavs(&bench, out)?;
            Ok(())
        }
        [bios, option, kind, out_option, out] if option == "--import" && out_option == "--out" => {
            let data = std::fs::read(bios)?;
            let song = match kind.as_str() {
                "boot" => Song::from_ps2_boot(&RomDir::new(data)?)?,
                "licence" => {
                    let (shell, video) = shell(&data)?;
                    Song::from_ps1_licence(&shell, video)?
                }
                "intro" => {
                    let bios = Ps1Bios::new(data)?;
                    let intro = SceIntro::locate(&bios.shell_image()?, bios.identity())?;
                    Song::from_ps1_intro(&intro, intro.video)?
                }
                _ => return Err(Error::Corrupt(Format::Sequence, USAGE.into())),
            };
            std::fs::write(out, song.to_text()?)?;
            println!(
                "Imported {}: {} channels, {} bars, {} BPM -> {out}",
                song.name,
                song.channels.len(),
                song.bars(),
                song.bpm
            );
            for note in &song.import_notes {
                eprintln!("Import note: {note}")
            }
            Ok(())
        }
        [bios, option, song_path, out] if option == "--render" => {
            let song = Song::parse(&std::fs::read_to_string(song_path)?)?;
            let data = std::fs::read(bios)?;
            let bench = match song.bank {
                BankRef::Ps2 => SoundBench::ps2(&RomDir::new(data)?)?,
                BankRef::Ps2Logo => SoundBench::ps2_logo(&RomDir::new(data)?)?,
                BankRef::Ps1 => SoundBench::ps1(&shell(&data)?.0.bank),
                _ => {
                    return Err(Error::Corrupt(
                        Format::Sequence,
                        "unsupported instrument bank".into(),
                    ))
                }
            };
            song.write_wav(&bench, out)?;
            println!("Rendered {} -> {out}", song.name);
            Ok(())
        }
        _ => Err(Error::Corrupt(Format::Sequence, USAGE.into())),
    }
}
