# Pre-OSD hooks: RESET, RDRAM, KERNEL, EELOAD, the IOP boot and the gate modules

Scope: everything that runs *before* `rom0:OSDSYS` gets control, plus the IOP modules that gate
what OSDSYS (or a game) can do afterwards. Five ROMs, oldest first:

| tag | ROM | ROMVER | KERNEL build | notes |
|---|---|---|---|---|
| **1.00J** | SCPH-10000 | `0100JC20000117` | Dec 10 1999 | no `X*` modules, no `EELOADCNF`/`OSDCNF`, no `TESTMODE` |
| **dev** | DTL-H30101 | `0150AD20001228` | May 11 2000 | TEST kit; `secrman_for_dex`; **EELOAD byte-identical to 1.60E** |
| **1.60E** | SCPH-30004R | `0160EC20011004` | Jul 4 2001 | pre-`BootIllegal`, pre-`sce_dev5` |
| **1.60A** | SCPH-39001 | `0160AC20020207` | Feb 7 2002 | first with `BootIllegal`, `sce_dev5`, kernel ROMVER copy |
| **2.00** | SCPH-70004 | `0200EC20040614` | Feb 6 2003 | the ROM the other notes describe |

Conventions as in the other notes: **[V]** read from code/data (address and ROM given), **[I]**
inferred. Prefixes: `rom:` = ROM (`0xBFC00000`), `k:` = KERNEL (`0x80000000`), `el:` = EELOAD
(`0x82000`), `iop:<MODULE>+off` = offset inside the IRX's `.text` (Ghidra's ELF import puts
`.text` at 0, so `FUN_0000xxxx` in the exports = `.text+0xxxxx`). Unless a ROM is named, an IOP
address refers to the 2.00 copy of the module; §0.2 says which copies are byte-identical, so the
same offsets hold there.

Method: `tools/romdir.py` splits (scratchpad `kernel/r100 rdev r160e r160a r200`), `md5`/`cmp`
for identity, `tools/syscall_table.py`, `tools/r5900dis.py`/`r3000dis.py` (venv), and Ghidra
12.1 headless through a scratchpad copy of `analysis/hidden/run_ghidra*.sh` (`kernel/gh.sh`):
raw-EE imports of EELOAD 1.00J/1.60A/dev and KERNEL 1.00J/dev/1.60E/1.60A, IRX imports of
XCDVDMAN (2.00, 1.60E), XLOADFILE, LOADFILE, XMCMAN, XDEV9, ROMDRV, ADDDRV, XRMMAN2, XSIO2MAN,
XPADMAN, SECRMAN (cex), NCDVDMAN, CDVDMAN, UDNL, MODLOAD, REBOOT, EECONF, and a raw-R3000 import
of IOPBOOT at `0xBFC4A000`. The 2.00 EE exports in `analysis/{reset,kernel,eeload}` and the
earlier IRX exports in `analysis/hidden/` were reused. Exports are in the scratchpad (`kernel/gh/<name>/c`);
the Ghidra projects were deleted.

Builds on `notes/boot_sequence.md`, `notes/devkit_survey.md`, `notes/hidden_features.md`; it
repeats nothing from them except where a claim is corrected (marked **correction**).

---

## 0. Identity matrix: what is actually different across the five ROMs

### 0.1 EE side **[V: md5/cmp of the split modules]**

| module | 1.00J | dev | 1.60E | 1.60A | 2.00 |
|---|---|---|---|---|---|
| RESET | gen 1 (IOP tables differ) | gen 2 | = dev + date | gen 3 (EE part re-linked, `0xB000F500 ← -1`, kernel entered through kseg0) | = 1.60A + date |
| RDRAM | 1.00 routine | 1.50 routine | 1.60 | = 1.60E | 2.00 |
| KERNEL | 100 syscalls (no 0x6E/0x6F) | 101 | 101 | 101 + ROMVER copy | 101 |
| EELOAD | **own build** (no IOP reset, no TESTMODE, SIO prints) | **= 1.60E byte for byte** | ← | + `BootIllegal` | + `BootIllegal` |

### 0.2 IOP side **[V]**

Byte-identical in **all five** ROMs: `IOPBOOT`, `REBOOT`, `LOADFILE`, `CDVDMAN`, `MCMAN`,
`SIO2MAN`, `PADMAN`, `LOADCORE`, `SYSMEM`, `MODLOAD`, `IOMAN`, `EESYNC`, `SIFMAN`, `SIFCMD`,
`FILEIO`, `CDVDFSV`, `TBIN`. Identical in the four that have them (dev, 1.60E, 1.60A, 2.00):
`RMRESET`, `XSIO2MAN`, `XPADMAN`, `XSIFCMD`, `XFILEIO`, `ADDDRV`, `UDNL` (1.00J's UDNL differs
by 16 bytes), `XMCMAN` (2.00 differs), `XCDVDFSV` (2.00 differs).

| module | 1.00J | dev | 1.60E | 1.60A | 2.00 |
|---|---|---|---|---|---|
| SECRMAN | **cex** (`b8bac9`) | **dex** | cex = 1.00J | cex = 1.00J | cex = 1.00J |
| XCDVDMAN | — | v.a (2 MechaCon flags, debug prints) | v.b (2 flags, no prints, **no `sce_dev5`**) | v.c (+`sce_dev5`) | v.d (10 flags, +`sce_dev5`) |
| NCDVDMAN | — | — | — | — | only here (used by `EELOADCNF`) |
| XLOADFILE | — | v.a | = dev | +`cdrom0:sce_dev5` disc check | = 1.60A logic |
| XDEV9/XDEV9SERV, XRMMAN2 | — | — | — | — | only here |
| EECONF | `eeconfig` | = 1.00J | `eeconfig8` | = 1.60E | `eeconfig8` (differs) |
| ROMDRV | 1.00 | 1.50+ | ← | ← | ← |

So the retail SECRMAN is the *same file* from 1.00J (Jan 2000) to 2.00 (Jun 2004); the only
DEX module in the devkit is its SECRMAN. The devkit's EELOAD is literally the 1.60E one, which
settles the survey's open question about `0xF630 vs 0xF210`: the devkit image (assembled
2003-05-20) carries the 2001-10 EELOAD, not a 1.50 one.

---

## 1. Hardware / environment probes before any program runs

### 1.1 RESET (`rom:BFC00000`) **[V]**

* `rom:_reset_vector`: `mfc0 PRId`; `< 0x59` → IOP path `BFC02000`, else EE path `BFC00800`
  (all five). The IOP path chooses one of two SSBUS timing tables by `PRId ≥ 0x10 &&
  (0xBF801450 & 8) == 0` (2.00: `BFC02008`/`BFC0200C` are the two table pointers and
  `BFC02010`/`BFC02014` the matching second table — the same test MODLOAD's `ReBootStart` repeats,
  §5.2). The RAM-size byte and the `0xBF801060` value come from a table at `rom:BFC024A0`
  (2.00: `lw t1,4(t0)` → `RAM_SIZE`, `lb s0,2(t0)` → MB count, `BFC023A4`–`BFC023B0`).
