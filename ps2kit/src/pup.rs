//! PS3 System Update (PUP) container parser (plaintext structure only).
//!
//! # Legal and Ethical Stance
//!
//! As specified in `notes/research/ps3_pup.md` §6:
//! - This module parses **only** the plaintext container header, file-entry table,
//!   embedded TAR listings, unencrypted SCE/SELF headers, and unencrypted package descriptors.
//! - **No SCE decryption** is performed, linked, or facilitated.
//! - **No encryption keys** are bundled, accepted, fetched, or embedded.
//! - Firmware contents remain encrypted and are not decrypted or executed.

use crate::{Error, Format, Result};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// Magic bytes at the start of a PS3 PUP container (`SCEUF\0\0\0`).
///
/// See `notes/research/ps3_pup.md` §1.1.
pub const PUP_MAGIC: &[u8; 8] = b"SCEUF\0\0\0";

/// Magic bytes at the start of an SCE container (`SCE\0`).
///
/// See `notes/research/ps3_pup.md` §3.2.
pub const SCE_MAGIC: &[u8; 4] = b"SCE\0";

/// Known public names for standard PS3 PUP member IDs.
///
/// Note that member names are not stored in the PUP container file itself;
/// this mapping reflects public convention.
///
/// See `notes/research/ps3_pup.md` §1.2.
#[must_use]
pub fn pup_member_name(id: u64) -> &'static str {
    match id {
        0x100 => "version.txt",
        0x101 => "license.xml",
        0x102 => "promo_flags.txt",
        0x103 => "update_flags.txt",
        0x104 => "patch_build.txt",
        0x200 => "ps3swu.self",
        0x201 => "vsh.tar",
        0x202 => "dots.txt",
        0x203 => "patch_data.pkg",
        0x300 => "update_files.tar",
        0x501 => "spkg_hdr.tar",
        0x601 => "ps3swu2.self",
        _ => "unknown",
    }
}

/// Formats a byte slice as a lower-case hexadecimal string.
#[must_use]
pub fn hex_digest(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use std::fmt::Write;
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// The parsed header of a PS3 PUP container.
///
/// See `notes/research/ps3_pup.md` §1.1.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct PupHeader {
    /// Package version (typically 1).
    pub package_version: u64,
    /// Image/build version counter (e.g. `0x1_05F2` = 67,058 for 4.82).
    pub image_version: u64,
    /// Number of file entries in the table.
    pub file_count: u64,
    /// Total length of the header in bytes.
    pub header_length: u64,
    /// Total length of member data in bytes.
    pub data_length: u64,
}

/// An entry in the PUP file-entry table.
///
/// See `notes/research/ps3_pup.md` §1.2.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct PupEntry {
    /// Numeric member identifier (e.g. `0x100` for `version.txt`).
    pub id: u64,
    /// Offset of the member in the PUP file in bytes.
    pub offset: u64,
    /// Size of the member in bytes.
    pub size: u64,
}

impl PupEntry {
    /// The public name of this entry if known.
    #[must_use]
    pub fn name(&self) -> &'static str {
        pup_member_name(self.id)
    }
}

/// A stored 20-byte digest entry for a PUP member.
///
/// See `notes/research/ps3_pup.md` §1.3.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct PupDigestEntry {
    /// Index corresponding to entry table index.
    pub index: u64,
    /// Stored 20-byte HMAC-SHA1 digest.
    pub digest: [u8; 20],
}

impl PupDigestEntry {
    /// Formats the digest as a 40-character hex string.
    #[must_use]
    pub fn hex(&self) -> String {
        hex_digest(&self.digest)
    }
}

/// Source backing a [`Pup`] instance.
#[derive(Debug)]
enum PupSource {
    Memory(Vec<u8>),
    File(PathBuf),
}

/// A PS3 PUP update container.
///
/// Provides access to the plaintext container structure, metadata,
/// embedded TAR archives, and unencrypted SCE/SELF headers.
#[derive(Debug)]
pub struct Pup {
    /// Header information.
    pub header: PupHeader,
    /// Member entries in file order.
    pub entries: Vec<PupEntry>,
    /// Stored digests per entry.
    pub digests: Vec<PupDigestEntry>,
    /// Stored 20-byte header digest.
    pub header_digest: [u8; 20],
    source: PupSource,
}

impl Pup {
    /// Opens a PUP file on disk, reading only the header and tables.
    ///
    /// Member contents are read on-demand without loading the entire container into memory.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let mut file = File::open(&path)?;
        let file_size = file.metadata()?.len();

        if file_size < 0x30 {
            return Err(Error::Corrupt(Format::Bios, "PUP file too small for header".into()));
        }

        let mut header_buf = [0u8; 0x30];
        file.read_exact(&mut header_buf)?;

        let (header, min_header_len) = parse_header_prefix(&header_buf, file_size)?;

        let mut full_header = vec![0u8; header.header_length as usize];
        file.seek(SeekFrom::Start(0))?;
        file.read_exact(&mut full_header)?;

        let (entries, digests, header_digest) = parse_tables(&full_header, &header, min_header_len)?;

