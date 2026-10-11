//! PS3 disc and save metadata, without decryption or authentication.
//!
//! The scope follows `notes/research/ps3_pup.md` §6. Format facts below are \[I\],
//! from the PS3 Developer wiki's [PARAM.SFO](https://www.psdevwiki.com/ps3/PARAM.SFO)
//! §Internal Structure and named-key sections, [PS3_DISC.SFB](https://www.psdevwiki.com/ps3/PS3_DISC.SFB)
//! §Internal structure/Hybrid Flags, [Bluray disc](https://www.psdevwiki.com/ps3/Bluray_disc)
//! §Encryption (region table only), and [PARAM.PFD](https://www.psdevwiki.com/ps3/PARAM.PFD)
//! §PFD Header. None is verified against a local PS3 disc or save.
//!
//! ISO support is ISO 9660 only: UDF-only images are rejected. Sector-zero ranges
//! describe *plaintext* regions: BE u32 count at 0, reserved u32 at 4, then count
//! pairs of BE u32 first/last LBAs at 8. Endpoints are treated as inclusive \[I\];
//! gaps are encrypted. Every sector of a file must lie in plaintext ranges before
//! any of its bytes are read. An absent map never grants permission to read files,
//! even when their names are visible. No key input or decryption path exists.

use crate::probe::DiscFiles;
use crate::pup::{parse_sce_header, Pup, SceHeader};
use crate::sectors::SectorReader;
use crate::{Error, Format, Result};
use std::collections::{HashSet, VecDeque};
use std::fmt;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

fn corrupt(message: impl Into<String>) -> Error {
    Error::Corrupt(Format::DiscImage, message.into())
}
fn range(bytes: &[u8], offset: usize, length: usize) -> Result<&[u8]> {
    offset
        .checked_add(length)
        .and_then(|end| bytes.get(offset..end))
        .ok_or_else(|| corrupt("PS3 metadata offset/length out of bounds"))
}
fn le16(bytes: &[u8], offset: usize) -> Result<u16> {
    let b = range(bytes, offset, 2)?;
    Ok(u16::from_le_bytes([b[0], b[1]]))
}
fn le32(bytes: &[u8], offset: usize) -> Result<u32> {
    let b = range(bytes, offset, 4)?;
    Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}
fn be32(bytes: &[u8], offset: usize) -> Result<u32> {
    let b = range(bytes, offset, 4)?;
    Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}
fn be64(bytes: &[u8], offset: usize) -> Result<u64> {
    let b = range(bytes, offset, 8)?;
    Ok(u64::from_be_bytes(std::array::from_fn(|i| b[i])))
}
fn ctext(bytes: &[u8]) -> Option<&str> {
    std::str::from_utf8(bytes.split(|&b| b == 0).next()?).ok()
}

/// A comparable PS3 firmware version; `04.8200` and `4.82` compare equally.
/// \[I\] PARAM.SFO §PS3_SYSTEM_VER; PUP spelling: `ps3_pup.md` §2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub struct FirmwareVersion {
    major: u32,
    fraction: u16,
}
impl FirmwareVersion {
    /// Parses decimal major and one to four fractional digits, without floating point.
    pub fn parse(text: &str) -> Result<Self> {
        let text = text.trim();
        let (major, fraction) = text.split_once('.').ok_or_else(|| corrupt("invalid PS3 firmware version"))?;
        if major.is_empty()
            || !major.bytes().all(|b| b.is_ascii_digit())
            || fraction.is_empty()
            || fraction.len() > 4
            || !fraction.bytes().all(|b| b.is_ascii_digit())
        {
            return Err(corrupt(format!("invalid PS3 firmware version: {text:?}")));
        }
        let major = major.parse().map_err(|_| corrupt("PS3 firmware major version overflow"))?;
        let fraction: u16 = fraction.parse::<u16>().map_err(|_| corrupt("invalid PS3 firmware fraction"))?
            * 10u16.pow(4 - fraction.len() as u32);
        Ok(Self { major, fraction })
    }
}
impl fmt::Display for FirmwareVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let fraction = format!("{:04}", self.fraction);
        let fraction = fraction.trim_end_matches('0');
        write!(f, "{}.{:0<2}", self.major, fraction)
    }
}

/// An SFO value; binary special values and unknown formats remain byte-exact.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum SfoValue {
    /// UTF-8 text without its terminating NUL.
    Text(String),
    /// Little-endian int32 bits, including flags (signed interpretation is caller-specific).
    Integer(u32),
    /// Binary, unknown-format or reserved (zero used length) data.
    Raw(Vec<u8>),
}
impl fmt::Display for SfoValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Text(s) => write!(f, "{s:?}"),
            Self::Integer(n) => write!(f, "{n} (0x{n:08x})"),
            Self::Raw(b) => write!(f, "{} raw bytes", b.len()),
        }
    }
}

/// One PSF index entry, with the on-disk format and both lengths retained.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct SfoEntry {
    /// NUL-terminated key from the key table.
    pub key: String,
    /// Format: 0x0004 special/binary, 0x0204 UTF-8, 0x0404 int32 \[I\].
    pub format: u16,
    /// Used data length, including text's terminator.
    pub length: u32,
    /// Allocated data length.
    pub max_length: u32,
    /// Relative data-table offset.
    pub data_offset: u32,
    /// Parsed used bytes.
    pub value: SfoValue,
}

