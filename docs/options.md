# Options: modes, tabs, towers, cameras, BIOS depth, gimmicks

Merged from three contributors on 2026-10-09: **[C]** Claude (this tool's author), **[G]** Entaku seat GLM,
**[K]** Entaku seat Kimi. Where two or three proposed the same thing it is listed once with all tags.
Cost: S = days, M = a week or two, L = a milestone. Every asset is derived at run time from the
user's own ROM, memory card or disc, or dropped in by the user; nothing ships with the app.

One correction to the council's premise: the history record *does* carry a date (BCD, the last
launch), so "age" needs no inference from folder timestamps.

## A. UI/UX — modes, tabs, controls, insight

1. **Tabs** Boot / Save data / Disc / BIOS / Lab, replacing the single sidebar. [C] S
2. **Scenario bar / what-if panel** — force disc state, console region, video mode, fast boot, empty history; the boot reroutes live and the hand-off card shows which OSDSYS branch was taken. [C][G][K] M
3. **Frame-stepped scrubber** with phase ticks, hover thumbnails, backward stepping. [C][G][K] M
4. **Compare mode** — two cards, two BIOS or NTSC vs PAL side by side, frame-locked, divergences flagged. [C][G][K] M–L
5. **Provenance inspector** — click any element (tower, logo, chime, fog) → record #, ROM offset, disc LBA, VAB program; every panel cites its source address. [C][G][K] M
6. **Facts as you watch** — hand-off card lines light up as the frame passes them. [C] S
7. **kprintf boot log pane**, the kernel's own strings from the user's ROM, pinned to the timeline, click to seek. [G][K] M
8. **Memory-map visualiser** — EE RAM / IOP as ELF segments and modules land, from the hand-off data. [K] M
9. **Command palette** (Ctrl-K) to any segment, record, module or setting. [G] S
10. **Multi-card profiles** — several card images, switch or overlay histories. [G] S
11. **Photo mode** — pause, free camera FOV/DOF, PNG. [G] S
12. **Guided tour / explain overlay** on first run. [G][K] M
13. **Legibility mode** — reduced flash on the red screen, colour-blind palettes, reduced motion. [K] S
14. **Export** — PNG sheet, APNG, WAV, JSON facts, CSV history, SVG skyline; research bundle with input *hashes* only. [C][G][K] S

## B. Save-data tab — graphs and tower treatments

15. **Graph grid** — launch histogram + Pareto curve, last-launch dates over time, title-prefix donut (SLES/SLUS/SCPS…), eviction simulator. [C][G][K] M
16. **Ring / spiral / concentric layouts**, slowly revolving as the tab's background, camera at the hub. [C][G][K] S–M
17. **Colour by attribute** — count, age (record date), region letter, publisher family; auto legend. [C][G][K] S
18. **Theme packs** — Midnight, Blueprint wireframe, CRT phosphor, Paper, chrome; hue slider. [G][K] S
19. **Tag towers with the card's own icons** — `icon.sys`/`.ico` mesh billboarded above its tower; fallback: title ID in FNTASCII. [C][G][K] L
20. **Plaques from the disc** — the PS2 logo the logo screen already extracts, or user-dropped cover scans from a folder. [C][G][K] M
21. **Tower inspector card** on click — decoded ID, count, first/last date, linked saves, icon. [G] S
22. **Time-machine scrub** — drag a date, towers grow to that point. [G] M
23. **Alternate history sliders** — edit counts, inject launches, drag records, watch the skyline. [C][K] S
24. **Ghost towers** — evicted records as wireframe; orphan saves/records flagged. [C][G] M
25. **Two-card blend** — second card in a second theme, shared titles joined by a thread. [K] M
26. **District clustering** by publisher prefix. [K] S

## C. Camera paths and console homages (motion grammar only, never assets)