        Ok(Self {
            header,
            entries,
            digests,
            header_digest,
            source: PupSource::File(path),
        })
    }

    /// Parses a PUP container from an in-memory byte slice.
    pub fn parse(data: &[u8]) -> Result<Self> {
        let total_size = data.len() as u64;
        if total_size < 0x30 {
            return Err(Error::Corrupt(Format::Bios, "PUP data too small for header".into()));
        }

        let (header, min_header_len) = parse_header_prefix(&data[..0x30], total_size)?;

        if total_size < header.header_length {
            return Err(Error::Corrupt(
                Format::Bios,
                format!("PUP data ({} bytes) smaller than header_length ({})", total_size, header.header_length),
            ));
        }

        let (entries, digests, header_digest) = parse_tables(&data[..header.header_length as usize], &header, min_header_len)?;

        Ok(Self {
            header,
            entries,
            digests,
            header_digest,
            source: PupSource::Memory(data.to_vec()),
        })
    }

    /// Finds a member entry by its numeric ID.
    #[must_use]
    pub fn entry(&self, id: u64) -> Option<&PupEntry> {
        self.entries.iter().find(|e| e.id == id)
    }

    /// Reads a range of bytes from the PUP container.
    pub fn read_range(&self, offset: u64, size: u64) -> Result<Vec<u8>> {
        let total_len = self.header.header_length.saturating_add(self.header.data_length);
        let end = offset.checked_add(size).ok_or_else(|| {
            Error::Corrupt(Format::Bios, "read range overflow".into())
        })?;
        if end > total_len {
            return Err(Error::Corrupt(
                Format::Bios,
                format!("read range {offset}..{end} exceeds PUP container length {total_len}"),
            ));
        }

        match &self.source {
            PupSource::Memory(data) => {
                let start_idx = offset as usize;
                let end_idx = end as usize;
                if end_idx > data.len() {
                    return Err(Error::Corrupt(Format::Bios, "range outside memory buffer".into()));
                }
                Ok(data[start_idx..end_idx].to_vec())
            }
            PupSource::File(path) => {
                let mut file = File::open(path)?;
                file.seek(SeekFrom::Start(offset))?;
                let mut buf = vec![0u8; size as usize];
                file.read_exact(&mut buf)?;
                Ok(buf)
            }
        }
    }

    /// Reads the entire contents of a member by its ID.
    pub fn read_member(&self, id: u64) -> Result<Vec<u8>> {
        let entry = self.entry(id).ok_or_else(|| {
            Error::NotFound(Format::Bios, format!("member {id:#x} ({}) not found in PUP", pup_member_name(id)))
        })?;
        self.read_range(entry.offset, entry.size)
    }

    /// Reads a sub-slice of a member by its ID.
    pub fn read_member_range(&self, id: u64, offset: u64, size: u64) -> Result<Vec<u8>> {
        let entry = self.entry(id).ok_or_else(|| {
            Error::NotFound(Format::Bios, format!("member {id:#x} ({}) not found in PUP", pup_member_name(id)))
        })?;
        if offset.saturating_add(size) > entry.size {
            return Err(Error::Corrupt(
                Format::Bios,
                format!("range {offset}..+{} exceeds member size {}", size, entry.size),
            ));
        }
        self.read_range(entry.offset.saturating_add(offset), size)
    }

    /// Reads the firmware version string from `version.txt` (id `0x100`).
    ///
    /// See `notes/research/ps3_pup.md` §2.
    pub fn version(&self) -> Result<String> {
        let bytes = self.read_member(0x100)?;
        let text = std::str::from_utf8(&bytes).map_err(|e| {
            Error::Corrupt(Format::Bios, format!("version.txt is not valid UTF-8: {e}"))
        })?;
        Ok(text.trim().to_string())
    }

    /// Reads the update flags string from `update_flags.txt` (id `0x103`).
    ///
    /// See `notes/research/ps3_pup.md` §2.
    pub fn update_flags(&self) -> Result<String> {
        let bytes = self.read_member(0x103)?;
        let text = std::str::from_utf8(&bytes).map_err(|e| {
            Error::Corrupt(Format::Bios, format!("update_flags.txt is not valid UTF-8: {e}"))
        })?;
        Ok(text.trim().to_string())
    }

    /// Reads the progress dots string from `dots.txt` (id `0x202`).
    ///
    /// See `notes/research/ps3_pup.md` §2.
    pub fn dots(&self) -> Result<String> {
        let bytes = self.read_member(0x202)?;
        let text = std::str::from_utf8(&bytes).map_err(|e| {
            Error::Corrupt(Format::Bios, format!("dots.txt is not valid UTF-8: {e}"))
        })?;
        Ok(text.trim().to_string())
    }

    /// Parses the licence agreement metadata for each locale from `license.xml` (id `0x101`).
    ///
    /// Does not extract or bundle licence prose.
    ///
    /// See `notes/research/ps3_pup.md` §2.1.
    pub fn license_locales(&self) -> Result<Vec<LicenseLocale>> {
        let bytes = self.read_member(0x101)?;
        parse_license_locales(&bytes)
    }

    /// Parses the directory listing of an embedded ustar archive (e.g. `update_files.tar` `0x300`).
    ///
    /// See `notes/research/ps3_pup.md` §3.1 and §3.4.
    pub fn tar_entries(&self, id: u64) -> Result<Vec<TarEntry>> {
        let entry = self.entry(id).ok_or_else(|| {
            Error::NotFound(Format::Bios, format!("member {id:#x} not found in PUP"))
        })?;

        match &self.source {
            PupSource::Memory(data) => {
                let start = entry.offset as usize;
                let end = start.saturating_add(entry.size as usize);
                if end > data.len() {
                    return Err(Error::Corrupt(Format::Bios, "tar member bounds exceed PUP data".into()));
                }
                parse_tar(&data[start..end])
            }
            PupSource::File(path) => {
                let mut file = File::open(path)?;
                file.seek(SeekFrom::Start(entry.offset))?;
                parse_tar_stream(&mut file, entry.size)
            }
        }
    }

    /// Inspects and reconciles all package members in `update_files.tar` (id `0x300`).
    ///
    /// Cross-checks each package against its counterpart in `spkg_hdr.tar` (id `0x501`).
    ///
    /// See `notes/research/ps3_pup.md` §3.1–§3.4.
    pub fn packages(&self) -> Result<Vec<PackageInfo>> {
        let uf_entries = self.tar_entries(0x300)?;
        let spkg_headers = self.read_spkg_headers()?;

        let mut packages = Vec::with_capacity(uf_entries.len());

        for m in uf_entries {
            // Read first 0x300 bytes of package (SCE header 0x20..0x280 + descriptor 0x80 = 0x300)
            let read_len = m.size.min(0x300);
            let pkg_bytes = self.read_member_range(0x300, m.data_offset, read_len)?;

            let sce_header = SceHeader::parse(&pkg_bytes)?;

            let (pkg_desc, rvk_desc, uncomp_ok, stored_ok) = if sce_header.header_type == 3 {
                // PKG type
                let desc_offset = sce_header.header_length as usize;
                if desc_offset.saturating_add(0x80) <= pkg_bytes.len() {
                    let desc = PkgDescriptor::parse(&pkg_bytes[desc_offset..desc_offset + 0x80])?;
                    let uncomp_ok = desc.uncompressed_size == sce_header.data_length.saturating_sub(0x80);
                    let stored_ok = desc.stored_size == m.size.saturating_sub(0x300);
                    (Some(desc), None, uncomp_ok, stored_ok)
                } else {
                    (None, None, false, false)
                }
            } else if sce_header.header_type == 2 {
                // RVK type
                let desc_offset = sce_header.header_length as usize;
                if desc_offset.saturating_add(0x20) <= pkg_bytes.len() {
                    let desc = RvkDescriptor::parse(&pkg_bytes[desc_offset..desc_offset + 0x20])?;
                    (None, Some(desc), true, true)
                } else {
                    (None, None, false, false)
                }
            } else {
                (None, None, true, true)
            };

            // Cross-check against spkg_hdr if present
            let spkg_name = format!("{}.spkg_hdr.1", m.name);
            let spkg_hdr_matched = spkg_headers.iter().find(|(name, _)| name == &spkg_name).map(|(_, hdr_bytes)| {
                pkg_bytes.len() >= 0x20 && &pkg_bytes[..0x20] == hdr_bytes
            });

            packages.push(PackageInfo {
                name: m.name,
                member_size: m.size,
                mtime: m.mtime,
                sce_header,
                pkg_descriptor: pkg_desc,
                rvk_descriptor: rvk_desc,
                uncompressed_reconciled: uncomp_ok,
                stored_reconciled: stored_ok,
                spkg_hdr_matched,
            });
        }

        Ok(packages)
    }

    /// Parses the plaintext SELF and ELF headers for an updater SELF (e.g. `ps3swu.self` `0x200` or `ps3swu2.self` `0x601`).
    ///
    /// See `notes/research/ps3_pup.md` §3.5.
    pub fn self_info(&self, id: u64) -> Result<SelfInfo> {
        let entry = self.entry(id).ok_or_else(|| {
            Error::NotFound(Format::Bios, format!("member {id:#x} not found in PUP"))
        })?;

        // First 0x300 bytes cover the SCE header (0x20), SELF ext header (0x50), app-info (0x20),
        // ELF64 header (0x40), and up to 7 program headers (7 * 0x38 = 0x188). 0x300 is plenty.
        let read_len = entry.size.min(0x400);
        let bytes = self.read_member_range(id, 0, read_len)?;

        SelfInfo::parse(&bytes)
    }

    fn read_spkg_headers(&self) -> Result<Vec<(String, [u8; 32])>> {
        if self.entry(0x501).is_none() {
            return Ok(Vec::new());
        }
        // spkg_hdr.tar is 80 KiB; reading it completely is fast and convenient
        let spkg_bytes = self.read_member(0x501)?;
        let tar_entries = parse_tar(&spkg_bytes)?;

        let mut results = Vec::with_capacity(tar_entries.len());
        for entry in tar_entries {
            let offset = entry.data_offset as usize;
            if offset.saturating_add(32) <= spkg_bytes.len() {
                let mut hdr = [0u8; 32];
                hdr.copy_from_slice(&spkg_bytes[offset..offset + 32]);
                results.push((entry.name, hdr));
            }
        }
        Ok(results)
    }
}

