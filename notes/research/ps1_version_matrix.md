# PS1 BIOS version matrix — every dump the user has, side by side

Breadth survey (2026-10-10) of the 15 PlayStation BIOS images in the PCSX2 bios folder, the
PSP firmware 6.60 POPS image (`PSXONPSP660.BIN`), and the PS1-mode content of the six PS2 rom0
images (1.00 J, 1.50 devkit, 1.60 E, 1.60 A ×2, 2.00 E). Everything was read from the dumps
with `tools/r3000dis.py`-style capstone disassembly, `cmp`, python hashing and one Ghidra pass
over the PSP shell (scratchpad project, deleted). Nothing here is copied from the ROMs beyond
short hex at patch sites. Tags: **[V]** read in the dump at the given offset, **[I]** inferred.
Public knowledge (psx-spx, redump/No-Intro names) is used for orientation only and is marked.

Companion notes: `notes/ps1_boot.md` (the PS2's PS1 licence screen, decompiled),
`notes/research/kernel_eeload_hooks.md` §1.1 (how the PS2 IOP enters PS1 mode).

ROM offsets below are file offsets; the ROM is mapped at `0xBFC00000`, so `0x7FF32` is
`0xBFC7FF32`. "Shell" = the program copied to `0x80030000`; "kernel" = the part copied to
`0xA0000500`; "bootstrap" = the ROM-resident code at `0xBFC00000–0xBFC10000`.

---

## 0. Headlines

1. **Only two PS1 kernels exist in this whole set.** Bootstrap + kernel (`0x0000–0x18000`,
   header words excluded) is byte-identical across 2.2 J/A/E, 3.0 A/E, 4.1 A/E and 4.5 A, and
   byte-identical (code; only padding filler differs) in **PSXONPSP660** — the kernel Sony
   shipped in Dec 1995 ran unchanged into the PSone (May 2000) and the PSP (2011). 1.0/1.1/2.0
   share the other kernel. 3.0 J and 4.0 J differ from the 2.2 kernel by 24 and 21 bytes. [V]
2. **The 2.0→2.2 kernel change** removed the interactive ROM monitor (≈1 100 + 1 000
   instructions of debug shell with `exec/load/sector/mem/pad/led/dbc…` commands and a register
   dumper), and **added** `GetSystemInfo` (`BFC0D4F0`), a pre-exec CD-ROM re-verification
   (`ReadTOC` 0x1E + `GetID` 0x1A, `SystemError('D',0x38B)` on failure, gated by a flag the
   shell sets at `0xA000DFFC`), a user-RAM wipe before `Exec`, POST-code stages 1–6, and
   RAM_SIZE programming through `0x1F801060` bit 8–9 instead of constants. [V]
3. **The licence/logo check is a J thing in 1.0–4.0, letter-driven only from 4.5.** J shells set
   the check flag to 1 at entry and carry the reference PlayStation-logo TMD; U/E 2.2–4.1
   shells set it to 0 (the `Licensed by…Inc.` string is dead data) and ship no TMD; 2.0 E has no
   string at all. 4.5 (PSone) is the first "world" shell: `E` → Europe strings, `A` → check
   off, anything else → `Inc.` — exactly the PS2 rom0:LOGO logic. [V]
4. **4.1 A and 4.1 E are one shell** (SHA-1 of `0x18000–0x64000` identical; only the version
   string differs). 3.0 A (scph5501) and the file labelled SCPH-7003 are the same file. [V]
5. **The `[h]` SCPH-5000 image is not a hack, it is a text-mode-corrupted file**: every LF in
   the ROM got a CR in front of it (first at `0x24A`, inside the reset code), plus eight
   string edits by hand in the kernel's TTY strings (`Ver.2.5`→`1.2`, `1993,1994`→`1999,2000`,
   `%08x`→`%0fx`, `GPU_sync(BG)`→`GPU-sync(BrG)`, `2/8`→`4/16`, `6 button`→`8 button`). It cannot
   boot. [V]
6. **PSXONPSP660 = unmodified 2.2-lineage kernel + a stripped PS2-LOGO-lineage shell.** The shell
   (sys.c 1.140, 1998) is 135 KB, LZ-packed with the OSD packer, draws the **ROM's own** logo
   TMD instead of the disc's, shows the disc's sector-4 text without comparing it, has no
   GetID, no `SystemErrorBootOrDiskFailure`, no region-letter code (NTSC fixed), and ends with
   a 120-field wait. No new BIOS calls for the PSP host appear. The version string was edited
   to `4.5 05/25/00 J` (a combination no retail console carries). [V]
7. **The PS2 does not embed a 512 KB PS1 BIOS.** It keeps three PS1 things at their PS1 ROM
   offsets so the unchanged PS1 kernel code works — the SJIS font `KROM` at `0x66000`
   (byte-identical to every PS1 image), `KROMG` at `0x64000` (= the U/E block, minus a tail
   table) and `VERSTR` at `0x7FF30` — and replaces bootstrap+kernel with `TBIN` (ROM monitor
   2.6, 1999) + `SBIN`, and the shell with `LOGO`. TBIN/SBIN/KROM/KROMG are byte-identical in
   all six PS2 ROMs; only LOGO (3 builds) and VERSTR change. [V]

---

## 1. Identity table

Header words at `0x100` (BCD date), `0x104` (console type, always 3), `0x108` (maker string),
`0x12C`/`0x129` (model string with the `CEX` marker — no image carries `DEX`), version string at
`0x7FF32` (absent in 1.0), kernel banner at `0xDB51`/`0xDFB1`. [V for every cell]

