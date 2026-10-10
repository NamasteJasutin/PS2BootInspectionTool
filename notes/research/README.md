# Research: the hidden hand-overs of the PlayStation boot ROMs

Four surveys (2026-10-09) across five ROMs — SCPH-10000 (1.00 J), DTL-H30101 (1.50 A devkit),
SCPH-30004R (1.60 E), SCPH-39001 (1.60 A), SCPH-70004 (2.00 E, with ROM1/ROM2/EROM/NVM/MEC) —
and four discs (Tekken Tag Tournament USA/EU, Tekken PS1 EU/JP). Every claim is marked [V]
(read in code or data, with the address) or [I] (inferred). These notes feed Camp 4 of
`docs/campaign.md`: the hook map, ROM explorer, NVM inspector and disc probe.

| note | subject | lines |
|---|---|---|
| `osdsys_hooks.md` | OSDSYS: argv vocabulary, the files it looks for (card update, DVD player, HDD), SYSTEM.CNF keys, NVRAM bits, inputs, disc-type dispatch | 396 |
| `kernel_eeload_hooks.md` | RESET/RDRAM/KERNEL/EELOAD/IOP: hardware probes, EELOAD's grammar, SECRMAN/XCDVDMAN gates, DECI2 and serial, reset strings | 606 |
| `games_probe.md` | what the four discs ask of the console, IOPRP images, PS1DRV's per-title tables, PS2LOGO's hand-over state | 618 |
| `rom1_erom_nvm.md` | ROM1 (DVD player), ROM2, EROM, the NVM field map and command chain, MEC | 434 |

## What changed in our understanding

- **Four OSD programs, not five**: 1.60 A and 1.60 E unpack to the same image — but the
  SCPH-30004R's loader stub **hot-patches eight words** into it before `ExecPS2`, relaxing the
  disc-ID and disc-type re-checks. 1.00 J is a different program altogether (`MOPEN`/`MCLOCK`/
  `MBROWS` modules, browser replaceable from a card descriptor).
- **Every ROM has the memory-card system-update hook**, with a version-specific file name
  (`osdsys.elf` → `osd160`/`osd170`/`osd210.elf` + `osdmain.elf`) and the region letter from
  ROMVER; the DVD-player-from-card path is gated on `rom1:DVDID` from 1.50 on; the HDD
  hand-over (`ATAD` → `HDDLOAD` → `rom0:HDDBOOT`) is live in 1.50/1.60 and dead in 2.00.
- **Only `BOOT2`, `BOOT`, `VER` are read from SYSTEM.CNF by OSDSYS**; `VMODE` is read by nothing
  in the 2.00 E ROM; PS1DRV honours a hidden `PSD1.0.0` line; TBIN reads `TCB`/`EVENT`/`STACK`.
- **No button skips the opening.** The argv words grow from four `Boot…` (1.00) to
  `BootWarning`/`BootIllegal`/`SkipMc`/`SkipHdd` (1.60) and `SkipForbid` (2.00);
  `Initialize` is dead code.
- **`BootIllegal` is XLOADFILE's `cdrom0:sce_dev5` check**: disc type → `sceCdReadKey(0,0,0x4B)`
  → error byte `0 / 0x30 / 0x37 / other` decides run / BootIllegal / BootError(-204) / BootError.
- **The pre-OSD layer barely differs**: IOPBOOT, REBOOT, LOADFILE, MODLOAD, CDVDMAN, MCMAN,
  SIO2MAN, PADMAN, LOADCORE byte-identical in all five; retail SECRMAN unchanged 2000→2004;
  the devkit's EELOAD = 1.60 E's. The devkit's only pre-OSD difference is `secrman_for_dex`.
- **BoardID is `lhu 0x1F803800`** (a TOOL UART register); ROMGEN prints year-monthday;
  ROMGSCRT is orphaned code; no ROM honours `host:` or a serial/DECI2 program load (the SIO
  RX handler prints `pc=` and discards the bytes). 1.00 J's EELOAD narrates every load on the
  EE serial port and never resets the IOP.
- **The games ask the console nothing**: Tekken Tag's ELF makes no `GetOsdConfigParam`,
  `GetMemorySize`, `MachineType`, ROMVER or NVRAM call; video mode is hard-coded; it reboots
  the IOP with `IOPRP165.IMG` and gates only on libmc/libpad versions. No anti-piracy logic.
- **PS1DRV carries region-specific per-title tables** (2.00 E: 182 SLES/SCES records; 1.60 A:
  71; 1.00 J: 175) of eight string-encoded parameters; neither Tekken is in them; 2.00 E ignores
  OSDSYS's argv and re-parses SYSTEM.CNF; 1.00/1.60 also load a card `TITLE.DB`.
