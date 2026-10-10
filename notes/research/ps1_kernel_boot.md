# The PS1 kernel's boot path and hand-off checks

Scope: what the PlayStation ROM does between reset and the first instruction of a game — the
bootstrap in ROM, the kernel it copies to RAM, the hand-over to the shell and back, the disc
decision, `SYSTEM.CNF`, the PS-X EXE load. The shell's *visual* scenes and the per-dump version
matrix are other notes; this one follows the decision logic and gives the addresses a tool needs
to print "the kernel read X at LBA Y and compared it with Z at ROM 0x…".

| tag | file (PCSX2 bios dir) | 0xBFC00100 date / type | kernel maker (0xBFC0012C) | tail string (0xBFC7FF32) |
|---|---|---|---|---|
| **1.0J** | `Sony PlayStation BIOS (J)[SCPH-1000].bin` | `0x19940922` / 3 | `CEX-1000 KT-3  by S.O.` | *(none)* |
| 2.2A | `Sony PlayStation SCPH-1001 - DTLH-3000 BIOS v2.2 …(US).bin` | `0x19951204` / 3 | `CEX-3000/1001/1002 by K.S.` | `System ROM Version 2.2 12/04/95 A` |
| 2.2E | `Sony PlayStation BIOS (E)(v2.2)(1995-12-04)[DTLH-3002].bin` | `0x19951204` / 3 | same | `… 2.2 12/04/95 E` |
| 2.2J | `Sony PlayStation BIOS (J)(v2.2)(1995-12-04)[SCPH-5000].bin` | `0x19951204` / 3 | same | `… 2.2 12/04/95 J` |
| **3.0A** | `scph5501.bin` (primary reference) | `0x19951204` / 3 | same | `… 3.0 11/18/96 A` |
| 3.0E | `Sony PlayStation BIOS (E)(v3.0)(1997-01-06)[SCPH-5502 + SCPH-5552].bin` | `0x19951204` / 3 | same | `… 3.0 01/06/97 E` |
| 4.1A | `Sony PlayStation BIOS (U)(v4.1)(1997-12-16)[SCPH-7001 + SCPH-9001].bin` | `0x19951204` / 3 | same | `… 4.1 12/16/97 A` |
| **4.5A** | `Sony PSone BIOS (U)(v4.5)(2000-05-25)[SCPH-101].bin` | `0x19951204` / 3 | same | `… 4.5 05/25/00 A` |

**[V]** = read in this dump's code or data at the address given; **[I]** = inferred. Addresses:
`rom:` 0xBFC0xxxx is the bootstrap running in place; `k:` 0x0000xxxx is the kernel after its
copy to RAM; `sh:` 0x8003xxxx–0x8009xxxx is the shell after its copy. Unless a tag is named, an
address is from 3.0A; §0 says why the `rom:`/`k:` addresses hold for every CEX-3000 image.

Method: Ghidra 12.1 headless (`MIPS:LE:32:default`), one project per ROM, with the ROM at
0xBFC00000 and two extra initialised blocks cut from the same file — `0x10000..0x18000` at
0x00000500 (kernel) and `0x18000..0x7FFF0` at 0x80030000 (shell) — plus uninitialised RAM/IO
blocks; the A0/B0/C0 vectors named with `t1` as the function number (as `tools/ghidra/Ps1Pre.java`
does). The 4.1/4.5 shells are LZ-packed (§2.3) and were unpacked first with a 25-line Python
re-implementation of the in-ROM unpacker. Capstone (`tools/r3000dis.py` with `skipdata`) for the
spots the decompiler mangled. Projects deleted; exports were in the scratchpad.

---

## 0. One kernel, eight shells **[V]**

Bytes `0x00000..0x17FFF` of every CEX-3000 image in the table — bootstrap **and** kernel — are
identical (`md5` of `0x0000..0xFFFF` = `d12f8c88…`, of `0x10000..0x17FFF` = `2f471835…`) from
2.2 (Dec 1995) to the PSone 4.5 (May 2000). Only the shell at `0x18000..0x7FFF0` and the tail
string change. 1.0J (SCPH-1000) is a different build of both halves (§6.4). So:

* every `rom:`/`k:` finding below holds for 2.2A/E/J, 3.0A/E, 4.1A, 4.5A without re-checking;
* the "ROM version" a user sees (`System ROM Version 4.5`) is the **shell's** version; the
  kernel underneath is the 1995-12-04 one, and `GetSystemInfo(0)` returns `0x19951204` on all of
  them (§1.6);
* the two "DTL-H" labelled files carry retail `CEX-3000` kernels and retail tail strings (`A`,
  `E`); nothing in these bytes distinguishes a devkit (§6.5).

---

## 1. Reset → kernel

### 1.1 Reset vector and RAM layout **[V]**

```
rom:BFC00000  write 0x0013243F → 0x1F801010 (BIOS ROM delay/size), 0xB88 → 0x1F801060 (RAM_SIZE)
rom:BFC00070  j BFC00150
rom:BFC00150  memory-control registers 0x1F801000..0x1F80101C, 0x1F801060 = 0xB88
rom:BFC001B0  all 31 GPRs zeroed; 0xFFFE0130 (cache control) ← 0x804 / 0x800 / 0x1E988
rom:BFC00248  I-cache invalidate loops (isolated-cache stores), D-cache (scratchpad) touch
rom:BFC003A8  zero 0xA0009000..0xA000C160 — the bootstrap's own .bss (jmp_buf, config, buffers)
rom:BFC003CC  sp = 0x801FFF00, gp = 0xA0010FF0
rom:BFC003EC  RAM 0x60 ← 2 (MB), 0x64 ← 0, 0x68 ← 0xFF; SPU main/reverb volumes ← 0
rom:BFC00418  j BFC06EC4   (rom_main)
```

`rom:BFC00420` (`kernel_copy`): copies **0x8BF0 bytes from 0xBFC10000 to 0xA0000500** and jumps
to it; `k:0500` is a stub `j k:0598` that zeroes the kernel's .bss `k:7460..k:8920` and returns
to the bootstrap. The real kernel image is 0x8000 bytes (0xBFC10000–0xBFC17FFF); the copy
over-runs 0xBF0 bytes into the start of the shell image, which lands harmlessly at
`k:8500..k:90F0` (partly inside the zeroed .bss). The kernel is linked for 0x00000500 (kuseg
`lui 0` everywhere); the vectors `k:00A0/00B0/00C0` dispatch through tables at **0x200 (A0),
0x874 (B0), 0x674 (C0)** (`k:05C4/05E0/0600`: `lw t0, table[t1*4]; jr t0`). B0/C0 tables are
part of the kernel image; the **A0 table is copied by the bootstrap** from
`rom:BFC04300..BFC04604` (0xC1 words) to 0x200 (`rom:BFC042D0`), then `init_a0_b0_c0_vectors`
(`rom:BFC042A0`) writes the three jump stubs, then the kernel's `C0:1C AdjustA0Table`
(`k:0540`) overwrites entries `A0:00..09` and `A0:3B..3E` (file and std I/O) with kernel
addresses from `k:093C`/`k:0964`.

