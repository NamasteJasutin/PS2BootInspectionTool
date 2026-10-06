# `rom0:PS2LOGO` — the "PlayStation 2" logo screen

Subject: the program OSDSYS launches for a PlayStation 2 disc (`LoadExecPS2("rom0:PS2LOGO", 1,
{boot path})`, see `osdsys_flow.md` §2.5). It shows the white "PlayStation 2" lettering with blue
streaks converging on it, plays a short chime, waits, and boots the game. Source: the SCPH-70004
ROM 2.00 E dump; everything is static analysis (Ghidra project `ghidra_proj_logo/`, export in
`analysis/ps2logo/`). **[V]** = verified instruction by instruction or by re-running the maths,
**[I]** = inferred. Addresses are in the *unpacked* image (`extracted/ps2logo_100000.bin`,
loaded at `0x100000`; image offset = address − `0x100000`).

Reference renders of the vector layers (my re-implementation, `tools/logo_anim.py`) are in
`analysis/ps2logo/frames/`; `outline_t29_zoom.png` shows the finished lettering outline.

## 0. In one paragraph

PS2LOGO is a 0x30B6C-byte SCE program (libgraph/libdma/libcdvd 1631 + newlib) hidden behind the
same loader stub + LZ stream as OSDSYS. It reads the region from `rom0:ROMVER`, loads
`rom0:OSDSND` and uploads a one-sample sound bank, **reads the logo bitmap from the first 12
sectors of the game disc** (the logo is *not* in the BIOS), verifies its checksum on J/E
consoles, then runs a ~26-field (NTSC) / 22-field (PAL) animation at 60/50 Hz: the logo bitmap is
drawn every field, starts heavily blurred and sharpens; a lavender outline of the lettering grows
out of the screen centre; three sets of enlarged blue "ghost" letters shrink into place leaving
additive ribbon trails; the whole frame is fed back into itself (87.5 %) for a smeary glow that is
then released. Five voices of a 0.45 s chime play at the start. After the animation the last frame
is held for 120 fields (2.0 s NTSC / 2.4 s PAL), the SPU is shut down and
`LoadExecPS2(argv[1], …)` starts the game.

## 1. Container and program structure **[V]**

* ROM module `PS2LOGO`: BIOS offset `0x32BCE0`, 0x19EA0 bytes (ROMDIR). It is a MIPS ELF whose
  entry (`0x1000008`) is the same loader stub as OSDSYS (`.text.Expand*`): it clears bss, expands
  the LZ stream in its `.data` section (ELF file offset `0x1000`, 0x18C5B bytes, OSD LZ format of
  `tools/osd_unpack.py`) to **`0x100000`** (0x30B6C bytes) and jumps there with argc/argv.
  `tools/osd_unpack.py --raw extracted/rom0/PS2LOGO 0x1000 extracted/ps2logo_100000.bin`.
* Unpacked image: code `0x100000`–~`0x1211xx`, data to `0x130B6C`, bss `0x130B80`–`0x1E02CC`,
  `$gp = 0x1389F0`, `_start = 0x100008`, `main = 0x102040`. `tools/ghidra/run_ps2logo.sh`
  regenerates `analysis/ps2logo/` (524 functions).
* Library identification: `PsIIlibgraph1631`, `PsIIlibdma 1631`, `PsIIlibcdvd`, `PsIIlibkernl1631`
  (SCE SDK 1.6.3.1), plus a small in-house GS packet layer (§3.1). Single thread, no handlers.

### 1.1 `main` (`0x102040`) in order

```
Region()                              # 0x101F60: rom0:ROMVER byte 4 → 0 J/H, 1 A, 2 E, 3 C (cached in 0x130B50)
isPAL = (Region() == 2)               # 0x130B3C
logoDesc.height = isPAL ? 77 : 64     # 0x1217EC, also 0x130B4C
logoDesc.name   = isPAL ? "imagedata/osd_logo_B_PAL.raw" : "imagedata/osd_logo_B.raw"   # label only, no file I/O
sceSifInitRpc(0); sceSifLoadModule("rom0:OSDSND", 0, 0)
SoundInit()                           # 0x107458, §5
GraphicsInit()                        # 0x102188 → 0x1000C0 (GS reset, display env) → 0x1021B0:
                                      #   VRAM alloc, LoadLogoFromDisc (§2), draw env + clear, shape fix-up, t = isPAL ? 14 : 17
if Region() == 1 or checksumOK:       # 0x130B40 == 0; A-region consoles never check
    PlayChime()                       # 0x1075C8: SE 0..4
    AnimationLoop()                   # 0x102270, §4
    HoldAndStopSound()                # 0x107568: 120 vsyncs, free bank, SPU quit
    Cleanup()                         # 0x1025A8 (texture descriptor reset, sceGsSyncPath)
    if argc >= 2: LoadExecPS2(argv[1], argc-2, argv+2)      # boots the disc ELF, never returns
else:
    Cleanup()
return 0                              # _start then calls Exit() (syscall 4)
```

