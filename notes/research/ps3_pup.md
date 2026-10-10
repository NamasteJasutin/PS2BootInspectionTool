# PS3UPDAT.PUP: what a PS3 firmware update file shows without any key

Subject: the one PS3 file in the user's PCSX2 bios folder, `PS3UPDAT.PUP` (206 167 196 bytes,
file mtime 2018-08-17 — the download date, not the build date), surveyed 2026-10-10 to answer
"what could PS3 support honestly mean for the PSX Boot Inspection Tool?". The project stance
applies unchanged: **we ship no Sony code, data or keys, we never decrypt an SCE container, and
we never go looking for keys**. Everything below was read with python3/`tar`/`xxd`/`strings`
on the plaintext parts of the container; the extraction lived in the scratchpad and was deleted.

Conventions: **[V]** read in this file at the stated offset; **[I]** inferred or public knowledge
of the PUP/SCE formats (psdevwiki-level orientation; nothing was fetched or copied). Hex is
quoted only where the structure needs it; no digest, metadata or payload bytes beyond a prefix.

Summary first: a PUP is a **plaintext table of contents around 50 encrypted boxes**. The
container header, the firmware version, the licence text in 20 languages, two tiny flag files,
two tar directories (names, sizes, mtimes of every update package) and the first 0x300 bytes of
every package (SCE header + a package descriptor with kind, sequence number, timestamp and
compressed/uncompressed sizes) are readable. Not one byte of an emulator, a splash image or a
sound is. That is the whole honest scope.

---

## 1. Container (§Q1)

### 1.1 Header [V]

| offset | field | value | note |
|---|---|---|---|
| 0x00 | magic | `53 43 45 55 46 00 00 00` = `SCEUF\0\0\0` | [V] |
| 0x08 | package_version (u64 BE) | 1 | [V] |
| 0x10 | image_version (u64 BE) | 0x1_05F2 (67 058) | [V]; a monotonically increasing build counter, not the "4.82" string [I] |
| 0x18 | file_count (u64 BE) | 9 | [V] |
| 0x20 | header_length (u64 BE) | 0x290 (656) | [V] = 0x30 + 9×32 (entries) + 9×32 (digests) + 32 (header digest) |
| 0x28 | data_length (u64 BE) | 0xC49DA0C (206 166 540) | [V]; header + data = 206 167 196 = file size, exact |

All integers in the PUP header are big-endian 64-bit [V] (Cell/PPC convention [I]).

### 1.2 File-entry table (9 × 32 bytes at 0x30) [V]

| # | entry @ | id | public name [I] | data offset | size | end | contiguous |
|---|---|---|---|---|---|---|---|
| 0 | 0x030 | 0x100 | version.txt | 0x290 | 5 | 0x295 | yes |
| 1 | 0x050 | 0x101 | license.xml | 0x295 | 309 599 | 0x4BBF4 | yes |
| 2 | 0x070 | 0x103 | update_flags.txt | 0x4BBF4 | 5 | 0x4BBF9 | yes |
| 3 | 0x090 | 0x200 | ps3swu.self | 0x4BBF9 | 5 668 944 | 0x5B3C49 | yes |
| 4 | 0x0B0 | 0x201 | vsh.tar | 0x5B3C49 | 10 240 | 0x5B6449 | yes |
| 5 | 0x0D0 | 0x202 | dots.txt | 0x5B6449 | 3 | 0x5B644C | yes |
| 6 | 0x0F0 | 0x300 | update_files.tar | 0x5B644C | 194 426 880 | 0xBF21C4C | yes |
| 7 | 0x110 | 0x501 | spkg_hdr.tar | 0xBF21C4C | 81 920 | 0xBF35C4C | yes |
| 8 | 0x130 | 0x601 | ps3swu2.self | 0xBF35C4C | 5 668 944 | 0xC49DC9C | yes |

Each entry is `id u64, offset u64, size u64, pad u64 (0)` [V]. The members are packed
back-to-back with no alignment padding; the last one ends exactly at the file size [V]. Absent
from this retail PUP: 0x102 promo_flags.txt, 0x104 patch_build.txt, 0x203 patch_data.pkg [V]
(those appear in some other PUP kinds [I]). Member names are **not stored** in the file; the id →
name mapping is public convention [I].

