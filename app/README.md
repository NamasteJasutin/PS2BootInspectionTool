# PS2 Boot Inspection Tool — developer notes

> The cross-platform Rust app at the repository root is the maintained version; this Swift/Metal
> app is kept as the reference it was ported from and only supports ROM 2.00 E.

A Swift/Metal re-implementation of the tower scene of the PS2 boot screen, written from the
behavioural notes in `../notes/`. It contains no Sony code or data: textures and the scene's
data tables are read at run time from **your own BIOS dump**, and the towers come from
**your own memory card** (PCSX2 `.ps2` images or folder cards).

```sh
./make_app.sh && open "PS2 Boot Inspection Tool.app"   # or: swift run BootScreen
```

On start it looks in `~/Library/Application Support/PCSX2/{bios,memcards}`; use *Open…* for
other locations. Supported BIOS: ROM 2.00 E (`0200EC20040614`, SCPH-70004) — other versions
keep the same data at different addresses and need an entry in `OpeningLayout.byROMVersion`.

## What you can do

* **Play history**: the card's `B?DATA-SYSTEM/history` if present. The BIOS only writes that
  file when it launches a disc itself (PCSX2 full boot, not fast boot), so many cards have
  none; then the app can invent one from the titles that have saves on the card, or from two
  sliders (titles × launches), or show the empty new-console scene.
* **Video mode**: NTSC (60 Hz, 224-line field) or PAL (50 Hz, 256-line field, 1.2× time
  step, different pixel aspect and text placement), chosen from the BIOS region.
* **Full boot**: the default scene — one clock from power-on to the point where the game would
  start. Phase ONE (BIOS): the black boot period, the opening with your towers. Phase TWO
  (disc): the hand-off gap while OSDSYS reads SYSTEM.CNF, updates the history and loads
  PS2LOGO; the logo; then an end card. The chime, the transition cue and the logo chime are
  placed on that clock, so the visualiser, scrubbing and the free camera work across it.
* **Disc introspection** (sidebar *Disc*, and the end card): what the console would read and
  do with the disc image — ISO volume and type, the disc-state code, SYSTEM.CNF, the boot ELF's
  location, size, entry point and segments, the logo checksum, how the play history would
  change — and the exact chain of named functions (OSDSYS → KERNEL → EELOAD → PS2LOGO → kernel
  → ELF entry) up to the instruction where the game takes over. Nothing from the disc is run.
* **PlayStation 2 logo**: the disc-boot screen from `rom0:PS2LOGO` — the lettering bitmap read
  and descrambled from the first 12 sectors of a game disc image (checksum-verified; filled from
  the BIOS outline when no disc is given), the progressive blur, the lavender outline unfolding
  from the centre, three ghost-letter ribbon trails, the per-field feedback glow, the five-voice
  chime, and the 120-field hold.
* **Scenes**: the boot animation, or the red "Please insert a PlayStation or PlayStation 2
  format disc" screen (dolly-in, spinning light source with 87.5 % feedback, flares, 128
  drifting puffs, red-tinted prisms, text in the console's language, its own ambient sound).
* **Power-on**: an adjustable black period (default 3 s, the estimate in
  `notes/boot_sequence.md`) with a readout of what the console is doing at that moment —
  two IOP boots, loading OSDSYS, mounting the card, decoding assets, uploading the sound bank.
* **Time**: pause, scrub, slow down, loop; choose when the drive "identifies the disc",
  which is what releases the camera into its dive.
* **Free camera**: drag to look, scroll to fly, Ctrl+scroll to rotate the view, right-drag to slide.
* **Camera path**: overlay the scripted camera's route — a spine through every frame, rungs
  every 10 frames that point screen-up (so the roll shows as a twist), coloured gates where
  the lettering, dive, defocus, fade and scene end fire, and the live view frustum — and
  export it as CSV.