Where things actually live, by A0 entry **[V `rom:BFC04300` table]**: 0x3F `printf`, 0x41–0x43
`LoadExeHeader/LoadExeFile/DoExecute`, 0x51 `LoadAndExecute`, 0x54/0x71 `CdInit`, 0x56/0x72
`CdRemove`, 0x55/0x70 `_bu_init`, 0x96–0x99 `Add{CDROM,MemCard,DuartTty,DummyTty}Device`,
0x9C/0x9D `SetConf/GetConf`, 0x9F `SetMemSize`, 0xA0 `WarmBoot`, 0xA1
`SystemErrorBootOrDiskFailure`, 0xA2–0xA6 CD IRQ/sector functions, 0xA7–0xAF card callbacks,
0xB2 `ioabort_raw`, 0xB4 `GetSystemInfo` all point at `rom:BFC0xxxx` — the CD-ROM filesystem,
the memory-card driver, the EXE loader and the boot logic **run from ROM**, not from the RAM
kernel. Names are psx-spx's for these table slots [I]; the addresses and what the code does
are [V]. Every `SystemError*` slot (0x3A, 0x40, 0x4F, 0x50, 0x52, 0x53, 0x9A, 0x9B, 0xA1) points
at a thunk that calls itself through the table (`rom:BFC0D8D0..D950`: `li t1,N; j 0xA0`) —
**a system error is an infinite loop**, nothing is printed, the only trace is the POST byte (§1.3).

### 1.2 The boot main `rom:BFC067E8` (reached from `rom_main BFC06EC4` → `BFC06784`) **[V]**

`BFC06784` builds the two file names `"cdrom:" + "SYSTEM.CNF;1"` and `"cdrom:" + "PSX.EXE;1"`
(strings `rom:BFC0E1A8/E130/E1B0/E140`) on the stack and passes them to the main. In order:

| POST | step | address / detail |
|---|---|---|
| F, E | `rom_main`: expansion-ROM **pre-boot** hook (§6.1), `ttyflag(0xA000B9B0) = 0` | `BFC06EC4` |
| 1 | SR &= ~0x401 (interrupts off), SPU volumes 0 | `BFC067E8` |
| 2 | `kernel_copy` (§1.1) | `BFC00420` |
| 3 | A0 table → 0x200, vectors, `C0:1C AdjustA0Table`, `C0:07 InstallExceptionHandlers`, `B0:18 SetDefaultExitFromException` | `BFC042D0/42A0/DB10/DB20/D9A0` |
| 4 | I_STAT/I_MASK = 0; `C0:12 InstallDevices(ttyflag=0)` — the **kernel** writes POST 1..6 inside this call (`k:4360`) | `BFC0DB30` → `k:27C0` |
| 5 | `printf("\nPS-X Realtime Kernel Ver.2.5\nCopyright 1993,1994 …")` | `rom:BFC0DFB0` |
| 6 | default config `{TCB 4, EVENT 0x10, STACK 0x801FFF00}` from `rom:BFC0E14C` → `0xA000B940`; "KERNEL SETUP!": `C0:08 SysInitMemory(0xA000E000, 0x2000)`, ExCB ×4, `C0:01`, `C0:0C InitDefInt(3)`, EvCB alloc (0x1C bytes each, "Configuration : EvCB 0x%02x"), TCB alloc (0xC0 each, "TCB 0x%02x"), `C0:00 EnqueueTimerAndVblankIrqs(1)` | `BFC04610/04678/0472C` |
| — | `setjmp(0xA000B980)`: non-zero → `SystemError('B', 0x385)` | `BFC02240` |
| 7 | **shell**: copy 0x67FF0 bytes `0xBFC18000 → 0x80030000`, `FlushCache`, `jalr 0x80030000` (§2) | `BFC06FF0` |
| 8 | I_STAT/I_MASK = 0; `CdInit` (§3.1): CD events, wait for `A0:95`, 50 000-iteration delay, ISO9660 init; expansion-ROM **post-boot** hook (§6.1); `printf` "BOOTSTRAP LOADER Type C Ver 2.1   03-JUL-1994" | `BFC073A0`, `BFC070AC/7148`, `rom:BFC0E0C4` |
| 9 | `open("cdrom:SYSTEM.CNF;1", 1)` → §4 | `B0:32` via `BFC0D890` |
| — | second "KERNEL SETUP!" with the parsed config (`BFC06F28`), `printf("boot file     : %s")` | |
| — | **wipe user RAM** `0xA0010000 .. sp` (`BFC0D850`), `LoadExeFile(bootfile, 0xA000B870)`; 0 → `SystemError('B', 0x38A)` | `BFC03A18` |
| — | `printf("EXEC:PC0(%08x)  T_ADDR(%08x)  T_SIZE(%08x)")`, `s_addr = STACK, s_size = 0`, `EnterCriticalSection` | |
| — | **pre-exec disc check** (§3.4), `DoExecute(hdr, 1, 0)` | `BFC0D570` → `BFC03CF0` |
| F | "End of Main" → `SystemError('B', 0x38C)`: a game that returns hangs the console | |

Every `setjmp` re-arms the same `jmp_buf` with a new code; `A0:B2 ioabort_raw` (`rom:BFC06700`)
is `longjmp(0xA000B980, code)`, so any kernel I/O abort during boot lands in
`SystemErrorBootOrDiskFailure('B', <code of the last stage armed>)`. Codes armed in order:
0x385 (before the shell), 0x399 (after CD init), 0x386 (after the banner), 0x387 (before
`open`), 0x391 (SYSTEM.CNF absent branch) / 0x38F (opened) / 0x390 (read), 0x388 (before setup
2), 0x389 (before load), 0x38B (before exec); direct calls: 0x38A (load failed), 0x38C (returned),
0x38D (shell, logo mismatch §3.3). `'B'` = 0x42 is the first argument; the shell's copy of the
pre-exec check uses `'D'` = 0x44.

### 1.3 POST byte 0x1F802041 **[V]**

