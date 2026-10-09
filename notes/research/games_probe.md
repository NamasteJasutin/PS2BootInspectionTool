# What a game asks of the console, and what the console asks of a game — four Tekken discs

Game-side companion to `boot_sequence.md` / `osdsys_flow.md` / `ps2logo.md` / `ps1_boot.md`:
for two PS2 discs and two PS1 discs, every hook that fires between the ROM (2.00 E, SCPH-70004,
with 1.60 A / 1.00 J for comparison) and the game — SYSTEM.CNF, the boot executable, the IOP
modules the disc ships, the kernel/libcdvd calls the game makes, the ROM's per-title tables, and
the state handed over. Everything was read from the images and ROMs listed below; nothing was
run.

Subjects (all read in place, raw 2352-byte MODE2 sectors, user data at +24):

| disc | image | boot file | md5 of boot file |
|---|---|---|---|
| Tekken Tag Tournament (USA) v2.00 | `…/Tekken Tag Tournament (USA) (v2.00).bin` (CD, 309 585 blocks) | `\SLUS_200.01;1` | `db545eb4…` |
| Tekken Tag Tournament (Europe) v2.00 | `…(Europe) (En,Fr,De,Es,It) (v2.00).bin` (CD, 297 501 blocks) | `\SCES_500.01;1` | `01d656ce…` |
| Tekken (Europe) 1995 | `Tekken (Europe) (Track 01).bin` + 27 audio tracks | `\SCES_000.05;1` → `\TEKKEN.EXE;1` | `8c14afc6…` / `f012bf4f…` |
| Tekken (Japan) v1.1 | `Tekken (Japan) (Track 01).bin` + 26 audio tracks | **no SYSTEM.CNF** → `\PSX.EXE;1` → `\TEKKEN.EXE;1` | `bcf3cc33…` / `3174bac3…` |

ROM side: `rom0:PS1DRV` 2.00 E (`extracted/rom0/PS1DRV`, "PlayStation Driver Version 1.1.0",
Aug 19 2003), 1.60 A (`scph39001.bin`, v1.1.0, Jul 24 2000), 1.00 J (`scph10000.bin`, v1.0.0);
`rom0:PS1ID` = `1.40`, `PS1VERJ` = `1.02`, `PS1VERA/E/C/H` = `1.11` (2.00 E only; the other two
ROMs have no PS1ID/PS1VER files). PS2LOGO from `extracted/ps2logo_100000.bin`.

Tooling (scratchpad only, nothing added to `tools/`): an ISO9660 reader for raw images, a
syscall/RPC-id scanner for EE ELFs, a BIOS-call scanner for PS-X EXEs, Ghidra headless on
PS1DRV 2.00/1.00, the TTT USA ELF, the disc's `CDVDFSV` 1.65 and `RSPU2DRV`. Tags: **[V]**
read in code/data at the cited place, **[I]** inferred.

---

## 1. The PS2 discs (Tekken Tag Tournament USA / Europe)

### 1.1 File list and SYSTEM.CNF **[V]**

Both discs are CD-ROM XA images (every sector mode 2 form 1, PVD at LBA 16, `sysid
PLAYSTATION`, volume `TEKKENTAGTOURNAMENT`, publisher/preparer `NAMCO LIMITED`, created
2000-11-02 (US) / 2000-11-06 (EU)). Identical layout:

```
/SYSTEM.CNF;1          57 (US) / 56 (EU) bytes, LBA 307873 / 295781
/SLUS_200.01;1  or  /SCES_500.01;1    2 873 728 / 2 889 856 bytes
/TEKKEN.BIN;1          630 474 752 / 605 710 336 bytes at LBA 24 (all game data, one file)
/IRX/IOPRP165.IMG;1    98 901   /IRX/SIO2MAN.IRX;1 8 317   /IRX/MTAPMAN.IRX;1 9 909
/IRX/PADMAN.IRX;1 42 613   /IRX/MCMAN.IRX;1 82 021   /IRX/MCSERV.IRX;1 5 865   /IRX/RSPU2DRV.IRX;1 66 077
```

Every IRX and the IOPRP are byte-identical between the two discs except `RSPU2DRV.IRX`, which
differs only in an embedded date string (`2000, 11, 02(US)` vs `2000, 11, 6(EU)`). No `DNAS`,
no `.ELF` other than the boot file, no `host`/`host0` paths anywhere on the disc.

SYSTEM.CNF, CR+LF line ends, three keys:

```
US:  BOOT2 = cdrom0:\SLUS_200.01;1 / VER = 2.00 / VMODE = NTSC
EU:  BOOT2 = cdrom0:\SCES_500.01;1 / VER = 2.00 / VMODE = PAL
```