Exit paths: a failed `sceCdRead` of the logo → `ExecOSD(1, {"BootBrowser"})` (`0x101540`); a
checksum mismatch on a J/E console → the animation is skipped and the program exits → kernel
returns to OSDSYS **[I: OSDSYS with argc = 0 takes its `BootError` path, i.e. the browser with
an error message]**. There is no "illegal disc" handling here; that all happens in OSDSYS before
PS2LOGO is launched. The `Ation "%s"&` / `,OSDSND` strings seen in the ROM are LZ-mangled
fragments of newlib's `assertion "%s" failed` and `rom0:OSDSND`.

## 2. The logo bitmap comes from the disc **[V]**

`LoadTexture` (`0x101778`) → `LoadImage` (`0x101540`):

1. `sceCdInit(SCECdINoD)`, `sceCdDecSet(1, 1, 5)` (`0x10B3A0`: S-command 10 with 3 bytes —
   enable XOR, enable rotate, shift 5), `sceCdRead(lbn 0, 12 sectors, buf, mode {try 16,
   spindle 4, pattern 2048})`, poll `sceCdSync(1)`, `sceCdDecSet(0, 0, 0)`.
   → 24,576 bytes at `0x1C1420`: the "PS2 logo" that every licensed disc carries in sectors 0–11.
   The drive descrambles it; **[I]** per emulator sources the raw sector bytes are
   `rotl(b ^ key, 5)`/`rotr(b ^ key, 3)` with `key = byte 0 of sector 0` — the app can validate
   its decryption with the checksums below.
2. Checksum (only J/H and E consoles): `sum of the 0x1800 little-endian u32 words`, XOR
   `0x62DB1E66` (J/H) or **`0x78134705` (E)**, must be 0 (`0x130B40`). A and C consoles skip it.
3. PAL re-wrap (`isPAL`): the 24,576 bytes are re-read as **71 rows of 344 bytes** and copied
   into a zero-filled 384×77 buffer at column 0, row 3 (rows 3–73, columns 0–343). So the PAL
   disc logo is a 344×71 image, the NTSC one 384×64; both 8-bit greyscale, one byte per pixel,
   row-major, top row first.
4. Pixel expansion (`0x101D20`, mode 0, 1 → 4 bytes): `RGBA = (v, v, v, v)`. Uploaded as
   PSMCT32 384×64 / 384×77 to VRAM word `0xF0000` (TBP0 `0x3C00`, TBW 6).

Consequence for the app: the lettering bitmap is on the game disc, not in the BIOS. Either read
sectors 0–11 from a disc image and descramble them, or synthesise the bitmap from the lettering
outline in the BIOS (§4.3, object 3 — 14 closed Bézier outlines, even-odd fill, drawn by the
app at the logo's screen position; its bounding box matches the bitmap's placement).

## 3. GS setup **[V]**

* `sceGsResetGraph(0, INTERLACE, isPAL ? PAL : NTSC, FIELD)` — interlaced, FFMD = FIELD (each
  field shows every other line of a full-height buffer). `SetGsVParam` from the OSD config.
* Three 640×512 PSMCT32 buffers in VRAM (words): front/back at `0x00000` and `0x50000`
  (FBP 0x00 / 0xA0, FBW 10), an auxiliary buffer at `0xA0000` (FBP 0x50) for the blur passes,
  the logo texture at `0xF0000`.
