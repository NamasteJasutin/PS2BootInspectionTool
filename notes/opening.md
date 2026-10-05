# The OSDSYS opening animation ("towers" scene)

Subject: `rom0:OSDSYS` from SCPH-70004 (ROM 2.00 E, 2004-06-14), decompressed image at
`0x200000`. Everything here is a description of observed behaviour; addresses are in the
decompressed image (`extracted/osdsys_200000.bin`). Items marked *(inferred)* were not
verified instruction by instruction.

The opening module occupies roughly `0x216270`–`0x220FFF` (≈45 KB of EE code) plus a
229-instruction VU1 micro-program and its packets at the very start of the data segment
(`0x273990`–`0x274910`).

## 1. Module skeleton

| Address | Proposed name | Role |
|---|---|---|
| `0x216270` | `OpeningRegister` | Registers three callbacks (`0x216200`, `0x2161E0`, `0x2161F0`) with the module manager (`0x2059C0`). |
| `0x2162C0` | `OpeningThread` | Thread body: `SleepThread` → run a scene to completion → `SignalSema(0x27E6F8)` → loop. |
| `0x216520` | `OpeningPickScene` | `startScene = (ctx[0x5E8] == 4)`; `letterbox = (screenType() != 1)`. |
| `0x216568` | `OpeningStartSound` | Queues "play sequence" (`0x5014`): sequence 0 = `SNDBOOTS`, the boot chord, for the tower scene; 6 = `SNDWARNS` for the warning scene. |
| `0x216340` | `OpeningInit` | Resets GS/VIF state, loads textures, builds scene data, zeroes the frame counter. |
| `0x216490` | `OpeningRun` | Frame loop: `while (scene != 2) { FrameBegin; scene-specific draw; FrameEnd; }`. |
| `0x2163C8` | `FrameBegin` | Sets scissor to (1,1)–(w−2,h−2), runs the timeline/camera (`0x21A608`). |
| `0x216448` | `FrameEnd` | Draws overlays (logo text), letterbox bars, waits for vsync, `frame++`. |
| `0x2165A0` | `OpeningDecideNext` | After the scene: turns the drive/disc status code into the next OSD state. |
| `0x21A608` | `TimelineStep` | Stage machine + camera physics + view/projection matrices. Returns the scene to run next. |
| `0x21D8C0` | `Scene0Dispatch` | sub-state 0: `Scene0Start` (`0x21D2F8`); 1: `Scene0Draw` (`0x21D3F0`); 2: request exit. |
| `0x21E5B0` | `Scene1Dispatch` | Same for the second scene (see `opening_scene1.md`). |

Globals: `scene` `0x2C8704`, `nextScene` `0x2C8708`, `startScene` `0x2C870C`,
`subState` `0x2C8710`, `letterbox` `0x2C8714`, `frame` `0x2C8700` (rendered-frame counter; it is
seeded with the back-buffer index so its parity always identifies the buffer being drawn),
`ctx` = the global OSD context at `0x1F0000` (`ctx[0xC50]`/`ctx[0xC54]` = draw width/height:
640×224 NTSC, 640×256 PAL; `ctx[0xC40]` = back-buffer index; `ctx[0xC44]` = current field).

Scene 0 ends when `TimelineStep` returns a scene number different from the current one: the
dispatcher bumps `subState` to 2, which sets `nextScene = 2` and leaves the loop.

## 2. Render plumbing

The module does not use a scene graph; it emits GS register writes directly.