/// A plaintext PARAM.SFO (PSF) key/data table. \[I\] PARAM.SFO §Internal Structure.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Sfo {
    /// Stored PSF version word (normally 0x101).
    pub version: u32,
    /// Absolute key-table offset.
    pub key_table_offset: u32,
    /// Absolute data-table offset.
    pub data_table_offset: u32,
    /// Every entry in table order.
    pub entries: Vec<SfoEntry>,
}
impl Sfo {
    /// Parses checked offsets, terminated keys/text, int32s and opaque special values.
    /// Unknown formats are preserved. Duplicate keys and overlapping allocations are errors.
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if range(bytes, 0, 4)? != b"\0PSF" {
            return Err(corrupt("invalid PARAM.SFO magic"));
        }
        let version = le32(bytes, 4)?;
        let key_table_offset = le32(bytes, 8)?;
        let data_table_offset = le32(bytes, 12)?;
        let count = le32(bytes, 16)? as usize;
        let index_end =
            count.checked_mul(16).and_then(|n| n.checked_add(20)).ok_or_else(|| corrupt("SFO index table overflow"))?;
        let keys = key_table_offset as usize;
        let data = data_table_offset as usize;
        if index_end > keys || keys > data || data > bytes.len() {
            return Err(corrupt("invalid SFO table order/bounds"));
        }
        let mut entries = Vec::with_capacity(count);
        let mut names = HashSet::new();
        let mut allocations = Vec::new();
        let mut allocated = 0usize;
        for i in 0..count {
            let p = 20 + i * 16;
            let key_off = le16(bytes, p)? as usize;
            let key_bytes = bytes.get(keys + key_off..data).ok_or_else(|| corrupt("SFO key offset out of bounds"))?;
            if key_off != 0 && bytes[keys + key_off - 1] != 0 {
                return Err(corrupt("SFO key offset is not at a key boundary"));
            }
            let end = key_bytes.iter().position(|&b| b == 0).ok_or_else(|| corrupt("unterminated SFO key"))?;
            let key = std::str::from_utf8(&key_bytes[..end]).map_err(|_| corrupt("invalid UTF-8 SFO key"))?.to_owned();
            if key.is_empty() || !names.insert(key.clone()) {
                return Err(corrupt("empty or duplicate SFO key"));
            }
            let format = le16(bytes, p + 2)?;
            let length = le32(bytes, p + 4)?;
            let max_length = le32(bytes, p + 8)?;
            let data_offset = le32(bytes, p + 12)?;
            if length > max_length {
                return Err(corrupt(format!("SFO {key}: length exceeds maximum")));
            }
            let off = data.checked_add(data_offset as usize).ok_or_else(|| corrupt("SFO data offset overflow"))?;
            range(bytes, off, max_length as usize)?;
            allocated = allocated.checked_add(max_length as usize).ok_or_else(|| corrupt("SFO allocation overflow"))?;
            if allocated > bytes.len() - data {
                return Err(corrupt("SFO allocations exceed data table"));
            }
            if max_length != 0 {
                allocations.push((off, off + max_length as usize));
            }
            let used = range(bytes, off, length as usize)?;
            let value = match (format, length) {
                (_, 0) => SfoValue::Raw(Vec::new()),
                (0x0204, _) => {
                    // SAVEDATA_LIST_PARAM is game-defined; preserve binary instead of guessing.
                    let text = used.strip_suffix(&[0]);
                    match text.and_then(|s| if s.contains(&0) { None } else { std::str::from_utf8(s).ok() }) {
                        Some(text) => SfoValue::Text(text.to_owned()),
                        None if key == "SAVEDATA_LIST_PARAM" => SfoValue::Raw(used.to_vec()),
                        None => return Err(corrupt(format!("SFO {key}: invalid or unterminated UTF-8 text"))),
                    }
                }
                (0x0404, 4) => SfoValue::Integer(le32(used, 0)?),
                (0x0404, _) => return Err(corrupt(format!("SFO {key}: int32 length is not four"))),
                _ => SfoValue::Raw(used.to_vec()),
            };
            entries.push(SfoEntry { key, format, length, max_length, data_offset, value });
        }
        allocations.sort_unstable();
        if allocations.windows(2).any(|w| w[0].1 > w[1].0) {
            return Err(corrupt("overlapping SFO data allocations"));
        }
        Ok(Self { version, key_table_offset, data_table_offset, entries })
    }
    /// Finds an entry by its exact key.
    #[must_use]
    pub fn entry(&self, key: &str) -> Option<&SfoEntry> {
        self.entries.iter().find(|e| e.key == key)
    }
    /// Text value, or None for absent/binary/other-format keys.
    #[must_use]
    pub fn text(&self, key: &str) -> Option<&str> {
        match &self.entry(key)?.value {
            SfoValue::Text(s) => Some(s),
            _ => None,
        }
    }
    /// Int32 bits, or None for absent/other-format keys.
    #[must_use]
    pub fn integer(&self, key: &str) -> Option<u32> {
        match self.entry(key)?.value {
            SfoValue::Integer(n) => Some(n),
            _ => None,
        }
    }
    /// TITLE: the XMB title. \[I\] PARAM.SFO §TITLE.
    #[must_use]
    pub fn title(&self) -> Option<&str> {
        self.text("TITLE")
    }
    /// TITLE_ID: the content identifier. \[I\] PARAM.SFO §TITLE_ID.
    #[must_use]
    pub fn title_id(&self) -> Option<&str> {
        self.text("TITLE_ID")
    }
    /// CATEGORY: content kind; GD does not distinguish patches from other game data.
    #[must_use]
    pub fn category(&self) -> Option<Category> {
        self.text("CATEGORY").map(Category::decode)
    }
    /// PS3_SYSTEM_VER: minimum system software; absent is unknown, invalid is an error.
    /// \[I\] PARAM.SFO §PS3_SYSTEM_VER.
    pub fn ps3_system_ver(&self) -> Result<Option<FirmwareVersion>> {
        match self.entry("PS3_SYSTEM_VER") {
            None => Ok(None),
            Some(_) => self
                .text("PS3_SYSTEM_VER")
                .ok_or_else(|| corrupt("PS3_SYSTEM_VER is not text"))
                .and_then(FirmwareVersion::parse)
                .map(Some),
        }
    }
    /// APP_VER: application/patch version. \[I\] PARAM.SFO §APP_VER.
    #[must_use]
    pub fn app_ver(&self) -> Option<&str> {
        self.text("APP_VER")
    }
    /// VERSION: content version. \[I\] PARAM.SFO §VERSION.
    #[must_use]
    pub fn version(&self) -> Option<&str> {
        self.text("VERSION")
    }
    /// ATTRIBUTE bits, without a category-dependent interpretation.
    #[must_use]
    pub fn attribute(&self) -> Option<u32> {
        self.integer("ATTRIBUTE")
    }
    /// Documented ATTRIBUTE flags, decoded according to CATEGORY; unknown bits are retained.
    /// \[I\] PARAM.SFO §ATTRIBUTE. Speculative flags are deliberately left unknown.
    #[must_use]
    pub fn attribute_flags(&self) -> Option<DecodedFlags> {
        let bits = self.attribute()?;
        let table: &[(u32, &str)] = match self.category() {
            Some(Category::SaveData) => &[(1, "copy protected")],
            Some(
                Category::DiscGame
                | Category::HddGame
                | Category::Ps1Classic
                | Category::Ps2Classic
                | Category::Minis
                | Category::PspRemaster,
            ) => &[
                (1, "PSP remote play"),
                (2, "PSP export"),
                (4, "PSP remote play v2"),
                (8, "in-game XMB forced"),
                (16, "in-game XMB disabled"),
                (32, "background music"),
                (128, "PS Vita remote play"),
                (256, "Move warning"),
                (512, "navigation controller warning"),
                (1024, "camera warning"),
                (2048, "Move calibration notification"),
                (4096, "3D warning"),
                (0x10000, "install disc"),
                (0x20000, "install packages"),
                (0x80000, "game purchase enabled"),
                (0x400000, "licence logo disabled"),
                (0x800000, "Move enabled"),
            ],
            _ => &[],
        };
        Some(DecodedFlags::decode(bits, table))
    }
    /// RESOLUTION bits. \[I\] PARAM.SFO §RESOLUTION.
    #[must_use]
    pub fn resolution(&self) -> Option<u32> {
        self.integer("RESOLUTION")
    }
    /// Named video capabilities; these do not prove the attached display supports them.
    #[must_use]
    pub fn resolution_flags(&self) -> Option<DecodedFlags> {
        self.resolution().map(|b| {
            DecodedFlags::decode(
                b,
                &[
                    (1, "480 (4:3)"),
                    (2, "576 (4:3)"),
                    (4, "720 (16:9)"),
                    (8, "1080 (16:9)"),
                    (16, "480 (16:9)"),
                    (32, "576 (16:9)"),
                ],
            )
        })
    }
    /// SOUND_FORMAT bits. \[I\] PARAM.SFO §SOUND_FORMAT.
    #[must_use]
    pub fn sound_format(&self) -> Option<u32> {
        self.integer("SOUND_FORMAT")
    }
    /// Named audio capabilities; unnamed bits (including bit 1) remain visible.
    #[must_use]
    pub fn sound_flags(&self) -> Option<DecodedFlags> {
        self.sound_format().map(|b| {
            DecodedFlags::decode(
                b,
                &[(1, "LPCM 2.0"), (4, "LPCM 5.1"), (16, "LPCM 7.1"), (256, "Dolby Digital 5.1"), (512, "DTS 5.1")],
            )
        })
    }
    /// PARENTAL_LEVEL: rating gate; console settings are not known. \[I\] PARAM.SFO §PARENTAL_LEVEL.
    #[must_use]
    pub fn parental_level(&self) -> Option<u32> {
        self.integer("PARENTAL_LEVEL")
    }
    /// BOOTABLE: stored mode (0 unbootable, 1 game, 2 software). \[I\] PARAM.SFO §BOOTABLE.
    #[must_use]
    pub fn bootable(&self) -> Option<u32> {
        self.integer("BOOTABLE")
    }
    /// LICENSE: licence text. \[I\] PARAM.SFO §LICENSE.
    #[must_use]
    pub fn license(&self) -> Option<&str> {
        self.text("LICENSE")
    }
    /// SAVEDATA_DIRECTORY: declared save folder. \[I\] PARAM.SFO §SAVEDATA_DIRECTORY.
    #[must_use]
    pub fn savedata_directory(&self) -> Option<&str> {
        self.text("SAVEDATA_DIRECTORY")
    }
    /// DETAIL: save description. \[I\] PARAM.SFO §DETAIL.
    #[must_use]
    pub fn detail(&self) -> Option<&str> {
        self.text("DETAIL")
    }
    /// SUB_TITLE: save subtitle. \[I\] PARAM.SFO §SUB_TITLE.
    #[must_use]
    pub fn sub_title(&self) -> Option<&str> {
        self.text("SUB_TITLE")
    }
    /// SAVEDATA_LIST_PARAM as text if text is present; game-defined binary stays in entry().
    #[must_use]
    pub fn savedata_list_param(&self) -> Option<&str> {
        self.text("SAVEDATA_LIST_PARAM")
    }
}

