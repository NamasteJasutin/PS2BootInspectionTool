Title: The PS2 boot screen's towers are your play history, and the "PlayStation 2" logo is read from the disc — a write-up of the whole boot path

<URL>

Static RE of the PS2 BIOS from reset to the game's first instruction. Short version of the surprising bits:

- The towers are a plot of B?DATA-SYSTEM/history on the memory card: 21 records, six tower slots each, a tower on first launch and more at 14/24/34/44/54 launches. PCSX2 fast boot never writes that file, which is why most emulator users have never seen towers.
- The chime isn't a sample; it's an 8-note sequence played through the SPU2 from a 9-sample bank, with the hardware ADSR doing the envelopes.
- The lettering in the "PlayStation 2" logo isn't in the BIOS. PS2LOGO reads sectors 0–11 of the disc with the drive's descrambler on and checksums them per console region. Japan and the US share one logo master; Europe has its own.
- The red warning screen is only for disc state 0x74 (unreadable media); no disc goes to the clock.

There's a cross-platform tool that replays all of it from your own BIOS and card (https://github.com/NamasteJasutin/PS2BootInspectionTool), a Rust crate with the readers, and a Ghidra kit. No Sony data anywhere.

Flair: Discussion
