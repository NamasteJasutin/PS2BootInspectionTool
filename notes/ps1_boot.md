# The PlayStation 1 boot screens as a PS2 shows them

What a PS2 draws (and plays) between the browser and a PS1 game's first frame, reverse-engineered
from `rom0:LOGO` of the 2.00 E ROM (`SCPH-70004_BIOS_V12_PAL_200.BIN`), with `rom0:TBIN` read
only as far as needed. Everything an implementer needs to re-create the screens from the user's
own BIOS and disc image is here; the BootScreen app ships none of this data (see
`notes/hidden_features.md` §6 for how LOGO is packed).

Subject image: `extracted/ps1/logo_30000.bin` = `tools/osd_unpack.py --raw rom0:LOGO 0x54`
(0x210C0 bytes, MD5 `158f741c…`). Absolute addresses in the image are `0x8003xxxx`–`0x8005xxxx`
(1222 `lui 0x8005`, pointers such as `0x800316EC` in data), so the analysis base is
**0x80030000**; file offset = address − 0x80030000. The 1.60 E (30004R) and 1.60 A (39001) LOGO
images are byte-identical to it. Ghidra output: `analysis/ps1/logo/` (shell, R3000,
`tools/ghidra/Ps1Pre.java`), `analysis/ps1/tbin/` (PS1 kernel), `analysis/ps1/logo_j100/`
(the 1.00 J build, for §5). `tools/r3000dis.py` is a capstone disassembler used for the spots
where the decompiler was unclear.

Tags: **[V]** verified against this image / these discs, **[I]** inferred.

---

## 1. The chain on a PS2

```
OSDSYS LaunchPs1Disc (0x202D50, osdsys_flow §2.5)
  └─ LoadExecPS2("rom0:PS1DRV", 2, {title id, VER})                                   [V]
PS1DRV  (EE ELF at 0x200000, "PlayStation Driver Version 1.1.0", built Aug 19 2003)    [V]
  ├─ reads cdrom0:\SYSTEM.CNF;1 itself (title-ID compat table of 182 SxxS_ ids),
  │  programs the GS for PS1 output (PGIF, "GS_TS4, NTSC, (NO-)INTERLACE") and
  │  forwards the PS1 kernel's TTY ("TTY: …" strings)                                   [V strings]
  └─ reboots the IOP into PS1 mode                                                      [I: see below]
IOP RESET (0xBFC02000): PRId ≥ 0x10 and 0xBF801450 bit 3 set → romdir_lookup("TBIN") → jump  [V, boot_sequence §2.1]
TBIN  (0x4B800 in rom0 = 0xBFC4B800, runs in place; "PS-X Realtime Kernel Ver.2.5",
       "BOOTSTRAP LOADER Type C Ver 2.1", 1993-1999)                                    [V]
  ├─ stage 1: romdir "SBIN" as an ECOFF (magic 0x162) → .text/.data copied to their
  │  link addresses, .bss zeroed, entry called (SBIN = the kernel's device/pad half,
  │  "PS-X Control PAD Driver Ver 3.0", tty, cdrom device)              0xBFC529C4      [V]
  ├─ kernel init, "KERNEL SETUP!", event/TCB tables, memory clear
  ├─ stage 7, 0xBFC52AFC: romdir "LOGO" as ECOFF (fails: not ECOFF) then as a raw
  │  {u32 load address, u32 size} + payload file: 0x1468C bytes copied to 0x30000 and
  │  called. The payload is the 0x44-byte stub + LZ stream; the stub copies itself to
  │  0x80190000, decompresses the shell over 0x30000 and jumps to 0x30000 (= 0x80030000
  │  cached). Without a LOGO entry the kernel runs a no-shell fallback (0xBFC4BB00).   [V]
  ├─ LOGO/shell runs: §2–§4. It returns 0 only once a valid PS1 disc has been shown.
  └─ stage 8: open cdrom:SYSTEM.CNF;1 (0xBFC52148): "setup file : %s" → parse TCB/EVENT/
     STACK/BOOT (0xBFC4C210); missing/unreadable → defaults {TCB 4, EVENT 0x10,
     STACK 0x801FFF00} (12 bytes at 0xBFC592E8) and boot file "cdrom:PSX.EXE;1";
     "boot file : %s"; "Clear 0x10000 to 0x%x" (user RAM wiped up to the stack);
     LoadExec. The screen keeps whatever the shell left in VRAM until the game draws.   [V]
```

