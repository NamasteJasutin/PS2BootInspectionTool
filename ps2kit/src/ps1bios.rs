//! Standalone PS1/PSone/POPS BIOS identity and audit, derived from the supplied bytes.
//! Layout and feature evidence: `notes/research/ps1_version_matrix.md` §§1–2, 5–6.

use crate::bytes::Bytes;
use crate::rom::unpack;
use crate::{Error, Format, Result};

const BIOS_SIZE: usize = 0x80000;
const VERSION_OFFSET: usize = 0x7FF32;
const MAKER: &[u8] = b"Sony Computer Entertainment Inc.";

/// Kernel generations distinguished by the A0 copy loop and table, not a hash or version.
/// `ps1_kernel_boot.md` §§1.1, 1.6, 6.4: K1 implements 0xAF slots; K2 adds A0:B4.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum KernelGeneration {
    /// The shorter A0 table without GetSystemInfo.
    K1,
    /// The extended A0 table with GetSystemInfo.
    K2,
}

/// Shell storage identified from the entry instructions (`ps1_version_matrix.md` §§2, 6.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ShellStorage {
    /// The kernel's copy window is already executable shell code.
    Raw,
    /// The standard 0x4C-byte loader precedes an OSD LZ stream.
    Packed,
    /// POPS's two-stage loader precedes its LZ stream at +0x1B0.
    Pops,
}

/// Header date decoded from BCD at 0x100 (`ps1_version_matrix.md` §1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct BiosDate {
    /// Four-digit build year.
    pub year: u16,
    /// Calendar month, 1..=12.
    pub month: u8,
    /// Calendar day, 1..=31.
    pub day: u8,
}

/// Identity read from this image; a missing letter remains unknown, including 1.0/1.1.
/// Sources: `ps1_version_matrix.md` §§1–2; `ps1_kernel_boot.md` §§1.1, 1.6, 6.4.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Ps1Identity {
    /// Kernel build date; absent if its BCD or calendar fields are invalid.
    pub header_date: Option<BiosDate>,
    /// Maker text from 0x108.
    pub maker: String,
    /// CEX model text located within the header (starts at 0x129 or 0x12C).
    pub model: String,
    /// Complete tail version string, empty when absent.
    pub version: String,
    /// Tail letter, without guessing from the file name or model.
    pub region_letter: Option<char>,
    /// NUL-terminated PS-X Realtime Kernel banner located by content.
    pub kernel_banner: String,
    /// Generation established by A0 code features; unknown for unrecognised code.
    pub kernel_generation: Option<KernelGeneration>,
    /// Number of A0 slots read from its bootstrap copy loop.
    pub a0_entries: Option<usize>,
    /// Contiguous nonzero function slots; K1 implements 0xAF, K2 implements 0xB5.
    /// Dump correction to kernel §6.4: K1's copy includes 0xBF words, with zero padding.
    pub a0_implemented_entries: Option<usize>,
    /// GetSystemInfo's ROM address from A0:B4, when present.
    pub get_system_info: Option<u32>,
    /// How the shell is stored in ROM.
    pub shell_storage: ShellStorage,
    /// Raw copy-window length or decoded LZ output length.
    pub shell_length: usize,
}

/// A dump anomaly (`ps1_version_matrix.md` §§1, 5); missing version is normal on SCPH-1000.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Ps1AuditIssue {
    /// Standalone ROMs are exactly 512 KiB.
    UnexpectedSize {
        /// Actual byte count.
        actual: usize,
    },
    /// Nearly every LF has a preceding CR, consistent with a text-mode transfer.
    TextModeCorruption {
        /// First suspect inserted CR's file offset.
        first_offset: usize,
    },
    /// No valid version text in the ROM's tail.
    MissingVersion,
    /// Version text exists away from its expected tail offset.
    MisplacedVersion {
        /// Observed file offset.
        offset: usize,
    },
}

/// Audit statistics usable even when the file cannot be loaded as a BIOS.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Ps1Audit {
    /// Actual file length.
    pub size: usize,
    /// LF bytes, including those in CRLF pairs.
    pub lf_count: usize,
    /// CRLF pairs; divide by lf_count for CR-before-LF density.
    pub crlf_count: usize,
    /// First likely inserted CR when text-mode corruption is detected.
    pub first_shifted_offset: Option<usize>,
    /// Valid tail version's offset, including a displaced version.
    pub version_offset: Option<usize>,
    /// Size, transfer and version anomalies.
    pub issues: Vec<Ps1AuditIssue>,
}

impl Ps1Audit {
    /// Whether size or text-mode damage prevents a faithful standalone load.
    pub fn is_corrupt(&self) -> bool {
        self.issues.iter().any(|i| matches!(i, Ps1AuditIssue::UnexpectedSize { .. } | Ps1AuditIssue::TextModeCorruption { .. }))
    }
}