Who reads what on the 2.00 E console: OSDSYS reads `BOOT2` (checks the file name against the
MechaCon's disc ID, `osdsys_flow.md` §2.5) and `VER`; the strings `VMODE` does not occur in
OSDSYS, PS2LOGO, EELOAD or KERNEL — **`VMODE` is never consulted by this ROM** [V: `strings` of
the four images]. The video mode comes from the ELF itself (§1.6).

### 1.2 Boot ELF header **[V]**

Both: ELF32 LE MIPS, `e_entry 0x3572A0`, three `PT_LOAD`s with `p_align 0x1000`:

| segment | vaddr | filesz | memsz | flags | sections |
|---|---|---|---|---|---|
| 0 | `0x100000` | 0 | `0xF6A80` | RW | `.indata` (NOBITS) |
| 1 | `0x200000` | 0 | `0x140000` | RW | `.mfifo` (NOBITS) |
| 2 | `0x340000` | `0x2B6AE4` (EU `0x2BA9DC`) | `0x1C8DEF0` (EU `0x1C91DF0`) | RWX | `.text` `0xD567C`/`0xD67FC`, `.data`, `.vudata`, `.rodata` `0xF9828`, `.lit4`, `.sdata`, `.sbss`, `.bss` `0x19D6D70` (to `0x1FCDEF0`) |

45 section headers, no `.symtab` (stripped), 28 `.DVP.overlay…` sections (VU microcode overlay
tables, filler bytes only) and `.DVP.ovlytab/ovlystrtab` — the SCE VU assembler's overlay
records, a build leftover but no code. The crt0 at `0x3572A0` zeroes `0x5F6B00–0x1FCDEF0`
(`.sbss`+`.bss`) only, calls `InitMainThread`/`InitHeap` (syscalls 0x3C/0x3D), `FlushCache`,
`EI`, then `main(argc, argv)` from the kernel's argument block at `0x1FC63C0`, then `Exit` —
`main` (`0x3577E8`) ignores argc/argv. The two NOBITS segments and `.bss` above `0x1FCDEF0`
are not cleared by the program; they rely on the kernel having wiped RAM from `0x82000` in
`KLoadExec` (`boot_sequence.md` §2.2).

### 1.3 Every path / module / SDK string in the ELF **[V]** (`strings -t x`, offsets in the US file)

* Device paths: `cdrom:\IRX\RSPU2DRV.IRX;1`, `…MCSERV`, `…MCMAN`, `…PADMAN`, `…MTAPMAN`,
  `…SIO2MAN`, `cdrom:\IRX\IOPRP165.IMG;1` (`0x1BC880–0x1BC928`; note **`cdrom:` not
  `cdrom0:`**, the pre-1.6 device name, served by the IOPRP's own CDVDFSV/CDVDMAN). Memory card:
  `/BASLUS-20001TekkenTT` (EU `/BESCES-50001TekkenTT`) in the four usual spellings (`0x1BD108`).
  `rom0:UDNL ` (`0x2B4F80`, trailing space) — libkernl's `sceSifRebootIop` format string.
  **No `rom0:ROMVER`, no `rom1:`, no `host:`/`host0:`, no `mc0:` literal, no `cdrom0:`.**
* SDK library stamps (`0x1B59A0…`): `PsIIlibcdvd 1650`, `libdma 1620`, `libgraph1620`,
  `libmpeg 1620`, `libipu 1620`, `libkernl1620`, `libpkt 1620`, `librspu21620`, `libpad 1630`,
  `libmtap 1620`, `libmc 1650` — SDK 1.6.2–1.6.5 (the disc's IOPRP is 1.65, below).
* Debug/printf leftovers: the complete libcdvd trace set (`sceCdCbfunc= %d…`, `Libcdvd bind err
  S cmd`, `DiskReady 0`, `status called`, `Libcdvd call Clock read 1`, `sceCdStRead BLK Read…`),
  libdma `DSendM: not installed yet. sorry`, libgraph `sceGsSyncPath: DMA Ch.1 does not
  terminate` + register dumps, libmpeg decoder errors, newlib `bug in vfprintf: bad base`,
  libpad `Module version mismatch`, libmc `too old release of mcserv.irx / mcman.irx`, and the
  SDK TTY driver `TTY: packet size larger than expect / receive error / send err %d` — i.e. the
  game's `printf` goes through **DECI2** (syscall 0x7C, 10 call sites `0x404328…0x4044E0`),
  which the retail kernel special-cases into its DECI2 manager; with no host attached the
  output is simply lost (`hidden_features.md` §3). No `sceDbg*`, no assertion strings of the
  game's own, no source paths. Credits text is the only Namco prose.
* The one build-path leftover is on the IOPRP, not the ELF: its ROMDIR comment is
  `20000717-234040,conffile,ioprp165.img,xokano@rel-linux/~/tmp_165` (`tools/extinfo.py` on
  `IOPRP165.IMG`).

### 1.4 Start-up: what the game does to the IOP and the drive **[V]** (`0x357358`, called first thing from `main` → `0x357430`)

```
sceSifInitRpc(0)                                  0x402370
sceCdInit(SCECdINIT = 0)                          0x3F4D28  (binds S-cmd server 0x80000592, fno 0)
sceCdMmode(1)                                     0x3F5620  (S-cmd fno 0x22, 4-byte arg) — media mode = CD
do sceSifRebootIop("cdrom:\IRX\IOPRP165.IMG;1") while it fails      0x404190 → "rom0:UDNL %s"
do sceSifSyncIop() while 0                        0x404148
sceSifInitRpc(0); sceCdInit(0); sceCdMmode(1)     (again, on the rebooted IOP)
sceFsReset()                                      0x403350
for each of SIO2MAN, MTAPMAN, PADMAN, MCMAN, MCSERV, RSPU2DRV (pointer table 0x415700):
    while (sceSifLoadModule("cdrom:\IRX\<name>.IRX;1", 0, NULL) < 0) retry     0x403CC8
```

`sceCdMmode` = S-command `0x22` was confirmed on the disc's own CDVDFSV 1.65: its S-cmd RPC
dispatcher (`FUN_00004370`) case `0x22` prints `Media_Mode` and calls cdvdman import ordinal
**75** (`FUN_00003d6c`); case `0x23` is `Set Priority`. So the game explicitly declares *CD*
media (SCECdCD = 1) to the drive before every read. There is **no** `sceCdGetDiskType`,
`sceCdDiskReady`, `sceCdStatus`, `sceCdReadClock`, `sceCdRM/MV/RI`, `sceCdReadNVM/Config`,
`sceCdBootCertify`, `sceCdSearchFile` or `sceCdRead` call reachable from game code: every one
of those API functions is present in the linked libcdvd (S-cmd call sites `0x3F51C0 (0x1E)`,
`0x3F5278 (1)`, `0x3F5310 (2)`, `0x3F53C8 (3)`, `0x3F5460 (4)`, `0x3F5538 (5)`, `0x3F56E0
(0xF)`, `0x3F57D8 (0x1A)`; N-cmd `0x3F3E40 (2)`, `0x3F3F08 (3)`, `0x3F41E0 (4)`, `0x3F43B0 (5)`,
`0x3F44F8 (8)`, `0x3F4768 (9)`, `0x3F4850 (0xA)`, `0x3F4910 (0xB)`, `0x3F49D8 (0xC)`, `0x3F5BF8
(0xF)`) but has **no caller** outside libcdvd (Ghidra call graph, `functions.tsv`; only
`0x3F4D28` and `0x3F5620` are called, both from `0x357358`). The EE never reads the disc
itself: all game data is one file, `\TEKKEN.BIN;1`, opened on the **IOP** by Namco's
`RSPU2DRV` (`sceCdSearchFile` loop in `FUN_00000ed8`, then `sceCdRead`) and streamed through
its own RPC servers `0x80000901/902/903/9FE/9FF`.

### 1.5 Kernel syscalls the game actually invokes **[V]** (jal/j to the libkernl stubs at `0x3FFDC0–0x400650`; the stub for every number 0–0x7F is linked, so presence ≠ use)

Used: `SetGsCrt` (2, tail-called from `sceGsResetGraph` `0x3F6878`), `Exit` (4), Intc/Dmac
handler add/remove and enable/disable (0x10–0x1F incl. the interrupt-context variants),
thread set (0x20–0x25, 0x29, 0x2B, 0x2F, 0x30, 0x32, i-variants), `EndOfHeap`, semaphores
(0x40–0x45, iSignalSema), `FlushCache` (37 sites), `GetCop0` (−0x67), `sceSifStopDma` (0x6B),
`GsGetIMR/GsPutIMR`, `SetVSyncFlag` (0x73), `_print` (0x75, a kernel no-op), SIF 0x77–0x7A,
`Deci2Call` (0x7C).

**Never called**: `GetOsdConfigParam` (0x4B), `GetGsVParam` (0x4D), `SetOsdConfigParam`,
`PSMode` (0x7D), `MachineType` (0x7E), `GetMemorySize` (0x7F), `LoadExecPS2`, `ExecOSD`,
`ExecPS2`, `ResetEE`. No `rom0:ROMVER` open, no NVRAM read. **The game asks the console
nothing about itself** — not the region, not the language setting, not the memory size.

### 1.6 Region and video mode **[V]**

`sceGsResetGraph(0, INTERLACE=1, mode, …)` is called once, with the mode **hard-coded**:
`a2 = 2` (NTSC) at `0x39895C` in the US ELF, `a2 = 3` (PAL) at `0x3993CC` in the EU ELF. No
runtime region decision; the `VMODE` line of SYSTEM.CNF just restates it. US vs EU differences
beyond that are the title ID / save name strings, the EU localisation text (183 US-only / 122
EU-only strings, all UI text and credits) and a 0x1180-byte larger `.text`.

### 1.7 What the game *does* check: IOP module versions **[V]**

* libmc `sceMcInit` (`0x406560`): RPC `0x80000400` fno `0xFE` returns `{status, mcserv ver,
  mcman ver}`; **mcserv < 0x205 → "too old release of mcserv.irx"**, **mcman < 0x206 → "too
  old release of mcman.irx"**, either aborts memory-card support (return −0x78/−0x79).
* libpad `scePadInit` (`0x405090` → `0x406030`): PADMAN RPC `0x80000100` fno 1, command
  `0x12` (GetModVersion); **< 0x204 → "libpad: Module version mismatch"** and pad init fails.
* libmtap binds `0x80000701`; no version test.

The ROM's modules cannot pass these gates: rom0 `MCMAN`/`MCSERV` are version 0x101 and
`PADMAN` 0x114 (iopmod headers) in all three ROMs. That is why the disc ships its own (next
section) — and it is the one place where a wrong/old module set would make the game refuse
the console.

### 1.8 IOPRP165.IMG and the shipped IRXs vs the ROM **[V]** (iopmod headers; `tools/extinfo.py`)

| module | on disc (name, version) | rom0 2.00 E / 1.60 A / 1.00 J | source for the game |
|---|---|---|---|
| SIFCMD | `IOP_SIF_rpc_interface` 2.03 (IOPRP) | 1.01 | IOPRP |
| FILEIO | `FILEIO_service` 2.04 (IOPRP) | 1.01 | IOPRP |
| CDVDMAN | `cdvd_driver` 2.11 (IOPRP, dated 2000-07-17) | 1.04 | IOPRP |
| CDVDFSV | `cdvd_ee_driver` 2.11 (IOPRP) | 1.04 | IOPRP |
| SIO2MAN | `sio2man` 2.01 | 1.01 | disc IRX |
| MTAPMAN | `multitap_manager` 2.02 | — | disc IRX |
| PADMAN | `padman` 2.06 | 1.14 (0x114) | disc IRX |
| MCMAN | **`mcman_tool`** 2.11 (imports `secrman`) | `mcman` 1.01 | disc IRX |
| MCSERV | `mcserv` 2.08 | 1.01 | disc IRX |
| RSPU2DRV | `ZsRspu2Driver` 1.01 (SCE "rspu2 driver module version 1.2.0", libsnd2/libspu2 1600 inside) | — | disc IRX |
| sysmem, loadcore, intrman, ioman, sifman, thbase/thsemap/thevent, timrman, secrman, modload, vblank, stdio, sysclib, dmacman | — | ROM | rom0 via `UDNL` + `IOPBTCONF` |

The IOPRP only replaces the four SIF/file/CD modules (ROMDIR: RESET, ROMDIR, EXTINFO, SIFCMD,
FILEIO, CDVDMAN, CDVDFSV). Note the disc's MCMAN is the **"mcman_tool"** build (the devkit
name of mcman in SDK 1.6; it still imports the retail `secrman` for card authentication).

### 1.9 Anti-piracy / mod-chip / disc-swap checks **[V]**

None on the EE: no `sceCdDiskReady`/`sceCdStatus` loops, no timing checks, no `sceCdReadKey`,
no re-read of SYSTEM.CNF, no `GetDiskType` (see §1.4). The only disc-state logic is on the
IOP in `RSPU2DRV`: its streaming state machine (`FUN_00001ec0`) polls cdvdman ordinal 28
(`sceCdStatus` [I: ordinal name]) and, when the status equals 1 (`SCECdStatShellOpen` [I]),
drops into a "re-open the file / re-seek" state (`FUN_000030b8`, `FUN_00002fb0`) — a
tray-open recovery, not a check. The strings `TEKKEN TAG TOURNAMENT` / `NAMCO LIMITED` /
`2000, 11, 02(US)` inside RSPU2DRV (`0x9F70`) have **no code reference** (Ghidra
`data_xrefs.tsv`): an identification block, not a PVD comparison. The disc's `cdrom:` device
(CDVDFSV 2.11) serves `TEKKEN.BIN` by LBA after one `sceCdSearchFile`.

### 1.10 The PS2-logo sectors of these two discs (what PS2LOGO will see) **[V]**

Sectors 0–11 are scrambled as `ps2logo.md` §2 describes: every byte of the black border equals
one key byte, `0x0D` on the US disc and `0x8D` on the EU disc, and `rotl3(b ^ key)` yields a
readable "PlayStation 2" lettering bitmap (384×64 on US, 344-wide on EU). Both keys are what
PCSX2's serial-number formula predicts for `SLUS_200.01` (`(20001 & 0x1F) << 3 = 0x08 | …`) and
`SCES_500.01` (`(50001 & 0x1F) << 3 = 0x88 | …`) [I: formula from memory of PCSX2 `cdvdReadKey`].
Sectors 12–15 are zero.

**However the 2.00 E PS2LOGO checksum does not match either image**: the 0x1800-word sum of
the descrambled data is `0x8E77C557` (US) and `0x1E7F486A` (EU), while PS2LOGO wants
`0x62DB1E66` (J/A) or `0x78134705` (E) (`ps2logo_100000.bin` `0x101640–0x101674`,
re-verified). A brute force over every key byte × 8 rotations × both orders, and over
8/16/32-bit add and XOR folds of raw and descrambled data, gives no match. So either the
drive's descramble is not a per-byte XOR+rotate (`ps2logo.md` §10 already lists it as
unverified), or these two CD images would make a J/E console skip the logo animation (PS2LOGO
then exits without `LoadExecPS2` → kernel `Exit` → browser with an error, `ps2logo.md` §1.1).
A real PAL console does boot TTT EU with the logo, so the first explanation is the likely one
[I] — the key/rotate rule reproduces the picture but not the exact bytes. **Not resolved.**

---

## 2. The PS1 discs (Tekken Europe / Japan)

### 2.1 System area, SYSTEM.CNF, boot executables **[V]**

Sectors 0–15, licence text (LBA 4) and the TMD logo (LBA 5–11) are as `ps1_boot.md` §2
documents for exactly these two images (Japan: `…Inc.` + 31 lines of `0`; Europe:
`…(Europe)` + NUL; both logos byte-identical to the ROM copy); `ps2kit/src/disc.rs` parses
them. Not repeated here.

```
Europe SYSTEM.CNF (68 bytes, CR+LF, ends with ^Z 0x1A):
    BOOT = cdrom:SCES_000.05;1 / TCB = 4 / EVENT = 10 / STACK = 801FFF00
Japan: no SYSTEM.CNF at all → TBIN defaults {TCB 4, EVENT 0x10, STACK 0x801FFF00}, boot file cdrom:PSX.EXE;1
```

PS-X EXE headers (offset 0x10: pc0, gp0, t_addr, t_size; 0x30: sp; 0x4C: region text):

| file | pc0 | t_addr | t_size | sp | header text |
|---|---|---|---|---|---|
| EU `SCES_000.05` (507 904 B) | `0x80165398` | `0x80150000` | `0x7B800` | `0x801FFFF0` | `Sony Computer Entertainment Inc. for Europe area` |
| JP `PSX.EXE` (462 848 B) | `0x8015A760` | `0x80150000` | `0x70800` | garbage (`0x7E83E1EB`) | `…for Japan area` |
| EU `TEKKEN.EXE` (688 128 B) | `0x8007C15C` | `0x80010000` | `0xA7800` | garbage | **`…for Japan area`** (EU build re-used the JP header) |
| JP `TEKKEN.EXE` (716 800 B) | `0x8007C5D0` | `0x80010000` | `0xAE800` | garbage | `…for Japan area` |

The gp0/sp fields of three of the four headers contain stale tool-buffer bytes (`Entertai`,
x86 opcodes) — the 1995 `PS-X EXE` headers were not zeroed; only the EU boot file has a valid
sp. The boot file is the "GALAGA" loading-screen program (`Exit GALAGA.`, `Byebye from
GALAGA.`, `Create execute prosess.`); it plays `\MOVIE\LOGO.STR;1` and loads `\TEKKEN.EXE;1`
(`EXE_HEADER at $%08x`, `Load start [%s]`). The JP loader keeps its printf strings (`0x850–
0x9E4`); the EU loader has them stripped from the first 0x800 bytes but keeps the same libcd
tables (`$Id: intr.c,v 1.61 1995/05/18`).

### 2.2 What the executables ask of the console **[V]** (BIOS A0/B0/C0 calls found by scanning for `jr $t2` with `li $t2, 0xA0/B0/C0` and `li $t1, n`; `scratchpad/games/psxscan.py`)

Loader (EU/JP identical set): `A0:13 SaveState, 15 strcat (EU), 17 strcmp, 18 strncmp, 19
strcpy (EU), 2B memset, 2F rand, 30 srand, 39 InitHeap, 49 GPU_cw, 72 _96_remove, A0 WarmBoot
(EU only)`; `B0:07 DeliverEvent, 08–0D events, 12 InitPad, 13 StartPad, 14 StopPad, 15/16
OutdatedPad*, 17 ReturnFromException, 19 SetCustomExitFromException, 38 exit, 5B
ChangeClearPad`; `C0:0A ChangeClearRCnt`.
TEKKEN.EXE adds the memory-card set: `A0:70 _bu_init, AC _card_async_load_directory`,
`B0:32/34/35/36 FileOpen/Read/Write/Close, 41 FormatDevice, 42 firstfile, 4A InitCard, 4B
StartCard, 4E write_card_sector, 50 allow_new_card`; `A0:44 FlushCache` (EU only).

Not present in any of the four: `A0:B4 GetSystemInfo`, `A0:9D GetConf`, `B0:56/57
GetC0/B0Table`, any `SCEx`/`SCEE`/`SCEA`/`SCEI` string, any `_96_init` re-init after boot
(the loaders call `_96_remove` once, before handing over to TEKKEN.EXE). No read of the SubQ /
`CdlGetlocP` outside libcd's normal command table; no sector-timing loop. **No libcrypt or
anti-mod logic** — consistent with a 1995 title.

**BIOS ROM reads — present but dead** [V]: the two EU executables (not the JP ones) contain a
routine from SCE's kernel bootstrap source (`$Id: sys.c,v 1.129 1995/03/10 … suzu`, strings
`SYSTEM.CNF`, `PSDEMO`, `PSX.EXE`, `cdrom:`, `def = %s conf = %s serial = %08x %08x`) at
`0x801654C0` (loader) / `0x800766BC` (TEKKEN.EXE). It prints `def/conf` plus the two BIOS
header words `*(u32*)0xBFC00100` and `*(u32*)0xBFC00104` ("serial"), and if they equal
`0x19940728` and `0x2000` it `jalr`s **directly into ROM at `0xBFC0E228`**, otherwise calls
`A0:A0h WarmBoot`. Its two wrappers (`0x80165454`: `strcpy(0xA000DF00, "PSDEMO")` then
reboot(`PSX.EXE`,`SYSTEM.CNF`); `0x80165494`: reboot(`PSX.EXE`,`SYSTEM.CNF`)) have **no
callers and no data references** in either file — linked-in library code that never runs. On a
PS2, `0xBFC00100/104` are rom0's ROMGEN words (`hidden_features.md` §3), so even if it ran it
would take the WarmBoot path. The JP TEKKEN.EXE uses `bu00:` directly for saves; the EU one
embeds the save name `BESCES-00005<TEKKEN>`.

