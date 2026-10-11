# Changelog

## Unreleased

- `banks::{BankRef, BankKey, fingerprint, header_extent, sequence_extent}`: which BIOS bank a program number addresses, a portable run-time content key for any bank in the caller's files, and the true extents of SShd headers and SSsq sequences.
- `ps1menu::{Ps1Menu, MenuFrame, MenuScreen, MenuStyle, MenuSprite, MenuPolygon}` locates standalone PS1 shell menus across revisions by content. Unpacks 2-bit packed label TIMs (`0x80031800` \[V\]) with anti-aliased runtime ramps, slices atlases into individual sub-rectangles (screen 0 option labels, screen 1 CARD 1/CARD 2 headers, 15-block grids, action row, screen 3 track grid, EXIT, modes, screen 4 no-disc message), omits 3-D procedural button models with provenance notes, and reports `Error::NotFound` for redesigned menus (3.0 E / 4.5) to maintain clean display without noise.
- `sf2::{Sf2Writer, sf2_from_bench, VolEnv, adsr_to_vol_env}` exports any `SoundBench` (and optional caller-rendered `Percussion`) as a SoundFont 2 (`.sf2`, spec 2.04) file with `INFO` provenance, 16-bit mono PCM in `sdta/smpl` with 46-zero padding, presets in bank 0 (programs) and bank 1 (one-shot effects), and percussion in bank 128 preset 0. SPU ADSR envelopes map to timecents and centibels. `bench <bios> --sf2 <out.sf2> [--bank boot|logo|ps1]` provides CLI export.
- `extract::Extractor::scan` walks ISO files and embedded archives, validates VAB/VH+VB (including aligned archive bodies), SEQ, VAG, SShd/SSbd, SSsq, inferred IECS HD/BD and integer PCM WAVE; preserves native bytes, duplicate provenance and rejected-hit reasons. `Extraction::write` replaces owned Soundfonts sets with an index and local-extraction README. `extract <image> [--out <dir>]` and `extract --all <folder>` count CUE/BIN sets once. Caller oracles: Tekken EU/JP each validate 38/39 VAB hits, 20 distinct banks; Tekken Tag USA validates 2,350/2,355 VAG hits, 647 distinct samples; its SShd/SEQ/RIFF hits are invalid. `extract::{read_index, read_native, asset_bench, asset_sample, bank_parts}` read a written set back.

- `bench::SoundBench::ps2_logo` exposes the content-located logo effect bank and decoded source PCM; `SoundBench::render_song` renders a caller's sequence through the console's own sequencer and VAB voices; `SoundBench::from_samples` normalizes caller PCM/rates/loops to 44.1 kHz. `SoundBench::voice_parameters` gives the sample step and left/right gains the console uses for a tone at a key and velocity, and `SoundBench::any_sample_pcm` the PCM of any sample index including effect samples, for callers playing a bank live. No audio, fixed BIOS source indices, or BIOS tables ship.

- `ps3::{Sfo, Sfb, Ps3Disc, Ps3Save}` inspect caller-supplied PS3 disc/save metadata and documented flag fields. `ps3card <disc folder|iso|save folder> [--firmware 4.82 | --pup PS3UPDAT.PUP]` prints a cited, ordered firmware/update hand-off and file readability. ISO 9660 extents crossing encrypted sector-zero regions are never read; absent region maps remain unknown and UDF-only images are unsupported. The shared plaintext SCE prefix reader reports EBOOT headers; PFD inspection stops at magic/version and save data is listed by name/size. No keys, decryption, authentication or game-code inspection.

- `icon::{IconSys, IconModel, SaveIcon}` read the user's PS2 save descriptors, triangle/morph data and raw/RLE RGB5551 textures; preserve lighting, title bytes and animation keys, evaluate inferred shape weights, and map save folders to history title IDs. `SaveIcon::read_folder` reports malformed saves; `from_card` skips them.