fn version_at(d: &[u8], o: usize) -> Option<String> {
    let bytes = d.get(o..)?.get(..96.min(d.len() - o))?;
    let end = bytes.iter().position(|&b| b == 0)?;
    let s = std::str::from_utf8(&bytes[..end]).ok()?;
    (s.starts_with("System ROM Version ") && s.bytes().all(|b| (32..127).contains(&b))).then(|| s.to_string())
}

pub(crate) fn region_letter(version: &str) -> Option<char> {
    if version.starts_with("System ROM Version ") {
        version.split_whitespace().last().filter(|s| s.len() == 1)?.chars().next().filter(char::is_ascii_uppercase)
    } else {
        version.chars().nth(4).filter(char::is_ascii_uppercase)
    }
}

/// Audits size, CR-before-LF density and version placement without repairing any bytes.
/// `ps1_version_matrix.md` §5: the [h] image first shifts at 0x24A, and has no lone LF.
/// The density threshold is a diagnostic heuristic, not a console rule.
pub fn audit(d: &[u8]) -> Ps1Audit {
    let lf_count = d.iter().filter(|&&b| b == b'\n').count();
    let crlf_count = d.windows(2).filter(|w| *w == b"\r\n").count();
    let first_shifted_offset = (lf_count >= 128 && crlf_count * 100 / lf_count >= 95)
        .then(|| d.windows(2).position(|w| w == b"\r\n")).flatten();
    let version_offset = if version_at(d, VERSION_OFFSET).is_some() { Some(VERSION_OFFSET) } else {
        // Earlier shells contain a fallback "System ROM Version 1.0" diagnostic, not identity.
        d.get(0x7FE70..).and_then(|tail| tail.windows(19).enumerate()
            .find_map(|(i, w)| (w == b"System ROM Version ").then(|| version_at(d, 0x7FE70 + i)).flatten().map(|_| 0x7FE70 + i)))
    };
    let mut issues = Vec::new();
    if d.len() != BIOS_SIZE { issues.push(Ps1AuditIssue::UnexpectedSize { actual: d.len() }) }
    if let Some(first_offset) = first_shifted_offset { issues.push(Ps1AuditIssue::TextModeCorruption { first_offset }) }
    match version_offset {
        None => issues.push(Ps1AuditIssue::MissingVersion),
        Some(offset) if offset != VERSION_OFFSET => issues.push(Ps1AuditIssue::MisplacedVersion { offset }),
        _ => {}
    }
    Ps1Audit { size: d.len(), lf_count, crlf_count, first_shifted_offset, version_offset, issues }
}

/// A comparison finding over files supplied by the caller; no known-image hashes are used.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Ps1SetIssue {
    /// Two images are byte-identical.
    Duplicate {
        /// First caller-supplied name.
        first: String,
        /// Second caller-supplied name.
        second: String,
    },
    /// A named region disagrees with the actual tail version letter.
    RegionMismatch {
        /// Caller-supplied file name.
        file: String,
        /// Region letter implied by (J), (U)/(US), or (E).
        filename_letter: char,
        /// Letter read from the version string.
        version_letter: char,
    },
}

/// Finds duplicate bytes and explicit file-name region versus tail-letter disagreements.
/// Naming evidence: `ps1_version_matrix.md` §1; applies also to PS2 PS1-mode tail strings.
pub fn audit_set(images: &[(&str, &[u8])]) -> Vec<Ps1SetIssue> {
    let mut issues = Vec::new();
    for (i, &(name, data)) in images.iter().enumerate() {
        for &(other, bytes) in &images[..i] {
            if data == bytes { issues.push(Ps1SetIssue::Duplicate { first: other.into(), second: name.into() }) }
        }
        let upper = name.to_ascii_uppercase();
        let filename_letter = if upper.contains("(J)") { Some('J') } else if upper.contains("(U)") || upper.contains("(US)") { Some('A') } else if upper.contains("(E)") { Some('E') } else { None };
        let version_letter = audit(data).version_offset.and_then(|o| version_at(data, o)).and_then(|s| region_letter(&s));
        if let (Some(filename_letter), Some(version_letter)) = (filename_letter, version_letter) {
            if filename_letter != version_letter { issues.push(Ps1SetIssue::RegionMismatch { file: name.into(), filename_letter, version_letter }) }
        }
    }
    issues
}

/// A standalone BIOS and its decoded identity; owned bytes are never modified.
#[derive(Debug, Clone)]
pub struct Ps1Bios {
    data: Vec<u8>,
    identity: Ps1Identity,
}

impl Ps1Bios {
    /// Recognises a 512 KiB standalone dump, excluding ROMDIR RESET images.
    /// Detection evidence: `ps1_shell_scenes.md` §5; identity header: matrix §1.
    pub fn detect(d: &[u8]) -> bool {
        d.len() == BIOS_SIZE && d.get(0x108..0x128) == Some(MAKER)
            && !d.windows(10).any(|w| w == b"RESET\0\0\0\0\0")
            && d.windows(20).any(|w| w == b"PS-X Realtime Kernel")
    }

