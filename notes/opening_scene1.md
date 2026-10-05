# Opening module — scene 1 (the red "insert disc" screen)

Scope: the second scene of the opening thread (`FUN_002162c0`), selected when the global
`0x1F05E8 == 4` at wake-up (`FUN_00216520` sets `DAT_002c870c = 1`, and `FUN_00216340` copies it
into the scene mode `DAT_002c8704`). Scene 0 (towers) is covered in `notes/opening.md`; helper
names used below (PktBegin/PktEnd/PktAddAD, GsSetReg, SetTest, SetAlpha, TexSprite, FlatSprite,
SetFrame, SetTexture, ZWrite, ZTest, blend table `0x288f10`, texture table `0x287700`) are the
ones established there.

All addresses are for the unpacked OSDSYS image (SCPH-70004, ROM 2.00 PAL). Constants were read
from the image with `tools/peek.py`. "Verified" = read directly from the decompilation/data;
"Inferred" = my interpretation of what it looks like on screen.

## 1. What the scene is (summary)

A slow dolly-in through additive fog billboards towards a flickering dark-red light source,
with five small refracting glass prisms floating in front of it, heavy frame-feedback blur, and
a per-language text texture ("Please insert a PlayStation or PlayStation 2 format disc.")
fading in once the camera has stopped. Everything is transformed on the EE core and sent as
plain GIF A+D packets — **this scene never kicks a VU1 batch** (see §9).

## 2. Dispatch and start-up

Verified:

- `FUN_0021e5b0` — scene-1 dispatcher, same shape as the scene-0 one (`FUN_0021d8c0`), keyed on
  the sub-state `DAT_002c8710`: 0 → run start-up `FUN_0021fbe0`, bump to 1 and fall into the
  frame function; 1 → frame function `FUN_0021fcb8`; 2 → request exit (`DAT_002c8708 = 2`).
  The sub-state is bumped to 2 by `FUN_002163c8` when the timeline (`FUN_0021a608`) returns a
  mode different from the current one.
- `FUN_0021fbe0` — scene start:
  - camera position `(0, 0, z)`; view direction vector `0x288ec0 = (0, -0.03, 1.0)`
    (`DAT_002c81dc = -0.03`, a slight downward tilt); up vector `(0, 1, 0)`; roll angle
    `DAT_002c8798 = 0`.
  - calls `FUN_0021a5a0` (timeline preset, below), sets the same three light directions as
    scene 0 — `(0,0,-1)`, `(0.5,0.5,0)`, `(-0.5,-0.5,0)` — and calls `FUN_00218688` once to prime
    the feedback buffer.
- `FUN_0021a5a0` — timeline preset for this scene: stage index `DAT_00346ec0 = 4`, camera
  `z = 672.0`, z velocity `DAT_00346e98 = 2.16` units/frame, z acceleration
  `DAT_00346e88 = -0.0178`, roll velocity `DAT_00346eb8 = 0.00462` rad/frame, everything else 0,
  `DAT_002c8794 = 0`.
- One-time init (run for *both* scenes from `FUN_00216340` → `FUN_0021e578`): `FUN_0021eaa8`
  (light-source discs), `FUN_00219dd0` (fog billboards), `FUN_0021e328` (prisms + shading
  callback), then clears `DAT_002c87e4` (first-frame flag), `DAT_002c87f4` (fade-out flag),
  `DAT_002c8800` (fade-out start frame).
- Texture loading (`FUN_00216918`): descriptor field `[3]` is a load group. Group 2 (TEXOFOG0 =
  entry 1, TEXOFLAR = entry 9) and group 3 (the text, entries 13–24) are uploaded only when the
  scene mode is 1; group 3 additionally only uploads entry `13 + language`
  (`FUN_00204f50()`).

## 3. Timeline (stages 4–7 of `FUN_0021a608`)

Threshold table `0x289058` = 16, 56, 104, **320, 672, 800, 1160**, 1160; the stage index
advances whenever camera z exceeds `threshold[stage]`. Integration is per frame with
`dt = 1.0`, or `1.2` when `FUN_00205908() != 0` (PAL, 50 Hz).

Verified:

| stage | z range | what the code does |
|---|---|---|
| 4 | start, z = 672 | no case in the switch: pure integration. z velocity 2.16 decelerating by 0.0178 per frame. |
| 5 | 672 < z ≤ 800 | no case: integration continues. |
| 6 | z > 800 | zeroes all x/y/z velocities, accelerations and jerks and the roll *acceleration* (roll velocity is left alone, so the roll continues at 0.00462 rad/frame). Then polls the disc status (below). |
| 7 | z > 1160 | returns mode 2 (exit) and resets the timeline (`FUN_0021a550`). |