* **Packet builder.** `PktBegin` (`0x216DD0`) opens a GIF A+D packet in one of two
  alternating buffers; `PktAddAD(pkt, reg, value)` (`0x216EC0`) appends one register write;
  `PktEnd` (`0x216E48`) pads, closes, and sends it through VIF1 (DIRECT). Convenience
  wrappers set one piece of state each:

  | Address | Name | GS register(s) |
  |---|---|---|
  | `0x2171B8` | `GsSetReg(reg, val)` | any |
  | `0x217278` | `SetTest(ate, atst, aref, afail, date, datm, zte, ztst)` | `TEST_1` |
  | `0x217318` | `SetAlpha(enable, mode, fix)` | `PABE`, `ALPHA_1` |
  | `0x217368` | `SetScissor(rect)` | `SCISSOR_1` |
  | `0x217468` | `SetFrame(fbp, psm, w, h, clear)` | `FRAME_1`, `SCISSOR_1` (+ optional clear sprite) |
  | `0x2182D0` / `0x2184F8` | `SetTexture(tex, mip, mmin, k)` | `TEX0_1`, `TEX1_1`, `MIPTBP1/2_1` |
  | `0x218540` | `SetZTest(on)` | `TEST_1` = ZTST ALWAYS (0) or GEQUAL (1) |
  | `0x218588` | `SetZWrite(on)` | `ZBUF_1` (16-bit Z, mask bit) |
  | `0x218628` | `SetFieldOffset(on, field)` | `XYOFFSET_1` (+½ line on odd fields) |
  | `0x217398` | `TexSprite(dst, src, rgba, abe, z)` | textured sprite |
  | `0x217408` | `FlatSprite(dst, rgba, abe, z)` | untextured sprite |

* **Blend modes.** `SetAlpha` indexes an 11-entry table at `0x288F10` of (A,B,C,D) selectors:

  | Mode | Equation |
  |---|---|
  | 0 | `Cd + Cs·FIX` (additive, constant) |
  | 1 | `Cd − Cs·FIX` (subtractive, constant) |
  | 2 | `lerp(Cd, Cs, FIX)` |
  | 3 | `Cs + Cd·FIX` |
  | 4, 7 | `lerp(Cd, Cs, As)` (normal alpha) |
  | 5 | `Cd + Cs·As` (additive) |
  | 6 | `Cd − Cs·As` (subtractive) |
  | 8 | `Cd + Cs·Ad` (additive, masked by destination alpha) |
  | 9 | `Cd − Cs·Ad` |
  | 10 | `lerp(Cd, Cs, Ad)` |

* **Textures.** A table of 25 descriptors at `0x287700` (stride `0xF0`) names an asset index,
  CLUT, size, mip count, header skip and source format. `LoadTexture` (`0x217B98`) converts to a
  GS format, generates mip levels with a 2×2 box filter for 16-bit sources, and uploads.
  Source formats: 0 = RGBA32, 2 = 16-bit (PS1-style TIM, 0x14-byte header skipped),
  3 = 8-bit alpha with white RGB, 4 = 8-bit alpha with black RGB (both expanded to 32-bit),
  5 = (intensity, alpha) byte pairs expanded to 32-bit,
  0x14 = 4-bit indexed with a 16-entry CLUT.

  | # | Asset | Size | Use in scene 0 |
  |---|---|---|---|
  | 0 | `TEXOSCE` | 256×64, fmt 5 | "Sony Computer / Entertainment" lettering |
  | 1 | `TEXOFOG0` | 128×128 | (scene 1) |
  | 2–5 | `TEXOFOG1`…`4` | 64×64 | fog layers |
  | 6 | `TEXOWAL0` | 256×256 + 2 mips | tower wall texture |
  | 7 | `TEXOCRLE` | 64×64 RGBA | (scene 1) |
  | 8 | `TEXOCRBL` | 64×64 RGBA | light orbs (soft disc) |
  | 9 | `TEXOFLAR` | 128×128 | (scene 1) |
  | 10 | `TEXOREF` | 128×128 | glass-cube reflection map (a photo of rubble) |
  | 11, 12 | `TEXOBLP`, `TEXOBLPR` | 64×64 8-bit alpha | glass-cube reflection masks |
  | 13–24 | `TEXOPNG?` | 512×128 4-bit | per-language "insert disc" text (scene 1) |

  Which entries get loaded depends on the scene (descriptor field 3: 0 = both, 1 = scene 0,
  2 = scene 1, 3 = scene 1 and only the current language).