fn parse_header_prefix(header_buf: &[u8], total_size: u64) -> Result<(PupHeader, u64)> {
    if &header_buf[..8] != PUP_MAGIC {
        return Err(Error::NotA(Format::Bios, "invalid PUP magic (expected SCEUF)".into()));
    }

    let package_version = u64::from_be_bytes(header_buf[8..16].try_into().unwrap());
    let image_version = u64::from_be_bytes(header_buf[16..24].try_into().unwrap());
    let file_count = u64::from_be_bytes(header_buf[24..32].try_into().unwrap());
    let header_length = u64::from_be_bytes(header_buf[32..40].try_into().unwrap());
    let data_length = u64::from_be_bytes(header_buf[40..48].try_into().unwrap());

    // Arithmetic check 1: header_length + data_length == file size
    let expected_total = header_length.checked_add(data_length).ok_or_else(|| {
        Error::Corrupt(Format::Bios, "header_length + data_length overflow".into())
    })?;
    if expected_total != total_size {
        return Err(Error::Corrupt(
            Format::Bios,
            format!("PUP header ({header_length}) + data ({data_length}) = {expected_total} != file size {total_size}"),
        ));
    }

    // Arithmetic check 2: table inside header
    // 0x30 + 32*n (entries) + 32*n (digests) + 32 (header digest) = 0x30 + 64*n + 32
    let tables_len = file_count
        .checked_mul(64)
        .and_then(|v| v.checked_add(0x30))
        .and_then(|v| v.checked_add(32))
        .ok_or_else(|| Error::Corrupt(Format::Bios, "file count arithmetic overflow".into()))?;

    if header_length < tables_len {
        return Err(Error::Corrupt(
            Format::Bios,
            format!("PUP header_length ({header_length}) too small for entry and digest tables ({tables_len})"),
        ));
    }

    Ok((
        PupHeader {
            package_version,
            image_version,
            file_count,
            header_length,
            data_length,
        },
        tables_len,
    ))
}

