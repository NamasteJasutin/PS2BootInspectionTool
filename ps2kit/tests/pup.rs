//! Tests for the PS3 PUP container parser (`ps2kit::pup`).
//!
//! Includes synthetic container tests (covering parsing, arithmetic checks,
//! and error handling on malformed input) and oracle tests against the user's
//! real `bios/PS3UPDAT.PUP` file (skipped silently when absent).

use ps2kit::pup::{Pup, SCE_MAGIC};
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn create_tar_block(name: &str, size: u64, mtime: u64) -> [u8; 512] {
    let mut block = [0u8; 512];
    let name_bytes = name.as_bytes();
    block[..name_bytes.len().min(100)].copy_from_slice(&name_bytes[..name_bytes.len().min(100)]);

    // mode 0644
    let mode = format!("{:07o}\0", 0o644);
    block[100..108].copy_from_slice(mode.as_bytes());

    // size in octal
    let size_oct = format!("{:011o}\0", size);
    block[124..136].copy_from_slice(size_oct.as_bytes());

    // mtime in octal
    let mtime_oct = format!("{:011o}\0", mtime);
    block[136..148].copy_from_slice(mtime_oct.as_bytes());

    // typeflag '0'
    block[156] = b'0';

    // magic "ustar  \0"
    block[257..265].copy_from_slice(b"ustar  \0");

    block
}

fn build_synthetic_pup() -> Vec<u8> {
    // Member 0: version.txt (5 bytes)
    let m0_data = b"1.00\n";
    // Member 1: dots.txt (3 bytes)
    let m1_data = b"...";

    // Member 2: update_files.tar
    // 1 file: TEST_PACKAGE.pkg (size 896 = 0x380 bytes)
    // - SCE header (0x20 bytes)
    // - SCE metadata padding up to 0x280 (608 bytes)
    // - PKG descriptor (0x80 bytes = 128 bytes)
    // - Ciphertext / stored payload (0x80 bytes = 128 bytes)
    // Total package size = 0x20 + 0x260 + 0x80 + 0x80 = 0x380 (896 bytes)
    let mut pkg_data = vec![0u8; 896];
    // SCE header
    pkg_data[..4].copy_from_slice(SCE_MAGIC);
    pkg_data[4..8].copy_from_slice(&2u32.to_be_bytes()); // version 2
    pkg_data[8..10].copy_from_slice(&0u16.to_be_bytes()); // key_revision 0
    pkg_data[10..12].copy_from_slice(&3u16.to_be_bytes()); // header_type 3 (PKG)
    pkg_data[12..16].copy_from_slice(&0u32.to_be_bytes()); // metadata_offset 0
    pkg_data[16..24].copy_from_slice(&0x280u64.to_be_bytes()); // header_length 0x280
    pkg_data[24..32].copy_from_slice(&0x100u64.to_be_bytes()); // data_length 0x100

    // PKG descriptor at 0x280
    pkg_data[0x280..0x284].copy_from_slice(&3u32.to_be_bytes()); // constant 3
    pkg_data[0x284..0x288].copy_from_slice(&1u32.to_be_bytes()); // kind 1 (CoreOS)
    pkg_data[0x288..0x290].copy_from_slice(&1u64.to_be_bytes()); // sequence 1
    pkg_data[0x290..0x298].copy_from_slice(&0x0001_0000_0000_0000u64.to_be_bytes()); // version 1.00
    pkg_data[0x298..0x2A0].copy_from_slice(&0x80u64.to_be_bytes()); // uncompressed = 0x100 - 0x80 = 0x80
    pkg_data[0x2A0..0x2A8].copy_from_slice(&0x80u64.to_be_bytes()); // stored = 896 - 0x300 = 0x80
    pkg_data[0x2A8..0x2B0].copy_from_slice(&0x4000_0000u64.to_be_bytes()); // flags

    let mut tar_data = Vec::new();
    let header_block = create_tar_block("TEST_PACKAGE.pkg", pkg_data.len() as u64, 1500000000);
    tar_data.extend_from_slice(&header_block);
    tar_data.extend_from_slice(&pkg_data);
    // Pad package to 512-byte boundary (896 + 128 = 1024)
    tar_data.extend_from_slice(&[0u8; 128]);
    // 512-byte zero block to terminate tar
    tar_data.extend_from_slice(&[0u8; 512]);

    let file_count = 3u64;
    let header_len = 0x30 + file_count * 32 + file_count * 32 + 32; // 272 = 0x110
    let m0_offset = header_len;
    let m1_offset = m0_offset + m0_data.len() as u64;
    let m2_offset = m1_offset + m1_data.len() as u64;
    let data_len = m0_data.len() as u64 + m1_data.len() as u64 + tar_data.len() as u64;

    let mut pup = Vec::new();
    // PUP Header (0x30 bytes)
    pup.extend_from_slice(b"SCEUF\0\0\0");
    pup.extend_from_slice(&1u64.to_be_bytes()); // package_version
    pup.extend_from_slice(&0x100u64.to_be_bytes()); // image_version
    pup.extend_from_slice(&file_count.to_be_bytes()); // file_count
    pup.extend_from_slice(&header_len.to_be_bytes()); // header_length
    pup.extend_from_slice(&data_len.to_be_bytes()); // data_length

    // Entries (3 * 32 bytes)
    // 0: version.txt
    pup.extend_from_slice(&0x100u64.to_be_bytes());
    pup.extend_from_slice(&m0_offset.to_be_bytes());
    pup.extend_from_slice(&(m0_data.len() as u64).to_be_bytes());
    pup.extend_from_slice(&[0u8; 8]);

    // 1: dots.txt
    pup.extend_from_slice(&0x202u64.to_be_bytes());
    pup.extend_from_slice(&m1_offset.to_be_bytes());
    pup.extend_from_slice(&(m1_data.len() as u64).to_be_bytes());
    pup.extend_from_slice(&[0u8; 8]);

    // 2: update_files.tar
    pup.extend_from_slice(&0x300u64.to_be_bytes());
    pup.extend_from_slice(&m2_offset.to_be_bytes());
    pup.extend_from_slice(&(tar_data.len() as u64).to_be_bytes());
    pup.extend_from_slice(&[0u8; 8]);

    // Digests (3 * 32 bytes)
    for i in 0..3 {
        pup.extend_from_slice(&(i as u64).to_be_bytes());
        pup.extend_from_slice(&[0xAA; 20]);
        pup.extend_from_slice(&[0u8; 4]);
    }

    // Header digest (32 bytes)
    pup.extend_from_slice(&[0xBB; 20]);
    pup.extend_from_slice(&[0u8; 12]);

    assert_eq!(pup.len(), header_len as usize);

    // Append member data
    pup.extend_from_slice(m0_data);
    pup.extend_from_slice(m1_data);
    pup.extend_from_slice(&tar_data);

    pup
}