* **VU1.** `Scene0Init` kicks the DMA chain at `0x273990`, which uploads one micro-program
  (disassembly: `analysis/vu1/opening_init_chain.txt`). It is a classic strip renderer:
  - *Init entry (0x000)*: multiplies the local→world matrix (VU mem 4–7) by the world→screen
    matrix (mem 0–3), caches the light-colour matrix (mem 8–11), the light-direction matrix
    (mem 12–15) and two clip bounds (mem 16–17), then `XGKICK`s a small A+D packet (mem 18–20)
    that sets `ALPHA_1` and `PABE`.
  - *Per-strip entry (continued with MSCNT)*: input at TOP = GIF tag, N positions, N normals,
    N colours, N STQ; output N × (STQ, RGBAQ, XYZ2). Per vertex: `p' = M·p`, `q = 1/p'.w`,
    `xyz = p'.xyz·q` (→ 12.4 fixed), `st = st·q`; `n·L` for three directional lights clamped
    at 0, pushed through the light-colour matrix (three colours + ambient), multiplied by
    the vertex colour. If the projected point is outside `0 < x,y < 4095` or `1 < w < 65535`,
    that vertex and the next two get the "no kick" bit, i.e. any strip triangle touching an
    out-of-range vertex is dropped (no real clipping).
  - A second variant (taken when the float at mem 21.x is non-zero) additionally offsets ST
    by the transformed normal — a cheap environment map. Scene 0's towers use the plain path.

## 3. Timeline and camera (`TimelineStep`, `0x21A608`)

The camera is a point moving along +Z with second-order kinematics; every quantity has a
velocity, an acceleration and (for Z) a jerk, integrated once per frame with a time step of
`1.0` (NTSC) or `1.2` (PAL), so the animation takes the same wall-clock time on both.

State (floats): position `0x288EB0` (x,y,z), view direction `0x288EC0` = (0,0,1), up vector
`0x288ED0` = (sin roll, cos roll, 0), roll angle `0x2C8798`.
Initial values (`Scene0Start`): position (0,0,16), roll −0.12 rad, Z velocity 0.04/frame,
roll velocity 0.001 rad/frame. Roll wraps at ±π.

A stage counter (`0x346EC0`) advances whenever the camera Z passes the next threshold in
`{16, 56, 104, 320, 672, 800, 1160}` (`0x289058`), and can also be advanced by events:

| Stage | Z range | Behaviour |
|---|---|---|
| 1 | 16–56 | **Idle drift.** Two variants, selected by the HDD-boot flag (`0x205960`; always 0 on this ROM, so only the first is reachable here): <br>• *normal*: Z jerk = 4·10⁻⁷; once the disc state (`ctx[0x10]`) has settled — 100 (no disc) or a recognised/rejected type 0x6A–0x70, 0x72–0x75 — and more than 2 s have passed, go to stage 2. While the drive is still detecting (0x65–0x69) the camera just keeps drifting. <br>• *HDD boot*: roll velocity pinned to 0.0004 rad/frame; Z acceleration −0.00014 for the first 3.3 s (the camera slows almost to a stop), then +0.000025; leaves after 20 s, or earlier when the HDD side reports ready/failed, switching Z acceleration to 0.003. |
| 2 | 56–104 | **Dive.** Forced after 10 s at the latest. On entry (once) it latches the disc state and queues a sound cue: PS1 disc / audio CD / DVD-Video → fade the boot sequence out quickly (`0x5015`, release 0x0F, ≤1.4 s); PS2 disc → start sequence 7 (`SNDRCLKS`) and fade the boot sequence slowly (release 0x11, ≤5.5 s); anything else (no disc, or first-time setup pending) → start sequence 1 (`SNDTNNLS`, the transition into the menu) and let the boot sequence ring out. Then: *normal* → Z acceleration 0.0099, roll acceleration 0.000195; *HDD boot* → Z jerk 0.0004, roll acceleration 0.00008. |
| 3 | >104 | Returns `scene+1`, which ends scene 0. |

