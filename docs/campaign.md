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
`PlayHistory::launch`), tower inspector (hover → record, highlighted), CSV/text exports.
Open: Pareto curve, themes, time-machine scrub, ghost towers, SVG export.

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

## Summit — 2.0

Write-up part II, wiki pages for the new formats, ps2kit 1.0 with a frozen API, release.

## Rules of the climb

- Nothing from Sony ships; every asset is derived from the user's files or dropped in by them.
- Homages borrow motion, never artwork. No emulator inside the tool.
- Every fact the UI states cites a source (record #, ROM offset, disc LBA, note section).
- A camp is done when its exit criteria pass on all three CI platforms.