Written by `rom:BFC01A60` (`sb a0, 0x2041(0x1F800000)`), by the kernel's `k:4360` (1..6 during
`InstallDevices` `k:27C0`/`k:2870`: 1 at entry, 3 before `AddDevice` of the kernel's built-in
device `k:6DC0`, 4 after it, then the dummy/DUART tty (`k:6AC0` = `A0:99` / `k:6AD0` = `A0:98`),
5, `C0:13 FlushStdInOutPut`, 6, and 2 before `A0:96 AddCDROMDevice` + `A0:97 AddMemCardDevice`
(`k:6AA0/6AB0`)) and by the shell's `sh:80030798`
(0 while reading the system area, 1, 2, 5 just before returning to the ROM, 7 before the logo
error). The full sequence a tool can predict on a good boot is therefore
`F E 1 2 3 [1 3 4 5 6 2] 4 5 6 7 [0 1 2 5] 8 9` and `F` on any error — an emulator exposing the
register gives a stage trace for free.

### 1.4 Version words and strings **[V]**

* `0xBFC00100` u32 BCD kernel date (`0x19951204`; 1.0J `0x19940922`), `0xBFC00104` u32 = 3 in
  all eight images, `0xBFC00108` "Sony Computer Entertainment Inc.", `0xBFC0012C` the maker
  string (`CEX-3000/1001/1002 by K.S.`; 1.0J `CEX-1000 KT-3  by S.O.`).
* Kernel: `"PS-X Realtime Kernel Ver.2.5"` (`rom:BFC0DFB1`, printed at POST 5), `"BOOTSTRAP
  LOADER Type C Ver 2.1   03-JUL-1994"` (`rom:BFC0E0C5`, printed at POST 8), `"PS-X ROM monitor
  Ver.2.3"` (`rom:BFC0E00D`) — **referenced by nothing**, a leftover (its command set survives in
  1.0J, §6.4). `"PS-X Control PAD Driver  Ver 3.0"` at `rom:BFC16D34` is in the kernel image.
* Shell: the tail string `0xBFC7FF32` "System ROM Version X.X mm/dd/yy R" + NUL + "Copyright
  1993-19xx (C) Sony …" is **read from ROM by the shell** (`sh:800307A8` → `local = 0xBFC7FF32`),
  not from its copy; the shell also carries a hard-coded "System ROM Version 1.0" fallback
  (`sh:8005AEA4`) printed when the tail string is empty — 1.0J has no tail string and prints that.
* The shell takes **its region from the last character** of the tail string: `'E'` → PAL
  (`sh:800307A8` returns `str[len-1] == 'E'`, used by `sh:80040420` for the video mode and by
  `sh:8003F1C0` for the 512-line screen offsets); 4.5 also tests `'A'` (§3.3).

### 1.5 TTY: what goes to the serial port **[V]**

Nothing, on a retail ROM. `ttyflag` (`0xA000B9B0`) is written 0 by `rom_main` and by the reset
code and never set; `InstallDevices(0)` → `k:2870` installs `A0:99 AddDummyTtyDevice`
(descriptor `rom:BFC0E350`, name `tty`, desc `CONSOLE`, flags 1) instead of `A0:98
AddDuartTtyDevice` (`rom:BFC0E59C`, flags 3). The DUART driver is fully present —
`rom:BFC0C890` initialises a Signetics 2681 at **0x1F802020** (`PTR rom:BFC0E530`), two channels
named `s2681_0`/`s2681_1`, `"%s baud rate set to %d"` (`rom:BFC0E568`) — i.e. the DTL-H2000
development-board UART on the expansion bus, **not** the console's own SIO at 0x1F801050, which
no bootstrap or kernel function touches (register census of all ROM/kernel functions: only
0x1F8010xx DMA/IRQ/RAM-size, 0x1F801D8x SPU, 0x1F80181x GPU, 0x1F802041 POST). So every
`printf` in this note goes to the dummy device. A tool can still **replay the TTY log** from the
format strings in boot order: the two banners, "KERNEL SETUP!", "Configuration : EvCB 0x%02x",
"TCB 0x%02x", "setup file    : %s", "%s\t%08x" per numeric key, "BOOT =\t%s", "argument =\t%s",
"boot file     : %s", "EXEC:PC0(…)", "boot address  : %08x %08x\nExecute !", "S_ADDR(…)", "End
of Main"; the shell adds "System Controller ROM Version %02x/%02x/%02x %02x", "!!!WARNING!!! :
Not PS Disk", "Audio Disk !!", "Shell Opened !!", "CdGetToc() is Cancelled !!".

### 1.6 `A0:B4 GetSystemInfo(index)` `rom:BFC0D4F0` **[V]**

Jump table `rom:BFC0DCB0` (15 entries): 0 → u32 `0xBFC00100` (date), 1 → `0xBFC00104` (3),
2 → pointer `0xBFC0012C` (maker string), 5 → `RAM[0x60] << 10` (KB: 2048), every other index
→ u16 table `rom:BFC0E600[index]` = {7: 0x400, 9: 0x200, 12/13/14: 1, else 0}. Absent from the
1.0J table (its A0 table ends at 0xAE, §6.4).

---

## 2. Kernel → shell → kernel

### 2.1 Hand-over **[V]**

`rom:BFC06FF0`: `memcpy(0x80030000, 0xBFC18000, 0x67FF0)`, `FlushCache`, `jalr 0x80030000`. The
shell receives **a0 = 7** (the POST stage left in the register by `rom:BFC01A60`; `shell_entry`
takes no parameter) and nothing else — no argv, no region, no pointer. It returns with a plain
`jr ra` after the boot decision (§3.2) and the bootstrap carries on at POST 8. The ROM keeps no
result from the shell: it simply trusts that the shell only returns for a disc the drive has
identified as licensed. Everything the bootstrap then needs (`SYSTEM.CNF`, the EXE) it reads
itself through its own ISO9660 driver (§3.1), re-initialised at POST 8 because the shell used the
drive directly in the meantime.

### 2.2 What the shell does with the kernel **[V 3.0A]**