- The numbers are tuned so the dolly eases out exactly at the stage boundary:
  `v² = 2.16² − 2·0.0178·128 ≈ 0.11`, i.e. the camera arrives at z = 800 after ≈ 103 frames
  (NTSC) with ≈ 0.33 units/frame left, and stage 6 freezes it there (z ≈ 800.3).
- Stage 6 exit logic (only when `0x1F000C == 0`): latches the status code `0x1F0010` into
  `DAT_002c87a0`.
  - For codes 100, 0x6A–0x70, 0x73, 0x75 (and 0x72 when `0x1F0CF8 ≥ 1`): on the first such frame
    set `DAT_002c87f4 = 1`, send sound command `FUN_00200be8(0x5015, 6, 0, 0xF)` and remember the
    frame number in `DAT_002c8800`; once more than 0x80 frames have passed, return mode 2.
  - For any other code: only if a fade-out was already started and 0x80 frames have passed,
    return mode 2; otherwise stay.
  - 0x72 with `0x1F0CF8 < 1`: do nothing.
- After exit, `FUN_002165a0` (common tail of the opening thread) maps `DAT_002c87a0` to the next
  top-level state.

Inferred: stage 7 / z > 1160 is never reached in practice (the camera is frozen at ≈ 800); the
`z > 1128` fade branch in `FUN_0021fa08` and the 1160 reference points in the brightness formula
look like leftovers of a longer fly-through. The status codes that end the scene presumably mean
"the drive state changed / a new disc is being recognised" (see `notes/osdsys_flow.md`).

## 4. Frame function `FUN_0021fcb8`

The first call only sets `DAT_002c87e4 = 1` (one blank frame). Afterwards, in order:

1. `FUN_0021fb38` — background light source + feedback
2. `FUN_0021a058` — fog billboards
3. `FUN_0021e518` — five glass prisms
4. `FUN_0021fa08` — black fade overlay

followed by the common per-frame tail `FUN_00216448` (text overlay §8, optional letterbox bars,
buffer flip).

### 4.1 `FUN_0021fb38` — light source and feedback

- Computes the scene brightness
  `B = DAT_002c9018 = ((740 − (1160 − z)) · 128 / 740) · 0.6 = (z − 420) · 0.1038`
  → 26.2 at z = 672, 39.4 at z = 800.
- ZWrite off → `FUN_0021ed48` (animate discs) → `FUN_0021f0c0` (small discs) → `FUN_0021f588`
  (large discs + flares) → ZWrite on.
- `FUN_00218878(1, 2, 0x70, 0xffffff, 0x80, field)`: blends the saved previous frame back over
  the screen with blend mode 2 and FIX = 0x70, i.e. `new = 0.875·previous + 0.125·current`
  (scene 0 uses 0x50 = 0.625). Then `FUN_00218688` saves the result as the next "previous".
  Only the discs/flares are inside this feedback loop; fog, prisms and text are drawn after the
  copy is taken.

## 5. The light source: discs and flares

### 5.1 Geometry (`FUN_0021eaa8`, verified)

Seven discs, `i = 0..6`, each stored twice as a 17-vertex fan (centre + 16 rim points at
`k·2π/16`, all at local z = 0):

- set A at `0x34b180 + i·0x110`: rim radius 8
- set B at `0x34b8f0 + i·0x110`: rim radius `28 + 8·i` (28 … 76)

Per-disc state: position `0x34c110 + i·0x10 = (0, 0, 1160.0)`, rotation vector
`0x34c0a0 + i·0x10 = (0, 0, i·0.925·2π/7)`.

Colours (`0x34c060…`): set A centre `(0x80, 0x10, 0x28)`, rim `(0,0,0)`; set B centre
`(0x30, 0x08, 0x0C)`, rim `(0,0,0)`.

### 5.2 Animation (`FUN_0021ed48`, verified)

Per frame, for disc `i` with `n = i + 1`:

- rotation vector `x += 0.2·n`, `y += 0.27·n`, `z += 0.35·n` radians, each wrapped to (−π, π].
- position: `r = (7 − i)² · 2π / 64 / 2` (2.40 for disc 0 … 0.05 for disc 6),
  `φ = wrap((frame mod 201)/32 − π + (7 − i)²·2π/64)`, `x = r·cos φ`, `y = r·sin φ`
  (one orbit per 201 frames; 201/32 ≈ 2π so the sawtooth is seamless). z stays 1160.