PS1 mode on the IOP: the condition RESET tests is bit 3 of 0xBF801450 [V]. PS1DRV writes IOP
registers directly from the EE (0x1F801014/1404/140C/1414/1420/1470/1472, SPU 0x1F801C00,
SPU2 0x1F9007C0…) but no write to 0x1450 was found in it, nor in IOPBOOT/LOADCORE/UDNL (they
only mask bit 0); how the bit gets set (MechaCon or SBUS side) was not determined [I]. The
switch is not needed by an implementer — only its effect: TBIN runs instead of IOPBOOT.

The PS2's PS1 "BIOS ROM" at 0xBFC00000 is rom0 itself: the shell reads its region letter from
**0xBFC7FF32**, which in rom0 is the `VERSTR` file (offset 0x7FF30, "System ROM Version 5.0
06/14/04 E" + "Copyright 1993-1999 (C) Sony Computer Entertainment Inc."), i.e. the same
string a PS1 BIOS keeps at the end of its 512 KB [V].

---

## 2. The PS1 disc system area (sectors 0–15)

Read in `0x80030C40` (disc check) with the libcd-style reader `0x80030ECC(count, lba, buf)`
(LBA+150 → BCD MSF, CdlSetloc, read `count` 2048-byte sectors in mode 0x80 = double speed) into
the uncached buffer `0xA0010000` [V]. Only three things are read:

| What | Sectors | Bytes used | Where it goes |
|---|---|---|---|
| Licence text | LBA 4 (00:02:04), 1 sector | first line only | `0x80059500` (max 72 chars + NUL) |
| PlayStation logo (TMD) | LBA 5–11, 7 sectors read | first **0x3278** (12 920) bytes | stays in the read buffer, drawn from there |
| Disc ID | CD command 0x1A GetID | 8-byte response | `0x800510C8` ← bytes 4–7 ("SCEx") |

Sectors 0–3 and 12–15 are never read [V]. On both test discs sectors 0–3 are '0'/'\n' filler
(Japan) or zeros (Europe); 12–15 are zeros; sector 11 after the TMD is 0xFF filler (1416
bytes) [V, Tekken E/J].

### 2.1 Sector 4 — licence text [V]

Mode 2 Form 1 user data (raw 2352-byte sector: offset 24, subheader `00 00 08 00`). The shell
copies bytes from offset 0 up to the first `\n` or NUL, stopping after 72 characters, and
`strcmp`s the result (A0:17h) with one of three strings embedded in the shell:

| Address in image | Length | Text (10 spaces, `Licensed  by` with two spaces, 10 spaces, then) |
|---|---|---|
| `0x8003F010` | 70 | `Sony Computer Entertainment Euro pe   ` (note "Euro pe" + 3 trailing spaces) |
| `0x8003F058` | 67 | `Sony Computer Entertainment(Europe)` |
| `0x8003F09C` | 64 | `Sony Computer Entertainment Inc.` |

(No America string: see §5.) What follows the first line is ignored; the two discs seen:
Tekken (Japan): `…Inc.` + `\n`, then 31 lines of `0` characters (62–63 + `\n` each) filling the
2048 bytes; Tekken (Europe, 1995): `…(Europe)` + NUL, zeros to the end.

### 2.2 Sectors 5–11 — the logo, a PS1 TMD [V]

The data at sector 5 offset 0 is a standard libgs **TMD** (3-D model) file; the shell's parser
`0x800307DC` reads it as follows (all little-endian, offsets from the start of sector 5):

```
0x00  u32 id        0x41                      (not checked)
0x04  u32 flags     0                         (offsets relative to 0x0C; not checked)
0x08  u32 nobj      1                         (not checked; only object 0 is used)
0x0C  object 0:
      +0x00 u32 vert_top     0x231C  → vertices at 0x0C + vert_top   = file 0x2328
      +0x04 u32 n_vert       337
      +0x08 u32 normal_top   0x2DA4  → normals at 0x0C + normal_top  = file 0x2DB0
      +0x0C u32 n_normal     153
      +0x10 u32 primitive_top 0x1C   (the parser assumes 0x1C: primitives at file 0x28)
      +0x14 u32 n_primitive  560     (this is the loop count)
      +0x18 u32 scale        7       (ignored)
0x28  primitives, 560 × 16 bytes:
      u32 header   olen=4, ilen=3, flag=0, mode=0x20   → bytes 04 03 00 20
      u32 colour   r, g, b, mode-copy (0x20)
      u32          normal index (lo16) | vertex 0 index (hi16)
      u32          vertex 1 index (lo16) | vertex 2 index (hi16)
0x2328  vertices, 337 × 8: s16 x, y, z, pad     (x −611..608, y −775..231, z −943..594; +y is down)
0x2DB0  normals, 153 × 8: s16 nx, ny, nz (4.12), pad
0x3278  end — exactly the number of bytes compared
```

The parser also accepts mode 0x30 records (5 words: colour, then three normal|vertex pairs; the
three normals are averaged and the mode byte is rewritten to 0x20), but this logo uses only mode
0x20. Nothing else is validated; vertex indexes are not range-checked. Each primitive becomes a
0x24-byte entry at `0x80059550` (3 × {x,y,z}, the normal, the colour word) and a POLY_F3 packet
(0x14 bytes) at `0x80051120` / `+12000` per frame buffer (room for 600).

Colours used (r,g,b): 0xFF,0x08,0x08 red (191 tris), 0xF7,0xBD,0x00 yellow (126),
0x42,0x6B,0xAD blue (134), 0x39,0xA5,0x94 teal (108), 0xC8,0xC8,0xC8 grey (1) [V].

**Validation:** when the licence check is enabled (§5) the shell compares the 0x3278 bytes read
from the disc byte-for-byte with its own copy of the same TMD at **`0x8003F540`** (image offset
0xF540). The first mismatch calls A0:A1h `SystemErrorBootOrDiskFailure('B', 0x38D)`, which in
TBIN's ROM A0 table (`0xBFC4FC90`, entry 0xA1 = `0xBFC58AC0`) is a stub that re-dispatches
through the 0xA0 vector and never returns: the console hangs on a black screen before anything
is drawn [V]. The copy that is *drawn* is the disc's (pointer `0x8004282C`, initialised to
`0x8003F540`, overwritten with `0xA0010000` before the read), so on consoles that skip the check
(§5) whatever the disc carries is shown. Both Tekken discs carry a TMD identical to the ROM copy
[V]. Implementers can therefore take the logo from either the disc (sectors 5–11) or the user's
BIOS (`LOGO` unpacked, 0xF540, 0x3278 bytes) — they must be the same for a licensed disc.

