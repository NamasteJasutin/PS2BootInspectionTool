# Survey: the main menu / clock module and the browser

Subject: `rom0:OSDSYS` from SCPH-70004 (ROM 2.00 E), decompressed image at `0x200000`. This
is a *survey* of the two UI modules that run after the opening when no disc is present — the
clock / main menu / system-configuration module (thread `0x221450`, `BootClock`) and the
browser (thread `0x248418`, `BootBrowser`) — written to judge what a native re-creation would
involve. It is not a frame-exact spec. Vocabulary (`ctx`, `EndFrameFlip`, tweens, string ids)
is as in `osdsys_flow.md`; render plumbing as in `opening.md` §2.

**[V]** = read in the decompilation (`analysis/osdsys/c/`) or checked against the data;
**[I]** = inferred (SDK names, screen names from memory of the real console, skimmed code).
Nothing has been compared with hardware or an emulator.

---

## 0. Where the code is

Reachability from the two thread entries over `functions.tsv` splits the `0x220F80`–`0x257140`
region cleanly — **no function in it is used by both threads** [V]:

| region | reached from | functions | decompiled lines | bytes | what |
|---|---|---|---|---|---|
| `0x220F80`–`0x236AB0` | clock thread only | 293 | 11,234 | 72 KB | main menu, 3D clock, system configuration, initial-setup wizard, hour-marker renderer |
| `0x236AC0`–`0x247BB8` | browser thread only | 245 | 9,966 | 66 KB | memory-card icon loader/renderer (VU1), icon.sys parser, async MC request layer (`0x2396B0`…), Copy/Delete/Properties dialogs |
| `0x247C18`–`0x257140` | browser thread only | 171 | 8,056 | 49 KB | browser proper: top level, card contents, disc item, CD player |
| `0x20A900`–`0x20F000` | both | — | ≈3,000 | — | text engine (fonts, markup, glyph cache), shared with the launcher |
| `0x257140`–`0x273990` | both | ≈210 each | ≈10,800 | 61 KB | SDK: pad, mc RPC, cdvd, libgraph/libpacket/libdma (`0x26Exxx`, `0x272xxx`), VU0 matrix maths (`0x265xxx`, `0x273xxx`), libc |

Totals reachable: clock 620 functions / 24.9k lines / 152 KB; browser 752 / 32.4k / 198 KB;
shared between them 273 / 12.7k / 74 KB.

---

## 1. Clock / main-menu module (thread `0x221450`)

### 1.1 What it shows [V structure, I appearance]

* **Clock screen**: a dark, time-tinted background; a 3D clock built from **12 hour markers**
  (small hexagonal crystals, mesh in the executable at `0x295BD0`) placed on a circle of
  radius 20 (`0x22B590`: angle `i/12` of a turn, `0x41A00000` = 20.0). The marker for the
  current hour is highlighted (`0x370820 = hour % 12`, `0x22AC38`), a ring/needle angle follows
  the minutes (`0x2C9050/52` = minute/hour angle in 1/65536 turns, eased toward the target with
  factors `0x2C8268/0x2C826C`), and a per-second fade `1 − sec/60` (`0x370A90`). Time comes from
  a software clock (`0x374AE0` ms, `+4` s, `+8` min, `+0xC` h; accessors `0x231A90` seconds,
  `0x231B40` minutes, `0x231C20` hours) synced from the RTC fields in `ctx[0xCB8..]`.
  Objects are depth-sorted into a linked list (`0x22B350`) and drawn twice (two passes,
  `0x22BDF8` → `0x234A70`) with a feedback-blur post pass (`0x2328E0`, n sprites of the previous
  frame; `0x232478(1,0,0)` selects the front buffer as texture).
* **Main menu** on the right half: two items **Browser** / **System Configuration** (table
  `0x28B0C8`: string ids `0x5B`, `0x5C`), drawn by `0x22E428` at x = 430 (`0x1AE`), y from
  `height/2 + 14`, 16 px (NTSC) / 18 px (PAL) apart; selected item uses colour set `0x28A7F0`,
  others `0x28A800`. Button bar at the bottom (`0x221E90`) shows up to 4 icon+label pairs from
  a per-screen table (`0x28A568`, 8 screens × 5 words, Japan/non-Japan variants); the icons
  are sub-rectangles of `TEXCSTSL` (ids 0–1, 28×14: SELECT/START) and `TEXCMARU` (ids 2–5,
  25×12: the ○ × △ □ glyphs), UV table `0x28A4F0` (`0x221C10`).
