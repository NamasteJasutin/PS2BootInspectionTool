# rom1 / rom2 / erom, NVM and MEC: the companion images and the per-console data

Subject: everything a PS2 "BIOS set" carries besides `rom0`, and the per-console data the
boot ROM asks the MechaCon for. Files examined:

| file | size | identity |
|---|---|---|
| `SCPH-70004_BIOS_V12_PAL_200.ROM1` (project dir and PCSX2 bios dir, same md5 `82dc50fa…`) | 512 KiB | rom1 of a SCPH-70004, DVD player **3.10**, built `20040729-164054` (ROMDIR comment), 0x68D90 bytes used, rest `00` |
| `SCPH-70004_BIOS_V12_PAL_200.ROM2` | 512 KiB | **byte-identical to the ROM1 file** (`cmp` clean). The SCPH-70004 has no rom2 chip; the dumper read the rom2 window and got the rom1 image mirrored. Not a Chinese font ROM. |
| `SCPH-70004_BIOS_V12_PAL_200.EROM` | 3 MiB | the DVD player's "erom": **not a ROMDIR image, mostly plain gzip** (§2) |
| `…/PCSX2/bios/rom1.bin` | 199 680 B | a stray rom1 of an **SCPH-3000x-generation Oceania console**: DVD player **2.10**, region letter `O`, built `20010517-202844`, 0x30A30 bytes used; it belongs to none of the five rom0 images here (§1.4) |
| `SCPH-70004_BIOS_V12_PAL_200.NVM` (both copies, md5 `019c6720…`) | 1 KiB | real retail dump (full MechaCon tables) |
| `DTL-H30101_…_[6A001573].nvm` | 1 KiB | real devkit dump |
| `scph39001.NVM`, `PS2 Bios 30004R V6 Pal.NVM` | 1 KiB | PCSX2-made: zero except the OSD block at `0x310` |
| `scph10000.NVM` | 1 KiB | PCSX2-made, **entirely zero** |
| the three `.mec`/`.MEC` files | 4 B | all `03 06 02 00` = PCSX2's `DEFAULT_MECHA_VERSION`; **none is a real dump** (§4) |

Conventions: **[V]** read in the named code/data (addresses are module-relative for IOP
modules, virtual for EE images), **[I]** inferred, **[V-PCSX2]** read in PCSX2's
`pcsx2/CDVD/CDVD.cpp` / `CDVD_internal.h` / `ps2/BiosTools.cpp` (master, fetched 2026-10-09;
PCSX2 is the reference for *where in the 1 KiB file* a MechaCon command reads, because the real
MechaCon firmware is not readable). Not repeated here: `notes/research/osdsys_hooks.md` §2.2
(DVD-player launch conditions in OSDSYS per build) and §4.1 (the OSD config bit map),
`hidden_features.md` §4 (MechaCon branches in 1.50/1.60) and §5, `devkit_survey.md` §4.
Decompilations made for this note (scratchpad, deleted): rom0 2.00 `XCDVDMAN`, `XCDVDFSV`,
`XLOADFILE`, `ROMDRV`, `ADDDRV`, rom1 `UDNL`; EE disassembly of the 1.00 J OSDSYS and of the
decompressed DVD player.

---

## 1. rom1: the DVD player ROM

### 1.1 What is in the two images [V `tools/romdir.py`, `tools/extinfo.py`]

SCPH-70004 rom1 (`20040729`):