Inferred: increments of 0.2–2.4 rad per frame are far too fast to read as rotation — each disc
takes an essentially unrelated orientation every frame. Combined with additive blending and the
87.5 % feedback this produces a smeared, flickering glow rather than visible discs.

### 5.3 Drawing (`FUN_0021f0c0` set A, `FUN_0021f588` set B, verified)

- ZTest always, `SetAlpha(1, mode 0, FIX 0x80)` → `Cs·1.0 + Cd` (additive).
- Colours are scaled by `B/64` and clamped to 0..255: at rest (B ≈ 39) set A centre ≈
  `(79, 10, 25)`, set B centre ≈ `(30, 5, 7)`; rims black.
- Per disc: build rotation (`FUN_00273420`) → translate (`FUN_00273760`) → multiply with the
  world→screen matrix; whole-fan screen clip test (`FUN_00273070`, 17 points) — if it fails the
  disc is skipped; otherwise PRIM `0x14D` (triangle fan, Gouraud, alpha-blend, untextured),
  centre vertex then the 16 rim vertices then the first rim vertex again, each as RGBAQ + XYZF2
  after CPU projection (`FUN_002177c0`).

### 5.4 Flares (`FUN_0021e970` → `FUN_0021e620`, verified)

Called at the end of `FUN_0021f588` with `n = min(int(B/16), 8)`. Texture = entry 9 (TEXOFLAR,
128×128, 16-bit). Five additive sprites (`SetAlpha(1, 0, FIX)`, PRIM sprite via TexSprite,
z = 0xFFFFFF), all centred on the same projected point:

| # | scale | half-size `s` (dest = 2s × s px) | FIX | colour |
|---|---|---|---|---|
| 1 | 1.0 | 0x70 (224×112) | n + 2 | (0x80, 0x40, 0x40) |
| 2 | 1.0 | 0xAA (340×170) | n + 2 | same |
| 3 | 1.0 | 0x100 (512×256) | n + 2 | same |
| 4 | 1.0 | 0x1C0 (896×448) | n + 2 | same |
| 5 | 0.9 | `int(B/8 + 420)` (≈ 848×424) | `clamp(3n + B/2, 0, 255)` | (0x80, 0x70, 0x60) |

Centre point: `FUN_0021e620` overwrites disc 0's x/y with a tiny orbit
(`radius = 0.004·49 = 0.196`, angle `((frame & 31) + 49)·0.1`), rebuilds disc 0's matrix and
projects its centre vertex; the sprite is placed at
`(sx·scale + w/2 − s, sy·scale + h/2 − s/2)` where `sx, sy` are the projected offsets from the
screen centre. The dest height is half the width because the frame buffer is field-height
(224/256 lines).

Inferred: at rest FIX is only 4/128 for the four reddish rings and ≈ 25/128 for the big warm
one — a faint halo that the feedback loop integrates into the soft red glow in the middle of the
screen.

## 6. Fog billboards (`FUN_00219dd0` init, `FUN_0021a058` draw)

Verified:

- Shared mesh at `0x345b50`: 3×3 vertices, `x, y ∈ {−15, 0, 15}`, z = 0 → four quads
  (index table `0x288fd0`), UVs `{0, 0.5, 1}²` (`0x289010`).
- Shared vertex colours at `0x345be0`: all black except the centre vertex, which gets one random
  value chosen at init: `r = 64 + rand % 64`, `g = b = 0.75·r`, alpha 0x80. So each billboard is
  a soft radial blob.
- 128 instances: position `0x345c70 + k·0x10` with `x, y = (rand % 4800 − 2400)·0.01`
  (±24 units) and `z = 477 + rand % 805`; rotation vector `0x346470 + k·0x10 = (0,0,0)`.
- Per frame, instance `k` (with `m = k + 1`):
  - rotation z gets `+m·0.0001` on odd frames and `−m·0.0001` on even frames (net zero —
    a ≤ 0.013 rad shimmer), wrapped to (−π, π];
  - `z −= int(m·0.02 + 1.2)` → 1, 2 or 3 units/frame depending on the index (drifts towards the
    camera);
  - recycling: when `z − camZ − 32 < 0` the instance is moved to `z = camZ + 805` and skipped
    this frame;
  - fade `f` (0..64): ramps in over the 192 units behind the far plane (`camZ + 805 − z`) and
    out over the last 192 units before the near limit (`z − camZ − 32`); otherwise 64.
  - global factor `g`: 128 once `camZ ≥ 672` (always true in this scene; below that it would
    ramp with `(550 − (672 − z))·128/550·4`).