### 1.3 Digest table (9 × 32 bytes at 0x150) and header digest (32 bytes at 0x270) [V]

Each digest entry is `index u64, 20 bytes, 4 bytes pad (0)` [V]; indices run 0..8 in order
[V]. The final 32 bytes at 0x270 hold 20 digest bytes + 12 zero bytes [V]. Public knowledge
says these are **HMAC-SHA1** digests over each member and over the header respectively, keyed
with a fixed PUP HMAC key held by Sony's updater [I]. Only the first eight bytes of each are
reproduced here, as identification, not as data: file digests begin `159449d0…`, `cc5b8370…`,
`fd7c8939…`, `bb3f170c…`, `d9b66e0d…`, `1aa4749d…`, `99ab472a…`, `79ae8323…`, `029df4c4…`;
header digest `34d5362d…` [V].

**Verification policy.** Checking these needs the PUP HMAC key, which we will not ship, embed,
hint at, or default-fetch. Two clean options:

- Show the stored digests as hex and stop. Zero controversy; still useful (two PUPs with the
  same version.txt but different digests are different builds).
- Offer `--pup-hmac-key <hex>` as a user-supplied 64-byte secret, kept only in memory, never
  logged, never written to config; when present the tool recomputes HMAC-SHA1 per member and
  prints match/mismatch. HMAC-SHA1 is a generic primitive, so this adds no Sony logic to the
  repo. It is **not decryption**, it is integrity checking the user chose to run with their own
  material. It is acceptable under the stance, but it is also optional: the recommendation is
  to ship the "show digests" mode first and add the keyed mode only if a real user asks.

What we can verify **without** any key, and should: header arithmetic (0x30+64n+32 =
header_length), header_length+data_length = file size, entry contiguity and bounds, digest
indices 0..n-1, tar well-formedness of 0x201/0x300/0x501, and the SCE-header arithmetic of
every package (§3). This PUP passes all of them [V].

---

## 2. Plaintext members (§Q2)

| member | bytes | content [V] |
|---|---|---|
| version.txt | 5 | `4.82\n` (`34 2e 38 32 0a`) |
| update_flags.txt | 5 | `0000\n` |
| dots.txt | 3 | `...` (three ASCII periods, no newline) |
| vsh.tar | 10 240 | a ustar archive holding **one empty regular file named `a`**, mode 0644, uid/gid 0 `root/root`, mtime 2006-12-18 05:09:46 UTC; 1 header block + 19 zero blocks |
| license.xml | 309 599 | the system-software licence agreement, see §2.1 |

[I] `4.82` is the retail firmware released 2017-08-29; the build timestamps inside (§3) are
2017-08-24. The 2006-12 mtime of the `vsh.tar` placeholder is the launch-era date every retail
PUP has carried since; the member is a vestigial slot [I]. `update_flags.txt = 0000` is the
retail "no special update flags" value [I]. `dots.txt` is a progress-indicator string [I].

### 2.1 license.xml [V]