### 2.3 Japan vs Europe **[V]**

| | Europe | Japan |
|---|---|---|
| SYSTEM.CNF | yes (BOOT=SCES_000.05) | **none** |
| title ID OSDSYS derives (`0x203390`) and records in the play history | `SCES_000.05` | `???` (PSX.EXE fallback; `osdsys_flow.md` §2.5) |
| `VER` argument to PS1DRV | `???` (no VER line → `0x2AF398`) | `???` |
| ROM licence check (2.00 E, letter E) | passes (`…(Europe)`, 67 chars) | fails → black screen loop (`ps1_boot.md` §5.2) |
| executable region header | loader "Europe area", game "Japan area" | "Japan area" |
| dead `sys.c` BIOS-probe code | present | absent |

---

## 3. PS1DRV: the per-title table, the arguments, the OSD settings

### 3.1 Three builds **[V]** (ELF at `0x200000`, entry `0x200008`, `PsIIlibkernl130`)

| ROM | banner | built | `.text` | records | regions in table | title source | TITLE.DB | SYSTEM.CNF fallbacks |
|---|---|---|---|---|---|---|---|---|
| 1.00 J | `PlayStation Driver Version 1.0.0` | — | `0x187F4` | **175** at `0x218880` | SLPS 154, SLPM 13, SCPS 7, SIPS 1 | `strcpy(buf, argv[1])` (`0x209A54`) | `mc0:/BIDATA-SYSTEM/TITLE.DB`, `mc1:…` | none |
| 1.60 A | `…1.1.0` | Jul 24 2000 17:54:40 | `0x188B4` | **71** at `0x218900` | SLUS 56, SCUS 15 | `strcpy(buf, argv[1])` (`0x209B38`) | `mc0:/BADATA-SYSTEM/TITLE.DB`, `mc1:…` | none |
| 2.00 E | `…1.1.0` | Aug 19 2003 21:04:26 | `0x184E4` | **182** at `0x218500` | SLES 161, SCES 21 | **re-parses `cdrom0:\SYSTEM.CNF;1` itself, ignores argv** (`0x209BA4`: `$a0` is overwritten by the path before any use) | removed | `\PSXMYST\MYST.CCS;1` → `SLPS_000.24`; boot file `PSX.EXE` and `\CDROM\LASTPHOT\ALL_C.NBN;1` present → `SLPS_000.65`; unreadable → `???` |

