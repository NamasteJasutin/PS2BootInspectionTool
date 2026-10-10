# The PS1 shell's scenes and assets: standalone BIOS vs the PS2's embedded copy

What a PlayStation draws and plays from power-on to the game (the SCE intro, the licence
screen, the no-disc menu), where every asset sits in a standalone 512 KB BIOS, how the code
finds it, and what it takes to point `ps2kit::ps1` at such a dump. Companion to
`notes/ps1_boot.md` (the PS2's `rom0:LOGO`, which is the licence screen only). The kernel's boot
decisions and the full version matrix are covered by other notes; this one is scenes + assets.

Tags: **[V]** read in the named dump at the given address, **[I]** inferred.

## Images used

| key | file (PCSX2 bios folder unless noted) | ROM version string (0x7FF32) | shell at 0x18000 | shell MD5 |
|---|---|---|---|---|
| `u30` (primary) | `scph5501.bin` (MD5 `490f666e…`) | `System ROM Version 3.0 11/18/96 A` | raw, 0x67FF0 bytes | `b7e72c06…` |
| `j10` | `Sony PlayStation BIOS (J)[SCPH-1000].bin` | (none — string absent) | raw | `31ffc78f…` |
| `u45` | `Sony PSone BIOS (U)(v4.5)(2000-05-25)[SCPH-101].bin` | `4.5 05/25/00 A` | packed (0x344B8 → 0x60990) | `ffe1de49…` |
| `u22` | `Sony PlayStation SCPH-1001 - DTLH-3000 BIOS v2.2 …` | `2.2 12/04/95 A` | raw | `337fcd79…` |
| `e30` | `Sony PlayStation BIOS (E)(v3.0)(1997-01-06)[SCPH-5502 + SCPH-5552].bin` | `3.0 01/06/97 E` | raw | `3c5251d1…` |
| `j40` | `Sony PlayStation BIOS (J)(v4.0)(1997-08-18)[SCPH-7000].bin` | `4.0 08/18/97 J` | packed (0x3200B → 0x5BDA0) | `e89fd12b…` |
| `ps2` | first 512 KB of `SCPH-70004_BIOS_V12_PAL_200.BIN` (repo root) | `5.0 06/14/04 E` | none (`rom0:LOGO` at 0xA560) | `158f741c…` (unpacked LOGO) |

Shell images are analysed at base **0x80030000** (file offset = address − 0x80030000). Ghidra
exports (`Ps1Pre.java` + `OsdExport.java`, `MIPS:LE:32`) were made for `u30`, `j10` and `u45`
in the scratchpad; `u22`, `e30`, `j40` were checked with a byte scanner and capstone only.
Previews rendered in the scratchpad (not committed): `sheet_u30.png`, `sheet_j10.png`,
`sheet_e30.png`, `sheet_u45.png`, `sheet_ps2_200e.png` (every TIM of each shell),
`sce_sheet.png` / `sce_t000..180.png` (the SCE intro re-rendered from the parameters below).

---

## 0. How the shell gets into RAM

* Every PS1 kernel seen copies **0x67FF0 bytes from 0xBFC18000 to 0x80030000**, flushes the
  cache and calls 0x80030000 (`lui a1,0xBFC1; lui a2,6; ori a2,0x7FF0; ori a1,0x8000; jal memcpy;
  lui a0,0x8003` at BIOS 0x6FFC in `u22`/`u30`/`e30`/`j40`/`u45`, 0x6F78 in `j10`) [V]. No
  ROMDIR, no size field: the shell is a fixed slice of the ROM.
* In 1.0–3.0 the slice *is* the shell (entry `addiu sp,-0x30` at 0x18000) [V].
* In **4.0 and 4.5 the slice is packed**: 0x18000 holds a 0x4C-byte stub (`lui a0,0x8003;
  addiu a0,0x4C; lui a1,0x8019; …` copy loop, `FlushCache` via A0:44h, `j` to the decompressor
  at the end of the copied payload: 0x801C200C in `j40`, 0x801C44B8 in `u45`), then the **same LZ
  stream format as the PS2's OSD/LOGO** (`u32` size, 30-token big-endian descriptors) starting
  at BIOS **0x1804C** [V]. `tools/osd_unpack.py --raw <bios> 0x1804c` unpacks both (`j40`:
  0x3200B → 0x5BDA0 bytes, `u45`: 0x344B8 → 0x60990). The PS2's `rom0:LOGO` is the same scheme
  with an 8-byte `{load address, size}` prefix (stream at module +0x54), which is why
  `ps2kit::ps1` unpacks at 0x54 [V].
* `PSXONPSP660.BIN` (4.5 J, PSP) uses a different two-stage stub at 0x18000 (copies two blocks to
  0x80191000/0x80190000, sizes read from 0x80046114/0x800301A0) and is **not** unpacked by the
  0x1804C rule [V]; out of scope here.
* The kanji font the shell reads at 0xBFC66000 (BIOS 0x66000, 0x19E70 bytes) is **byte-identical
  in all six PS1 dumps and in the PS2's `rom0:KROM`** [V]; the ROM version string is at 0x7FF32
  (absent, all zero, in SCPH-1000) [V].

---

## 1. Scene inventory of a standalone boot (`u30`, 3.0 A)

`shell_entry` (0x80030000) [V]:

```
read 0xBFC7FF32 → is_pal = last char == 'E'                     0x800307A8
prescale both note tables by 5/6 when PAL                       0x80040420
SPU init: reverb mode 6, master L/R 0x7F/0x6F, CD/ext in 0      0x8003FDE0
VAB open (0x80069000)                                           0x8003FEB8
SCE table tick 0 (keys the "sweep"), reverb volume 94           0x8003F940
── scene A: SCE intro ──                                        0x800404C0 → 0x80040520
VSync, ResetGraph(3), CD callbacks/events, pad                  0x8004ED4C, 0x80035C04, 0x80035DFC
MechaCon version ("System Controller ROM Version …") ≥ 95/07/06 0x80035F74
wait for the tray/disc state                                    0x800359D4(1)
PS disc present → ── scene B: licence screen ──                 0x80030694(1) → return to kernel
otherwise      → ── scene C: menu ──                            0x80030264 (loops until a disc boots)
```

Display for all three scenes: **640×480 interlaced, 15-bit**, draw env (0,0,640,480), disp env
(0,2,640,478); PAL adds `screen.x=3, screen.y=27` (flag at 0x80078068 = 1 selects the
interlaced path in the intro; a 640×240 double-buffered path exists but is not taken) [V].
Every frame wait is `VSync(0)` = one field.

### 1a. The SCE intro (0x80040520) [V]

Assets:

| what | address (`u30`) | format |
|---|---|---|
| "SONY" logotype | 0x80074C70 | TIM 4-bit + 16-colour CLUT, 240×48, navy on transparent |
| "COMPUTER ENTERTAINMENT" | 0x80076330 | TIM 4-bit, 240×60 |
| "TM" | 0x80077F90 | TIM 4-bit, 24×12 |
| diamond | none — three Gouraud polygons built in code (0x80040BB4) from parameters at 0x80078060 |
| intro music | note table 0x80074A80 (40 events + 9999 terminator) over the VAB at 0x80069000 |

Timeline (NTSC fields; PAL in brackets):

1. 8 fields black (two calls of a 4-frame clear/draw helper 0x8004788C). [`j10`: 4]
2. **Background fade** 0x800478DC(60 [50]): 61 [51] fields, the full-screen fill colour goes
   linearly from (0,0,0) to **(0xB4,0xB4,0xB4)** = 180 grey. Nothing else is drawn.
3. **Diamond loop**, 180 [150] fields: the diamond is drawn every field; a counter starts after
   the slide finishes and the loop ends when it reaches 120 [100].
   * Quad (POLY_G4, code 0x38, opaque): corners (−128,0) red, (0,−128) orange, (0,128) orange,
     (128,0) red, around screen centre (320,240). Size 0x100 (0x80078060), brightness 70 %
     (0x80078064): red = (70·255/100, 0, 0) = (178,0,0), orange = (178, 70·200/100 = 140, 0).
   * Two triangles (POLY_G3, 0x30), apex red, base orange: left = (0,0),(128,−128),(128,128)
     at (−128,0); right = (0,0),(−128,−128),(−128,128) at (128,0). They start as the two halves
     of the quad and over **T = 60 [50]** fields slide and shrink: position
     `(−128 + 80·t/T, −48·t/T)` / mirrored, scale `0x1000 − 0x8D2·t/T` (1.0 → 0.448), with
     `t` capped at T (0x800411F4). Drawn at OT[4], the quad at OT[5] (reverse OT, so the
     triangles are on top). The result is the SCE "S" diamond (`sce_sheet.png`).
   * From the field after the slide ends the three logotype sprites appear, centred at
     (320,80), (320,412) and (352,365) (sprite objects anchor on their centre, 0x80044824), and
     their CLUTs are **interpolated over 17 fields** from flat 22/31 grey (= the 180 background,
     so they fade in) to their real colours (0x800417A0: per 5-bit component
     `c·t/17 + 22·(17−t)/17`).
4. Exit: VSync, ResetGraph(3); the grey screen with the logo stays until the licence screen or
   the menu redraws — on a real console that is the CD spin-up time [I].

Total 249 [209] fields ≈ 4.15 s before the disc logic starts. No GTE, no texture other than
the three sprites, no fog.

Music (0x8003FA5C ticks the SCE table once per diamond-loop field; 0x8003F940 fires the
`t = 0` events before the intro and then jumps the clock to **89 [74]**, so events 30–89 fire on
the first loop field) [V]:

| t | prog | note | vel | pan | | t | prog | note | vel | pan |
|---|---|---|---|---|---|---|---|---|---|---|
| 0 | 1 | 36 | 127 | 80 | | 160 | 3 | 67 | 95 | 56 |
| 0 | 1 | 48 | 50 | 48 | | 165 | 3 | 62 | off | |
| 30 | 1 | 36, 48 | off | | | 169 | 1 | 29 | 127 | 48 |
| 90 | 3 | 50 | 35 | 40 | | 170 | 1 | 41 | 127 | 80 |
| 100 | 3 | 55 | 45 | 56 | | 170 | 3 | 62 | 55 | 72 |
| 105 | 3 | 50 | off | | | 175 | 3 | 67 | off | |
| 110 | 3 | 60 | 50 | 72 | | 180 | 3 | 64 | 60 | 88 |
| 115 | 3 | 55 | off | | | 185 | 3 | 62 | off | |
| 120 | 3 | 62 | 60 | 88 | | 190 | 3 | 67 | 90 | 72 |
| 125 | 3 | 60 | off | | | 195 | 3 | 64 | off | |
| 130 | 3 | 67 | 80 | 72 | | 199 | 1 | 29 | off | |
| 135 | 3 | 62 | off | | | 200 | 3 | 62 | 45 | 56 |
| 140 | 3 | 60 | 55 | 56 | | 200 | 1 | 41 | off | |
| 145 | 3 | 67 | off | | | 205 … 245 | 3 | 67/64/62 alternating, vel 50/80/30/45 | | |
| 150 | 3 | 62 | 60 | 40 | | 265 | 3 | 64 | off | |
| 155 | 3 | 60 | off | | | 9999 | end | | | |

Program 1 is the **"sweep"** (VAG 3, the 22 304-byte sample, centre note 60, ADSR 0x8088/0xDFF1):
notes 36 and 48 at `t = 0` are the rising whoosh under the black/grey fade, keyed off on the
first diamond field; notes 29 and 41 at 169–200 are the second, lower sweep under the text.
Program 3 (VAG 1, ADSR 0x8C7A/0xDFED) is the chime arpeggio, keyed every 5 fields. The table
is identical in all six PS1 shells [V] and absent from the PS2 [V]. When the menu or the
licence screen starts, 0x8003FB54 keys off prog 3 notes 62/64 and prog 1 notes 29/41 — the
voices this table leaves sounding [V].

### 1b. The licence screen (0x80030694) [V]

Byte-for-byte the same logic as the PS2's (`notes/ps1_boot.md` §4): disc check 0x8003EC40 reads
sector 4 (text, first line ≤ 72 chars), sectors 5–11 (TMD, 0x3278 bytes compared when the
check flag is on), GetID; phase A fade 0x80041B10 with the frame counter 60..90, fog near
`frame×133 − 6980` (0x85/0x1B44 at 0x80041D0C), GeomScreen 1024, back colour (100,100,100), far
colour black; phase B text 0x8003F1C0, 30 fields, wordmark RGB += 5 per field, 8 sprites; tail
until the note table's 9999 (0x80074BC8; identical to the PS2's) [V].