* **System Configuration** list (`0x22D770`, title id `0x5C`, item table `0x28AEB0`, 0x38 bytes
  per item: `{string id, type, …, value table, draw/draw-selected/enter/… callbacks}`):
  Clock Adjustment, Screen Size (4:3 / Full / 16:9), Digital Out (Optical) (On/Off),
  Component Video Out (RGB / Y Cb/Pb Cr/Pr), Language, plus **Remote Control** (On/Off) if the
  remote flag `0x3749E0[0x13]` is set (`0x223758`). Items are value-cycled in place
  (`0x2239F8`/`0x223A20` draw, `0x223C48` enter) except Clock Adjustment (`0x2227A8` …
  `0x223400`, its own editor with 12/24 h, date format, DST and a time-zone list from the
  `TZLIST` asset [I]) and Language (`0x223D30`, list editor).
* **Version Information** (△ from the main menu, `0x226158` → tween `0x28B190`, drawn by
  `0x2262C0`): one line per registered module (names/versions from the module registry,
  `osdsys_flow.md` §2.2: Console, PlayStation Driver, DVD Player, …); "Options" opens a
  sub-page for the PS1 driver (Disc Speed, Texture Mapping — string `0x6C`) / DVD player
  (`0x225F18`). The worker command `0x16` (`0x206CC8`) refreshes the version strings first.
* **Initial-setup wizard** (`0x2297C0` / `0x229B68`, runs when `NeedsInitialSetup()`): step
  0/1 PS2 logo banner (`TEXCKLG?` 368×125 and `TEXCKLF?` 137×117 RGB24 pictures, N/P = NTSC/PAL
  sizes, drawn by `0x22A648`), step 2 "Select language.", step 3 "Select time zone." / "Is
  daylight savings time in effect?", step 4 "Settings completed. Settings can be adjusted
  later in System Configuration.", step 5 → `SetupDone()` (`0x205940`) and exit to the normal
  menu.
* Sounds: `SoundCmd(0x5015, 1)` on entry (clock ambience, asset `SNDCLOKS` [I]), `0x5015, 2`
  with a fade code on exit, `0x5200, 1, n` for UI effects (n = 4 confirm, 6 cursor, 10 cancel).

### 1.2 Thread, frame loop and exit [V]

```
ClockThread 0x221450:  SleepThread; SoundCmd(0x5015,1); 0x2366D8 (sin/cos table, VU0 matrices);
                       loop { 0x221040 (GS/pad reset, aspect) ; ClockRun 0x2210A8 ; SignalSema(0x27E6F8); SleepThread }
ClockRun 0x2210A8:     ctx[0x5E4]=0; 0x2214D0 (init: VU1 program 0x274910, 10 textures 0x32..0x3B,
                       tweens, pad, text, menu tables, fade-in 0x231278(2));  WaitVSyncBegin;
                       lastDisc = ctx[0x13C4] (or 100 if ctx[0x5EC]==1)
                       frame loop:  disc-state watch (§1.4) ; 0x2215A0 (frame) ; EndFrameFlip ; VideoApplyOutputMode
                       exit: write ctx[0x5E8]/ctx[0x14]/ctx[0x5EC]=2 ; ctx[0x5E4]=1 ; 0x2721B8 (GS sync)
```

`0x2215A0`, one frame, in order: camera/projection (`0x221658`, `0x273118` = perspective with
aspect `0x2C8204/08` for PAL/NTSC), background + previous-frame blur (`0x221720`), 3D clock
objects (`0x22BF00`), fade tick (`0x2314C0`), **screen state update** (`0x22E780`), button bar
(`0x2221A8`), overlay sprites (`0x222258`), hour animation (`0x22B208`), menus/dialogs draw
(`0x22E958`, `0x22EB58`), fade level (`0x2314F8`), clock/RTC sync (`0x232030`), popups
(`0x2315D8`), **pad decode** (`0x232338`).

### 1.3 "Screens" and the tween system [V]

There is no screen enum. Every panel owns a **tween** `{duration, counter, justFinished,
phase}` (16 bytes; phase 0 idle, 1 opening, 2 open, 3 closing) with helpers `0x231100` reset,
`0x231110` open, `0x231130` close, `0x231160` tick, `0x2310E8(t, phase)` test,
`0x2310C0(t, 0x80)` → 0..128 progress. A screen is "the panels whose tweens are not idle";
`0x22E780` ticks all of them every frame:

| tween | panel | update / draw | opened by |
|---|---|---|---|
| `0x28B0F0` | main menu (Browser / System Configuration) | `0x22E730` → `0x22E368`, `0x22E428`, `0x22E608` (input) | fade-in finished (`0x22E2D8`) |
| `0x28B08C` | System Configuration list | `0x22E100` → `0x22D1A8`, `0x22D770`, `0x22E020` | `0x22D088` (confirm on item 1) |
| `0x28B190` | Version Information | `0x226978` → `0x2262C0` | `0x226158` (△) |
| `0x296780`, `0x2967C0` | clock zoom / menu slide-out | `0x22CDC8` → `0x22C918`; `0x22CFB8` | entering the configuration |
| `*(0x2C88C8)+0x1C` | generic value editor panel (`0x2C88D0` → struct with title id, tween, vtable at `+0x50`) | `0x2252D0` → `0x224A88`; `0x226F48` draws; `0x227388` updates | item `enter` callbacks |
| `0x295470/80/90` | initial-setup wizard | `0x2297C0`, `0x229B68` | `0x229620` when setup needed |
| `0x293C28` | "Settings completed"/clock-set popup | `0x228960` | wizard |
| `0x2971A0` | screen fade (`0x2C906C` state 1 in / 2 hold / 3 out / 4 black, level `0x2C9070` 0..0x80) | `0x231278`, `0x2314F8` | module entry/exit |

Which button bar to show is decided by `0x22E9E8(i)` (visibility/alpha of panel *i*):
1 main menu (Enter, Version), 2 configuration list (Display, Back, Enter), 3/4 version pages
(Back, Options / Back), 5 editor (Back, Enter), 6 clock only (Display), 7 (Back, Enter),
8 = wizard custom (`0x221DE8`). [I] "Display" is the SELECT-button label.

### 1.4 Input [V]

`0x232338` (end of every frame) builds `buttons = ~((ctx[0xC5A] << 8) | ctx[0xC5B])` when a
pad of type 2/6 is present (`ctx[0xC78]`), then `0x2C8A44` held, `0x2C8A48` newly pressed,
`0x2C8A4C` released, `0x2C8A50` auto-repeat for up/down (first repeat after `0x2C8A40` =
30 frames (25 PAL, set by the input thread), then every 3rd frame). Bits, after the input
thread's region swap (`0x208DB8` swaps `0x20`/`0x40` outside Japan/China):
`0x1000` up, `0x4000` down, `0x2000` right, `0x8000` left, **`0x20` confirm** (○ in
Japan, × elsewhere), **`0x40` cancel**, `0x10` △ (Version), `0x80` □, `0x800` start,
`0x100` select. The remote control is folded into the same word by the input thread
(`0x201A28`). Main-menu handling `0x22E608`: up/down move `0x28B0E8` with a cursor sound,
confirm on item 0 → `0x22E268(0)` (close menu, exit to browser), item 1 → `0x22D088`
(configuration), △ → Version.

### 1.5 Transitions [V]

* While the menu is up, the frame loop watches `ctx[0x10]`: a newly inserted bootable disc
  (`0x6A`–`0x6E`, `0x73`, `0x75`, audio CD with tracks) closes the menus (`0x22BF90` tests
  that no panel is open), plays the exit sound and fades out (`0x231278(3)`).
* Exit codes written by `ClockRun`: launch `ctx[0x14]` = 0 (`0x6E`), 1 (`0x6C/6D`), 2
  (`0x6A/6B`), 3 (`0x73/75`), 5 (`0x71`); or next module `ctx[0x5E8]` = 3 browser (user chose
  Browser → `lastDisc = 9999`, also `0x6F/0x70`), 5 CD player (`0x72`), 4 warning (`0x74`).
  `ctx[0x5EC] = 2`. Coming back from the browser re-enters the same thread via `main`.
* Configuration changes go through the config worker (`ctx[0xA4]` commands `0x10/0x12/0x1A` →
  NVRAM write + `ApplyOsdConfigToKernel`); the menu waits on `ctx[0xB0]` (5 idle / 8 done).

### 1.6 Assets used [V]

| asset | size | format (per `0x230B78`, index = texture slot) | role [I] |
|---|---|---|---|
| `TEXCFLOW` (slot 0) | 64×64 | 8-bit intensity → grey RGB, A=0x7F | background flow |
| `TEXCKABE` (1) | 64×64 RGB24, upscaled ×2 to 128×128 | background "wall" (soft mottled blue) |
| `TEXCBUMP` (2), `TEXCBINV` (3), `TEXCREFA` (5) | 64×64 8-bit intensity | rubble/bump photos: crystal reflection maps |
| `TEXCSMOK` (4), `TEXCNAVI` (6), `TEXCBLUR` (7) | 64×64 8-bit alpha, RGB white | smoke blob, flat disc, soft glow |
| `TEXCSTSL` (8) | 64×64 (intensity, alpha) pairs | SELECT/START button icons (and sprites) |
| `TEXCMARU` (9) | 64×64 RGBA32 | ○ × △ □ button icons |
| `TEXCKLGN/P`, `TEXCKLFN/P` | 368×105/125, 135×97 / 137×117 RGB24 | PS2 logo + wordmark pictures for the setup wizard |
| `FONTM`, `FNTASCII`, `FNTEX000/001`, `FNTEXOSD`, `FNTADD00` | §4 | text |
| `TZLIST` | — | time-zone table for Clock Adjustment |
| `SNDCLOKS` + `0x5200` effects | — | ambience and UI sounds |
| VU1 program chain `0x274910` | — | hour-marker vertex program (not disassembled) |