* Display env: `sceGsSetDefDispEnv(640, 512, dx 0, dy = isPAL ? 0 : −32)` → PAL shows all 512
  lines (DY 72, DH 511, logo centred); NTSC puts the 512-line window at DY 18, DH 511, i.e. the
  buffer hangs 32 field lines above the standard picture — the logo lands ≈32 frame lines above
  the centre of the normally visible 448 lines **[I: visual consequence]**.
* Draw env (`0x102AF8`, once): `FRAME` FBW 10, `XYOFFSET (1728, 1792)` = (2048−320, 2048−256),
  scissor 0–639 × 0–511, no Z, no alpha test. Both buffers are cleared to black once with a
  640×1024 sprite. **Nothing clears the frame afterwards**; the frame is a feedback surface.
* Rendering is a one-field pipeline: packets built during field *i* are sent at the start of
  field *i+1* (`0x100320`: `sceGsSyncPath`, wait VBlank, put display env, send VIF1/GIF
  packets, swap). One loop iteration = one VBlank = one field (60/50 Hz).

### 3.1 Packet layer (for reading the decompile)

`0x100710(n, reg, val, …)` emits a GIF A+D packet of *n* register writes (reg ids are GS
register numbers, bit 7 = "add context index"). `0x100F38(reg, field, value, …)` assembles a
register value from the field table at `0x121110` (99 records `{reg, n, n×{field id, width,
shift}}`). `tools/logo_calls.py <func>` prints every call with decoded register names/fields.
`0x100890(…)` opens a PACKED packet with NREG = {RGBAQ, XYZ2} for the vector layers.

## 4. The animation loop (`0x102270`) **[V]**

State: `t` (`0x130B44`, field counter, **starts at 17 NTSC / 14 PAL** — the first part of the
authored timeline is skipped), `running` (`0x130B48`, initially 1), parity (`0x130B04`).

```
loop:
    FrameBegin()                                   # 0x100320
    DrawLogo()                                     # 0x102600, §4.1
    n = clamp((33 − t)·240/33, 0, 240)             # PAL: (28 − t)·240/28
    f_blur = LogoBlur(n)                           # 0x105C68, §4.2 ; returns 1 while n > 0
    f_A = DrawLayers(layer A = objects 3, 1)       # 0x104320, §4.3
    if t < 34 (PAL 29): ScreenBlur(2)              # 0x1065E0, §4.4
    f_B = DrawLayers(layer B = objects 2, 0)
    a = t ≤ 25 ? 112 : clamp((41 − t)·112/15, 0, 112)      # PAL: t ≤ 21 ? 112 : (34 − t)·112/12
    Feedback(alpha = a)                            # 0x106EF0, §4.5
    f_fb = (a ≥ 32)                                # PAL: a ≥ 27
    if t < 34 (PAL 29): ScreenBlur(2)
    FrameEnd()                                     # parity ^= 1
    t += 1
    done = !(f_blur | f_A | f_B | f_fb)
    if done: run two more iterations, then leave
```

Durations: the last layer to finish is object 0's oldest ribbon curve (t − 12 ≥ 29 → **t = 41
NTSC**, t − 10 ≥ 24 → **t = 34 PAL**); plus two fields. So the loop runs t = 17…42 (26 fields,
0.43 s) on NTSC and t = 14…35 (22 fields, 0.44 s) on PAL. Then the final frame stays on screen
for 120 VBlanks (`0x107568`) while the chime rings out, then the game loads (the screen stays
until the game resets the GS).

### 4.1 The logo quad (`0x102600`)

One textured triangle strip, context 1, no blending (`ABE = 0`):

| | NTSC | PAL |
|---|---|---|
| screen X | 149.5 … 533.5 (384 px) | same |
| screen Y | 223.5 … 287.5 (64 px) | 216.5 … 293.5 (77 px) |
| UV | (0,0) … (384, h) | same |

`RGBAQ (128,128,128,64)`, `TEX0` TW 9 / TH 7 (512×128), `TEX1 MMAG = 1` (bilinear), `TEXA
TA0 = TA1 = 128, AEM = 1`, `CLAMP` region-clamp 0…511. The visible content is narrower than 384
px (the PAL data is 344 wide; the outline in §4.3 spans X 154…485), so the lettering sits
centred at X ≈ 320, Y = 256 (buffer coordinates).

### 4.2 Logo blur (`0x105C68(n, frontFBP, auxFBP)`)