Single file, UTF-8 bytes, CRLF line ends (42 lines, no lone LF), wrapped in `<xml version="1.0"
encoding="UTF-8">` and 20 `<locale lang="…" encoding="utf16">` blocks (the `encoding="utf16"`
attribute describes the target, the bytes are UTF-8). Every locale has exactly the same 39
string ids, `msg_update_eula_1` … `msg_update_eula_39`, each followed by a `<!--dtype="E"-->`
comment (780 in total). `msg_update_eula_1` is the title ("…System Software License Agreement
(Version 1.4)" in English), `_2` the date line, `_39` the copyright line "©2012 Sony Interactive
Entertainment Inc. All rights reserved" in every locale. Only a few words are quoted here.

| lang | UTF-8 bytes | date line (`msg_update_eula_2`) |
|---|---|---|
| zh-s | 11 703 | 2009年12月10日 |
| zh-t | 11 968 | 2009年12月10日 |
| da | 14 620 | 10. Dec, 2009 |
| nl | 15 993 | 10 Dec 2009 |
| en | 13 736 | December 10, 2009 |
| fi | 14 322 | 10. Joulu 2009 |
| fr | 17 050 | 10 Déc. 2009 |
| de | 15 911 | 10. Dez 2009 |
| it | 15 059 | 10 Dic 2009 |
| ja | 15 079 | 2012年10月24日 |
| ko | 14 183 | 2012년 10월 24일 |
| no | 14 457 | 10 Des, 2009 |
| pt | 15 577 | 10 de Dez de 2009 |
| ru | 27 381 | 10 Дек., 2009 |
| es | 14 832 | 10 de Dic de 2009 |
| sv | 15 283 | 10 Dec 2009 |
| pl | 16 738 | 10 gru 2009 |
| pt-br | 15 369 | 10 de dezembro de 2009 |
| en-gb | 13 735 | December 10, 2009 |
| tr | 15 627 | 24 Ekim 2012 |

Observations [V]: three locales (ja, ko, tr) carry a 2012-10-24 agreement date, the other 17 the
2009-12-10 one, while the copyright line is 2012 and the company name is already "Sony
Interactive Entertainment Inc." everywhere (the 2016 rename, so the text was re-issued after
2016 without re-dating the agreement). `en` and `en-gb` differ by a single byte. Russian is
the longest at 27 KB. The licence is the only human-readable prose in the whole file: a
`strings -n 8` sweep of the remaining 205.8 MB yields ~50 000 hits, all random 8-char runs at
the rate uniform data produces, plus the tar header fields of §3 [V].

---

## 3. update_files.tar and spkg_hdr.tar (§Q3)

### 3.1 Tar directory of update_files.tar (id 0x300) [V]

ustar, 48 regular members, all owner `pup_tool/psnes` (uid 1002, gid 700), mode 0644, 13
trailing zero blocks. Member mtimes are 2017-08-24 10:30:42–43 UTC; the timestamps baked into
the dev_flash names (`…_2017_08_24_192904`) are the same instant in UTC+9, i.e. the PUP was
assembled in Japan at 19:30 local time [V].

