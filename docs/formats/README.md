# Tool File Formats and Specifications

This directory documents the custom file formats developed for the **PSX Boot Inspection Tool** (`ps2kit` and `psxbootinspect`).

## Stance and Philosophy

In accordance with the project's zero-shipped-assets stance:
1. **Zero Sony code, assets, tables, or decrypted payloads ship.**
2. All file formats described here are **declarative, symbolic, and metadata-focused**:
   * They describe processing and container layouts.
   * They reference user-supplied BIOS and update dumps at runtime.
   * They can be safely committed, version-controlled, and diffed without copyright infringement.

## Format Specifications

* [`pupsnap.md`](pupsnap.md) — **The `.pupsnap` Firmware Snapshot Format**:
  A deterministic, tab-delimited snapshot format for PlayStation 3 Update Packages (`PS3UPDAT.PUP`). Captures container headers, member tables, digest lists, unencrypted metadata (`version.txt`, `license.xml`), and 0x80-byte plaintext package descriptors for fast structural inspection and firmware-to-firmware diffing without decryption.