- Draw state: ZTest always, ZWrite off, texture entry 1 (TEXOFOG0, 128×128 16-bit),
  `SetAlpha(1, mode 0, FIX = f·g/128)` → additive with FIX ≤ 0x40. Screen clip test on the
  mesh; four triangle strips, PRIM `0x5C` (strip, Gouraud, textured, alpha-blend), each vertex
  RGBAQ + perspective-correct ST (`uv·q`) + XYZF2.

Inferred: this is the drifting "smoke" — up to 128 half-strength additive puffs streaming past
the camera, tinted only by the frame's existing red content (the puffs themselves are a warm
grey).

## 7. Glass prisms (`FUN_0021e328` init, `FUN_0021e518` → `FUN_0021ddb8` draw)

The machinery is the scene-0 glass-cube renderer (`FUN_002205f8` transform, `FUN_00220e18`
screen grab, `FUN_00220a70` pass state, `FUN_0021fe08`/`FUN_00220250`/`FUN_00220078` UV
generators, `FUN_00220bd0` face emitter); only the parameters and the shading callback differ.

### 7.1 Setup (`FUN_0021e328`, verified)

- `FUN_0021fd00(1.2, 0, 0, 0)`: cube half-size 1.2 (scene 0: 1.8); per-face base colour
  `(128, 128, 128)` (scene 0 passes offsets −16, −16, +24).
- Shading callback `DAT_002c8808 = FUN_0021d930`.
- For `i = 0..4`, `s = (i − 2)·0.8`, with 0 replaced by 0.9 → −1.6, −0.8, 0.9, 0.8, 1.6:
  - position = `(T.x, T.y, (T.z − 2.5)·128 + 788)` from table `0x289ed0`:

    | i | x | y | z |
    |---|---|---|---|
    | 0 | −10.41 | 4.16 | 1113.5 |
    | 1 | −12.92 | −4.27 | 1026.8 |
    | 2 | 2.76 | 0.15 | 993.8 |
    | 3 | 4.10 | −1.32 | 864.2 |
    | 4 | −2.43 | 0.54 | 823.0 |

  - initial rotation = `(s·((2i) mod 9)/4, s·((2i) mod 8)/5, s·((2i) mod 7)/6)`
  - rotation speed (rad/frame) = `(0.004/s, 0.003·s, s/800 + 0.002)`; `FUN_002205f8` adds it
    every frame and wraps to (−π, π].

Positions are static; with the camera parked at z ≈ 800 the prisms sit 23, 64, 194, 227 and
313 units away, i.e. one large prism left of centre and four progressively smaller ones.

### 7.2 Drawing (`FUN_0021ddb8`, verified)

Per prism: transform; if the 8 corners fail the screen clip test the prism is skipped. Otherwise
`FUN_00220e18` copies the current frame into off-screen buffer A (`DAT_00288e90`), then ten
passes, back faces (5 passes into buffer A) then front faces (5 passes into the visible frame).
The ten pass descriptors are byte-identical to scene 0's (`FUN_0021bb90`):

| pass | faces | target | texture | blend (table idx) | FIX / intensity | UV source | colour mode |
|---|---|---|---|---|---|---|---|
| 0 | back | buffer A | frame buffer | 4, blending off, AA1 edge | 0x7A | screen-space refraction | 2 |
| 1 | back | A | TEXOBLPR (12) | 5 `Cs·As + Cd` | 0x80 | planar, offset −0.00375 | 0 (flat 0x80) |
| 2 | back | A | TEXOREF (10) | 8 `Cs·Ad + Cd` | 0x2A | environment, strength 0 | 3 |
| 3 | back | A | TEXOBLP (11) | 5 | 0x80 | planar, offset +0.00375 | 0 |
| 4 | back | A | TEXOREF (10) | 8 | 0x2A | environment, strength 0 | 3 |
| 5 | front | frame | buffer A | 4, blending off, AA1 edge | 0xF0 | screen-space refraction, magnification −0.084 | 2 |
| 6 | front | frame | TEXOBLPR | 5 | 0x80 | planar, offset −0.0075 | 0 |
| 7 | front | frame | TEXOREF | 8 | 0x40 | environment, strength 0.5 | 3 |
| 8 | front | frame | TEXOBLP | 5 | 0x80 | planar, offset +0.0075 | 0 |
| 9 | front | frame | TEXOREF | 8 | 0x40 | environment, strength 0.5 | 3 |