| member | size | SCE type | descriptor kind / seq (§3.3) | uncompressed | mtime UTC |
|---|---|---|---|---|---|
| BDIT_FIRMWARE_PACKAGE.pkg | 1 966 992 | 3 PKG | 7 / 1 | = (stored) | 2017-08-24 10:30:42 |
| BDPT_FIRMWARE_PACKAGE_301R.pkg | 951 040 | 3 PKG | 7 / 2 | = | " |
| BDPT_FIRMWARE_PACKAGE_302R.pkg | 951 040 | 3 PKG | 7 / 3 | = | " |
| BDPT_FIRMWARE_PACKAGE_303R.pkg | 951 040 | 3 PKG | 7 / 4 | = | " |
| BDPT_FIRMWARE_PACKAGE_304R.pkg | 1 639 296 | 3 PKG | 7 / 5 | = | " |
| BDPT_FIRMWARE_PACKAGE_306R.pkg | 787 200 | 3 PKG | 7 / 7 | = | " |
| BDPT_FIRMWARE_PACKAGE_308R.pkg | 1 639 296 | 3 PKG | 7 / 8 | = | " |
| BDPT_FIRMWARE_PACKAGE_310R.pkg | 787 200 | 3 PKG | 7 / 9 | = | " |
| BDPT_FIRMWARE_PACKAGE_312R.pkg | 1 639 296 | 3 PKG | 7 / 10 | = | " |
| BDPT_FIRMWARE_PACKAGE_314R.pkg | 787 200 | 3 PKG | 7 / 11 | = | " |
| BDPT_FIRMWARE_PACKAGE_316R.pkg | 1 639 296 | 3 PKG | 7 / 12 | = | " |
| BDPT_FIRMWARE_PACKAGE_318R.pkg | 1 402 704 | 3 PKG | 7 / 13 | = | " |
| BLUETOOTH_FIRMWARE.pkg | 644 547 | 3 PKG | 3 / 0x19 | 1 013 760 | " |
| CORE_OS_PACKAGE.pkg | 5 895 953 | 3 PKG | 1 / 0xFFF | 7 340 000 (0x6FFFE0) | 10:30:43 |
| MULTI_CARD_FIRMWARE.pkg | 28 636 | 3 PKG | 3 / 0x18 | 46 235 | " |
| SYS_CON_FIRMWARE_01000006.pkg | 4 864 | 3 PKG | 8 / 0xB8E | = | " |
| SYS_CON_FIRMWARE_01010303.pkg | 4 864 | 3 PKG | 8 / 0xC16 | = | " |
| SYS_CON_FIRMWARE_01020302.pkg | 4 864 | 3 PKG | 8 / 0xD52 | = | " |
| SYS_CON_FIRMWARE_01030302.pkg | 4 864 | 3 PKG | 8 / 0xDBF | = | " |
| SYS_CON_FIRMWARE_01040402.pkg | 4 864 | 3 PKG | 8 / 0xE69 | = | " |
| SYS_CON_FIRMWARE_01050002.pkg | 4 864 | 3 PKG | 8 / 0xF29 | = | " |
| SYS_CON_FIRMWARE_01050101.pkg | 4 864 | 3 PKG | 8 / 0xF38 | = | " |
| SYS_CON_FIRMWARE_S1_00010002083E0832.pkg | 4 864 | 3 PKG | 8 / 0x1_00000832 | = | " |
| UPL.xml.pkg | 1 310 | 3 PKG | 4 / 0x18 | 5 818 | " |
| RL_FOR_PACKAGE.img | 576 | 2 RVK | (rvk) 3 / 2 | 0x40 | 2013-06-19 16:26:55 |
| RL_FOR_PROGRAM.img | 736 | 2 RVK | (rvk) 4 / 1 | 0xE0 | 2017-08-24 05:51:13 |
| dev_flash3_023.tar.aa.2017_08_24_193000 | 1 786 | 3 PKG | 3 / 0x17 | 337 920 | 10:30:43 |
| dev_flash_000.tar.aa.2017_08_24_192904 | 2 077 | 3 PKG | 3 / 0 | 40 960 | " |
| dev_flash_001.tar.aa.2017_08_24_192904 | 4 972 203 | 3 PKG | 3 / 1 (flag FE) | 9 226 240 | " |
| dev_flash_002.tar.aa.2017_08_24_192904 | 5 380 360 | 3 PKG | 3 / 2 (flag FE) | 9 318 400 | " |
| dev_flash_003.tar.aa.2017_08_24_192904 | 4 080 676 | 3 PKG | 3 / 3 (flag FE) | 7 127 040 | " |
| dev_flash_004.tar.aa.2017_08_24_192904 | 30 557 | 3 PKG | 3 / 4 (flag FF) | 112 640 | " |
| dev_flash_005.tar.aa.2017_08_24_192904 | 8 013 289 | 3 PKG | 3 / 5 | 9 748 480 | " |
| dev_flash_006.tar.aa.2017_08_24_192904 | 9 801 456 | 3 PKG | 3 / 6 | 9 820 160 | " |
| dev_flash_007.tar.aa.2017_08_24_192904 | 10 005 663 | 3 PKG | 3 / 7 | 10 035 200 | " |
| dev_flash_008.tar.aa.2017_08_24_192904 | 10 283 648 | 3 PKG | 3 / 8 | 10 311 680 | " |
| dev_flash_009.tar.aa.2017_08_24_192904 | 10 277 876 | 3 PKG | 3 / 9 | 10 342 400 | " |
| dev_flash_010.tar.aa.2017_08_24_192904 | 9 870 166 | 3 PKG | 3 / 10 | 9 922 560 | " |
| dev_flash_011.tar.aa.2017_08_24_192904 | 9 166 087 | 3 PKG | 3 / 11 | 9 195 520 | " |
| dev_flash_012.tar.aa.2017_08_24_192904 | 10 109 513 | 3 PKG | 3 / 12 | 10 465 280 | " |
| dev_flash_013.tar.aa.2017_08_24_192904 | 8 526 486 | 3 PKG | 3 / 13 | 9 216 000 | " |
| dev_flash_014.tar.aa.2017_08_24_192904 | 8 066 522 | 3 PKG | 3 / 14 | 9 789 440 | " |
| dev_flash_015.tar.aa.2017_08_24_192904 | 8 420 199 | 3 PKG | 3 / 15 | 10 475 520 | " |
| dev_flash_016.tar.aa.2017_08_24_192904 | 7 971 259 | 3 PKG | 3 / 16 | 8 785 920 | " |
| dev_flash_017.tar.aa.2017_08_24_192904 | 8 516 785 | 3 PKG | 3 / 17 | 9 431 040 | " |
| dev_flash_018.tar.aa.2017_08_24_192904 | 5 325 099 | 3 PKG | 3 / 18 | 6 400 000 | " |
| dev_flash_019.tar.aa.2017_08_24_192904 | 9 163 307 | 3 PKG | 3 / 19 | 9 318 400 | " |
| dev_flash_020.tar.aa.2017_08_24_192904 | 8 787 558 | 3 PKG | 3 / 20 | 10 342 400 | " |
| dev_flash_021.tar.aa.2017_08_24_192904 | 10 276 702 | 3 PKG | 3 / 21 | 10 414 080 | " |
| dev_flash_022.tar.aa.2017_08_24_192904 | 5 580 955 | 3 PKG | 3 / 22 | 5 632 000 | " |