`n` iterations of a down/up resample of the logo neighbourhood, each with bilinear filtering,
so the result is a progressively stronger blur (≈ σ 9 px at n = 116, 0 at n = 0):

```
for i in 0..n-1:
    # pass 1: frame → aux.  Source = frame rect (120.5,200.5)–(504.5,296.5)  [384×96]
    #         dest   = aux rect (0,0)–(239.25−0.5i, 71.25−0.5i)   (NTSC; PAL height 84.25−0.5i)
    # pass 2: aux → frame.   Source = aux rect (0.5,0.5)–(239.75−0.5i, 71.75−0.5i)
    #         dest   = frame rect (119.94,199.94)–(503.94,295.94)  (±1/16 px jitter on odd i)
```
Both passes are opaque sprites (`ABE = 0`, `RGBA 128`), `TEX1 MMAG = MMIN = 1`. NTSC aux
height constant 72, offset 2012; PAL 85, offset 2006 (`0x130B54/0x130B58`). Schedule: n = 116,
109, 101, … 7, 0 for t = 17 … 33 (NTSC); 120 … 8, 0 for t = 14 … 28 (PAL). Re-creation hint:
a Gaussian blur of the logo with σ ∝ √n (n = 116 ≈ 9 px, 58 ≈ 6 px, 14 ≈ 3 px) is a good
stand-in.

### 4.3 Vector layers (`0x104320`)

Four keyframed objects. Object index runs 3 → 0; the per-object arrays are stored in that
reversed order (`0x121770` types `{0,1,1,1}` → object 3 is type 0, objects 2,1,0 type 1;
`0x121790` layer A `{1,0,1,0}` → objects 3 and 1 are drawn *before* the screen blur,
`0x1217B0` layer B → objects 2 and 0 after it).

**Shape keyframes** (`0x12AFF8` NTSC / `0x12AFD8` PAL: 4 × `{keyframe*, count}`; keyframe =
`{int t, polyline** shape, int nPolylines}`; polyline = array of 0x30-byte nodes `{float f[8];
int type; int pad[3]}`): type 0 = move-to `(f0,f1)`, 1 = line-to, 2 = cubic Bézier with control
points `(f0,f1)`, `(f2,f3)` and end `(f4,f5)` from the previous point, 3 = end of polyline. At
init (`0x102E00`) every point gets `x −= 128, y += 128` (authoring-canvas → centred
coordinates). Screen position: `X = x + 320`, `Y = 256 − y` (PAL: `Y = 256 − 1.2018·y`,
`0x130A2C/0x130A30`). All keyframes of one object share the node structure, so a shape at time
*t* is the **node-wise linear morph** between the two bracketing keyframes
(`w = (t_next − t)/(t_next − t_prev)` on the earlier one; after the last key the shape stays).

Tessellation (`0x103460`/`0x1035B8`, `0x103CC8`, `0x104088`): a Bézier is flattened into **4
steps** (3 interior samples at s = ¼, ½, ¾) unless the control polygon (after one de Casteljau
split, truncated to int) is longer than 3.5 × the chord or the chord exceeds 2000, in which case
it is a straight line; samples closer than 1 px to the previous one are skipped (lines only).
Ribbons compute the per-segment counts once from the shape at the *current* time
(`0x103318`) and reuse them for all five curves so vertex counts match.

| object | type | polylines / nodes | shape keys (t: NTSC / PAL) | colour `(r,g,b)` | alpha keys `t:a` (NTSC) |
|---|---|---|---|---|---|
| 3 | 0 line strips | 14 closed outlines / 215 (721 points) | 0 → point at centre; **29 / 24** → the full lettering outline (X 154…485, Y 225…286) | (192,168,255) lavender | 0:0 11:0 14:128 24:128 32:64 36:0 |
| 1 | 1 ribbon | 9 / 53 | 0 (huge, off-screen), 3, 10, **24 / 20** = letters l, y, S, t, t, n, a, a, i-dot in place | (96,128,192) | 0:0 12:0 19:35 22:84 28:128 32:64 41:0 |
| 2 | 1 ribbon | 2 / 10 | 0, 3, 10, **20 / 16** = the "o" (outer+inner) | (48,64,96) | 12:0 20:84 28:128 32:64 41:0 |
| 0 | 1 ribbon | 3 / 28 | 0, 3, 10, **29 / 24** = P, a, 2 | (64,64,144) | 12:0 15:102 28:128 32:64 41:0 |

