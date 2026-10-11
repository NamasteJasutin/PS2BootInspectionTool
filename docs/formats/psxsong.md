# The `.psxsong` Format Specification

The `.psxsong` format is a line-oriented, human-readable, and version-controlled text format developed for `ps2kit::song`. It represents musical compositions, arranger patterns, and sound recipes derived from PlayStation 1 and PlayStation 2 boot and OSD sound assets.

In keeping with the project's core principles:
* **No audio bytes or samples ship in `.psxsong` files.**
* Songs store **symbolic note data and DSP parameter recipes** referencing the user's own BIOS banks at runtime.

---

## 1. Syntax and Lexical Grammar

A `.psxsong` file is encoded in **UTF-8**, with one record per line.
* Blank lines and lines beginning with `#` (comments) are ignored by the parser.
* Strings are enclosed in double quotes (`"..."`) and support standard C-style escapes: `\"`, `\\`, `\n`, `\r`, `\t`.
* Unquoted tokens are delimited by whitespace (spaces or tabs).
* Optional fields are indicated by a hyphen (`-`).
* Maximum file size accepted by `Song::parse` is **4 MiB** (`MAX_TEXT = 4 << 20`), and record count is bounded by `MAX_ITEMS * 4` (32,768 records).

---

## 2. File Structure and Line Types

A valid file consists of three sequential blocks: the **Header**, zero or more **Annotations**, and one or more **Channel** definitions.

### 2.1 Header

The header begins with a version declaration, followed by configuration directives:

```text
psxsong <version>
name "<song_name>"
bank <bank_tag>
clock <bpm> <ticks_per_beat> <beats_per_bar>
mix <sequence_volume> <tail_ticks> <end_tick|->
loop <start_bar> <end_bar> | -
```

* `psxsong <version>`: Format version (`1` or `2` in current implementation; `3` in library specification).
* `name "<song_name>"`: Quoted song title string.
* `bank <bank_tag>`: Default sound bank for the song:
  * `ps1`: PlayStation 1 shell VAB bank (`BankRef::Ps1`).
  * `ps2`: PlayStation 2 boot/ambience instrument bank (`BankRef::Ps2`).
  * `logo`: PlayStation 2 game-logo effect bank (`BankRef::Ps2Logo`, valid in version ≥ 2).
* `clock <bpm> <ticks_per_beat> <beats_per_bar>`:
  * `bpm`: Beats per minute (`u32`, typically 90–120).
  * `ticks_per_beat`: PPQN resolution (`u32`, typically 480).
  * `beats_per_bar`: Time signature numerator (`u32`, typically 4).
* `mix <sequence_volume> <tail_ticks> <end_tick|->`:
  * `sequence_volume`: Master playback volume (`u8`, 0–127).
  * `tail_ticks`: Release/reverb tail hold ticks after last note (`u32`).
  * `end_tick`: Explicit playback cutoff tick (`Option<u32>`, `-` if unbounded).
* `loop <start_bar> <end_bar> | -`:
  * Loop range in exclusive bar indices (`Option<(usize, usize)>`), or `-` if non-looping.

### 2.2 Annotations

Zero or more metadata annotations may precede the channels:
```text
annotation "<note_text>"
```
Used by import tools to document provenance or timing adjustments (e.g. `Song::from_ps1_licence` or `Song::from_sequence`).

### 2.3 Channels

Each channel defines an instrument, an arrangement timeline across bars, and pattern note data:

```text
channel "<channel_name>" <instrument_tag> <volume> <pan>
[voice <key|-> "<name>" "<reason>" ...]
[omission "<reason>"]
arrangement <cell_0> <cell_1> ... <cell_n>
[pattern <length_ticks>
 note <start> <length> <key> <velocity>
 ...
 endpattern]
endchannel
```

* `channel <name> <instrument_tag> <volume> <pan>`:
  * `name`: Quoted channel label.
  * `instrument_tag`:
    * Numeric string (e.g. `0`–`127`): MIDI program ID (`Instrument::Program`).
    * `effect:<id>`: One-shot effect ID in bank (`Instrument::Effect`, version ≥ 2).
    * `drum`: Single DSP drum voice (`Instrument::Drum`, version ≥ 2).
    * `kit`: Multi-lane drum kit (`Instrument::Kit`, version ≥ 2).
  * `volume`: Channel volume (`u8`, 0–127).
  * `pan`: Stereo panning (`u8`, 0 = left, 64 = center, 127 = right).

#### Voice Directive (`voice`)
Required for `drum` and `kit` channels (must appear before `arrangement`):
```text
voice <key> "<name>" "<reason>" <bank> <source> <start_ms> <pitch> <pitch_drop> <pitch_drop_ms> <decay_ms> <attack_ms> <low_pass_hz> <high_pass_hz> <gain> <triggers> <noise_bank> <noise_source> <noise_start> <noise_pitch> <noise_mix>
```
* `key`: MIDI note key (e.g. `36` for Kick, or `-` for a single drum channel).
* `name`: Quoted voice label.
* `reason`: Quoted spectral reason / derivation rationale.
* `bank`: Source bank tag (`ps1`, `ps2`, or `logo`).
* `source`: Sample reference: `s<index>` (raw sample index) or `p<prog>:<key>` (tone lookup).
* `start_ms`: Initial sample skip offset in milliseconds (`f32`, 0.0..=120000.0).
* `pitch`: Semitone tuning offset (`f32`, -96.0..=96.0).
* `pitch_drop`: Initial pitch drop in semitones (`f32`, -48.0..=48.0).
* `pitch_drop_ms`: Duration of pitch drop in milliseconds (`f32`, 0.0..=1000.0).
* `decay_ms`: Envelope decay time to -60 dB in milliseconds (`f32`, 1.0..=1500.0).
* `attack_ms`: Linear attack time in milliseconds (`f32`, 0.0..=decay_ms).
* `low_pass_hz`: Optional low-pass cutoff in Hz (`f32`, 10.0..=20000.0, or `-` for bypass).
* `high_pass_hz`: Optional high-pass cutoff in Hz (`f32`, 10.0..=20000.0, or `-` for bypass).
* `gain`: Output gain factor before soft clipping (`f32`, 0.0..=8.0).
* `triggers`: Comma-separated millisecond offsets for retrigger attacks (e.g. `12.0,24.0`, or `-`). Up to 8 retrigger offsets (0.0..=400.0).
* `noise_bank` / `noise_source` / `noise_start` / `noise_pitch` / `noise_mix`: Parameters for optional secondary noise layer (or `- - 0 0 0` if omitted).