### 2.3 GetID [V]

CD command 0x1A; the INT2 (success) response of 8 bytes {stat, flags, type, atip, 'S','C','E',x}
is copied to a stack buffer and bytes 4–7 go to `0x800510C8..CB` with a NUL at `0x800510CC`; if
byte 4 is 0 it becomes a space. On INT5 (error) the shell only prints `!!!WARNING!!! : Not PS
Disk` / `Audio Disk !!` / `Shell Opened !!` (A0:3Fh printf, so TTY only) and returns −1; the
caller retries forever (§5). The four letters are displayed on the licence screen (§4).

---

## 3. The SCE boot screen — not in this shell [V]

The PS2's LOGO is the PS1 shell **without** the "Sony Computer Entertainment" diamond screen.
The image contains exactly one TMD (the PS logo, `0x8003F540`), two TIMs (`0x8004E370` the
200×40 "PlayStation" wordmark, `0x8004F350` a 20×8 "TM" mark), one VAB (`0x80042850`) with
three samples, and one note-event table (`0x8004E2D0`); no other model, bitmap or sequence
exists, and no code path draws anything before the disc check. The same is true of the 1.00 J
and the devkit 1.50 builds (same asset set at slightly different addresses). So on a PS2 the
first thing that appears after the black screen is the PS logo of §4. The "ascending chime"
belongs to the licence screen (§4.4); the SCE-diamond sweep sound is absent. The frame counter
of the logo screen starts at 60 rather than 0, which is probably where the removed screen used
to live [I].