Totals [V]: the 23 `dev_flash_000…022` members plus `dev_flash3_023` are 172 630 229 bytes on
disk and 195 809 280 bytes (186.7 MiB) uncompressed; their compression ratio is 0.88 overall
and close to 1.00 for the big ones, which is what a tar of already-encrypted SELF/SPRX files
compresses to [I]. CoreOS is exactly 7 MiB minus 0x20 bytes uncompressed [V] — the fixed size
of the CoreOS flash region [I]. `dev_flash_000` (40 KiB) and `dev_flash_004` (110 KiB) are tiny
and carry high-byte flags FE/FF in their sequence word that the others lack [V]; the public
convention is that dev_flash numbering is just a 10 MiB-ish chunking of one big tar, so a
small chunk is one that ends a logical group [I].

Binwalk over a dev_flash member finds nothing [V]; Shannon entropy of every payload after
0x300 is 7.5–8.0 bits/byte [V].

### 3.2 SCE header of every member (plaintext, 0x20 bytes) [V]

All 50 SCE containers in the PUP (48 tar members + the two SELFs) read, big-endian:

| field | offset | value |
|---|---|---|
| magic | 0x00 | `SCE\0` |
| version | 0x04 | 2 |
| key_revision | 0x08 (u16) | 0x0000 for every PKG/RVK; **0x0001** for ps3swu.self, **0x000D** for ps3swu2.self |
| header_type | 0x0A (u16) | 1 = SELF (ps3swu, ps3swu2), 2 = RVK (the two `RL_FOR_*.img`), 3 = PKG (everything else) |
| metadata_offset | 0x0C (u32) | 0 for PKG/RVK; 0x3A0 for the SELFs |
| header_length | 0x10 (u64) | 0x280 for PKG, 0x200 for RVK, 0x880 for SELF |
| data_length | 0x18 (u64) | uncompressed payload length (PKG: descriptor 0x80 + content; SELF: the ELF's size 0xB82460) |

`header_length + data_length == member size` only for the BDPT/SYS_CON packages and the RVKs
[V]; everywhere else data_length exceeds the member size because the payload is compressed
(public: zlib under the encryption layer, which we cannot reach) [I]. The region 0x20–0x280 of
each PKG header has entropy 7.6–7.7 [V]: that is the encrypted metadata (keys and section
table) [I]. Nothing there is parsed by us.

### 3.3 The 0x80-byte package descriptor at header_length (plaintext) [V]

A pleasant surprise: every type-3 package has **0x80 bytes of structured plaintext** between
the SCE header and the ciphertext, and the RVKs have the same idea at 0x200. Layout as read,
with the interpretation marked [I] where it is ours:

```
+0x00  u32  3                 constant
+0x04  u32  kind              1 CoreOS · 3 flash tar (dev_flash*, BLUETOOTH, MULTI_CARD) · 4 UPL.xml · 7 BD drive fw · 8 syscon fw   [I]
+0x08  u64  sequence/id       dev_flash_NNN -> NNN (001–003 OR'd with FE<<56, 004 with FF<<56); dev_flash3 0x17; CoreOS 0xFFF; BD 1..13; syscon a build counter
+0x10  u64  version/stamp     CoreOS 0x0004_0082_0000_0000 (= 4.82); dev_flash 0x2017_0824_1929_04_00 (BCD date-time); BLUETOOTH/MULTI_CARD 0x2013_0620_0558_17_00; syscon 0x0001_0000_0000_0006 = "01000006" etc.; BD 0x0001_0060 / 0x0004_0060 / 0x0004_0070
+0x18  u64  uncompressed size (= SCE data_length - 0x80, every member)
+0x20  u64  stored size       (= member size - 0x300, every member)
+0x28  u64  flags             0x4000_0000 for CoreOS, BLUETOOTH, MULTI_CARD; 0 otherwise   [I: "compressed" is NOT what it means, dev_flash is compressed too]
+0x40  u64  3 · u64 0x40 · u64 0 · u64 uncompressed size · u64 1 · u64 1   (a one-entry sub-table, meaning unknown)   [V bytes, I label]
+0x80  ciphertext begins (entropy 8.0)
```

Example, `CORE_OS_PACKAGE.pkg` @0x280 [V]:
`00000003 00000001 00000000 00000fff | 00040082 00000000 00000000 006fffe0 | 00000000 0059f411 00000000 40000000`.

So **the firmware version and the build timestamp are recoverable from each package without
the PUP's own version.txt** [V], and the descriptor doubles as a consistency check (two sizes
that must match the SCE header and the tar member size; all 46 PKGs pass).

