# ps2kit

[![crates.io](https://img.shields.io/crates/v/ps2kit.svg)](https://crates.io/crates/ps2kit) [![docs.rs](https://docs.rs/ps2kit/badge.svg)](https://docs.rs/ps2kit)

Readers for PlayStation 2 BIOS dumps, PCSX2 memory cards and game disc images, and a
re-implementation of what the console computes at start-up: the boot screen's towers from the
play history, the camera path, the chime, the logo program's timing.

Nothing from Sony is included. Every table, texture, sound and bitmap is read from files you
supply at run time; the crate's CI tests run on synthetic inputs only.

```toml
[dependencies]
ps2kit = "0.2"
```

```rust
use ps2kit::{memcard::MemoryCard, history::PlayHistory};

let card = MemoryCard::open("Mcd001.ps2")?;
for r in &PlayHistory::from_card(&card)?.records {
    if !r.is_empty() {
        println!("{} launched {} times, towers {:06b}", r.name, r.count, r.mask);
    }
}
```

| module | what it reads or does |
|---|---|
| `rom` | the ROMDIR container and the OSD LZ scheme |
| `memcard` | PCSX2 card images (with or without ECC) and folder cards |
| `history` | the play-history file (`B?DATA-SYSTEM/history`), the data behind the boot screen's towers |
| `sectors`, `disc` | ISO / BIN+CUE sector access, `SYSTEM.CNF`, the boot ELF, the hand-off a console would perform |
| `logo` | `rom0:PS2LOGO` assets and the logo bitmap on a disc's first sectors (descrambling, region checksums) |
| `sound` | OSD sound bank, sequences and the SPU envelope, rendered to PCM |
| `bios`, `locate` | the opening's data tables, found in any supported ROM by content |
| `sim` | towers, camera and timelines as functions of the frame number |
| `ps1` | the PS1 licence screen a PS2 shows for a PlayStation disc (`rom0:LOGO`, TMD, TIM, VAB) |

Supported BIOS versions for `bios`/`logo`/`sound`: ROM 1.50–2.00 of the OSD generation with
`TEX*` assets (tested: 2.00 E, 1.60 E, 1.60 A, DTL-H30101 1.50 A). ROM 1.00 is a different
generation and is rejected with a clear error. The other readers do not depend on a BIOS.

`ps2history <card>` (a small binary in the crate) lists a card and dumps its history.

Requires Rust 1.87. `cargo test` runs the synthetic tests; `tests/oracles.rs` (not in the
published crate) also checks against a BIOS and discs on the developer's machine and passes
trivially without them. Breaking changes are listed in `CHANGELOG.md`.

The formats are documented in the `notes/` directory of the
[PS2 Boot Inspection Tool](https://github.com/NamasteJasutin/PS2BootInspectionTool), the app
built on this crate. MIT licence.