/// CATEGORY codes \[I\], PARAM.SFO §CATEGORY. Game data includes installs, patches and DLC.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Category {
    /// DG: disc game.
    DiscGame,
    /// HG: HDD game.
    HddGame,
    /// GD: game data (may be a patch, install, DLC or common data).
    GameData,
    /// SD: PS3 save data.
    SaveData,
    /// MS: Minis save data.
    MinisSave,
    /// 1P: PS1 Classic.
    Ps1Classic,
    /// 2P: PS2 Classic.
    Ps2Classic,
    /// 2G: installed PS2 disc game.
    Ps2Game,
    /// 2D: PS2 data.
    Ps2Data,
    /// MN: PSP Minis.
    Minis,
    /// PE: PSP remaster.
    PspRemaster,
    /// PP: PSP package.
    PspPackage,
    /// AP/AM/AV/AT: photo/music/video/TV application (code retained).
    Application(String),
    /// TR/VR/VI/XR/DP/IP/AR/TI/DM: disc content (code retained).
    DiscContent(String),
    /// Unrecognized code, retained without guessing.
    Unknown(String),
}
impl Category {
    /// Decodes a public category code, preserving unsupported ones.
    #[must_use]
    pub fn decode(code: &str) -> Self {
        match code {
            "DG" => Self::DiscGame,
            "HG" => Self::HddGame,
            "GD" => Self::GameData,
            "SD" => Self::SaveData,
            "MS" => Self::MinisSave,
            "1P" => Self::Ps1Classic,
            "2P" => Self::Ps2Classic,
            "2G" => Self::Ps2Game,
            "2D" => Self::Ps2Data,
            "MN" => Self::Minis,
            "PE" => Self::PspRemaster,
            "PP" => Self::PspPackage,
            "AP" | "AM" | "AV" | "AT" => Self::Application(code.into()),
            "TR" | "VR" | "VI" | "XR" | "DP" | "IP" | "AR" | "TI" | "DM" => Self::DiscContent(code.into()),
            _ => Self::Unknown(code.into()),
        }
    }
}
impl fmt::Display for Category {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::DiscGame => "disc game (DG)",
            Self::HddGame => "HDD game (HG)",
            Self::GameData => "game data: install, patch or DLC (GD)",
            Self::SaveData => "save data (SD)",
            Self::MinisSave => "Minis save (MS)",
            Self::Ps1Classic => "PS1 Classic (1P)",
            Self::Ps2Classic => "PS2 Classic (2P)",
            Self::Ps2Game => "PS2 game (2G)",
            Self::Ps2Data => "PS2 data (2D)",
            Self::Minis => "Minis (MN)",
            Self::PspRemaster => "PSP remaster (PE)",
            Self::PspPackage => "PSP package (PP)",
            Self::Application(s) => return write!(f, "application ({s})"),
            Self::DiscContent(s) => return write!(f, "disc content ({s})"),
            Self::Unknown(s) => return write!(f, "unknown category ({s})"),
        })
    }
}

/// A bit mask with documented labels and the remaining bits preserved.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct DecodedFlags {
    /// Original mask.
    pub bits: u32,
    /// Documented enabled capabilities.
    pub labels: Vec<&'static str>,
    /// Bits whose interpretation is unknown or category-inapplicable.
    pub unknown_bits: u32,
}
impl DecodedFlags {
    fn decode(bits: u32, table: &[(u32, &'static str)]) -> Self {
        let labels = table.iter().filter(|(mask, _)| bits & mask != 0).map(|(_, label)| *label).collect();
        let known = table.iter().fold(0, |n, (mask, _)| n | mask);
        Self { bits, labels, unknown_bits: bits & !known }
    }
}
impl fmt::Display for DecodedFlags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{:08x}", self.bits)?;
        if !self.labels.is_empty() {
            write!(f, " ({})", self.labels.join(", "))?;
        }
        if self.unknown_bits != 0 {
            write!(f, "; unknown bits 0x{:08x}", self.unknown_bits)?;
        }
        Ok(())
    }
}

