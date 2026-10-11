# The `.pupsnap` Format Specification

The `.pupsnap` format is a line-oriented, deterministic, and diffable text format implemented in `ps2kit::pup`. It serializes the complete plaintext metadata, table structures, and package descriptors of a PlayStation 3 Update Package (`PS3UPDAT.PUP`) without extracting, storing, or decrypting any copyrighted Sony payloads.

---

## 1. Design Purpose and Ethics

A standard `PS3UPDAT.PUP` file is approximately 200 MB in size and contains encrypted system firmware. Transporting or diffing raw PUP files across environments is bandwidth-heavy and legally fraught.

The `.pupsnap` format:
* Extracts **only unencrypted container metadata, archive tables of contents, and package descriptors** (producing a file under 50 KB).
* Never stores, parses, or requires decryption keys.
* Enables strict structural verification and firmware-to-firmware diffing via the `pupinfo --diff` CLI command.

---

## 2. Syntax and Lexical Structure

* **Encoding**: UTF-8.
* **Magic Header**: The very first line must be exactly:
  ```text
  pupsnap 1
  ```
  Any file beginning with a different string or version identifier is rejected with `Error::NotA(Format::Bios, ...)`.
* **Record Structure**: Every subsequent line represents a single key-value record, delimited by an ASCII tab character (`\t`):
  ```text
  <section>.<field><TAB><value>
  ```
* **Ordering**: Records are sorted alphabetically within each section.
* Lines missing the tab delimiter or containing malformed numbers cause parser termination with `Error::Corrupt`.

---

## 3. Section and Field Reference

The code structure in `ps2kit::pup` serializes fields into the following hierarchical sections:

### 3.1 `header` — PUP Container Header
Fields read directly from the 0x00–0x30 container header:
* `header.package_version`: Outer PUP format version (`u64`, typically `1`).
* `header.image_version`: Build version counter (`u64`, formatted as `{:#x}`, e.g. `0x105f2`).
* `header.file_count`: Total number of container members (`u64`, typically `9`).
* `header.header_length`: Length of the outer container header in bytes (`u64`, typically `656`).
* `header.data_length`: Total byte length of member data (`u64`).
* `header.header_digest`: 40-character hexadecimal string of the 20-byte HMAC-SHA1 header digest.

### 3.2 `meta` — Plaintext Metadata Files
Strings extracted from unencrypted container members:
* `meta.version`: Firmware version string from `version.txt` (ID `0x100`, e.g. `4.82`).
* `meta.update_flags`: Flags string from `update_flags.txt` (ID `0x103`, e.g. `0000`).
* `meta.dots`: Progress dot string from `dots.txt` (ID `0x202`, e.g. `...`).

### 3.3 `entry.<index:03>` — File Entry Table
One entry per member in the PUP header table (0-indexed, 3-digit zero-padded):
* `entry.<index>.id`: 64-bit member identifier (`{:#x}`, e.g. `0x100`, `0x300`, `0x200`).
* `entry.<index>.offset`: Byte offset within the PUP container (`u64`).
* `entry.<index>.size`: Size of the member in bytes (`u64`).
* `entry.<index>.digest`: 40-character hexadecimal HMAC-SHA1 digest of the member.

### 3.4 `licence.<index:03>` — EULA Locales
Extracted from `license.xml` (ID `0x101`) across all supported languages:
* `licence.<index>.lang`: Language code string (e.g. `en`, `ja`, `zh-s`).
* `licence.<index>.bytes`: Byte length of the locale XML block (`usize`).
* `licence.<index>.date`: Date string parsed from `msg_update_eula_2` (e.g. `December 10, 2009`).

### 3.5 `tar_300.<index:03>` & `tar_501.<index:03>` — Tar Directory Entries
Directory entries from `update_files.tar` (ID `0x300`) and `spkg_hdr.tar` (ID `0x501`):
* `tar_300.<index>.name`: Archive member filename string (e.g. `CORE_OS_PACKAGE.pkg`).
* `tar_300.<index>.size`: File size in bytes (`u64`).
* `tar_300.<index>.mtime`: Modification time in seconds since UNIX epoch (`u64`).

### 3.6 `pkg.<index:03>` — Package Metadata and Descriptors
Metadata for each package within `update_files.tar`:
* Container fields:
  * `pkg.<index>.name`: Member filename string.
  * `pkg.<index>.member_size`: Tar member size in bytes (`u64`).
  * `pkg.<index>.mtime`: UNIX timestamp (`u64`).
  * `pkg.<index>.sce_ver`: SCE container version (`u32`, typically `2`).
  * `pkg.<index>.sce_key_rev`: Key revision index (`u16`).
  * `pkg.<index>.sce_hdr_type`: Header type (`u16`, `3` for PKG, `2` for RVK).
  * `pkg.<index>.sce_meta_off`: Metadata offset (`u32`).
  * `pkg.<index>.sce_hdr_len`: Header length (`u64`, typically `0x280`).
  * `pkg.<index>.sce_data_len`: Declared uncompressed payload length (`u64`).
  * `pkg.<index>.spkg_matched`: Counterpart match in `spkg_hdr.tar` (`true`, `false`, or `none`).
  * `pkg.<index>.uncomp_ok`: Boolean indicating whether `pkg_descriptor.uncompressed_size == sce_data_len - 0x80`.
  * `pkg.<index>.stored_ok`: Boolean indicating whether `pkg_descriptor.stored_size == member_size - 0x300`.