So the table is **per ROM region**, not a union: the E ROM only knows European IDs, the A ROM
American, the J ROM Japanese. The two Tekken discs are in none of them (`SCES_000.05` absent
from 2.00 E; Tekken Japan has no ID at all). The 1.00 J table contains the typo
`SLPS_02.261`; 2.00 E has duplicates `SLES_025.15` (two different parameter sets, the first
match wins… no: the loop does not break, so the **last** match wins) and `SLES_023.43`;
twelve IDs are lower-case (`sles_006.60`…). The lookup is the EE-optimised `strcmp`
(`0x211598`, 128-bit compares) — **case-sensitive**, so a lower-case entry only matches a
disc whose `BOOT` line is lower-case.

### 3.2 Record layout and parameters **[V]**

36-byte records: `{char *id; char *p1 … *p8}` — every value is a *string* (e.g. `" 160"`,
`"-120"`, `"1024"`) converted with `atoi` at match time (`0x209E08`, loop bound `0xB6` = 182).
The eight numbers are, in the order of the driver's own trace line
`(poly:%d, sprt:%d, mecha:%x, null:%d, hl:%d, vblank:%d, pgpu:%d, speial:%d, linear:%d)`
(`0x21A6E0`, printed by `main` `0x200278`):

| # | name | global | where it goes [I unless noted] |
|---|---|---|---|
| 1 | `poly` | `0x700000A8` | GPU polygon-command path (`0x203C7C`) |
| 2 | `sprt` | `0x700000AC` | sprite path (`0x204918`) |
| 3 | `mecha` | `0x21CD90` | **written to CDVD register `0x1F402014`** (`0x20906C`, [V]): table value if ≥ 3, else `0xFE` when the OSD "disc speed" bit is set, else `2` |
| 4 | `null` | `0x7000193C` | main loop (`0x2054F0`) |
| 5 | `hl` | `0x70001940` | printed as `hl + 4`; used in `0x2064E8` |
| 6 | `vblank` | `0x7000001C` | printed as `vblank + 0x244`; timing constant in `0x209084` |
| 7 | `pgpu` | `0x70001930` | printed as `pgpu + 4`; main loop |
| 8 | `special` | `0x70001918` | `0x203C7C`, `0x207D94` |
| — | `linear` | `0x70001938 \| 0x7000193B` | not a column: the OSD texture-mapping bit (§3.4) |