/// One PS3_DISC.SFB key/value record; values are retained as raw bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct SfbEntry {
    /// Fixed 16-byte key, without NUL padding.
    pub key: String,
    /// Absolute value offset.
    pub offset: u32,
    /// Value bytes, including padding.
    pub value: Vec<u8>,
}
/// PS3 disc descriptor. \[I\] PS3_DISC.SFB §Internal structure.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Sfb {
    /// Big-endian version word (typically 0x10000).
    pub version: u32,
    /// Key/value records in table order.
    pub entries: Vec<SfbEntry>,
}
impl Sfb {
    /// Parses the 0x20-byte prefix and 0x20-byte entries up to padding or the first value.
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if range(bytes, 0, 4)? != b".SFB" {
            return Err(corrupt("invalid PS3_DISC.SFB magic"));
        }
        range(bytes, 0, 0x20)?;
        let version = be32(bytes, 4)?;
        let mut p = 0x20;
        let mut first_value = bytes.len();
        let mut entries = Vec::new();
        let mut names = HashSet::new();
        let mut allocations = Vec::new();
        let mut allocated = 0usize;
        while p < first_value {
            let record = range(bytes, p, 0x20)?;
            if record[..16].iter().all(|&b| b == 0) {
                break;
            }
            let key = ctext(&record[..16]).ok_or_else(|| corrupt("invalid SFB key text"))?.to_owned();
            if key.is_empty() || !names.insert(key.clone()) {
                return Err(corrupt("empty or duplicate SFB key"));
            }
            let offset = be32(record, 16)?;
            let length = be32(record, 20)? as usize;
            let start = offset as usize;
            if start < p + 0x20 {
                return Err(corrupt("SFB value overlaps index table"));
            }
            range(bytes, start, length)?;
            allocated = allocated.checked_add(length).ok_or_else(|| corrupt("SFB allocation overflow"))?;
            if allocated > bytes.len() {
                return Err(corrupt("SFB allocations exceed input size"));
            }
            let value = range(bytes, start, length)?.to_vec();
            first_value = first_value.min(start);
            if length != 0 {
                allocations.push((start, start + length));
            }
            entries.push(SfbEntry { key, offset, value });
            p += 0x20;
        }
        if entries.is_empty() || p > first_value {
            return Err(corrupt("empty or overlapping SFB table"));
        }
        allocations.sort_unstable();
        if allocations.windows(2).any(|w| w[0].1 > w[1].0) {
            return Err(corrupt("overlapping SFB values"));
        }
        Ok(Self { version, entries })
    }
    /// Text value up to its first NUL; non-text values remain in entries.
    #[must_use]
    pub fn text(&self, key: &str) -> Option<&str> {
        ctext(&self.entries.iter().find(|e| e.key == key)?.value)
    }
    /// TITLE_ID in the descriptor (usually hyphenated).
    #[must_use]
    pub fn title_id(&self) -> Option<&str> {
        self.text("TITLE_ID")
    }
    /// HYBRID_FLAG's characters; `g` enables game content and `u` advertises an update \[I\].
    #[must_use]
    pub fn hybrid_flags(&self) -> Option<&str> {
        self.text("HYBRID_FLAG")
    }
}

/// Inclusive sector interval declared plaintext by the disc's region table \[I\].
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct PlainRegion {
    /// First plaintext logical sector.
    pub first: u32,
    /// Last plaintext logical sector (inclusive).
    pub last: u32,
}
/// Sector-zero region table \[I\], PS3 Developer wiki Bluray disc §Encryption.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Ps3RegionMap {
    /// Plaintext ranges in ascending order; gaps are encrypted.
    pub plaintext: Vec<PlainRegion>,
}
impl Ps3RegionMap {
    /// Parses a full 2048-byte sector; rejects empty, overlapping or reversed ranges.
    pub fn parse(sector: &[u8]) -> Result<Self> {
        range(sector, 0, 2048)?;
        let count = be32(sector, 0)? as usize;
        if count == 0 || count > (2048 - 8) / 8 {
            return Err(corrupt("invalid PS3 region count"));
        }
        let mut plaintext: Vec<PlainRegion> = Vec::with_capacity(count);
        for i in 0..count {
            let first = be32(sector, 8 + i * 8)?;
            let last = be32(sector, 12 + i * 8)?;
            if first > last || (i == 0 && first != 0) || plaintext.last().is_some_and(|prev| prev.last >= first) {
                return Err(corrupt("invalid PS3 plaintext region order"));
            }
            plaintext.push(PlainRegion { first, last });
        }
        Ok(Self { plaintext })
    }
    /// Whether every sector touched by this byte extent is declared plaintext.
    #[must_use]
    pub fn is_plaintext(&self, lba: u64, size: u64) -> bool {
        let Some(end) = lba.checked_add(size.div_ceil(2048)) else {
            return false;
        };
        let mut next = lba;
        for r in &self.plaintext {
            if u64::from(r.last) < next {
                continue;
            }
            if u64::from(r.first) > next {
                return false;
            }
            next = u64::from(r.last) + 1;
            if next >= end {
                return true;
            }
        }
        size == 0
    }
}

/// Whether file bytes can be read, independent of encryption inside a SELF/PUP.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum FileReadability {
    /// Folder bytes or an extent fully declared plaintext.
    Readable,
    /// At least one sector is in an encrypted gap; no file bytes were read.
    EncryptedRegion,
    /// Image has no region table; no file bytes were read.
    UnknownRegionMap,
    /// No such file in the supplied folder/index.
    Missing,
    /// Read/metadata parsing failed (diagnostic retained).
    Unreadable(String),
}
impl fmt::Display for FileReadability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Readable => f.write_str("readable"),
            Self::EncryptedRegion => f.write_str("encrypted region — needs the disc key, not read"),
            Self::UnknownRegionMap => f.write_str("region table absent — readability unknown, not read"),
            Self::Missing => f.write_str("missing"),
            Self::Unreadable(s) => write!(f, "unreadable: {s}"),
        }
    }
}
/// File identity and readability, without content or a claim of authenticity.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Ps3File {
    /// Normalized relative path (ISO version suffix removed).
    pub path: String,
    /// File size, absent for a missing file.
    pub size: Option<u64>,
    /// First LBA for an ISO, absent for a folder/missing file.
    pub lba: Option<u64>,
    /// Why it can or cannot be read.
    pub readability: FileReadability,
}