(Blend table entry 8 at `0x288f90` = A:Cs, B:0, C:Ad, D:Cd.)

Differences from scene 0:

- refraction strength (2nd argument of `FUN_0021fe08`) is `DAT_002c8138 = 0.8` (scene 0: 1.0);
- back-face environment passes use strength 0 (scene 0: −0.25);
- ZWrite is switched on before the passes (scene 0 instead re-applies the interlace XY offset);
- `FUN_0021e518` wraps the five prisms in ZTest always / GEQUAL.

### 7.3 Shading callback `FUN_0021d930` (verified)

Called once per emitted vertex by `FUN_00220bd0` with the edge term
`f = (1 − |N·V|)² / 2`.

- UV: for the screen-space modes (descriptor UV field ≠ 0) `u = x/1024`,
  `v = (y − fieldOffset)/256` with `q = 1` — unlike scene 0's callback there is no clamping to
  the visible buffer area; for mode 0, `st = uv·q`, `q = 1/w` (perspective-correct).
- Colour (then clamped to 0..0x80, alpha 0x80), `k = pass FIX/128`:
  - mode 0: flat `(0x80, 0x80, 0x80)`
  - mode 1: face base colour
  - mode 2: `(base + 32·f) · k` — brightens towards silhouette edges
  - mode 3: `r = base.r · (0.6·f + 0.2) · k`, `g` and `b` the same **divided by 3** — this is
    what tints the reflection passes red in this scene (scene 0's callback uses its own
    constants at `0x2c809c…`).

## 8. Overlays

### 8.1 Black fade (`FUN_0021fa08`, verified)

`FUN_00218f50("B", a)` draws a full-screen black FlatSprite with standard alpha blending.

- `z < 800`: `a = 128 − (z − 672)` → opaque at the start, clear exactly when the camera arrives.
- `800 ≤ z ≤ 1128`: nothing.
- `z > 1128`: `a = clamp((32 − (1160 − z))·4, 0, 128)` (not reached, see §3).
- Fade-out: once `DAT_002c87f4` is set (§3), a counter `DAT_002c87e8` rises by 1 per frame to
  0x80 and is drawn as an additional black overlay — a 128-frame fade to black, matching the
  0x80-frame wait before the scene returns mode 2.

### 8.2 Text (`FUN_00219498` → `FUN_002192e8`, verified)

- Armed by `FUN_00219558` only for this scene (`DAT_002c900c = 0`; in scene 0 it is −1 =
  disabled). Triggers once `z > 800`.
- Alpha counter `DAT_002c9010`: +1 per frame up to 0x70 (112 frames); −1 per frame down to 0
  while the fade-out flag is set.
- Texture: entry `13 + FUN_00204f50()` of the texture table (`0x288330 + lang·0xF0`): TEXOPNGJ,
  E, F, S, G, I, D, P, R, K, H, C in that order (indices 13–24). 512×128, 4-bit indexed with a
  16-entry CLUT: `0x287680` (grey ramp with rising alpha) for 13–23, `0x2876c0` for entry 24;
  `FUN_00216918` also swaps entry 14's CLUT to `0x2876c0` when `FUN_00205880()` is 3..6 (the
  same condition under which the asset loader substitutes TEXOPNGM/TEXOPNGW for TEXOPNGE).
- Draw: one TexSprite, source (0, 0, 512, 128) → dest (64, 88, 512, 64); when
  `FUN_00205908() != 0` (PAL) dest y and height are multiplied by 0.526271/0.457627 = 1.15
  (→ y 101, height 73). Colour `(a, a, a)`, alpha 0x80, `SetAlpha(1, mode 5)` = `Cs·As + Cd`
  (additive), z = 0xFFFFFF. Before it, `FUN_00218af8(0, …)` is called with a zero pass count,
  which only resets Z/alpha/texture-filter state.

## 9. VIF1 / VU1 usage

Verified: none of the scene-1 functions touch the VIF1 DMA registers or the VU1 packet at
`0x2740d0`/`0x274250` (the only direct users are the scene-0 functions `FUN_0021d470` and
`FUN_0021cc38`). All scene-1 geometry is transformed on the EE with the sceVu0-style helpers
(`FUN_00273420` rot, `FUN_00273760` trans, `FUN_00273910` mul, `FUN_00273070` clip test,
`FUN_002177c0` project + FTOI4) and emitted as A+D GIF packets through PktBegin/PktEnd. So
neither VU1 path (lit path at 0x021, environment-map path at 0x063) is exercised by this scene.
The micro-program is still uploaded, because `FUN_0021d470` runs unconditionally from
`FUN_00216340`.