Observed values: `mecha` ∈ {4, 7, 9, 11, 12, 14, 15, 19, 20, 22, 23, 28, 144, 160}; `null` ∈
{32, 1024, 4096}; `hl`/`pgpu` ∈ {−4, 8}; `vblank` ∈ {−180, −120, −60, 50, 120, 150, 170, 180,
240, 640}; `special` ∈ {1, 2}. Full grouped tables for all three ROMs are in the appendix.

### 3.3 Two override channels **[V]**

1. **`PSD1.0.0` in SYSTEM.CNF** (all three builds: `0x20A330` in 2.00 E, `0x209EA8` in
   1.00 J). After the table, PS1DRV re-reads `cdrom0:\SYSTEM.CNF;1` (≤ 0x800 bytes) and looks
   for a line `PSD1.0.0 = p1,p2,p3,p4,p5,p6,p7,p8` (key at line start, `=`, comma list, up to
   128 chars per field; parser `0x209F9C`); any fields present overwrite the table values. A
   disc can therefore carry its own PS1DRV parameters. Neither Tekken disc does.
2. **`TITLE.DB` on a memory card** (1.00 J and 1.60 A only, `0x209F18`): loads `rom0:SIO2MAN`,
   `rom0:SECRMAN`, `rom0:MCMAN`, opens `mc0:/B?DATA-SYSTEM/TITLE.DB` then `mc1:…`
   (`?` = I on the J ROM, A on the A ROM), reads the whole file to `0x20800000`, finds the
   line whose key is the title ID (same `key = v1,v2,…` grammar, parser `0x2098F4` /
   `0x209C0C`) and applies it. This is the field-updatable compatibility database Sony
   dropped in the 2003 build.

### 3.4 Console settings reaching the driver **[V]**

* `GetOsdConfigParam` (syscall 0x4B) word 0, **bits 5–12** (`>> 5`, one byte) = the OSD's
  `ps1drvConfig` byte, printed as `eeprom_data[0]:0x%02x` (`0x20A418`, `main`): `0x01` →
  disc speed *fast* (`mecha` default `0xFE`), `0x10` → texture mapping *smooth* (`0x70001937/
  38/3B` = 1), `0x11` → both; **any other value → both off**. These are the two items of the
  Version Information → PlayStation Driver → Options page (`menu_survey.md` §1.1). The 1.00 J
  build reads the same byte; the NVRAM itself is never read by PS1DRV (no CDVD RPC: the
  `sceCdInit`/`ReadNVM` slots in `main` are stubs `0x20A404/0x20A40C` returning 0).
* Bit 3 of the same word (`videoOutput`, RGB vs component) and **bit 1 of `GetGsVParam()`**
  (syscall 0x4D) go straight into GS `SMODE1` bits 25 (GCONT/RGBYC) and 36 (`0x2003A8`,
  `0x200AEC`) — the only other console state the driver consults.
* Nothing else: no `rom0:ROMVER`, no region, no `GetMemorySize`, no MechaCon query. The video
  mode is the PS1 game's own (`GS_TS4, NTSC, (NO-)INTERLACE` strings; PAL comes from the game
  programming the PS1 GPU, mapped by the PGIF handler).
* Besides `0x1F402014` the driver pokes the IOP/SPU2 compatibility registers from the EE
  (`0x209520`: `0x1F801014`, `0x1F801404/140C/1414`, `0x1F9007C0/7C6/7C8/019A`, SPU
  `0x1F000180…1B2`) and installs the PGIF handler (`SetPgifHandler`, `0x20B240`).

### 3.5 `rom0:PS1ID` / `PS1VER?` **[V]**

2.00 E only: `PS1ID` = `1.40`, `PS1VERJ` = `1.02`, `PS1VERA/C/E/H` = `1.11`. OSDSYS reads them
only for the Version Information page (`0x205F00`, `osdsys_flow.md` §2.2 module 5); PS1DRV
itself never opens them, and its banner says `1.1.0` regardless. The number the user sees
("PlayStation Driver 1.11") is therefore a ROM text file, not the driver's version.

---

## 4. PS2LOGO's hand-over state (verifying and extending `ps2logo.md`)

Re-read from `analysis/ps2logo/` (`main` `0x102040`, `LoadLogoFromDisc` `0x101540`,
`GsInit` `0x1000C0`, `Region` `0x101F60`):

* **Console checks**: `rom0:ROMVER` byte 4 only (`A`→1, `C`→3, `E`→2, `H`/`J`→0, anything else
  −1 → treated as NTSC and "not A"); `GetOsdConfigParam` bit 3 → `SetGsVParam(videoOutput)`
  (`0x1000C0`) — this is how the RGB/component choice reaches PS1DRV's `GetGsVParam` (§3.4)
  and the game. **No** `sceCdGetDiskType`, `sceCdBootCertify`, `sceCdRM/MV`, NVRAM, MechaCon
  region query, DVD-vs-CD test: the only libcdvd traffic is `sceCdInit(SCECdINoD)`,
  `sceCdDecSet(1,1,5)` (S-cmd `0x18` in libcdvd 1.6.3.1), `sceCdRead(0, 12, …)` (N-cmd),
  `sceCdSync(1)` (callers in `functions.tsv`: `0x109388`, `0x10B3A0`, `0x108500`, `0x109108`
  all from `0x101540` only). Media type and region were decided by OSDSYS before launch
  (`osdsys_flow.md` §4.1).
* **Disc checks**: the 12-sector logo read (a failed read → `ExecOSD(1,{"BootBrowser"})`) and
  the J/E checksum (§1.10). Nothing in SYSTEM.CNF is read by PS2LOGO.
* **Hand-over**: `LoadExecPS2(argv[1], argc − 2, argv + 2)`. OSDSYS launched
  `LoadExecPS2("rom0:PS2LOGO", 1, {bootpath})` and the kernel/EELOAD convention puts the program
  path in `argv[0]` (`boot_sequence.md` §2.3 `ExecLoaded`; PS1DRV relies on the same, §3.1),
  so PS2LOGO sees `argc = 2`, `argv = {"rom0:PS2LOGO", "cdrom0:\SLUS_200.01;1"}` and the game
  is started as `LoadExecPS2("cdrom0:\SLUS_200.01;1", 0, …)` → via EELOAD (IOP reset with
  `rom0:UDNL rom0:EELOADCNF`, `LoadElfAll`, `ExecPS2(entry, gp, 1, {path})`) → the game gets
  `argc = 1`, `argv[0] = "cdrom0:\SLUS_200.01;1"` [I: argc arithmetic; V: TTT ignores it].
  Memory: `KLoadExec` wipes RAM from `0x82000` before EELOAD runs (`boot_sequence.md` §2.2), so
  nothing of PS2LOGO's image (`0x100000–0x1E02CC`) survives into the game. GS: the last logo
  frame stays displayed (PS2LOGO holds it 120 fields, shuts the SPU down via OSDSND, never
  clears), the IOP is rebooted by EELOAD and again by the game (§1.4), so IOP-side state is
  gone twice over. `VER`/`VMODE` never reach the game.