#[test]
fn synthetic_pup_parsing_and_checks() {
    let bytes = build_synthetic_pup();
    let pup = Pup::parse(&bytes).expect("synthetic PUP must parse");

    assert_eq!(pup.header.package_version, 1);
    assert_eq!(pup.header.image_version, 0x100);
    assert_eq!(pup.header.file_count, 3);
    assert_eq!(pup.entries.len(), 3);
    assert_eq!(pup.digests.len(), 3);

    assert_eq!(pup.version().unwrap(), "1.00");
    assert_eq!(pup.dots().unwrap(), "...");

    let tar_entries = pup.tar_entries(0x300).expect("tar entries must parse");
    assert_eq!(tar_entries.len(), 1);
    assert_eq!(tar_entries[0].name, "TEST_PACKAGE.pkg");
    assert_eq!(tar_entries[0].size, 896);

    let packages = pup.packages().expect("packages must parse");
    assert_eq!(packages.len(), 1);
    assert_eq!(packages[0].name, "TEST_PACKAGE.pkg");
    assert!(packages[0].uncompressed_reconciled);
    assert!(packages[0].stored_reconciled);
    let desc = packages[0].pkg_descriptor.as_ref().unwrap();
    assert_eq!(desc.kind_name(), "CoreOS");
}

#[test]
fn synthetic_pup_malformed_inputs_never_panic() {
    let valid = build_synthetic_pup();

    // Truncated header
    assert!(Pup::parse(&valid[..20]).is_err());
    assert!(Pup::parse(&valid[..0x100]).is_err());

    // Bad magic
    let mut bad_magic = valid.clone();
    bad_magic[0..5].copy_from_slice(b"BADMG");
    assert!(Pup::parse(&bad_magic).is_err());

    // Header + data != total length
    let mut bad_size = valid.clone();
    bad_size[40..48].copy_from_slice(&999999u64.to_be_bytes());
    assert!(Pup::parse(&bad_size).is_err());

    // Overlapping entries (entry 1 offset moved backward)
    let mut overlapping = valid.clone();
    let m0_offset = u64::from_be_bytes(overlapping[0x38..0x40].try_into().unwrap());
    overlapping[0x58..0x60].copy_from_slice(&m0_offset.to_be_bytes());
    assert!(Pup::parse(&overlapping).is_err());

    // Gap between entries (entry 1 offset moved forward)
    let mut gap = valid.clone();
    let m1_offset = u64::from_be_bytes(gap[0x58..0x60].try_into().unwrap());
    gap[0x58..0x60].copy_from_slice(&(m1_offset + 10).to_be_bytes());
    assert!(Pup::parse(&gap).is_err());

    // Out of order digest index
    let mut bad_digest = valid.clone();
    // Digest 0 index at 0x90
    bad_digest[0x90..0x98].copy_from_slice(&5u64.to_be_bytes());
    assert!(Pup::parse(&bad_digest).is_err());

    // Truncated trailing data
    let mut truncated_data = valid.clone();
    truncated_data.pop();
    assert!(Pup::parse(&truncated_data).is_err());
}