---

## 4. The licence screen

Entry: `0x80030000` → region/video setup (`0x800301BC`, §5) → SPU/libsnd init (`0x800315A0`:
SsInit, 24 voices, reverb mode 6 with reverb enabled, SPU master volume L 0x3FFF R 0x37BF,
CD/external inputs at volume 0, SPU unmuted) → VAB open+transfer (`0x80031678`) → CD events
(`0x80031220`) → loop on `0x800300B0` until it returns 0.

`0x800300B0`: disc check (§2) → licence string check (§5) → reverb output volume 94/127
(`0x8003184C(0x5E)` → `0x80037438` writes 94×0x7FFF/0x7F = 0x5EBB to vLOUT/vROUT
0x1F801D84/86; setting the reverb mode had zeroed them) → **phase A** logo fade (`0x800302F0`)
→ **phase B** text (`0x800319F0`) → **phase C** wait for the jingle (`0x800313F0` loop) →
ResetGraph(3) → return 0.

All timings are in VSync(0) waits = video **fields** (1/60 s NTSC, 1/50 s PAL; the display is
interlaced).

### 4.1 Display setup [V]

* Video mode from the ROM letter: 'E' → SetVideoMode(PAL); otherwise NTSC.
* ResetGraph(0); ClearImage(0,0,640,480 (NTSC) or 512 (PAL)) → black.
* Two "frame buffers" that are the **same** VRAM area: draw env (0,0,640,480), disp env
  (0,2,640,478); interlaced, 15-bit. Phase B switches the draw env to (0,2,640,478) and sets
  its tpage to GetTPage(4-bit, 640, 0). `isbg` = 0 (no automatic clear).
* PAL only: disp.screen.x = 3, disp.screen.y = 27 (otherwise 0,0); disp.screen.h = 236 in
  phase A ((480−8)/2) and 239 in phase B ((480−2)/2). libgpu's PutDispEnv then issues the
  usual GP1 display-range commands.
* GTE: InitGeom, SetGeomOffset(320, 240), SetGeomScreen(1024), SetBackColor(100,100,100),
  SetFarColor(0,0,0), colour matrix `0x800427D8`, light matrix `0x800427B8`, rotation+
  translation matrix `0x800427F8`:

| Matrix (4.12) | Rows |
|---|---|
| Light directions | (0, 0, 4000), (0, −4000, 0), (−4000, 0, 0) |
| Light colours (rows R,G,B; columns light 1..3) | (4000, 3000, 4000) × 3 |
| Rotation | (4092, 0, −188), (−73, 3775, −1590), (173, 1591, 3771) ≈ −22.9° about X, −2.6° about Y |
| Translation | (0, −340, 5888) |

The logo never moves; projected it spans roughly x 214–426, y 47–218 of the 640×480 frame [I
from the matrices and vertex extents].

### 4.2 Phase A: the logo fades in, 31 fields [V]

Frame counter `0x80042828` runs 60..90 inclusive. Before the loop: the two drone notes are
keyed on (§4.4) and SetDispMask(1) is issued (the display was blanked by ResetGraph). Per frame:

1. swap buffer, ClearOTagR(1024 entries), add a black full-screen POLY_F4 at ot[1023].
2. `SetFogNear(a, 1024)` with **a = frame × 133 − 6980** (frame 60 → 1000, frame 90 → 4990);
   DQB stays 0x1400000. In GTE terms the depth cue is p = 1.25 × (1 − a / SZ) clamped to 0..1,
   so a polygon at depth SZ is fully black while a ≤ SZ/5 and fully lit when a ≥ SZ. With the
   logo at SZ ≈ 4900–6500 the first frame is black and the last frame still darkens the
   farther (upper) parts by up to ~1.25 × (1 − 4990/SZ) — the shading the real screen has.
3. per primitive: RotAverageNclip3 (RTPT, NCLIP; back-facing triangles are dropped; OTZ via
   AVSZ3 with ZSF3 = 0x155), NormalColorDpq (NCDS: single normal, three directional lights +
   ambient 100/256, times the TMD colour, then interpolated toward black by p); otz >> 2 must be
   1..1023 → AddPrim(ot[otz]). Flat POLY_F3, no texture, no blending.
4. DrawSync(0), VSync(0), PutDrawEnv, PutDispEnv, DrawOTag(reverse OT).

After the loop the drone notes are keyed off (`0x80031904`). Nothing else is drawn yet.

### 4.3 Phase B: wordmark fade + text, 30 fields [V]

Nothing clears the screen and the logo is not redrawn: it stays in VRAM from phase A. Per
frame the OT holds only SPRT packets (at ot[5]), each a 4-bit textured sprite with
semi-transparency off; VSync(0), PutDispEnv, PutDrawEnv, DrawOTag, then one sequencer tick
(§4.4). Sprite 0's RGB goes 0, 5, 10, … 145 over the 30 frames (byte increments of 5; 128 is
"neutral", so it ends 13% over-bright, saturating white); every other sprite is drawn at RGB
128 from the first frame.

Sprite table `0x8004F3E0`, 8 entries × 0x1C: {u32 TIM pointer, RECT clut (x,y,w,h), RECT image
(x,y,w,h in 16-bit VRAM units), u16 x, y, w, h on screen}:

| # | Source | VRAM image rect | Screen x,y | w×h | Shown when |
|---|---|---|---|---|---|
| 0 | TIM `0x8004E370`: "PlayStation" wordmark, 200×40, 4-bit, 16-grey CLUT (white→black, index 15 transparent) | (640,0) 50×40 | 228, 280 | 200×40 | always; this is the one that fades in |
| 1 | TIM `0x8004F350`: "TM", 20×8, 4-bit | (640,40) 5×8 | 354, 401 | 20×8 | GetID gave a region string |
| 2 | text: the 4 GetID letters (e.g. `SCEE`) | (640,48) 16×16 | 288, 396 | 64×16 | GetID gave a region string |
| 3 | text: licence chars 0–15 | (640,64) 64×16 | 221, 336 | 256×16 | always |
| 4 | text: chars 16–31 | (640,80) 64×16 | 333, 336 | 256×16 | always |
| 5 | text: chars 32–47 | (640,96) 64×16 | **118** / 154 / 128, 356 | 256×16 | always |
| 6 | text: chars 48–63 | (640,112) 64×16 | **306** / 342 / 316, 356 | 256×16 | always |
| 7 | text: chars 64–79 | (640,128) 64×16 | **482** / – / 508, 356 | 96×16 | 70- or 67-char text only |

The x of entries 5–7 depends on the length of the sector-4 line (`0x80031CD8`): 70 chars
("Euro pe", or "Amer  ica" on an American disc) → 118/306/482 and 8 sprites; 64 ("Inc.") →
154/342 and 7 sprites; 67 ("(Europe)") → 128/316/508 and 8; any other length → 7 sprites at
the 70-char positions. CLUTs sit at (640,480) wordmark, (640,481) TM, (640,482) text; the text
tpage is GetTPage(4-bit, 640, 0).