* **Layers**: switch each stage of the console's frame on and off.
* **Boot chime**: synthesised from the BIOS, not played from a recording — the console has
  none. `SNDBOOTH` (instrument bank), `SNDBOOTB` (nine ADPCM samples) and `SNDBOOTS` (an
  8-note sequence) are read from `rom0:SNDIMAGE`, the driver's pitch and pan tables from
  `rom0:OSDSND`, and the app re-runs the driver's note → voice → pitch/volume arithmetic and
  the SPU2 envelope generator. The transition cue (`SNDTNNLS`) is placed where the dive
  starts. Playback follows pause, scrub and speed (speed changes pitch, as it would).
* **Visualiser**: *Volume* (RMS and peak meters with peak hold), *Wave* (the last 21 ms) or
  *Equalizer* (32 log-spaced bands, 40 Hz – 16 kHz) of the chime, drawn over the picture.

## Tools

```sh
swift run ps2history <card.ps2|folder> [ls [dir] | cat <file>]   # dump the history / browse a card
swift run ps2history --bios <bios>                               # check a BIOS can be read
swift run BootScreen --render <frame> out.png --bios <bios> [--card <card> | --titles N --launches N]
        [--pal] [--scene warning --exit seconds | --scene logo [--iso <disc image>]] [--free x,y,z,yaw,pitch] [--disc seconds] [--wrap] [--show-path] [--no-<layer>]...
swift run BootScreen --chime <bios> out.wav [dive-frame]        # the synthesised boot sound
swift run BootScreen --path [disc-seconds]                       # camera route as CSV (frame, time, z, roll, up, stage)
```

## Layout

| File | Role |
|---|---|
| `Sources/PS2Kit/MemoryCard.swift` | Read-only PS2 memory card filesystem (raw images with/without ECC, folder cards). |
| `Sources/PS2Kit/History.swift` | The 21-record history table; synthetic histories following the console's update rules. |
| `Sources/PS2Kit/BIOS.swift` | ROMDIR, the OSD LZ scheme, texture decoding, per-ROM data layout. |
| `Sources/PS2Kit/Simulation.swift` | Towers from history, camera timeline, closed-form motion of orbs/fog/cubes. |
| `Sources/PS2Kit/Sound.swift` | SShd bank / SSsq sequence parsers, PS-ADPCM decoder, the driver's voice maths and SPU envelope, chime renderer. |
| `Sources/BootScreen/Audio.swift`, `VisualizerView.swift` | Timeline-locked playback (AVAudioEngine) and the three visualisers. |
| `Sources/BootScreen/Renderer.swift` | The frame: towers → smear → fog → orbs → glass → defocus → overlays. |
| `Sources/BootScreen/{App,Model,OfflineRender}.swift` | SwiftUI shell, app state, windowless frame export. |

## Where it knowingly differs from the console

* **Glass cubes** are a two-pass approximation (refracted copy of the frame, tint, reflection
  map masked by the noise texture) of the console's ten passes; UV generation and strengths
  are eyeballed, not ported.
* **Smear** is computed statelessly as a weighted sum of the last 12 frames (same weights as
  the console's 62.5 % feedback), so scrubbing and pausing give the right picture. The
  console's half-width feedback buffer is not imitated.
* Rendered at 1280×960 progressive rather than 640×448 interlaced; no edge anti-aliasing;
  geometry is depth-buffered instead of painter-sorted; out-of-range triangles are clipped
  rather than dropped.
* The orb seed is fixed (the console's depends on its C-library `rand()` state).
* **Chime without reverb**: the console runs every voice through the SPU2's reverb (a
  studio-style preset); the app plays the dry mix. Timing uses the nominal 60 Hz sequencer
  update (the console's timer runs ~1.7 % fast).
* **Logo blur passes** run on two small textures covering only the logo region (the console
  resamples in its full frame buffer), and use exactly symmetric down/up rectangles; the console's rectangles
  carry GS half-pixel offsets that do not map 1:1 onto Metal texel centres.
* The warning scene's ambient piece is rendered for its first minute and looped; the console
  plays the full 5.5-minute sequence with its own loop points.
* Unverified against hardware: which way is up for the tower grid, and whether tower-cap
  colours overflow (toggle *8-bit overflow on tower caps* to see the alternative).