fn parse_tables(
    header_bytes: &[u8],
    header: &PupHeader,
    _tables_len: u64,
) -> Result<(Vec<PupEntry>, Vec<PupDigestEntry>, [u8; 20])> {
    let count = header.file_count as usize;
    let mut entries = Vec::with_capacity(count);
    let mut digests = Vec::with_capacity(count);

    let entries_start = 0x30;
    let digests_start = entries_start + count * 32;
    let header_digest_start = digests_start + count * 32;

    if header_bytes.len() < header_digest_start + 32 {
        return Err(Error::Corrupt(Format::Bios, "PUP header buffer truncated".into()));
    }

    // Read entry table
    let mut expected_offset = header.header_length;
    for i in 0..count {
        let off = entries_start + i * 32;
        let id = u64::from_be_bytes(header_bytes[off..off + 8].try_into().unwrap());
        let entry_offset = u64::from_be_bytes(header_bytes[off + 8..off + 16].try_into().unwrap());
        let size = u64::from_be_bytes(header_bytes[off + 16..off + 24].try_into().unwrap());

        // Arithmetic check 3: contiguous members
        if entry_offset != expected_offset {
            return Err(Error::Corrupt(
                Format::Bios,
                format!("PUP entry {i} ({id:#x}) at offset {entry_offset:#x} is not contiguous (expected {expected_offset:#x})"),
            ));
        }
        expected_offset = expected_offset.checked_add(size).ok_or_else(|| {
            Error::Corrupt(Format::Bios, "member size offset overflow".into())
        })?;

        entries.push(PupEntry { id, offset: entry_offset, size });
    }

    // Check last entry ends at header_length + data_length
    let total_end = header.header_length.saturating_add(header.data_length);
    if count > 0 && expected_offset != total_end {
        return Err(Error::Corrupt(
            Format::Bios,
            format!("PUP last entry ends at {expected_offset} != total data end {total_end}"),
        ));
    }

    // Read digest table
    for i in 0..count {
        let off = digests_start + i * 32;
        let index = u64::from_be_bytes(header_bytes[off..off + 8].try_into().unwrap());
        if index != i as u64 {
            return Err(Error::Corrupt(
                Format::Bios,
                format!("PUP digest entry index mismatch: got {index}, expected {i}"),
            ));
        }
        let mut digest = [0u8; 20];
        digest.copy_from_slice(&header_bytes[off + 8..off + 28]);
        digests.push(PupDigestEntry { index, digest });
    }

    // Read header digest
    let mut header_digest = [0u8; 20];
    header_digest.copy_from_slice(&header_bytes[header_digest_start..header_digest_start + 20]);

    Ok((entries, digests, header_digest))
}

/// Metadata for a licence agreement locale from `license.xml`.
///
/// See `notes/research/ps3_pup.md` §2.1.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct LicenseLocale {
    /// Language code (e.g. `"en"`, `"ja"`, `"zh-s"`).
    pub lang: String,
    /// Byte length of the locale block.
    pub byte_length: usize,
    /// Date line (`msg_update_eula_2`).
    pub date: String,
    /// Copyright line (`msg_update_eula_39`).
    pub copyright: String,
}