* Two ROMDIR lookups, in this order: `TBIN` (name at `BFC02488`, jump at `BFC023E4` with
  `a0 = MB`) — only when `PRId ≥ 0x10` and `0xBF801450` bit 3 is set (boot_sequence §2.1) — then
  `IOPBOOT` (name at `BFC02478`, jump at `BFC0244C`) with **`a0 = RAM MB, a1 = 0, a2 = 0, a3 = 0`**
  (`BFC0243C`–`BFC02450`). `a1`/`a2` matter: §1.5.
* RESET generations (§0.1): 1.00J → 1.50/1.60E differ only in the IOP half (`0x2000`+, the
  SSBUS tables, 390 bytes); 1.60E → 1.60A re-links the EE half (`0x82C`–`0x1CAD`): the kernel
  loader is now entered through its kseg0 alias (`lui v1,0xBFC0; addiu 0xC00; and 0x9FFFFFFF;
  jalr`), `0xB000F500` gets `-1` instead of `1`, and `k0/k1/sp/ra` are zeroed first. No new
  probe appears in any generation.

### 1.2 RDRAM: memory size is detected, never configured **[V]**

Unchanged from devkit_survey §2.1/§3.3 for all five: `rdram_init` returns `size_MB << 20`
(2.00 `BFC4113C`), the kernel keeps it at the `GetMemorySize` variable. The 1.00J module prints
`# Total accessable memory size: %d MB (%c:%d:%d:%d)` (one fewer field than later builds) and
`# failed to initialize memory: InitRDRAM returned %d`. All five print to the EE SIO. 1.00J's
`RDRAM1`/`RDRAM2` are unreferenced alternates (survey §1.1); they are not byte-identical to the
devkit's `RDRAM`.

### 1.3 KERNEL **[V]**

| probe | where (2.00 / others) | used for |
|---|---|---|
| `0x70003FF0` (RDRAM result) and ROM word `0xBFC001F8` | `k:_kernel_entry` `80001000` (all five) | `GetMemorySize`; `MachineType` (0x7E) returns the word, `PSMode` (0x7D) ORs `0x8000` into it; the word is `0` in all five ROMs (`xxd 0x1F8`) |
| COP0 `PRId` (r15) | `Deci2Init`: 2.00 `8000EF2C` (`k_GetCop0(0xF)`); 1.00J `8000C0B8`, dev `8000E4A0`, 1.60E `8000EE60`, 1.60A `8000ECF8` | `CPUID=%x` in the DECI2 banner |
| **`lhu 0xBF803800`** | `8000EF50`–`8000EF60` (2.00); the decompile of all four other kernels shows the same `uRambf803800` argument | **`BoardID=%x`** — this closes the survey's open question: BoardID is a 16-bit read of IOP-bus address `0x1F803800`, the same `0x1F8038xx` block as the TOOL UART (`0x1F80380C`/`0x1F803820`, hidden_features §3.3). On a retail board nothing answers there **[I]** (open-bus value), which is why the banner's BoardID is meaningless on consoles and PCSX2 |
| `lhu 0xBFC00102`, `lhu 0xBFC00100` | `8000EF5C`–`8000EF70` | `ROMGEN=%04x-%04x` = the BCD date at ROM `0x100` printed as *year-monthday*: 2.00 `2004-0614`, 1.00J `2000-0117` (the 1.00J RESET holds `17 01 00 20`) |
| `GetMemorySize` | banner `%dM` | |
| **`rom0:ROMVER` (1.60A only)** | `k:8000D518`, called from `GsInit` `8000D3C0` (= the first thing `HardwareInit` does) | finds the ROMDIR (`8000D5F0`), looks up `ROMVER` (`8000D688`), copies 15 bytes to `k:80015838`; `# panic !` + `Exit(1)` if missing. **Nothing reads `0x80015838`** (full-image scan for `lui 0x8001 / 0x5838..0x5847`; only the writer). 1.60E, dev, 1.00J and 2.00 kernels contain no `ROMVER` string at all — **correction** to devkit_survey §1.2: the "1.60 ROMVER reader" exists only in the 2002 kernel and is write-only |
| GS revision `(GS_CSR >> 16) & 0xFF == 1`, `(0x80022618 & 0xF0) == 0x20`, OSD config word `0x80015960` | `k_SetGsCrt` `8000BD58` (2.00) | selects the SMODE1/SYNCH1 register sets; the "0 → NTSC/PAL by config" default comes from `SetOsdConfigParam` bits, not from ROMVER |

`ROMGSCRT` (1.60A and 2.00 only, ROM `+0x80000` = `0x9FC80000`, 0x3898/0x3840 bytes) is **not a
table but code**: header `"ROMGSCRT"`, two entry vectors at `+0xC`/`+0x10` → `0x9FC80218`
(a 3×`lh` wrapper → `0x9FC82D48`, SetGsCrt-shaped) and `0x9FC80230` (5-arg → `0x9FC837C8`), a
per-mode jump table from `+0x50`, and at `0x9FC80250` a read of **IOP RAM word `0x3C0` through
`0xBC000000`** (the EECONF config cache, §1.5). **No module in either ROM references it**: byte
search of every ROMDIR entry for `lui 0x9FC8`/`0xBFC8` pairs and for the entry pointers finds
only ROMGSCRT itself; the unpacked OSDSYS/TESTMODE string tables have no `ROMGSCRT` and no
`rom0:ROMGSCRT`. **[V]** It is reachable only by something that knows the fixed ROM address
(the HDD-OSD / DVD-player updates are the candidates **[I]**). **correction** to devkit_survey
§1.1/§3.1: the kernel's `SetGsCrt` does not use it.

### 1.4 EELOAD's one probe **[V]**

`*(byte*)(0x1F402005) & 0x10` → `rom0:TESTMODE` else `rom0:OSDSYS`: 2.00 `el:8258C`, 1.60A
`el:825A8`, dev = 1.60E `el:8253C`. **1.00J has no such test** (`el:82220` main: `rom0:OSDSYS`
is the only cold-boot target and `rom0:TESTMODE` does not exist in that ROM). EELOAD never reads
ROMVER, NVRAM or the MachineType word in any ROM.

### 1.5 IOP boot: IOPBOOT, EECONF **[V]**