Differences from the PS2 build:

| | `u30` | PS2 LOGO |
|---|---|---|
| matrices | light 0x80078070, colour 0x80078090 (same values); rotation **computed at run time**: `RotMatrix({260, −30, 0})` from the SVECTOR at 0x800780C4, `TransMatrix((0,−340,5888))` from 0x800780CC; the 0x40-offset block the PS2 loader reads holds `{0x23E8, 0x1000, 0x1000, 0, frame, angles, translation}` instead of a baked matrix | baked (4092,0,−188 / −73,3775,−1590 / 173,1591,3771) — equals Rx(260)·Ry(−30) within ±1 [V, recomputed] |
| wordmark | two TIMs: **0x80066FA0 "PlayStation®"** (used when the check flag is set) and **0x80067F80 "PlayStation™"** (used when it is 0, i.e. on this 3.0 A) + TM sprite 0x80068F60 | one TIM (®) + TM sprite |
| sprite positions | centre-anchored: wordmark (328,290) 200×38, TM (364,403), text pieces at y 340 / 360, GetID letters (320,400) | top-left (228,280), (354,401), y 336/356 |
| licence strings | one: 0x8005AE60 `…Inc.` (64) | three (Euro pe / (Europe) / Inc.) |
| logo copy | **none in the image**; the check flag 0x80079E2C is stored 0 at 0x80030054 and never set, so the compare loop (still present at 0x8003ECFC with the `'B', 0x38D` hang) is dead — whatever the disc carries is shown | ROM copy at 0x8003F540, flag from the ROM letter |
| sound | same VAB (0x80069000, 0xBA80 bytes, identical), same drones (prog 2 note 31 / prog 0 note 36, 0x80040318) and key-offs (0x800403A0), reverb volume 94 | same |