Colour keyframes (`0x12B538` NTSC / `0x12B518` PAL: 4 × `{key*, count}`, key = 5 ints
`{t, r, g, b, a}`), linearly interpolated, then `rgb · min(a,255)/128` clamped to 255
(`0x105A70`). PAL tables have times ×5/6 and colours ×1.1 (e.g. (70,70,158), a up to 140).

Drawing (`0x1059D0` state): `TEST` off, **`ALPHA (A=Cs, B=0, C=As, D=Cd)` = additive**,
vertex alpha 0x80, Z = 0.
* Type 0 (`0x104478`): `PRIM LINESTRIP, IIP, ABE, AA1` (anti-aliased additive lines), one
  strip per polyline, all vertices the current colour.
* Type 1 (`0x1047C8`): five copies of the shape at times `t − d₄, t − d₃, t − d₂, t − d₁, t`
  with `dₖ = int(k · Δ · r)` where Δ = 3.8 (object 0) / 2.8 (objects 1, 2) (`0x130A28/0x130A24`),
  r = 0.84 NTSC / 0.7 PAL (`0x130A34/0x130A38` and `0x130A3C/0x130A40`): offsets
  **12, 9, 6, 3, 0** (object 0) and **9, 7, 4, 2, 0** (objects 1, 2) on NTSC; 10, 7, 5, 2, 0 and
  7, 5, 3, 1, 0 on PAL. Between consecutive copies a `TRISTRIP, IIP, ABE` ribbon is drawn
  (vertex pairs `(curveₖ[i], curveₖ₋₁[i])`), with brightness multipliers `{0, 0.3, 1.0, 0.3, 0}`
  (`0x12F228`) on the five curves: the band is brightest at offset d₂ and fades to nothing at
  both the oldest and the current copy — a comet-like motion trail of the shrinking letters.
  Negative times clamp to 0.

What it looks like: the three ribbon objects start as the letters scaled up 3–4× (partly
off-screen), rush inwards along the morph (most of the travel happens in keys 0 → 3 → 10, well
before `t` starts at 17, so what is visible is the final convergence) and settle exactly on the
lettering by t = 20…29, while object 3's outline unfolds from the centre point into the letter
outlines over t = 0…29 (visible from ≈ 60 % done, since t starts at 17) and glows at full
intensity from t = 14 to 24.

### 4.4 Screen blur (`0x1065E0(2, frontFBP, auxFBP)`)

Two iterations of: whole frame (640×512) → aux rect (0,0)–(479.25−0.5i, 385.25−0.5i) →
back over the whole frame (−0.06…639.94 × −0.06…510.94). Opaque, bilinear. Applied twice per
field (after layer A and after the feedback) while t < 34 NTSC / 29 PAL. Net effect: a mild
soft-focus (≈1–2 px) on everything, including the logo and the trails.

### 4.5 Feedback (`0x106EF0(frontFBP, backFBP, 32, a, 0x84)`)

The *previous* field's buffer is drawn over the current one as a sprite covering
(2,2)–(638,510) (UV 0.5…640.5 × 0.5…512.5, i.e. shrunk by 2 px per side, a 0.994× zoom
towards the centre), `RGBA (0x84, 0x84, 0x84, a)`, `ALPHA (A=Cs, B=Cd, C=As, D=Cd)`:

`out = lerp(current, 1.03 · previous, a/128)` with a = 112 (0.875) until t = 25, then linearly
to 0 at t = 41 (NTSC; PAL 21 → 34, §4 formulas). While active, everything persists with a
per-field factor of ≈0.90 and the freshly drawn (blurred) logo only contributes 12.5 % per
field, which stretches the apparent sharpening over ~8 more fields and brightens the steady
state (0.125/0.098 ≈ 1.27×, clamped). The streak trails decay into a glow instead of
vanishing. Once a = 0 the surface stops accumulating; the held final frame is the sharp logo on
black (plus negligible remnants of the last faint ribbons).

### 4.6 Timeline summary (NTSC fields, t = 17 at the first drawn field; PAL ×5/6)