fn parse_license_locales(bytes: &[u8]) -> Result<Vec<LicenseLocale>> {
    let text = std::str::from_utf8(bytes).map_err(|e| {
        Error::Corrupt(Format::Bios, format!("license.xml is not valid UTF-8: {e}"))
    })?;

    let mut locales = Vec::new();
    let mut search_from = 0;

    while let Some(start) = text[search_from..].find("<locale ") {
        let abs_start = search_from + start;
        let tag_end = match text[abs_start..].find('>') {
            Some(i) => abs_start + i,
            None => break,
        };
        let tag_header = &text[abs_start..tag_end];
        let lang = extract_attribute(tag_header, "lang").unwrap_or_default();

        let block_end = match text[abs_start..].find("</locale>") {
            Some(i) => abs_start + i + "</locale>".len(),
            None => text.len(),
        };

        let block = &text[abs_start..block_end];
        let date = extract_str_tag(block, "msg_update_eula_2").unwrap_or_default();
        let copyright = extract_str_tag(block, "msg_update_eula_39").unwrap_or_default();

        locales.push(LicenseLocale {
            lang,
            byte_length: block.len(),
            date,
            copyright,
        });

        search_from = block_end;
    }

    Ok(locales)
}

fn extract_attribute(tag: &str, attr: &str) -> Option<String> {
    let pattern = format!("{attr}=\"");
    let start = tag.find(&pattern)? + pattern.len();
    let end = tag[start..].find('"')? + start;
    Some(tag[start..end].to_string())
}

fn extract_str_tag(xml: &str, str_id: &str) -> Option<String> {
    let pattern = format!("id=\"{str_id}\">");
    let start = xml.find(&pattern)? + pattern.len();
    let end = xml[start..].find("</str>")? + start;
    let raw = &xml[start..end];
    // Clean &#xa; and newlines
    let cleaned = raw.replace("&#xa;", "").replace(['\r', '\n'], "");
    Some(cleaned.trim().to_string())
}

/// A directory entry in a ustar archive.
///
/// See `notes/research/ps3_pup.md` §3.1.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct TarEntry {
    /// File path/name.
    pub name: String,
    /// Size of the file in bytes.
    pub size: u64,
    /// Modification timestamp in seconds since UNIX epoch.
    pub mtime: u64,
    /// File permissions mode.
    pub mode: u32,
    /// Type flag (`b'0'` regular file, `b'5'` directory, etc.).
    pub typeflag: u8,
    /// Offset of the file's data within the tar archive.
    pub data_offset: u64,
}

/// Parses a ustar archive directory from an in-memory byte slice.
pub fn parse_tar(data: &[u8]) -> Result<Vec<TarEntry>> {
    let mut cursor = 0;
    let mut entries = Vec::new();

    while cursor + 512 <= data.len() {
        let block = &data[cursor..cursor + 512];
        if block.iter().all(|&b| b == 0) {
            // End of archive
            break;
        }

        let entry = parse_tar_header(block, (cursor + 512) as u64)?;
        let size = entry.size;
        entries.push(entry);

        let padded_size = size.checked_add(511).unwrap_or(size) / 512 * 512;
        let next = (cursor + 512).checked_add(padded_size as usize).ok_or_else(|| {
            Error::Corrupt(Format::Bios, "tar entry size overflow".into())
        })?;
        cursor = next;
    }

    Ok(entries)
}

fn parse_tar_stream(reader: &mut File, total_size: u64) -> Result<Vec<TarEntry>> {
    let mut entries = Vec::new();
    let mut offset = 0u64;

    while offset + 512 <= total_size {
        let mut block = [0u8; 512];
        reader.read_exact(&mut block)?;
        if block.iter().all(|&b| b == 0) {
            break;
        }

        let entry = parse_tar_header(&block, offset + 512)?;
        let size = entry.size;
        entries.push(entry);

        let padded = size.saturating_add(511) / 512 * 512;
        offset = offset.saturating_add(512).saturating_add(padded);
        reader.seek(SeekFrom::Current(padded as i64))?;
    }

    Ok(entries)
}

fn parse_tar_header(block: &[u8], data_offset: u64) -> Result<TarEntry> {
    if block.len() < 512 {
        return Err(Error::Corrupt(Format::Bios, "tar header truncated".into()));
    }
    let name_raw = &block[..100];
    let name_len = name_raw.iter().position(|&b| b == 0).unwrap_or(100);
    let base_name = std::str::from_utf8(&name_raw[..name_len]).unwrap_or("").trim();

    let prefix_raw = &block[345..500];
    let prefix_len = prefix_raw.iter().position(|&b| b == 0).unwrap_or(155);
    let prefix = std::str::from_utf8(&prefix_raw[..prefix_len]).unwrap_or("").trim();

    let full_name = if prefix.is_empty() {
        base_name.to_string()
    } else {
        format!("{prefix}/{base_name}")
    };

    let mode = parse_octal(&block[100..108]).unwrap_or(0) as u32;
    let size = parse_octal(&block[124..136]).unwrap_or(0);
    let mtime = parse_octal(&block[136..148]).unwrap_or(0);
    let typeflag = block[156];

    Ok(TarEntry {
        name: full_name,
        size,
        mtime,
        mode,
        typeflag,
        data_offset,
    })
}