---

## 5. Per-disc summary

| | TTT USA | TTT Europe | Tekken Europe | Tekken Japan |
|---|---|---|---|---|
| media / sectors | CD, 2352 M2F1 | CD, 2352 M2F1 | CD + 27 audio | CD + 26 audio |
| SYSTEM.CNF | BOOT2, VER 2.00, VMODE NTSC | BOOT2, VER 2.00, VMODE PAL | BOOT, TCB, EVENT, STACK | **none** |
| title ID (OSDSYS/history) | SLUS_200.01 | SCES_500.01 | SCES_000.05 | `???` |
| what the ROM checks before launch | MechaCon disc key → ID = BOOT2 name; PS2LOGO: ROMVER region, logo sum (**fails on image**) | same | licence line (E), logo TMD (ok) | licence `Inc.` on an E console → **fails** |
| game → console queries | none (no syscalls 0x4B/0x4D/0x7D–0x7F, no ROMVER/NVRAM) | none | none (dead `0xBFC00100` probe only) | none |
| game → drive | `sceCdMmode(CD)`, IOP reboot with IOPRP165, 6 IRX loads; data via IOP `RSPU2DRV` | same | libcd reads, `_96_remove` | same |
| version gates | mcserv ≥ 2.05, mcman ≥ 2.06, padman ≥ 2.04 (disc ships 2.08/2.11/2.06) | same | — | — |
| video | hard-coded NTSC | hard-coded PAL | PS1 game's own | PS1 game's own |
| anti-piracy / mod-chip | none found | none found | none | none |
| PS1DRV table / overrides | — | — | not in 2.00 E table; no PSD1.0.0 | no ID; no PSD1.0.0 |
| debug leftovers | SDK trace strings, DECI2 printf, `.DVP.overlay`, IOPRP build path `xokano@rel-linux` | same | stripped loader strings; dead sys.c probe | full loader printf set |

---

## 6. Tool-worthy findings (ranked)

1. **ELF probe report** — for a boot ELF: segments/entry, SDK library stamps, the device
   paths/IRX list, which kernel syscall stubs are *called* (not merely linked), which libcdvd
   S/N-command numbers are reachable from game code (string-anchored wrappers at `Scmd fail
   sema` / `Ncmd fail sema`), hard-coded `sceGsResetGraph` mode, and the RPC server ids bound.
   Everything in §1.3–§1.6 came from two small scanners (`elfscan.py`, `callers.py`,
   `cdcalls.py` in the scratchpad) that need no Ghidra; S-cmd numbers can be named by
   decoding the disc's own CDVDFSV dispatcher (§1.4) or a per-SDK table.
2. **IOP module inventory** — ROMDIR/EXTINFO of `IOPRP*.IMG` plus the iopmod name/version of
   every `.IRX` on the disc, against the user's rom0, with the libmc/libpad minimum-version
   gates extracted from the ELF (§1.7–§1.8). Shows exactly which modules the game trusts the
   ROM for and whether the ROM alone would fail.
3. **PS1DRV table + override view** — parse the user's `rom0:PS1DRV` (36-byte pointer records,
   region-specific), show whether the inserted PS1 disc's `BOOT` ID matches (case-sensitive),
   decode the eight parameters and the `mecha` → `0x1F402014` / OSD-byte logic, and check the
   disc's SYSTEM.CNF for a `PSD1.0.0` line and (1.00/1.60 ROMs) a card `TITLE.DB`. Also show
   that 2.00 E ignores the OSDSYS-supplied ID and what `???` titles get.
4. **Disc system-area report** — PS2: key byte, descrambled logo preview, the 0x1800-word sum
   vs the two PS2LOGO constants (flag mismatches as in §1.10); PS1: licence line length/variant
   vs the ROM letter, TMD compare (already in ps2kit), presence/absence of SYSTEM.CNF and the
   resulting history ID.
5. **SYSTEM.CNF key audit** — list every key and which ROM component reads it (`BOOT2`/`VER`:
   OSDSYS; `VMODE`: nobody; `BOOT`/`TCB`/`EVENT`/`STACK`: TBIN; `PSD1.0.0`: PS1DRV).
6. **PS-X EXE probe** — header sanity (stale gp/sp bytes, region text), BIOS A0/B0/C0 call set,
   `0xBFC0xxxx` references and whether they are reachable (§2.2), `SCEx`/libcrypt markers.
7. **Hand-over trace** — argv chain OSDSYS → PS2LOGO/PS1DRV → game, with the `???` fallbacks
   and the kernel's RAM wipe, as a one-screen diagram per disc.

---

## 7. Could not determine

* The exact drive-side descramble of sectors 0–11: the per-byte XOR/rotate reproduces the
  picture but not PS2LOGO's checksum on either TTT image (§1.10). Either the rule or the
  images are off; needs a known-good image or a hardware read.
* The semantics of PS1DRV's `poly/sprt/null/hl/vblank/pgpu/special` beyond where they are
  consumed (§3.2); and what CDVD register `0x1F402014` does with `2`/`0xFE`/table values
  (named "mecha" by the driver; PCSX2 labels the register "PS1 mode?") — MechaCon-side.
* Whether the ROM's PS1 TBIN checks the `…for Japan area` header text (it is not read by LOGO;
  TBIN's LoadExec was not re-examined here).
* Why Tekken Europe's `TEKKEN.EXE` carries the dead `sys.c` 1.129 boot/probe routine — it
  looks like an SCEE-era `libapi`/`libsn` object linked but unreferenced; its ROM entry
  `0xBFC0E228` with header `{0x19940728, 0x2000}` presumably targets a 1994 devkit BIOS [I].
* `sceCdMmode`'s effect on a CD-only drive path (the game asks for CD mode twice; CDVDMAN 2.11
  honours it in `sceCdSpinCtrlIOP speed= %d` [I from strings]).
* The cdvdman ordinal names 28 (`sceCdStatus`) and 13 (`sceCdDiskReady`) used by RSPU2DRV are
  from the SDK export order, not verified against a symbol table.

---

## Appendix: PS1DRV title tables (grouped by parameter set `poly,sprt,mecha,null,hl,vblank,pgpu,special`) **[V]**

### 2.00 E (SCPH-70004): 182 records, 34 distinct sets, table at `0x218500` (file `0x19500`)