Simulating the integrator gives a minimum length of ≈4.1 s (2 s drift + 2.1 s dive) for the
normal variant, longer by however long the drive needs to identify the disc. If the state
never settles, the drift alone carries the camera past z = 56 after ≈10.2 s, and stage 2 then
starts the dive immediately (its own 10 s limit has already expired). The HDD-boot variant
takes ≈21.5 s. All of this is identical in PAL and NTSC.

Each frame it then builds:
* normal-light matrix from three light directions at `0x288EE0` — scene 0 uses
  (0,0,−1), (0.5,0.5,0) and (−0.5,−0.5,0);
* world→view from (position, direction, up);
* view→screen with screen distance 1024, pixel-aspect Y factor 0.4576 (NTSC) / 0.5263 (PAL),
  centre (2048,2048), Z range 1…16777215. That is a horizontal field of view of ≈35°.

## 4. Scene 0 data: towers from the play history

`Scene0Init` (`0x21D470`) turns the console's play history into geometry. This is why the
boot screen looks different on every console.

**Input.** 21 history records of 22 bytes at `ctx+0x138`. Bytes 0–15 = title ID string
(empty string = unused record), byte 16 = launch counter (1–63), byte 17 = a 6-bit mask of
the towers this title has earned, byte 18 = the index of the most recently earned bit, or 7
once the record is maxed out. A title earns its first bit on first launch and another,
randomly chosen, at launches 14, 24, 34, 44 and 54 (see `osdsys_flow.md` for the file,
`mc?:/B?DATA-SYSTEM/history`, and its update rules).

**Grid.** There are 14 × 9 = 126 tower slots. A fixed table (`0x2891E0`) gives each of the
21 records six (column,row) slots, so records never share a slot (21 × 6 = 126).

Slot ownership (letter = history record A…U = 0…20, digit = *k*):

```
      col:    0    1    2    3    4    5    6    7    8    9   10   11   12   13
   row 0:    A0   A4   B1   B4   B5   C1   C3   D0   D2   E0   E2   E5   F0   F2
   row 1:    A1   A5   B2   G2   C0   C2   C4   D1   D3   E1   E3   J0   F1   F3
   row 2:    A2   B0   B3   G3   G5   H0   C5   H4   D4   I0   E4   J1   J4   F4
   row 3:    A3   G0   G1   G4   L2   H1   H3   H5   D5   I1   I3   J2   J5   F5
   row 4:    K0   K1   K2   K5   L3   H2   M2   M4   N0   N3   I4   J3   O0   O1
   row 5:    P0   P2   K3   L0   L4   M0   M3   M5   N1   I2   I5   U0   U2   O2
   row 6:    P1   P3   K4   L1   L5   M1   R4   S1   N2   N4   T1   U1   U3   O3
   row 7:    Q0   P4   P5   Q4   R0   R2   R5   S2   S4   N5   T2   T4   U4   O4
   row 8:    Q1   Q2   Q3   Q5   R1   R3   S0   S3   S5   T0   T3   T5   U5   O5
```

For record *i*, for *k* in 0..5 with slot = table[i][k]:
* if `k == record.index` → the slot is the record's *growing* tower. With `c = record.count`
  and `n = c` if `c < 14`, else `4 + (c − 14) mod 10`:
  `fill = A[n]`, `size = B[n]`, where
  `A = {.2,.4,.6,.8,1,1,1,1,1,1,1,1,1,1}` and `B = {.1,.1,.1,.1,.1,.2,.3,.4,.5,.6,.7,.8,.9,1}`.
* else if bit *k* of `record.mask` is set → a *finished* tower, `fill = size = 1`.
* otherwise the slot stays empty and nothing is drawn there.

Put together with the update rules: a factory-fresh console (or one booted without its
memory card) shows no towers at all. A title's first tower fades in over launches 1–4 as a
small stub and grows to full length by launch 13; at launch 14 it becomes permanent and a
second stub appears in another of the title's six slots, and so on every 10 launches until
all six stand (launch 63).