27. **Orbit suite** — orbit, figure-eight, street dolly, crane reveal. [C][G][K] S
28. **Tornado** — rising/descending helix, towers lean as it passes. [C][G][K] M
29. **Xbox 2001** — dive through an X-shaped particle tunnel / light-streak chase into the skyline. [C][G][K] M
30. **Xbox 360** — ribbons converge, camera rides one into orbit. [C][G][K] M
31. **GameCube** — eased 90° step orbit with a tick, or a cube rolling the streets folding into the logo. [C][G][K] S–M
32. **Wii** — crane to zenith, towers align into a channel grid in white fog. [C][G][K] M
33. **Dreamcast** — spiral dive with a floor ripple. [C][G][K] M
34. **PS1 "reverence"** — locked camera, black → logo → licence, existing renderer. [G][K] S
35. **PS3/PSP** — XMB wave glide / horizontal bar pan. [C][G][K] S
36. **Switch** — snap cuts between towers on a click. [G][K] S
37. **Saturn "assembly"** — tower polygons fly in from chaos and slot together. [K] M
38. **Director mode** — keyframe spline editor (Catmull-Rom), record from the free camera, JSON import/export, seeded shuffle of path+theme+layout. [C][G][K] L

## D. Deeper interaction with the BIOS files

39. **ROM explorer** — ROMDIR tree over rom0/rom1/rom2/erom, EXTINFO dates, hex, extract, links to the notes. [C][G][K] M
40. **Two-dump diff lab** — module-aware byte diff, entropy heat-map, Markdown/JSON report. [C][G][K] M
41. **Font atelier** — FNTASCII/FONTM/KROM sheets, type anything, PNG export. [C][G][K] M
42. **Asset gallery** — TEXC/TEXB/ICOB TIMs, OSD menu strings. [C] M
43. **NVM inspector** — named fields, what PCSX2 fabricated, edit-then-preview of the menu. [C][G][K] M
44. **Hook map** — which hidden hand-overs (MC update, DVD player, HDD OSD, TESTMODE bit) would fire for *your* files; update-hook lab where you place a dummy file and watch the log. [C][G][K] M–L
45. **Clock/main-menu scene + Version Information page.** [C] L
46. **VAB instrument bench / chime lab** — browse programs, on-screen keyboard, re-voice the chime, WAV. [C][G][K] M
47. **TESTMODE explorer/dossier** — colour bars and screen flow reconstructed, labelled as such. [C][G][K] M
48. **DECI2 pane** — the stub's protocols with annotated mock traffic. [K] M
49. **Logo/region verdict lab** — synthesise wrong-region or malformed logos and SYSTEM.CNF, watch the verdicts. [G][K] M
50. **DVD player panel** — rom1/erom modules, versions, region masks. [K] S
51. **Hidden-content dossier tab** with jump-to-byte links into the hex view. [G] S

## E. Gimmicks

52. **"Your life in towers" stats card** — total launches, top game, busiest year; share PNG. [C][G] S
53. **Tower physics toy** — flick a tower, it topples and rebuilds. [G] M
54. **Interactive disc tray** on the red screen — drop your art, watch the verdict animate. [G] S
55. **Boot remix sequencer** — reorder segments, custom durations. [G] M
56. **CRT / VHS post-FX presets.** [G][K] S
57. **Attract / screensaver mode** and wallpaper loop export. [C][G][K] S
58. **Round-number flourish** — gold pulse at 50/100/500 launches. [G] S
59. **Guess-that-tower quiz.** [K] S
60. **Skyline poster export** (SVG). [K] S
61. **Weather from play** — fog over neglected titles, sun on the favourite. [K] S
62. **Memory-card exchange** — import a friend's bundle, boot a blended skyline. [K] M
63. **PS2-on-a-TV diorama** — the boot on a CRT model in a dark room. [K] M
64. **Konami code → tornado**; audio-reactive towers from the mic. [C] S–M

## Top picks

- GLM: save-data tab + orbital background; kprintf pane; icon tagging; ROM diff lab; director mode + homages.
- Kimi: provenance inspector; icon tagging; what-if panel; path editor + seeded shuffle; ROM explorer + diff.
- Claude: what-if/scenario bar (2) first — smallest and it changes what the tool *is*; then the save-data tab (15–17) with ring layout; icon tagging (19) as the flagship; provenance inspector (5); director mode (38) last, because every camera homage becomes a JSON file once it exists.

## Dissents on record

- GLM: do **not** embed an EE/IOP emulator for "100 % fidelity"; keep reimplementation that can be cited, diffed and paused, and let PCSX2 emulate.
- Kimi: do **not** ship or fetch any logo/cover-art pack; tag only from the user's card icons, discs or dropped files, and never let a failed fetch look like "no art".
- Claude: agree with both; add that homages borrow motion only — no sphere, blades, cube, or channel-grid artwork.