Texture slots are described by `{ptr, log2 w, log2 h}` at `0x296D40` (12 bytes each) and
uploaded to GS memory from `0x700000` upward in `0x230E58`. No `ICOIMAGE` icon is used by
this module.

---

## 2. Browser (thread `0x248418`)

### 2.1 What it shows [V structure, I appearance]

A horizontal row of **top-level items** (`0x245A10`, slots 1..9): the disc (slot 1, icon by
disc state: `ICOBDISC`/`ICOBPS1D`/`ICOBPS2D`/`ICOBDVDD`/`ICOBCDDA`/`ICOBQUES`) and up to 8
memory-card positions (2 ports × 4 multitap slots; `0x2C8A60/64` = card present per port),
each a `ICOBPS2M` / `ICOBPS1M` / `ICOBPKST` icon with a caption ("Memory Card/", "%d KB Free",
"Unformatted"). Entering a card (`0x2C8DE4` = level 2..9) shows its **contents**: one 3D icon
per save directory, loaded from the card's `icon.sys` + icon file, with the title text,
"Loading..." while reading; the disc item shows "Reading disc..." / "The disc could not be
read." and launches when confirmed. □/△ opens the **item menu** (`0x2C8DE0 == 1`,
`0x23F588` → `0x240F08`): **Copy** (destination picker, "Do you wish to overwrite…",
"Copying… Do not remove memory cards."), **Delete** ("Are you sure?"), **Properties** (File
Type, File Size, Location, Last Updated, File Protection); unformatted cards get "Format
now?". `0x2C8DE0 == 2` is the **CD player** (`0x255F98`, `0x2546A8`, `0x254CA0`: Track %d,
Play Mode Normal/Program/Shuffle, Repeat Off/All/1, textures `TEXBCDPB`, "Editing…" program
editor), entered directly when `ctx[0x5E8] == 5` (`0x2C8D08`).

Hidden combo (`0x2482B8`) [V]: hold **LEFT + □ + L1 + L2 + R1 + R2** (word `0x808F`) for
120 frames, release, then press **START** (`0x800`): `ScreenType` is reset to 4:3
(`0x204238(0)`), written to NVRAM through worker command `0x1A`, and the word "Reset"
(`0x2C8CE8`) is shown at y = 186 for 27 frames — the documented way out of an unviewable
16:9/full-screen setting.

### 2.2 Thread and frame loop [V]

```
BrowserThread 0x248418:  SleepThread; loop {
    0x247F78 init (0x247CD8 GS/VRAM layout, 0x247DE0 fonts + state, 0x247EA8 textures, 0x24D608)
    0x2467E0(90 NTSC / 75 PAL frames) intro timer ; WaitVSyncBegin ; 0x215F38(0x680000)
    frame: 0x247FA8 { 0x245A10 disc/slot state + icon creation ; 0x246220 clear ; 0x2462D8 ;
                      0x246D00 ; 0x24D7B8 pad ; 0x24D4D0 screen update/draw ; 0x246188 ; 0x246A28 }
           0x2482B8 screen-reset combo ; EndFrameFlip
    until exit requested (0x2C8D00 counts 1..90) and no MC operation pending (0x2C8C34)
    BrowserCommitExit 0x248130 ; SignalSema(0x27E6F8); SleepThread }
```

`0x24D4D0` dispatches on `0x2C8DE0` (0 browse: `0x24E410` input/scroll; 1 item menu:
`0x23F588`; 2 CD player: `0x255F98`), then draws in fixed order: `0x24DA40`, `0x24DFC0`,
`0x242CA0`, `0x24C208`, menu/CD overlays, card info header `0x24A198`, icon row `0x250970`,
`0x250E50`, dialogs (`0x24CC98` error, `0x24CE68` disc error), captions `0x246A50`,
`0x24BF80`, "Loading…" `0x24AEA8`, "Reading disc…" `0x24B150`, 3D icons `0x24B870`, and the
level-change animation (`0x2C8DCC` 0..26/21 frames, `0x2C8DD0`).