- `romdiff::RomSet` compares caller-supplied PS2 modules/EXTINFO, unpacked OSDSYS and loader stores, opening/logo/sound tables, or standalone PS1 regions, with byte-exact row-local clusters and evidence-bearing chronological captions. `romdiff <dump> <dump> [...]` prints a compact identity matrix and changelog; unsupported comparisons remain explicit diagnostics.
- `hooks::HookMap` reports content-confirmed, version-specific PS2 boot probes, card update/DVD/browser paths, HDD/flash reachability, PS1 overrides and NVM/input gates with verdicts and cited addresses. `hookmap <bios> [--card f.ps2] [--nvm f.nvm] [--disc f]` discovers BIOS companions and groups the findings; live CDVD/card-authentication outcomes remain explicit unknowns.
- `nvm::{Nvm, BiosSet}` decode MechaCon EEPROM settings, validate the IOP's full-byte checksums, mask console IDs, predict cited OSD behaviour, and inspect PCSX2-named BIOS companions, DVD region sets, ROM2 mirrors, EROM gzip headers and MEC version gates. `nvminfo <bios.bin>` prints the inspection report; EROM payload CRCs remain unverified without an inflater.
- `ps1intro::SceIntro` locates a standalone shell's SCE logotypes, intro note table and code literals, with explicit fallback provenance; `SceTimeline`, `SceFrame` and the shared VAB mixer expose its fields, CLUT fades, sweep and chime. POPS and PS2 LOGO report no intro. Dump corrections to the scene notes include the four-field 1.1 lead-in, smaller 4.0 logotypes and 4.1 PAL variants.
- `ps1bios::Ps1Bios` detects standalone PS1/PSone/POPS dumps, reads their identity and kernel code features, and opens raw, standard packed and POPS shells. `audit` reports size/text-mode damage and missing/displaced versions; `audit_set` finds byte-identical files and named-region disagreements without a shipped hash list.
- `ps1::Ps1Shell::{load_ps1, locate}` reuse the PS2 licence loader with rotation derived from angles (or early-shell initialization stores), both wordmarks and all stored licence strings. `LicencePolicy` follows the shell's compare gate and flag stores. Existing `logo`/`logo_bytes` stay compatible and are empty without a reference; `reference_logo` and `comparison_logo_bytes` expose its optional presence and conditional use.
- Load SCPH-10000 (1.00 J) openings through the module and address in `rom0:OSOPEN`: raw LZ programs, content-located 0xE0 descriptors and inline textures mapped to the later texture names. Japanese/English warning masks become grey/alpha atlases in the common shape; other warning languages remain absent.
- Read 1.00's embedded boot sound bank/sequences and its two-segment, NTSC-only PS2LOGO. Plain ELF program images now retain every PT_LOAD segment, gaps and zero-filled memory.

## 0.3.0 — 2026-10-09

The boot can now lead somewhere other than the logo, the history table can be launched into,
and the PS1 logo is available to other cameras. Breaking changes are the first three lines.

- `sim::BootOutcome` (`Game`, `Ps1Game`, `Menu`, `Warning`) and `sim::BootPlan`; `BootSequence::from_plan` builds the segments the outcome calls for, `BootSequence::new` is the `Game` shorthand. `BootSequence` gains `outcome`, `warning` (the warning scene's timeline) and `warning_start()`.
- `sim::Segment` is `#[non_exhaustive]` and gains `Warning` and `Menu`; a sequence no longer always has five segments. `with_logo` sets `outcome = Ps1Game` and is a no-op on a sequence without a logo segment.
- `disc::HandoffStep::IllegalDisc` (state 0x74); `disc::boot_outcome(&[HandoffStep], enforce)` reads the outcome off the facts.
- The `RegionCheck` sentence for a rejected disc no longer ends with "The tool does not enforce region locks." (it can now).
- `RomDir::entries()` lists every module as `(name, offset, size)` in image order.
- `ps1::Ps1Shell::lit_triangles(tmd, field) -> Vec<LitTri>`: the logo's lit, depth-cued triangles in GTE camera space before projection, for other cameras; `project` is built on it and unchanged.
- `PlayHistory::launch(title_id, date, &mut SplitMix) -> LaunchEffect`: the console's `HistoryUpdate` applied to the table (growth, tower bits, freeze at 63, random empty slot, eviction with the console's tie-break quirk, drop when all maxed).

## 0.2.0 — 2026-10-08

Breaking changes (one per line). Everything the readers compute, every number and every
hand-off sentence are unchanged; this release reshapes the API.