| file (as named in the folder) | SHA-1 | size | hdr date | model string | version string @0x7FF32 | kernel banner | matches |
|---|---|---|---|---|---|---|---|
| `(J)[SCPH-1000]` | `343883a7…dd070` | 512K | 1994-09-22 | `CEX-1000 KT-3  by S.O.` | **none** (0x7FF32 is data) | Realtime Kernel 2.5 / ROM monitor 2.3 | SCPH-1000 1.0 J |
| `(J)(v1.1)[SCPH-3000]` | `b06f4a86…7cc4e` | 512K | 1994-09-22 | `CEX-1000 KT-3  by S.O.` | `1.1 01/22/1995` (no letter) | 2.5 / 2.3 | SCPH-3000 1.1 J |
| `(E)(v2.0)[SCPH-1002]` | `20b98f3d…cc62c` | 512K | 1994-09-22 | `CEX-1000 KT-3  by S.O.` | `2.0 05/10/95 E` | 2.5 / 2.3 | SCPH-1002 2.0 E |
| `(J)(v2.2)[SCPH-5000]` | `ffa7f9a7…2c30d` | 512K | 1995-12-04 | `CEX-3000/1001/1002 by K.S.` | `2.2 12/04/95 J` | 2.5 / 2.3 | SCPH-5000 2.2 J |
| `(J)(v2.2)[SCPH-5000][h]` | `81622ace…57a6` | **526 083** | 1995-12-04 | same | `2.2 12/04/95 J` @**0x80632** | `Kernel Ver.1.2 … 1999,2000` (edited) | corrupted copy of the above, §5 |
| `SCPH-1001 - DTLH-3000 … (US)` | `10155d8d…8ebf` | 512K | 1995-12-04 | `CEX-3000/1001/1002 by K.S.` | `2.2 12/04/95 A` | 2.5 / 2.3 | SCPH-1001 2.2 A (the widely known `scph1001.bin`); DTL-H3000 claim §4 |
| `(E)(v2.2)[DTLH-3002]` | `b6a11579…6df2c` | 512K | 1995-12-04 | same | `2.2 12/04/95 E` | 2.5 / 2.3 | DTL-H3002 2.2 E |
| `(J)(v3.0)[SCPH-5500]` | `b05def97…b6917` | 512K | 1995-12-04 | same | `3.0 09/09/96 J` | 2.5 / 2.3 | SCPH-5500 3.0 J |
| `scph5501.bin` | `0555c6fa…9f30b` | 512K | 1995-12-04 | same | `3.0 11/18/96 A` | 2.5 / 2.3 | SCPH-5501/5503/7003 3.0 A |
| `(U)(v3.0)[SCPH-7003]` | `0555c6fa…9f30b` | 512K | — | — | — | — | **same file as scph5501.bin** |
| `(E)(v3.0)[SCPH-5502 + SCPH-5552]` | `f6bc2d1f…fd3f0` | 512K | 1995-12-04 | same | `3.0 01/06/97 E` | 2.5 / 2.3 | SCPH-5502/5552 3.0 E |
| `(J)(v4.0)[SCPH-7000]` | `77b10118…eb3e9` | 512K | **1997-05-29** | `CEX-7000/-7001 by K.S.    ` | `4.0 08/18/97 J` | 2.5 / 2.3 | SCPH-7000 4.0 J |
| `(U)(v4.1)[SCPH-7001 + SCPH-9001]` | `14df4f6c…647a9` | 512K | 1995-12-04 | `CEX-3000/1001/1002 by K.S.` | `4.1 12/16/97 A` | 2.5 / 2.3 | SCPH-7001/7501/9001 4.1 A |
| `(E)(v4.1)[SCPH-7502 + SCPH-9002]` | `8d5de56a…418c1d` | 512K | 1995-12-04 | same | `4.1 12/16/97 E` | 2.5 / 2.3 | SCPH-7502/9002 4.1 E |
| `PSone BIOS (U)(v4.5)[SCPH-101]` | `dcffe16b…8378` | 512K | 1995-12-04 | same | `4.5 05/25/00 A` | 2.5 / 2.3 | SCPH-101 4.5 A |
| `PSXONPSP660.BIN` | `96880d1c…b4e14` | 512K | 1995-12-04 | same | `4.5 05/25/00 J` | 2.5 / 2.3 | no retail console; §6 |
| PS2 1.00 J rom0 (`scph10000.bin` = the long-named file) | rom0 `aea061e6…` | 4M | 2000-01-17 | `PS compatible mode by M.T.` | `5.0 01/17/00 T` | TBIN: Kernel 2.5 (1993,1994,1998,1999) / monitor 2.6 | ROMVER `0100JC20000117` |
| PS2 1.50 devkit (`DTL-H30101…rom0`) | — | 4M | 2000-12-28 | same | `5.0 12/28/00 A` | same TBIN | ROMVER `0150AD20001228` |
| PS2 1.60 E (`[SCPH30004]` = `PS2 Bios 30004R V6 Pal.bin`) | rom0 `8fa04085…` | 4M | 2001-10-04 | same | `5.0 10/04/01 E` | same TBIN | ROMVER `0160EC20011004` |
| PS2 1.60 A (`scph39001.bin` = `[SCPH39001]`) | rom0 `f9a5d629…` | 4M | 2002-02-07 | same | `5.0 02/07/02 A` | same TBIN | ROMVER `0160AC20020207` |
| PS2 1.60 A (`(U)…[SCPH39004]`) | — | 4M | 2002-03-19 | same | `5.0 03/19/02 E` | same TBIN | ROMVER `0160AC20020319` — **PS1-mode letter is E** although ROMVER says A |
| PS2 2.00 E (`SCPH-70004…200.BIN`) | — | 4M | 2004-06-14 | same | `5.0 06/14/04 E` | same TBIN | ROMVER `0200EC20040614` |

Mislabel / duplicate findings [V]:
* `(U)(v3.0)(1996-11-18)[SCPH-7003].bin` and `scph5501.bin` are byte-identical.
* `scph10000.bin` = `Sony PlayStation 2 BIOS (J)(v0.1)(2000-01-17)[SCPH10000].bin`;
  `scph39001.bin` = `…(U)(v1.6)(2002-02-07)[SCPH39001].bin`; `PS2 Bios 30004R V6 Pal.bin` =
  `…(E)(v1.6)(2001-10-04)[SCPH30004].bin` — three pairs of duplicates.