`IOPBOOT` (`rom:BFC4A000`, identical in all five) takes `(ram_MB, mode, cmdline)`:
`ram_MB < 3 → 2`; builds the name `"IOPBTCONF"` then **overwrites its 9th character with
`'0' + mode`** (`BFC4A0E0`–`BFC4A0EC`: `strcpy(name,"IOPBTCONF"); name[8] = mode + '0'`, retry with the plain name at `BFC4A110`), looks
that up in the ROMDIR and falls back to `IOPBTCONF` if absent. So `mode 2` selects
**`IOPBTCON2`** — the list without `EECONF/SIFCMD/LOADFILE/CDVDFSV` and with `SIO2MAN MCMAN`
that every ROM carries — and the cold boot (`mode 0`, §1.1) tries the non-existent `IOPBTCON0`
first. The parser understands `@<hex>` (load address), `!addr <n>` (`BFC4A244`) and `#` comments;
the optional `cmdline` is copied to IOP `0x20020` and linked into the boot-mode list that
`QueryBootMode` serves (`BFC4A074`–`BFC4A0A4`; which key it gets was not traced). Who passes
`mode 2`: MODLOAD's `ReBootStart` for any non-empty reboot string (§5.2), which no in-ROM
module calls; TBIN/PS1 mode is the likely user of `IOPBTCON2` **[I]** (TBIN was not
decompiled). On the five ROMs as shipped the cold boot and every UDNL reboot use `IOPBTCONF`
or the archive's own list.

`EECONF` (12th entry of `IOPBTCONF`; 1.00J/dev `eeconfig`, 1.60+ `eeconfig8`) is the first code
that talks to the MechaCon on every boot (2.00 copy, `iop:EECONF+0x638`):

1. `QueryBootMode(3)`: if absent or its flags `& 3 == 0`, spin up to `0x30000` iterations until
   **CDVD status `0x1F402005` bit 3** is set (MechaCon ready **[I]**).
2. `AllocSysMemory(1, 0x60)` and **store the pointer at IOP RAM `0x3C0`** (`iRam000003c0`).
3. S-commands `0x40` (OpenConfig: `{0, 0, 4}`), `0x41` ×4 (ReadConfig, 15-byte records with an
   additive checksum, `+0x1FC`), `0x43` (CloseConfig) → bytes `0x00..0x3B`; then OpenConfig
   `{1, 0, 2}` + 2 × `0x41` → bytes `0x3C..0x59`; `byte[0xF] bit 3 ← byte[0x4B] bit 3`. Any
   failure → the `0x3C0` pointer is reset to 0.
4. If `QueryBootMode(4)` halfword is 0: `+0x5C0` — if `(0xBF803204 - 0x60) < 2` write
   `0xBF803218 = 0` (TOOL board block, §1.3 **[I]**), else if DEV9 rev `(0xBF80146E & 0xF0) ==
   0x30` write `0xB600000A = 0`, else `0xBF801470/0xBF801472 = 0` (DEV9 PCMCIA power off
   **[I]**); then if `(0xBF80847C & 0xF0000000) == 0x10000000` program the i.LINK controller
   (`0xBF808414 = 0x417F0000`, wait `0xBF808400 & 1`, mask registers `0xBF80844C/20/28/30`).
5. If CDVD status bit 1 set and bit 2 clear: OpenConfig `{1,0,1}`, read one record, and if any of
   its 15 bytes is non-zero **write it back as zeros** (`0x42` WriteConfig through `+0x39C`).

The `0x3C0` pointer is what OSDSYS reads with `LoadFileGetIopAddr(0x3C0)` (boot_sequence §2.4
step 9; `analysis/osdsys_named/c/00209eb8.c:176`, then the config bytes behind it) and what
ROMGSCRT reads (§1.3). The 1.00J `eeconfig` build was not decompiled; its strings are the same.

### 1.6 Region / video letter **[V]**

Nothing before OSDSYS branches on the ROMVER letters: EELOAD and the IOP modules never open
`ROMVER`; the only pre-OSD reader is the 1.60A kernel copy (§1.3), unused. Region and video
mode first matter in OSDSYS (`sceCdBootCertify` block, osdsys notes). In XCDVDMAN 2.00
`sceCdBootCertify` is S-command `0x1A` with the 4 bytes `{major, minor, region, type}`
(`iop:XCDVDMAN+0x6F3C`, debug print `BootCertify %d %d %d %d`); the reply byte is read into a
stack temporary and **discarded** by the IOP; what the MechaCon does with it is not in the ROM.

### 1.7 `host:` / `hostfs` **[V]**

No pre-OSD module of any of the five ROMs contains `host`, `host0:`, `hostfs`, `dsidb`, `drfp`,
`dsnetm` or `ttyp` (strings of every non-packed ROMDIR entry; the only hits are a `ThreadInfo`
debug-type string in TESTSPU). The kernel's only DECI2 text is the banner and the panic string.

---

## 2. EELOAD's argv / command grammar

### 2.1 How EELOAD is entered **[V]**

| entry | argc / argv[0..] | ROMs |
|---|---|---|
| cold boot (`CreateRomThread("EELOAD")`) | `argc = 0` | all |
| `LoadExecPS2(path, argc, argv)` syscall 6 → `KLoadExec` (`k:800055A0`; 2.00 `80005598`) | `"EELOAD"`, `path`, `argv…` | all |
| `ExecOSD(argc, argv)` syscall 0x7B | `"EELOAD"`, `"rom0:OSDSYS"`, `argv…` | all |
| `Exit()` syscall 4 → `ExitToBrowser` (1.00J `k:80005900`; dev/1.60E/1.60A `k:800059A8`; 2.00 `800059A0`) | `"EELOAD"`, `"rom0:OSDSYS"`, `"BootBrowser"` | all |

### 2.2 The grammar, by generation **[V]**

```
argv[1] starts with "moduleload"   → option list follows (1.00J: el:82294, string 0x895C8)
argv[1] starts with "moduleload2 " → the REST of argv[1] is the IOP reset string (1.50+ only;
                                     2.00 el:90750, 1.60A el:907D0, dev el:90AC0)
   -m <path>   LOADFILE fno 0  (LF_F_MOD_LOAD)                 1.00J el:82BE8 / 2.00 el:82A90
   -k <path>   LOADFILE fno 4  (LF_F_MG_MOD_LOAD, KELF module) 1.00J el:82C08 / 2.00 el:82AB0
   -x <path>   LOADFILE fno 5  (LF_F_MG_ELF_LOAD, KELF, "all"), then ExecPS2 at once with
               argv = {<path>, rest…}; failure → BootError (never BootIllegal)
   anything else ends the option list
last argv      LOADFILE fno 1  (LF_F_ELF_LOAD, "all") → ExecPS2(entry, gp, argc-n, &argv[n])
               -2 → "BootIllegal" (1.60A, 2.00 only); other <0 → "BootError <path>"
```