fn parse_octal(bytes: &[u8]) -> Option<u64> {
    let s = std::str::from_utf8(bytes).ok()?.trim().trim_matches('\0').trim();
    if s.is_empty() {
        return Some(0);
    }
    u64::from_str_radix(s, 8).ok()
}

/// Plaintext SCE container header (0x20 bytes).
///
/// See `notes/research/ps3_pup.md` §3.2.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct SceHeader {
    /// Magic bytes (`SCE\0`).
    pub magic: [u8; 4],
    /// Container version (typically 2).
    pub version: u32,
    /// Key revision (`0` for PKG/RVK, `1` for `ps3swu.self`, `13` for `ps3swu2.self`).
    pub key_revision: u16,
    /// Header type (`1` = SELF, `2` = RVK, `3` = PKG).
    pub header_type: u16,
    /// Metadata offset (`0` for PKG/RVK, `0x3A0` for SELF).
    pub metadata_offset: u32,
    /// Header length in bytes (`0x280` PKG, `0x200` RVK, `0x880` SELF).
    pub header_length: u64,
    /// Uncompressed payload length in bytes.
    pub data_length: u64,
}

impl SceHeader {
    /// Parses an SCE header from a 0x20-byte slice.
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < 0x20 {
            return Err(Error::Corrupt(Format::Bios, "SCE header truncated".into()));
        }
        if &bytes[..4] != SCE_MAGIC {
            return Err(Error::NotA(Format::Bios, "invalid SCE magic".into()));
        }

        let magic = [bytes[0], bytes[1], bytes[2], bytes[3]];
        let version = u32::from_be_bytes(bytes[4..8].try_into().unwrap());
        let key_revision = u16::from_be_bytes(bytes[8..10].try_into().unwrap());
        let header_type = u16::from_be_bytes(bytes[10..12].try_into().unwrap());
        let metadata_offset = u32::from_be_bytes(bytes[12..16].try_into().unwrap());
        let header_length = u64::from_be_bytes(bytes[16..24].try_into().unwrap());
        let data_length = u64::from_be_bytes(bytes[24..32].try_into().unwrap());

        Ok(Self {
            magic,
            version,
            key_revision,
            header_type,
            metadata_offset,
            header_length,
            data_length,
        })
    }
}

/// Plaintext package descriptor (0x80 bytes) following the SCE header in type-3 packages.
///
/// See `notes/research/ps3_pup.md` §3.3.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct PkgDescriptor {
    /// Constant value (typically 3).
    pub constant: u32,
    /// Package kind (`1` CoreOS, `3` Flash TAR, `4` UPL.xml, `7` BD drive FW, `8` Syscon FW).
    pub kind: u32,
    /// Sequence / identifier word.
    pub sequence: u64,
    /// Version / timestamp word.
    pub version_stamp: u64,
    /// Uncompressed payload size in bytes.
    pub uncompressed_size: u64,
    /// Stored payload size in bytes.
    pub stored_size: u64,
    /// Flags word (e.g. `0x4000_0000` for CoreOS/Bluetooth/MultiCard).
    pub flags: u64,
}

impl PkgDescriptor {
    /// Parses a package descriptor from a 0x80-byte slice.
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < 0x80 {
            return Err(Error::Corrupt(Format::Bios, "PKG descriptor truncated".into()));
        }

        let constant = u32::from_be_bytes(bytes[0..4].try_into().unwrap());
        let kind = u32::from_be_bytes(bytes[4..8].try_into().unwrap());
        let sequence = u64::from_be_bytes(bytes[8..16].try_into().unwrap());
        let version_stamp = u64::from_be_bytes(bytes[16..24].try_into().unwrap());
        let uncompressed_size = u64::from_be_bytes(bytes[24..32].try_into().unwrap());
        let stored_size = u64::from_be_bytes(bytes[32..40].try_into().unwrap());
        let flags = u64::from_be_bytes(bytes[40..48].try_into().unwrap());

        Ok(Self {
            constant,
            kind,
            sequence,
            version_stamp,
            uncompressed_size,
            stored_size,
            flags,
        })
    }

    /// Public name of the package kind.
    #[must_use]
    pub fn kind_name(&self) -> &'static str {
        match self.kind {
            1 => "CoreOS",
            3 => "Flash TAR",
            4 => "UPL.xml",
            7 => "BD Drive FW",
            8 => "Syscon FW",
            _ => "Unknown",
        }
    }
}

/// Plaintext descriptor header for an RVK revocation list image.
///
/// See `notes/research/ps3_pup.md` §3.3.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct RvkDescriptor {
    /// RVK kind (`3` package RVK, `4` program RVK).
    pub kind: u32,
    /// Sequence word.
    pub sequence: u32,
    /// Version stamp.
    pub version_stamp: u64,
    /// Number of entries.
    pub entries: u32,
}

