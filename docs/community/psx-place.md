Title: What a PlayStation 2 does between power-on and the game — the boot screen, the history file, and the logo that lives on the disc

I've written up a static reverse-engineering of the PS2 boot path (ROM 2.00 E, cross-checked against 1.60 E/A and the DTL-H30101 devkit) and released the tooling that came out of it.

The article: <URL>

Things in it that I haven't seen documented elsewhere:
- The OSD LZ format OSDSYS and PS2LOGO are stored in (30-token groups, BE descriptor with a 2-bit offset/length split), with a reference decoder.
- The play-history file (B?DATA-SYSTEM/history): 21 × 22-byte records, the tower rules (one tower on first launch, another at 14/24/34/44/54, frozen at 63), the random slot/bit choice and the eviction rule with its tie-break quirk.
- Every render stage of the opening with its constants (camera integrator, 0.625 feedback smear, the ten-pass glass cubes, defocus, fade), and the chime as a sequenced SPU score rather than a sample.
- PS2LOGO reads the lettering from the disc's sectors 0–11 (rotl(b^key,3) descramble), checksums it per region (E 0x78134705, J 0x62DB1E66), and J/A discs share one master.
- The warning scene's trigger is disc state 0x74 only — "no disc" goes to the clock/menu.

Tools (no Sony data in any of them — you bring your own BIOS):
- PS2 Boot Inspection Tool, Windows/Linux/macOS: replays the boot from your BIOS + PCSX2 memory card + disc image, free camera, time control, chime, full hand-off readout. https://github.com/NamasteJasutin/PS2BootInspectionTool/releases
- ps2kit, a Rust crate with the readers (ROMDIR/LZ, memory cards, history, ISO/BIN sectors, logo, sound bank/sequence). https://crates.io/crates/ps2kit
- ps2-bios-ghidra: one-command headless Ghidra set-up for RESET/KERNEL/EELOAD/OSDSYS/PS2LOGO with 750+ symbol names for 2.00 E. https://github.com/NamasteJasutin/ps2-bios-ghidra

What I'd most like from this community: a real-hardware capture with timestamps (IOP ready bits, first visible frame) to settle the durations, which are all estimates; and anyone with a 1.00 J ROM who wants the table discovery extended to that OSD generation.