* **1.00J (`el:82220`)**: `moduleload` only; **no IOP reset at all** on the LoadExecPS2 path
  (the IOP keeps whatever OSDSYS loaded, LOADFILE must still be alive); every step is printed
  with `kprintf`-style routines **straight to the EE SIO** (`el:824A8` writes `0x1000F180`
  after polling `0x1000F130 & 0x8000`): `load_module %s`, `module id = %d`, `load_Kmodule %s`,
  `load_Kelf %s`, `# LoadExec '%s': pc=%08x` (after `-x`), `# LoadExec '%s':pc=%08x` (final
  ELF), `# Loader 'rom0:OSDSYS':pc=%08x` (cold boot), `# Loader: can't load  %s`, and
  `\n# LoadExec: can't load %s\n` before `BootError`. So a 1.00J console prints the OSDSYS entry
  point on the serial port at every boot, and every game launch prints its ELF entry.
* **dev = 1.60E (`el:82350`)**: adds `moduleload2 `, the IOP reset (`IopResetAndWait("rom0:UDNL
  rom0:EELOADCNF")` unless `moduleload2` supplied a string), `rom0:TESTMODE`, and the
  `too long parameter '%s'` string of an SDK `sceSifRebootIop`-style helper (2.00 `el:85180`:
  prepends `"rom0:UDNL "` to its argument, 80-byte limit) that **nothing calls** in any build
  (dead library code). No `BootIllegal`.
* **1.60A (`el:82388`) and 2.00 (`el:82388`)**: as above plus `BootIllegal` on `-2` from the
  final load (`el:82548`/`el:8254C` 1.60A). The Ghidra decompile of 1.60A shows a second
  `LoadElfAll` after the `-2` test; the assembly (`bgezl` + delay-slot `lw a0`) shows it is one
  load.

Consequences: a caller can boot the IOP from *any* image with
`LoadExecPS2("moduleload2 rom0:UDNL <img> [<img>…]", …)` — OSDSYS itself uses
`moduleload2 rom1:UDNL rom1:DVDCNF` for the DVD player (osdsys_flow §3). The `-x` item is the
only encrypted-ELF entry and it never produces `BootIllegal` (**correction** to
hidden_features §2.3: `-2` → `BootIllegal` applies to the final plain-ELF argument, i.e. a
`cdrom0:` game, not to the memory-card `-x` update).

### 2.3 Where `BootIllegal` really comes from: XLOADFILE's disc check **[V, 2.00; 1.60A has the same strings and sizes ±48 bytes]**

`XLOADFILE` `loadelf` (`+0x1320`): if the path starts with `"cdrom"` it opens the file with
mode `4` and sets `DAT_21A0 = 1`; after the ELF/program headers are read (`+0x1180`) it runs
`+0x1098`:

```
t = GetDiskType()              # open "cdrom0:sce_dev5", ioctl2 0x10030 → CDVD reg 0x1F40200F
t < 5                          → -204 (no disc / still detecting)      → BootError
5 ≤ t < 0x12 (unknown, PS1 CD, PS1 CDDA) → -2                             → BootIllegal
0x12..0x14 (PS2 CD, PS2 CDDA, PS2 DVD):
    sceCdReadKey(0, 0, 0x4B, key)     # ioctl2 0x10020 → XCDVDMAN+0x7EE8: seek + read key regs 0x1F402020..38
    e = last CDVD error byte          # ioctl2 0x10010 → XCDVDMAN+0x5200
    e == 0                      → 0 (load continues)
    e == 0x30 or 0x37           → GetDiskType()==0 ? -204 : -2            → BootIllegal
    other e                     → -204                                     → BootError
t ≥ 0x15 (DVD-Video 0xFD, CDDA 0xFE, ILLEGAL 0xFF) → -2                   → BootIllegal
then ioctl2 0x10000 (SpinCtrl, 1)
```

So the "illegal disc" screen is a MechaCon verdict relayed through `sceCdReadKey(…,0x4B)`'s
error byte **[V]**; what 0x30/0x37 mean inside the MechaCon is not in the ROM **[I]: the copy-protection
/ wobble check**. 1.60E/dev XLOADFILE (`0x2A09` bytes, no `sce_dev5` string) has no such step,
consistent with their EELOAD having no `BootIllegal`. The 1.60A XCDVDMAN gained the raw
`sce_dev5;1` device handle (hidden_features §4.3) precisely for this check. XCDVDMAN 2.00's
ioctl2 map (`+0xAF0`): `0x6310` → IOP `PRId`; `0x6311/0x6312` → set/get bit 0 of `0xBF808284`
(rejected with `-22` when `PRId < 0x23`); `0x10000` SpinCtrl; `0x10008` → `+0x8238` (3 args);
`0x10010` last error; `0x10020` ReadKey; `0x10030` disc type.

### 2.4 LOADFILE / XLOADFILE function tables **[V]**