- **PS2LOGO checks only ROMVER's region letter and the OSD video-output bit**; it passes
  `argv[1]` on; the kernel wipes RAM. (The games note's checksum "discrepancy" came from
  unstripped 2352-byte sectors; ps2kit's `rotl(b ^ key, 3)` + word sum is right.)

- **rom1 is the DVD player** (3.10 on the SCPH-70004, stamp `20040729`) with per-region
  `DVDID?`/`DVDVER?` and eight `EROMDRV?` MagicGate KELFs; it is ROM unit 1 registered by
  `rom0:ADDDRV` on the SSBUS during OSD/DVD IOP boots only. Without it a 1.50+ OSD accepts no
  card player either. The 70004's rom2 file is a byte mirror of rom1 (rom2 matters only for
  China). The stray `rom1.bin` is a 2.10 Oceania image from 2001 that PCSX2 never loads.
- **erom is mostly not encrypted**: 40 CRC-valid gzip members, the DVD player program as a
  raw image at `0x200000`, parental-control code and its own `eeprom` module; only the
  directory/loader KELFs are opaque. PCSX2 does not load erom at all.
- **NVM, verified end to end** (RPC fno → cdvdman ordinal → S-command): the 15+1-byte config
  block checksum, PCSX2's two layouts against real dumps, the region-parameter string
  `EEengEE` whose byte 6 selects every `?` suffix, and who reads what (OSDSYS: area 1, model
  name, region params; EECONF: areas 0/1; DVD player: area 2; the console IDs: nobody on
  retail). **PCSX2's default OSD block has a wrong 7-bit checksum**, the IOP rejects it, and
  the OSD runs on zeros — the "defaults" are never honoured.
- **All three MEC files are PCSX2's default**, parsed by XCDVDMAN as 6.02.00, which opens all
  ten version gates; a real 5.xx MechaCon would lose `sceCdReadRegionParams`.

## Tool-worthy, merged and ranked for Camp 4

1. **Hook map** — for the user's ROM + card + NVM + disc: which hand-overs would fire
   (card update file name and gate, DVD player override, HDD boot reachability, TESTMODE bit,
   `BootIllegal` verdict), each with its source address. (osdsys 1/3/5, kernel 1)
2. **NVM inspector** — layout by ROM version, checksum validation (flagging PCSX2's bad
   default), bytes `0x0F`–`0x14` and the option nibbles, region params, real-vs-fabricated
   classification; predicts the first-boot wizard, the menu items and the S-commands the OSD
   sends; plus EECONF's 0x60-byte MechaCon cache. (osdsys 4, kernel 3, rom1 1/2)
2b. **BIOS-set completeness check** — rom1 stamp vs rom0, DVD letter set vs NVM `0x186`, erom
   present and real, rom2 mirror detection, MEC = default. (rom1 3)
2c. **erom explorer** — inflate the 40 gzip members, show the player version, strings and the
   EROMDRV zone bytes, no key needed. (rom1 4)
3. **ELF / disc probe** — segments, SDK stamps, IRX list, syscalls *called*, libcdvd commands,
   hard-coded video mode, IOPRP module versions vs the ROM, libmc/libpad gates, SYSTEM.CNF key
   audit with who reads each key. (games 1/2/5)
4. **PS1DRV table + override view** — parse the user's PS1DRV table, match the inserted disc's
   ID, decode the parameters, check `PSD1.0.0`/`TITLE.DB`. (games 3)