`sh:80030000`: `A0:72 CdRemove` (`rom:BFC072B8` — closes the five CD events `0xA000B9B8..C8`
and `A0:A3 DequeueCdIntr`; the `cdrom` device stays registered), `InitHeap(0x80140000,
0x20000)`, the region/version parse, GPU/video setup, its own CD IRQ handlers via `C0:02
SysEnqIntRP` (`sh:80035C04`: two handler records, six `OpenEvent(0xF0000003, 0x10/0x20/0x40/
0x80/0x8000/0x200, 0x2000)`), then the decision of §3.2. Kernel services reachable from the shell
on the **boot path** (transitive closure over the 307 functions `shell_entry` can reach before
returning): `A0:17 strcmp, 1B strlen, 2F rand, 33 malloc, 34 free, 39 InitHeap, 3F printf, 72
CdRemove, A1 SystemError`, `B0:08/09/0B/0C` events, `C0:02/03`, `syscall 1/2`. **No pad call,
no card call, no file open**: `InitPad/StartPad` (`sh:8003DEBC`) and the `bu00:` opens
(`sh:8003AE7C/8003BD9C`) belong to the menu that runs only after the decision fails.

### 2.3 The 4.x shells are packed **[V 4.1A, 4.5A]**

`sh:80030000` in 4.1/4.5 is a 0x4C-byte stub: copy the stream at `0x8003004C` (ROM 0x1804C;
0x30DB4 bytes in 4.1, 0x345C8 in 4.5) to **0x80190000**, `FlushCache`, jump to the unpacker
inside it (4.5: `0x801C44B8`), which writes the real shell back to 0x80030000 and re-enters it.
Format: 3-byte LE output length, then 32-bit **big-endian** control words; bits 1–0 of each word
pick the split (offset bits = 14 − n, length bits = 2 + n), the top 30 bits are read MSB first:
0 = literal byte, 1 = 16-bit BE pair `(len = (v >> (14−n)) + 3, dist = (v & (0x3FFF >> n)) + 1)`.
Unpacked sizes: 4.1 0x5AD40, 4.5 0x60990. Every address below for 4.5 refers to the unpacked
image at 0x80030000. (Byte-for-byte reimplementable from the user's dump; the tool must unpack
before it can read the 4.x licence strings.)

---

## 3. The disc decision

### 3.1 The bootstrap's ISO9660 driver (POST 8) **[V]**

`CdInit` `rom:BFC073A0` = `rom:BFC07330` (open three CD events `0xF0000003/0x10,0x20,0x40` at
`BFC071A0`, `A0:A2 EnqueueCdIntr`, spin until `A0:95 CdInitSubFunc` ≠ 0) + a 50 000-iteration
delay + `rom:BFC07410`:
1. `CdReadSector(1, LBA 16, 0xA000B070)`; byte 1.. must be `"CD001"` (`rom:BFC0E2E0`).
2. From the PVD: path-table size (+0x84 → `0xA0009D70`), **path-table L LBA (+0x8C)**, root
   extent, volume space size (+0x50 → `0xA0009D7C`).
3. Reads the path table (one sector) and **XORs its words into `0xA0009D7C`**: this is the
   kernel's *disc identity*; every open file keeps a copy (`fcb+4`) and `read` fails with error
   0x10 if the live value differs (`rom:BFC07A04`) — the swap detection on the kernel side.
4. Parses up to **44 directories** (0x2C) into `0xA00095B0` (0x2C bytes each).
Return value **ignored by the main**; a failure surfaces later as `open()` −1.