impl RvkDescriptor {
    /// Parses an RVK descriptor header.
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < 0x20 {
            return Err(Error::Corrupt(Format::Bios, "RVK descriptor truncated".into()));
        }
        let kind = u32::from_be_bytes(bytes[0..4].try_into().unwrap());
        let sequence = u32::from_be_bytes(bytes[4..8].try_into().unwrap());
        let version_stamp = u64::from_be_bytes(bytes[8..16].try_into().unwrap());
        let entries = u32::from_be_bytes(bytes[16..20].try_into().unwrap());

        Ok(Self {
            kind,
            sequence,
            version_stamp,
            entries,
        })
    }
}

/// Information and reconciliation status for a package in `update_files.tar`.
///
/// See `notes/research/ps3_pup.md` §3.1–§3.4.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct PackageInfo {
    /// TAR member file name.
    pub name: String,
    /// TAR member size in bytes.
    pub member_size: u64,
    /// Modification time.
    pub mtime: u64,
    /// Plaintext SCE container header.
    pub sce_header: SceHeader,
    /// PKG descriptor if type-3 package.
    pub pkg_descriptor: Option<PkgDescriptor>,
    /// RVK descriptor if type-2 revocation list.
    pub rvk_descriptor: Option<RvkDescriptor>,
    /// Whether uncompressed size reconciles (`uncompressed == SCE data_length - 0x80`).
    pub uncompressed_reconciled: bool,
    /// Whether stored size reconciles (`stored == member_size - 0x300`).
    pub stored_reconciled: bool,
    /// Whether the first 0x20 bytes match the counterpart in `spkg_hdr.tar`.
    pub spkg_hdr_matched: Option<bool>,
}

/// Plaintext extended SELF header at offset 0x20.
///
/// See `notes/research/ps3_pup.md` §3.5.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct SelfExtendedHeader {
    /// Extended header type (typically 3).
    pub header_type: u64,
    /// Offset to app-info block.
    pub app_info_offset: u64,
    /// Offset to ELF header.
    pub elf_offset: u64,
    /// Offset to program headers (phdrs).
    pub phdrs_offset: u64,
    /// Offset to section headers (shdrs).
    pub shdrs_offset: u64,
    /// Offset to section info table.
    pub section_info_offset: u64,
    /// Offset to SCE version block.
    pub sce_version_offset: u64,
    /// Offset to control info block.
    pub control_info_offset: u64,
    /// Size of control info block.
    pub control_info_size: u64,
}

impl SelfExtendedHeader {
    /// Parses an extended SELF header.
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < 0x50 {
            return Err(Error::Corrupt(Format::Bios, "SELF extended header truncated".into()));
        }
        Ok(Self {
            header_type: u64::from_be_bytes(bytes[0..8].try_into().unwrap()),
            app_info_offset: u64::from_be_bytes(bytes[8..16].try_into().unwrap()),
            elf_offset: u64::from_be_bytes(bytes[16..24].try_into().unwrap()),
            phdrs_offset: u64::from_be_bytes(bytes[24..32].try_into().unwrap()),
            shdrs_offset: u64::from_be_bytes(bytes[32..40].try_into().unwrap()),
            section_info_offset: u64::from_be_bytes(bytes[40..48].try_into().unwrap()),
            sce_version_offset: u64::from_be_bytes(bytes[48..56].try_into().unwrap()),
            control_info_offset: u64::from_be_bytes(bytes[56..64].try_into().unwrap()),
            control_info_size: u64::from_be_bytes(bytes[64..72].try_into().unwrap()),
        })
    }
}

/// Plaintext app-info block from a SELF file.
///
/// See `notes/research/ps3_pup.md` §3.5.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct SelfAppInfo {
    /// Authorization identifier.
    pub auth_id: u64,
    /// Vendor identifier.
    pub vendor_id: u32,
    /// SELF type (`4` for application).
    pub self_type: u32,
    /// Application version (`0x0004_0082_0000_0000` = 4.82).
    pub version: u64,
}

impl SelfAppInfo {
    /// Parses an app-info block.
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < 0x20 {
            return Err(Error::Corrupt(Format::Bios, "SELF app-info truncated".into()));
        }
        Ok(Self {
            auth_id: u64::from_be_bytes(bytes[0..8].try_into().unwrap()),
            vendor_id: u32::from_be_bytes(bytes[8..12].try_into().unwrap()),
            self_type: u32::from_be_bytes(bytes[12..16].try_into().unwrap()),
            version: u64::from_be_bytes(bytes[16..24].try_into().unwrap()),
        })
    }
}

/// Plaintext ELF64 header inside a SELF file.
///
/// See `notes/research/ps3_pup.md` §3.5.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Elf64Header {
    /// ELF class (`2` = 64-bit).
    pub class: u8,
    /// Data encoding (`2` = big-endian).
    pub data: u8,
    /// OS / ABI (`102` = Cell OS LV2).
    pub os_abi: u8,
    /// Object file type (`2` = EXEC).
    pub e_type: u16,
    /// Machine architecture (`21` = PPC64).
    pub e_machine: u16,
    /// Entry point address.
    pub e_entry: u64,
    /// Program header table offset.
    pub e_phoff: u64,
    /// Section header table offset.
    pub e_shoff: u64,
    /// Number of program headers.
    pub e_phnum: u16,
    /// Number of section headers.
    pub e_shnum: u16,
}

