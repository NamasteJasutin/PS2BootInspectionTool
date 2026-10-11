# Tool File Formats and Specifications

This directory documents the custom file formats and synthesis architectures developed for the **PSX Boot Inspection Tool** (`ps2kit` and `ps2bootinspect`).

## Stance and Philosophy

In accordance with the project's zero-shipped-assets stance:
1. **Zero Sony code, assets, tables, or decrypted payloads ship.**
2. All file formats described here are **declarative, symbolic, and metadata-focused**:
   * They describe musical structures, processing algorithms, and container layouts.
   * They reference user-supplied BIOS and update dumps at runtime.
   * They can be safely committed, version-controlled, and diffed without copyright infringement.

## Format Specifications

* [`psxsong.md`](file:///Users/jasutin/Dev/PS2BootScreen/.worktrees/w1-wiki/docs/formats/psxsong.md) — **The `.psxsong` Musical Format**:
  A line-oriented, canonical, diffable text format representing multi-channel musical arrangements, patterns, notes, and DSP parameters. Covers versions 1 through 3, instrument models (`Instrument::Program`, `Instrument::Effect`, `Instrument::Drum`, `Instrument::Kit`), bank keys (`BankRef`), content fingerprints, and strict parser migration rules.

* [`pupsnap.md`](file:///Users/jasutin/Dev/PS2BootScreen/.worktrees/w1-wiki/docs/formats/pupsnap.md) — **The `.pupsnap` Firmware Snapshot Format**:
  A deterministic, tab-delimited snapshot format for PlayStation 3 Update Packages (`PS3UPDAT.PUP`). Captures container headers, member tables, digest lists, unencrypted metadata (`version.txt`, `license.xml`), and 0x80-byte plaintext package descriptors for fast structural inspection and firmware-to-firmware diffing without decryption.

* [`drum-kit-recipes.md`](file:///Users/jasutin/Dev/PS2BootScreen/.worktrees/w1-wiki/docs/formats/drum-kit-recipes.md) — **Drum Voice Recipes & Kit Derivation**:
  The mathematical and DSP specification for `DrumVoice` parameters and the algorithmic selection of General MIDI drum lanes in `Kit::derive`. Explains how raw BIOS audio assets are categorized by FFT spectral analysis (centroid brightness, band flatness, harmonic peaks) and verified against empirical acoustic thresholds (`DrumMetrics`).
