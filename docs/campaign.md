# The campaign mountain

The route from today's replay tool to a research instrument, in camps. Each camp is a
shippable release; nothing at a higher camp is started before the camp below is on `main`.
Options are numbered as in `docs/options.md`.

```
                                   ▲ Summit — 2.0 "Inspection"
                                  / \   write-up II, wiki pages, crates 1.0
                                 /   \
                     Camp 5 ────/     \──── flagship: towers tagged with the card's own
                    icons (19), clock/menu scene + Version Information (45), browser
                   /                       \
          Camp 4 ─/                         \─ BIOS depth: ROM explorer (39), hook map (44)
                 /   from notes/research, NVM inspector (43), font atelier (41), VAB bench (46),
                /    two-dump diff (40), TESTMODE dossier (47)
               /                                 \
      Camp 3 ─/                                   \─ cameras: orbit suite (27), tornado (28),
             /   homages (29–37), director mode (38)
            /                                           \
   Camp 2 ─/                                             \─ save-data tab: graphs (15), ring
          /   layout (16), colour by attribute (17), themes (18), inspector (21),
         /    time-machine (22), alternate history (23), ghost towers (24), exports (14)
        /                                                      \
Camp 1 ─────────────────────────────────────────────────────────── control: tabs (1),
        scenario bar (2) with real outcomes, facts-as-you-watch (6), scrubber ticks (3)
        provenance on the hand-off card (5, first cut)
Base ──────────────────────────────────────────────────────────── today: opening, warning,
        PS2 logo, PS1 licence, hand-off facts, ps2kit 0.2
```

## Camp 1 — control (on main, 2026-10-09)

Goal: the tool stops only replaying what the files say and starts answering "what would the
console do if…". Everything is derived from facts ps2kit already computes.

| step | what | where |
|---|---|---|
| 1.1 | `BootOutcome` and a `BootPlan`: a sequence that ends in the menu (no disc), the warning scene (illegal disc / region rejected), the logo → game, or the PS1 licence | `ps2kit::sim` |
| 1.2 | `HandoffStep::IllegalDisc`; `boot_outcome(&[HandoffStep])` derives the outcome from the facts | `ps2kit::disc` |
| 1.3 | Scenario bar: console region override, disc override (as loaded / no disc / illegal), "enforce the console's checks" toggle; the full sequence reroutes live | app model + UI |
| 1.4 | Sidebar tabs: Boot · Save data · Disc · BIOS | app UI |
| 1.5 | Facts as you watch: the current hand-off step highlighted on the card; scrubber phase ticks | app UI |
| 1.6 | BIOS tab first cut: ROMVER, region, module table (name, offset, size) | app UI |
| 1.7 | Save-data tab first cut: records table + launch histogram | app UI |

Exit criteria: 26+ ps2kit tests green, clippy clean, renders unchanged for the default
scenario, `--handoff` output unchanged, Camp 1 tagged `v1.1.0`.

## Camp 2 — save data (in progress)

Done: launch histogram, last-launches-over-time, records with dates, ring/spiral layouts
with revolve, colour by count/age/region/publisher, alternate history (launch/undo/reset via
`PlayHistory::launch`), tower inspector (hover → record, highlighted), CSV/text exports, Pareto curve (share of play), SVG export of the graphs.
Open: themes, time-machine scrub, ghost towers.

Graphs (histogram + Pareto, dates over time, prefix donut, eviction simulator), ring and
spiral layouts that revolve behind the graphs, colour by count/age/region/publisher, theme
packs, tower inspector, time-machine scrub, alternate-history sliders, ghost towers, CSV/JSON/
SVG exports. Tag `v1.2.0`.

## Camp 3 — cameras (started early: orbit, tornado, crane, figure-eight, zenith paths)

A `CameraPath` abstraction in the app (keyframes, Catmull-Rom, easing), the orbit suite,
tornado, the console homages as data, director mode with JSON import/export and seeded
shuffle. Tag `v1.3.0`.

## Camp 4 — BIOS depth