The two revocation lists are smaller and simpler [V]: `RL_FOR_PROGRAM.img` @0x200 reads
`00000004 00000001 00040082 00000000 | 00000006 …` — type 4 (program RVK) [I], version 4.82,
6 entries [I] — and `RL_FOR_PACKAGE.img` @0x200 reads `00000003 00000002 00010000 00000000 |
00000001 …` — package RVK, 1 entry [I]; both followed by ciphertext.

### 3.4 spkg_hdr.tar (id 0x501) [V]

ustar, 48 members named `<member>.spkg_hdr.1`, each **exactly 640 (0x280) bytes**, mode 0755,
same owner, mtimes 10:30:41–42 UTC (a second before the packages were tarred). Each one is a
full SCE header for the same package: its first 0x20 bytes are **byte-identical** to the
package's SCE header (so data_length matches, 48/48) and it diverges at 0x20 — the encrypted
metadata is different ciphertext [V]. Public meaning: from firmware 3.56 the updater verifies
packages through a second header encrypted under a newer key set ("spkg"), shipped alongside
so that older and newer loaders can both check the same payload [I]. For us it is a second,
independent copy of the plaintext header fields, useful only as a cross-check.

### 3.5 The two updater SELFs (0x200, 0x601) [V]

`ps3swu.self` and `ps3swu2.self` are the same program under two key revisions. Their plaintext
SELF headers are identical except the key_revision byte (0x01 vs 0x0D); every byte from the
metadata area onward differs. Readable fields:

| field | value [V] |
|---|---|
| SELF ext header @0x20 | header_type 3, appinfo @0x70, ELF header @0x90, phdrs @0xD0, shdrs @0x567850, section-info @0x260, sce_version @0x340, control-info @0x350 (size 0x70) |
| app info | auth_id 0x1070_0003_FD00_0001, vendor 0x0100_0002, self_type 4 (application [I]), version 0x0004_0082_0000_0000 (= 4.82) |
| ELF header (plaintext) | ELF64 big-endian, OSABI 102 (CELL LV2 [I]), e_machine 21 (PowerPC64), e_type EXEC, entry 0x4E8800, 7 phdrs, 32 shdrs |
| phdrs | PT_LOAD RX 0x10000..+0x4C42A0; PT_LOAD RW 0x4E0000..+0x301D4; PT_LOAD 0x10000000..+0x514880 (flags 0x400004, SPU image [I]); PT_LOAD 0x10520000 (+0x146450 file / 0x172AC8 mem); an empty PT_LOAD; PT_TLS; PT_PROC_PARAM (0x60000001) |
| section info | four loadable sections marked compressed=2 encrypted=1 at file offsets 0x880, 0x208AB0, 0x22DA50, 0x4A1F00 |
| control info | type 1 (flags, first byte 0x40) size 0x30; type 2 (digest) size 0x40 |