const SFB: &str = "PS3_DISC.SFB";
const SFO: &str = "PS3_GAME/PARAM.SFO";
const UPDATE: &str = "PS3_UPDATE/PS3UPDAT.PUP";
const EBOOT: &str = "PS3_GAME/USRDIR/EBOOT.BIN";
const CARD_FILES: &[&str] =
    &[SFB, SFO, "PS3_GAME/ICON0.PNG", "PS3_GAME/PIC0.PNG", "PS3_GAME/PIC1.PNG", "PS3_GAME/SND0.AT3", UPDATE, EBOOT];
const MAX_METADATA: u64 = 1 << 20;
const MAX_FILE_READ: u64 = 512 << 20;
fn normalized(path: &str) -> String {
    path.trim_start_matches('/')
        .split('/')
        .map(|p| p.split(';').next().unwrap_or(p).to_ascii_uppercase())
        .collect::<Vec<_>>()
        .join("/")
}

#[derive(Debug)]
enum DiscSource {
    Folder,
    Iso(PathBuf),
}

/// PS3 disc metadata and readable-file inventory; payloads are not decrypted or executed.
#[derive(Debug)]
#[non_exhaustive]
pub struct Ps3Disc {
    /// Disc descriptor if readable and well-formed.
    pub sfb: Option<Sfb>,
    /// Game metadata if readable and well-formed.
    pub sfo: Option<Sfo>,
    /// All indexed files, plus missing expected card files.
    pub files: Vec<Ps3File>,
    /// ISO plaintext map; absent for folders or images without a map.
    pub regions: Option<Ps3RegionMap>,
    /// Readable disc update's version, via the existing PUP parser.
    pub update_version: Option<FirmwareVersion>,
    /// Only the EBOOT's plaintext 0x20-byte SCE prefix, via pup's shared helper.
    pub eboot_header: Option<SceHeader>,
    source: DiscSource,
    folder_paths: Vec<(String, PathBuf)>,
}
impl Ps3Disc {
    /// Opens a disc folder or ISO 9660 image; malformed optional metadata is reported in files.
    /// ISO directories are checked before DiscFiles indexes them; encrypted payloads are refused.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let (source, mut files, regions, folder_paths) = if path.is_dir() {
            let paths = folder_files(path)?;
            let files = paths
                .iter()
                .map(|(name, p)| {
                    Ok(Ps3File {
                        path: normalized(name),
                        size: Some(std::fs::metadata(p)?.len()),
                        lba: None,
                        readability: FileReadability::Readable,
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            (DiscSource::Folder, files, None, paths)
        } else {
            let image_size = std::fs::metadata(path)?.len();
            if !image_size.is_multiple_of(2048) {
                return Err(corrupt("PS3 ISO size is not a multiple of 2048"));
            }
            let mut reader = SectorReader::open(path)?;
            if reader.is_raw() {
                return Err(corrupt("PS3 requires a 2048-byte-sector ISO"));
            }
            let sector0 = reader.read(0, 1)?;
            range(&sector0, 0, 2048)?;
            let regions = if sector0.iter().all(|&b| b == 0) { None } else { Some(Ps3RegionMap::parse(&sector0)?) };
            if regions.as_ref().is_some_and(|r| r.plaintext.iter().any(|p| u64::from(p.last) >= image_size / 2048)) {
                return Err(corrupt("PS3 region table exceeds image size"));
            }
            check_iso_directories(&mut reader, image_size, regions.as_ref())?;
            let index = DiscFiles::open(path)?;
            let files = index
                .list()
                .iter()
                .filter(|e| !e.is_directory)
                .map(|e| Ps3File {
                    path: normalized(&e.path),
                    size: Some(e.size as u64),
                    lba: Some(e.lba as u64),
                    readability: match &regions {
                        Some(r) if r.is_plaintext(e.lba as u64, e.size as u64) => FileReadability::Readable,
                        Some(_) => FileReadability::EncryptedRegion,
                        None => FileReadability::UnknownRegionMap,
                    },
                })
                .collect();
            (DiscSource::Iso(path.to_owned()), files, regions, Vec::new())
        };
        for &name in CARD_FILES {
            if !files.iter().any(|f| f.path == name) {
                files.push(Ps3File { path: name.into(), size: None, lba: None, readability: FileReadability::Missing });
            }
        }
        files.sort_by(|a, b| a.path.cmp(&b.path));
        if files.windows(2).any(|w| w[0].path == w[1].path) {
            return Err(corrupt("duplicate normalized PS3 file path"));
        }
        if files
            .iter()
            .filter(|f| [SFB, SFO].contains(&f.path.as_str()))
            .all(|f| f.readability == FileReadability::Missing)
        {
            return Err(Error::NotFound(Format::DiscImage, "PS3_DISC.SFB / PS3_GAME/PARAM.SFO".into()));
        }
        let mut disc = Self {
            source,
            files,
            regions,
            folder_paths,
            sfb: None,
            sfo: None,
            update_version: None,
            eboot_header: None,
        };
        disc.sfb = disc.inspect(SFB, |d| Sfb::parse(&d.read_limited(SFB, MAX_METADATA)?));
        disc.sfo = disc.inspect(SFO, |d| {
            let sfo = Sfo::parse(&d.read_limited(SFO, MAX_METADATA)?)?;
            sfo.ps3_system_ver()?;
            Ok(sfo)
        });
        disc.update_version = disc.inspect(UPDATE, |d| {
            let pup = match &d.source {
                DiscSource::Folder => Pup::open(d.folder_path(UPDATE)?)?,
                DiscSource::Iso(_) => Pup::parse(&d.read_limited(UPDATE, MAX_FILE_READ)?)?,
            };
            FirmwareVersion::parse(&pup.version()?)
        });
        disc.eboot_header = disc.inspect(EBOOT, |d| {
            let h = parse_sce_header(&d.read_prefix(EBOOT, 32)?)?;
            if h.header_type != 1
                || h.header_length < 32
                || h.header_length > d.file(EBOOT).and_then(|f| f.size).unwrap_or(0)
            {
                return Err(corrupt("EBOOT is not a bounded SELF container"));
            }
            Ok(h)
        });
        Ok(disc)
    }
    fn inspect<T>(&mut self, path: &str, parse: impl FnOnce(&Self) -> Result<T>) -> Option<T> {
        if self.file(path)?.readability != FileReadability::Readable {
            return None;
        }
        match parse(self) {
            Ok(value) => Some(value),
            Err(e) => {
                if let Some(file) = self.files.iter_mut().find(|f| f.path == path) {
                    file.readability = FileReadability::Unreadable(e.to_string());
                }
                None
            }
        }
    }
    /// Finds a normalized file record, including an expected-but-missing file.
    #[must_use]
    pub fn file(&self, path: &str) -> Option<&Ps3File> {
        let path = normalized(path);
        self.files.iter().find(|f| f.path == path)
    }
    fn folder_path(&self, path: &str) -> Result<&Path> {
        self.folder_paths
            .iter()
            .find(|(name, _)| normalized(name) == normalized(path))
            .map(|(_, p)| p.as_path())
            .ok_or_else(|| Error::NotFound(Format::DiscImage, path.into()))
    }
    fn readable(&self, path: &str) -> Result<&Ps3File> {
        let file = self.file(path).ok_or_else(|| Error::NotFound(Format::DiscImage, path.into()))?;
        if file.readability != FileReadability::Readable {
            return Err(corrupt(format!("{path}: {}", file.readability)));
        }
        Ok(file)
    }
    /// Reads a file only if its entire extent is plaintext; limits allocation to 512 MiB.
    /// A readable SELF/PUP still contains encrypted payloads, which are never interpreted.
    pub fn read_file(&self, path: &str) -> Result<Vec<u8>> {
        self.read_limited(path, MAX_FILE_READ)
    }
    fn read_limited(&self, path: &str, limit: u64) -> Result<Vec<u8>> {
        let size = self.readable(path)?.size.ok_or_else(|| corrupt("missing file size"))?;
        if size > limit {
            return Err(corrupt(format!("{path}: exceeds {limit}-byte inspection limit")));
        }
        self.read_prefix(path, size)
    }
    fn read_prefix(&self, path: &str, length: u64) -> Result<Vec<u8>> {
        let file = self.readable(path)?;
        let size = file.size.ok_or_else(|| corrupt("missing file size"))?;
        if length > size {
            return Err(corrupt(format!("{path}: truncated")));
        }
        let data = match &self.source {
            DiscSource::Folder => {
                let mut data = Vec::new();
                File::open(self.folder_path(path)?)?.take(length).read_to_end(&mut data)?;
                data
            }
            DiscSource::Iso(p) => {
                let mut reader = SectorReader::open(p)?;
                let lba = usize::try_from(file.lba.ok_or_else(|| corrupt("missing ISO LBA"))?)
                    .map_err(|_| corrupt("ISO LBA overflow"))?;
                let mut data = reader.read(lba, length.div_ceil(2048) as usize)?;
                data.truncate(length as usize);
                data
            }
        };
        if data.len() as u64 != length {
            return Err(corrupt(format!("{path}: short file read")));
        }
        Ok(data)
    }
    /// Ordered metadata hand-off, with a firmware gate and explicit authentication/code limits.
    /// \[I\] PARAM.SFO named-key sections; plaintext scope: `ps3_pup.md` §6.
    #[must_use]
    pub fn handoff(&self, console_firmware: Option<&str>) -> Vec<Ps3HandoffStep> {
        let sfo = self.sfo.as_ref();
        let console = console_firmware.and_then(|s| FirmwareVersion::parse(s).ok());
        let required = sfo.and_then(|s| s.ps3_system_ver().ok().flatten());
        let verdict = FirmwareVerdict::compare(console, required);
        let state = |p: &str| self.file(p).map(|f| f.readability.clone()).unwrap_or(FileReadability::Missing);
        vec![
            Ps3HandoffStep::DiscDetected {
                title_id: self.sfb.as_ref().and_then(|s| s.title_id()).map(str::to_owned),
                hybrid_flags: self.sfb.as_ref().and_then(|s| s.hybrid_flags()).map(str::to_owned),
                readability: state(SFB),
            },
            Ps3HandoffStep::Metadata {
                title: sfo.and_then(Sfo::title).map(str::to_owned),
                title_id: sfo.and_then(Sfo::title_id).map(str::to_owned),
                category: sfo.and_then(Sfo::category),
                app_ver: sfo.and_then(Sfo::app_ver).map(str::to_owned),
                bootable: sfo.and_then(Sfo::bootable),
                readability: state(SFO),
            },
            Ps3HandoffStep::Firmware { console, required, verdict },
            Ps3HandoffStep::Update { version: self.update_version, required, verdict, readability: state(UPDATE) },
            Ps3HandoffStep::Parental { level: sfo.and_then(Sfo::parental_level) },
            Ps3HandoffStep::Capabilities {
                resolution: sfo.and_then(Sfo::resolution_flags),
                sound: sfo.and_then(Sfo::sound_flags),
            },
            Ps3HandoffStep::Executable { header: self.eboot_header.clone(), readability: state(EBOOT) },
            Ps3HandoffStep::Unknown,
        ]
    }
}

// DiscFiles reads directories eagerly. Validate their structure and region coverage first,
// so encrypted directory bytes cannot reach that general-purpose ISO walker. File extents
// are checked for bounds/interleaving/multi-extent support without reading file contents.
fn check_iso_directories(reader: &mut SectorReader, image_size: u64, regions: Option<&Ps3RegionMap>) -> Result<()> {
    let guard = |lba: u64, size: u64| -> Result<()> {
        let end =
            lba.checked_mul(2048).and_then(|p| p.checked_add(size)).ok_or_else(|| corrupt("ISO extent overflow"))?;
        if end > image_size {
            return Err(corrupt("ISO extent exceeds image size"));
        }
        Ok(())
    };
    if regions.is_some_and(|r| !r.is_plaintext(16, 2048)) {
        return Err(corrupt("ISO volume descriptor is in an encrypted region, not read"));
    }
    let pvd = reader.read(16, 1)?;
    if pvd.len() != 2048 || pvd[0] != 1 || &pvd[1..6] != b"CD001" || pvd[6] != 1 {
        return Err(corrupt("no ISO 9660 primary volume descriptor; UDF-only PS3 images are unsupported"));
    }
    if le16(&pvd, 128)? != 2048 || u16::from_be_bytes([pvd[130], pvd[131]]) != 2048 {
        return Err(corrupt("unsupported ISO logical block size"));
    }
    let root = range(&pvd, 156, pvd[156] as usize)?;
    if root.len() < 34
        || root[1] != 0
        || root[25] != 2
        || root[26] != 0
        || root[27] != 0
        || root[32] != 1
        || root[33] != 0
        || le32(root, 2)? != be32(root, 6)?
        || le32(root, 10)? != be32(root, 14)?
    {
        return Err(corrupt("invalid ISO root directory record"));
    }
    let mut queue = VecDeque::from([(le32(root, 2)? as u64, le32(root, 10)? as u64)]);
    let mut seen = HashSet::new();
    while let Some((lba, size)) = queue.pop_front() {
        guard(lba, size)?;
        if !seen.insert(lba) {
            continue;
        }
        if seen.len() > 65536 || size == 0 || size > 256 * 2048 {
            return Err(corrupt("unsupported ISO directory size/count"));
        }
        if regions.is_some_and(|r| !r.is_plaintext(lba, size)) {
            return Err(corrupt("ISO directory is in an encrypted region, not read"));
        }
        let bytes = reader.read(lba as usize, size.div_ceil(2048) as usize)?;
        if bytes.len() < size as usize {
            return Err(corrupt("short ISO directory"));
        }
        let mut p = 0;
        while p < size as usize {
            let length = bytes[p] as usize;
            if length == 0 {
                p = (p / 2048 + 1) * 2048;
                continue;
            }
            if length < 34 || p + length > size as usize || p % 2048 + length > 2048 {
                return Err(corrupt("invalid ISO directory record length"));
            }
            let record = range(&bytes, p, length)?;
            if record[1] != 0 || record[26] != 0 || record[27] != 0 || record[25] & 0x80 != 0 {
                return Err(corrupt("ISO extended attributes, interleaving or multi-extent files are unsupported"));
            }
            if 33 + record[32] as usize > length {
                return Err(corrupt("ISO name exceeds record"));
            }
            let child_lba = le32(record, 2)? as u64;
            let child_size = le32(record, 10)? as u64;
            if be32(record, 6)? as u64 != child_lba || be32(record, 14)? as u64 != child_size {
                return Err(corrupt("inconsistent ISO both-endian extent"));
            }
            guard(child_lba, child_size)?;
            let name = &record[33..33 + record[32] as usize];
            if name.is_empty() || (name != [0] && name != [1] && (name.contains(&b'/') || name.contains(&b'\\'))) {
                return Err(corrupt("invalid ISO file name"));
            }
            if record[25] & 2 != 0 && name != [0] && name != [1] {
                queue.push_back((child_lba, child_size));
            }
            p += length;
        }
    }
    Ok(())
}

fn folder_files(root: &Path) -> Result<Vec<(String, PathBuf)>> {
    let mut queue = VecDeque::from([root.to_owned()]);
    let mut files = Vec::new();
    let mut directories = 0;
    while let Some(dir) = queue.pop_front() {
        directories += 1;
        if directories > 65536 {
            return Err(corrupt("too many PS3 directories"));
        }
        for item in std::fs::read_dir(&dir)? {
            let item = item?;
            let p = item.path();
            let kind = item.file_type()?;
            if kind.is_dir() {
                queue.push_back(p);
            } else if kind.is_file() || (kind.is_symlink() && p.is_file()) {
                let name = p
                    .strip_prefix(root)
                    .map_err(|_| corrupt("file outside PS3 folder"))?
                    .to_str()
                    .ok_or_else(|| corrupt("non-UTF-8 PS3 filename"))?
                    .to_owned();
                files.push((name, p));
                if files.len() > 100000 {
                    return Err(corrupt("too many PS3 files"));
                }
            }
        }
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(files)
}

/// Firmware gate outcome only; this never asserts that drive authentication succeeded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum FirmwareVerdict {
    /// Console version is at least the declared minimum.
    Satisfied,
    /// Console version is older than the declared minimum.
    UpdateRequired,
    /// Either version is absent/unrecognized.
    Unknown,
}
impl FirmwareVerdict {
    /// Compares versions; missing information stays unknown.
    #[must_use]
    pub fn compare(console: Option<FirmwareVersion>, required: Option<FirmwareVersion>) -> Self {
        match (console, required) {
            (Some(c), Some(r)) if c >= r => Self::Satisfied,
            (Some(_), Some(_)) => Self::UpdateRequired,
            _ => Self::Unknown,
        }
    }
}

/// Ordered, source-cited facts for a PS3 disc hand-off. Display renders a complete sentence.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Ps3HandoffStep {
    /// Disc descriptor and hybrid content advertisement.
    DiscDetected {
        /// Descriptor title ID.
        title_id: Option<String>,
        /// Stored hybrid characters, including unknown characters.
        hybrid_flags: Option<String>,
        /// Descriptor readability.
        readability: FileReadability,
    },
    /// Game's XMB identity and bootable mode.
    Metadata {
        /// TITLE.
        title: Option<String>,
        /// TITLE_ID.
        title_id: Option<String>,
        /// Decoded CATEGORY.
        category: Option<Category>,
        /// APP_VER.
        app_ver: Option<String>,
        /// BOOTABLE mode.
        bootable: Option<u32>,
        /// Metadata readability.
        readability: FileReadability,
    },
    /// Comparison against the declared firmware minimum.
    Firmware {
        /// User-supplied console version (possibly from their PUP).
        console: Option<FirmwareVersion>,
        /// PS3_SYSTEM_VER.
        required: Option<FirmwareVersion>,
        /// Gate result.
        verdict: FirmwareVerdict,
    },
    /// Disc-carried firmware update and whether it can satisfy the minimum.
    Update {
        /// Readable PUP's version.txt.
        version: Option<FirmwareVersion>,
        /// Declared firmware minimum.
        required: Option<FirmwareVersion>,
        /// Console's firmware gate result.
        verdict: FirmwareVerdict,
        /// PUP readability.
        readability: FileReadability,
    },
    /// Parental-control requirement (console settings unknown).
    Parental {
        /// PARENTAL_LEVEL.
        level: Option<u32>,
    },
    /// Declared output capabilities (display/audio setup unknown).
    Capabilities {
        /// RESOLUTION flags.
        resolution: Option<DecodedFlags>,
        /// SOUND_FORMAT flags.
        sound: Option<DecodedFlags>,
    },
    /// Plaintext SCE prefix and the encrypted-code boundary.
    Executable {
        /// Plaintext 0x20-byte SCE header if readable and valid.
        header: Option<SceHeader>,
        /// EBOOT readability.
        readability: FileReadability,
    },
    /// Drive authentication, installed console state and the game's code remain unknown.
    Unknown,
}
impl Ps3HandoffStep {
    /// Evidence/provenance citation; console behavior is inferred, not emulated.
    #[must_use]
    pub fn source(&self) -> &'static str {
        match self {
            Self::DiscDetected { .. } => "PS3_DISC.SFB; PS3 Developer wiki PS3_DISC.SFB §Hybrid Flags [I]",
            Self::Metadata { .. } => "PS3_GAME/PARAM.SFO; PS3 Developer wiki PARAM.SFO §CATEGORY/BOOTABLE [I]",
            Self::Firmware { .. } => {
                "PARAM.SFO PS3_SYSTEM_VER + user-supplied console version; PARAM.SFO §PS3_SYSTEM_VER [I]"
            }
            Self::Update { .. } => {
                "PS3_UPDATE/PS3UPDAT.PUP version.txt; notes/research/ps3_pup.md §1–2; update offer [I]"
            }
            Self::Parental { .. } => "PARAM.SFO PARENTAL_LEVEL; PARAM.SFO §PARENTAL_LEVEL [I]",
            Self::Capabilities { .. } => "PARAM.SFO RESOLUTION/SOUND_FORMAT; PARAM.SFO §RESOLUTION/§SOUND_FORMAT [I]",
            Self::Executable { .. } => "PS3_GAME/USRDIR/EBOOT.BIN SCE prefix; notes/research/ps3_pup.md §3.2/§3.5",
            Self::Unknown => "notes/research/ps3_pup.md §6; disc authentication/game execution limits [I]",
        }
    }
}
fn shown<T: fmt::Display>(value: &Option<T>) -> String {
    value.as_ref().map(ToString::to_string).unwrap_or_else(|| "unknown".into())
}
impl fmt::Display for Ps3HandoffStep {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DiscDetected { title_id, hybrid_flags, readability } => {
                write!(f, "Disc detected: SFB {readability}; ID {}; hybrid flags {}", shown(title_id), shown(hybrid_flags))?;
                if let Some(flags) = hybrid_flags { write!(f, " (game {}, update {})", flags.contains('g'), flags.contains('u'))?; }
                f.write_str(".")?;
            }
            Self::Metadata { title, title_id, category, app_ver, bootable, readability } => write!(f, "PARAM.SFO {readability}: title {}, ID {}, category {}, app version {}, BOOTABLE {}.", shown(title), shown(title_id), shown(category), shown(app_ver), shown(bootable))?,
            Self::Firmware { console, required, verdict } => {
                write!(f, "Console firmware {}; disc requires {}: ", shown(console), shown(required))?;
                f.write_str(match verdict { FirmwareVerdict::Satisfied => "firmware gate permits boot.", FirmwareVerdict::UpdateRequired => "update required before boot.", FirmwareVerdict::Unknown => "firmware gate is unknown." })?;
            }
            Self::Update { version, required, verdict, readability } => {
                write!(f, "Disc update {readability}; firmware {}", shown(version))?;
                if *verdict == FirmwareVerdict::UpdateRequired {
                    match (version, required) {
                        (Some(v), Some(r)) if v >= r => f.write_str("; the console would offer this update before boot")?,
                        (Some(_), Some(_)) => f.write_str("; this update is below the game's requirement")?,
                        _ => f.write_str("; the needed update cannot be identified from readable disc metadata")?,
                    }
                }
                f.write_str(".")?;
            }
            Self::Parental { level } => write!(f, "Parental level {}; the console's parental settings are unknown.", shown(level))?,
            Self::Capabilities { resolution, sound } => write!(f, "Declared video {}; sound {}; the console's display/audio configuration is unknown.", shown(resolution), shown(sound))?,
            Self::Executable { header, readability } => {
                write!(f, "EBOOT.BIN {readability}")?;
                if let Some(h) = header { write!(f, ": SELF, SCE version {}, key revision 0x{:04x}, type {}, metadata offset 0x{:x}, header {} bytes, payload {} bytes", h.version, h.key_revision, h.header_type, h.metadata_offset, h.header_length, h.data_length)?; }
                f.write_str(". A retail SELF's code is encrypted; only its plaintext SCE prefix is inspected.")?;
            }
            Self::Unknown => f.write_str("What we cannot know: BD-drive disc authentication, installed console state, SELF authentication or the game's own code. Passing the firmware gate does not prove a successful boot; no decryption is performed.")?,
        }
        write!(f, " [{}]", self.source())
    }
}