| entry | size | EXTINFO | note |
|---|---|---|---|
| `RESET`, `ROMDIR`, `EXTINFO` | 0x10, 0x350, 0x3CC | | standard ROMDIR header (the first 16 bytes of the chip are the build stamp `20040729-164054`) |
| `DVDID{J,M,U,E,O,R,A,C}` | 6 (J: 6) | date only | `"3.10<letter>\n"`, letter = the entry's own suffix |
| `DVDVER{J,M,U,E,O,R,A,C}` | 5–6 | date only | the **display** string: `3.10\n` (J), `3.10M`, `3.10U`, `3.10E`, **`3.10A` for O**, **`3.10D` for R**, **`3.10G` for A**, `3.10C` |
| `DVDCNF` | 0x19E | date | a mini ROM image (`RESET/ROMDIR/EXTINFO/IOPBTCONF`, stamp `20040729-164054`) whose `IOPBTCONF` lists the IOP module set for DVD playback: `@800 SYSMEM LOADCORE EXCEPMAN INTRMANP INTRMANI SSBUSC DMACMAN TIMEMANP TIMEMANI SYSCLIB HEAPLIB EECONF THREADMAN VBLANK IOMAN MODLOAD ROMDRV ADDDRV STDIO SIFMAN SIFCMD REBOOT LOADFILE CDVDMAN CDVDFSV SIFINIT FILEIO SECRMAN SIO2MAN DBCMAN SIO2D PADMAN2 RMMAN RMMAN2 LIBSD SDRDRV EESYNC` |
| `LOADCORE 2.05, TIMEMANI 2.02, SYSCLIB 2.03, THREADMAN 2.03, IOMAN 2.03, MODLOAD 2.08, ROMDRV 2.01, STDIO 2.03, SIFMAN 2.03, SIFCMD 2.08, LOADFILE 2.02` | | all dated `2004-07-29` | a **newer IOP kernel set** than rom0 2.00's; UDNL picks these over rom0's copies by version (§1.2) |
| `CDVDMAN 2.35`, `CDVDFSV 2.35`, `FILEIO 2.16` | 0x1687D, 0xC1A5, 0x45F5 | | the DVD-capable cdvd driver (rom0 2.00 ships `XCDVDMAN`/`NCDVDMAN`) |
| `SIO2MAN 3.00, DBCMAN 3.01, SIO2D 3.02, PADMAN2 3.04 (ds2u_d), RMMAN 1.16, RMMAN2 2.04` | | | pad + **two remote-control drivers** (RMMAN2 = the slim's built-in IR receiver) |
| `LIBSD 3.04, SDRDRV 4.01, EESYNC 2.01` | | | |
| `UDNL` | 0x1F75 | `2004-05-31` | IOP re-boot loader (differs from rom0's UDNL) |
| `EROMDRV{J,M,U,E,O,R,A,C}` | 0xF81 each | ver 1.01 | **MagicGate-encrypted KELF modules** (magic `01 00 00 01`), one per DVD region; the `erom0:` device driver (§2.3) |

`rom1.bin` (`20010517`): `DVDID` = `"2.10O\n"`, `DVDVER` = `"2.10A\n"`, `DVDCNF` (0x196, stamp
`20010517-202844`, module list `… SIO2MAN MCMAN MCSERV PADMAN RMMAN LIBSD SDRDRV EESYNC`),
`CDVDMAN 2.11`, `CDVDFSV 2.11`, `SIO2MAN 2.04`, `PADMAN 3.03`, `RMMAN 1.09`, `LIBSD 1.04`,
`SDRDRV 1.04`, `UDNL` (dated `2000-10-04`), one `EROMDRV` (0xFB1, ver 1.01, KELF). No kernel
modules: rom0's are used. So between 2001 and 2004 the rom1 grew from "only the modules that
must be newer than rom0" to "a complete IOP kernel" and from one region to eight. [V]

### 1.2 How `rom1:` comes to exist, and how its modules win over rom0's [V]

* `rom0:ROMDRV` (2.00, 1.60, 1.50, 1.00) manages **four ROM units** (table of 4 × 12 bytes at
  module `0x910`): unit 0 = `0xBFC00000` registered at init (ROMDIR searched in the first
  0x40000 bytes, `FUN_00000074`); units 1–3 are added by an export `AddRomUnit(unit, base)`
  (`FUN_000000e4`, searches 0x8000 bytes for a ROMDIR, error `-0xA2` if none) and removed by
  `FUN_000001a4`. Any module may register another ROM window as `rom<n>:`.
* `rom0:ADDDRV` (2.00/1.60/1.50; **absent from 1.00 J**) is a 0x160-byte module whose entry
  does: save `ssbusc GetDelay(1)`/`GetBaseAddress(1)`, set `SetBaseAddress(1, 0xBE000000)`,
  `SetDelay(1, 0x0018344F)`, then `romdrv AddRomUnit(1, 0xBE000000)`; on failure it prints
  `ROM directory not found` and restores both registers. `ssbusc` index 1 is the SSBUS register
  `0xBF801400` (table at ssbusc `0x36C`). So **`rom1:` = ROM unit 1 at IOP/EE physical
  `0x1E000000`** — exactly where PCSX2 maps `rom1` (`Memory.cpp`: `vtlb_MapBlock(eeMem->ROM1,
  0x1e000000, 4 MB)`; rom2 at `0x1e400000`). [V ADDDRV disassembly, V-PCSX2]
* ADDDRV is in the IOP module list of the **OSD** boot (`rom0:OSDCNF` of 1.50/1.60/2.00) and of
  the **DVD** boot (`rom1:DVDCNF`), not in the cold boot (`rom0:IOPBTCONF`) nor in the game boot
  (`rom0:EELOADCNF`). A game therefore has no `rom1:` unless it reboots the IOP with its own
  list. [V the four lists]
* `UDNL` (the loader that `sceSifIopReset("rom1:UDNL rom1:DVDCNF")` runs): it builds its
  image list from (a) rom0 at `0xBFC00000` (label `ROM`), (b) **the ROM at `ssbusc
  GetBaseAddress(1) | 0xA0000000`** (label `ROM2` in the code, i.e. the rom1 chip), (c) every
  file named on its command line (read with ioman, scanned for a ROMDIR in the first 0x1000
  bytes), then resolves each name of the final `IOPBTCONF` across all images
  (`FUN_0000016c`, `FUN_00001250` looks for the `RESET`+`ROMDIR` signature; `IOPBTCONF`
  supports `!addr` and `!include`). Which copy wins when two images hold the same name is
  by version/date comparison [I: the EXTINFO versions in rom1 are all ≥ rom0's; not traced
  further]. This is why the 70004 rom1 carries a full kernel set.

### 1.3 The DVD-Video launch chain, the IOP/MechaCon half [V unless noted]

OSDSYS side (per build, conditions, argv): `osdsys_hooks.md` §2.2. What happens after
`LoadExecPS2("moduleload2 rom1:UDNL rom1:DVDCNF", {"-k rom1:EROMDRV<L>", "-m erom0:UDFIO",
"-x erom0:DVDPL<L>"})` (2.00; 1.50/1.60: `EROMDRV`, `DVDELF`, no letter):

1. EELOAD resets the IOP with `rom1:UDNL rom1:DVDCNF` (§1.2) — the IOP now runs rom1's
   `CDVDMAN 2.35` etc.
2. `-k` → EELOAD `LoadModuleEncrypted` (`0x82AB0`) → LOADFILE MG module load; `-x` →
   `LoadElfEncrypted` (`0x82C40`, LOADFILE fno 5 `LF_F_MG_ELF_LOAD`) [`analysis/symbols/eeload.tsv`].
   The MG loads are serviced by `SECRMAN` through the MODLOAD/LOADFILE callbacks it registers
   (SECRMAN imports `modload`; XLOADFILE itself imports no `secrman`) [V imports]; SECRMAN's
   KELF handshake is the `mechacon auth 0x80…0x8F` / `sceMgWriteHeaderStart 0x90` /
   `sceMgReadKbit/Kcon 0x94–0x97` sequence (`hidden_features.md` §4.2; PCSX2 implements the
   header parse at `CDVD.cpp` case `0x8F`: checks `u16[0x14]` = header size, prints zones from
   byte `0x1C`, extracts Kbit/Kcon from the BIT table) [V-PCSX2].
3. `EROMDRV<L>` is therefore decrypted by the MechaCon's keys, and only then can it register
   the `erom0:` device. The player ELF (`erom0:DVDPL<L>` / `DVDELF`) is loaded through that
   device *and* MG-decrypted again by `-x`. `UDFIO` (an IRX from erom) is the UDF file-system
   driver for DVD-Video [I from its name and the player's `VIDEO_TS.IFO` strings].
4. The MechaCon's **region gate** is the KELF header: each `EROMDRV<L>` has a different zone
   byte at header offset `0x1C` (`J` 0x01, `U` 0x02, `E` 0x04, `O` 0x08, `A` 0x10, `R` 0x20,
   `C` 0x40, `M` 0x80 — PCSX2's `mg_zones` order; verified `E` = `0x04` in `EROMDRVE`, `O` =
   `0x08` in rom1.bin's `EROMDRV`) and a different encrypted body (259 differing bytes between
   any two of the eight 70004 copies, all after the header). A real MechaCon refuses a KELF
   whose zone bits do not include its own zone (PCSX2 only logs them) [V headers, I enforcement].
   Header fields read: `0x10` u32 = ELF size `0xF01` (= file size − `0x80` header),
   `0x14` u16 = header size (`0x80` in 2004, `0xB0` in 2001), `0x18` u16 flags `0x022C`,
   `0x1C` zone. [V]

### 1.4 Versions, provenance, and the memory-card player [V]

* **Version Information "DVD Player" line** (2.00): module 6's init `FUN_00206868` opens
  `rom1:DVDID<L>` (L = NVM region letter, §3.4; `rom1:DVDID` if the MechaCon gives none),
  parses `major.minor<letter>` into `0x2DA458/5C/60`, then reads **`rom1:DVDVER<L>`** (≤ 31
  bytes) into `0x2DA438` — that string, e.g. `3.10E`, is what the page prints. With no rom1
  the entry's version string stays empty and the line is omitted (`FUN_00206CC8` skips modules
  whose version string is empty). 1.50/1.60: same with `rom1:DVDID`/`DVDVER`. 1.00 J: the
  version comes from the **memory card** file `/BIEXEC-DVDPLAYER/dvdplayer-j.ver` (Japanese UI)
  or `-e.ver` (English UI), read via the mc RPC (`0x201670`); nothing in rom0 1.00 J names
  `rom1:` at all.
* **Card-player acceptance** (1.50+; `FUN_002061E8`): the card's `dvdplayer.id` must carry the
  **same region letter** as `rom1:DVDID` and a **strictly higher** `major.minor`, and its
  `dvdplayer-e.ver` must be neither `"1.00\n"` nor `"1.01\n"` (the two original memory-card
  players are blacklisted in every 1.50+ build — the literals are present in 1.50, 1.60 and
  2.00). Consequence verified in the 2.00 disassembly (`0x2063A0`: `beq $s3,-2` is the only
  bypass and `-2` is never produced by the parser): **a BIOS set without rom1 can never run a
  memory-card DVD player on 1.50+**, and the ROM player is of course gone too → the DVD-Video
  launcher returns 5 → message `0x61` "DVD Player is not set up."
* `rom1.bin` provenance: rom1 images carry the same `YYYYMMDD-HHMMSS` stamp as their rom0;
  `20010517` matches none of `20000117` (1.00 J), `20001228` (1.50 dev), `20011004` (1.60 E),
  `20020207` (1.60 A), `20040614` (2.00 E). Its DVD letter `O` and KELF zone `0x08` (Oceania)
  place it with an SCPH-3000x sold in Australia/NZ [I: model]. PCSX2 never loads it: `LoadExtraRom`
  only tries `<bios>.rom1` and `<bios basename>.rom1` (case-insensitive), so a file literally
  named `rom1.bin` is ignored unless the BIOS itself is `rom1.bin` [V-PCSX2].
* The eight `DVDVER` display suffixes are not the region letters (`O`→`A`, `R`→`D`, `A`→`G`,
  `J`→none). A tool should print `DVDVER<L>` as the user-visible string and `DVDID<L>` as the
  compare key.

---

## 2. erom: the "encrypted" DVD player image

### 2.1 Format [V data]

* No ROMDIR, no `RESET` string, no ELF header, no KELF magic at offset 0. Overall byte entropy
  7.991 bits; the histogram is flat (9 925–16 585 per value, expected 12 288).
* **40 complete gzip members** (`1F 8B 08 00`, mtime 0, xfl 2, OS 3 = Unix, no name) at
  `0x44EB4 … 0x2FFC4C`; every one inflates with a correct CRC-32 and ISIZE. Inflated total
  ≈ 21 MB. They are plaintext resource files and one program:

| members (erom offset) | inflated size | first bytes | what [I unless noted] |
|---|---|---|---|
| `0x44EB4` | 13 676 | pointer table at EE `0x6A2218…` | resource index |
| `0x47A50`–`0xA993C` (14) | 42–87 KB (four are exactly 55 572) | tables at `0x643A98…` | per-language OSD resources (the player supports ~14 OSD languages: `SUBGTIDITALNACRKGSHCESSEHTWTNCKDEDPJONHPIFRFEBKHYMUR` = DVD language-code list in the program) |
| `0xAF350`–`0xE59E0` (5) | 144–299 KB | tables at `0x659018…` | graphics sets |
| `0xF88F8`–`0x26D164` (13) | 330–511 KB | tables at `0x5C6E30…` | graphics sets |
| `0x28E9F0`–`0x2A45CC` (5) | 22–38 KB | tables at `0x5BD948…` | |
| **`0x2B1768`** | **3 776 964** | `08 00 E0 03 78 81 84 AF` = `jr $ra; sw …` | **the DVD player program itself, as a raw memory image loaded at `0x200000`** [V: `jal` targets cluster in `0x200000–0x2BFFFF`, data references in `0x500000–0x5Cxxxx`; the image is 0x39A000 bytes, ending just below the resource tables] |
| `0x2FFC4C` | 3 832 | zeros | padding/bss seed |

* Between the gzip members sit **41 high-entropy gaps** (entropy 7.77–7.997): the first is
  `0x0–0x44EB4` (282 KiB), the others 0x14–0xC92F bytes each, 0x19C bytes of tail. These hold
  whatever is encrypted: the `erom0:` directory, `UDFIO`, the eight `DVDPL<L>` loader KELFs
  (the OSDSYS `-x` target), and possibly more resources [I: the names are known from OSDSYS,
  their location in the gaps is not provable without decrypting EROMDRV].
* The program contains the strings `PsIIlibcdvd 2801`, `PsIIlibgraph2801`, `PsIIlibipu 2800`,
  `PsIIlibpkt 2800`, `PsIIlibpad2 2800`, `rom0:ROMVER`, `rom0:OSDVER`, `rom0:ADDDRV`,
  `rom0:OSDSYS`, the model strings `DTL-T10000`, `DTL-H10000`, `SCPH-10000`, module names
  `player`, `eeprom`, `inport`, `demux`, `advplay`, `navi2/presen2/navi4vr/presen4vr`,
  `ER_PLAYER_PARENTAL`, `CommandParentalLevelSelect`, `CommandSetTmpPML`, VR-mode file names
  (`VR_MANGR.IFO`, `VR_MOVIE.VRO`), karaoke/remote key names, and the version-mismatch
  checks `librm/rmman.irx`, `librm2/rmman2.irx`. It reads `rom0:ROMVER`/`OSDVER` itself and
  has its own `eeprom` (NVRAM) module with `sceCdOpenConfig` (wrapper at `0x284328`, same
  `(mode | block<<8 | n<<16)` packing as OSDSYS) — the parental level/password and the
  player's own settings live in NVM config area 2 (§3.3) [I for the area]. **PCSX2 cannot run
  any of this**: it does not load the erom file at all (`BiosTools.cpp` loads only `rom1`/`rom2`;
  `MemoryTypes.h` has no EROM region), and even with rom1 the `-k rom1:EROMDRV` KELF would need
  real MechaCon decryption, which PCSX2 fakes (`0x8D/0x8E` pass data through). [V-PCSX2]
* The claim "erom is encrypted" is therefore **half true**: the executables and the directory
  are, the bulk (gzip) is not. A tool can inflate the 40 members, extract the player's strings
  and version, and show the eight-region EROMDRV zone bytes without any key.

### 2.2 Who loads what, when [V]

`EROMDRV<L>` (rom1, KELF) → MG-loaded by SECRMAN/MODLOAD at the DVD boot → registers `erom0:`
→ `UDFIO` and `DVDPL<L>` are opened through it → `DVDPL<L>` is MG-loaded by `-x` → it inflates
the gzip members from the erom into RAM (`0x200000` program, `0x5Bxxxx–0x6Axxxx` resources)
[I for the last step: the gzip members are the only plaintext, the loader must be the KELF].
Nothing in rom0 2.00 references `erom0:` except OSDSYS's three argv strings; `TESTMODE`
(1.50/1.60/2.00) only reads `rom1:ROMDIR` ("can't read rom1:").

---

## 3. NVM: the 1 KiB MechaCon EEPROM

### 3.1 How ROM code reaches it [V]

All access is through MechaCon **S-commands** (CDVD regs `0x1F402016/17/18`) issued by
`XCDVDMAN` (`FUN_0000413C(cmd, params, nparams, result, nresult)` in 2.00) and exposed to the EE
through `XCDVDFSV`'s S-command RPC server (OSDSYS calls `sceSifCallRpc(0x414A20, fno, …)`).
Mapping established from the XCDVDFSV 2.00 dispatcher (`FUN_0000738C`) → cdvdman import
ordinal → XCDVDMAN function → S-command byte:

| EE RPC fno | cdvdman ord | XCDVDMAN 2.00 | S-cmd | params → result | PCSX2 source of the bytes |
|---|---|---|---|---|---|
| `0x06` / `0x07` | 22 / 23 | `FUN_000067E8` | `0x12` / `0x13` | → 1 status + 8 bytes | `ilinkId` |
| `0x08` / `0x09` | 26 / 27 | `FUN_00007CA0` ("ReadNVM call addr=…") | `0x0A` / `0x0B` | **word address** (big-endian 2 bytes) → status + 2 data bytes; PCSX2: `address*2 < 1024` | any offset, 16 bits at a time |
| `0x12` / `0x13` | 41 / 42 | `FUN_00006BDC` / `FUN_00006C58` | `0x03` sub `0x45` / `0x44` | → status + 8 | `consoleId` |
| `0x14` | 43 | — | `0x03` sub `0x00` (`sceCdMV`, `FUN_00006CE4`, "MECACON Version …") | → 4 bytes | the `.mec` file |
| `0x1A` | 64 | `FUN_0000685C` (`sceCdRM`) | `0x17` × 2, param = part 0 / 8 | → status + 8 each = 16-byte model string; **only if MechaCon ≥ 1.05.00, else the literal `M_NAME_UNKNOWN`** | `modelNum` |
| `0x0E` / `0x0F` | 31 / 32 | `FUN_00008278` / `FUN_0000830C` | `0x40` params `[mode, block, n]` / `0x43` | OSDSYS passes `(block, mode, n)` → bytes `mode | block<<8 | n<<16`; n < 0x45 | config areas |
| `0x10` / `0x11` | 33 / 34 | `FUN_00008348` / `FUN_00008540` | `0x41` / `0x42` | 16 bytes per block: **15 data + 1 checksum = low byte of the sum of the 15**; read verifies and flags status 1 on mismatch, write computes it. The RPC returns **15 bytes per block** (OSDSYS `memcpy(dst, buf+8, n*0xF)`) | |
| `0x43` | 189 | `FUN_00007A60` | `0x36` | → status + 14 bytes; **refused with status `0x100` unless MechaCon ≥ 6.00.00** (`DAT_00009C1C`) | `regparams` + mecha zone |
| — (EECONF, direct register access) | — | `rom0:EECONF` | `0x40–0x43` | reads area 0 blocks 0–3 and area 1 blocks 0–1 at every IOP boot (`EECONF 0x6DC…`), mirrors area-1-block-1 byte 0 bit 3 (component video) into its in-RAM copy, and under a CDVD-status condition zeroes area-1 block 0 | |

Who calls what in rom0 2.00: OSDSYS uses only OpenConfig/ReadConfig/CloseConfig (`0x203DA8`),
Open/WriteConfig (`0x20F928`), `sceCdRM` (`0x205D28` → Version page "Console" line, `Unknown`
if empty), `sceCdReadRegionParams` (`0x2050D0`). Its ILinkID/ConsoleID/ReadNVM/WriteNVM
wrappers (`0x25B3C8`, `0x25B768`, `0x25C4B0`, `0x25C5B0`, …) **have no callers** (dead libcdvd
code). Among IOP modules, only `CDVDFSV`/`XCDVDFSV` import the ID functions (plus `HDDLOAD`
in 1.50/1.60 importing `sceCdReadILinkID`, for the HDD-boot path that this ROM never takes).
KERNEL and EELOAD touch nothing. So on a retail boot the NVM bytes that *matter* are the config
areas, the model string, and (2.00 only) the region parameters. [V]

### 3.2 PCSX2's two layouts, checked against the dumps [V-PCSX2 table, V occupancy]

`nvmlayouts[]` (`CDVD_internal.h`): `{biosVer, config0, config1, config2, consoleId, ilinkId,
modelNum, regparams, mac}` =

| | ROM < 1.70 (`0x000`) | ROM ≥ 1.70 (`0x146`, i.e. "1.46" as hex compare of BCD "0170"? the code compares `biosVer <= BiosVersion` with `BiosVersion` = `0x0200` for 2.00; both 1.60 ROMs select layout 0, 2.00 selects layout 1) |
|---|---|---|
| config area 0 ("config0") | `0x280` | `0x270` |
| config area 1 ("config1") — the OSD area | `0x300` | `0x2B0` |
| config area 2 | `0x200` | `0x200` |
| consoleId (8) | `0x1C8` | `0x1F0` |
| ilinkId (8, +2 trailer in layout 1) | `0x1C0` | `0x1E0` |
| modelNum (16) | `0x1A0` | `0x1B0` |
| regparams (12 used) | `0x180` | `0x180` |
| "mac" (8; S-cmd `0x37`, "used by EECONF" per PCSX2; not found in this EECONF) | `0x198` | `0x198` |

Config area/block → file offset: `area_base + 16 × block`; `cdvdReadConfig` allows 4 blocks in
area 0, 7 in area 2, unlimited index in area 1. OSDSYS's `OpenConfig(1,0,2)` → **area 1 blocks 0
and 1** = `0x300–0x31F` (layout 0) / `0x2B0–0x2CF` (layout 1); the OSD settings are in **block 1**
(`0x310` / `0x2C0`), block 0 (`0x300` / `0x2B0`) byte 0 holds the PS1-driver option bits. Both
agree with every file: the devkit and the two PCSX2 1.60 files have their block at `0x310`, the
SCPH-70004 at `0x2C0`.

### 3.3 Byte-offset map

Offsets are file offsets; "L0/L1" = which layout. Values are described, not quoted, for
identifiers. "reader" = who consumes it in the ROM set.

| offset | L | field | SCPH-70004 (real) | DTL-H30101 (real) | PCSX2-made files | reader / effect |
|---|---|---|---|---|---|---|
| `0x000–0x12F` | both | MechaCon calibration/parameter tables (`FF` = erased past `0x0B4`, more tables `0x0C0–0x12F`, `0x140–0x171`) | present | `0x000–0x08F` only | `00` | nothing in rom0/rom1; MechaCon-internal (laser/servo values) [I] |
| `0x180–0x18B` | both | **region parameters** (12): `[0]` ROM region letter (= `ROMVER[4]`), `[1]` OSD region letter (→ `OSDVER[4]`: `J A E C R K H`), `[2..4]` default OSD language (`jpn eng fre spa ger ita dut por rus kor tch sch`), `[5]` PS1 BIOS region letter (= `VERSTR[0x22]`), `[6]` **DVD-player region letter** (`J M U E O R A C`), `[7..11]` zero | `EEengEE` + 5×`00` | `"300"` + binary (`33 30 30 14 67 11 2C 01 05 28 00 79`) — a pre-PStwo MechaCon stores something else here [I] | 2.00 files: PCSX2 writes `PStwoRegionDefaults[region]` (e.g. `EEengEE`, `AAengAU`, `JJjpnJJ`, `CCschJC`); <2.00: left zero | S-cmd `0x36` result bytes `[3..10]` → OSDSYS 2.00 `0x27B3F0[2..9]`: `[3..6]` patch `rom0:OSDVER`'s `????` (`FUN_00205160`) → `ConsoleRegion`, `PickDefaultLanguage`; `[2]` (file `0x180`) picks `rom0:PS1VER<L>` (`FUN_00205288`); `[8]` (file `0x186`) picks `rom1:DVDID<L>`, `DVDVER<L>`, `EROMDRV<L>`, `erom0:DVDPL<L>` (`FUN_002052D0`). **1.50/1.60 never read it** (no fno `0x43`). |
| `0x18C–0x197` | both | erased | `FF` | `FF` | `00` | — |
| `0x198–0x19F` | both | "mac" (PCSX2 name) | `00 04 1F 00 00 00 1F 04` | `FF` | `00` | S-cmd `0x37`; no ROM caller found |
| `0x1A0–0x1AF` | L0 | model string | `FF`, `[0x1AF] = 07` | `DTL-H30101` + `00` | `00` | `sceCdRM` → Version page "Console" |
| `0x1B0–0x1BF` | L1 | model string | `SCPH-70004` + 6×`00` | `00 4E`, then `FF` | `00` | idem; PCSX2 files → **"Unknown"** on the Version page (`FUN_00205D28`) |
| `0x1C0–0x1C7` | L0 | i.LINK ID (8) | 8 bytes of pattern `00 xx FF FF FF FF yy zz` (same shape as PCSX2's default `00 AC FF FF FF FF B9 86`) | 8 bytes, no `FF` run | `00` (these three files pre-date PCSX2's default-ID generation; current PCSX2 writes the default + `00 18` trailer in L1) | S-cmd `0x12`; no ROM reader on retail; HDDLOAD on 1.50/1.60 |
| `0x1C8–0x1CF` | L0 | console ID (8) | 18 bytes of data `0x1C8–0x1D9` then `FF` (layout-1 console does not follow L0 here) | 8 bytes; bytes 2–3 = `11 01` | `00` | S-cmd `0x03/0x45`; no ROM reader |
| `0x1E0–0x1E9` | L1 | i.LINK ID (8) + 2-byte trailer | present | 16 bytes of data `0x1E0–0x1EF` (not this field on L0) | `00` | idem |
| `0x1F0–0x1F9` | L1 | console ID (8) + 2-byte trailer; bytes 2–3 = `11 01` as on the devkit | present | `0x1F0–0x1FB` data | `00` | idem |
| `0x200–0x22F` | both | config area 2, blocks 0–2 | `00` | `00` | `00` | — |
| `0x230–0x26F` | both | config area 2, blocks 3–6 (checksummed) | `03 02 01 00 … 02 01 01` / `04 … 53 55 65 64 65 64 65` / `64 00 00 CD AB 00 00 10 03` / `… 33 D5 05 04 … 11` | `00` | `00` | **only the real slim has them** → written by the DVD player (`eeprom` module: parental level/password, language, display settings) [I]; the OSD never opens area 2 |
| `0x270–0x27F` | L1 | config area 0, block 0 | `00 03 00 … 03` | `FF` | `00` | EECONF reads area 0 blocks 0–3 at IOP boot; purpose of the bytes unknown |
| `0x280–0x28F` | L0 blk 0 / L1 blk 1 | config area 0 | `40 00 00 00 00 10 … 50` | `01 00 … 01` | `00` | idem |
| `0x290–0x29F` | L0 blk 1 | config area 0 | `00` | `40 00 … 40` | `00` | idem |
| `0x2B0–0x2BF` | L1 | **area 1 block 0**: byte 0 bits 0/4 = PS1 driver Disc Speed / Texture Mapping; bytes 1–14 = option nibbles of the Version-page "Options" entries | `00` | — | — (PCSX2 only writes block 1) | OSDSYS `LoadNvramConfig` → `ctx[0x1224..]` → `SetOsdConfigParam.ps1drvConfig` → PS1DRV |
| `0x2C0–0x2CF` | L1 | **area 1 block 1 = OSD config** (`osdsys_hooks.md` §4.1 for the bits) | `35 21 C0 3C 20 25 … 97`: Digital Out off, 16:9, RGB, English, initialised, date format 2, tz offset +60 min, tz index 37, bit "remote-control item shown" | — | — | OSDSYS (every build) via `OpenConfig(1,0,2)`; checksum `0x97` ✓ |
| `0x300–0x30F` | L0 | area 1 block 0 (PS1 driver bits, as above) | — | `00` | `00` | idem |
| `0x310–0x31F` | L0 | area 1 block 1 = OSD config | `00` (not used on L1) | `3D 21 AE 20 00 73 … 9F`: Digital Out off, 16:9, **component**, English, initialised, DST, date format 1, tz offset −480 min (UTC−8), tz index 115 | 39001: `30 21 81 0E … E0` (English, initialised, tz +270 min); 30004R: `30 21 86 D4 00 14 … BF` (English, initialised, tz −300 min = UTC−5, index 20); 10000: zero | idem |
| `0x320–0x3FF` | both | 224 bytes of key material (MagicGate / per-console keys; repeated 8-byte groups at `0x340`, `0x35x/0x36x/0x37x` share a tail) | present | `00`/`FF` | `00` | not readable through any S-command the ROM issues (ReadNVM could, nothing does) [I] |

The 11-bit time-zone offset and the 9-bit time-zone index are both in the block: offset at
block-1 bytes `[2].0-2`+`[3]` (signed minutes: `0x6D4` → −300, `0x620` → −480, `0x03C` → +60),
index at `[4].0`+`[5]` (`CfgTimezone` `0x204570`: values ≥ `0x80` are shown as "custom" `0x80`,
the raw value kept in `0x27A848`). The checksum rule holds for every non-zero block in every
file (also for the PCSX2-written ones, because the *IOP side* computes it, not the emulator).
[V]

### 3.4 Fabricated vs real, and what differs between the files [V]

* **PCSX2 creates** (`cdvdCreateNewNVM`): zeros; region params (2.00+ ROMs only, from
  `PStwoRegionDefaults`); the default i.LINK ID (+ `00 18` on L1); the OSD block from
  `biosLangDefaults[region]` (`30 21 00 00 00 70 … 41` = English, *not* initialised, tz index
  `0x70`; Japan `20 20 … 30`; China `30 2B … 4B` = sch). It re-creates the file whenever the OSD
  block is all zero or (2.00+) the region params are. Nothing else is ever synthesised: no
  model string (→ "Unknown"), no MechaCon tables, no console ID, no area-2 blocks. The three
  PCSX2 files here are older than the ID default (ilink slot zero) and `scph10000.NVM` is
  entirely zero (PCSX2 will regenerate it with the Japan block on next use).
* **Real dumps** are recognisable by: non-zero bytes below `0x180`, a model string, `FF` runs
  (erased EEPROM), data at `0x320+`, and for a slim the area-2 blocks and the `0x2C0` OSD block.
* The two layouts are visibly different in the dumps: the devkit has nothing at `0x1B0`,
  `0x1E0`, `0x2C0` and everything at `0x1A0`, `0x1C0/0x1C8`, `0x280/0x290`, `0x310`; the slim
  the reverse (and its `0x1C0` slot still holds an 8-byte ID-like value). Which layout a ROM
  expects is decided by the ROM version — in reality by the MechaCon generation that shipped
  with it; PCSX2's `0x146` threshold is its approximation.

---

## 4. MEC: the MechaCon version

* `sceCdMV` = S-cmd `0x03` sub `0x00` → 4 bytes. XCDVDMAN keeps byte 0 as a status
  (bit 7 = busy), builds **`version = b1<<16 | b2<<8 | b3`** (`0x685C–0x68AC`:
  `lbu 0x19/0x1A/0x1B($sp)`), and derives its gates from it (`FUN_00003430`): [V]

| gate | 1.60 | 2.00 | what it enables (2.00) |
|---|---|---|---|
| ≥ 1.05.00 | — | `sceCdRM` | model-name read (else `M_NAME_UNKNOWN`) |
| ≥ 1.08.00 | `DAT_8A6C` | `DAT_9BFC` | disc-change/tray handling (`hidden_features.md` §4.1) |
| ≥ 2.02.00 | `DAT_8A70` | `DAT_9C00` | DVD read-timing path |
| ≥ 2.04.00 | — | `DAT_9C04` | `sceCdCancelPOffRdy` (S-cmd `0x1B`) |
| ≥ 2.08.00 | — | `DAT_9C08` | set, no user |
| ≥ 5.00.00 | — | `DAT_9C0C` | S-cmds `0x1D 0x1F 0x21 0x22 0x24 0x25 0x26` and the N-cmd wrapper `0x49DC` (wake-up time, RC bypass, …) |
| ≥ 5.02.00 | — | `DAT_9C10` | S-cmd `0x27` (PCSX2: "GetPS1BootParam, China models") |
| ≥ 5.04.00 | — | `DAT_9C14` | S-cmd `0x28`, `0x03/0xEF` (PCSX2: console temperature) |
| ≥ 5.06.00 | — | `DAT_9C18`; `DAT_9C24 = (b3 & 0xF) == 1` | tray-request test path `0x7E20` |
| **≥ 6.00.00** | — | `DAT_9C1C` | **`sceCdReadRegionParams`** (S-cmd `0x36`) — the only way OSDSYS 2.00 learns the OSDVER letters |

* PCSX2 reads the 4 bytes verbatim from `<bios>.mec` and returns them as the S-cmd result; its
  default `0x00020603` is stored little-endian, so the console sees `03 06 02 00` → status/zone
  byte `0x03`, **version 6.02.00**. That is why the default satisfies every gate above
  (including the region-params one, without which a 2.00 BIOS would boot with a `0200????`
  OSDVER and fall back to `ROMVER`'s region). PCSX2 also uses byte 0 as the MechaCon's
  **MagicGate zone index** (`0x36` result `[1] = 1 << mec[0]`, `mg_zones[3]` = "Oceania"), which no
  ROM code reads. [V-PCSX2, V XCDVDMAN]
* The three `.mec` files on disk are all exactly that default (the 70004's is dated 2025-09-19,
  the day PCSX2 first saw that BIOS). **No real MechaCon version is available here**; the real
  byte order of a dumped MEC (whether PS2 tools store `[status, b1, b2, b3]` as the MechaCon
  returns it) and the SCPH-70004's actual firmware number (XCDVDMAN 2.00's 5.0x–5.06 gates
  suggest the 7000x generation is 5.xx or 6.xx) could not be determined. [I]

---

## 5. Cross-ROM: what each rom0 expects beside it

| rom0 | `rom1:` references | DVD player comes from | `ADDDRV` in `OSDCNF` | region letters from | `rom1:` names used | `erom0:` | `rom2:` |
|---|---|---|---|---|---|---|---|
| 1.00 J `0100JC20000117` | **none** (no ADDDRV, no UDNL arg, no TESTMODE string) | memory card only (`mc?:/BIEXEC-DVDPLAYER/dvdplayer.elf`, `dvdplayer-j/-e.ver`) | no (module absent) | — | — | — | — |
| 1.50 dev `0150AD20001228` | OSDSYS (`rom1:DVDID`, `DVDVER`, `EROMDRV`, `UDNL`, `DVDCNF`), TESTMODE (`rom1:ROMDIR`) | rom1 + erom, or a newer card player | yes | `ROMVER[4]` only | unsuffixed | `UDFIO`, `DVDELF` | — |
| 1.60 A `0160AC20020207`, 1.60 E `0160EC20011004` | same | same | yes | `ROMVER[4]` only | unsuffixed | same | — |
| 2.00 E `0200EC20040614` | same + letter-suffixed names, `rom2:` | same | yes | NVM region params via MechaCon ≥ 6.00 (`OSDVER`), else `ROMVER[4]` | `DVDID?`, `DVDVER?`, `EROMDRV?` (fallback unsuffixed) | `UDFIO`, `DVDPL?` (fallback `DVDELF`) | **China only**: `ConsoleRegion()==3` → `rom0:ADDROM2` is loaded (`0x20A048`) and the fonts `GB18030`/`FNTASCI2` and texture `TEXOPNGM` are taken from `rom2:` (`LoadAssetArchives` `0x206E58`); regions 4–6 (Russia/Korea/Asia) swap only `TEXOPNGW`. The PAL 2.00 ROM has no `ADDROM2` file, so on this console the `rom2:` path is dead. |

Companion-image expectations for a tool ("your BIOS expects…"):
* 1.00 J: nothing; a `.rom1` would be ignored by the ROM (PCSX2 would still map it).
* 1.50/1.60: a rom1 with one `DVDID`/`DVDVER`/`EROMDRV`/`UDNL`/`DVDCNF` (+ `CDVDMAN 2.1x`
  etc.) and an erom; the pair must share the rom0's build stamp [I: convention, verified only
  for the 70004 pair].
* 2.00: a rom1 with the eight-letter sets, an erom, and an NVM whose `0x186` letter selects the
  set; plus, for a Chinese console, a rom2 and `rom0:ADDROM2`.
* Any build: `ADDDRV` + `ROMDRV` register `rom1:` at `0x1E000000` only during the OSD/DVD IOP
  boots, so a game never sees it.

---

## 6. Tool-worthy findings (ranked)

1. **NVM inspector** — parse the 1 KiB file with the layout chosen from `ROMVER` (§3.2),
   verify every 16-byte config block's checksum (§3.1), decode the OSD block (language, screen,
   output, DST, time format, time-zone offset/index, "initialised", remote/progressive bits),
   the PS1-driver bits, the region-parameter string (`EEengEE` → ROM/OSD region, language,
   PS1 and DVD letters), the model string, and classify the file as *real dump* vs
   *PCSX2-fabricated* (§3.4) — e.g. "Version page will show Console: Unknown", "first-boot
   wizard will run", "area-2 DVD settings present → came from a slim".