**Text rendering** (`0x80032040`, called per 16-char piece): each ASCII character is converted
to full-width Shift-JIS (`0x8003DB30`: space → 0x8140, '.' → 0x8144, '(' ')' → 0x8169/816A,
digits → 0x824F.., 'A'.. → 0x8260.., 'a'.. → 0x8281..), mapped to a glyph index (`0x8003DC70`:
0x8140–0x84BE → 0..523 in JIS order skipping the gaps, 0x889F–0x9872 → the block at
0xBFC69D68) and fetched from the kernel's kanji font ROM at **0xBFC66000 = rom0:KROM**: 30 bytes
per glyph = 15 rows of 16 big-endian bits. Glyph indexes 0x93–0xD0 (the full-width digits,
A–Z, a–z) and the space use a proportional table at `0x8004F274 + index×4` {u16 left-shift,
u16 advance}: digits/capitals 16 px except '1' (shift 2, 8), 'I' (5, 8), 'J' (1, 12); lower-case
12 px except 'i' 'j' 'l' 'r' 8, 'm' 'v' 'w' 16; space (entry 0xD1) shift 5, advance 4; every
other glyph 16 px, no shift. The row bits are shifted left by `shift`, each pixel is ORed with
its left neighbour (1-px bold), and the next glyph overwrites from `advance` on, so a glyph is
effectively clipped to its advance. Output is a 4-bit TIM whose CLUT entry 0 is 0 (transparent)
and entry 1 is **0xDAD6** (15-bit r=g=b=22, i.e. light grey ≈ 181), 256 px wide per 16-char
row, 16 rows. The piece x positions above are exactly where the proportional text of the
previous piece ends (e.g. "Sony Computer En" = 188 px: 118 + 188 = 306), which is why
"Euro pe" carries a space: piece 4 "tertainment Euro" is 180 px, 306 + 180 = 486, and piece 5
" pe   " is placed at 482. An implementer can render the whole line proportionally from x = 221
(line 1, y 336) and from 118/154/128 (line 2, y 356) and get the same result to within the 4 px
overlap of the 70-char case.

### 4.4 Sound [V]

One **VAB** (libsnd bank) at `0x80042850`: header "pBAV" v6, 4 programs, 8 tones, 3 VAGs,
master volume 0x7F; the VH is 0x1220 bytes (32 + 128×16 program table + 4×16×32 tone table +
256×2 VAG sizes), the VB (headerless SPU-ADPCM, 16-byte blocks) follows at `0x80043A70` and ends
at `0x8004E2D0` (0xA860 bytes; the header's own "waveform size" field says 0xBA80 but the VAG
size table is the authority):

| VAG | Address | Bytes |
|---|---|---|
| 1 | `0x80043A70` | 13 472 |
| 2 | `0x80046F10` | 7 328 |
| 3 | `0x80048BB0` | 22 304 |

Programs (each two tones: tone 0 pan 0 left, tone 1 pan 0x7F right, right tone fine-shift
0x14; tone mode 4 = reverb on; key range 0–127):

| Prog | VAG | Centre note | Tone vols | Prog vol | ADSR1 / ADSR2 | Used by |
|---|---|---|---|---|---|---|
| 0 | 1 | 72 | 0x7F / 0x7F | 0x78 | 0xB8FF / 0xCFAE | drone (phase A), bass (sequence t=0) |
| 1 | 3 | 60 | 0x7F / 0x5A | 0x7F | 0x8088 / 0xDFF1 | not keyed by the shell |
| 2 | 2 | 54 | 0x7F / 0x5A | 0x64 | 0xB5FF / 0xDFE0 | drone (phase A) |
| 3 | 1 | 72 | 0x7F / 0x78 | 0x64 | 0x8C7A / 0xDFED | the ascending chime |