| t | logo blur n | outline (obj 3) | ribbons | feedback a | notes |
|---|---|---|---|---|---|
| 17 | 116 | 59 % unfolded, full brightness | obj 0 at a=106, obj 1/2 still a=0 | 112 | chime starts here |
| 20 | 94 | 69 % | obj 2 appears (84), obj 1 (35→84 by 22) | 112 | |
| 24 | 65 | 83 %, starts fading at 32 | all at ~100–128, settling (obj 2 done at 20) | 112 | |
| 28–29 | 36–29 | complete at 29 | brightest (128) ; obj 1 done at 24, obj 0/3 at 29 | 97–89 | |
| 33 | 0 | fading (64 → 0 at 36) | 64 → 0 at 41 | 59 | logo sharp from here |
| 37–41 | 0 | gone | trails only | 29 → 0 | feedback released |
| 42 | — | — | — | 0 | last field, then 120-field hold |

## 5. Sound **[V]**

RPC to OSDSND (server `0x80000601`, the same command set as `sound.md` §1.4) from
`SoundInit` (`0x107458`); the bank is embedded in the program:

* `sceSifAllocIopHeap(0x20000)` → IOPBUF; SIF-DMA the bank **body** (`0x12B7C0`, 0x2C90 bytes)
  to IOPBUF and the **header** (`0x12B5C0`, 0x1E0 bytes, "SShd", programs/velocity/LFO tables
  absent, SE table at +0x80) to IOPBUF + 0x10000.
* `cmd 0x19` (SPU driver init), `cmd 2 (0)` (select core 0), `0x500C (1, 0)` + `0x500D (1, 0, 0)`
  (core 1 reverb off), `0x500C (0, 4)` + `0x500D (0, 0x1428, 0x1428)` (core 0: reverb preset 4,
  return volume 5160 ≈ 16 %), `0x5005 (body, header, 0x5010)` → bank id (`0x130B64`), `0x5007`,
  `0x5201 (bank, 0x12)` SE master volume 18, `0x5012 (0/1, 0x3FFF, 0x3FFF)`.
* Play (`0x1075C8`, right before the loop): **`0x5200 (bank, 0..4)`** — five sound effects at
  once. All five use the single sample (`analysis/ps2logo/snd/PS2LOGO_sample_44100.wav`, 19,936
  samples, 0.45 s at 44.1 kHz, fundamental ≈ 460 Hz, peaks within the first 0.15 s, no loop):

  | SE | pitch (Hz@44.1k base) | pan L/R | ADSR | flags |
  |---|---|---|---|---|
  | 0 | 33,269 (×0.754, ≈ F4) | hard L | A exp 02, S lin dec 7F, R 0F | |
  | 1 | 32,795 (×0.744) | hard R | same | |
  | 2 | 24,742 (×0.561, ≈ C4) | centre | A exp 11 (slower), S 38 | |
  | 3 | 44,100 (×1) | centre | as SE 0 | |
  | 4 | 33,032 (×0.749) | centre | as SE 0 | reverb |

  (`tools/snd_dump.py analysis/ps2logo/snd/PS2LOGO_hd.bin` prints the table.)
* After the loop (`0x107568`): 120 VBlanks, `0x5008 (bank)`, `0x5011 (0x10000)` (zero SPU
  RAM), `0x5002` (SPU quit).

## 6. Data in the image (offsets = address − 0x100000) **[V]**

