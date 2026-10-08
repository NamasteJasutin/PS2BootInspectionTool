# PS2 Boot Inspection Tool v2 (Rust)

Cross-platform rewrite of the macOS app: the same behaviour, one codebase for Windows,
Linux and macOS (and, later, the browser via WebGPU).

```sh
cargo run --release -p ps2bootinspect                          # the app
cargo run --release -p ps2kit --bin ps2history -- <card.ps2>   # dump a card's play history
cargo run --release -p ps2bootinspect -- --render 95 out.png --bios <bios> --titles 21 --launches 60 --pal
cargo test -p ps2kit                                           # oracle tests against the Python/Swift outputs
```

| Crate | Contents |
|---|---|
| `ps2kit` | Portable core, no UI: memory card filesystem, ROMDIR + OSD LZ, BIOS asset loader and texture decoding, play history, scene simulation and boot sequence, sound synthesis, PS2LOGO parser and disc logo, ISO/SYSTEM.CNF/ELF reader, hand-off steps. |
| `ps2bootinspect` | The app: wgpu renderer (opening, warning and logo scenes), cpal audio with the three visualisers, egui sidebar, headless `--render`. |

Status: feature parity with the Swift app for the scenes, sound, time/camera controls, disc
panel and hand-off card. Default folders are searched on start (PCSX2's `bios` and `memcards`
under Library/Application Support, Documents or ~/.config; `~/PS2ISO`).