Key-on helper `0x80036388(0, vab, prog, note, vel, pan)`: keys every tone of `prog` covering
`note` (two voices per note) with the velocity scaled by the tone and program volumes (libsnd
arithmetic, not decoded exactly); velocity 0 = key-off; `0x8003748C` flushes to the SPU. The
pitch follows libsnd: SPU pitch 0x1000 at the tone's centre note, 2^(Δsemitone/12) elsewhere.
SPU common settings (`0x800315A0`, old-libspu masks mapped to registers in `0x80038924`):
main volume 0x1F801D80/82 = 0x3FFF / 0x37BF (R is 0x6F/0x7F of L), reverb mode 6 of the
10-mode table at `0x8004F938` (0x44 bytes of reverb registers per mode; 6 = libspu SPACE [I
name]) with SPUCNT reverb on, reverb output volume 0x1F801D84/86 = 0x5EBB/0x5EBB from the
start of phase A, CD/external input volumes 0.

* **Phase A**, before the first frame: prog 2 note 31 vel 0x7F pan 0x40 and prog 0 note 36
  vel 0x3C pan 0x40 — the low "boom" (pitches 0x1000×2^(−23/12) ≈ 0x43B and 0x200). Keyed
  off after the 31st frame.
* **Phase B/C**, the sequence table at `0x8004E2D0` (8-byte events {u32 time, u8 prog, note,
  vel, pan}, terminator time 9999), ticked once per field from the first text frame, PAL times
  pre-scaled by 5/6 (`0x80031984`):

| t (NTSC fields) | prog | note | vel | pan |
|---|---|---|---|---|
| 0 | 0 | 31 | 80 | 64 |
| 12 | 3 | 48 | 50 | 40 |
| 16 | 3 | 58 | 55 | 56 |
| 20 | 3 | 63 | 65 | 72 |
| 22 | 3 | 58 | off | |
| 24 | 3 | 65 | 70 | 88 |
| 25, 26 | 3 | 48, 63 | off | |
| 28 | 3 | 70 | 80 | 72 |
| 30 | 3 | 60 | 60 | 56 |
| 31 | 3 | 65 | off | |
| 34 | 3 | 65 | 74 | 40 |
| 36, 38 | 3 | 60, 70 | off | |
| 40 | 3 | 67 | 78 | 56 |
| 41 | 3 | 65 | off | |
| 44 | 3 | 72 | 70 | 72 |
| 48, 50 | 3 | 67, 72 | off | |

### 4.5 How long, and what ends it [V]

| | NTSC fields | PAL fields |
|---|---|---|
| Phase A logo fade | 31 | 31 |
| Phase B text (30 sequencer ticks) | 30 | 30 |
| Phase C: ticks until the terminator, each after a VSync, plus one final VSync | 22 | 13 |
| **Total from first logo frame to shell exit** | **83 (≈1.38 s)** | **74 (≈1.48 s)** |

Phase C ends when the sequencer reaches the 9999 entry (tick 51 NTSC, 42 PAL); no input is
read. The shell then ResetGraph(3)s, closes its events and returns 0 to the kernel, which reads
SYSTEM.CNF and loads the PS-X EXE (§1) while the licence screen stays on the display; the
sustained notes keep decaying until the game resets the SPU. Before the first logo frame the
screen is black for as long as the three CD operations take (drive-dependent).

---

## 5. Region behaviour

### 5.1 The ROM letter decides everything (`0x800301BC`) [V]

The last character of the string at 0xBFC7FF32 (rom0:VERSTR):

| Letter | Video | Licence/logo check | Accepted sector-4 line |
|---|---|---|---|
| `E` | PAL (512-line clear, screen offset 3,27) | on | `…Euro pe   ` (70) or `…(Europe)` (67) |
| `A` | NTSC | **off** (`0x800510C0` = 0) | anything; the disc's text and logo are shown as-is |
| anything else (`J`, `T`…) | NTSC | on | `…Inc.` (64) |

ROMs seen: 2.00 E `06/14/04 E`, 1.60 E `10/04/01 E`, 1.60 A `02/07/02 A`, devkit 1.50
`12/28/00 A`, 1.00 J `01/17/00 T`. Nothing from the PS2 side reaches the shell: PS1DRV's
arguments (title id, VER) are consumed by PS1DRV; the OSD's console region (OSDVER/NVM) is not
consulted.