Built on `notes/research/*` (the four hook catalogues): ROM explorer, hook map ("which hidden
hand-overs fire for your files"), NVM inspector, font atelier, VAB bench, two-dump diff,
TESTMODE dossier, DVD-player panel. ps2kit 0.3 carries the new readers. Tag `v1.4.0`.

## Camp 5 — flagship

`icon.sys`/`.ico` parsing and the icon renderer, towers tagged with the user's own save
icons, the clock/main-menu scene (FNTASCII text engine, 3D clock, feedback blur), Version
Information, the browser top level. Tag `v1.5.0`.

## The PSX range — PS1, PSone, POPS, and an honest PS3 card

A second ridge beside the PS2 mountain, opened by the round-two surveys (`notes/research/README.md`,
"Round two"). The PS2 tool already climbs half of it: `ps2kit::ps1` renders the licence screen
from the PS1-mode pieces of a PS2 rom0, and every one of its locators hits every standalone PS1
shell. The app becomes the **PSX Boot Inspection Tool**: one window, a console switch
(PS1 · PS2 · PS3), each console showing only what its files can honestly support.

| step | what | where | reuse |
|---|---|---|---|
| R1 | **PS2 1.00 J loads**: `ProgramImage` from a raw LZ stream (`MOPEN`), a second texture-descriptor shape (0xE0 stride, pixel pointer at +8), textures read from the image instead of rom0 files | `ps2kit::locate`, `bios` | content scans already find slots/positions/growth/orbs |
| R2 | **`Ps1Bios`**: detect a 512 KB dump, unpack 4.x shells, version/letter/date/model, kernel cluster K1/K2, shell hash; duplicate / mislabel / text-mode-corruption audit | `ps2kit::ps1` | `rom::unpack`, `Ps1Shell` locators |
| R3 | **Standalone licence screen**: `Ps1Shell::locate(image, krom, version)` with two front ends, matrix from angles, optional reference logo, `LicencePolicy { Unconditional, Always, Never, ByLetter }` read from the shell's flag store, both wordmarks, 1 or 3 licence strings | `ps2kit::ps1`, app | licence renderer, timeline, VAB |
| R4 | **PS1 hand-off card**: `SYSTEM.CNF` with the kernel's grammar (prefix keys, hex values, the 0x180 argument), EXE loadability, the 40-entry root rule, the per-shell check, the pre-exec re-check, the drive's `GetID` answer as a declared assumption; the Scenario bar gains PS1 outcomes (licensed / unchecked / no disc / audio CD) | `ps2kit::disc` (`Ps1Verdict`), app | Scenario bar, hand-off card |
| R5 | **SCE intro with sound**: `SceIntro` + `SceTimeline` (grey fade, Gouraud diamond, CLUT-faded logotype, second note table over the same VAB); free camera on it like on the licence | `ps2kit::ps1`, renderer | per-vertex colour on the triangle path |
| R6 | **PS1 BIOS tab**: identity card, layout strip coloured by cluster, licence policy, host-modification view (retail vs PS2-embedded vs POPS), revision diff (2.0 → 2.2 showcase), boot narration (POST bytes + dummy TTY), asset browser | app | BIOS tab, ROMDIR table |
| R7 | **PS3 card**: PUP identity (header checks, members, stored digests as hex, licence locales, SCE headers, package descriptors, SELF/ELF headers), PUP-vs-PUP diff; optional bring-your-own plaintext `dev_flash` directory → `ps1_rom.bin` through R2/R3, `ps2_*emu` searched for a ROMDIR | a small `ps3pup` module/crate, app | none needed |
| R8 | **No-disc menu mock-up**: static screens from the shell's TIMs and screen table | app | asset browser |

Order: R1 and R2–R4 first (mostly reuse, and they turn every dump the user owns into a working
input), then R5 as the ridge's flagship scene, then R6 alongside Camp 4's hook map (same panel
family), R7 when convenient, R8 last. Each step ships when it is on `main` and green on CI.

Rules specific to the ridge:

- PS1, PSone, POPS and PS2 PS1-mode are all **the user's dumps**; nothing is identified by a
  shipped table of hashes — clusters are computed between the files the user loads.
- PS3: we parse plaintext container structure only. No SCE decryption, no keys shipped,
  fetched or accepted for decryption, no pointers to tools that do it, no PS3 splash look-alike.
  The only key-shaped input is an optional user-supplied HMAC check of the stored digests.
- What a ROM + disc image cannot know (the drive's `GetID` verdict, wobble-groove region, the
  controller date) is printed as an assumption, never as a finding.
- Naming: the app's display name becomes "PSX Boot Inspection Tool"; the repository and the
  `ps2kit` crate keep their names until the Summit (renames are outward-facing and need a
  deliberate decision).

## Summit — 2.0

Write-up part II, wiki pages for the new formats, ps2kit 1.0 with a frozen API, release.

## Rules of the climb

- Nothing from Sony ships; every asset is derived from the user's files or dropped in by them.
- Homages borrow motion, never artwork. No emulator inside the tool.
- Every fact the UI states cites a source (record #, ROM offset, disc LBA, note section).
- A camp is done when its exit criteria pass on all three CI platforms.