`dev_cd_open` `rom:BFC078A4`: if the drive status (`A0:A6`, `rom:BFC07F28`) has bit 4 (lid was
opened / media changed) the whole init re-runs; the name is **upper-cased**, `";1"` appended when
no `';'` is present (`rom:BFC0E2E8`), looked up by `rom:BFC083CC`: leading `\` = absolute else
cwd (`0xA0009D80`) + `\`; up to 8 path components; a directory is loaded by `rom:BFC07700`,
which reads **one sector only** and keeps at most **40 entries** (0x18 bytes each at
`0xA00091F0..95B0`); matching (`rom:BFC08020`) is exact except `?`. So the boot file and
`SYSTEM.CNF` must sit among the first 40 records of the first 2048 bytes of the root directory,
spelled upper-case with a `;1` version — a tool can check a disc image against exactly that.
`dev_cd_read` `rom:BFC07A04`: size must be a multiple of 0x800 and the file position
sector-aligned (else error 0x16 and −1), reads `size >> 11` sectors from `LBA + pos >> 11`,
clamps to the file size.

### 3.2 The shell's decision (3.0A `sh:80030000`) **[V]**

The shell drives the CD controller itself (register pointers `sh:80079C50..5C` = 0x1F801800..03)
through a step machine (`sh:80036400`, steps → commands in `sh:80036200`). In `shell_entry`:

1. `sh:80035F74`: **`Test 0x19` sub 0x20** (controller BIOS date/version, `sh:80039380`),
   printed as "System Controller ROM Version yy/mm/dd v"; `DAT_80079E30 = (date ≥ 95/07/06)`,
   also stored as `0xA000DFFC` for the bootstrap (§3.4).
2. `sh:800359D4(1)`: **`Getstat 0x01`** (step 5, up to 6 ticks; status bit 4 "lid open" → retry
   once, else `e38 = 0`), then **`GetID 0x1A`** (step 0x16, up to 0x2D0 = 720 ticks of
   `sh:80059CB0`). The 8-byte second response is stored at `0x8007A140` (`sh:80058B98`):
   `[0] stat, [1] flags, [2] type, [3] atip, [4..7] "SCEx"`. Decision, verbatim:
   * INT2 (no error flag) **and** `flags & 0x80 == 0` → `e3C = 1` *"PlayStation disc"*;
   * `stat & 0x08` (ID error): `flags & 0x40` → `e34 = 1` *"no disc"*; `flags & 0x80` →
     `e34 = 0, e3C = 0` *"unlicensed"*; `flags & 0x10` → `e40 = 1` *"audio CD"*.
3. If `e3C`: when the controller is new enough, `sh:80057020` sends **`ReadTOC 0x1E`** and fails
   on `stat & 0x1D` (error, seek error, ID error, lid); a second `GetID` round; then
   `sh:80030694(1)` = the licence screen (§3.3). Returns ≥ 0 → `sh:80030798(5)` → **return to the
   ROM**.
4. Otherwise the shell falls into its menu (`sh:80030264`): CD player when `e40`, the main menu
   when unlicensed/no disc (`DAT_800EA7C0 = 4`), and from the menu loop (`sh:800309A0`) a disc
   that later passes the same `GetID` test is booted through `sh:80030694(0)` → `e68 = 1` →
   `shell_entry` returns.

So on 3.0A **the only gate between "disc in" and "SYSTEM.CNF read" is bit 7 of the GetID flags
byte** — the controller's own licensed/unlicensed verdict. The four SCEx letters are stored
(`sh:80079F1C`, `' '` for NUL) and only *drawn* (`sh:8003F414`: text object at (0x140, 400),
off the visible 240/256-line area; `"NotPS"` is drawn in the same place by the menu path) —
**never compared with the console's region**. No "Please insert PlayStation CD-ROM" string
exists in any of the eight images (`strings` and xrefs); the menu's icon state 4 is what an
unlicensed or data disc produces.

### 3.3 The system area (sectors 4–15) and the licence check — by shell **[V each]**

`sh:8003EC40` (3.0A) `ReadSystemArea`: POST 0; buffer `0xA0010000`; `sh:8003EF28(count, lba)` =
`Setloc 0x02` (MSF of lba+150) + `Setmode 0x0E` (0x80) + `ReadN 0x06`/`ReadS 0x1B`
(`sh:80057608`, `sh:80059488`); 1 sector at **LBA 4**; the first line (to `'\n'`/NUL, max 72 bytes) copied to `0x800ED0E8`; then **7 sectors
from LBA 5** (0x3800 bytes, the logo model); then — gated on a flag — a byte compare of the
**first 0x3278 bytes** of the sectors-5.. data with a reference blob in the shell, mismatch →
POST 7 and `SystemError('B', 0x38D)` (hang); then a third `GetID` storing the SCEx letters
and printing the diagnostics (`!!!WARNING!!! : Not PS Disk` when `stat & 0x80 && flags & 0x80`,
`Audio Disk !!`, `Shell Opened !!`). Back in `sh:80030694`, the same flag gates
`strcmp(line, "          Licensed  by          Sony Computer Entertainment Inc.")` (64 chars,
`sh:8005AE60`); mismatch → −1 → **not booted** (no error, the menu). The flag:

| shell | flag | text compared | logo blob (0x3278 bytes, md5 `8b27d11c…`) |
|---|---|---|---|
| 1.0J | *none — unconditional* (`sh:800307A0`, `sh:8003E530`) | `…Inc.` | at `sh:80077744`, compared; read errors also → 0x38D |
| 2.2J | constant **1** (`sh:80030058`, `0x8007CBFC`) | `…Inc.` | at `sh:800599B0` (ROM 0x419B0), compared |
| 2.2A | constant 0 (`sh:80030054`) | — | absent |
| 2.2E | constant 0 (`sh:80030058`, `0x800777CC`) | — | absent |
| 3.0A | constant 0 (`sh:80030054`, `0x80079E2C`) | — | absent (the compare reads `sh:80079E48`, a variables area — dead code) |
| 3.0E | constant 0 (`sh:80030000`, `0x8007797C`) | — | absent |
| 4.1A | constant 0 (`sh:8003005C`, `0x8008AD48`) | — | absent |
| **4.5A** | **= letter ≠ 'A'** (`sh:80030A6C`: set 1, cleared when the tail letter is `A`; `'E'` selects the Europe strings) | `E`: `…Entertainment Euro pe   ` (70) or `…Entertainment(Europe)` (67); else `…Inc.` (64) (`sh:8006C5E8/C630/C674`) | at `sh:80069360`, compared when flag = 1 |

Consequences a tool can state: on every **A** console in this set and on 2.2E/3.0E the disc's
licence text and logo are *displayed verbatim and never verified* — only the drive's GetID
verdict counts; a Japanese 1.0/2.2 console insists on the exact `Inc.` line and the exact
12 920-byte logo prefix (a Europe/America licence file boots only if its logo bytes are the same
— the blob is identical in 1.0J, 2.2J, 4.5A and in the PS2's `rom0:LOGO`, so the logo is one
constant and only the text line differs by region); a PSone 4.5 behaves like the PS2's PS1 shell
(`notes/ps1_boot.md` §5): off for `A`, Europe strings for `E`, `Inc.` otherwise — but this
particular dump is `A`, so **on it the check is off**. `ps2kit::disc::Ps1Verdict::judge`
already encodes the 4.5/PS2 rule; it needs a *per-shell* mode: `Unconditional` (1.0J),
`Always` (2.2J), `Never` (2.2A/E, 3.0A/E, 4.1A), `ByLetter` (4.5A, PS2 LOGO), decided from the
user's dump by reading the flag store in `shell_entry` or, simpler, by the presence of the
Europe strings + the `'A'` test.

### 3.4 The bootstrap's pre-exec disc check `rom:BFC0D570` **[V]**

Right before `DoExecute`, if `0xA000DFFC` ≠ 0 (set by the shell when the controller's BIOS date
is ≥ 95/07/06): `ExitCriticalSection`, then `rom:BFC0D72C` sends **`ReadTOC 0x1E`** and
`rom:BFC0D7BC` sends **`GetID 0x1A`** by banging 0x1F801800–03 directly (index 0, command
register, `Reset`-style parameter flush 0x1F/0x18 on 0x1F801803), waits for an INT, and takes the
first response byte as status; `status & 0x1D` ≠ 0 or INT5 → `SystemErrorBootOrDiskFailure('D',
0x38B)` → hang. With an unlicensed disc GetID answers INT5 with stat 0x0A (ID error bit 3) → caught.
This is the kernel-side half of the anti-swap pair (shell: the GetID flags; ROM: status bits after
the shell has already run), and it exists in every CEX-3000 image; **1.0J has neither the check
nor the flag nor the user-RAM wipe** (`rom:BFC06798` goes `EnterCriticalSection → DoExecute`).

### 3.5 Non-PlayStation media, summary **[V 3.0A, I for the controller's answers]**

| drive says (GetID) | shell | bootstrap |
|---|---|---|
| licensed (INT2, flags.7 = 0) | licence screen, return | `SYSTEM.CNF` → EXE; pre-exec `ReadTOC/GetID` must still pass |
| unlicensed (INT5, stat.3, flags.7) | main menu, state 4, polls every 0x3C ticks (`sh:800309A0`) | never reached |
| audio (flags.4) | CD player (`e40`) | never reached |
| no disc (flags.6) | main menu (`e34`), polls | never reached |
| lid open (stat.4) | `Getstat` retried, `e38 = 1` | — |
| licensed but `SYSTEM.CNF` missing | — | `cdrom:PSX.EXE;1` with defaults TCB 4 / EVENT 0x10 / STACK 0x801FFF00 |
| `PSX.EXE` also missing / not in the first root sector | — | `LoadExeFile` = 0 → `'B' 0x38A` hang, POST F |

---

## 4. `SYSTEM.CNF` and the PS-X EXE

### 4.1 Grammar (`rom:BFC008A0`, `BFC00944`, `BFC00B7C`) **[V]**

* The file is `open`ed and **up to 0x800 bytes** read into `0xA000B070`, NUL-terminated at the
  byte count; 0 bytes read = treated as absent (defaults + `PSX.EXE`).
* The three config words `0xA000B940/44/48` are **zeroed first**, then each of `TCB`, `EVENT`,
  `STACK` (`rom:BFC0DD10/14/1C`) is searched **line by line as a prefix** — `strncmp(line, key,
  strlen(key))` at each line start (case-sensitive; a NUL ends the text), and the **first** line
  that starts with the key decides. **Correction (2026-10-10, re-read at `rom:BFC00A64` /
  `rom:BFC00C38` [V]):** the character after the key must be a space or `=`; otherwise the routine
  returns with the field unset and never looks further — so `TCBX = 5` does *not* set TCB, `BOOT2
  = …` (a PS2 disc) does *not* satisfy `BOOT`, and either line placed first hides a proper later
  line. After the key: skip ctype-space (table `rom:BFC0DDB1`, bit 3:
  TAB, LF, VT, FF, CR, space), require `=`, skip spaces, then **hex digits with no prefix** (ctype
  bits 0x44) — `TCB = 10` means 16; missing key ⇒ **0** (not the ROM default). Printed
  `"%s\t%08x"`.
* `BOOT` (`rom:BFC0DD24`): same positioning; the value runs to `'\n'` (which is replaced by NUL —
  a CR before it is harmless because the file name stops at the first space-class byte); the
  **file name** is the first token (`strcpy` into `0xA000B8B0`); whatever follows the token is
  `strncpy`ed into **RAM 0x00000180, 0x80 bytes max** and printed as `"argument =\t%s"` — a
  second word on the BOOT line survives into the game's address space at 0x180, although
  `DoExecute` is called with `argc 1, argv 0`. 0x180 is also zeroed when `SYSTEM.CNF` is absent.
* Nothing else is parsed: no `VER`, no `BOOT2`, no comments, no quoting, no `0x`.
* Second "KERNEL SETUP!" (`rom:BFC06F28`) rebuilds the heap, EvCBs and TCBs with the parsed
  counts (0 ⇒ zero-size tables) and re-opens the CD events (`BFC071A0`).

### 4.2 The executable (`rom:BFC03A18 LoadExeFile`, `BFC03C90`, `BFC03CF0 DoExecute`) **[V]**

* `open(name, 1)`; `read(fd, 0xA000B070, 0x800)`; **the only validation is that 0x800 bytes were
  read** — the `"PS-X EXE"` magic, the `"Sony Computer Entertainment Inc. for …"` region text and
  the checksum field are **never looked at**. Bytes 0x10..0x4B of the sector (pc0, gp, t_addr,
  t_size, d_addr, d_size, b_addr, b_size, s_addr, s_size, sp/fp/gp/ra/base) are copied to the
  header struct `0xA000B870`.
* `read(fd, t_addr, t_size)` — **return value ignored**; the CD device requires `t_size % 0x800
  == 0` (else EINVAL and nothing is loaded) and reads straight to `t_addr` with no range check;
  `FlushCache`.
* `DoExecute`: saves s0/ra/sp/fp/gp into the header (+0x28..+0x38), zero-fills `b_addr..b_size`
  (header +0x18/+0x1C), `sp = fp = s_addr + s_size` **only if `s_addr ≠ 0`** (the main sets
  `s_addr = STACK, s_size = 0`, so `STACK = 0` or no `STACK` line leaves the kernel's own stack
  0x801FFF00 in place — not a crash), `gp = header.gp`, `jalr pc0` with `a0 = argc, a1 = argv`.
* Malformed file, outcomes: shorter than 2048 bytes → `'B' 0x38A` hang; ≥ 2048 bytes of anything
  → executed at whatever `pc0` says; `t_size` not a sector multiple → jump into unloaded memory.
* `A0:51 LoadAndExecute(name, sp, fp)` (`rom:BFC03AA4`) is the chain-loader games may call: it
  upper-cases and `;1`-completes the name, runs it, then **re-loads and re-runs the boot file**
  (`"Execute the boot file %s."`), and spins for ever on `"No boot file !"`.
* `A0:A0 WarmBoot` (`rom:BFC06CA4`): the main again minus POST codes, shell, expansion hooks and
  `SYSTEM.CNF` — it re-executes the boot file name still at `0xA000B8B0` (the bootstrap's .bss is
  only cleared by the reset code) with the current config; `A0:9C/9D SetConf/GetConf`
  (`rom:BFC06750/06728`) read and write `{TCB, EVENT, STACK}` at `0xA000B940`.

---

## 5. Memory cards at boot **[V]**

Not touched. `AddMemCardDevice` (`A0:97`, `rom:BFC0C1FC`) only registers the `bu` descriptor
(`rom:BFC0E3E4`: name `bu`, blocksize 0x80, desc `MEMORY CARD`, **init = the return-0 stub
`rom:BFC06FDC`**); the SIO0 traffic lives in `_bu_init` (`rom:BFC09914`) and
`InitCard/StartCard` (`B0:4A/4B`, `k:5DA8/4C70`), which only the device `open` path and games
call. The function that would do all three at once (`rom:BFC0C2E8`) is unreferenced dead code.
The shell's boot path reaches no card service (§2.2); its `bu00:`/`bu10:` opens, `_card_info`,
`allow_new_card`, `__tmp_file` belong to the memory-card manager screen. No file is looked for on
a card before a disc boots, in any of the eight shells (string census: the only `bu` paths are
`bu00:`, `bu10:`, `bu00:s002` (kernel test leftovers), `bu00:__tmp_file`, `bu10:__tmp_file`).

---

## 6. Hidden and undocumented

### 6.1 Expansion-ROM hooks `rom:BFC0703C/070AC/0711C/07148` **[V]**

Two byte-compares of the ROM string `"Licensed by Sony Computer Entertainment Inc."`
(`rom:BFC0E288`, 44 chars + NUL):
* at **0x1F000084**: match → `rom_main` calls `*(u32*)0x1F000080` **before anything else** (before
  the kernel copy, POST F→E);
* at **0x1F000004**: match → after the shell and `CdInit` (POST 8) the bootstrap prints
  `"PIO SHELL for PlayStation(tm)"` and the string, then calls `*(u32*)0x1F000000`.
Both are plain `jalr`; if the hook returns, boot continues. This is the whole cheat-cartridge /
Xplorer / "PIO shell" mechanism, and it is **not gated by anything** (no DIP, no button, no
region). An expansion-port device inspector can test the two strings and report which hook fires.

### 6.2 Boot-relevant kernel functions nobody documents in the shell **[V]**

`A0:9F SetMemSize(MB)` `rom:BFC06680`: accepts only 2 or 8, programs `0x1F801060` bits 9–11
(`0x300` for 8 MB) and RAM `0x60`, prints `"Change effective memory : %d MBytes"` / `"Effective
memory must be 2/8 MBytes"` (1.0J: `2/8/16`, `rom:BFC0DD48`); `A0:A0 WarmBoot`; `A0:B4
GetSystemInfo`; `A0:A1` = hang. Everything else in the A0 table is library code or device drivers.

### 6.3 Buttons, test modes **[V, negative]**

No pad read on the boot path (§2.2); the bootstrap has no pad code at all; no DIP-switch read
(0x1F802000 area is only written for the POST byte). There is no button combination that alters
the boot of these ROMs. The only run-time switches are hardware: the expansion ROM strings
(§6.1), the controller's BIOS date (§3.4), the GetID verdict (§3.2).

### 6.4 1.0J (SCPH-1000): what the oldest kernel lacks and leaks **[V]**

* Bootstrap `rom:BFC06798` = the 3.0 main minus: the default-config copy (literal `0x10` events,
  `4` TCBs for the first setup), the user-RAM wipe (`BFC0D850`), the `0xA000DFFC` pre-exec
  `ReadTOC/GetID` check. A0 table at `rom:BFC042F0` has **0xAF entries** (0x00..0xAE): no
  `card_write_test`, `ioabort_raw`, `GetSystemInfo` slots.
* Kernel date `0x19940922`, maker `CEX-1000 KT-3  by S.O.`, same `Ver.2.5` and `Type C Ver 2.1`
  strings, same tables otherwise.
* Shell: no tail version string, no PAL/NTSC letter logic, licence text **and** logo compared
  unconditionally with debug prints `"wiz_text : %s"`, `"Strings Data is O.K."`, `"Polygon Data
  is O.K."`, `"max() : cdRom() = %02x"`, and a sector-read error during the system-area read is
  itself fatal (`0x38D`).