| parameters | n | title IDs |
|---|---|---|
| `0,0,160,0,0,0,0,0` | 71 | SCES_007.98 SLES_000.21 SLES_000.71 SLES_000.74 SLES_000.94 SLES_001.04 SLES_001.05 SLES_002.53 SLES_002.54 SLES_006.59 sles_006.61 SLES_007.59 SLES_009.10 SLES_010.03 SLES_010.28 SLES_010.29 SLES_017.49 SLES_025.74 SLES_100.71 SCES_007.99 SCES_008.00 SCES_008.01 SCES_008.02 SLES_100.74 SLES_200.74 SLES_300.74 SLES_101.04 SLES_201.04 SLES_301.04 SLES_101.05 SLES_201.05 SLES_301.05 SLES_002.52 SLES_106.59 SLES_206.59 SLES_306.59 sles_006.60 sles_106.60 sles_206.60 sles_306.60 sles_106.61 sles_206.61 sles_306.61 SLES_015.76 SLES_022.33 SLES_022.35 SLES_022.37 SLES_022.54 SLES_024.73 SLES_024.74 SLES_024.75 SLES_020.61 SLES_023.96 SLES_005.93 SLES_000.70 SLES_100.70 SLES_200.70 SCES_002.19 SLES_000.20 SLES_000.29 SLES_008.89 SLES_013.45 SLES_017.66 SLES_017.67 SLES_018.94 SLES_020.15 SLES_013.01 SLES_020.24 SLES_020.25 SLES_020.26 SLES_020.27 |
| `0,0,14,0,0,0,0,0` | 23 | SLES_009.72 SLES_025.29 SLES_009.69 SLES_009.70 SLES_009.71 SLES_109.72 SLES_009.73 SLES_109.73 SLES_009.74 SLES_109.74 SLES_009.75 SLES_109.75 SLES_009.76 SLES_109.76 SLES_009.77 SLES_109.77 SLES_012.27 SLES_112.27 SLES_025.30 SLES_025.31 SLES_025.32 SLES_025.33 SLES_026.98 |
| `0,0,144,0,0,0,0,0` | 10 | SLES_001.65 SLES_101.65 SLES_001.66 SLES_101.66 SLES_001.67 SLES_101.67 SLES_001.91 SLES_101.91 SLES_001.92 SLES_101.92 |
| `0,1,0,0,0,0,0,0` | 9 | SLES_109.98 SCES_019.79 SLES_009.98 SCES_022.22 SLES_007.93 SLES_007.94 SLES_007.95 SCES_001.63 SLES_001.52 |
| `0,0,23,0,0,0,0,0` | 9 | SLES_000.56 SLES_021.98 SLES_039.34 SLES_009.78 SCES_000.06 SCES_015.65 SCES_115.65 SCES_215.65 SCES_315.65 |
| `0,0,15,0,0,0,0,0` | 5 | sces_014.30 SLES_014.72 SLES_014.74 sles_017.59 SLES_014.73 |
| `6,0,0,0,0,0,0,0` | 5 | SLES_019.77 SLES_019.78 SLES_018.16 SLES_019.75 SLES_019.76 |
| `0,0,4,0,0,0,0,0` | 5 | SCES_000.02 SCES_009.84 SLES_015.49 SLES_020.63 SLES_020.64 |
| `13,9,0,0,0,0,0,0` | 5 | SLES_009.14 SLES_009.15 SLES_009.16 SLES_009.17 SLES_009.18 |
| `0,0,22,0,0,0,0,0` | 4 | SLES_000.33 SLES_012.98 SLES_009.99 SLES_009.53 |
| `12,0,0,0,0,0,0,0` | 3 | SLES_005.01 SLES_023.43 SLES_023.43 |
| `0,0,7,0,0,0,0,0` | 3 | SLES_014.33 SLES_027.64 SLES_027.65 |
| `0,0,0,1024,0,0,0,0` | 3 | SLES_008.58 SLES_005.97 SLES_018.75 |
| `0,0,28,0,0,0,0,0` | 3 | SLES_027.40 SLES_033.71 SLES_023.40 |
| `0,2,0,0,0,0,0,0` | 2 | SLES_000.99 SLES_001.40 |
| `0,0,4,1024,0,0,0,0` | 2 | SCES_017.62 SLES_013.55 |
| `0,0,11,0,0,0,0,0` | 2 | SCES_023.80 SCES_123.80 |
| `18,2,0,0,0,0,0,2` | 2 | SLES_009.27 SLES_014.28 |
| `0,0,0,0,0,640,0,0` | 1 | SCES_012.59 |
| `0,0,9,0,0,0,0,0` | 1 | SLES_005.78 |
| `0,0,12,0,0,0,0,0` | 1 | SLES_010.40 |
| `5,0,0,0,0,0,0,0` | 1 | SLES_012.26 |
| `0,0,20,0,0,0,0,0` | 1 | SLES_014.60 |
| `18,0,0,0,0,0,0,0` | 1 | SLES_002.96 |
| `3,1,4,0,0,150,0,0` | 1 | SLES_025.15 (first entry) |
| `7,0,0,0,0,0,0,2` | 1 | SLES_010.80 |
| `1,0,0,0,0,0,0,0` | 1 | SLES_013.17 |
| `0,0,0,1024,-4,-120,-4,0` | 1 | SLES_007.55 |
| `2,1,4,0,0,180,0,0` | 1 | SLES_025.15 (second entry — wins) |
| `0,1,160,0,0,0,0,0` | 1 | SLES_004.37 |
| `0,2,160,0,0,0,0,0` | 1 | SLES_009.08 |
| `1,1,0,0,0,0,0,0` | 1 | SLES_013.93 |
| `0,0,0,4096,-4,-180,-4,0` | 1 | SLES_001.22 |
| `0,0,19,0,0,0,0,0` | 1 | SLES_002.09 |

### 1.60 A (SCPH-39001): 71 records, 27 distinct sets, table at `0x218900` (file `0x19900`)

| parameters | n | title IDs |
|---|---|---|
| `0,0,160,0,0,0,0,0` | 25 | SCUS_945.56 SLUS_000.19 SLUS_001.20 SLUS_002.70 SLUS_004.92 SLUS_005.24 SLUS_005.30 SLUS_010.08 SLUS_010.68 SLUS_011.03 SLUS_001.34 SLUS_001.35 SLUS_001.36 SLUS_001.82 SLUS_006.35 SLUS_002.71 SLUS_002.72 SLUS_002.73 SLUS_009.34 SLUS_001.65 SLUS_001.66 SLUS_001.67 SCUS_941.73 SCUS_945.05 SLUS_000.35 |
| `0,0,14,0,0,0,0,0` | 7 | SLUS_004.21 SLUS_007.47 SLUS_007.48 SLUS_009.23 SLUS_005.92 SLUS_007.47 SLUS_007.56 |
| `0,1,0,0,0,0,0,0` | 4 | SLUS_005.84 SLUS_005.60 SLUS_004.23 SCUS_946.05 |
| `0,2,0,0,0,0,0,0` | 3 | SCUS_945.09 SLUS_005.22 SLUS_001.26 |
| `0,0,0,1024,-4,-120,-4,0` | 3 | SLUS_001.84 SCUS_944.16 SLUS_006.30 |
| `0,0,11,0,0,0,0,0` | 2 | SCUS_944.55 SCUS_944.88 |
| `0,0,9,0,0,0,0,0` | 2 | SCUS_946.08 SLUS_001.64 |
| `0,0,4,0,0,0,0,0` | 2 | SLUS_004.40 SCUS_941.94 |
| `0,0,0,0,0,-60,0,0` | 2 | slus_007.78 SLUS_007.89 |
| `0,0,15,0,0,0,0,0` | 2 | SLUS_007.87 slus_008.18 |
| `6,0,0,0,0,0,0,0` | 2 | SLUS_008.42 SLUS_008.28 |
| `0,0,144,0,0,0,0,0` | 2 | SCUS_947.00 SCUS_947.01 |
| `0,0,0,0,0,0,0,1` | 1 | SCUS_942.21 |
| `2,0,0,0,0,0,0,0` | 1 | SCUS_946.02 |
| `0,0,5,0,0,0,0,0` | 1 | SLUS_002.40 |
| `3,1,4,0,0,150,0,0` | 1 | SLUS_006.41 |
| `0,0,4,1024,0,0,0,0` | 1 | SLUS_007.53 |
| `4,1,0,0,0,0,0,0` | 1 | SLUS_007.96 |
| `0,0,0,0,0,120,0,0` | 1 | SLUS_008.62 |
| `1,0,0,0,0,0,0,0` | 1 | SLUS_010.69 |
| `0,0,0,32,0,0,0,0` | 1 | SLUS_000.99 |
| `0,9,160,0,0,0,0,0` | 1 | SLUS_008.70 |
| `0,0,0,1024,0,0,0,0` | 1 | SLUS_002.56 |
| `0,0,0,0,0,170,0,0` | 1 | SCUS_945.67 |
| `18,0,0,0,0,0,0,0` | 1 | SLUS_003.41 |
| `0,0,160,1024,0,0,0,0` | 1 | SLUS_004.93 |
| `0,0,0,0,0,240,0,0` | 1 | SLUS_006.06 |