### 1c. The no-disc shell (0x80030264) [V unless marked]

A data-driven screen machine: a 0x28-byte descriptor per screen at **0x800665F0**
`{label-TIM, item table, object table, static-sprite table, cursor table, n, 0, rect 640×480,
rect (0,480,640,32)}`, five screens: 0 main menu, 1 memory card, 2 (dialogue), 3 CD player,
4 CD player "no disc" [V]. Switching (0x80031A68) uploads the screen's label TIM and builds its
sprite objects; per-frame drawing is 0x80036400 / 0x80045110 [V]. **No music and no sound
effects**: the only key-on sites in the image are the intro, the licence screen and the
key-offs above (0x80052D98 callers) [V]. The CD player plays audio tracks through the drive
(`CdlPlay`, "cdPause() is Cancelled" strings) [V strings].

| asset | address (`u30`) | format | used for |
|---|---|---|---|
| "MAIN MENU" title plate | 0x8005AF20 | TIM 4-bit 176×47, blue radial frame | screen 0 (static-sprite table 0x80065028: at (540,67)?) |
| "MEMORY CARD" plate | 0x8005CA18 | 200×36 | screen 1 |
| "CD PLAYER" plate | 0x8005F824 | 176×47 | screen 3 |
| main-menu labels | 0x8005BF88 | 220×48 | "MEMORY CARD", "CD PLAYER" (each twice) |
| memory-card labels | 0x8005D868 | 248×130 | CARD1 CARD2 FILE EXIT COPY "COPY ALL" DELETE YES NO, Japanese "はい/いいえ"…, "Are you sure?" |
| CD-player labels | 0x8006088C | 256×108 | 1–20, "<20", EXIT, COMPLETE/SHUFFLE/PROGRAM/REPEAT ALL/1, "Please insert PlayStation CD-ROM." |
| no-disc message | 0x800623CC | 256×108 | "Please insert PlayStation CD-ROM." (screen 4) |
| cursor arrow | 0x8006672C | 32×32 purple 3-D arrow | pointer table 0x80066270 |
| cross | 0x80066CB8 | 32×32 purple "+" | pointer 0x80066F18, memory-card screen [I] |
| procedural textures | built on the GPU at menu start (0x80030D20) and read back as 16-bit TIMs: six 108×35 strips (0x80048E20 into 0x801A8800…), seven shaded discs (0x800491C4: Ø120 dark-blue→light-blue, five Ø56 in green/blue/red/orange/yellow, one Ø88 purple) and a 140×28 bar (0x80048990, (62,62,140)) | | the CD player's round buttons and the memory-card blocks [I] |
| save icons | from the memory card itself (`bu00:`/`bu10:` `__tmp_file`) | | |