LOADFILE (`.data+0x1BC0`, file `0x1C60`), identical in all five: fno 0 `+0x150` loadmodule, 1
`+0x240` loadelf, 2 `+0x420` set val, 3 `+0x364` get val, 4 `+0x1FC` MG-module (through
MODLOAD's SECRMAN callbacks), 5 `+0x2FC` MG-ELF. XLOADFILE 2.00 (`.data+0x1EC0`): same six plus
**fno 6 `+0x4C8` `loadbuffer`** (load a module already in IOP RAM: `loadbuffer: addrres %x args
%d arg %s`). Server id `0x80000006`, version `0x101` (LOADFILE) / `0x102` (XLOADFILE). So the
KELF path (`fno 5`) exists in 1.00J too — the RPC table is the same file.

### 2.5 The config archives **[V: `strings` of every ROM's `EELOADCNF`/`OSDCNF`/`IOPBTCONF`/`IOPBTCON2`]**

| list | 1.00J | dev / 1.60E / 1.60A | 2.00 |
|---|---|---|---|
| `IOPBTCONF` (cold boot) | 29 modules, boot_sequence §3.1 | identical | identical |
| `IOPBTCON2` | `… ROMDRV STDIO SIFMAN IGREETING CDVDMAN SECRMAN SIO2MAN MCMAN` (no `ADDDRV`) | + `ADDDRV` after `ROMDRV` | = |
| `EELOADCNF` | **absent** | first list with `XLOADFILE XCDVDMAN` in place of `LOADFILE CDVDMAN` | same but **`NCDVDMAN`** (a 2.00-only, smaller CDVDMAN without `sceCdBlueLEDCtl`/config/NVM prints) |
| `OSDCNF` | **absent** | 37 modules (no `XRMMAN2`) | 38 (+ `XRMMAN2`) |

Each archive is a ROMDIR (`RESET ROMDIR EXTINFO IOPBTCONF`) whose EXTINFO comment names the
build (`aki@aki-linux … 20001228` for dev, `kuma@rom-server/~/f10kDVE4.3` 1.60E, `g30k` 1.60A,
`k200iop` 2.00). 1.00J has no archives because its OSDSYS loads drivers itself
(`-m rom0:SIO2MAN`, `rom0:MCMAN`, `rom0:MCSERV`, `rom0:PADMAN`, `rom0:CLEARSPU`, `rom0:OSDSND`
strings in its OSDSYS) instead of rebooting the IOP.

### 2.6 UDNL: how a config line changes what loads **[V, 2.00 copy = dev/1.60E/1.60A]**

`UDNL` (`iop:UDNL+0x16C` main) is the "updater" MODLOAD runs after an IOP reset (§5.2):
`-v` (`+0x17B0`) raises verbosity; every other argument is a file path opened through IOMAN
(any device whose driver is resident: `rom0:`, `rom1:` after ADDDRV, `cdrom0:`, `mc0:` …),
read whole into memory and parsed as a ROMDIR. The ROM at `0xBFC00000` is always slot 0.
`IOPBTCONF` is then looked up **from the last argument back to the ROM** (`+0x55C` iterates
`n-1 … 0`), so an archive given later overrides one given earlier and both override `rom0`. The
conf parser (`+0x5F4`) accepts `@<hex>` (load address), `!addr <n>`, `!include <file>` (recursive
include from the same image set), `#` comments, and module names resolved with the same
last-image-first rule — which is why `OSDCNF` only has to contain an `IOPBTCONF`. Errors:
`file '%s' can't open`, `pannic ! can not alloc memory`, `panic ! '%s' not found` (then
`trap`). UDNL then disables interrupts, relocates the first two listed modules itself and jumps
to the second with the boot-image descriptor (`+0x9D0`): it is a private IOPBOOT and never
returns to MODLOAD. Hence an `OSDCNF`/`EELOADCNF`/`DVDCNF` line is simply "name from the
highest-priority image that has it"; a memory-card or disc archive given as an extra argument
would replace ROM modules by name.

### 2.7 Dispatch of each target **[V]**

| target | path |
|---|---|
| `rom0:OSDSYS` | EELOAD cold boot / `ExecOSD` / `Exit` → LOADFILE fno 1 `"all"` → `ExecPS2(entry, gp, 1, {"rom0:OSDSYS"})` (cold boot gives `argc = 1`; `Exit` gives `{"rom0:OSDSYS","BootBrowser"}`) |
| `rom0:TESTMODE` | EELOAD cold boot, `0x1F402005 & 0x10` (not 1.00J) |
| `rom0:PS1DRV` | **not an EELOAD path**: `PS1DRV` is an IOP IRX launched by OSDSYS (`rom0:PS1DRV` string in every OSDSYS); the PS1 mode itself is TBIN at the IOP reset vector (§1.1) |
| disc ELF (`cdrom0:\SLxS_…;1`) | OSDSYS `LoadExecPS2` → EELOAD reset `rom0:UDNL rom0:EELOADCNF` → fno 1 → XLOADFILE disc check (§2.3) → `BootIllegal`/`BootError`/run |
| `mc0:` ELF | only through `-x` (fno 5, KELF via SECRMAN `SecrCardBootFile`); 1.00J OSDSYS looks for `mc?:/BIEXEC-SYSTEM/osdsys.elf` and `mc?:/BIEXEC-DVDPLAYER/dvdplayer.elf`, the DVD player having shipped on a memory card for SCPH-10000 |
| DVD player from ROM | OSDSYS `moduleload2 rom1:UDNL rom1:DVDCNF` + `-x` of the player ELF (osdsys_flow §3); `rom1:` is created by ADDDRV (§3.5) |

---

## 3. IOP-side gates

### 3.1 SECRMAN **[V]**

Export table (2.00 = 1.00J = 1.60E = 1.60A, same file): 4 `SetMcCommandHandler`, 5
`SetMcDevIDHandler`, 6 `SecrAuthCard` (`+0xA4`), 7 `SecrResetAuthCard`, 8–10
`SecrCardBootHeader/Block/File`, 11–13 `SecrDiskBootHeader/Block/File`. Imports: `cdvdman[29]`,
`ioman`, **`modload[12]` = `SetSecrmanCallbacks`** — at start it hands MODLOAD the three
disk-boot functions, which is how LOADFILE fno 4/5 and the reboot-mode-1 updater (§5.2) decrypt
KELFs. Steps (strings in order): `mechacon auth 0x80..0x88, 0x8f`, `card auth 0x00..0x14, 0x60`,
`card auth key change` (cex only; hidden_features §4.2), `card decrypt start, 0x40..0x43`,
`SecrCardBootFile`/`SecrDiskBootFile`: `Cannot decrypt header`, `Set Header failed`,
`secr_set_header: fail pol_cal_cmplt / write_HD_start / write_data`, `check sum error %d`,
`kbit_offset %d`, `kc_offset %d`, `don't get elf header`, `don't get program header`.

Who authenticates a memory card: **XMCMAN**, not OSDSYS. `XMCMAN` imports `secrman[4,5,6]`;
its init (`+0x1944`) registers its SIO2 command and dev-id handlers with SECRMAN, and
`SecrAuthCard` is called from the card-detect/mount routine `+0x2C74` (behind export `+0x82C`).
It also registers `SetCheckKelfPathCallback` (`modload[13]`, `+0xE204`) so MODLOAD can validate
`mc?:` KELF paths, and timestamps files with the MechaCon RTC (`cdvdman[51]`, `+0x1768`,
fallback 2000-04-03 on error). **At the first boot no card is touched**: `IOPBTCONF` has no
`SIO2MAN`/`MCMAN`; the first card access is OSDSYS's `TryMcSystemUpdate` after boot #2 (and on
1.00J after its own `-m rom0:MCMAN`).

### 3.2 XCDVDMAN MechaCon-version gates **[V]**

All builds read the version with S-command `0x03` sub `0x00` (`sceCdMV`; 2.00 `+0x6CE4`, retry
≤ 100 while reply byte 0 has bit 7; `_sceCdMV error`) and compare `b1<<16 | b2<<8 | b3`:

| threshold | dev / 1.60E / 1.60A | 2.00 flag | gates (2.00) |
|---|---|---|---|
| ≥ 1.08.00 | flag A | `+0x9BFC` | read-command issue path `+0x4D74` (with disc-type `0x14/0xFC/0xFD/0xFE` cases) |
| ≥ 2.02.00 | flag B | `+0x9C00` | DVD read set-up `+0x3848` |
| ≥ 2.04.00 | — | `+0x9C04` | S-cmd `0x1B` `CancelPOffRdy` is only sent when set (`+0x6FA4`) |
| ≥ 2.08.00 | — | `+0x9C08` | set, unused |
| ≥ 5.00.00 | — | `+0x9C0C` | S-cmd `0x21` (8-byte BCD time + 2 bytes, `+0x71C4` = `sceCdSetWakeUpTime`-style) and five more sites (`+0x49DC`, `+0x7708`, `+0x79A8`, `+0x751C`, `+0x76A4`, `+0x7B28`, `+0x73A8`) |
| ≥ 5.02.00 | — | `+0x9C10` | S-cmd `0x27` (13-byte reply, `+0x7798`); otherwise returns status `0x100` without talking to the MechaCon |
| ≥ 5.04.00 | — | `+0x9C14` | S-cmd `0x28` (`+0x7858`) and S-cmd `0x03` sub `0xEF` (`+0x78C8`, 14-bit value × 3125/100) |
| ≥ 5.06.00 | — | `+0x9C18`, and `+0x9C24 = (b3 & 0xF) == 1` | `sceCdTrayReq(1)` is **not sent** when `+0x9C24` (`+0x7E20`) |
| ≥ 6.00.00 | — | `+0x9C1C` | S-cmd `0x36` (15-byte reply, `+0x7A60`) |

