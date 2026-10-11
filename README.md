<p align="center"><img src="docs/icon.png" width="160" alt=""></p>

# PSX Boot Inspection Tool

A desktop app (Windows, Linux, macOS) that replays the PlayStation 2 and PlayStation start-up from
**your own** BIOS dumps, memory cards and game discs, and lets you look inside them: what the boot
screens are built from, how your play history shapes them, what the console does between power-on
and the moment a game takes over — and hear every instrument they carry, in your BIOS and in your
games. PS1 and PS2 in full, and an honest PS3 card. (Formerly the *PS2 Boot Inspection Tool*.)

![The opening, built from the play history on a memory card](docs/opening.png)

Nothing from Sony ships with the app. You bring a BIOS image, a PCSX2 memory card and, if you
want the disc phase, a disc image; the tool reads them at run time.

## What you can inspect

### Memory card data, as towers

The towers in the PS2 boot screen are a picture of the console's play history: every title on
the card owns six slots in a 14×9 grid, a title's first launch plants a tower, and more towers
grow with the number of launches. The tool reads `B?DATA-SYSTEM/history` straight from a PCSX2
card image or folder card and builds the skyline from it, so two cards give two different
boot screens, side by side if you like. A table in the sidebar shows each record — title ID,
launch count, which of its towers exist and which one is still growing.

Cards without a history file (PCSX2 fast boot never writes one) are not a dead end: the tool
can stand in a history made from the titles that have saves on the card, or let you dial in a
synthetic one (titles × launches) to see how the skyline fills up over a console's life.

### A free camera

The console's camera only ever flies straight through. Switch to the free camera to fly around
the field, look at the towers from the side or from above, and see the scripted camera's own
route drawn in space — its rungs, the points where the lettering, the dive, the defocus and the
fade trigger, and its view frustum moving along. Blender's camera controls: middle-drag (or drag) to orbit, Shift for pan, Ctrl or the wheel to
dolly, numpad 1/3/7 for the axis views, Home to frame everything. The route can be exported as CSV.

![The scripted camera path seen from the side](docs/camera-path.png)

### The full boot, both phases

*Full boot* plays the whole sequence on one clock:

1. **BIOS phase** — the black period after power-on, with a readout of what the console is
   busy with (IOP boots, loading the OSD program, mounting the card, decoding assets, uploading
   the sound bank), then the opening with your towers, chime and all.
2. **Disc phase** — the hand-over to the disc: the OSD program identifies the disc, reads
   `SYSTEM.CNF`, records the launch in your history and starts `PS2LOGO`; the "PlayStation 2"
   logo plays with the lettering read from the disc's first sectors; then the point where the
   game would start.

For a **PlayStation 1 disc** the second phase is the PS1 licence screen instead: the PS1
shell that lives inside the PS2 BIOS (`rom0:LOGO`) draws the logo model read from the disc's
sectors 5–11, fades it in through the GTE depth cue, then the "PlayStation" wordmark, the
licence line in the kernel's font and the drive's region letters, with the shell's drone and
ascending chime — and the hand-off card follows `PS1DRV` to the PS-X EXE.

The second phase follows the facts. The **Scenario** panel lets you change them: pretend the
console is of another region, tell the drive it found no disc or an illegal one, or switch the
region lock off. A US disc on a European BIOS then ends in the red warning scene, as it would
on the console; an empty tray ends at the clock / main menu; the segment strip under the
scrubber shows which path the sequence takes.

The tool stops exactly there and shows what would happen next instead of doing it: the disc's
title ID and type, `SYSTEM.CNF`, the boot executable's location, size, entry point and memory
segments, the logo checksum, how your play history would change, and the chain of named
functions from the OSD program through the kernel and `PS2LOGO` to the first instruction of
the game. The game is never run.

![The dive, a second before the fade](docs/dive.png)

### Time, sound, picture

- Pause, scrub, slow down, loop. Choose when the drive "identifies" the disc and how long the
  hand-over takes — both change the timing the way they do on the console.
- The boot chime is synthesised from the BIOS the way the console's sound chip plays it, placed
  on the same clock as the picture; a volume meter, waveform or 32-band equaliser can be shown
  over it.
- NTSC (60 Hz) or PAL (50 Hz) presentation, chosen from the BIOS region; the console's
  language for the warning text; every layer of the frame can be switched off to study the one
  underneath.
