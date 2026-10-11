# Drum Voice Recipes and Algorithmic Kit Derivation

This document specifies the DSP architecture of `DrumVoice` and the algorithmic derivation of General MIDI-like drum kits (`Kit::derive`) in `ps2kit::song`.

The tool ships **no drum samples, audio tables, or hardcoded sample indices**. Instead, drum instruments are synthesized on the fly from the user's own BIOS audio assets using measured spectral analysis, pitch shifting, envelope shaping, filtering, layering, and retriggering.

---

## 1. `DrumVoice` Parameter Model

A `DrumVoice` represents an authored one-shot DSP recipe applied to a caller-supplied sample reference (`SampleRef`). All processing is calculated in 48 kHz stereo floating-point PCM before rendering.

### Struct Fields

```rust
pub struct DrumVoice {
    pub source: SampleRef,
    pub start_ms: f32,
    pub pitch: f32,
    pub pitch_drop: f32,
    pub pitch_drop_ms: f32,
    pub decay_ms: f32,
    pub attack_ms: f32,
    pub low_pass_hz: Option<f32>,
    pub high_pass_hz: Option<f32>,
    pub noise_mix: Option<NoiseLayer>,
    pub gain: f32,
    pub retriggers_ms: Vec<f32>,
}
```

* `source`: `SampleRef` addressing either a raw sample index (`SampleSource::Sample(usize)`) or a program tone (`SampleSource::Program { program: u8, key: u8 }`) within `BankRef::Ps2`, `BankRef::Ps1`, or `BankRef::Ps2Logo`.
* `start_ms`: Milliseconds of recorded audio to skip at the start of the source sample. Allows skipping ambient lead-in swells or silent attack offsets.
* `pitch`: Target pitch transposition in semitones.
* `pitch_drop`: Initial pitch envelope offset in semitones, falling linearly to 0 over `pitch_drop_ms`. Essential for percussive pitch sweeps (kick drums, toms).
* `pitch_drop_ms`: Duration of the pitch drop envelope in milliseconds.
* `decay_ms`: Exponential decay envelope duration to -60 dB, followed by a short linear fade to zero.
* `attack_ms`: Linear attack time in milliseconds.
* `low_pass_hz`: Optional one-pole low-pass filter cutoff frequency in Hz.
* `high_pass_hz`: Optional one-pole high-pass filter cutoff frequency in Hz.
* `noise_mix`: Optional secondary layer (`NoiseLayer { source, start_ms, pitch, mix }`) using a high-passed noise source from the bank.
* `gain`: Pre-limiter linear gain multiplier (0.0..=8.0).
* `retriggers_ms`: List of millisecond offsets for additional attack strikes (simulating handclaps).

### Validation Bounds (`DrumVoice::validate`)

To guarantee real-time stability and bound memory consumption, `DrumVoice::validate` enforces finite numerical constraints:

| Parameter | Minimum | Maximum | Unit / Description |
|---|---|---|---|
| `start_ms` | 0.0 | 120,000.0 | Milliseconds (max 2 minutes) |
| `pitch` | -96.0 | +96.0 | Semitones (±8 octaves) |
| `pitch_drop` | -48.0 | +48.0 | Semitones (±4 octaves) |
| `pitch_drop_ms` | 0.0 | 1,000.0 | Milliseconds |
| `decay_ms` | 1.0 | 1,500.0 | Milliseconds (max 1.5 seconds) |
| `attack_ms` | 0.0 | `decay_ms` | Cannot exceed decay duration |
| `gain` | 0.0 | 8.0 | Linear factor |
| `low_pass_hz`, `high_pass_hz` | 10.0 | 20,000.0 | Filter cutoff in Hz |
| `retriggers_ms` length | 0 | 8 | Maximum 8 retrigger strikes |
| `retriggers_ms` offsets | 0.0 | 400.0 | Milliseconds from initial attack |
| `noise_mix.mix` | 0.0 | 4.0 | Linear noise layer mix factor |

---

## 2. Spectral Source Selection (`Kit::derive`)

When building a drum kit from a loaded sound bank (`SoundBench`), `Kit::derive` performs an automated spectral survey across all decoded PCM samples to identify optimal raw sources:

1. **Transient Location**: Scans each sample in 512-sample chunks to identify the loudest window, determining `start_ms` and bypassing initial silence.
2. **FFT Analysis**: Computes a 32,768-point power spectrum at 44.1 kHz.
3. **Spectral Metrics**:
   * **Spectral Centroid / Brightness**: Frequency-weighted power sum divided by total power (`Hz`).
   * **Spectral Flatness**: Ratio of geometric mean to arithmetic mean in the 2 kHz–16 kHz band. High flatness indicates broadband, noise-like signals; low flatness indicates tonal signals.
   * **Peak Frequency**: Highest harmonic peak between 30 Hz and 8,000 Hz.
4. **Source Classification**:
   * `dark`: Sample with minimum brightness and a detectable peak (`peak > 0.0`).
   * `noise`: Sample with `flatness > 0.015` and `brightness > 1500.0 Hz`, maximizing `flatness * brightness`.
   * `body`: Sample with lowest spectral flatness among programs classified as `Style::MalletsAndPlucks`, or a dark tonal sample (`peak > 0.0` and `brightness < 1500.0 Hz`).
   * `effects`: Shortest one-shot effect by sample duration.