The label TIMs' CLUTs are placeholders (two entries, index 1 = 0x0C63); the shell builds the
real colours at run time, and the texture pages carry the labels twice side by side plus
leftover rows [V by image, purpose I]. GTE for the menu: GeomOffset (320,240), GeomScreen 512,
back colour black, **far colour (128,128,255)** — the menu's 3-D elements fog toward light blue
[V, 0x80030D20].

---

## 2. Audio summary

* One VAB (`pBAV` v6, 4 programs / 8 tones / 3 VAGs, master 0x7F, 0xBA80 bytes) in every shell;
  **identical to the PS2's** at the byte level in all six PS1 dumps [V]. Program/tone/ADSR
  tables are in `ps1_boot.md` §4.4; program 1 (VAG 3), unused on the PS2, is the intro sweep.
* SPU programming: SsInit-style (24 voices), reverb mode 6 with reverb on, main volume
  0x3FFF/0x37BF, CD and external inputs 0 (0x8003FDE0), reverb output 94/127 (0x80040094(0x5E))
  set before the intro and again before the licence screen [V]. Two voices per note (the stereo
  tone pair). The intro keys at most 4 voices at once (two sweep notes + two chime notes × 2).
* Sequence: the two 8-byte event tables above, ticked per field, PAL ×5/6 [V].
* Coverage by `ps2kit::ps1`: `Vab::parse` and `render_sound` already play exactly this bank
  with the licence table; the intro needs only (a) the second table (same format, located by
  its first event `00000000 01 24 7F 50`), (b) the clock offset (fire `t ≤ 0` at field 0, then
  continue from 89/74 at the first diamond field), (c) the key-offs at the end. The libsnd volume
  arithmetic is still approximate (`ps1_boot.md` §6).