* `(J)[SCPH-1000]` has **no** version string anywhere; the "1.0" is a convention. Its header
  date 1994-09-22 is the kernel's build date, shared with 1.1 and 2.0 E.
* The `[SCPH39004]` file is named `(U)` but its PS1-mode version string ends in `E` (the PS1
  shell will treat it as PAL, ps1_boot §5.1); its ROMVER says `A`. One of the two is wrong for
  a real SCPH-39004 (which is a PAL model) — the ROMVER region letter is the suspicious one [I].
* The region letter lives only in the shell's version string, never in the header: 1.0 and
  1.1 have no letter (1.1's string is `01/22/1995`, four-digit year, no letter).
* The `CEX` in every PS1 header is part of the model string (`CEX-1000`, `CEX-3000/1001/1002`,
  `CEX-7000/-7001`); the devkit DTL-H3002 carries the same retail string — no `DEX` marker
  exists in any PS1 image. The `CEX` at `0x2C2E` of the 1.00 J PS2 rom0 is a build path in
  EXTINFO (`…/iop_protokernel/system/kernelbin/PS2CEX/Rom`), not a PS1 marker.
* Header date vs version date: 2.2–4.1 all keep the 1995-12-04 header (the kernel's date) while
  the shell's date advances — the header dates the kernel, the `0x7FF32` string dates the
  shell. Only 4.0 J re-stamped the header (1997-05-29) and the model string.

---

## 2. Layout map (located by code)

### 2.1 PS1 images — the two copy loops [V]

* Reset at `BFC00000`: `0x1F801010 ← 0x0013243F` (ROM delay/size), `0x1F801060 ← 0xB88`
  (RAM_SIZE), `j BFC00150`, cache/COP0 init, then at `0x420` (1.0–2.0: `0x410`):
  `memcpy(0xA0000500, 0xBFC10000, 0x8BF0)` and `jr 0xA0000500` — **kernel = ROM 0x10000,
  0x8BF0 bytes**, in every PS1 image (also PSXONPSP660).
* Shell loader at `0x6FF0` (1.0–2.0: `0x6F6C`): `memcpy(0x80030000, 0xBFC18000, 0x67FF0)`,
  FlushCache, `jalr 0x80030000` — **shell = ROM 0x18000–0x7FFF0**, in every PS1 image.
  The kernel copy overshoots into `0x18000–0x18BF0`; those bytes are the shell's head
  (its entry at `0x18000` is `addiu sp,-0x18; sw ra; jal …`), harmless at `0xA00085xx` [V+I].
* Inside the shell window the real content ends well before `0x7FF30`; the rest is filler:

| image | shell data ends | filler | `0x64000–0x65CB7` | `0x65CB7–0x66000` | `0x66000–0x7FE70` | `0x7FE70–0x7FF30` |
|---|---|---|---|---|---|---|
| 1.0 J / 1.1 J | `0x635D4` / `0x632B4` (+0xAA gaps) | 00/AA/FF | shell tail | shell tail | SJIS font (same in all) | 0xC0 bytes glyph data (same in all PS1) |
| 2.2 J / 3.0 J | `0x64BE9` / `0x64BB9` | 00/AA | **shell tail** (J shells are larger) | 00 | font | glyphs |
| 4.0 J | `0x4A164` | 00 | font block A | 00 | font | glyphs |
| 2.0 E / 2.2 E(dev) / 3.0 E | `0x5F479`/`0x5F7B9`/`0x5F969` | 00/FF | block A (`0x64000–0x65CB7`, = PS2 KROMG) | 16-bit table (U/E only) | font | glyphs |
| 2.2 A / 3.0 A | `0x61DD9` / `0x61E19` | 00/AA | block A | table | font | glyphs |
| 4.1 A/E | `0x48DFC` (packed) | 00 | block A | table | font | glyphs |
| 4.5 A | `0x4C610` (packed) | 00 | block A | table | font | glyphs |
| PSXONPSP660 | `0x38A00` (packed + raw tail) | **FF** | block A | **FF** | font | glyphs, then `FFFF` at `0x7FF2E` |

* Font: `Krom2RawAdd` in the kernel (ROM `0x16108`/`0x16130`, A0 area) maps SJIS `0x8140–0x84BF`
  to `0xBFC66000` and `0x889F–0x9873` to `0xBFC69D68`, 30 bytes per glyph [V]. The block at
  `0x66000–0x7FE70` (0x19E70 bytes) is **byte-identical in all 14 retail/devkit PS1 images, in
  PSXONPSP660 and in every PS2 rom0 (`KROM`)** — SHA-1 prefix `50bd0533` [V].
* Block A at `0x64000–0x65CB7`: identical in all U/E images and the PS2 `KROMG` file
  (0x1CB7 bytes); 16-px-wide bitmap rows [V]; its consumer was not located in the kernel (no
  `lui 0xBFC6 / ori 0x4000` pair) — probably shell-side half-width glyphs [I]. The `0x65CB7–
  0x66000` 16-bit table that follows in U/E images is zeroed by the PS2 and FF-filled by the
  PSP, so nothing in kernel or PS2/PSP shells needs it [I].
* 4.0/4.1/4.5 shells are **LZ-packed**: `0x18000` holds a 0x4C-byte stub that copies the
  stream to `0x80190000`, calls A(44h) FlushCache, and jumps to a decompressor placed at the
  stream's tail; the stream at `0x1804C` starts with a 24-bit size (`0x5BDA0` 4.0, `0x5AD40`
  4.1, `0x60990` 4.5) and is the same format the PS2 OSD uses (`tools/osd_unpack.py --raw
  <img> 0x1804c` unpacks it) [V]. 1.0–3.0 shells are raw.

### 2.2 Region hashes and clusters [V]

SHA-1 prefixes over fixed windows (`boot` = `0x0–0x100` + `0x180–0x10000`):

| image | hdr `0x100–0x180` | boot | kernel `0x10000–0x18000` | shell `0x18000–0x64000` | `0x64000–0x66000` | font | ver |
|---|---|---|---|---|---|---|---|
| 1.0 J | 6542d7b2 | **B1** ea536166 | **K1** 0f1e8bd1 | f8279709 | f1ceb6c7 | F cddc96d3 | 4afc3e40 |
| 1.1 J | 6542d7b2 | B1 | K1 | e14d6559 | fda7c3c0 | F | 0e62ef84 |
| 2.0 E | 6542d7b2 | B1 | K1 | 72c79e4d | A 12da5d12 | F | 04829c2b |
| 2.2 J | 55a32fb3 | **B2** 61be23f4 | **K2** 78ecbfdc | 343ce41d | 458f5377 | F | 2865f44a |
| 2.2 A | 55a32fb3 | B2 | K2 | 9e400d23 | A | F | 2878bc47 |
| 2.2 E dev | 55a32fb3 | B2 | K2 | ecbc36e5 | A | F | a6e98e81 |
| 3.0 J | 55a32fb3 | **B3** 02bf49b7 | K2 | 0fea5163 | 194d7283 | F | 427b8385 |
| 3.0 A | 55a32fb3 | B2 | K2 | 647c88ff | A | F | afe59001 |
| 3.0 E | 55a32fb3 | B2 | K2 | f61cff6f | A | F | 8de54315 |
| 4.0 J | 9c4c7be7 | **B4** 98209c5e | K2 | 2bdb08e8 | 06314572 | F | f98e9838 |
| 4.1 A | 55a32fb3 | B2 | K2 | **S41** 9f2d21d4 | A | F | 2984d743 |
| 4.1 E | 55a32fb3 | B2 | K2 | **S41** 9f2d21d4 | A | F | d53e6dcb |
| 4.5 A | 55a32fb3 | B2 | K2 | 441756a5 | A | F | f3c3f69d |
| PSXONPSP660 | 55a32fb3 | B2′ (FF filler) | K2′ (FF filler) | ea30a98a | 5eb44f82 | F (+2 bytes) | 27c1beba |

Clusters: kernels **K1** {1.0, 1.1, 2.0 E} and **K2** {2.2 J/A/E, 3.0 J/A/E, 4.0 J, 4.1 A/E,
4.5 A, PSP}; bootstraps B1 = K1 set, B2 = K2 set minus 3.0 J (B3: +24 bytes at `0x26–0x48`)
and 4.0 J (B4: first word + header); shells all unique except **4.1 A = 4.1 E** and 3.0 A = the
SCPH-7003 file. Asset blocks: see §3.4.

### 2.3 PS2 rom0 — what plays the PS1 BIOS's part [V]

The first 512 KB of a PS2 rom0 is the IOP/EE RESET code and the IOP kernel modules, not a PS1
BIOS. PS1 mode is entered by RESET (`0xBFC02000` path, `PRId ≥ 0x10` and `0xBF801450` bit 3)
through a ROMDIR lookup of `TBIN` (kernel_eeload_hooks §1.1). The PS1-shaped pieces, per ROMDIR:

| ROMDIR file | offset (all six) | size | SHA-1 prefix | identical across PS2 ROMs | PS1 equivalent |
|---|---|---|---|---|---|
| `TBIN` | `0x4B800` | 0xDF70 | 2dc6fccb | **yes, all six** | bootstrap + kernel (runs in place at `0xBFC4B800`; "PS-X Realtime Kernel Ver.2.5", "ROM monitor Ver.2.6", `Copyright 1993,1994,1998,1999`) |
| `SBIN` | `0x3170…0x35C0` | 0x6FA0 | 2f3d9c8c | yes | kernel's device/pad half as an ECOFF (ps1_boot §1) |
| `LOGO` | `0xA110…0xA560` | 0x1456C / 0x14748 / 0x14694 | 99bce935 / db8b8e21 / 999b0835 | **three builds**: 1.00 J; 1.50 dev; 1.60 E = 1.60 A = 1.60 A2 = 2.00 E | the shell (stub + LZ, unpacks to 0x20E50/0x20F60/0x210C0 at `0x80030000`) |
| `KROMG` | `0x64000` | 0x1CB7 | fe02d6da | yes | PS1 U/E block A at `0x64000` — 364 bytes differ inside `0x64000–0x65CB7` and the PS1 tail table is zeroed [V]; same glyph set with edits [I] |
| `KROM` | `0x66000` | 0x19E70 | 50bd0533 | yes | **byte-identical** to every PS1 image's `0x66000–0x7FE70` |
| `VERSTR` | `0x7FF30` | 0x5E | per ROM | no (date/letter) | the `0x7FF32` version string; `Copyright 1993-1999` |

The 0xC0 bytes at `0x7FE70–0x7FF30` that every PS1 image fills with glyph data are zero in all
PS2 ROMs (84 byte differences vs 3.0 A, all in that window) [V]. The ROMDIR places `KROMG`,
`KROM` and `VERSTR` at exactly the PS1 offsets — the only reason is the unchanged PS1 kernel
code (`Krom2RawAdd`, region-letter read) that addresses them absolutely [I].

---

## 3. Revision deltas

### 3.1 1.0 → 1.1 → 2.0 (same kernel K1, three shells) [V]

Kernel and bootstrap are byte-identical in the three. Differences are shell-only:
* 1.0 J: shell has no `System ROM Version` string at all, so no region letter and no
  `0x7FF32` read; it has the oldest libgs sources inlined (`$Id: sys.c,v 1.16 1994/08/23`,
  `ext.c 1.15`, `prim.c 1.15`, `intr.c 1.21`, `pad.c 1.10`), no `System Controller ROM Version`
  string (the MechaCon version readout), no `<<cdPause() is Cancelled.>>`. Licence check:
  unconditional `strcmp` against `Licensed by Sony Computer Entertainment Inc.` at
  `0x800307D0` (shell `0x7D0`), with the reference TMD at shell `0x47744`.
* 1.1 J: adds the version string (`1.1 01/22/1995`), the `System Controller ROM Version`
  display, `font.c 1.15`; TIM set re-drawn (44x47, 62x105, 64x226 all new hashes); logo TMD
  moves to `0x46410`. Check still unconditional (`0x800307B0`).
* 2.0 E: first PAL build. `$Id` lines reduced to intr.c/pad.c; **no licence string and no
  TMD** (no check at all); new E-shaped text sprites (36x142/36x143/23x128 TIMs).

### 3.2 2.0 → 2.2: the big kernel step (K1 → K2) [V]

Normalised instruction diff of `0x0–0x18000` (2.0 E vs 2.2 A; 2.2 J/E identical to 2.2 A):

| site (2.2) | what | evidence |
|---|---|---|
| `BFC00014–1C` | reset now also writes `0x1F801060 ← 0xB88` before the jump | 3 insns replace nops |
| `BFC045BC–D0` | six `cache` ops added in the cache-init path | insert |
| `BFC06680` SetMem | RAM_SIZE now computed: `v = *0x1F801060 & ~0x700; 2 MB → v; 8 MB → v\|0x300` instead of the constants `0x880`/`0xB80` | code |
| `BFC068DC–0694C` kernel init | copies the 12-byte default config {TCB 4, EVENT 0x10, STACK 0x801FFF00} from ROM `0xE14C` to `0xA000B940` and passes those values to the EvCB/TCB allocators instead of the literals 0x10 / 4 | code + data at `0xE14C` |
| `BFC0D4F0` **GetSystemInfo(index)** | new: jump table at ROM `0xDCB0` for index 0–0xE: 0/1 → `lw 0xBFC00100+4·i` (date, console type); 2 → maker string; 3 → pointer `0xBFC0012C` (model string); 4 → `*0x60 << 10` (RAM in KB); others → `lh 0xBFC0E600[i]` | 228 new insns; psx-spx calls this A(B4h) [orientation] |
| `BFC0D570` pre-exec hook | new: `EnterCriticalSection`; if `*0xA000DFFC != 0` → CD command **0x1E** then **0x1A (GetID)** through the register table at ROM `0xE620` (`1F801800..03`), waiting on INT type, `SystemErrorBootOrDiskFailure('D', 0x38B)` on INT5/error; `ExitCriticalSection`; then the original exec (`BFC03CF0`). Called from both boot paths (`BFC06C78`, `BFC06E98`) | code |
| `BFC0D850` RAM wipe | new: `for (p = 0xA0010000; p < sp; p += 4) *p = 0` — user RAM cleared before `Exec`, called at `BFC06BB4`/`BFC06DD4` | code; the PS2 TBIN prints `Clear 0x10000 to 0x%x` for the same step |
| kernel `A0004360` POST | `sb a0, 0x1F802041` stage reporter existed in 1.0; 2.2 adds call sites with stages 1–4 in kernel init (`A0002828…A00028A4`) and 5–6 | code |
| kernel strings | `EXIT:%s %s` → `ioabort exit:%s %s` | strings |
| **removed** | the whole ROM monitor: 1 082 insns at `BFC0E1C8–BFC0FFFC` plus `0x16F50–0x18000` of command tables/strings (`exec/load/read/sector/sc/mem/sd/ad/dd/showcom/addcom/delcom/mem1/mem2/pad1/pad2/led/dbc/dhc/dwc`, `Lisenced by Sony…`, `original/licensed mentenance code!`, `Effective memory must be 2/8/16 MBytes`, `$Id: kmem.c,v 1.1 1994/01/24`, a 32-register dumper). 2.2 leaves `0xE630–0x10000` and `0x16F51–0x18000` zero | strings + diff |

Who sets `0xA000DFFC`? Only the shell: `sw zero,-0x2004(0xA001xxxx)` at shell `0x1DF5C` and a
`sw t6` at `0x1E0AC` (2.2 A); in 4.5 at `0x64D8`/`0x6620` of the unpacked shell [V]. So the
kernel's ReadTOC/GetID re-check before a game starts is armed by the shell after the licence
screen, on every 2.2+ console, independent of region [V+I].

### 3.3 2.2 → 3.0 → 4.0/4.1 → 4.5 [V]

* **Kernel/bootstrap**: unchanged (K2/B2) except:
  * 3.0 J only: 24 bytes at `0x26–0x48` — a 255-iteration delay loop and `sw zero → 0x00086A24`
    inserted in the nop slot after the ROM-delay setup, before `j BFC00150` [V]; purpose not
    determined (a RAM poke before the main init) [I].
  * 4.0 J only: first word `lui t0,0x14` → `0x1F801010 ← 0x0014243F` (ROM size field 0x14
    instead of 0x13, i.e. a 1 MB ROM window) [V]; plus the re-stamped header (date 1997-05-29,
    `CEX-7000/-7001 by K.S.`). psx-spx documents `0x13243F` as the normal 512 KB value
    [orientation] → the SCPH-7000 board carries a different ROM part [I].
* **2.2 → 3.0 shell** (A: 204 of ~43 000 instructions differ in the code half; same for E/J):
  entry gains `B(15h) OutdatedPadInitAndStart(0x20000001, 0)` at `0x8003024C`; small state
  changes in the CD-player/memory-card menu (`0x80037xxx`); one text sprite per region
  re-drawn (A: 64x108 `231f5690`→`c619f704`; E: 36x142 pairs; J: 64x226); copyright line
  `1993,1994,1995` → `1993-1996` (A) / `1993-1997` (E) / `1993,1994,1995,1996` (J).
* **3.0 → 4.0/4.1 shell**: new packed build. Drops the `$Id` strings; adds the libmcrd layer
  (`Access Denied. : …` ×20, `libmcrd: event overflow`), `BPLAYSTATION` icon strings and
  `error PSXload` (PocketStation support [I]); new memory-card UI sprites (three 4x16 TIMs,
  50x36, 50x44, 60x44, 60x54, 6x10, 8x32 variants). 4.0 J keeps the J behaviour (flag 1, TMD at
  `0x38780`); **4.1 A and 4.1 E are the same shell** with `region=='E' ? 512 : 480` lines and
  the check flag hard-set to 0.
* **4.1 → 4.5 shell** (PSone): the "world" shell — three licence strings
  (`…Euro pe   ` 70, `…(Europe)` 67, `…Inc.` 64), the reference TMD is back (`0x39360`), and
  the region function `0x80030A6C` sets flag=1, returns 1 for `E` (PAL), clears the flag for
  `A`, else NTSC with `Inc.` [V] — the same decision table as the PS2 rom0:LOGO (ps1_boot
  §5.1). New GUI art (two 32x128, 12x48, 48x48, 64x100 TIMs); 60x54/6x10 redrawn.

### 3.4 The licence/logo check matrix [V]

Compare site = the `strcmp(disc_line, "          Licensed  by          Sony Computer
Entertainment Inc.")` call; flag = the word tested right before it.

| shell | `Licensed by` string(s) | reference TMD | check flag at entry | effective check | `SystemError(…,0x38D)` present |
|---|---|---|---|---|---|
| 1.0 J, 1.1 J | Inc. | yes | (no flag, unconditional) | **on** | yes |
| 2.0 E | **none** | no | — | **off** | **no** |
| 2.2 J, 3.0 J, 4.0 J | Inc. | yes | `=1` (`sw t7`, `0x80030058`/`0x80030048`) | **on** | yes |
| 2.2 A, 2.2 E dev, 3.0 A, 3.0 E | Inc. | no | `=0` (`sw zero`, `0x80030054`/`0x80030058`) | **off** (string is dead data) | yes (other error paths) |
| 4.1 A/E | Inc. | no | `=0` (`0x8003005C`) | **off** | yes |
| 4.5 A | 3 strings | yes | `=1`, cleared for `A` | **letter-driven** | yes |
| PS2 LOGO 1.00 J | Inc. | yes | constant 1 | on | yes (ps1_boot §5.3) |
| PS2 LOGO 1.50 dev | none | yes | — | off | yes |
| PS2 LOGO 1.60/2.00 | 3 strings | yes | letter-driven | letter-driven | yes |
| **PSXONPSP660** | **none** | yes (drawn, not compared) | — | **off** | **no** |

Asset clustering across every shell (hashes of located TIM/TMD/VAB blocks): the PlayStation
logo TMD (`3d032a12`, 1 object, 337 vertices, 560 primitives) is identical wherever present
(1.0 J … 4.5, all PS2 LOGO builds, PSP); the boot-sound VAB header (`ea548f72`, 4 programs,
3 VAGs) is identical in all 17 shells; the 50x40 (`43d91df2`/`81e29b20`) and 5x8 (`de9674d6`)
TIMs are identical in every shell from 1.1 on [V]. So the licence screen's model, sound and
"Sony Computer Entertainment" sprites never changed from 1995 to the PSP.

---

## 4. Retail vs devkit

* **SCPH-1002 2.0 E vs DTL-H3002 2.2 E**: different kernels (K1 vs K2, §3.2) — so the real
  difference is the generation, not devkit-ness. Shell-wise DTL-H3002 is a 2.2 E shell: same
  E text sprites as 2.0/3.0 E, plus the `Licensed by…Inc.` string that 2.0 E lacks (flag 0,
  so unused), and the copyright line `1993,1994,1995`. Normalised code similarity of the shell
  code half: 2.0 E vs 2.2 dev 0.95; 2.2 dev vs 3.0 E 0.83 (3.0 E is the bigger step) [V].
* **What the devkit skips**: nothing that can be seen. DTL-H3002's bootstrap and kernel are
  byte-identical to SCPH-1001's (0 differing bytes below `0x18000`), its header says `CEX`, the
  pre-exec ReadTOC/GetID hook is present and reachable, and the shell's check flag is 0 exactly
  like retail A/E 2.2. No `DEX`, no serial/`host:` loader, no extra command [V]. (The ROM
  monitor that would have made a devkit interesting was removed in 2.2 for everyone, §3.2.)
* **`SCPH-1001 - DTLH-3000`**: the file is the known SCPH-1001 2.2 A image (SHA-1
  `10155d8d…`, the common `scph1001.bin`). Nothing in it refers to a DTL model; the claim that
  the DTL-H3000 shipped the identical ROM cannot be checked from this folder — there is no
  separate DTL-H3000 dump [I]. By analogy with DTL-H3002 (= retail 2.2 kernel + retail-style
  2.2 E shell) it is plausible [I].

---

## 5. The `[h]` SCPH-5000 image (526 083 bytes) [V]

* Not a header or trailer: the file equals the clean SCPH-5000 for `0x0–0x249`; the first
  divergence is at `0x24A`, where the clean ROM has byte `0x0A` (inside `addiu t2,zero,0` of the
  reset code) and `[h]` has `0x0D 0x0A`. Every `0x0A` byte of the ROM (1 793 of them, 306 below
  `0x10000`) is preceded by an inserted `0x0D`; the file has 1 794 CRLF pairs and no lone LF.
  Two lone `0x0D` bytes of the clean ROM (`0x569E2`, `0x5F0AE`) are missing. Classic
  text-mode transfer / text-editor save.
* On top of that, eight hand edits in the kernel's TTY strings (clean → `[h]`, clean offsets):
  `0xDED3` `GPU_sync(FG/BG/BG2)` → `GPU-sync(FG/BrG/BrG2)` and `%08x` → `%0fx` (the latter
  repeated at `0xE0AE`, `0xE22B`, `0xE4F5`); `0xDFCA` `Kernel Ver.2.5 / Copyright 1993,1994` →
  `Ver.1.2 / 1999,2000`; `0xE022` `ROM monitor Ver.2.3` → `1.1`; `0xE0E1` `Type C Ver 2.1
  03-JUL-1994` → `1.1 03-Jan-2000`; `0xE199` `2/8 MBytes` → `4/16`; `0xE4F5` `6 button PAD` →
  `8 button PAD`.
* Consequence: the version string lands at `0x80632` (+0x700), the kernel copy loop would copy
  shifted bytes, and the reset code itself is corrupted at `0x24A` — the image cannot run. A
  tool should flag: size ≠ 512 KB, CRLF density, and "header intact but body shifted".

---

## 6. PSXONPSP660 — what Sony changed to run PS1 inside the PSP

### 6.1 Provenance [V]

* Header, bootstrap and kernel (`0x0–0x18000`) equal the 2.2-lineage retail code byte for byte;
  the only differences are the filler bytes `0xE630–0x10000` and `0x16F60–0x18000`, which are
  `0xFF` instead of `0x00` (10 864 bytes, all filler). So the PSP runs the SCPH-1001/5501/7001/
  101 kernel unmodified — including GetSystemInfo, the pre-exec ReadTOC/GetID hook and the RAM
  wipe. (The hook stays unarmed: nothing in the PSP shell writes `0xA000DFFC`.)
* Font `0x66000–0x7FE70` identical; glyph tail `0x7FE70–0x7FF2E` identical; `0x7FF2E/2F` = `FFFF`
  (retail `0000`); `0x7FF93–0x80000` zeroed (retail 4.5 has 0x6D bytes of glyph data there);
  `0x65CB7–0x66000` FF (retail: table). Version string `System ROM Version 4.5 05/25/00 J`,
  `Copyright 1993-2000`: the 4.5 A string with the letter changed to `J`.
* Closest retail by byte diff: tie between every K2 image (10 864 differing bytes below
  `0x18000`, all filler); the version string says 4.5 A; the shell is **not** the 4.5 shell.

### 6.2 The shell [V]

* `0x18000`: a new stub (`0x1B0` bytes): copies the LZ stream (`0x15F64` bytes from
  `0x181B0`, length word at `0x2E114`) to `0x80191000`, copies the decompressor (`0x118` bytes
  from `0x18088`) to `0x80190000`, FlushCache, jumps; the decompressor (same OSD LZ format:
  24-bit size, 30-token descriptors, `(h & (0x3FFF>>n))+1` offsets) writes `0x20F40` bytes to
  `0x80030000`, zeroes the packed buffer, FlushCache, `j 0x80030000`. Unpacked:
  `osd_unpack.py --raw PSXONPSP660.BIN 0x181b0`.
* The unpacked program is a **sibling of the PS2 rom0:LOGO shell**: `$Id: sys.c,v 1.140
  1998/01/12`, `$Id: intr.c,v 1.75 1997/02/07`, `Library Programs (c) 1993-1997`, the same
  TMD (`3d032a12`), VAB (`ea548f72`), 50x40 and 5x8 TIMs, the same libcd-style sector reader
  (`FUN_80030C90(count, lba, buf)`, mode 0x80, BCD MSF), the same event setup
  (`FUN_80030FD4`: six `OpenEvent(0xF0000003, …)` CD events). Size 0x20F40 vs LOGO's 0x210C0.
* Ghidra pass (204 functions) against the LOGO analysis in `analysis/ps1/logo/`:

| LOGO (1.60/2.00) | PSP | change |
|---|---|---|
| `shell_entry`: `FUN_800301BC` reads `0xBFC7FF32`, letter → PAL (0x200 lines) or NTSC, licence mode | `shell_entry` (96 bytes): no letter read, `0x1E0` lines hard-coded in `FUN_80030580` | **region logic removed; NTSC only** |
| `FUN_800300B0`: read sectors, `if (DAT_800510C0) strcmp(line, one of 3 strings) → -1` | `FUN_80030098`: read, then straight to drawing | **licence compare removed** (A(17h) strcmp no longer called) |
| `FUN_80030C40`: LBA 4 (text) + LBA 5–11 (disc TMD, 0x3278 bytes) + GetID (CD 0x1A) → `SystemErrorBootOrDiskFailure('B',0x38D)` on bad logo | `FUN_80030F50`: **LBA 4 only**, 1 sector, first line ≤72 chars to `0x8005E8D0` | **disc logo not read, GetID not issued, no error path** (A(A1h) gone) |
| `FUN_800302F0` draws the TMD from the disc buffer | `FUN_80030580` draws **the ROM's embedded TMD** (`FUN_80030934(&DAT_800406A0)`) | the PSP always shows Sony's logo, whatever the disc has |
| `FUN_800319F0` phase B: text from `0x80059500`, layout by length (70 → 6 lines, 64 → 5) | `FUN_80030110` + `FUN_800302E4`: same layout rule on `0x8005E8D0` | kept — the disc's own text is still displayed |
| `!!!WARNING!!! : Not PS Disk`, `Audio Disk !!`, `Shell Opened !!`, `System ROM Version 1.0`, `Copyright 1993,1994` | absent | TTY diagnostics and self-identification stripped |
| returns 0 after the screen | `FUN_80030060(0x78)`: 120 × VSync wait, then return 0 | **2 s hold added** |
| BIOS calls: A 13,17,1B,27,28,2A,3F,44,49,72,A1; B 7,8,B,C,17,18,19,3F,56,5B; C 2,3,A | A 13,1B,27,3F,44,49,72; B same; C same | no new syscalls; strcmp/bzero/memcpy/SystemError dropped |

* `0x2E118–0x38A00` of the ROM (after the packed stream, before the FF filler) is **raw,
  uncompressed shell data**: 76 % of its bytes match the unpacked shell at a constant offset
  (ROM − 0x168E0 = shell `0x17838…`), including the `Library Programs…` string at `0x37940`
  and the 50x40/5x8 TIMs at `0x35D80`/`0x36D60`. Nothing jumps or points there — build slack
  from the unpacked image left in the ROM file [V data, I interpretation].
* Not found: any `0x1F80xxxx` register outside the GPU/CD/SPU set the LOGO shell uses, any new
  string, any PSP-specific hook. The PSP host does its work on the emulator side; the BIOS
  image itself only lost checks [V for the shell; I for "the host does it"].

### 6.3 Reading of the whole image

PSXONPSP660 = `K2 kernel (unchanged)` + `LOGO-lineage shell minus every check` + `standard
font` + `4.5 A version string with letter J`. With letter `J` a 4.5-style shell would have
insisted on `Inc.` discs; the PSP shell sidesteps that by never looking at the letter.

---

## 7. The PS2's embedded PS1 BIOS vs POPS

| | PS2 1.00 J | PS2 1.50 dev | PS2 1.60 E / A / A2 | PS2 2.00 E | PSXONPSP660 |
|---|---|---|---|---|---|
| kernel | TBIN (new build, monitor 2.6, 1999) + SBIN — identical in all six | ← | ← | ← | retail K2 kernel untouched |
| kernel strings vs K2 | adds `Clear 0x10000 to 0x%x`, `LoadModuleByEE`; same `BOOTSTRAP LOADER Type C Ver 2.1 03-JUL-1994`, same TTY test strings (`test 6 button PAD…`) | | | | same as K2 |
| shell | LOGO 1.00 J build (`Inc.` only, check constant 1) | LOGO dev build (no licence string) | LOGO 1.60 build (3 strings, letter-driven) | = 1.60 | LOGO-lineage, all checks removed |
| shell draws | disc's TMD, compared to ROM TMD | disc's TMD | disc's TMD | disc's TMD | **ROM TMD** |
| GetID | yes | yes | yes | yes | **no** |
| region letter | `T` (treated as "other") | `A` (check off) | `E` / `A` / **`E`** | `E` | `J`, never read |
| font `KROM` | = PS1 | = | = | = | = PS1 |
| `KROMG` / tail table | PS1 block A edited, table zeroed | = | = | = | PS1 block A unchanged, table FF |
| version string | `5.0 01/17/00 T` | `5.0 12/28/00 A` | `5.0 10/04/01 E` … | `5.0 06/14/04 E` | `4.5 05/25/00 J` |
| post-screen | returns to TBIN → SYSTEM.CNF → Exec (ps1_boot §1) | | | | returns to the K2 kernel's own boot path (`cdrom:SYSTEM.CNF;1`) after a 2 s hold |

So the two hosts took opposite routes [V+I]: the PS2 rewrote the kernel side (TBIN/SBIN run
from ROM, no `0xA0000500` copy, ROMDIR-based loading) but kept the full licence/logo/GetID
screen with region logic; the PSP kept the 1995 kernel byte-for-byte and gutted the screen.
Across PS2 versions nothing PS1-related changed except the LOGO build (1.00 J → 1.50 dev →
1.60) and the version string: the 1.60 A2 (`SCPH39004`) ROM is the odd one, with an `E`
PS1-mode letter under an `A` ROMVER.

---

## 8. What a PSX Boot Inspection Tool should show (ranked)

1. **Identity card** from the dump alone: SHA-1; `0x100` BCD date; model string and `CEX`;
   version string, date and region letter from `0x7FF32` (or "none — 1.0"); kernel banner from
   the `PS-X Realtime Kernel` string; kernel/bootstrap cluster (K1/K2, B1–B4) by hashing
   `0x0–0x100 + 0x180–0x18000`; shell hash. For PS2 rom0: ROMVER vs VERSTR letter, TBIN/LOGO
   build hashes. For 4 MB inputs, show the PS1-mode pieces by ROMDIR, not "first 512 KB".
2. **Mislabel / duplicate / corruption detection**: identical SHA-1 under two names
   (5501 = 7003; three PS2 pairs); file-name region vs `0x7FF32` letter vs ROMVER (SCPH39004);
   size ≠ 512 KB with CRLF density > 0.3 % and header intact → "text-mode corrupted" (the `[h]`
   case) with the first shifted offset; version string not at `0x7FF32`.
3. **Check-policy view**: parse the shell entry for the licence flag store and the region
   function (`lui 0xBFC7 / ori 0xFF32`), list the licence strings and whether a reference TMD
   is embedded → "this BIOS will/won't compare disc text and logo; PAL/NTSC decided by …".
   Also whether the kernel's pre-exec ReadTOC/GetID hook exists (`0x1E`/`0x1A` sequence at
   `BFC0D600–BFC0D850`) and whether the shell arms it (`-0x2004` store).