- The red "Please insert a PlayStation or PlayStation 2 format disc" screen is included as its
  own scene.

### Three workspaces

**Boot** (the scene: console, files, scenario, cameras, director, save data and towers),
**Inspect** (BIOS, Disc, Lab, Fonts — every console in one place) and **Sound** (Sound,
Soundfonts). The Sound workspace takes the focus: the boot pauses and resumes where it was when you
leave.

### The PlayStation, too

Load any standalone PS1, PSone or PSP-POPS BIOS dump and the PS1 console boots its own shell:
the SCE intro (diamond, logotype and its music, rebuilt from the dump), then the licence screen
with that shell's own licence policy, wordmark (® or ™) and SCEx label — read from its code, not
from the version number. With no disc it ends where the console does, in the shell's MAIN MENU
(MEMORY CARD / CD PLAYER) drawn from the dump's own plates and labels; the redesigned menu of
3.0 E and the PSone 4.5 is not reconstructed yet and is captioned instead. The BIOS tab identifies every dump (kernel K1/K2, shell storage, policy)
and audits the set (duplicates, mislabels, a text-mode-corrupted copy); the Disc tab shows how the
PS1 kernel reads `SYSTEM.CNF` (the first line starting with a key decides — a `BOOT2` line before
`BOOT` hides the PS1 boot file) and its boot steps.

### Research instruments (Inspect)

The **Lab** shows, for your BIOS + card + NVM + disc, which hidden hand-overs the console looks for
and whether they would fire (card system update, card DVD player, HDD boot, TESTMODE, the illegal-
disc verdict, the 2.00 hidden Remote Control page…), each with its ROM address; what the game asks
of the console (SDK libraries, IOP modules, version gates, syscalls — Tekken Tag asks nothing); the
PS1 kernel's POST stages and the TTY log it would have printed; PS1 memory cards (undelete
candidates, SoundScope performances); PS3 update snapshots and a PS3 disc/save metadata card. The
BIOS tab adds the companion files (NVM decoded and checksum-validated, MEC, ROM1/2, EROM); **Fonts**
browses every glyph in the BIOS. Command-line tools: `nvminfo`, `hookmap`, `discprobe`, `romdiff`
(which modules changed between your dumps), `bench`, `ps1card`, `pupinfo`, `ps3card`, `extract`.

### Sound: every instrument in your BIOS and your games

**Sound** auditions every program in the PS2 boot bank and the PS1 shell's bank, grouped by
measured style (keys, mallets, leads, pads, drones, swells, air…), and every one-shot effect and
sample, on a keyboard; any note exports as WAV.

**Soundfonts from your games.** Games carry far more instruments than the BIOS. The *Soundfonts*
tab extracts the sound banks from your own disc images — PS1 VAB (and split VH/VB), SEQ and VAG;
PS2 SShd/SSsq, HD/BD and VAG; RIFF WAVE — checking each by parsing it, into a Soundfonts folder
(the platform's data folder by default, or one you choose): one set per disc, in the native
formats, with an index of where each bank came from. Nothing in that folder comes with the app;
`extract <disc>` / `extract --all <folder>` do the same from the command line.

**SoundFont 2 export.** Any bank — your BIOS's PS2 boot and logo banks or PS1 shell VAB, or a bank
extracted from one of your discs (named after its file on the disc) — exports as `.sf2` for
samplers and DAWs.

### Beyond the PS2 (the Save data tab)

The Save-data tab reads
the history file as data: a launch histogram, the records with their last-launch dates, and an
**alternate history** — launch any title (known or new) the way the console's `HistoryUpdate`
would record it today, watch a tower bit get planted at the 14th launch, or a 22nd title evict
the least-played record; undo, reset, export as CSV.

The towers can also leave the console's grid: a **ring** or **spiral** around the camera's
path, slowly revolving, coloured by launch count, by how long ago the title was last played,
by region or by publisher family; and the camera can leave its scripted route for an orbit, a
tornado, a crane shot, a figure-eight or a straight-down view — motion the console never had,
but nothing that isn't yours.

## Requirements and set-up

- Windows, Linux or macOS (the app is Rust on wgpu, so the three builds come from one code base).
- A PS2 and/or PS1 BIOS dump. PS2: ROM 1.00 J (SCPH-10000), the 1.50 devkit, 1.60 E/A and 2.00 E
  are verified — tables are located by content, so other builds should load too. PS1: every
  revision from SCPH-1000 to the PSone 4.5, and the PSP's POPS BIOS.
- A PCSX2 memory card (`.ps2` image, with or without ECC, or a folder card). Optional: a game
  disc image (`.iso`, or a raw `.bin`/`.cue` CD rip) for the disc phase and the genuine logo.

```sh
cargo run --release -p psxbootinspect                    # the app (Rust toolchain from rustup.rs)
cargo run --release -p ps2kit --bin ps2history -- <card>  # dump a card's play history
cargo run --release -p psxbootinspect -- --render 95 out.png --bios <bios> --titles 21 --launches 60 --pal
cargo test -p ps2kit                                     # checks against the Python tools and the Swift app
cargo run --release -p psxbootinspect -- --ui out.png --tab bios --script "tab lab; shot a.png"
                                                         # the real UI, headless: scripted input, rendered to PNG
```

Pre-built binaries for Windows, Linux and macOS come out of the GitHub Actions workflow
(`.github/workflows/build.yml`) for every push to `main`.

On first start the app looks in PCSX2's `bios` and `memcards` folders (Library/Application
Support, Documents or `~/.config`) and in `~/PS2ISO`; use the *Open…* buttons for other
locations.