---

## 3. The PS2's embedded copy vs the standalone shells [V]

* `rom0:LOGO` contains **only the licence screen**: no SCE table, no logotype TIMs, no diamond
  code, no menu, no label TIMs, no `bu00:` strings. The 0x18000 slot of a PS2 rom0 is unrelated
  data (the kernel reads `LOGO` via ROMDIR instead).
* What survives is byte-identical: VAB, licence note table, light/colour matrices, logo TMD
  (0x3278 bytes, same in `j10`, `j40`, `u45`), KROM font, proportional-width table.
* **Closest standalone revision: 4.5 (PSone).** Only `u45` and the PS2 build have the three
  licence strings (`u45` at 0x8006C5E8/630/674; PS2 0x8003F010/058/09C), the letter rule
  (`'E'` → PAL; `'A'` → check off, else on; 0x80030A6C in `u45`), the ® wordmark + TM sprite
  (`u45` leaves the TM wordmark TIM at 0x8007C780 unreferenced) and the same sprite geometry.
  The PS2 build was then re-linked with newer libgpu (`sys.c 1.140 1998/01/12`, `intr.c 1.75`
  vs `sys.c 1.5 1995/06/02` in every PS1 shell), which also bakes the rotation matrix and
  moves every table (all offsets differ; nothing lines up).
* Offsets of the shared assets: VAB `u30` 0x80069000 / `u45` 0x8007D800 / PS2 0x80042850;
  licence table 0x80074BC8 / 0x800893C8 / 0x8004E2D0; light matrix 0x80078070 / 0x8008E370 /
  0x800427B8; width table 0x80078240 / 0x8008E540 / 0x8004F4C0; TMD — / 0x80069360 / 0x8003F540.

---

## 4. Differences across revisions [V]