4. **Host-modification view** for POPS and PS2-embedded images: diff against the K2 kernel
   (report "kernel unchanged / filler only"), unpack the shell, compare BIOS-call sets and
   the disc-read sequence with the reference LOGO/4.5 shells, list stripped strings, flag the
   edited letter and the raw build slack.
5. **Layout map**: kernel copy (`0x420`), shell copy (`0x6FF0`), packed-shell detection
   (stub + 24-bit size at `0x1804C`), content end vs filler, font/block-A/table presence —
   drawn as a 512 KB strip with clusters coloured by hash.
6. **Revision diff view**: normalised instruction diff between two dumps' bootstrap+kernel
   (the 2.0→2.2 diff is the showcase: ROM monitor out, GetSystemInfo/ReadTOC-GetID/RAM-wipe
   in) and asset diff for shells (TIM/TMD/VAB hashes with dimensions, new/removed strings).
7. **Asset browser**: TIMs (4bpp, dimensions), the logo TMD, the boot VAB — decoded from the
   user's dump, never shipped; shows which images share which art (the 1995 logo/sound set is
   identical through the PSP).
8. **Kernel facts** for the curious: GetSystemInfo table (`0xDCB0`), default TCB/EVENT/STACK
   (`0xE14C`), CD register table (`0xE620`), font mapping (`Krom2RawAdd`), POST stages.

---

## 9. Not determined / open

* Consumer of block A (`0x64000`) — no absolute reference in the kernel; likely shell-side [I].
* Purpose of 3.0 J's extra `sw zero → 0x00086A24` at reset, and of 4.0 J's `0x14` ROM-size
  field (no second SCPH-7000 dump to compare).
* Whether DTL-H3000 really equals SCPH-1001 (no dump of it here).
* The J-shell logo comparison (`0x38D` present; the TMD compare routine itself was not
  decompiled in this breadth pass — ps1_boot documents it for the PS2 build).
* Exact byte accounting of the `[h]` file (+1 795 = 1 792 CRs + 3 bytes of edits − 2 dropped
  CRs leaves 2 bytes unexplained; a full alignment was not pursued).