/// Only the documented plaintext PARAM.PFD magic/version; encrypted header tables stay opaque.
/// \[I\] PS3 Developer wiki PARAM.PFD §PFD Header; scope: `ps3_pup.md` §6.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct PfdHeader {
    /// Stored magic (`\0\0\0\0PFDB`).
    pub magic: [u8; 8],
    /// Big-endian version word; unknown versions are retained.
    pub version: u64,
}
impl PfdHeader {
    /// Parses the plaintext first 16 bytes; a full 0x60-byte outer header must be present.
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        range(bytes, 0, 0x60)?;
        if &bytes[..8] != b"\0\0\0\0PFDB" {
            return Err(corrupt("invalid PARAM.PFD magic"));
        }
        Ok(Self { magic: std::array::from_fn(|i| bytes[i]), version: be64(bytes, 8)? })
    }
}
/// One save data file, listed by name/size only.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct SaveDataFile {
    /// Relative filename, including subdirectory if any.
    pub name: String,
    /// File length; data is never read.
    pub size: u64,
}
/// PS3 save's metadata, icon presence/size and plaintext PFD prefix; no integrity claims.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Ps3Save {
    /// PARAM.SFO metadata.
    pub sfo: Sfo,
    /// ICON0.PNG size if present (its content is not decoded).
    pub icon_size: Option<u64>,
    /// PARAM.PFD size if present.
    pub pfd_size: Option<u64>,
    /// Plaintext magic/version if present; encrypted table bytes are not interpreted.
    pub pfd: Option<PfdHeader>,
    /// All other regular files, by name and size only.
    pub data_files: Vec<SaveDataFile>,
}
impl Ps3Save {
    /// Opens a save directory; malformed SFO/PFD is an error, absent icon/PFD is allowed.
    pub fn open(dir: impl AsRef<Path>) -> Result<Self> {
        let paths = folder_files(dir.as_ref())?;
        let mut names = HashSet::new();
        if paths.iter().any(|(n, _)| !names.insert(normalized(n))) {
            return Err(corrupt("duplicate normalized save file path"));
        }
        let path = |key: &str| paths.iter().find(|(n, _)| normalized(n) == key).map(|(_, p)| p);
        let sfo_path = path("PARAM.SFO").ok_or_else(|| Error::NotFound(Format::MemoryCard, "PARAM.SFO".into()))?;
        if std::fs::metadata(sfo_path)?.len() > MAX_METADATA {
            return Err(corrupt("save SFO exceeds inspection limit"));
        }
        let sfo = Sfo::parse(&std::fs::read(sfo_path)?)?;
        let icon_size = path("ICON0.PNG").map(std::fs::metadata).transpose()?.map(|m| m.len());
        let pfd_size = path("PARAM.PFD").map(std::fs::metadata).transpose()?.map(|m| m.len());
        let pfd = if let Some(p) = path("PARAM.PFD") {
            let mut header = Vec::new();
            File::open(p)?.take(0x60).read_to_end(&mut header)?;
            Some(PfdHeader::parse(&header)?)
        } else {
            None
        };
        let data_files = paths
            .iter()
            .filter(|(n, _)| !["PARAM.SFO", "PARAM.PFD", "ICON0.PNG"].contains(&normalized(n).as_str()))
            .map(|(n, p)| Ok(SaveDataFile { name: n.clone(), size: std::fs::metadata(p)?.len() }))
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { sfo, icon_size, pfd_size, pfd, data_files })
    }
}