| address | what |
|---|---|
| `0x121110` | GS register field table (§3.1), 0x320 bytes |
| `0x121440` | `sceGsDBuff` (display/draw envs), patched at init |
| `0x121770` / `0x121790` / `0x1217B0` | object type / layer A / layer B tables (8 words each, reversed) |
| `0x1217D0` | logo texture descriptor: `{name*, 1 (bytes/px), 0 (mode), buf*, vram, PSM 0, 384, h}` (h patched 64/77) |
| `0x1217F0`–`0x12AE87` | shape node lists and polyline pointer arrays (object 3's keys at `0x124040`/`0x1268D0`, object 0 `0x126E50`…, 1 `0x128840`…, 2 `0x12A8B0`…; PAL and NTSC keyframes point at the **same** shape data and differ only in times) |
| `0x12AE88`–`0x12AF47` | PAL shape keyframes (object 0: `0x12AE88`, 1: `0x12AEB8`, 2: `0x12AEE8`, 3: `0x12AF18`) |
| `0x12AF30`–`0x12AFD7` | NTSC shape keyframes (0: `0x12AF30`, 1: `0x12AF60`, 2: `0x12AF90`, 3: `0x12AFC0`) |
| `0x12AFD8` / `0x12AFF8` | PAL / NTSC object tables `{keys*, count}` ×4 |
| `0x12B038`–`0x12B054` | tessellation constants `80, 0, 1, 1, 1, 3.5, 2000, 4` |
| `0x12B078`…`0x12B238` | PAL colour keyframes (0: `0x12B078`, 1: `0x12B108`, 2: `0x12B1A8`, 3: `0x12B238`) |
| `0x12B2C8`…`0x12B488` | NTSC colour keyframes (0: `0x12B2C8`, 1: `0x12B358`, 2: `0x12B3F8`, 3: `0x12B488`) |
| `0x12B518` / `0x12B538` | PAL / NTSC colour tables `{keys*, count}` ×4 |
| `0x12B5C0` | sound bank header (0x1E0) ; `0x12B7C0` body = one SPU-ADPCM sample (0x2C90) |
| `0x12F0A0` | `"BootBrowser"` ; `0x12F1A0` / `0x12F1F8` logo names ; `0x12F1C0` `"rom0:ROMVER"` ; `0x12F218` `"rom0:OSDSND"` |
| `0x12F228` | ribbon brightness multipliers `{0, 0.3, 1.0, 0.3, 0, 0}` |
| `0x130A24` / `0x130A28` | ribbon Δ 2.8 / 3.8 ; `0x130A2C`/`0x130A30` PAL y scale 1.0926/0.9091 ; `0x130A34`/`0x130A38` rate 0.84/0.7 (and `0x130A3C`/`0x130A40` same) |
| `0x130B3C` | isPAL ; `0x130B40` checksum result ; `0x130B44` t ; `0x130B4C` logo height ; `0x130B54`/`0x130B58` blur aux height/offset (NTSC 72/2012, PAL 85/2006) |

The full decoded tables are in `analysis/ps2logo/anim_tables.txt` (`tools/logo_anim.py --dump`).

## 7. Variants **[V]**

* Region (from `rom0:ROMVER`, not from the disc): PAL vs NTSC selects video mode, logo height
  64/77 and the 344-byte re-wrap, the keyframe tables (times ×5/6, colours ×1.1), the ribbon
  time rate, the blur constants and the start value of t. The checksum is enforced on J/H and E
  consoles only.
* **Nothing in this build depends on the disc type** (CD vs DVD) or on the disc's region: the
  lettering is always drawn from the disc's own bitmap in white (greyscale texture, no tint), the
  streaks are always blue. If some discs show a different logo it is because their sectors 0–11
  differ (e.g. older or other-region masters), not because of PS2LOGO.
* The program never touches the memory card, the play history or OSD config beyond
  `GetOsdConfigParam` for `SetGsVParam`.

## 8. Address → name

| address | name | address | name |
|---|---|---|---|
| `0x100008` | `_start` | `0x103020` | `ShapeAt(obj, t, out)` (line objects) |
| `0x1000C0` | `GsInit` | `0x103180` | `RibbonShapeAt(out, obj, t, time, buf)` |
| `0x100320` | `FrameBegin` (sync, vsync, send, swap) | `0x103318` | `RibbonSegmentCounts` |
| `0x100500` | `FrameEnd` (parity++) | `0x103460` / `0x1035B8` | `FlattenShape` / `FlattenMorph` |
| `0x100510` | `PacketSelect(buf)` | `0x103768` / `0x103918` | ribbon variants of the same |
| `0x100598` / `0x100710` / `0x100890` | GIF tag / A+D packet / PACKED packet builders | `0x103CC8` / `0x103EC0` | `BezierFlatten` (lines / ribbons) |
| `0x100C30` | `VifCodeBuild` | `0x104088` | `BezierSubdivCount` |
| `0x100F38` | `GsRegBuild(reg, field, val, …)` | `0x1041C0` | `BezierPolyLength` |
| `0x101490` / `0x1014C0` | VRAM alloc / size in words | `0x104320` | `DrawLayers(enable, type, t)` |
| `0x101540` | `LoadLogoFromDisc` | `0x104478` | `DrawLineObject` |
| `0x101778` | `LoadTexture(desc)` | `0x1047C8` | `DrawRibbonObject` |
| `0x101A48` / `0x101BD8` | field-table lookup / value assembly | `0x105A70` | `ColourAt(obj, t)` |
| `0x101D20` | `ExpandPixels` | `0x1059D0` | `SetAdditiveState` |
| `0x101F60` | `Region` | `0x105C68` | `LogoBlur(n, fbp, aux)` |
| `0x102018` | `RegionOrZero` | `0x1065E0` | `ScreenBlur(n, fbp, aux)` |
| `0x102040` | `main` | `0x106EF0` | `Feedback(dst, src, inset, alpha, gain)` |
| `0x102188` / `0x1021B0` | `GraphicsInit` / `SceneInit` | `0x107368` / `0x1073D8` | SIF DMA to IOP / wait |
| `0x102270` | `AnimationLoop` | `0x1073E8` | `SoundBankUpload` |
| `0x1025A8` | `Cleanup` | `0x107458` | `SoundInit` |
| `0x102600` | `DrawLogo(t, abe)` | `0x1075C8` | `PlayChime` |
| `0x102AF8` | `DrawEnvAndClear` | `0x107568` | `HoldAndStopSound` |
| `0x102E00` | `ShapeFixup` (−128/+128) | `0x108500` | `sceCdRead` |
| `0x109108` / `0x109388` | `sceCdSync` / `sceCdInit` | `0x10B3A0` | `sceCdDecSet` |
| `0x10C738` / `0x10C7C8` | OSDSND RPC bind / call | `0x117200` / `0x117220` | `Exit` / `LoadExecPS2` syscalls |
| `0x119BA0` | `sceSifLoadModule` | `0x11B5E0` | `sceSifInitRpc` |
| `0x11A628` / `0x11A2D0` / `0x11A570` | `sceOpen` / `sceRead` / `sceClose` | `0x11D458` | `WaitVBlankStart` |
| `0x120510` / `0x120BB8` / `0x1204B0` | `sceGsSetDefDBuff` / `sceGsSetDefDispEnv` / `sceGsSwapDBuff` | `0x120808` / `0x120B20` / `0x120E90` | `sceGsSyncPath` / `sceGsSyncV` / `sceGsResetGraph` |

## 9. What the app needs

From the user's BIOS (`PS2LOGO` at `0x32BCE0`, unpack the stream at module offset `0x1000`):
* the four objects' shape keyframes and node lists (§6 addresses; parser in
  `tools/logo_anim.py`, ~150 lines) — object 3 gives the lettering outline, 0–2 the ghost
  letters; apply the −128/+128 fix-up and the PAL y-scale;