### 5.2 What happens on a mismatch [V]

* Wrong **logo** (check on): `SystemErrorBootOrDiskFailure('B', 0x38D)` → hang, black screen
  (§2.2). This runs before the text compare.
* Wrong **licence text** (check on): `0x800300B0` returns −1 → `shell_entry` calls it again,
  forever: black screen, no message, the drive is re-read each pass. No "insert PlayStation
  disc" message exists in this shell; it has no GUI at all.
* **GetID failure** (audio disc, unlicensed, lid open): the TTY strings `!!!WARNING!!! : Not PS
  Disk`, `Audio Disk !!`, `Shell Opened !!` and the same endless retry. On a PS2 these cases
  are normally caught earlier by OSDSYS, which only launches PS1DRV for a PS1 disc.
* A disc with a region string the MechaCon rejects never gets this far (drive-side) [I].

### 5.3 PS2 build vs original PS1 shell [V for the builds compared]

* The 1.60/2.00 build (`System ROM Version 1.0`, sys.c 1.140 of 1998-01-12) is the only one
  with all three licence strings and the letter-driven selection above. The 1.00 J build
  (`analysis/ps1/logo_j100/`) has only the `Inc.` string, its check flag is a constant 1 (never
  cleared), and the letter only selects PAL — so a Japanese 1.00 PS2 insists on `Inc.`
  regardless. The devkit 1.50 A build has no licence string at all, consistent with 'A' never
  checking. All builds share the same TMD, TIMs and VAB.
* Versus a real PS1 shell: no SCE diamond screen, no CD player / memory-card menu, no "please
  insert" screen; it is the licence-screen part only, looping on the disc check. Whether the
  original PS1 BIOS also skips the checks for 'A' consoles is public lore and was not verified
  here [I].

---

## 6. Verified / inferred / not determined

**[V] verified against the image and the discs**

* LOGO = raw {0x30000, size} file loaded and called by TBIN stage 7; the shell's entry is
  0x80030000; TBIN's SYSTEM.CNF/PSX.EXE boot path and defaults.
* Sector reads: LBA 4 × 1, LBA 5 × 7, GetID; the 72-char first-line copy; exact strcmp against
  the three strings; the 0x3278-byte logo compare against 0x8003F540; 'B'/0x38D on mismatch.
* The TMD layout, the parser's assumptions (primitive_top = 0x1C, only modes 0x20/0x30, no
  range checks), vertex/normal/primitive counts and colours; both Tekken discs match the ROM
  copy byte for byte.
* All display parameters, matrices, fog formula constants (133/6980, DQB 0x1400000), frame
  counts (31/30/22|13), sprite table, text metrics, text colour, TIM sizes.
* VAB layout, VAG addresses/sizes, program/tone attributes, the sequence table, PAL 5/6 scaling,
  the key-on calls of phase A, SPU master/reverb settings.
* Region letter logic, including the 1.00 J and devkit variants.

**[I] inferred**

* The mechanism that sets 0xBF801450 bit 3 (PS1DRV's side of the IOP reboot).
* The libspu name of reverb mode 6 (SPACE); the purpose of the SPU-RAM clear at
  0xE000–0x11000 during reverb setup (the register writes themselves are verified).
* That nothing patches the RAM copy of the A0 table so that A0:A1h really hangs (it does in the
  ROM table).
* Which SCEx letters the PS2's drive reports for each disc (the shell just displays bytes 4–7).
* The on-screen extent of the projected logo (computed, not rendered).

**Not determined**

* The exact GP1 display-range values for PAL (libgpu computes them from screen.x = 3,
  screen.y = 27; the table at 0x8004FD44 was not decoded).
* The precise libsnd volume arithmetic (velocity × tone × program × master) and the fine-shift
  unit, so absolute SPU voice volumes/pitches are approximate.
* The CD timing before the first frame (drive-dependent, not in the ROM).