#### Omission Directive (`omission`)
For `kit` channels, records lanes that could not be derived from the available bank:
```text
omission "<reason_string>"
```

#### Arrangement Directive (`arrangement`)
Defines the pattern indices assigned to each bar:
```text
arrangement 0 0 1 - 0 1
```
Each cell is either a 0-based pattern index or `-` for an empty bar.

#### Pattern Blocks
```text
pattern <length_ticks>
note <start> <length> <key> <velocity>
endpattern
```
* `pattern <length_ticks>`: Length of the pattern in ticks (must equal `ticks_per_beat * beats_per_bar`).
* `note <start> <length> <key> <velocity>`:
  * `start`: Tick offset within pattern (`u32`).
  * `length`: Duration in ticks (`u32`, > 0).
  * `key`: MIDI key (`u8`, 0–127).
  * `velocity`: Note velocity (`u8`, 1–127).

---

## 3. Format Versions and Capabilities

### Version 1 (`psxsong 1`)
* **Legacy Program Songs**: Designed for melodies arranged using standard ROM instrument programs.
* Supports only `bank ps1` and `bank ps2`.
* Channel instruments must be integer program IDs (0–127).
* No `voice`, `omission`, or effect tags permitted.

### Version 2 (`psxsong 2`)
* **Extended Instrument Songs**: Adds support for one-shot effects and sample-derived DSP synthesis.
* Adds `bank logo` (`BankRef::Ps2Logo`).
* Supports `Instrument::Effect(usize)` via `effect:<id>`.
* Supports `Instrument::Drum(DrumVoice)` and `Instrument::Kit(Kit)` via `voice` and `omission` directives.
* Strictly validates parameter bounds (no `NaN`, `inf`, or out-of-range floats).

### Version 3 (`psxsong 3` — Multi-Bank & Fingerprinted Library Specification)
* **Library Scoping**: Rather than relying on rigid, single-bank assumptions (`ps1` or `ps2`), version 3 introduces explicit bank fingerprints.
* **Bank Fingerprints**: Banks are identified by domain-separated SHA-256 hashes of their parsed header and declared body:
  * Prevents ambiguity between different console ROM versions (e.g. SCPH-10000 12-entry effect bank B05 vs later 14-entry bank B04).
* **Multi-Bank Arrangements**: Channels may specify independent bank bindings, allowing a single composition to blend PS1 chimes with PS2 boot drones.
* **Portable Migration**: Songs retain human-readable role hints (e.g. `role:ps2_boot_ambience`) so that when opened on a machine with a different BIOS dump, compatible instruments are rebound automatically.

---

## 4. Migration and Compatibility Rules

1. **Automatic Serialization Upgrade**:
   `Song::to_text` automatically emits `psxsong 2` if:
   * `song.bank == BankRef::Ps2Logo`, or
   * Any channel uses an instrument other than `Instrument::Program`.
   If a song only uses programs on `ps1` or `ps2`, it emits canonical `psxsong 1`.
2. **Downward Compatibility**:
   `Song::parse` rejects any file where `psxsong 2` features are marked as `psxsong 1` (e.g. `effect:`, `drum`, `kit`, or `logo` bank in a version 1 file).
3. **Strict Parameter Validation**:
   The parser enforces strict bounds during loading:
   * Floating-point fields must be finite (`!v.is_finite()` causes syntax errors).
   * Note lengths must be non-zero.
   * Pattern lengths must be consistent.
   * Total text length cannot exceed 4 MiB.
   * Any syntax failure reports the exact 1-based line number.

---

## 5. Complete Example (Version 2)

```text
psxsong 2
name "Opening Groove"
bank ps2
clock 90 480 4
mix 100 960 -
loop 0 2
annotation "Imported from PS2 boot sequence with derived kit"
channel "Kick & Hat" kit 100 64
voice 36 "Kick" "darkest sample 0, centroid 340 Hz, peak 65 Hz" ps2 s0 0.0 -12.0 18.0 25.0 250.0 1.0 100.0 20.0 2.0 - - - 0 0 0
voice 42 "Closed hat" "bright noise sample 3, centroid 3200 Hz" ps2 s3 0.0 12.0 0.0 30.0 40.0 0.3 - 10000.0 2.0 - - - 0 0 0
omission "Snare: noise exists but no tonal body"
arrangement 0 0
pattern 1920
note 0 240 36 110
note 480 120 42 90
note 960 240 36 100
note 1440 120 42 95
endpattern
endchannel
```

''Source: [https://github.com/NamasteJasutin/PS2BootInspectionTool PS2BootInspectionTool], ps2kit/src/song.rs and notes/research/sound_inventory.md §7.''