impl Elf64Header {
    /// Parses an ELF64 header.
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < 0x40 {
            return Err(Error::Corrupt(Format::Bios, "ELF header truncated".into()));
        }
        if &bytes[..4] != b"\x7fELF" {
            return Err(Error::NotA(Format::Bios, "invalid ELF magic".into()));
        }

        let class = bytes[4];
        let data = bytes[5];
        let os_abi = bytes[7];

        let e_type = u16::from_be_bytes(bytes[16..18].try_into().unwrap());
        let e_machine = u16::from_be_bytes(bytes[18..20].try_into().unwrap());
        let e_entry = u64::from_be_bytes(bytes[24..32].try_into().unwrap());
        let e_phoff = u64::from_be_bytes(bytes[32..40].try_into().unwrap());
        let e_shoff = u64::from_be_bytes(bytes[40..48].try_into().unwrap());
        let e_phnum = u16::from_be_bytes(bytes[56..58].try_into().unwrap());
        let e_shnum = u16::from_be_bytes(bytes[60..62].try_into().unwrap());

        Ok(Self {
            class,
            data,
            os_abi,
            e_type,
            e_machine,
            e_entry,
            e_phoff,
            e_shoff,
            e_phnum,
            e_shnum,
        })
    }
}

/// ELF64 program header (Phdr).
///
/// See `notes/research/ps3_pup.md` §3.5.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Elf64Phdr {
    /// Segment type (`1` = `PT_LOAD`, etc.).
    pub p_type: u32,
    /// Segment flags (`5` = RX, `6` = RW, `0x400004` = SPU image).
    pub p_flags: u32,
    /// Offset in file.
    pub p_offset: u64,
    /// Virtual address.
    pub p_vaddr: u64,
    /// Physical address.
    pub p_paddr: u64,
    /// Segment file size.
    pub p_filesz: u64,
    /// Segment memory size.
    pub p_memsz: u64,
    /// Alignment.
    pub p_align: u64,
}

impl Elf64Phdr {
    /// Parses an ELF64 program header from a 56-byte slice.
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < 56 {
            return Err(Error::Corrupt(Format::Bios, "ELF program header truncated".into()));
        }
        Ok(Self {
            p_type: u32::from_be_bytes(bytes[0..4].try_into().unwrap()),
            p_flags: u32::from_be_bytes(bytes[4..8].try_into().unwrap()),
            p_offset: u64::from_be_bytes(bytes[8..16].try_into().unwrap()),
            p_vaddr: u64::from_be_bytes(bytes[16..24].try_into().unwrap()),
            p_paddr: u64::from_be_bytes(bytes[24..32].try_into().unwrap()),
            p_filesz: u64::from_be_bytes(bytes[32..40].try_into().unwrap()),
            p_memsz: u64::from_be_bytes(bytes[40..48].try_into().unwrap()),
            p_align: u64::from_be_bytes(bytes[48..56].try_into().unwrap()),
        })
    }
}

/// Plaintext headers and structure of a PS3 SELF container (such as `ps3swu.self`).
///
/// See `notes/research/ps3_pup.md` §3.5.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct SelfInfo {
    /// SCE container header.
    pub sce_header: SceHeader,
    /// Extended SELF header.
    pub ext_header: SelfExtendedHeader,
    /// App-info block.
    pub app_info: SelfAppInfo,
    /// Plaintext ELF64 header.
    pub elf_header: Elf64Header,
    /// Program headers.
    pub program_headers: Vec<Elf64Phdr>,
}

impl SelfInfo {
    /// Parses the plaintext structures of a SELF file.
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let sce_header = SceHeader::parse(bytes)?;
        if sce_header.header_type != 1 {
            return Err(Error::NotA(Format::Bios, "not a SELF container (header_type != 1)".into()));
        }

        if bytes.len() < 0x70 {
            return Err(Error::Corrupt(Format::Bios, "SELF header truncated".into()));
        }
        let ext_header = SelfExtendedHeader::parse(&bytes[0x20..0x70])?;

        let app_off = ext_header.app_info_offset as usize;
        if bytes.len() < app_off + 0x20 {
            return Err(Error::Corrupt(Format::Bios, "SELF app-info out of bounds".into()));
        }
        let app_info = SelfAppInfo::parse(&bytes[app_off..app_off + 0x20])?;

        let elf_off = ext_header.elf_offset as usize;
        if bytes.len() < elf_off + 0x40 {
            return Err(Error::Corrupt(Format::Bios, "SELF ELF header out of bounds".into()));
        }
        let elf_header = Elf64Header::parse(&bytes[elf_off..elf_off + 0x40])?;

        let phdr_off = ext_header.phdrs_offset as usize;
        let mut program_headers = Vec::with_capacity(elf_header.e_phnum as usize);
        for i in 0..elf_header.e_phnum as usize {
            let off = phdr_off + i * 56;
            if bytes.len() < off + 56 {
                return Err(Error::Corrupt(Format::Bios, "ELF program headers out of bounds".into()));
            }
            program_headers.push(Elf64Phdr::parse(&bytes[off..off + 56])?);
        }

        Ok(Self {
            sce_header,
            ext_header,
            app_info,
            elf_header,
            program_headers,
        })
    }
}