    /// Reads identity and validates shell storage; corrupt transfers return a BIOS error.
    pub fn new(data: Vec<u8>) -> Result<Self> {
        let report = audit(&data);
        if report.is_corrupt() { return Err(Error::Corrupt(Format::Bios, format!("standalone PS1 dump: {:?}", report.issues))) }
        if !Self::detect(&data) { return Err(Error::NotA(Format::Bios, "not a standalone PS1 BIOS".into())) }
        let d = data.as_slice();
        let bcd = |b: u8| ((b & 15) <= 9 && (b >> 4) <= 9).then_some((b >> 4) * 10 + (b & 15));
        let header_date = (|| {
            let day = bcd(d[0x100])?;
            let month = bcd(d[0x101])?;
            let year = bcd(d[0x103])? as u16 * 100 + bcd(d[0x102])? as u16;
            ((1..=12).contains(&month) && (1..=31).contains(&day)).then_some(BiosDate { year, month, day })
        })();
        let model = d[0x128..0x180].windows(4).position(|w| w == b"CEX-").map(|i| d.cstr(0x128 + i, 0x58 - i)).unwrap_or_default();
        let version = report.version_offset.and_then(|o| version_at(d, o)).unwrap_or_default();
        let banner = d.windows(20).position(|w| w == b"PS-X Realtime Kernel").ok_or_else(|| Error::NotFound(Format::Bios, "kernel banner".into()))?;
        let (a0_entries, a0_implemented_entries, get_system_info) = a0_features(d).map(|(table, n)| {
            let info = (n > 0xB4).then(|| d.u32(table + 0xB4 * 4)).filter(|&p| (0xBFC00000..0xBFC10000).contains(&p));
            let implemented = (0..n).take_while(|&i| (0xBFC00000..0xBFC10000).contains(&d.u32(table + i * 4))).count();
            (Some(n), Some(implemented), info)
        }).unwrap_or((None, None, None));
        let kernel_generation = match (a0_entries, a0_implemented_entries, get_system_info) {
            (Some(0xBF), Some(0xAF), None) => Some(KernelGeneration::K1),
            (Some(0xC1), Some(0xB5), Some(_)) => Some(KernelGeneration::K2),
            _ => None,
        };
        let shell_storage = match (d.u32(0x18000), d.u32(0x18004)) {
            (0x3C048003, 0x2484004C) => ShellStorage::Packed,
            (0x3C058019, 0x34A51000) => ShellStorage::Pops,
            (w, _) if w >> 16 == 0x27BD => ShellStorage::Raw,
            _ => return Err(Error::Corrupt(Format::Bios, "unknown PS1 shell loader stub".into())),
        };
        let shell_length = decode_shell(d, shell_storage)?.len();
        let identity = Ps1Identity { header_date, maker: d.cstr(0x108, 32), model, region_letter: region_letter(&version), version,
            kernel_banner: d.cstr(banner, 160), kernel_generation, a0_entries, a0_implemented_entries, get_system_info, shell_storage, shell_length };
        Ok(Self { data, identity })
    }

    /// The original dump bytes.
    pub fn data(&self) -> &[u8] { &self.data }

    /// Identity established from this dump's header, strings and code.
    pub fn identity(&self) -> &Ps1Identity { &self.identity }

    /// Raw or unpacked shell at RAM base 0x80030000; matrix §§2.1, 6.2.
    pub fn shell_image(&self) -> Result<Vec<u8>> { decode_shell(&self.data, self.identity.shell_storage) }

    /// SJIS font at ROM 0x66000..0x7FE70 (`ps1_version_matrix.md` §2.1).
    pub fn krom(&self) -> &[u8] { &self.data[0x66000..0x7FE70] }
}

fn decode_shell(d: &[u8], storage: ShellStorage) -> Result<Vec<u8>> {
    let s = &d[0x18000..0x7FFF0];
    match storage {
        ShellStorage::Raw => Ok(s.to_vec()),
        ShellStorage::Packed => unpack(s, 0x4C),
        ShellStorage::Pops => unpack(s, 0x1B0),
    }
}

// Locate the A0 copy by its destination (0x200), loop and ROM source/end registers.
// `ps1_kernel_boot.md` §1.1 and §6.4: the two builds move this routine by 0x10.
fn a0_features(d: &[u8]) -> Option<(usize, usize)> {
    for o in (0..0x10000 - 40).step_by(4) {
        if d.u32(o) != 0x3C04BFC0 || d.u32(o + 4) != 0x3C05BFC0
            || d.u32(o + 8) >> 16 != 0x2484 || d.u32(o + 12) >> 16 != 0x24A5
            || d.u32(o + 16) != 0x24060200 || d.u32(o + 20) != 0x8C870000
            || d.u32(o + 24) != 0x20840004 || d.u32(o + 28) != 0x20C60004
            || d.u32(o + 32) != 0x1485FFFC || d.u32(o + 36) != 0xACC7FFFC { continue }
        let start = d.u16(o + 8) as usize;
        let end = d.u16(o + 12) as usize;
        if start < end && end <= 0x10000 && (end - start).is_multiple_of(4) {
            return Some((start, (end - start) / 4));
        }
    }
    None
}