* the colour keyframes, the type/layer tables, the ribbon constants (Δ, rate, multipliers);
* the sound bank header + body (SE table + one ADPCM sample; decode with `tools/snd_vag2wav.py`
  `decode_vag`), to be mixed as five voices with the pitches/pans of §5;
* constants: start t (17/14), blur schedule, feedback schedule, logo quad placement, 640×512
  field-rate rendering, XYOFFSET/screen mapping.

Not in the BIOS: the logo bitmap (disc sectors 0–11, §2). Synthesise it from object 3's
outlines (even-odd fill, slight anti-aliasing; 344-px wide content centred at x = 320) or read a
disc image.

## 10. Open questions

* The exact descrambling of sectors 0–11 (`sceCdDecSet(1,1,5)`) is done by the drive; the
  XOR-with-first-byte + rotate-5 rule is from memory of emulator sources, not from this code.
  The checksums in §2 let an implementation verify it.
* Whether the NTSC display window really lands 32 frame lines high (DY 18) on a real TV was not
  checked; on this PAL console it is centred.
* The pre-animation black period (IRX load + 12-sector read) was not timed; it is at least a
  few hundred ms and depends on the drive.
* `0x12B5C0` bank: SE flags/unknown fields and the exact SPU reverb preset 4 response are as in
  `sound.md`; the chime mix was not auralised beyond decoding the sample.
* The blur strength equivalence (σ ≈ 9 px at n = 116) is an estimate of the bilinear down/up
  cascade, not a measurement.