| | 1.0 J (SCPH-1000) | 2.2 A / 3.0 A / 3.0 E | 4.0 J | 4.5 A (PSone) | PS2 LOGO |
|---|---|---|---|---|---|
| packing | raw | raw | LZ at 0x1804C | LZ at 0x1804C | LZ in `rom0:LOGO` +0x54 |
| intro black lead-in | 4 fields | 8 | 8 | 8 | no intro |
| intro fade/slide/hold | 61 / 60 / 120, no PAL variant | 61/60/120, PAL 51/50/100 | as 3.0 | as 3.0; diamond height ×0x133/0x100 on PAL; PAL uses larger logotypes | — |
| logotypes | SONY 240×48 0x800742E0, CE 240×60 0x800759A0, TM 24×12 0x80077600 | SONY 240×48 0x80074C70, CE 240×60, TM 24×12 | same set | NTSC: SONY 200×36 0x80089470, CE 200×44 0x8008A2C4, ® 24×10 0x8008E2A8 at (436,417); PAL: SONY 240×44 0x8008B438 at (338,69), CE 240×54 with ® baked 0x8008C918 at (338,448) | — |
| logo TMD in ROM | 0x80077744 | **none** (`u22`, `u30`, `e30`) | 0x80068780 | 0x80069360 | 0x8003F540 |
| licence/logo check | unconditional; prints "Polygon Data is O.K." / "wiz_text"; read errors also hang `'B'`,0x38D | flag constant 0 (dead code) | flag constant 1 | flag = letter ≠ 'A' | flag = letter ≠ 'A' (1.00 J: constant 1) |
| licence strings | Inc. (0x80059738) | Inc. (0x8005AE60 / 0x8005A850) | Inc. (0x8006BA08) | three | three |
| wordmark | ® only (0x800675B0) + TM 20×8 | ® 0x80066FA0 or ™ 0x80067F80 by the flag (™ on 2.2/3.0 A, E) | ® 0x80079A00 / ™ 0x8007A9E0 | ® 0x8007B7A0 (™ TIM present, unused) | ® |
| menu | Japanese: メインメニュー 172×47 0x8005ACE8, メモリーカード 136×34, CDプレーヤー 172×47, green pencil cursor 64×48 0x8005A684, "PlayStation規格のディスクではありません" 256×226 0x800613CC | English plates (3.0 A); **3.0 E has the redesigned menu** (144×142 ×4, 92×128, 64×120, 164×66, 232×102 atlases, run-time CLUTs) | old Japanese design (titles as 1.0) | redesigned menu (128×128 ×4, 144×48 ×5, 48×48 ×2, 192×48 ×2, 232×102; 256×100 "Please insert PlayStation format disc") | none |
| SCE / licence tables, VAB, KROM | identical in all | | | | VAB/licence table/KROM identical, SCE table absent |

Timings of the licence screen (31 + 30 + 22|13 fields) and the fog/colour constants are the
same in every build checked (0x85/0x1B44, 60..90, +5 per field) [V for `u30`, `j10`, `u45`; I
for `u22`, `e30`, `j40` from the shared code].

---

## 5. Loader plan for `ps2kit::ps1` (or a `psxkit` sibling)

**Detection** (`Ps1Bios::detect(&[u8]) -> bool`): length == 0x80000, no `RESET\0…` ROMDIR entry
(every PS2 rom0 has one; no PS1 dump does), `b"Sony Computer Entertainment Inc."` at 0x108
(all seven PS1 dumps incl. PSP; the word at 0x100 is a BCD date), kernel string
`PS-X Realtime Kernel` present. Version = NUL-terminated string at 0x7FF32 (may be empty:
SCPH-1000), region letter = its last character [V].