5. **ROM explorer with the identity matrix** — collapse byte-identical modules across dumps,
   label orphaned code (ROMGSCRT, 1.60 A's write-only ROMVER copy), show the 1.60 E stub
   patches, EXTINFO dates. (kernel 8/10, osdsys 2)
6. **Reset-string / IOP module-set predictor** — `moduleload2`, UDNL's last-image-first lookup,
   `!include/!addr`, `IOPBTCON<mode>`: predict the exact IOP module set for any reset string.
   (kernel 2)
7. **Serial-log view** — the kernel's banner (CPUID, BoardID, ROMGEN, memory) and 1.00 J's
   `# Loader … pc=` narration, in boot order. (kernel 4/6)
8. **MechaCon-version lens** — XCDVDMAN's ten thresholds and the S-commands each unlocks.
   (kernel 5)
9. **Disc-type → action table** with per-version exceptions, and the argv vocabulary per ROM.
   (osdsys 6/7)

---

## Round two: PS1, PSone, POPS, PS2 1.00 and the PS3 update (2026-10-10)

Four more surveys over 15 PS1 dumps (SCPH-1000 … PSone 4.5, two "devkit" files), the PSP 6.60
POPS BIOS (`PSXONPSP660.BIN`), the PS1-mode pieces of six PS2 rom0s, and a PS3 `PS3UPDAT.PUP`.
Same [V]/[I] discipline. They feed the PSX range in `docs/campaign.md`.

| note | subject | lines |
|---|---|---|
| `ps1_kernel_boot.md` | reset → kernel → shell → CD boot decision, `SYSTEM.CNF` grammar, EXE loading, hidden checks, POST/TTY, hand-off list | 558 |
| `ps1_shell_scenes.md` | SCE intro, licence screen, no-disc menu: assets, timings, GTE constants, audio; loader plan for standalone dumps | 323 |
| `ps1_version_matrix.md` | identity table of every image, layout by code, kernel/shell clusters, revision deltas, devkit, `[h]`, POPS, PS2 PS1 mode | 460 |
| `ps3_pup.md` | PUP container, plaintext members, `update_files.tar`, SCE headers and descriptors, what is and is not reachable without keys | 417 |

### What we learnt

- **Two PS1 kernels in six years.** K1 (1.0/1.1/2.0) and K2 (2.2 Dec 1995 → PSone 4.5 → the
  PSP's POPS, byte-for-byte; 3.0 J/4.0 J differ by 24/21 bootstrap bytes). `GetSystemInfo(0)`
  answers `0x19951204` everywhere from 2.2 on. K1 → K2 removed a ~2 000-instruction ROM debug
  monitor (its command table survives as dead text in the SCPH-1000's kernel shadow) and added
  `GetSystemInfo`, a pre-exec `ReadTOC` + `GetID` re-check (`SystemError('D', 0x38B)`) and the
  user-RAM wipe. Layout, located by the copy loops: kernel ROM `0x10000` → `0xA0000500`
  (copy at `0x420`); shell ROM `0x18000..0x7FFF0` → `0x80030000` (copy at `0x6FF0`).
- **The boot logic lives in ROM, not in RAM**: ISO9660, EXE loader, card driver and every
  `SystemError` sit behind the A0 table at `0xBFC0xxxx`. A boot failure is an infinite loop with
  POST byte `0xF`, nothing printed. The shell gets `a0 = 7` and returns nothing.
- **The licence check is a property of the shell, not of the version number** (the three notes
  agree once read side by side): J shells 1.0–4.0 check text + the 0x3278-byte logo against a
  reference copy; U/E 2.2–4.1 store a constant 0 (the strings are dead data, no reference logo);
  2.0 E has no string; PSone 4.5 decides by ROM letter (`A` off, `E` Europe strings) exactly like
  the PS2's `rom0:LOGO`. A tool must read the flag store from the shell's code. On 3.0 A the only
  gate before `SYSTEM.CNF` is bit 7 of the drive's `GetID` byte; the SCEx letters are drawn
  off-screen and never compared.
- **`SYSTEM.CNF` has quirks a linter can teach**: the first line starting with a key decides and
  must continue with `=` (`BOOT2 = …` before `BOOT` hides the PS1 boot file — corrected against
  `rom:BFC00C38` after the R4 build; the kernel note first said the opposite), values are hex without prefix (`TCB = 10` is 16), missing keys are 0 not the ROM
  defaults, `STACK = 0` inherits the BIOS stack, and trailing text after the boot file is copied
  to RAM `0x180`. The PS-X EXE header is never validated; `t_size % 0x800 ≠ 0` loads nothing. The
  root directory is read as one sector, 40 entries: the boot file must be in its first 2 KB.
- **No pad, card or SIO on the boot path**; cards are never initialised before a disc boots. The
  Signetics 2681 DUART driver (DTL-H2000 board) ships uninstalled in every ROM. Expansion-ROM
  hooks are ungated string compares at `0x1F000084` (pre-kernel) and `0x1F000004` (post-shell).
- **The SCE intro is fully derivable**: grey background fade, a code-built diamond (one Gouraud
  quad + two sliding triangles over 60 fields), three logotype TIMs faded by CLUT interpolation,
  and a second 40-event note table over the *same* VAB as the licence chime (program 1, never
  keyed on the PS2, is the sweep). Re-rendered from the extracted parameters during the survey.
- **4.x shells are packed with the PS2 OSD's LZ** (stream at ROM `0x1804C`); our unpacker already
  opens them. Every `ps2kit::ps1` magic locator hits every PS1 shell; the standalone shells build
  the logo rotation from angles (260, −30, 0) at run time, which reproduces the PS2's baked
  matrix within ±1.
- **Two hosts, opposite choices.** The PS2 has no 512 KB PS1 BIOS: TBIN/SBIN replace the kernel,
  LOGO (a PSone-4.5 sibling, re-linked, intro and menu stripped) replaces the shell; only `KROM`,
  `KROMG`, `VERSTR` keep PS1 offsets. POPS kept the 1995 K2 kernel untouched and gutted the shell:
  it draws the ROM's own logo instead of the disc's, shows the disc text without comparing, no
  `GetID`, no region code (NTSC fixed), a 120-field hold, version string edited to `4.5 05/25/00 J`.
- **The user's files, audited**: `scph5501.bin` = the SCPH-7003 dump byte-for-byte; the
  "SCPH-1001 - DTLH-3000" file is plain SCPH-1001 (and DTL-H3002 = SCPH-1002's kernel; the
  devkit's permissiveness must be in the drive firmware); `[h]` is a text-mode-corrupted copy
  (CR before every LF from `0x24A`) with eight hand-edited TTY strings — unbootable; the
  `[SCPH39004]` PS2 file is labelled (U) but its PS1-mode letter is `E`; three PS2 duplicate pairs.
- **PS2 1.00 J (SCPH-10000)** fails today for an architectural reason: its opening is the
  `rom0:MOPEN` plug-in (raw LZ stream from offset 0, unpacks to 0x92C8C bytes at `0x500000`)
  and it has **no named asset table** — textures are inline. The 15 texture descriptors are the
  1.60 shape at a 0xE0 stride (width/height/format at +0x18/+0x1C/+0x28) with a pointer to the
  pixels at +8 instead of an asset index at +4 (first record at `0x51C678`). Slot, position,
  growth and orb tables are found by the existing content scans (`0x1D9B0`, `0x1DEE0`,
  `0x1E630`, `0x1D850` into the image). Also in the 1.00 loader: `mc0:/BIEXEC-SYSTEM/OSBROWS`
  (the browser replaceable from a card) and the Myst/Last Photo PS1 title fallbacks.
- **PS3 4.82 PUP**: nothing of the PS1/PS2 emulators or the boot splash is reachable without
  keys, and we will not go there. What is plaintext: the SCEUF header and digest table, version,
  the licence in 20 locales, the 48-member `update_files.tar`, every SCE header, an 0x80-byte
  per-package descriptor (kind, sequence, build stamp `2017-08-24 19:29:04`, sizes that reconcile
  exactly), `spkg_hdr.tar`'s alternate headers, and the updater SELF's ELF/program headers.

### Tool-worthy, merged and ranked for the PSX range

1. **PS1 identity card + audit** — any 512 KB dump (or the PS1-mode pieces of a PS2 rom0, by
   ROMDIR): date, model string, version/letter at `0x7FF32`, kernel cluster K1/K2, shell hash,
   packed/raw; duplicate, mislabel and text-mode-corruption detection. (matrix 1/2, kernel 3)
2. **Standalone licence screen** — detect (512 KB, no ROMDIR, SCE string at `0x108`), unpack the
   shell, matrix from angles, optional reference logo, licence policy read from the code, both
   wordmarks, 1 or 3 licence strings. Nearly all reuse. (scenes 1, scenes §5)
3. **PS1 hand-off card** — `SYSTEM.CNF` with the real grammar, EXE loadability, the 40-entry root
   rule, the per-shell check, the pre-exec re-check, and the drive's `GetID` answer printed as a
   declared assumption. Extends `Ps1Verdict`. (kernel 1/5/8)
4. **SCE intro with sound** — the PS1's signature scene, absent from the PS2. (scenes 2)
5. **PS2 1.00 J loads** — raw-LZ module path, 0xE0 inline-texture descriptors. (above)
6. **Host-modification view** — retail vs PS2-embedded vs POPS: kernel unchanged/replaced, shell
   calls and disc-read sequence compared, stripped strings, edited letter. (matrix 4)
7. **Revision diff + layout strip** — the 2.0 → 2.2 kernel diff as showcase; a 512 KB strip
   coloured by cluster. (matrix 5/6)
8. **Boot narration** — POST byte sequence and the dummy-TTY log rebuilt from the format strings;
   1.0 J monitor leftovers. (kernel 4/7)
9. **Asset browser** — TIMs, logo TMD, both note tables, VAB per-program use, KROM font. (scenes 4)
10. **PUP identity card** (+ PUP-vs-PUP diff) — zero keys, framed honestly as "what is this file",
    not a boot inspection. (pup a/c)
11. **Bring-your-own plaintext `dev_flash`** — a directory the user already has: `ps1_rom.bin`
    through item 2, `ps2_*emu` searched for a ROMDIR and reported found / not found. Accept a
    directory, never a PUP; never decrypt, fetch keys or name tools. (pup b)
12. **No-disc menu mock-up** — static screens from the shell's TIMs and screen table; last.
    (scenes 5)