So without any key one can say "this is the 4.82 updater, a PPU executable with an embedded
SPU program, four encrypted compressed segments, entry 0x4E8800". Its code is unreachable.

---

## 4. Where the PS1/PS2/PSP emulators live (§Q4)

Public dev_flash layout [I], none of it visible in this file:

| path | role |
|---|---|
| `dev_flash/ps1emu/ps1_emu.self` | PS1 emulator for CECHA/B (hardware EE+GS boards: PS1 runs on the PS2 chips) |
| `dev_flash/ps1emu/ps1_netemu.self` | PS1 software emulator (all later models; also used for PS1 Classics) |
| `dev_flash/ps1emu/ps1_newemu.self` | newer PS1 software emulator |
| `dev_flash/ps1emu/ps1_rom.bin` | a **PS1 BIOS image** (512 KiB) used by the software emulators |
| `dev_flash/ps2emu/ps2_emu.self` | PS2 "emulator" for CECHA/B (real EE+GS; mostly a loader) |
| `dev_flash/ps2emu/ps2_gxemu.self` | CECHC/E: real GS, software EE |
| `dev_flash/ps2emu/ps2_netemu.self` | full software PS2 emulator (PS2 Classics) |
| `dev_flash/ps2emu/ps2_softemu.self` | full software PS2 emulator, disc-less models |
| `dev_flash/pspemu/psp_emulator.self`, `release/`, `psp_translator.self` | PSP Minis/Remasters emulator |
| `dev_flash/vsh/module/vsh.self`, `…/resource/*.rco`, `…/resource/coldboot*.raf`/`.ac3` | XMB shell and boot resources (§5) |
| `dev_flash/sys/internal/`, `dev_flash/sys/external/*.sprx` | lv2 system modules |

**Reachable from this PUP without keys: none of the above contents.** What *is* observable
[V]: that 4.82's dev_flash is 24 chunks totalling 186.7 MiB uncompressed, each chunk's exact
uncompressed and stored size, the 2017-08-24 19:29:04 JST build stamp, the 7 MiB CoreOS size,
and the 4.82 version word in CoreOS, the RVK and both SELFs. We cannot even see the tar member
names inside a dev_flash chunk (the tar is inside the ciphertext), so we cannot say *which*
chunk holds `ps1emu/` or `ps2emu/` — a tool that claimed to would be guessing from public
decrypted-firmware listings, and we should not present that as read from the user's file. The
PS2 emulators embed their own copies of PS2 ROM content (the EE/IOP boot ROM the emulated
console runs) [I]; the same wall applies.

---

## 5. PS3 boot itself (§Q5) [I]

What the console shows: a black screen → the "Sony Computer Entertainment" wordmark with the
orchestral swell ("coldboot"), then the XMB with the slowly moving wave background and the
ambient hum. These are dev_flash resources, not CoreOS: `dev_flash/vsh/resource/coldboot.raf`
(a packed scene/animation container) with `coldboot_stereo.ac3` / `coldboot_multi.ac3` for the
chime, and the wave shader/texture resources plus `vsh.self`'s own code for the XMB background;
the system-font and RCO (`*.rco`) files drive the text. Earlier firmwares carried the splash
as a separate image set; by 4.xx it is the `.raf` container [I].

In this PUP: no byte of any of it is plaintext [V] — they would sit inside one of the
dev_flash chunks, which are compressed then encrypted. There is no analogue of the PS2 ROM's
plain `ROMDIR` with `OSDSYS`/`IOPBTCONF` lying open. The PS3 kept its splash and shell under
the same lock as its kernel.

Contrast with what the project already does for PS2 and PS1: those ROMs are plaintext by
design (there is no key), which is exactly why "bring your own BIOS" works for them and why the
PS3 cannot be done the same way from a PUP.

---

## 6. Honest options for the tool (§Q6), ranked