2. **BIOS-set completeness / provenance check** — per rom0 (§5): is a rom1 present, does its
   build stamp match, does it carry the letter set the NVM's `0x186` selects, is the erom
   there (and is it real: 40 gzip members with CRCs), is rom2 a mirror of rom1 (SCPH-70004
   case), does the `.mec` equal PCSX2's default. Explain the consequences: "no rom1 → the
   card DVD player can never be accepted either" (§1.4), "PCSX2 ignores erom and cannot run the
   DVD player regardless" (§2.1).
3. **DVD-player provenance** — show `DVDID<L>` (compare key) vs `DVDVER<L>` (display string),
   the EROMDRV KELF header fields and zone byte per region, the inflated player's version and
   library strings (`PsIIlibcdvd 2801`, …) and the card-update acceptance rule (region letter
   equal, version strictly higher, `1.00`/`1.01` blacklisted).
4. **MechaCon-version gate table** (§4) — given a `.mec` (real or default), list which
   XCDVDMAN features are on, and flag that PCSX2's default is "6.02.00, zone Oceania" so that
   every gate is open; with a real 5.xx value a 2.00 ROM would lose `sceCdReadRegionParams` and
   fall back to `ROMVER`'s region.
5. **Who-reads-what matrix** (§3.1) — the only NVM consumers on a retail boot are OSDSYS
   (config area 1, model name, region params), EECONF (areas 0/1 at every IOP boot) and the DVD
   player (area 2); the i.LINK/console IDs are never read by the ROM (only by HDDLOAD on
   1.50/1.60) — useful to answer "does the BIOS care about my console ID?" with "no".
