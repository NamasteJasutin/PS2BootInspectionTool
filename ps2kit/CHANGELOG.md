# Changelog

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