**Placement.** Slot base positions come from a 14×9 table of model coordinates (`0x2895F0`,
columns 1.3 apart in X, rows 1.3 apart in Y, with hand-tuned Z offsets for some slots),
mapped to world space as `x = (mx + 4.8)·4`, `y = (my − 6.5)·4`, `z = (mz + 4)·12 + 150`.
The grid therefore spans about −36…+32 in X and −21…+21 in Y at a typical depth of z ≈ 168,
with a pitch of 5.2 units.

**Tower shape.** Each tower is a box of 4 × 4 units in X/Y whose half-length along Z is
`h = max(size·30, 3)`. Its centre is pushed back by `fill·30 − h`, so that the near cap of a
tower with `fill = 1` sits at the same depth regardless of `size`. `alpha = (1 − fill)·128`
is kept for shading (0 for finished towers).

**Light map** (`0x21C4A0`). A 20×20 brightness table (`0x349950`) is precomputed from two
radial falloffs over the grid plus a deterministic per-cell jitter:
`v = 0.85·(clamp(255·(R − 2·d₁)/R, 32, 255) + clamp(127.5·(R − 4·d₂)/R, 32, 255)) − 10·(((i+j)·i / (j+1)) mod 11 − 5)`,
clamped to 32…220, with `R = √5202 ≈ 72.1`, cell pitch 5.1, and the two centres at cell
coordinates (0, −5.1) and (5.1, 10.2). A tower at (col,row) reads entry (col+3, row+6).

**Fog mesh** (`0x2195B0`). A 17×17 vertex grid at z = 134, 6 units pitch, centred on the
axis, with a radial colour `c = clamp(96·(R − 4·d)/R, 0, 127)` per vertex (d measured from
(−5.1, 0)).

## 5. Scene 0 frame (`Scene0Draw`, `0x21D3F0`)

Order of operations each frame, after the frame buffer has been cleared to black:

1. **Towers** (`0x21CC38`), Z-test GEQUAL + Z-write, texture `TEXOWAL0` with trilinear
   mip-mapping.
   * Sort key per tower = |Δx| + |Δy| between tower and camera. Towers are emitted
     far-to-near by sweeping an integer threshold from the largest key down to 0 and drawing
     every not-yet-drawn tower whose key exceeds it.
   * Orientation: a rotation about Z of `((col+row+9)·(col+3) / (row+7)) mod 4` quarter
     turns (a fixed pseudo-random pattern). Towers that are still growing (`fill ≠ 1`) add a
     sway of `10°·sin(((frame mod 360) − 180)°)`, so they slowly rock ±10° with a 360-frame
     period while finished towers stand still.
   * Per-face vertex data (`0x21C988`, `0x21CB40`): near-end vertices get the light-map
     brightness × `(alpha ? alpha/128 : h/30)`; side faces use 0.8× that; far-end vertices
     are black, so every tower fades into the dark along its length. UVs cover a 0.23×0.23
     patch of the wall texture, shifted by a per-tower pseudo-random multiple of 1/256.
   * Six 4-vertex strips per tower are sent through the VU1 program (lit by the three
     directional lights with colours 1.0, 0.8, 0.8 and ambient 0.4). The far cap is
     degenerate (zero area) in the packet.
2. **Temporal blur** (`0x218878`): the previous frame's saved image is drawn over the new
   towers with blend mode 2, FIX = 0x50 → `frame = 0.625·previous + 0.375·current`.
   This is what produces the long smeared trails as the camera moves.
3. **Save for next frame** (`0x218688`): the result is copied, squeezed to half width, into
   an off-screen buffer.
4. **Fog** (`0x219870`): 6 layers of the fog mesh, layer *i* drawn at z = 134 − 5·i with
   texture `TEXOFOG{4,2,1,4,2,1}[i]`, additive with FIX = 0x14 (≈16 %), vertex colour
   (c/4, 2c/5, c) → blue. Each layer's texture scrolls in S at its own rate
   (`(14 − i)·(i + 1)·0.00005` per frame). Quads fully off-screen are skipped.