* Plaintext PKG Descriptor fields (for type-3 packages):
  * `pkg.<index>.pkg_constant`: Constant (`u32`, always `3`).
  * `pkg.<index>.pkg_kind`: Kind (`u32`, 1 = CoreOS, 3 = flash tar, 4 = UPL.xml, 7 = BD drive fw, 8 = syscon fw).
  * `pkg.<index>.pkg_seq`: Sequence ID (`{:#x}`).
  * `pkg.<index>.pkg_stamp`: Timestamp or version word (`{:#x}`).
  * `pkg.<index>.pkg_uncomp`: Uncompressed size in bytes (`u64`).
  * `pkg.<index>.pkg_stored`: Stored ciphertext size in bytes (`u64`).
  * `pkg.<index>.pkg_flags`: Descriptor flags (`{:#x}`).
* Plaintext RVK Descriptor fields (for type-2 revocation lists):
  * `pkg.<index>.rvk_entries`: Number of revocation entries (`u32`).
  * `pkg.<index>.rvk_kind`: Revocation list type (`u32`).
  * `pkg.<index>.rvk_seq`: Sequence number (`{:#x}`).
  * `pkg.<index>.rvk_stamp`: Version stamp (`{:#x}`).

### 3.7 `self.<index:02>` — Updater SELF Headers
Extracted from `ps3swu.self` (ID `0x200`) and `ps3swu2.self` (ID `0x601`):
* `self.<index>.id`: Member ID (`{:#x}`).
* `self.<index>.app_auth_id`: App authorization ID (`{:#x}`).
* `self.<index>.app_vendor_id`: App vendor ID (`{:#x}`).
* `self.<index>.app_self_type`: SELF application type (`u32`).
* `self.<index>.app_version`: Application version (`{:#x}`).
* `self.<index>.elf_class`: ELF class (`u8`, `2` for 64-bit).
* `self.<index>.elf_data`: ELF data encoding (`u8`, `2` for big-endian).
* `self.<index>.elf_os_abi`: OS/ABI (`u8`, `102` for CELL LV2).
* `self.<index>.elf_type`: ELF file type (`u16`).
* `self.<index>.elf_machine`: Machine architecture (`u16`, `21` for PowerPC64).
* `self.<index>.elf_entry`: Entry point address (`{:#x}`).
* `self.<index>.elf_phnum`: Program header count (`u16`).
* `self.<index>.elf_shnum`: Section header count (`u16`).
* `self.<index>.phdr.<j:02>.*`: Program headers: `align`, `filesz`, `flags`, `memsz`, `offset`, `paddr`, `type`, `vaddr`.

---

## 4. Snapshot Diffing (`PupDiff`)

The `PupSnapshot::diff` method computes structural differences between an older snapshot and a newer snapshot:
* **Firmware version**: Detects changes in `meta.version` (e.g. `4.81` → `4.82`).
* **Member tables**: Identifies added, removed, or size/digest-altered container members.
* **Package updates**: Identifies packages whose stored size, uncompressed size, or BCD build stamp changed.
* **Licence updates**: Detects added, removed, or text-updated language locales.

CLI tool `pupinfo` directly consumes `.pupsnap` files:
```bash
pupinfo --diff firmware_481.pupsnap firmware_482.pupsnap
```

---

## 5. Truncated Example

```text
pupsnap 1
header.data_length	206166540
header.file_count	9
header.header_digest	0000000000000000000000000000000000000001
header.header_length	656
header.image_version	0x105f2
header.package_version	1
meta.dots	...
meta.update_flags	0000
meta.version	4.82
entry.000.digest	0000000000000000000000000000000000000002
entry.000.id	0x100
entry.000.offset	656
entry.000.size	5
licence.000.bytes	13736
licence.000.date	December 10, 2009
licence.000.lang	en
tar_300.000.mtime	1503570642
tar_300.000.name	BDIT_FIRMWARE_PACKAGE.pkg
tar_300.000.size	1966992
pkg.000.member_size	1966992
pkg.000.mtime	1503570642
pkg.000.name	BDIT_FIRMWARE_PACKAGE.pkg
pkg.000.pkg_constant	3
pkg.000.pkg_flags	0x0
pkg.000.pkg_kind	7
pkg.000.pkg_seq	0x1
pkg.000.pkg_stamp	0x10060
pkg.000.pkg_stored	1966224
pkg.000.pkg_uncomp	1966224
pkg.000.sce_data_len	1966352
pkg.000.sce_hdr_len	640
pkg.000.sce_hdr_type	3
pkg.000.sce_key_rev	0
pkg.000.sce_meta_off	0
pkg.000.sce_ver	2
pkg.000.spkg_matched	true
pkg.000.stored_ok	true
pkg.000.uncomp_ok	true
self.00.app_auth_id	0x10700003fd000001
self.00.app_self_type	4
self.00.app_vendor_id	0x1000002
self.00.app_version	0x4008200000000
self.00.elf_class	2
self.00.elf_data	2
self.00.elf_entry	0x4e8800
self.00.elf_machine	21
self.00.elf_os_abi	102
self.00.elf_phnum	7
self.00.elf_shnum	32
self.00.elf_type	2
self.00.id	0x200
self.00.phdr.00.align	0x10000
self.00.phdr.00.filesz	5000000
self.00.phdr.00.flags	0x5
self.00.phdr.00.memsz	5000000
self.00.phdr.00.offset	0x10000
self.00.phdr.00.paddr	0x10000
self.00.phdr.00.type	0x1
self.00.phdr.00.vaddr	0x10000
```

''Source: [https://github.com/NamasteJasutin/PS2BootInspectionTool PS2BootInspectionTool], ps2kit/src/pup.rs and notes/research/ps3_pup.md.''