- `VideoMode` lives at the crate root (`ps2kit::VideoMode`); `sim::VideoMode` is a deprecated alias for one release, and `VideoMode::name()` is gone (a UI label; see the app).
- `Error` is now `NotA(Format, String)`, `Corrupt(Format, String)`, `NotFound(Format, String)`, `UnsupportedVersion(String)`, `Io(io::Error)` with a new `Format` enum; `NotACard`/`CardCorrupt`/`CardNotFound`/`NotABios`/`MissingModule`/`Bank`/`Sequence` are folded in. The `Display` texts are unchanged.
- `sound::write_wav` returns the crate `Result` instead of `io::Result`.
- Path arguments are `impl AsRef<Path>`: `MemoryCard::open`, `DiscImage::open`, `SectorReader::open`, `DiscLogo::read`, `Tmd::from_disc`, `write_wav`, `is_disc_image`.
- `RomDir::data` is private: `data()` and `into_data()`.
- `SectorReader::{sector_size, data_offset}` are private: getters of the same names.
- `locate::Image` is `locate::ProgramImage` with private fields: `new(data, base)`, `data()`, `base()`.
- `locate::{OsdLayout, LogoLayout, driver_tables}` are crate-private; `sound::DriverTables::from_driver(module)` locates the tables itself (the two offset arguments are gone).
- `SoundBank::body` and `LogoAssets::sample_body` are private: `body()` and `sample_body()`.
- `bios::OpeningAssets` has no public fields: `rom_version()`, `slots() -> &[[Slot; 6]; 21]`, `growth(step) -> Option<(fill, size)>`, `tower_position(column, row) -> Option<Vec3>`, `orb_colours() -> &[Vec3; 4]`, `cube_positions()`, `prism_positions()`, `texture(name)`, `textures()`; new `Slot { column, row }`, `SLOTS_PER_RECORD`, `GROWTH_STEPS`.
- New `ps2kit::Region` (`Japan`, `America`, `Europe`, `China`, `Asia`) with `from_romver_letter` and `from_title_id`; `disc::handoff_steps` and `Ps1Verdict::judge` take `Option<Region>` instead of `Option<char>` (an unrecognised letter is now `None`, i.e. "not checked", where 0.1 said "console ROM is unknown").
- `Ps1Licence::region` is `Option<Region>`; `DiscLogo::region` and `DiscImage::logo_region` are `Option<logo::LogoMaster>` (`Europe`, `JapanAmerica`; `Display` gives `E` / `J/A`).
- `disc::HandoffStep` is an enum of facts (`RegionCheck`, `Ps2DiscIdentified`, `LaunchRequest`, `SystemCnf`, `HistoryUpdate { before, after, new_tower }`, `LoadExec { module, argv }`, `LogoCheck`, `LoadElf`/`ExecElf`/`ElfSegment`, `Ps1RegionCheck`, `Ps1SystemArea`, `LoadPsxExe`, `Stop`, ...) with `who()` and `Display` reproducing 0.1's `who` / `what` strings exactly; new `RegionVerdict`, `LogoVerdict`, `Ps1LogoState`, `BootModule`.
- `history::PlayHistory::next_record(title_id) -> (Record, bool)` is the console's history-update rule the hand-off used to format inline.
- `logo::Node::kind` is a `NodeKind` (`Move`, `Line`, `Cubic`, `End`); `logo::Object::kind` is an `ObjectKind` (`LineStrips`, `Ribbon`).
- `logo::LogoAnimation::animated_fields(video)` is the one source of the 17..=42 / 14..=35 field range; `Timeline::logo` is built from it.
- `history::SplitMix::next` is `next_u64`, and `SplitMix` implements `Iterator<Item = u64>`.
- `sim::SceneKind` loses `Full` (an app mode, never produced by a `Timeline`), gains `Ps1Licence` (what `Timeline::ps1_licence` now reports), and loses `name()` and `ALL`.
- `sim::BootPhase` is an enum of phases with `seconds()`; the English captions are gone (see the app's `phase_caption`). `POWER_ON_PHASES`, `HANDOFF_PHASES`, `PS1_HANDOFF_PHASES` hold the variants; `phase_at` returns `Option<BootPhase>` by value.
- `sim::Timeline::states` and `sim::BootSequence::spans` are private: `states()` and `spans()`.
- `#[non_exhaustive]` on `disc::{ElfSegment, BootElf, DiscKind, Ps1Exe, Ps1Licence, Ps1Verdict, DiscImage}`, `ps1::{Prim, Tmd, Tim, NoteEvent, Ps1Shell, ScreenTri}`, `sim::{SceneKind, Tower, OpeningScene, Timeline, Span, BootSequence}` and the new enums; `OpeningAssets` drops it (no public fields).
- Minimum supported Rust is 1.87 (`is_multiple_of`).

Documented: every public item of `disc`, `ps1` and `sim` (the crate builds with
`#![warn(missing_docs)]` and no exemptions).

## 0.1.0

First release.