Pad: `0x24D7B8` builds the same button word (`0x2C8E30` held, `0x2C8E34` pressed) with the same
bit meanings as §1.4. Exits: `BrowserRequestExit(next)` `0x248008` (cancel at the top level →
`ctx[0x5E8] = 2`, clock module), `BrowserRequestLaunch(discState)` `0x248068` (confirm on the
disc → `ctx[0x14]` = 0/1/2/3, or 4 = warning for `0x74`), committed by `0x248130`.

### 2.3 Memory-card access [V]

The browser never calls the mc RPC directly. It posts commands to the **config/MC worker
thread** (`0x208800`, `ctx[0xA4]`, result in `ctx[0x5C0]`, state in `ctx[0xB0]`) through
wrappers at `0x2396B0`–`0x239F98` (port/slot from `port*4+slot` into `ctx[0x320]/[0x5C4]`,
path copied to `ctx+0x4B0`) and polls `0x239C50`:

| cmd | wrapper | RPC (SDK name [I]) |
|---|---|---|
| 3 / 4 | `0x2396B0` / `0x239728` | `sceMcOpen` / `sceMcClose` |
| 5 / 6 / 7 | | `sceMcSeek` / `sceMcRead` / `sceMcWrite` |
| 8 | | `sceMcGetDir` (`/*`, `%s/*`) |
| 9 / 0xC / 0xD | | `sceMcDelete` / `sceMcMkdir` / `sceMcChdir` (`0x25ECF8`) |
| 0xA | `0x239A30` / poll `0x239F98` | `sceMcGetInfo` (type, free, formatted) |
| 0xB | | `sceMcFormat` (`0x25EDE0`) |
| 0xE / 0xF / 0x19 | | `sceMcSync` / `sceMcSetFileInfo` / `sceMcGetEntSpace` (`0x25F130`) |

Per card the root listing is kept as records of `0x6A0` bytes (`0x2C8EA8[mode]`, count at
`0x2ADBE0 + mode*0x160`), each with the directory entry, type (`+0x690`: 2..9 → which
top-level slot), flags (`+0x90` bit 3 = broken/unknown) and the parsed `icon.sys`.

`icon.sys` parse (`0x238EC0`, buffer at `0x1860000`): magic `PS2D` (`0x2C8A80`), `+0x06`
title line break, `+0x0C` background transparency, `+0x10..+0x4F` four background RGBA
colours, `+0x50..+0xBF` light directions/colours (copied to the icon record `+0x258..`),
`+0xC0` title (68 bytes Shift-JIS, drawn with `FONTM` — this is why every region ships the
kanji font), `+0x104/+0x144/+0x184` icon file names (view/copy/delete). The folder's
`_SCE8`-style system icons and `B?DATA-SYSTEM` get the built-in `ICOBYSYS` instead
(`0x2C53EB`, `0x2C8A88`). Then `"%s/%s"` (folder/view icon) is read into one of **44 icon
slots** (`0x237610`: `0x48000` bytes of VIF packets + `0x9000` texture each) by the
parser below.

### 2.4 The icon (`.ico`) format — built-in `ICOB*` and card icons [V]

`0x2376C0` is a parser for Sony's icon format and all 13 `ICOB*` assets match it:

```
u32 magic   = 0x00010000
u32 shapes  (1, 2, 4, 6 or 8 — vertex-morph key shapes)
u32 attr    bits 0-1: ?, bit 2: has texture, bit 3: texture is RLE-compressed  (0x3 none, 0x7 raw, 0xF RLE)
f32 bgTransparency? (0.0 … 1.5 seen)
u32 nverts  (multiple of 3 — a triangle list; max 1800/1650/1500/1350/1200 for 1/2/4/6/8 shapes)
nverts × { shapes × {i16 x,y,z,w}   positions in 1/1024 units (× 0.0009765625)
           i16 nx,ny,nz,nw ; i16 u,v ; u8 r,g,b,a }
animation header: u32 id=1, u32 frameLength, f32 speed, u32 playOffset, u32 frameCount
frameCount × { u32 shapeId, u32 nkeys, (f32 time, f32 value) × nkeys }
texture: 128×128 16-bit A1B5G5R5 (32768 bytes) raw, or u32 size + RLE stream (attr 0xF)
```

| asset | verts | attr | anim frames | note |
|---|---|---|---|---|
| `ICOBDISC`, `ICOBPS1D`, `ICOBPS2D`, `ICOBDVDD` | 288 | `0xF` | 59 | generic / PS1 / PS2 / DVD disc |
| `ICOBCDDA` | 144 | `0x7` | 99 | audio CD (raw texture) |
| `ICOBPS1M` / `ICOBPS2M` / `ICOBPKST` | 396 / 300 / 726 | `0xF/0x7/0xF` | 99 | PS1 card / PS2 card / PocketStation |
| `ICOBFNOR` / `ICOBFSCE` / `ICOBFBRK` | 36 | `0xF/0xF/0x3` | 1440 | folder normal / SCE / broken (untextured) |
| `ICOBYSYS` | 36 | `0xF` | 99 | "Your System" settings folder |
| `ICOBQUES` | 672 | `0x3` | 99 | question mark, untextured |