## 10. Approximate timing (NTSC, dt = 1; PAL advances 1.2× per 50 Hz frame)

| frame | event |
|---|---|
| 0 | blank frame (`DAT_002c87e4`) |
| 1 → ≈ 103 | dolly 672 → 800, black overlay fades out linearly with z, brightness B 26 → 39 |
| ≈ 103 | camera frozen (roll continues, ≈ 0.26°/frame); text starts fading in |
| ≈ 215 | text at full strength (0x70) |
| … | holds until the disc status changes |
| status change | sound cmd `(0x5015, 6, 0, 0xF)`, text fades −1/frame, black overlay +1/frame |
| +129 frames | timeline returns mode 2; opening thread leaves the scene |

## 11. Open questions

- Meaning of the status codes in `0x1F0010` that end the scene, and of `0x1F000C` / `0x1F0CF8`
  (belongs to the OSDSYS flow notes).
- What the sound command `(0x5015, 6, 0, 0xF)` does (sound notes).
- Whether the ± alternation of the billboard rotation is intentional shimmer or a bug.
- `FUN_00205880()` values 3..6 (region/model variant that selects TEXOPNGM/W and the second
  CLUT) — not resolved here.

## 12. Address → proposed name

| address | proposed name | notes |
|---|---|---|
| `0x0021e5b0` | `Scene1_Dispatch` | sub-state machine |
| `0x0021fbe0` | `Scene1_Start` | camera/light reset, primes feedback |
| `0x0021a5a0` | `Timeline_PresetScene1` | stage 4, z 672, v 2.16, a −0.0178 |
| `0x0021fcb8` | `Scene1_Frame` | skips first frame |
| `0x0021fb38` | `Scene1_DrawLightSourceAndFeedback` | computes brightness `DAT_002c9018` |
| `0x0021ed48` | `Glow_UpdateDiscs` | rotation + orbit |
| `0x0021f0c0` | `Glow_DrawSmallDiscs` | set A, radius 8 |
| `0x0021f588` | `Glow_DrawLargeDiscsAndFlares` | set B, then flares |
| `0x0021e970` | `Glow_DrawFlares` | 5 sprites, TEXOFLAR |
| `0x0021e620` | `Glow_DrawFlareSprite` | (scale, halfSize, fix, r, g, b) |
| `0x0021eaa8` | `Glow_InitDiscs` | geometry + colours |
| `0x00219dd0` | `Smoke_Init` | mesh, colours, 128 random instances |
| `0x0021a058` | `Smoke_UpdateAndDraw` | TEXOFOG0 billboards |
| `0x0021e328` | `Prism_InitScene1` | installs `FUN_0021d930` |
| `0x0021e518` | `Scene1_DrawPrisms` | loop of 5 |
| `0x0021ddb8` | `Prism_DrawScene1` | 10-pass glass |
| `0x0021d930` | `Prism_ShadeVertexScene1` | red-tinted reflection |
| `0x0021fa08` | `Scene1_DrawFade` | fade-in by z, fade-out by counter |
| `0x00219498` | `Overlay_InsertDiscText_Update` | alpha counter |
| `0x002192e8` | `Overlay_InsertDiscText_Draw` | per-language TEXOPNG* |
| `0x00219558` | `Overlay_Reset` | arms SCE text or insert-disc text |
| `0x0021e578` | `Scene1_InitOnce` | discs, smoke, prisms |

Data:

| address | meaning |
|---|---|
| `0x002c9018` | scene brightness B |
| `0x002c87e4` / `0x002c87e8` / `0x002c87f4` / `0x002c8800` | first-frame flag / fade-out alpha / fade-out flag / fade-out start frame |
| `0x002c900c` / `0x002c9010` | text state / text alpha |
| `0x0034b180`, `0x0034b8f0` | disc fans, sets A and B (7 × 17 vertices) |
| `0x0034c0a0`, `0x0034c110` | disc rotation vectors / positions |
| `0x0034c060…0x0034c09c` | disc centre/rim colours |
| `0x00345b50`, `0x00345be0` | smoke mesh vertices / vertex colours |
| `0x00345c70`, `0x00346470` | smoke instance positions / rotations (128) |
| `0x00289ed0` | prism position table (5) |
| `0x002c8120…0x002c81dc` | scene-1 constants |