---

## 3. General MIDI Lane Recipes

Using the identified candidates, `Kit::derive` constructs specific GM-compatible drum lane voices:

### Kick (MIDI Key 36)
* **Source**: `dark` candidate.
* **Tuning**: Transposed to 65 Hz fundamental:
  :<code>pitch = (12.0 * log2(65.0 / dark.peak)).clamp(-96.0, -6.0)</code>
* **Envelope & Filter**: `pitch_drop = 18.0`, `pitch_drop_ms = 25.0`, `decay_ms = 250.0`, `attack_ms = 1.0`.
* **Filtering**: `low_pass_hz = 100.0`, `high_pass_hz = 20.0`, `gain = 2.0`.

### Closed Hi-Hat (MIDI Key 42)
* **Source**: `noise` candidate.
* **Tuning & Envelope**: `pitch = +12.0`, `decay_ms = 40.0`, `attack_ms = 0.3`.
* **Filtering**: `high_pass_hz = 10000.0`, `gain = 2.0`.

### Open Hi-Hat (MIDI Key 46)
* **Source**: `noise` candidate.
* **Tuning & Envelope**: `pitch = +12.0`, `decay_ms = 250.0`, `attack_ms = 0.3`.
* **Filtering**: `high_pass_hz = 10000.0`, `gain = 2.0`.

### Handclap (MIDI Key 39)
* **Source**: `noise` candidate.
* **Tuning & Envelope**: `pitch = 0.0`, `decay_ms = 100.0`, `attack_ms = 0.3`.
* **Filtering & Flam**: `high_pass_hz = 1500.0`, `gain = 2.0`, `retriggers_ms = [12.0, 24.0]`.

### Snare (MIDI Key 38)
* **Source**: `body` candidate layered with `noise` candidate.
* **Tuning**: Tonal body tuned to 220 Hz:
  :<code>pitch = (12.0 * log2(220.0 / body.peak)).clamp(-96.0, 96.0)</code>
* **Filtering**: `low_pass_hz = 450.0`, `high_pass_hz = 100.0`, `decay_ms = 180.0`, `gain = 1.5`.
* **Layer**: `noise_mix` set to `noise` candidate with `mix = 1.0` and `pitch = 0.0`.

### Rim / Click (MIDI Key 37)
* **Source**: Shortest OSD effect sample.
* **Envelope & Filter**: `decay_ms = 35.0`, `high_pass_hz = 500.0`.

### Toms (Low: Key 45, Mid: Key 47, High: Key 50)
* **Source**: `body` candidate.
* **Tuning**:
  * Low Tom (Key 45): tuned to 110.0 Hz.
  * Mid Tom (Key 47): tuned to 147.0 Hz.
  * High Tom (Key 50): tuned to 196.0 Hz.
* **Envelope & Filter**: `decay_ms = 220.0`, `low_pass_hz = 800.0`, `high_pass_hz = 40.0`.

---

## 4. Empirical Acoustic Verification (`DrumMetrics` & `Kit::offer`)

A synthesized voice is not accepted into the kit merely because a recipe was constructed. Every recipe is rendered to PCM and evaluated against objective acoustic criteria using `DrumMetrics`:

```rust
pub struct DrumMetrics {
    pub rms: f32,
    pub below_150_hz: f32,
    pub above_5khz: f32,
    pub above_2khz: f32,
    pub body_peak_hz: f32,
    pub decay_30db_ms: f32,
    pub duration_ms: f32,
}
```

### Measured Acceptance Gates

To be included in `kit.pieces`, the rendered voice must satisfy:
* **Non-Silence**: `rms > 1e-5` for all pieces.
* **Kick (Key 36)**:
  * `below_150_hz > 0.5`: more than 50% of spectral energy must reside in the sub-bass band (< 150 Hz).
  * `decay_30db_ms <= 400.0`: transient must decay by 30 dB within 400 ms (no lingering resonance).
* **Closed Hi-Hat (Key 42)**:
  * `above_5khz > 0.5`: more than 50% of spectral energy must be high-frequency (> 5 kHz).
  * `duration_ms < 80.0`: total sounding duration must be under 80 ms.
* **Snare (Key 38)**:
  * `body_peak_hz` in `150.0..=300.0`: tonal fundamental must land in the classic 150–300 Hz snare body window.
  * `above_2khz > 0.05`: snare wires must contribute at least 5% high-frequency energy.
  * `duration_ms < 300.0`: length must not exceed 300 ms.
* **Open Hi-Hat (Key 46)**:
  * `decay_30db_ms >= 80.0`: must sustain audibly (at least 80 ms to -30 dB).
  * `duration_ms <= 300.0`: must not exceed 300 ms.

### Explicit Omission Tracking

If a candidate source does not exist in the bank, or if the rendered voice fails any measurement check, the failure is recorded in `kit.omissions` with the exact measured metrics:
```text
Omission "Kick: source failed checks (darkest sample 2; low150=42.1%, -30dB=450 ms, RMS=0.012)"
```
This guarantees complete transparency: the user can see exactly why a particular drum voice was omitted rather than hearing a silent note or a poorly synthesized facsimile.

''Source: [https://github.com/NamasteJasutin/PS2BootInspectionTool PS2BootInspectionTool], ps2kit/src/song.rs (`Kit::derive`, `DrumVoice`, `DrumMetrics`).''
