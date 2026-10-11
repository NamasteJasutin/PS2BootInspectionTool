# Wiki contributions

Self-contained entries in MediaWiki markup, ready to paste into psdevwiki.com (psdevwiki.com/ps2, psdevwiki.com/ps3, psx-spx, or ps2tek) on the pages named in each file. Every fact is traceable to `notes/research/`; each entry ends with a reference to the repository so readers can check it. Nothing here reproduces Sony code, assets, or keys.

## Entries

### PlayStation 1
* [`ps1-system-cnf.wiki`](file:///Users/jasutin/Dev/PS2BootScreen/.worktrees/w1-wiki/docs/wiki/ps1-system-cnf.wiki) — PS1 `SYSTEM.CNF` as the kernel parses it: line-prefix matching, the `BOOT2` shadowing trap, un-prefixed hex, missing key defaulting to 0, root directory bounds (1 sector, 40 entries), and argument passing to RAM `0x180`.
* [`ps1-licence-policy.wiki`](file:///Users/jasutin/Dev/PS2BootScreen/.worktrees/w1-wiki/docs/wiki/ps1-licence-policy.wiki) — PS1 shell licence policy per revision: why the version number is not enough, the wobble-groove `GetID` gate, per-shell compare flags, and why North American and Western European consoles display disc logos unchecked.
* [`ps1-sce-intro.wiki`](file:///Users/jasutin/Dev/PS2BootScreen/.worktrees/w1-wiki/docs/wiki/ps1-sce-intro.wiki) — PS1 Sony Computer Entertainment intro: 640×480 interlaced timeline, procedural Gouraud diamond construction (quad and sliding triangles), CLUT-interpolated logotype fades, and sound driver clock jump with VAB sweeps and chime arpeggios.
* [`ps1-post-sequence.wiki`](file:///Users/jasutin/Dev/PS2BootScreen/.worktrees/w1-wiki/docs/wiki/ps1-post-sequence.wiki) — PS1 POST byte sequence (`0x1F802041`): full progression across K1 (1.0–2.0) and K2 (2.2–4.5) kernels, inner device installation stages, and error handling.
* [`ps1-card-undelete-soundscope.wiki`](file:///Users/jasutin/Dev/PS2BootScreen/.worktrees/w1-wiki/docs/wiki/ps1-card-undelete-soundscope.wiki) — PS1 memory card manager four-shoulder undelete (BIOS B0 function 0x46), late-shell mouse undelete unreachable branch bug, and SoundScope 24-preset visualizer with the `BPLAYSTATION` controller performance save file.

### PlayStation Portable (POPS)
* [`pops-ps1-bios.wiki`](file:///Users/jasutin/Dev/PS2BootScreen/.worktrees/w1-wiki/docs/wiki/pops-ps1-bios.wiki) — PSP POPS' PS1 BIOS (`PSXONPSP660.BIN`): unmodified 1995 K2 kernel, sibling of PS2 `rom0:LOGO` shell, stripped region and licence checks, embedded ROM TMD rendering, and 120-field hold.

### PlayStation 2
* [`ps2-ps1-mode.wiki`](file:///Users/jasutin/Dev/PS2BootScreen/.worktrees/w1-wiki/docs/wiki/ps2-ps1-mode.wiki) — PS2's PS1 backwards compatibility architecture: modular decomposition into `rom0:TBIN`, `SBIN`, `LOGO`, `KROM`, and `KROMG` (30-byte gaiji glyphs), hand-off sequence, and the SCPH-39004 regional mismatch.
* [`ps2-osd-sound.wiki`](file:///Users/jasutin/Dev/PS2BootScreen/.worktrees/w1-wiki/docs/wiki/ps2-osd-sound.wiki) — PS2 OSD sound assets: five byte-distinct banks, eight `SSsq` sequences (`SNDBOOTS`, `SNDCLOKS`, etc.), the NRPN-127 infinite loop mechanism, and the executable game logo chord.
* [`ps2-hidden-remote-option.wiki`](file:///Users/jasutin/Dev/PS2BootScreen/.worktrees/w1-wiki/docs/wiki/ps2-hidden-remote-option.wiki) — PS2 2.00 E hidden "Remote Control Off/On" option page: controller port 2 chord on Version Information Console item, input folding, MechaCon configuration byte `0x13` bit 7, and cold-boot S-command 0x30 gate.
* [`ps2-nvm-osd-checksum.wiki`](file:///Users/jasutin/Dev/PS2BootScreen/.worktrees/w1-wiki/docs/wiki/ps2-nvm-osd-checksum.wiki) — PS2 MechaCon NVM OSD configuration block checksum: 15-byte sum rule across Layout 0 (`0x310`) and Layout 1 (`0x2C0`), and PCSX2's bit-7-dropped default checksum bug triggering the first-boot setup wizard.
* [`disc-logo.wiki`](file:///Users/jasutin/Dev/PS2BootScreen/.worktrees/w1-wiki/docs/wiki/disc-logo.wiki) — The PS2 logo sectors (disc sectors 0–11): decryption algorithm, PAL/NTSC dimensions, and 32-bit word sum checksums.
* [`osd-lz-format.wiki`](file:///Users/jasutin/Dev/PS2BootScreen/.worktrees/w1-wiki/docs/wiki/osd-lz-format.wiki) — OSD LZ compression format used for `OSDSYS`, `PS2LOGO`, and packed PS1/POPS shells.
* [`play-history.wiki`](file:///Users/jasutin/Dev/PS2BootScreen/.worktrees/w1-wiki/docs/wiki/play-history.wiki) — Play history file (`B?DATA-SYSTEM/history`): 21-record table structure, tower generation rules, and eviction logic.
* [`romdir-extinfo.wiki`](file:///Users/jasutin/Dev/PS2BootScreen/.worktrees/w1-wiki/docs/wiki/romdir-extinfo.wiki) — ROMDIR EXTINFO records: date, version, comment, and build configuration paths.

### PlayStation 3
* [`ps3-pup-container.wiki`](file:///Users/jasutin/Dev/PS2BootScreen/.worktrees/w1-wiki/docs/wiki/ps3-pup-container.wiki) — PS3 PUP container plaintext structure: 64-bit big-endian headers, member directories, HMAC-SHA1 digests, unencrypted metadata (`version.txt`, `license.xml`), and 0x80-byte plaintext package descriptors.