The original native macOS app (Swift/Metal) is kept in `app/`; see `app/README.md`. It is
functionally the same but supports only ROM 2.00 E.

## Accuracy

The reconstruction comes from a static reading of the BIOS code; it has not been compared
against real hardware or an emulator capture. Known approximations: the glass cubes are a
two-pass stand-in for the console's ten passes, the chime plays without the console's reverb,
power-on and hand-over durations are estimates, and the orientation of the tower grid on screen
and a possible colour overflow on tower caps are unverified. Details are in the documentation below.

## Documentation

For anyone who wants to check the tool's claims or extend it to another BIOS version:

| | |
|---|---|
| `docs/writeup.md` | The whole boot path in one article, for reading rather than reference. |
| `notes/opening.md` | The tower scene: data, camera, every stage of the frame. |
| `notes/opening_scene1.md` | The warning scene. |
| `notes/boot_sequence.md` | Power-on to the first frame: call tree and timeline. |
| `notes/osdsys_flow.md` | The OSD program's control flow and the play-history file format. |
| `notes/ps2logo.md` | The logo program and the disc's logo sectors. |
| `notes/ps1_boot.md` | The PS1 shell inside the PS2 BIOS: the licence screen, the logo TMD on the disc, its sounds and region rules. |
| `notes/sound.md` | The boot chime: driver, bank and sequence formats. |
| `notes/menu_survey.md` | Survey of the main menu and browser (not re-created). |
| `notes/devkit_survey.md` | What the DTL-H30101 development-kit BIOS does differently (very little). |
| `notes/research/` | Four surveys of the hidden hand-overs across five ROMs and four discs — OSDSYS hooks, kernel/EELOAD/IOP probes, what games ask the console, ROM1/EROM/NVM — with a merged, ranked list of research tooling (`notes/research/README.md`). |
| `notes/hidden_features.md` | What every ROM carries that owners never see: the factory test program, the card update hook, the dormant kernel debugger, and what PCSX2 exposes of it. |
| `ps2kit/` | The portable core (Rust): every reader and the simulation, with oracle tests. |
| `psxbootinspect/` | The app (Rust: egui + wgpu + cpal). |
| `tools/`, `analysis/symbols/` | Scripts that derive all of the above from a BIOS image, and symbol tables for the decompilations they produce. |
| [ps2-bios-ghidra](https://github.com/NamasteJasutin/ps2-bios-ghidra) | The headless Ghidra set-up and symbol tables as a standalone kit, for decompiling your own BIOS. |
| [ps2kit on crates.io](https://crates.io/crates/ps2kit) | The readers and the simulation as a Rust library. |

## Licence

MIT (see `LICENSE`). The repository holds no Sony code or data: BIOS and disc images, memory
cards, and everything extracted or decompiled from them are ignored by `.gitignore`. The
screenshots in `docs/` are renders of the app.