Byte 3's low nibble is the MechaCon's model/region nibble **[I]** (the `≥ 5.06 && nibble == 1`
test is the only place it matters). Under an emulator the MechaCon-version reply decides which
of these S-commands the 2.00 driver issues at all; the full S-command set the 2.00 XCDVDMAN
can issue is `0x0B 0x0C 0x0F 0x10–0x1D 0x1F 0x21 0x22 0x24–0x28 0x36 0x40–0x43` (grep of
`+0x413C` call sites). The legacy `CDVDMAN` (first boot, identical in all five) has no version
probe and only `0x40/0x41/0x43` config plus NVM read/write.

### 3.3 `sceCdReadConfig` blocks and who reads them **[V]**

* S-commands `0x40` OpenConfig `{block, write?, count}`, `0x41` ReadConfig (15 bytes +
  checksum), `0x42` WriteConfig, `0x43` CloseConfig — implemented in CDVDMAN/XCDVDMAN
  (`ReadConfig fail Command busy / status: 0x%02x`) and in EECONF's private copy.
* **EECONF** at every IOP boot #1: block 0 × 4 + block 1 × 2 → IOP RAM via the `0x3C0` pointer
  (§1.5), plus the "clear block 1 record 0 if non-zero" write when CDVD status bits are
  `..01.` (§1.5 step 5).
* **OSDSYS**: `ReadConfigBlock` = OpenConfig `(1, 0, 2)` (hidden_features §5.3) and the `0x3C0`
  bytes (boot step 9).
* **ROMGSCRT** (dormant) reads the `0x3C0` cache (§1.3).
* `NCDVDMAN` (2.00 `EELOADCNF`) has no config/NVM strings: a game launched from the OSD runs
  with a CDVDMAN that cannot read NVRAM config at all until it loads its own driver.

### 3.4 DEV9 / network adapter detection (2.00 only) **[V]**

