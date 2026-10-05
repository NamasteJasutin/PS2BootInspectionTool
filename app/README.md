# BootScreen — a native re-creation of the PS2 opening

A Swift/Metal re-implementation of the tower scene of the PS2 boot screen, written from the
behavioural notes in `../notes/`. It contains no Sony code or data: textures and the scene's
data tables are read at run time from **your own BIOS dump**, and the towers come from
**your own memory card** (PCSX2 `.ps2` images or folder cards).

```sh
./make_app.sh && open BootScreen.app      # or: swift run BootScreen
```

On start it looks in `~/Library/Application Support/PCSX2/{bios,memcards}`; use *Open…* for
other locations. Supported BIOS: ROM 2.00 E (`0200EC20040614`, SCPH-70004) — other versions
keep the same data at different addresses and need an entry in `OpeningLayout.byROMVersion`.

## What you can do

* **Play history**: the card's `B?DATA-SYSTEM/history` if present. The BIOS only writes that
  file when it launches a disc itself (PCSX2 full boot, not fast boot), so many cards have
  none; then the app can invent one from the titles that have saves on the card, or from two
  sliders (titles × launches), or show the empty new-console scene.
* **Time**: pause, scrub, slow down, loop; choose when the drive "identifies the disc",
  which is what releases the camera into its dive.
* **Free camera**: drag to look, scroll to fly, right-drag to slide.
* **Camera path**: overlay the scripted camera's route — a spine through every frame, rungs
  every 10 frames that point screen-up (so the roll shows as a twist), coloured gates where
  the lettering, dive, defocus, fade and scene end fire, and the live view frustum — and
  export it as CSV.
* **Layers**: switch each stage of the console's frame on and off.

## Tools

```sh
swift run ps2history <card.ps2|folder> [ls [dir] | cat <file>]   # dump the history / browse a card
swift run ps2history --bios <bios>                               # check a BIOS can be read
swift run BootScreen --render <frame> out.png --bios <bios> [--card <card> | --titles N --launches N]
        [--free x,y,z,yaw,pitch] [--disc seconds] [--wrap] [--show-path] [--no-<layer>]...
swift run BootScreen --path [disc-seconds]                       # camera route as CSV (frame, time, z, roll, up, stage)
```

## Layout

| File | Role |
|---|---|
| `Sources/PS2Kit/MemoryCard.swift` | Read-only PS2 memory card filesystem (raw images with/without ECC, folder cards). |
| `Sources/PS2Kit/History.swift` | The 21-record history table; synthetic histories following the console's update rules. |
| `Sources/PS2Kit/BIOS.swift` | ROMDIR, the OSD LZ scheme, texture decoding, per-ROM data layout. |
| `Sources/PS2Kit/Simulation.swift` | Towers from history, camera timeline, closed-form motion of orbs/fog/cubes. |
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
* 60 Hz timing only (the console's 50 Hz mode plays the same motion with a 1.2× step).
* The orb seed is fixed (the console's depends on its C-library `rand()` state).
* No sound, and the second ("insert disc") scene is not implemented.
* Unverified against hardware: which way is up for the tower grid, and whether tower-cap
  colours overflow (toggle *8-bit overflow on tower caps* to see the alternative).