Each icon is converted once into a VIF1 chain (UNPACK V4-16 blocks of ≤ `(0x1F0/(shapes+6))/3*3`
vertices, `0x2376C0`) and rendered by the VU1 programs kicked at `0x274C30` / `0x274C50`
(`0x236AC0`, `0x236BC8`: upload matrices/lights, MSCAL). Draw entry `0x23E990` (rocking
motion from `sin/cos` of `0x2C8B78`, scale-in over 40/33 frames, language-dependent offsets
for the Chinese fonts). RLE texture decoding and 16-bit → GS conversion sit in `0x235638`
[I, 3.7 KB, not read].

### 2.5 Browser assets [V]

`TEXB*` are **PS1 TIM files** (`0x250140`: word 1 & 7 = 0 4-bit+CLUT, 1 8-bit+CLUT, 2 16-bit,
3 24-bit, 4 32-bit; width stored in 16-bit units): `TEXBNAV1/2`, `TEXBARRW`, `TEXBBTTN`,
`TEXBCPAR`, `TEXBOVAL`, `TEXBICHI` 64×64 RGBA32, `TEXBCDPB` 64×128 RGBA32 — navigation
arrows, button glyphs, card-parameter box, oval highlight, CD-player panel [I]. Asset index
table `0x2AD758` (8 TEXB + 13 ICOB), GS layout table `0x2ADF04` (0x1C bytes per texture).

---

## 3. Size estimate for a re-creation

| part | code read / size | distinct screens | assets | re-creation: do / skip |
|---|---|---|---|---|
| Clock screen (3D clock + background) | `0x221658`–`0x22C000`, `0x233930`–`0x236AB0`; ≈4k lines | 1 | 10 `TEXC*`, mesh `0x295BD0`, VU1 `0x274910` | **do** — the visual core; needs the hour-marker mesh, blur feedback, time easing |
| Main menu + button bar + text | `0x22E268`–`0x22EB58`, `0x221C10`–`0x2223D8`; ≈2k | 1 | `FNTASCII`, `FNTEXOSD`, button icons | **do** |
| System Configuration list + editors | `0x2227A8`–`0x225300`, `0x22D088`–`0x22E020`; ≈4k | 2 + 6 editors | `TZLIST` | partial: show the list; editors can be stubs (no NVRAM) |
| Version Information | `0x225F18`–`0x226978`; ≈1k | 2 | module registry strings | stub with fixed strings |
| Initial-setup wizard | `0x2297C0`–`0x22A648`; ≈1.5k | 4 steps | `TEXCKLG?`, `TEXCKLF?` | skip (only runs once per console) |
| Browser top level + card contents | `0x247C18`–`0x24E410`, `0x250970`…; ≈5k | 2 levels | 8 `TEXB*`, 13 `ICOB*` | **do** — read a PCSX2 card image, show icons |
| icon.sys / .ico loader + VU1 icon renderer | `0x236AC0`–`0x2384C0`; ≈3k | — | card data | **do** (CPU-side: parse, morph, RLE, 128² texture) |
| Copy / Delete / Properties / Format dialogs | `0x23A000`–`0x247BB8`; ≈6k | ≈8 dialogs | — | skip or Properties only (read-only app) |
| CD player | `0x2520D0`–`0x257140`, `0x23F588`…; ≈4k | 3 | `TEXBCDPB` | skip |
| Text engine | `0x20A900`–`0x20F000`; ≈3k | — | `FNTIMAGE` (+`FONTM` for Shift-JIS titles) | **do** ASCII + OSD symbols + markup subset; FONTM only if card titles must be exact |
| MC async layer | `0x2396B0`–`0x239F98`, worker `0x208800`; ≈1k | — | — | replace with direct file reads of the card image |

Rough total to re-create faithfully: ≈15k of the 32k decompiled lines; a trimmed app (clock +
menu + browser top level + icons) is ≈8k lines of behaviour, i.e. about 3–4× the opening
(`notes/opening.md` covers ≈5.8k lines).

---

## 4. Fonts [V — decoded with `tools/font2png.py` → `extracted/png/font_*.png`]

Uploader `0x20E000` (called from both modules): four 4-bit sheets, each preceded by the same
16-entry RGBA CLUT at `0x2801B0` (uploaded as PSMCT32 8×2), as GS `PSMT4`:

| asset | sheet | glyphs/row | glyph table | count | contents |
|---|---|---|---|---|---|
| `FNTASCII` (`0xF000` B) | 256 × 480 | 8 | `0x27E730` | 97 (`0x20`…`0x80`) | ASCII; `FNTASCI2` is the China variant (table `0x27EA30`) |
| `FNTEX000` | 512 × 760 | 16 | `0x27ED30` | 304 | extended Latin/Cyrillic/Hangul, Shift-JIS rows `0x81`–`0x87` mapped by `0x20E338` |
| `FNTEX001` | 512 × 760 | 16 | `0x27F6B0` | 304 | idem (index ≥ `0x130`) |
| `FNTEXOSD` | 512 × 80 | 16 | `0x280030` | 32 | OSD symbols: arrows, ®, ○△□×, "(PS2)", sun, link… addressed by markup `\x07oNNN` |
| `FONTM` (rom0, LZ-compressed, `0x170E44` B unpacked) | — | — | index via header `+0x0C` → block `{count 0x1142, …, glyph offset}` | 4418 | JIS kanji/kana, **26 × 26 4-bit, 338 bytes per glyph**, nibbles swapped at boot (`0x20DCC8`); drawn through a glyph cache (`0x20BDA8`, `0x20C570`) |
| `FNTADD00` | — | — | `0x280148` | — | extra glyphs for the kanji path (not read) |

Cells are **32 × 40 px**; a glyph table entry is `{i32 xOffsetInCell, i32 width}` (e.g. `' '`
= (0,13), `'!'` = (11,10), `'%'` = (2,29)). Pixel order: low nibble = left pixel. CLUT
index 0 transparent, 1–4 black with alpha 0x19…0x64 (the outline/shadow), 5–15 grey → white
with rising alpha; the quad colour (`0x20AE00`, default 0x80 grey) modulates it, so text
colour is set per draw. Pen state: `0x280428/2C` x/y in 1/16 px (`0x20AB08` sets them in
px), scale `0x300FC4/C8` (`0x20ABD8`), letter spacing `0x280430` (default −3), line height
`0x280424`, PAL stretch `0x300FC0` (`0x20ADC0`, 1.0 NTSC / `0x2C8228` PAL). Advance per
glyph = `scale × width × 0x28013C + scale × spacing` (`0x20B398`, which emits one textured
sprite per glyph with UV `(cellX+8, rowY+8)…` in 1/16 texel). `0x20CDE8` draws a string,
`0x20D290` measures it, `0x222378`/`0x2223D8` are the menu's coloured wrappers.

Markup inside strings (`0x20CA70`), all introduced by byte `0x07`: `a NNN` alpha, `c N`
colour slot, `o NNN` OSD symbol (`\x07o004` = ®), `p NN` / `p@X` fixed advance, `r N.NN`
scale (`\x07r0.90`), `s` space, `w N` width scale, `y ±XX` vertical offset, `g` reset;
`\n` new line, `\t` tab to 63-px stops. Bytes `0x81`–`0x9F` / `0xE0`+ start a 2-byte
Shift-JIS code.

Verification: `extracted/png/font_ascii_sample.png` composes "Browser" / "System
Configuration" from `FNTASCII` with these tables and renders correctly;
`font_FNTEXOSD.png` shows the 32 symbols. (FONTM sample decoded in the scratchpad with the
header above; Greek row `0x200`… came out readable.)

---

## 5. Not determined

* The exact look of the clock scene (colours per hour from `0x2C905C`, which `TEXC*` goes on
  which object, the VU1 program `0x274910`, the blur weights) — the code was skimmed, not
  reproduced; `0x235638` (3.7 KB) and `0x233F68` were not read.
* Whether "Display" (string `0x5F`) is the SELECT label and what it toggles.
* `FNTADD00` layout, `TZLIST` layout, the Clock Adjustment editor's exact fields.
* The browser's layout coordinates (icon row spacing, caption positions) and the CD player.

---

## 6. Address → proposed name