**Shell image** (`fn shell_image(bios) -> Result<Vec<u8>>`): `s = bios[0x18000..0x7FFF0]`; if
`s[0..4] == 3C 04 80 03` (the packed stub's `lui a0,0x8003`) → `rom::unpack(s, 0x4C)`; else raw.
Reject the PSP stub (`lui a1,0x8019; ori a1,0x1000` first) with a clear error.

**Refactor `Ps1Shell::load`** into `Ps1Shell::locate(image, krom, rom_version)` plus two front
ends: `load(rom: &RomDir)` (today's path: `unpack(LOGO, 0x54)`, `KROM`, `ROMVER`) and
`load_ps1(bios)` (image as above, `krom = bios[0x66000..0x7FE70]`, version from 0x7FF32).
The existing magic locators already hit every PS1 shell: `pBAV`, the licence table's first event
`00000000 00 1F 50 40`, the light matrix `(0,0,4000),(0,−4000,0),(−4000,0,0)` (colour matrix at
+0x20 in all), the width table `{0,16},{2,8},{0,16}`, the 4-bit TIM scan [V]. Changes needed:

1. **Rotation/translation**: if `lo+0x40` is not a plausible matrix (`rotation[0][0] > 4096`,
   the standalone block starts `E8 23 00 00`), read the SVECTOR at `lo+0x54` (`i16 ax, ay, az`)
   and the VECTOR at `lo+0x5C` (`i32 tx, ty, tz`) and build `Rx(ax)·Ry(ay)·Rz(az)` in 4.12
   (angles in 1/4096 turn). Verified to reproduce the PS2's baked matrix within ±1.
2. **Logo TMD optional**: `logo: Option<Tmd>`; 2.2/3.0 shells have none (the disc's is shown
   as-is). Keep the 0x3278 compare only when present and the check flag applies.
3. **Check flag per build**: expose `checks_licence()` as `LicencePolicy { Always, Never,
   ByLetter }` chosen from the ROM version: 1.0 → Always, 2.x/3.x → Never, 4.0 → Always,
   4.5/PS2 → ByLetter.
4. **Wordmark variant**: collect both 200×40 TIMs when present; ™ is shown by 2.2/3.0 when the
   check is off, ® otherwise; 4.x/PS2 always ®.
5. **Licence strings**: scan for every `"          Licensed  by"` (1 or 3) rather than assuming
   three; the length switch in `text_layout` already covers 64/67/70.
6. **SCE intro** (new `SceIntro` struct): logotype TIMs by size (240×48/240×60/24×12 in
   1.0–4.0; 200×36/200×44/24×10 NTSC and 240×44/240×54 PAL in 4.5), the note table by its first
   event `00000000 01 24 7F 50`, and constants (size 256, brightness 70, grey 180, T 60/50, fade
   17, hold 120/100, lead-in 8/4) hard-coded with the revision rules above — they are literals
   in code, not data. Rendering needs only Gouraud triangles and sprites (the app's existing
   flat-triangle path plus per-vertex colour) and the CLUT fade.
7. **Menu**: expose the label/title/cursor TIMs and the screen table as an asset browser; a
   faithful re-render needs the GPU-generated textures and the per-screen object tables and is
   a separate, larger job.
8. Keep `LicenceTimeline` as is; add `SceTimeline { lead_in, fade, slide, hold, text_fade }`.

---

## 6. Tool-worthy, ranked (value ÷ effort)

1. **Detect + load a standalone PS1 dump and show its licence screen** — reuse of everything
   (TMD lit triangles, font, VAB, timeline); only the matrix-from-angles and optional-TMD changes.
2. **SCE intro re-render with sound** — three sprites, three Gouraud polygons, one CLUT fade,
   one note table over the already-decoded VAB; the most recognisable PS1 scene and absent
   from the PS2 entirely.
3. **Revision inspector** — version string, packed/raw, check policy, which wordmark, which
   licence strings, whether a ROM logo copy exists (2.2/3.0 show the disc's logo unchecked).
4. **Asset browser** — every TIM with its VRAM/CLUT placement, both note tables, the VAB with
   per-program use (sweep / drones / chime), the KROM font, the width table.
5. **Menu screens** — titles, labels, cursor and the screen table as a static mock-up; full
   emulation of the procedural textures and the state machine last.
6. **PSP 4.5 J stub** — only if a user has it; needs its own unpacker.

**Not determined**: the exact purpose of each GPU-generated texture in the menu; the redesigned
(3.0 E / 4.5) menu atlases' real CLUTs; the on-screen placement of the main-menu plate (the
static-sprite table's `0x21C` field is unexplained); the 240-line path's use (flag constant 1).