* **The ROM debug monitor's text survives in the kernel image's .bss shadow**
  `0xBFC16F50..0xBFC177xx` (kernel `k:7450..`, zeroed at `k:0598` on every boot so never used):
  a complete command table — `exec/load/read/sector <file>`, `dc` (display kernel configuration),
  `sc <ev> <tcb> <sp>` (change it), `mem <Mbyte>`, `sd/ad/dd` device-driver management,
  `showcom/addcom/delcom`, `mem1/mem2` RAM tests, `pad1/pad2`, `cb` colour bar, `led <pat>`, `dip`
  (DIP switches), `fc`, `dbc/dhc/dwc`, `stop`, Control-key help, `"EVENT = %08x / TCB = %08x /
  STACK = %08x"`, and the expansion-ROM wording of the day: `"Sony Computer Entertainment Inc.
  original"` / `"Lisenced by Sony Computer Entertainment Inc."` [sic] → `"original/licensed/no
  mentenance code!"` and `"…shell code!"`. This is the DTL-H2000 board monitor the orphaned
  `"PS-X ROM monitor Ver.2.3"` string refers to. The CEX-3000 kernel's shadow is clean (81
  non-zero bytes, `AAAAAA`/`BBBBBB` fillers at `rom:BFC16F12/F32`).