#[test]
fn real_ps3_pup_oracle() {
    let path = root().join("bios").join("PS3UPDAT.PUP");
    let Ok(pup) = Pup::open(&path) else {
        // Silently skip on CI where dump is not present
        return;
    };

    // ps3_pup.md §1.1: image_version 0x1_05F2, file_count 9, header_len 656, data_len 206166540
    assert_eq!(pup.header.package_version, 1);
    assert_eq!(pup.header.image_version, 0x1_05F2);
    assert_eq!(pup.header.file_count, 9);
    assert_eq!(pup.header.header_length, 0x290);
    assert_eq!(pup.header.data_length, 206_166_540);
    assert_eq!(pup.entries.len(), 9);
    assert_eq!(pup.digests.len(), 9);

    // ps3_pup.md §1.3: header digest begins 34d5362d
    let hdr_digest_hex = ps2kit::pup::hex_digest(&pup.header_digest);
    assert!(hdr_digest_hex.starts_with("34d5362d"));

    // ps3_pup.md §2: version.txt = "4.82", update_flags = "0000", dots = "..."
    assert_eq!(pup.version().unwrap(), "4.82");
    assert_eq!(pup.update_flags().unwrap(), "0000");
    assert_eq!(pup.dots().unwrap(), "...");

    // ps3_pup.md §2.1: 20 locales in license.xml
    let locales = pup.license_locales().unwrap();
    assert_eq!(locales.len(), 20);
    let en_locale = locales.iter().find(|l| l.lang == "en").expect("en locale must be present");
    assert_eq!(en_locale.date, "December 10, 2009");
    assert!(en_locale.copyright.contains("Sony Interactive Entertainment"));

    // ps3_pup.md §3.1 & §3.4: 50 members in update_files.tar (48 packages + 2 RVKs), 48 in spkg_hdr.tar
    let uf_tar = pup.tar_entries(0x300).unwrap();
    assert_eq!(uf_tar.len(), 50);

    let spkg_tar = pup.tar_entries(0x501).unwrap();
    assert_eq!(spkg_tar.len(), 48);

    // Packages and reconciliation: all 48 packages must reconcile with descriptor and spkg_hdr
    let packages = pup.packages().unwrap();
    assert_eq!(packages.len(), 50);

    let mut pkg_count = 0;
    let mut rvk_count = 0;
    for p in &packages {
        assert!(p.uncompressed_reconciled, "{} uncompressed size must reconcile", p.name);
        assert!(p.stored_reconciled, "{} stored size must reconcile", p.name);
        if p.sce_header.header_type == 3 {
            pkg_count += 1;
            assert_eq!(p.spkg_hdr_matched, Some(true), "{} must match spkg_hdr", p.name);
        } else if p.sce_header.header_type == 2 {
            rvk_count += 1;
        }
    }
    assert_eq!(pkg_count, 48);
    assert_eq!(rvk_count, 2);

    // ps3_pup.md §3.5: ps3swu.self (key_rev 1) and ps3swu2.self (key_rev 13)
    let swu1 = pup.self_info(0x200).unwrap();
    assert_eq!(swu1.sce_header.key_revision, 1);
    assert_eq!(swu1.app_info.version, 0x0004_0082_0000_0000);
    assert_eq!(swu1.elf_header.e_entry, 0x4E8800);
    assert_eq!(swu1.program_headers.len(), 7);

    let swu2 = pup.self_info(0x601).unwrap();
    assert_eq!(swu2.sce_header.key_revision, 13);
    assert_eq!(swu2.app_info.version, 0x0004_0082_0000_0000);
    assert_eq!(swu2.elf_header.e_entry, 0x4E8800);
    assert_eq!(swu2.program_headers.len(), 7);
}