### 1.00 J (SCPH-10000): 175 records, 48 distinct sets, table at `0x218880` (file `0x19880`)

| parameters | n | title IDs |
|---|---|---|
| `0,1,0,0,0,0,0,0` | 43 | SLPS_018.13 SLPS_018.14 SLPS_018.15 SLPS_018.72 SLPS_018.73 SLPS_018.74 SLPS_019.54 SLPS_019.55 SLPS_019.56 SLPS_020.16 SLPS_020.17 SLPS_020.18 SLPS_020.19 SLPM_862.41 SLPM_862.42 SLPM_862.43 SLPM_862.44 SLPS_011.24 SLPS_009.30 SLPS_011.58 SLPS_009.76 SLPS_009.77 SLPS_005.24 SLPS_020.60 SLPS_020.61 SLPS_020.62 SLPS_020.63 SLPM_860.06 SLPS_006.18 SLPM_861.63 SLPS_023.49 SLPS_015.75 SLPS_008.01 SLPS_007.67 SLPS_007.14 SLPS_017.67 SLPS_024.25 SLPS_009.29 SLPS_009.86 SLPS_016.26 slps_002.13 SLPS_004.09 SLPS_003.95 |
| `0,0,0,1024,0,0,0,0` | 20 | SLPS_010.44 SLPM_860.22 SLPS_022.02 SLPS_011.19 SLPS_006.58 SLPS_012.83 SLPS_011.79 SLPS_021.94 SLPS_021.93 SLPS_021.92 SLPS_018.17 SLPM_861.85 SLPM_861.86 SLPM_861.87 SLPS_013.22 SLPS_009.57 SLPS_025.06 SLPS_025.17 SLPS_023.08 SLPS_015.59 |
| `0,0,7,0,0,0,0,0` | 13 | SLPS_012.81 SLPS_010.96 SLPS_013.71 SLPS_015.90 SLPS_015.89 SLPS_013.79 SLPS_012.19 SLPS_016.78 SLPS_015.88 SLPS_017.46 SLPS_012.05 SLPS_012.35 SLPS_021.00 |
| `0,2,0,0,0,0,0,0` | 9 | SLPS_004.33 SLPS_010.33 SLPS_014.99 SLPS_014.98 SLPS_014.97 SCPS_101.14 SCPS_101.13 SCPS_101.12 SLPS_001.77 |
| `0,0,9,0,0,0,0,0` | 8 | SLPS_007.26 SLPS_008.88 SLPS_023.01 SLPS_018.94 SCPS_100.28 SLPS_007.25 SLPS_024.51 SLPS_005.16 |
| `0,0,14,0,0,0,0,0` | 7 | SLPS_015.10 SLPS_015.11 SLPS_012.22 SLPS_012.23 SLPS_015.12 SLPS_015.13 SLPS_023.00 |
| `0,0,6,0,0,0,0,0` | 6 | SLPM_861.40 SLPS_010.18 SLPS_014.20 SLPS_015.66 SLPS_003.43 SLPS_003.44 |
| `4,0,0,0,0,0,0,0` | 5 | SLPS_016.83 SLPS_015.71 SLPS_024.41 SCPS_100.86 SLPS_009.83 |
| `0,0,4,0,0,0,0,0` | 5 | SLPS_004.63 SLPS_013.82 sips_600.10 SLPS_009.70 SLPS_002.51 |
| `2,0,0,0,0,0,0,0` | 3 | SLPS_000.24 SLPS_018.49 SLPS_006.52 |
| `6,0,0,0,0,0,0,0` | 3 | SLPS_006.53 SLPS_02.261 SLPS_026.13 |
| `18,0,0,0,0,0,0,0` | 3 | SLPS_007.76 SLPS_007.75 SLPS_006.10 |
| `0,1,0,1024,0,0,0,0` | 2 | SLPS_017.10 SLPS_014.63 |
| `0,15,0,0,0,0,0,0` | 2 | SLPS_008.15 SLPS_008.16 |
| `8,0,0,0,0,0,0,0` | 2 | SLPS_021.15 SLPS_007.21 |
| `10,0,0,0,0,0,0,0` | 2 | SLPS_002.47 SLPS_002.76 |
| `1,0,0,0,0,0,0,0` | 2 | SLPS_004.17 SLPS_015.00 |
| `0,0,3,0,0,0,0,0` | 2 | SLPS_014.95 SLPS_014.96 |
| `0,0,5,0,0,0,0,0` | 2 | SLPS_023.15 SLPS_005.55 |
| `0,0,11,0,0,0,0,0` | 2 | SCPS_101.17 SCPS_101.16 |
| `0,0,15,0,0,0,0,0` | 2 | SLPS_016.22 SLPS_013.54 |
| `0,0,0,0,8,0,0,0` | 2 | SLPS_016.92 SLPS_006.83 |
| `0,0,0,0,8,0,8,0` | 2 | SLPS_023.80 SLPS_023.81 |
| `30,0,0,0,0,0,8,2` | 2 | SLPS_007.77 SLPS_022.99 |
| `0,0,0,0,-4,0,-4,0` | 2 | SLPS_011.50 SLPM_861.42 |
| `0,2,0,1024,0,0,0,0` | 2 | SLPS_008.63 SLPS_000.65 |
| `5,0,0,0,0,0,0,0` | 1 | SLPS_011.31 |
| `0,3,10,1024,0,0,0,0` | 1 | SLPS_013.24 |
| `0,0,0,0,0,0,0,1` | 1 | SLPS_007.70 |
| `3,0,4,0,0,50,0,0` | 1 | SLPS_014.34 |
| `12,0,0,0,0,0,0,0` | 1 | SLPS_011.70 |
| `2,1,0,0,0,0,0,0` | 1 | SLPS_005.20 |
| `0,4,0,0,0,0,0,0` | 1 | SLPS_023.32 |
| `0,0,4,1024,0,0,0,0` | 1 | SLPS_012.36 |
| `0,0,13,0,0,0,0,0` | 1 | SLPS_010.19 |
| `0,0,8,0,0,0,0,0` | 1 | SLPM_860.03 |
| `9,0,0,0,0,0,0,0` | 1 | SLPS_012.53 |
| `0,0,0,1024,8,0,-4,0` | 1 | SLPS_007.96 |
| `0,0,12,0,0,0,0,0` | 1 | SLPS_007.04 |
| `0,0,0,0,0,-60,0,0` | 1 | SLPS_014.27 |
| `0,0,0,1024,0,50,0,0` | 1 | SLPS_021.43 |
| `0,1,9,0,0,0,0,0` | 1 | SLPS_023.71 |
| `0,5,0,0,0,0,0,0` | 1 | SLPS_003.09 |
| `30,2,0,0,0,0,0,2` | 1 | SLPS_017.56 |
| `0,0,0,1024,0,0,-4,0` | 1 | SLPS_021.27 |
| `6,3,0,0,0,0,0,0` | 1 | SLPS_001.10 |
| `21,0,0,0,0,0,0,2` | 1 | SLPS_007.17 |
| `0,0,0,0,0,-120,0,0` | 1 | SLPS_019.26 |