### 6.5 "DTL-H3000 / DTL-H3002" images **[V for the bytes, I for the hardware]**

Both are retail kernels (§0) with retail tail strings (`2.2 12/04/95 A` / `E`), retail shells
(flag constant 0, no logo blob), and no string, table entry or code path that mentions a devkit,
DIP switch, or `host:`/serial loading (the DUART driver is in *every* ROM, §1.5). The Debugging
Station's permissiveness toward CD-R and other-region discs is therefore not in the ROM the user
dumps; it has to be in the CD controller firmware (the sub-CPU that answers GetID). A tool
inspecting these files can only say "this is the SCPH-1001/1002 retail program".

---

## 7. What is software and what is not **[V/I as marked]**

| gate | where | visible in the ROM | tool can reproduce |
|---|---|---|---|
| wobble-groove SCEx read, licensed/unlicensed verdict, SCEx letters | CD controller sub-CPU; reported through `GetID` flags/stat and the 4 letters | **no** — the ROM only consumes the answer [V that nothing in the ROM reads the groove; I for the mechanism] | no; it can only *state the input it would need* ("drive says licensed, letters SCEA") |
| region lock | the controller refuses/accepts the groove (letters never compared by the shell, §3.2) | **no** | no |
| GetID re-check after the shell (`ReadTOC`+`GetID` status bits) | `rom:BFC0D570`, gated on controller date ≥ 95/07/06 | yes | yes, given the drive's answer |
| licence text (sector 4) vs ROM string | shell, per-version flag (§3.3) | yes | **yes, fully** from the dump + disc image |
| logo bytes (sectors 5–11, 0x3278) vs ROM blob | shell, same flag | yes | **yes, fully** (blob read from the user's dump) |
| region letter → PAL/NTSC and which strings | tail string `0xBFC7FF32` | yes | yes |
| `SYSTEM.CNF` grammar, `PSX.EXE` fallback, EXE load | bootstrap | yes | **yes, fully** |
| expansion-ROM hooks | bootstrap | yes | yes (needs the cartridge image) |
| memory card | — | nothing at boot | trivially |
| controller BIOS date | `Test 0x19/0x20` | consumed only | input the tool must assume |

What the drive-side firmware says is the one thing a ROM-plus-disc-image tool cannot derive; the
honest output is a line such as *"assuming the drive reports this disc as licensed (SCEA)"* with
everything after it computed.

---

## 8. Hand-off facts, in `HandoffStep` style (3.0A; bracketed notes for other shells)

1. **Reset** `0xBFC00000` → `0xBFC00150` → `rom_main 0xBFC06EC4`: POST F; expansion ROM at
   `0x1F000084` compared with `"Licensed by Sony Computer Entertainment Inc."` (`rom:BFC0E288`)
   → *no cartridge* / *pre-boot hook at `*0x1F000080` fires*; POST E.
2. **Kernel copy**: 0x8BF0 bytes `0xBFC10000 → 0xA0000500`, .bss `0x7460..0x8920` cleared;
   A0 table `0xBFC04300` → `0x200`; B0 `0x874`, C0 `0x674` in the image; `AdjustA0Table`. POST 1–3.
3. **Devices**: `tty` = dummy (`ttyflag 0xA000B9B0 = 0`), `cdrom` (`rom:BFC0E2F0`), `bu`
   (`rom:BFC0E3E4`, init stub); POST 4; TTY banner `"PS-X Realtime Kernel Ver.2.5"` (to nowhere).
4. **Kernel setup 1** with ROM defaults `0xBFC0E14C` = TCB 4, EVENT 0x10, STACK 0x801FFF00;
   heap `0xA000E000+0x2000`; POST 6; `setjmp(0xA000B980)` armed with code 0x385.
5. **Shell**: 0x67FF0 bytes `0xBFC18000 → 0x80030000`, entry `0x80030000`, a0 = 7 [4.x: stub
   unpacks `0x3xxxx` bytes to 0x80030000 via 0x80190000]; POST 7.
6. **Shell region**: last char of `0xBFC7FF32` "System ROM Version 3.0 11/18/96 A" → `'A'` →
   NTSC [`'E'` → PAL]; licence-check flag `0x80079E2C = 0` [1.0J/2.2J: on; 4.5: `letter ≠ 'A'`].
7. **Controller version**: `Test 0x19/0x20` → "System Controller ROM Version yy/mm/dd v"; date ≥
   95/07/06 → `0xA000DFFC = 1` (enables step 15) and `ReadTOC` in step 9.
8. **Disc identification**: `Getstat`, then `GetID 0x1A` → response at `0x8007A140`: *licensed*
   iff INT2 and `flags.7 = 0`; else *unlicensed* (`flags.7`), *no disc* (`flags.6`), *audio*
   (`flags.4`) → menu / CD player; polled every 0x3C ticks from the menu.
9. **Re-check**: `ReadTOC 0x1E` status `& 0x1D == 0` (if the controller is new enough), `GetID` again.
10. **System area**: `ReadN` LBA 4 → first line (≤ 72 chars) to `0x800ED0E8`; `ReadN` 7 sectors
    from LBA 5 to `0xA0010000`; *[flag on: first 0x3278 bytes compared with the shell blob at
    `sh:…` → mismatch = `'B' 0x38D` hang]*; GetID once more → SCEx letters to `0x80079F1C`.
11. **Licence text**: *[flag on: `strcmp(line, "          Licensed  by          Sony Computer
    Entertainment Inc.")` (`sh:8005AE60`) → mismatch = not booted]*; 3.0A: not compared, the line
    and the disc's logo are drawn as they are (`sh:8003F414`); POST 5; **shell returns**.
12. **CD re-init** (POST 8): `CdInit 0xBFC073A0`; PVD at LBA 16 must say `"CD001"`; path table
    read, words XORed → disc identity `0xA0009D7C`; ≤ 44 directories; expansion ROM at `0x1F000004`
    → post-boot hook `*0x1F000000` (prints `"PIO SHELL for PlayStation(tm)"`); banner
    `"BOOTSTRAP LOADER Type C Ver 2.1   03-JUL-1994"`.
13. **SYSTEM.CNF** (POST 9): `open("cdrom:SYSTEM.CNF;1")` — root directory, first sector, ≤ 40
    records, upper-case, `;1`; ≤ 0x800 bytes to `0xA000B070`; `TCB/EVENT/STACK` hex, missing = 0;
    `BOOT = <file> [argument → RAM 0x180]`; absent/empty → `cdrom:PSX.EXE;1` with the ROM defaults.
14. **Kernel setup 2** with the parsed counts; user RAM `0x10000..sp` wiped (`rom:BFC0D850`);
    `LoadExeFile`: 2048-byte header, **no magic check**, fields 0x10..0x4B → `0xA000B870`;
    `read(t_addr, t_size)` (must be a sector multiple, result unchecked); fail → `'B' 0x38A` hang.
15. **Pre-exec disc check** (`rom:BFC0D570`, if `0xA000DFFC`): `ReadTOC`, `GetID` status
    `& 0x1D` → `'D' 0x38B` hang on error [absent in 1.0J].
16. **Exec**: `EnterCriticalSection`; `DoExecute(0xA000B870, 1, 0)`: bss `b_addr/b_size` zeroed,
    `sp = STACK` (if ≠ 0), `gp`, `jalr pc0`. Return → `"End of Main"` → `'B' 0x38C` hang, POST F.

---

## 9. Tool-worthy, ranked

1. **PS1 hand-off card** (§8) for ROM + disc image: version/letter from the tail string, the
   per-shell licence mode (`Unconditional / Always / Never / ByLetter`) read from the dump, the
   sector-4 line and the 0x3278-byte logo compared with the dump's own blob when the mode says so,
   `SYSTEM.CNF` parsed with the real grammar (prefix keys, hex, `argument`), the EXE's loadability
   (`t_size % 0x800`, root-directory position ≤ 40/first sector, `;1`), the pre-exec check, and
   the one explicit assumption about the drive's GetID answer. Extends `Ps1Verdict::judge`.
2. **Shell unpacker** for 4.x dumps (§2.3) — prerequisite for 1 on 4.1/4.5/PSone and for the
   visual-scenes work; 25 lines, verifiable against the in-ROM code at `0x801C44B8`.
3. **Kernel identity check**: md5 of `0x0000..0x17FFF` tells the user their 2.2–4.5 dump carries
   the unchanged 1995-12-04 kernel; `0xBFC00100/104/12C`, the maker string, the A0 table size
   (0xAF vs 0xC1) and `GetSystemInfo` answers, all from the dump.
4. **POST / TTY replay** (§1.3, §1.5): the 0x1F802041 stage sequence and the dummy-TTY log
   reconstructed from the format strings with the user's values — a boot narration that is
   entirely derivable and visibly "what the console would have printed had it a port".
5. **SYSTEM.CNF linter** with the kernel's quirks: `TCB = 10` is 16, a missing `STACK` is "inherit
   the BIOS stack", a `BOOT2` line before `BOOT` hides the PS1 boot file, trailing words become the 0x180 argument, CR is fine.
6. **Expansion-ROM probe**: given a cartridge dump, test `0x1F000004`/`0x1F000084` against the
   ROM string and name the hook that fires and when (pre-kernel vs post-shell).
7. **1.0J monitor leftovers viewer** (§6.4): show the dead command table from the user's own
   SCPH-1000 dump — the only trace of the "PS-X ROM monitor Ver.2.3" that every ROM names.
8. **What-we-cannot-know banner** (§7): the GetID verdict, the SCEx letters and the controller
   date are drive-side inputs; print them as assumptions, never as findings.