`XDEV9` `+0x310`: DEV9 revision `0xBF80146E & 0xF0`: `0x20` → `CXD9566 detected` (PCMCIA
slot, `+0x15DC`), `0x30` → `CXD9611 detected` (expansion-bay SSBUS buffer, `+0x1978`), else
`unknown dev9 hardware` → module fails (OSDSYS's `Dev9LoaderThread` state 3). Inside `+0x15DC`:
`QueryBootMode(6)`; if present and `(*p & 0xFE) == 0x60` → **`dev9: T10K detected`**, then
`0xB4000000 == 0xA1` is required (`cannot detect AIF.` otherwise) and `0xB4000006 = 4` — the
DTL-T10000 TOOL's AIF chip, the only place a ROM module names the TOOL. Then the SPEED chip
probe (`+0xCD8`: `no card`, `SPEED Lite not found`, `Speed chip: Rev%x`, MANFID tuples). The
module also parses `-sa SA_THPRI|SA_THFIFO`. `XDEV9SERV` is the EE RPC face (`dev9_serv`).
No DEV9 module exists in the other four ROMs (DEV9 drivers come from the disc/HDD-OSD).

### 3.5 DVD-player ROM and decryption **[V]**

* `ADDDRV` (dev/1.60E/1.60A/2.00, 1113 bytes): saves SSBUS `GetDelay(1)`/`GetBaseAddress(1)`,
  sets `SetBaseAddress(1, 0xBE000000)` and `SetDelay(1, 0x18344F)`, then calls **ROMDRV export
  4** `AddRomDrive`-style with base `0xBE000000`; on failure prints `ROM directory not found`
  and restores the old timing. `ROMDRV` registers the IOMAN device `"rom"` whose unit n is entry
  n of a 12-byte table (`.data+0x940`); unit 0 is found by scanning `0xBFC00000..0xBFC40000` for
  the `RESET` ROMDIR (`+0x540`). **So `rom1:` = the ROMDIR at SSBUS chip-select 1, physical
  `0x1E000000` (the DVD-player flash)**; there is no `erom0:` device in any of these ROMs.
* `sceCdDecSet` lives in XCDVDFSV (`DEC SET call 0x%08x`) → cdvdman; it arms an XOR/rotate of
  every following sector read **[I: PCSX2's implementation; the cdvdman side was not traced]**,
  used by OSDSYS when it reads the encrypted player off `rom1:`/disc (osdsys_flow §3).
* The 1.60A HDDOSD and the dev/1.60A TESTMODE are the only other modules naming `rom1:`
  (TESTMODE's `DEV1ROM_TEST`, hidden_features §1.4).

### 3.6 Other gate modules **[V]**

* `XSIO2MAN` (`PsIIsio2man 1600`) and `SIO2MAN`: no probes; one diagnostic string.
* `XPADMAN` (`Pad Driver for OSD (2000/12/05)`, `PsIIpadman 2101`): current-limit check
  `padman: Over Consumpt Max 600mA[%d][%d]`, no environment branch.
* `XRMMAN2` (`PsIIrmman2`, 2.00 only): remote-control RPC, no probes.
* `RMRESET` (`rmreset start/end`): one-shot SIO2 reset of the remote receiver.
* `REBOOT` (`Reboot service module.(99/11/10)`): SIF handler `RebootByEE` → MODLOAD (§5.2).

---

## 4. Debug and development hooks

### 4.1 DECI2 **[V]**

Every kernel registers the same three protocols in `Deci2Init` (`0x201` DCMP, `0x21F` debug,
`0x230` KTTY) and special-cases syscall `0x7C` (hidden_features §3.1); addresses per ROM in
§1.3. Kernel builds: 1.00J `Dec 10 1999`, dev `May 11 2000`, 1.60E `Jul 4 2001`, 1.60A
`Feb 7 2002`, 2.00 `Feb 6 2003`; all say `EE DECI2 Manager version 0.06`. The link words that
`Deci2ReqSend` needs are never set without a host, in all five.

### 4.2 The `0x80000100` (Level-2 / debug) vector and the serial port **[V, 2.00; strings present in all five]**

`k:8001415C` (reached from the Level-2 vector) saves the context, then `k:80012B48`: **if INTC
status (`0x1000F000`) bit 12 (SIO) is set** it prints `\npc=%08x\n` (`k:80016A08`) with the
interrupted PC and calls `k:800136F0`, which reports `UART: Frame/Parity/Overrun error.`
(`k:80016BD0..`) from `0x1000F110`, clears it (`= 0xE`), then **reads and discards every byte
of the RX FIFO `0x1000F1C0`** while `0x1000F130 & 0xF00`, and writes `0x1000F130 = 7`. After
that the handler falls into the DECI2 event stub `k:80012BB0(2)` (hidden_features §3.2). So a
byte arriving on the EE serial port does nothing except print the PC and park the EE; **no
kernel in any ROM loads code from the serial port**. The TX side (`kprintf` → `0x1000F180`)
is the only use of the port. The only SIO offsets the 2.00 kernel references are `0xF110`
(error), `0xF130` (status), `0xF180` (TX) and `0xF1C0` (RX); no write to the control/baud
registers (`0xF100`, `0xF120`, `0xF140`, `0xF150`) was found in the linear disassembly, so the
port runs at whatever the reset defaults are **[V for the search, I for the defaults]**.

### 4.3 What a devkit owner would see, in order **[V]**

RDRAM `# Initialize memory …` / `# Total accessable memory size …`; kernel `# Initialize
Start … Done.`, `# TLB spad=0 …`, blank line, `EE DECI2 Manager version 0.06 <date> <time>`,
`  CPUID=%x, BoardID=%x, ROMGEN=%04x-%04x, %dM`; then **on 1.00J only** EELOAD's
`# Loader 'rom0:OSDSYS':pc=%08x`; on `LoadExecPS2`: `# Restart.` / `# Restart Done.`, and
(1.50+) `# Restart Without Memory Clear.` / `… Done.` when a program calls `ExecPS2`
(syscall 7 → `k:80002F80` → `k:800057E8`, which on 1.50+ terminates every other thread and
runs the no-clear hardware restart; **the 1.00J `k:800057E8` only sets up the thread** —
`ExecPS2` on 1.00J leaves other threads and hardware alone). Then 1.00J EELOAD's
`load_module` / `load_Kelf` / `# LoadExec '%s':pc=%08x` lines.

### 4.4 Stubs and markers **[V]**

`_print` (0x75) is `jr ra` in all five; `MachineType` reads the zero word in all five; the 1.00J
kernel lacks `SetOsdConfigParam2`/`GetOsdConfigParam2` (0x6E/0x6F → the "undefined syscall"
printer; `tools/syscall_table.py`, table `k:80011F80`), everything else in the table is the
same 100 handlers (differences are relocation offsets). No kernel or EELOAD tests ROMVER's
`D`, the MachineType word, NVRAM or BoardID to change behaviour. The TOOL-only hardware tests
are in IOP modules: `XDEV9` (`T10K detected`, AIF at `0xB4000000`) and `EECONF` (`0xBF803204`
block). **dev vs 1.60A**: outside SECRMAN the devkit's pre-OSD code is the 1.50/1.60E retail
code (EELOAD byte-identical to 1.60E; KERNEL differs from 1.60A only by the 2002 additions in
§1.3/§4.3; RESET by the date).

### 4.5 `host:` boot, serial/DECI2 program load **[V: denied]**

None of the five ROMs can: no `host` strings (§1.7), the SIO RX path discards data (§4.2), the
DECI2 manager's receive path is only ever fed by the SIF2 DMA that no ROM module programs for
a host, and the LOADFILE server opens paths through IOMAN, which in a cold boot has only
`rom`, `cdrom` (CDVDMAN) and the SIF `fileio` devices.

---

## 5. Reset-time behaviour

### 5.1 Reset strings **[V]**

| string | sender | ROMs |
|---|---|---|
| `rom0:UDNL rom0:EELOADCNF` | EELOAD `LoadExecPS2` path (`el:90760` 2.00, `el:907E0` 1.60A, `el:90AD0` dev/1.60E) | all but 1.00J |
| `rom0:UDNL rom0:OSDCNF` | OSDSYS main | all but 1.00J |
| `rom1:UDNL rom1:DVDCNF` (via `moduleload2`) | OSDSYS DVD player | 1.50+ (osdsys_flow §3) |
| `rom0:UDNL ` + arg | `el:85180` (2.00) — dead code | — |

Format of the SIF reset command: `sceSifIopReset(str, flags)` (`el:84FE0` 2.00) stops the
SIF DMA, copies the string into a 0x68-byte command block with id `0x80000003`, flag word `=
flags`, sends it, then sets SIF registers `4 ← 0x10000`, `4 ← 0x20000`, `0x80000002 ← 0`,
`0x80000000 ← 0`; EELOAD always passes `flags = 0`.

### 5.2 What the IOP does with it **[V, MODLOAD/REBOOT identical in all five]**

`REBOOT` registers the `RebootByEE` SIF handler (`Get Reboot Request From EE`) and calls
MODLOAD's handler `iop:MODLOAD+0x760`: the request must carry a file name (`Reboot fail! need
file name argument`); the string is split into ≤ 15 argv; **the flag byte picks the loader**:
`0` → plain `LoadModule` of `argv[0]` (limit `0x100000`), `1` → load through the SECRMAN
disk-boot callbacks (an **encrypted KELF updater**), anything else → fail. The updater's entry
is called with `(argc, argv, 0, module)`; if it returns, `return from updater '%s' return
value = %d` and `trap`. The `ReBootStart` export (`+0x12E0`, `ReBootStart: Terminate resident
Libraries`) is the generic re-entry: it calls every resident module's exit hook, rewrites the
SSBUS timing tables from RESET's two copies (`0xBFC02008/0xBFC0200C`, `0xBFC02010/0xBFC02014`,
chosen by `PRId > 0xF && !(0xBF801450 & 8)` — the RESET test), looks `IOPBOOT` up in the ROM
and calls it with `(ram_MB, mode, cmdline, 0)` where `mode = 1` for an empty string and
`mode = (flags & 0xFF00) | 2` with the string copied to IOP `0x480` otherwise → `IOPBTCON1`
(absent → `IOPBTCONF`) or **`IOPBTCON2`** (§1.5). It is an export for updaters; UDNL does not
import it (it boots the image it built itself, §2.6) and no other in-ROM caller was found, so
the `IOPBTCON2` route is latent in these ROMs **[V for the code, I for "latent"]**.

### 5.3 Kernel restart paths **[V]**

`KLoadExec` (`k:800055A0`; 2.00 `80005598`): `# Restart.`, terminate threads, `HardwareRestart`
(clears RAM from `0x82000`), copies `rom0:EELOAD` to `0x82000` with the arg block
`"EELOAD"\0<path>\0<args…>` and jumps; `ExecOSD` = `KLoadExec("rom0:OSDSYS", …)`;
`Exit` = `KLoadExec("rom0:OSDSYS", 1, {"BootBrowser"})` (1.00J `k:80005900`, others §2.1). There
is no `_InitSys`; the only other restart is `ExecPS2`'s no-clear variant (§4.3).

### 5.4 Memory card / pad / remote before OSDSYS **[V]**

None: the first-boot list has no SIO2 driver, EELOAD opens nothing but the ELF it loads, and
cold-boot OSDSYS receives `argc = 1`. Every argv flag OSDSYS acts on (`BootBrowser`,
`BootError`, `BootIllegal`, `BootClock`, `SkipMc`, …) is produced by the kernel/EELOAD/OSDSYS
itself, never from a hardware probe in this layer.

---

## 6. Cross-ROM table

| hook | 1.00J | dev | 1.60E | 1.60A | 2.00 |
|---|---|---|---|---|---|
| `0x1F402005 & 0x10` → TESTMODE | no | `el:8253C` | `el:8253C` | `el:825A8` | `el:8258C` |
| EELOAD `moduleload2 <reset string>` | no | yes | yes | yes | yes |
| EELOAD IOP reset on LoadExecPS2 | **no** | `rom0:UDNL rom0:EELOADCNF` | same | same | same |
| EELOAD `BootIllegal` / XLOADFILE disc check | no | no | no | yes | yes |
| EELOAD prints to EE SIO | **yes** (`el:824A8`) | no | no | no | no |
| `EELOADCNF`/`OSDCNF` | — | yes | yes | yes | yes (`NCDVDMAN`) |
| `IOPBTCON2` present | yes | yes | yes | yes | yes |
| kernel BoardID = `lhu 0xBF803800` | yes | yes | yes | yes | yes |
| kernel ROMVER copy (write-only) | no | no | no | `k:8000D518` | no |
| `ROMGSCRT` (unreferenced) | — | — | — | yes | yes |
| syscalls 0x6E/0x6F | **missing** | yes | yes | yes | yes |
| `ExecPS2` restarts hardware | **no** | yes | yes | yes | yes |
| SECRMAN | cex | **dex** | cex | cex | cex |
| XCDVDMAN MechaCon flags | (CDVDMAN: none) | 2 + prints | 2 | 2 | 10 |
| `sce_dev5;1` raw device | — | no | no | yes | yes |
| EECONF → IOP `0x3C0` config cache | `eeconfig` (not decompiled) | = 1.00J | `eeconfig8` | = | `eeconfig8` |
| DEV9 / T10K detection | — | — | — | — | `XDEV9` |
| `rom1:` via ADDDRV (`0xBE000000`) | — | yes | yes | yes | yes |
| `host:` / serial load | none | none | none | none | none |

---

## 7. Tool-worthy findings, ranked

1. **The disc-legality verdict is readable**: `BootIllegal` = XLOADFILE's `sce_dev5` sequence
   (§2.3) — disc type from `0x1F40200F`, then `sceCdReadKey(0,0,0x4B)` and the error byte
   (`0 / 0x30 / 0x37 / other`). A tool can show for a given disc image + emulated MechaCon
   exactly which of the four outcomes (run / BootIllegal / BootError -204 / BootError) EELOAD
   will take, and that 1.60E/dev never reach BootIllegal.
2. **The reset-string grammar and the config override order** (§2.2, §2.6, §5.2): `moduleload2
   <updater> <img…>` + UDNL's last-image-first lookup + `!include/!addr` + MODLOAD's flag byte
   (plain vs KELF updater) + `IOPBTCON<mode>` fallback. Enough to predict the exact IOP module
   set for any reset string a BIOS/OSD/game issues, and to explain why `IOPBTCON2` exists.
3. **EECONF's MechaCon config cache at IOP `0x3C0`** (§1.5): the first MechaCon transaction of
   every boot, with the exact S-commands, the 0x60-byte layout, the TOOL/DEV9/i.LINK register
   pokes, and the consumers (OSDSYS step 9, ROMGSCRT). A tool can render "what the console
   learned from NVRAM before the OSD ran" from an NVM file.
4. **BoardID = `lhu 0x1F803800`, ROMGEN = year-monthday** (§1.3): lets the serial-log view
   print the real banner for each ROM, and flags BoardID as a TOOL-board register.
5. **XCDVDMAN's ten MechaCon-version thresholds** (§3.2) with the S-commands each one unlocks:
   given an emulated MechaCon version, the tool can list which commands the 2.00 driver will or
   will not issue (`0x1B`, `0x21`, `0x27`, `0x28`, `0x03/0xEF`, `0x36`, `TrayReq`).
6. **1.00J EELOAD's serial narration** (§2.2, §4.3): `# Loader 'rom0:OSDSYS':pc=%08x` at every
   boot and `# LoadExec '%s':pc=%08x` per game — visible today in PCSX2's EE SIO console with
   the SCPH-10000 BIOS; and 1.00J never resets the IOP on `LoadExecPS2`.
7. **`ExecPS2` restarts the hardware from 1.50 on, not on 1.00J** (§4.3): a syscall-level
   behaviour difference games can hit.
8. **ROMGSCRT is orphaned code** (§1.3) and **the 1.60A kernel's ROMVER copy is write-only**:
   two "features" a ROM viewer should label as dormant rather than active.
9. **DEV9 identity**: `0xBF80146E` revision → CXD9566/CXD9611/unknown, T10K via
   `QueryBootMode(6)` + AIF (§3.4) — the only ROM-side TOOL detection, 2.00 only.
10. **Identity matrix** (§0): SECRMAN unchanged 2000→2004; devkit EELOAD = 1.60E; IOPBOOT,
    LOADFILE, MODLOAD, CDVDMAN identical in all five — a diff viewer can collapse these.

---

## 8. Could not determine

* The meaning of CDVD error bytes `0x30`/`0x37` after `ReadKey 0x4B`, of MechaCon version
  byte 3's low nibble, and of the `≥ 5.06 && nibble == 1` tray rule: MechaCon-side.
* What answers at `0x1F803800` (BoardID) and `0x1F803204/0x1F803218` on a TOOL, and what a
  retail board returns there; PCSX2's value was not checked.
* Who calls `ROMGSCRT` (`0x9FC80218/0x9FC80230`) — nothing in either ROM; a disc/HDD update
  is the guess.
* `QueryBootMode` keys 3, 4 and 6: their producers (IOPBOOT's `!addr`/cmdline records vs
  LOADCORE) were not traced; EECONF and XDEV9 only consume them.
* Whether the EE SIO Level-2 interrupt is unmasked during normal operation (so that a stray
  byte on the serial port would park the EE in the debug stub): the Status/INTC mask values
  were not followed through `HardwareInit`.
* `sceCdDecSet`'s cdvdman implementation, `TBIN`'s use of `IOPBTCON2`, the 1.00J `eeconfig`
  and `UDNL` differences (16 bytes), and `NCDVDMAN` beyond its string set.
* The 1.60A ↔ 2.00 XLOADFILE code diff was inferred from identical strings and a 48-byte size
  change, not from a function-level diff.
