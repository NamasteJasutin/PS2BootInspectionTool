# Research: the hidden hand-overs of the PS2 boot ROM

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