**(a) PUP identity card — recommended, zero keys, ~1 day of work.**
Parse and display: SCEUF header fields with the three arithmetic checks; the entry table with
public names; the stored digests as hex (optionally recomputed with a user-supplied key, §1.3);
version.txt; update_flags/dots; licence locales with byte lengths and the date/copyright lines
(the user can open any locale's full text — it is their file); the tar directories of 0x300 and
0x501 with mtimes and the JST-vs-UTC note; for each package the SCE header, the 0x80 descriptor
(kind, sequence, stamp, sizes) and the spkg_hdr cross-check; for the SELFs the app-info,
ELF header and program headers. All of this is container structure, none of it is Sony's
creative content, and no decryption is performed or implied. It is strictly weaker than what
the tool offers for PS1/PS2 — a "what is this file" card, not a boot inspection — and the UI
should say so plainly rather than pad it.

**(b) "Bring your own decrypted dev_flash" — feasible, keep it narrow, let the user carry the
burden.** If a user already has plain files from their own console or their own tools (we
neither link to nor name those tools), we can accept a directory and inspect only files whose
formats we already understand without Sony-specific logic:
- `ps1emu/ps1_rom.bin` is a PS1 BIOS image; `ps2kit::ps1` can identify it the way it does any
  PS1 ROM (date/version string, region, licence-screen resources) and the PS1 licence scene can
  render from it exactly as from a SCPH-xxxx dump. This respects the stance completely: the
  file is the user's, it is plaintext when it reaches us, we add nothing.
- `ps2emu/ps2_*emu.self`: decrypted SELFs are still SELF containers wrapping an ELF; we can
  parse the plaintext headers (§3.5 shows we already can). Finding an embedded PS2 ROM inside
  means looking for a `ROMDIR`/`RESET` directory in the ELF's data segments and handing it to
  `ps2kit::rom` — again generic, no keys. Whether the emulators embed a full rom0 or only
  pieces is unknown to us; the mode should report "found / not found", not promise.
- `vsh/resource/coldboot*.raf`: proprietary format; we would need public documentation and a
  clean reimplementation to render it. Out of scope unless someone wants a PS3-splash project;
  not a PS1/PS2 boot concern.
Rules for (b): accept a directory, never a PUP; never offer to "unpack" or "decrypt"; never
embed hashes of known decrypted files that would amount to shipping fingerprints of Sony
binaries beyond what identification needs; print a one-line reminder that the user is
responsible for how they obtained the files.

**(c) Other sensible things.**
- Use the PUP card as a *provenance* record for the user's PS1/PS2 ROMs: if they later supply
  a `ps1_rom.bin`, show which firmware's dev_flash it is claimed to come from (user-entered
  tag, not detected).
- A PUP diff: two PUPs → which members changed (digests, sizes, descriptor stamps). Zero keys,
  genuinely useful to firmware archivists.
- Nothing emulator-like. There is no "render the PS3 boot" without the resources, and the
  resources are behind the lock. Do not implement, approximate, or bundle a look-alike; a
  reconstructed SCE splash from memory would be our Sony data, not the user's.

**Legal/ethical note.** Parsing a container's plaintext table of contents, tar headers and
unencrypted descriptors is ordinary file-format work (the same category as reading a ZIP's
central directory) and the fields we show are facts about the user's file. Implementing SCE
decryption, shipping or fetching any key, or embedding decrypted firmware content would cross
into circumvention and redistribution; the tool does none of it, and the code should contain
no hook where a key *could* be dropped in except the optional HMAC integrity check, which is a
generic primitive over the user's own secret and performs no decryption. We also do not point
users at tools or sites for obtaining decrypted dev_flash.

---

## 7. Method and cleanup

Scripts (scratchpad, deleted): `pup.py` (header/entry/digest parse + member extraction),
`tars.py` (licence locales, tar directories), `sce.py` (SCE headers, spkg_hdr vs package),
`self.py` (SELF ext header/app-info/ELF/phdrs, ps3swu vs ps3swu2 diff). Extractions
(`ext/` 197 MB, `uf/` 186 MB) removed after the note. The PUP itself untouched.

Open questions worth a later look, all answerable without keys: the meaning of descriptor
`+0x28 = 0x40000000` (CoreOS/Bluetooth/multi-card only); why dev_flash 001–003 carry FE and
004 FF in the sequence word; whether image_version 0x105F2 maps to a public build list. None
of them changes the conclusion of §6.