5. **Only while camera z < 73:**
   * **Light orbs** (`0x21AD38`): four coloured lights — (32,128,0), (128,32,64), (128,0,0),
     (64,32,128) — on Lissajous paths
     `x = (10 − i)·cos a`, `y = (i + 3)·sin b`, `z = 88 + 12·cos a`,
     `a = (frame + seed + 17i)·(i + 10)·0.001`, `b = (frame + seed + 15i)·(i + 10)·0.0005`,
     with `seed = rand() mod 2345 + 3456` chosen at scene start (the C library generator is never
     seeded, so the value depends only on how many `rand()` calls preceded it). Each is drawn additively
     as a soft disc (`TEXOCRBL`, 1.6 units wide, in the orb's colour) plus a 0.5-unit white
     core, repeated for the last 4 frames' matrices as a short ghost trail. In addition a
     128-entry ring buffer of past screen positions per orb is drawn as an anti-aliased
     line strip (every 8th sample) fading towards the tail.
   * **Glass cubes** (`0x21BB90` ×5): five cubes of half-size 1.8, tinted (112,112,152),
     at fixed positions between z ≈ 82 and 111 (x,y within ±13), each tumbling about all
     three axes at its own constant rates (≈0.001–0.004 rad/frame). Cubes whose eight
     corners are all off-screen are skipped. Each visible cube is built up in ten passes;
     every pass is one small render-state record (target page, texture, blend mode, FIX,
     UV generator, colour generator):

     | Pass | Faces | Target | Texture | Blend | UV generator |
     |---|---|---|---|---|---|
     | 1 | back | off-screen copy of the frame | the live frame | none (opaque, AA1 edge smoothing) | screen position displaced along the face normal (refraction) |
     | 2 | back | 〃 | `TEXOBLPR` (alpha only) | mode 5 — adds nothing to RGB, **writes the alpha mask** | face-planar, shifted −0.00375 |
     | 3 | back | 〃 | `TEXOREF` | mode 8 (add × dest α) | reflection vector, scale −0.25 |
     | 4 | back | 〃 | `TEXOBLP` (alpha only) | mode 5 — alpha mask | face-planar, shifted +0.00375 |
     | 5 | back | 〃 | `TEXOREF` | mode 8 | reflection, −0.25 |
     | 6 | front | live frame | the off-screen copy (now containing the back faces) | none (opaque, AA1) | refraction, with an extra −0.084 magnification term |
     | 7 | front | 〃 | `TEXOBLPR` | mode 5 — alpha mask | planar −0.0075 |
     | 8 | front | 〃 | `TEXOREF` | mode 8 | reflection, +0.5 |
     | 9 | front | 〃 | `TEXOBLP` | mode 5 — alpha mask | planar +0.0075 |
     | 10 | front | 〃 | `TEXOREF` | mode 8 | reflection, +0.5 |

     `TEXOBLP`/`TEXOBLPR` are loaded as alpha-only textures (RGB = 0, A = the 8-bit
     value), so passes 2/4/7/9 leave the colour untouched and only deposit a blotchy
     pattern in the frame buffer's alpha channel; the following `TEXOREF` pass is added
     through that pattern (`Cd + Cs·Ad`). The two noise maps slide in opposite directions,
     which makes the reflections shimmer. *(The alpha-mask reading follows from the texture
     format and blend table; it was not confirmed on hardware.)* Before the first pass the current frame is copied to the off-screen page
     (`0x220E18`). Front/back is decided per face from the sign of its projected area.
     Because the back faces are composited into the copy that the front faces then
     refract, light appears to bend through two layers of glass. Vertex colours come from
     a per-vertex callback (`0x21B5F8`, installed in `0x2C8808`) offering flat grey, the
     cube tint, or the tint plus a rim term proportional to `(1 − |n·v|)²`.
6. **Defocus blur** (`0x21D190`): once z > 56, `n = min(3, (z − 56)/12)` passes of
   "copy the frame down to ≈7/8 size (minus a few more pixels per pass), then stretch it
   back to full size", both bilinear — a progressive defocus during the dive.
7. **Fade** (`0x21D240`): a full-screen black sprite with alpha `(z − 72)·4` (clamped to
   128), i.e. the picture fades to black between z = 72 and z = 104. The first two frames
   are forced black.

`FrameEnd` then adds:
* **"Sony Computer Entertainment"** (`0x219258`/`0x219098`): triggered when z > 18. A
  counter runs 0 → 240 → 0 in steps of 4 per frame; alpha = min(counter, 112). That is a
  28-frame fade-in, 64 frames at full strength and a 28-frame fade-out (2 s at 60 Hz). The
  two text lines of the texture are placed side by side on one screen line, at y = 105
  (NTSC) or 120 (PAL), x = 120 and 326, normal alpha blending at half brightness.
* **Letterbox** (`0x218DC8`): two black bars leaving a 16:9 window, drawn unless the
  "Screen Size" setting is *Full* (i.e. for both 4:3 and 16:9).
* Vsync hand-off (`0x207B80`) and `frame++`.

## 6. Putting it on a clock

Fastest case (disc state already settled), from simulating the integrator. Frame numbers
are NTSC (60 Hz); the times are the same on PAL because of the 1.2× step.

| Time | Frame | Camera z | Event |
|---|---|---|---|
| 0.00 s | 0 | 16 | `SNDBOOTS` was queued just before init; first two frames black; slow drift at 0.04/frame with towers, fog, orbs and glass cubes visible |
| 0.67 s | — | — | main chord of the boot sound enters (see `sound.md`) |
| 0.83 s | 50 | 18 | "Sony Computer Entertainment" starts fading in (28 frames) |
| 2.0 s | 121 | 21 | stage 2: dive begins (Z acceleration 0.0099/frame², roll accelerates); sound cue |
| 2.4 s | 142 | 25 | lettering starts fading out; gone by 2.8 s |
| 3.35 s | 201 | 56 | defocus blur passes start (1 → 3) |
| 3.65 s | 219 | 72 | fade to black begins; orbs and glass cubes stop being drawn at z = 73 |
| 4.1 s | 247 | 104 | scene ends; `OpeningDecideNext` |

If the drive is still identifying a disc, stage 1 simply lasts longer (≈10.2 s at most, see
§3), and everything after "2.0 s" shifts accordingly.

## 7. What happens next

`OpeningDecideNext` (`0x2165A0`) maps the disc state latched at the start of the dive to
the next OSD action — a launch request in `ctx[0x14]` (0 PS2 DVD, 1 PS2 CD, 2 PS1,
3 DVD-Video, 6 HDD), or the next module in `ctx[0x5E8]` (2 clock/main menu, 4 the warning
scene for an illegal disc, 5 the audio-CD player) — and the thread signals its completion
semaphore, on which `main` is waiting. See `osdsys_flow.md`.

## 8. Open questions

* **Tower cap brightness.** The near cap's normal faces light 0 exactly, so the VU1 lighting
  factor is 1.0 + 0.4 ambient = 1.4. With light-map values up to 220 and a finished tower
  (factor 1.0) the product reaches ≈308, above the 8-bit range the GS keeps from the packed
  RGBAQ field, and the micro-program has no clamp. Whether this wraps on hardware for
  brightly-lit finished towers, or whether I have a sign wrong in the cap normal, is
  unverified.
* **Camera Y orientation.** The projection uses a positive Y scale with the up vector
  (0,1,0); which way "up" ends on screen (and hence whether row 0 of the tower grid is the
  top or bottom) was not confirmed against a capture.
* **Fog mesh animation.** The mesh positions include a `sin/cos(frame·51)` wobble term, but
  the builder is only called from scene init (where `frame` is whatever the previous run
  left), so in practice the mesh is static and only the texture scroll animates.
* Not covered here: `rom0:PS2LOGO` (the "PlayStation 2" logo screen shown after a valid
  disc is found — a separate executable) and the browser/clock scenes of OSDSYS.