| address | name | role |
|---|---|---|
| `0x220FF0` / `0x220F80` / `0x221450` | `ClockRegister` / `ClockCreateThread` / `ClockThread` | module 2 |
| `0x2210A8` | `ClockRun` | frame loop + exit decision |
| `0x2214D0` / `0x221040` / `0x2215A0` | `ClockInit` / `ClockFrameReset` / `ClockFrame` | |
| `0x22E780` | `ClockScreensUpdate` | ticks every panel |
| `0x22E730` / `0x22E428` / `0x22E608` / `0x22E268` | `MainMenuUpdate` / `MainMenuDraw` / `MainMenuInput` / `MainMenuClose` | table `0x28B0C8` |
| `0x22E100` / `0x22D770` / `0x22D1A8` / `0x22D088` | `ConfigMenuUpdate` / `ConfigMenuDraw` / `ConfigMenuAnim` / `ConfigMenuOpen` | table `0x28AEB0`, `0x223758` builds it |
| `0x226158` / `0x226978` / `0x2262C0` | `VersionOpen` / `VersionUpdate` / `VersionDraw` | |
| `0x2297C0` / `0x229B68` / `0x229620` / `0x22A648` | `SetupWizardStep` / `SetupWizardDraw` / `SetupWizardStart` / `DrawRgb24Picture` | |
| `0x221E90` / `0x2221A8` / `0x22E9E8` | `ButtonBarDraw` / `ButtonBarSelect` / `PanelVisibility` | table `0x28A568` |
| `0x22BF00` / `0x22B638` / `0x22B3E8` / `0x22B350` / `0x22BDF8` | `ClockSceneDraw` / `ClockPlaceHourMarkers` / `ClockAddObject` / `ClockSortInsert` / `ClockDrawObjects` | |
| `0x22AA30` / `0x22AC38` / `0x22B208` | `ClockHandsInit` / `ClockHandsUpdate` / `ClockHourAnim` | |
| `0x231A90` / `0x231B40` / `0x231C20` / `0x232030` | `ClockSeconds` / `ClockMinutes` / `ClockHours` / `ClockSyncRtc` | |
| `0x234A70` / `0x233930` / `0x2328E0` / `0x232478` | `DrawCrystal` / `DrawCrystalPass` / `BlurFeedback` / `SetFrontBufferTexture` | |
| `0x231100` / `0x231110` / `0x231130` / `0x231160` / `0x2310E8` / `0x2310C0` | `TweenReset` / `TweenOpen` / `TweenClose` / `TweenTick` / `TweenInPhase` / `TweenLerp` | |
| `0x231278` / `0x2314F8` / `0x2314C0` | `FadeSet` / `FadeTick` / `FadeDraw` | |
| `0x232338` / `0x232318` / `0x232470` | `PadDecode` / `PadReset` / `PadSetRepeatDelay` | |
| `0x22FA20` / `0x230E58` / `0x230B78` / `0x231030` | `ClockTexturesInit` / `ClockTextureUpload` / `ClockTextureConvert` / `ClockTextureBind` | |
| `0x247C88` / `0x247C18` / `0x248418` | `BrowserRegister` / `BrowserCreateThread` / `BrowserThread` | module 3 |
| `0x247F78` / `0x247FA8` / `0x24D4D0` | `BrowserInit` / `BrowserFrame` / `BrowserScreenUpdate` | |
| `0x245A10` / `0x245508` | `BrowserTopLevelUpdate` / `BrowserSlotIcon` | |
| `0x24E410` / `0x23F588` / `0x255F98` | `BrowseInput` / `ItemMenuInput` / `CdPlayerInput` | modes 0/1/2 |
| `0x24A198` / `0x250970` / `0x24B870` | `CardInfoDraw` / `IconRowDraw` / `Icons3dDraw` | |
| `0x24D7B8` | `BrowserPadDecode` | |
| `0x248008` / `0x248068` / `0x248130` | `BrowserRequestExit` / `BrowserRequestLaunch` / `BrowserCommitExit` | |
| `0x2396B0` … `0x239F98`, `0x239C50` | `McReq*` / `McPoll` | async MC layer |
| `0x238EC0` / `0x238B50` / `0x237610` / `0x2376C0` | `IconSysParse` / `IconFileLoad` / `IconSlotLoad` / `IcoToVifChain` | |
| `0x236AC0` / `0x236BC8` / `0x23E990` | `IconVu1SetMatrix` / `IconVu1SetLights` / `IconDraw` | |
| `0x250140` / `0x250670` / `0x247EA8` | `TimDecode` / `BrowserTextureUpload` / `BrowserTexturesInit` | |
| `0x20E000` / `0x20DEB8` / `0x20DD58` | `FontsUpload` / `FontSheetUpload` / `GsImageUpload` | |
| `0x20CDE8` / `0x20D290` / `0x20CA70` / `0x20B398` / `0x20C570` | `TextDraw` / `TextMeasure` / `TextMarkup` / `GlyphEmit` / `KanjiEmit` | |
| `0x20E338` / `0x20BDA8` / `0x20DCC8` | `SjisToExtGlyph` / `KanjiCacheGet` / `FontmNibbleSwap` | |
| `0x20AB08` / `0x20AE00` / `0x20ABD8` / `0x20ABA8` | `TextSetPos` / `TextSetColor` / `TextSetScale` / `TextSetSpacing` | |