6. **erom explorer** — inflate the 40 gzip members, list sizes/EE load addresses, dump the
   DVD player's string table and language list; mark the 41 opaque gaps.
7. **`rom1:`/`rom2:` device model** — ROMDRV's four units, ADDDRV's SSBUS programming, UDNL's
   three-image merge; lets the tool explain why `rom1:` exists only in the OSD and DVD boots.

---

## 7. Could not determine

* The encryption of the erom gaps and of the KELF bodies (Kbit/Kcon are MechaCon secrets;
  public kelftool-style decryption was not attempted), hence the `erom0:` directory, the exact
  location/size of `UDFIO` and the eight `DVDPL<L>` loaders, and whether anything else hides in
  the 282 KiB first gap.
* How the `DVDPL<L>` loader finds and inflates the gzip members (it is inside a KELF).
* The physical address of the erom on the SCPH-70004 (3 MiB cannot follow rom1 inside the
  `0x1E000000` window at `+0x40000`; the EROMDRV knows, it is encrypted) and whether the
  dumper's `ROM2` file is a bus mirror or a copy artefact (byte-identity to ROM1 is certain).
* UDNL's exact tie-break when the same module name exists in rom0, rom1 and an argv image.
* The real MechaCon version of any of the consoles (all `.mec` files are PCSX2 defaults), and
  the semantics of the MEC byte 0 on real hardware (PCSX2: zone index).
* The meaning of the devkit's region-parameter block (`"300" + binary`), of config area 0
  blocks (`0x270–0x29F`), of the 2-byte trailers after the layout-1 IDs (not a byte sum), of the
  area-2 blocks' individual bytes (DVD player settings, encrypted code), and of the MechaCon
  tables below `0x180`.
* The S-command result layout of the real `0x36` beyond bytes `[3..10]` (PCSX2 zeroes
  `[11..14]`) and whether a real MechaCon returns `[1]` = zone bits.
* EECONF's reason for zeroing area-1 block 0 (the condition is CDVD status register
  `0x1F402005` bit 1 set and bit 2 clear, read at IOP boot); and the consumer of EECONF's
  0x60-byte copy (no IOP module imports `eeconf`).
* The PS1 driver's own reading of `ps1drvConfig` (expected via `GetOsdConfigParam`; not
  traced in PS1DRV).
