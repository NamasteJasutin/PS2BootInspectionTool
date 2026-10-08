# Draft: PCSX2 forum / Discord post

**Title:** A tool that replays the PS2 boot screen from your BIOS + PCSX2 memory card, and shows what the towers mean

I've released a small research tool that might interest people here: it reads your own BIOS dump and a PCSX2 memory card (`.ps2` image or folder card) and replays the boot screen the console would show for that card — towers and all — with a free camera, time control, the chime, and the full sequence through the PS2 logo to the point where a game would start (it never runs the game, it only tells you what would happen next).

Things PCSX2 users may not know, which the tool makes visible:

- The towers are your play history. OSDSYS keeps `B?DATA-SYSTEM/history` on the card: 21 records of 22 bytes with title ID, launch count, a 6-bit tower mask and the last launch date. A title gets one tower on its first launch and another at launches 14, 24, 34, 44 and 54.
- **Fast boot never writes that file**, so a card that has only ever been used with fast boot shows an empty field under full boot. The tool can stand in a history from the saves on the card so you can see what it would look like.
- The "PlayStation 2" logo is not in the BIOS: it's read from sectors 0–11 of the disc and checksummed per region (J and US discs share one master, EU has its own; A/C consoles don't check).

Binaries for Windows, Linux and macOS: https://github.com/NamasteJasutin/PS2BootInspectionTool/releases
Rust crate with the readers (memory card, history, ROMDIR/LZ, ISO/BIN sectors, logo): https://crates.io/crates/ps2kit
Write-up of the whole boot path: https://github.com/NamasteJasutin/PS2BootInspectionTool/blob/main/docs/writeup.md

Nothing from Sony ships with it — you need your own BIOS, as with PCSX2 itself. Happy to answer questions about any of the formats; the notes in the repository go into the details.
