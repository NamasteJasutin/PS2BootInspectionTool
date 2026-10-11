//! PlayStation 1 memory card reader, save directory with block chains, icon decoders,
//! shell L1+L2+R1+R2 undelete candidates, and SoundScope performance decoding.
//!
//! # Format Specifications
//! - Public PS1 memory card layout: Martin Korth (nocash), *PSX-SPX / Memory Card Data Format* \[I\]:
//!   128 KiB total (16 blocks of 8 KiB = 8192 bytes, 1024 frames of 128 bytes).
//!   Block 0 is the directory block:
//!   - Frame 0: Header frame (`MC` magic, XOR checksum at byte 127).
//!   - Frames 1..=15: Directory entries for data blocks 1..=15 (allocation state, file size,
//!     next block link, 20-byte ASCII filename, XOR checksum at byte 127).
//!   - Frames 16..=35: Broken sector list (32-bit sector index or 0xFFFFFFFF, XOR checksum).
//!   - Save block 0: Title frame (`SC` magic, icon display flag, Shift-JIS title, 16-color CLUT),
//!     followed by 1..=3 icon frames of 16×16 4-bit indexed pixels.
//! - DexDrive `.gme` format: 3904-byte header beginning with `"123-456-STD\0"` before the raw image.
//! - Memory card undelete: `notes/research/hidden_input.md` §6 \[V\]:
//!   PS1 retail shells restore the remembered last-deleted filename via BIOS B0 function 0x46
//!   upon receiving the four-shoulder chord L1+L2+R1+R2. ROM `0xBFC0AC78` verifies deleted states
//!   `0xA1`/`0xA2`/`0xA3`, checks for name conflicts and block availability, and restores states
//!   `0x51`/`0x52`/`0x53`.
//! - SoundScope performance recordings (`BPLAYSTATION`): `notes/research/hidden_input.md` §8–§9 \[V\]:
//!   Visualiser performances saved by the CD player at 0x1E80 payload length with SC header,
//!   icon/palette, 0x00010000 version constant, 900 8-byte event records (`frame: u32`, `duration: u16`,
//!   `buttons: u16`), and wrapping 32-bit checksum.

use crate::{Error, Format, Region, Result};
use std::fmt;

/// Total size in bytes of a raw PlayStation 1 memory card (128 KiB).
pub const PS1_CARD_SIZE: usize = 131_072;

/// Size in bytes of a single memory card block (8 KiB).
pub const PS1_BLOCK_SIZE: usize = 8_192;

/// Size in bytes of a single frame / sector (128 bytes).
pub const PS1_FRAME_SIZE: usize = 128;

/// Total number of 8 KiB blocks on a PS1 memory card (1 directory block + 15 data blocks).
pub const PS1_TOTAL_BLOCKS: usize = 16;

/// Number of frames per block (64 frames * 128 bytes = 8192 bytes).
pub const PS1_FRAMES_PER_BLOCK: usize = 64;

/// Size of the InterAct DexDrive `.gme` header preceding the raw card image.
pub const GME_HEADER_SIZE: usize = 3_904;

/// DexDrive header magic string.
pub const GME_MAGIC: &[u8] = b"123-456-STD";

/// Header frame magic bytes (Block 0, Frame 0).
pub const HEADER_FRAME_MAGIC: &[u8] = b"MC";

/// Save title frame magic bytes (Block N, Frame 0).
pub const TITLE_FRAME_MAGIC: &[u8] = b"SC";

/// SoundScope performance filename on PS1 memory cards (`notes/research/hidden_input.md` §9.2).
pub const SOUNDSCOPE_FILENAME: &str = "BPLAYSTATION";

/// Maximum number of event slots in a SoundScope performance file (`notes/research/hidden_input.md` §9.2).
pub const SOUNDSCOPE_MAX_EVENTS: usize = 900;

/// Expected SoundScope format version (`0x00010000`, `notes/research/hidden_input.md` §9.2).
pub const SOUNDSCOPE_VERSION: u32 = 0x0001_0000;

/// Block allocation state byte in a directory frame (`psx-spx` Memory Card Data Format \[I\]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum BlockState {
    /// In use: first-or-only block of a file (`0x51`).
    InUseFirst,
    /// In use: middle block of a file with 3 or more blocks (`0x52`).
    InUseMiddle,
    /// In use: last block of a file with 2 or more blocks (`0x53`).
    InUseLast,
    /// Free: freshly formatted block (`0xA0`).
    FreeFormatted,
    /// Free: deleted first-or-only block of a file (`0xA1`).
    DeletedFirst,
    /// Free: deleted middle block of a file (`0xA2`).
    DeletedMiddle,
    /// Free: deleted last block of a file (`0xA3`).
    DeletedLast,
    /// Other unrecognised or corrupted state byte.
    Unknown(u8),
}

impl BlockState {
    /// Converts a raw state byte into [`BlockState`].
    #[must_use]
    pub fn from_byte(b: u8) -> Self {
        match b {
            0x51 => Self::InUseFirst,
            0x52 => Self::InUseMiddle,
            0x53 => Self::InUseLast,
            0xA0 => Self::FreeFormatted,
            0xA1 => Self::DeletedFirst,
            0xA2 => Self::DeletedMiddle,
            0xA3 => Self::DeletedLast,
            other => Self::Unknown(other),
        }
    }

    /// Returns the raw byte value of this block state.
    #[must_use]
    pub fn to_byte(self) -> u8 {
        match self {
            Self::InUseFirst => 0x51,
            Self::InUseMiddle => 0x52,
            Self::InUseLast => 0x53,
            Self::FreeFormatted => 0xA0,
            Self::DeletedFirst => 0xA1,
            Self::DeletedMiddle => 0xA2,
            Self::DeletedLast => 0xA3,
            Self::Unknown(b) => b,
        }
    }

    /// True if the block is active and in use (`0x51`, `0x52`, or `0x53`).
    #[must_use]
    pub fn is_in_use(self) -> bool {
        matches!(self, Self::InUseFirst | Self::InUseMiddle | Self::InUseLast)
    }

    /// True if the block is marked deleted (`0xA1`, `0xA2`, or `0xA3`).
    #[must_use]
    pub fn is_deleted(self) -> bool {
        matches!(self, Self::DeletedFirst | Self::DeletedMiddle | Self::DeletedLast)
    }
}

/// Computes the 1-byte XOR checksum over the first 127 bytes of a 128-byte frame.
#[must_use]
pub fn frame_checksum(frame: &[u8]) -> u8 {
    let limit = frame.len().min(127);
    frame[..limit].iter().fold(0u8, |acc, &b| acc ^ b)
}

/// Decodes up to 64 bytes of Shift-JIS encoded title text into a Rust [`String`].
///
/// Implements full-width ASCII range (`0x824F..=0x8258` digits, `0x8260..=0x8279` uppercase,
/// `0x8281..=0x829A` lowercase, `0x8140` space), Japanese punctuation (`0x81`),
/// Hiragana (`0x829F..=0x82F1`), Katakana (`0x8340..=0x8396`), half-width Katakana (`0xA1..=0xDF`),
/// and standard ASCII (`0x20..=0x7E`).
#[must_use]
pub fn decode_sjis(bytes: &[u8]) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i < bytes.len() {
        let b1 = bytes[i];
        if b1 == 0 {
            break;
        }
        if (0x20..=0x7E).contains(&b1) {
            out.push(b1 as char);
            i += 1;
            continue;
        }
        if (0xA1..=0xDF).contains(&b1) {
            // Half-width Katakana
            if let Some(c) = char::from_u32(0xFF61 + (b1 - 0xA1) as u32) {
                out.push(c);
            }
            i += 1;
            continue;
        }
        if i + 1 < bytes.len() {
            let b2 = bytes[i + 1];
            // 2-byte Shift-JIS sequence
            if b1 == 0x81 && b2 == 0x40 {
                out.push('\u{3000}'); // Ideographic full-width space '　'
                i += 2;
                continue;
            }
            if b1 == 0x82 && (0x4F..=0x58).contains(&b2) {
                // Full-width digits '０'..'９' (U+FF10..U+FF19)
                if let Some(c) = char::from_u32(0xFF10 + (b2 - 0x4F) as u32) {
                    out.push(c);
                }
                i += 2;
                continue;
            }
            if b1 == 0x82 && (0x60..=0x79).contains(&b2) {
                // Full-width uppercase 'Ａ'..'Ｚ' (U+FF21..U+FF3A)
                if let Some(c) = char::from_u32(0xFF21 + (b2 - 0x60) as u32) {
                    out.push(c);
                }
                i += 2;
                continue;
            }
            if b1 == 0x82 && (0x81..=0x9A).contains(&b2) {
                // Full-width lowercase 'ａ'..'ｚ' (U+FF41..U+FF5A)
                if let Some(c) = char::from_u32(0xFF41 + (b2 - 0x81) as u32) {
                    out.push(c);
                }
                i += 2;
                continue;
            }
            if b1 == 0x82 && (0x9F..=0xF1).contains(&b2) {
                // Hiragana: ぁ..ん (U+3041..U+3093)
                if let Some(c) = char::from_u32(0x3041 + (b2 - 0x9F) as u32) {
                    out.push(c);
                }
                i += 2;
                continue;
            }
            if b1 == 0x83 && (0x40..=0x96).contains(&b2) {
                // Katakana: ァ..ヶ (U+30A1..U+30F6), accounting for unused 0x7F
                let offset = if b2 < 0x7F { (b2 - 0x40) as u32 } else { (b2 - 0x41) as u32 };
                if let Some(c) = char::from_u32(0x30A1 + offset) {
                    out.push(c);
                }
                i += 2;
                continue;
            }
            if b1 == 0x81 {
                if let Some(c) = decode_sjis_punctuation(b2) {
                    out.push(c);
                    i += 2;
                    continue;
                }
            }
            // Other 2-byte Shift-JIS lead bytes
            if (0x81..=0x9F).contains(&b1) || (0xE0..=0xEF).contains(&b1) {
                out.push('\u{FFFD}');
                i += 2;
                continue;
            }
        }
        // Unrecognised single byte fallback
        out.push(b1 as char);
        i += 1;
    }
    out
}

fn decode_sjis_punctuation(b2: u8) -> Option<char> {
    Some(match b2 {
        0x41 => '、', 0x42 => '。', 0x43 => '，', 0x44 => '．', 0x46 => '：', 0x47 => '；',
        0x48 => '？', 0x49 => '！', 0x4A => '゛', 0x4B => '゜', 0x4C => '´', 0x4D => '｀',
        0x4E => '¨', 0x4F => '＾', 0x50 => '￣', 0x51 => '＿', 0x5B => 'ー', 0x5C => '―',
        0x5D => '‐', 0x5E => '／', 0x5F => '＼', 0x60 => '〜', 0x61 => '‖', 0x62 => '｜',
        0x63 => '…', 0x64 => '‥', 0x65 => '‘', 0x66 => '’', 0x67 => '“', 0x68 => '”',
        0x69 => '（', 0x6A => '）', 0x6B => '〔', 0x6C => '〕', 0x6D => '［', 0x6E => '］',
        0x6F => '｛', 0x70 => '｝', 0x71 => '〈', 0x72 => '〉', 0x73 => '《', 0x74 => '》',
        0x75 => '「', 0x76 => '」', 0x77 => '『', 0x78 => '』', 0x79 => '【', 0x7A => '】',
        0x7B => '＋', 0x7C => '－', 0x7D => '±', 0x7E => '×', 0x80 => '÷', 0x81 => '＝',
        0x82 => '≠', 0x83 => '＜', 0x84 => '＞', 0x85 => '≦', 0x86 => '≧', 0x87 => '∞',
        0x88 => '∴', 0x89 => '♂', 0x8A => '♀', 0x8B => '°', 0x8C => '′', 0x8D => '″',
        0x8E => '℃', 0x8F => '￥', 0x90 => '＄', 0x91 => '¢', 0x92 => '£', 0x93 => '％',
        0x94 => '＃', 0x95 => '＆', 0x96 => '＊', 0x97 => '＠',
        _ => return None,
    })
}

/// A decoded PlayStation 1 save file found on a memory card.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Ps1Save {
    /// Region inferred from the 2-letter filename prefix (`BI` -> Japan, `BE` -> Europe, `BA` -> America)
    /// or product serial.
    pub region: Option<Region>,
    /// Product ID / serial (e.g. `"SLUS-00001"`, `"SCES-01234"`), or empty if non-standard.
    pub product: String,
    /// Save identifier / name portion of the filename, or full filename if non-standard.
    pub name: String,
    /// Full raw 20-character filename from the directory frame.
    pub filename: String,
    /// Decoded title string from Shift-JIS.
    pub title: String,
    /// Raw 64-byte Shift-JIS title bytes from the title frame.
    pub title_bytes: [u8; 64],
    /// 1-indexed block numbers occupied by this save (`1..=15`).
    pub blocks: Vec<u8>,
    /// Decoded icon frames (1 to 3 frames, each 16×16 RGBA pixels, row-major).
    pub icon_frames: Vec<[u8; 16 * 16 * 4]>,
    /// Icon display flag from the title frame (`0x11` = 1 static frame, `0x12` = 2 animated frames, `0x13` = 3 animated frames).
    pub icon_flag: u8,
    /// Total file size in bytes from the directory entry.
    pub file_size: usize,
    /// Raw payload bytes of all blocks in the save chain (`blocks.len() * 8192` bytes).
    pub data: Vec<u8>,
}

impl Ps1Save {
    /// Returns the number of 8 KiB blocks occupied by this save.
    #[must_use]
    pub fn block_count(&self) -> usize {
        self.blocks.len()
    }
}

/// An entry marked as deleted on the memory card evaluated as an undelete candidate
/// under the shell's L1+L2+R1+R2 shortcut (`notes/research/hidden_input.md` §6).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Ps1UndeleteCandidate {
    /// The save as it would be restored.
    pub save: Ps1Save,
    /// Whether the shell's L1+L2+R1+R2 undelete (B0:46) would successfully restore this save
    /// if it were the last deleted name remembered by the shell (`notes/research/hidden_input.md` §6.1).
    pub recoverable: bool,
    /// Reason if undelete would fail.
    pub failure_reason: Option<String>,
    /// Directory state transitions that undelete would apply: `(block_number, old_state, new_state)`.
    pub state_transitions: Vec<(u8, u8, u8)>,
}

/// A single recorded performance event in a SoundScope `BPLAYSTATION` score file
/// (`notes/research/hidden_input.md` §9.2).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct PerformanceEvent {
    /// Field / frame number when the event starts (little-endian u32).
    pub field: u32,
    /// Duration of the event in fields (little-endian u16).
    pub duration: u16,
    /// Digital button bitmask recorded (little-endian u16).
    pub buttons: u16,
    /// Visualiser preset selector (0..24), tracked from the initial selector/variant
    /// via Left/Right navigation (`notes/research/hidden_input.md` §8.1).
    pub preset: Option<u8>,
}

impl PerformanceEvent {
    /// Creates a new performance event.
    #[must_use]
    pub fn new(field: u32, duration: u16, buttons: u16, preset: Option<u8>) -> Self {
        Self {
            field,
            duration,
            buttons,
            preset,
        }
    }
}

/// A decoded SoundScope performance score file (`BPLAYSTATION`) recorded by the PS1 CD player
/// (`notes/research/hidden_input.md` §8–§9).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct SoundScopePerformance {
    /// Format version (expected `0x00010000`, `notes/research/hidden_input.md` §9.2).
    pub version: u32,
    /// Stored checksum from header offset 0x204.
    pub checksum: u32,
    /// Calculated wrapping 32-bit checksum over 900 event slots (`notes/research/hidden_input.md` §9.2).
    pub calculated_checksum: u32,
    /// True if stored checksum matches calculated checksum.
    pub checksum_valid: bool,
    /// Number of recorded event slots (capped at 900).
    pub recorded_count: u32,
    /// Initial scale parameter (`notes/research/hidden_input.md` §9.2).
    pub initial_scale: u32,
    /// Initial variant / preset selector parameter (`notes/research/hidden_input.md` §9.2).
    pub initial_variant: u32,
    /// Recorded visual control events.
    pub events: Vec<PerformanceEvent>,
}

impl SoundScopePerformance {
    /// Total duration of the performance in video fields.
    #[must_use]
    pub fn duration_fields(&self) -> u32 {
        self.events
            .iter()
            .map(|e| e.field.saturating_add(e.duration as u32))
            .max()
            .unwrap_or(0)
    }

    /// Parses a SoundScope performance file from save data bytes (`notes/research/hidden_input.md` §9.2).
    ///
    /// Accepts either the full save block payload starting with `SC` (at least 0x1E80 bytes)
    /// or the data starting at offset 0x200.
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let slice = if bytes.len() >= 0x1E80 && bytes.starts_with(TITLE_FRAME_MAGIC) {
            &bytes[0x200..0x1E80]
        } else if bytes.len() >= 0x1C80 {
            &bytes[..0x1C80.min(bytes.len())]
        } else {
            return Err(Error::Corrupt(
                Format::MemoryCard,
                format!("SoundScope payload is {} bytes, shorter than expected 0x1E80", bytes.len()),
            ));
        };

        let read_u32 = |o: usize| -> u32 {
            u32::from_le_bytes(slice[o..o + 4].try_into().unwrap_or([0; 4]))
        };

        let version = read_u32(0);
        let checksum = read_u32(4);
        let recorded_count = read_u32(8);
        let initial_scale = read_u32(12);
        let initial_variant = read_u32(16);

        // Calculate wrapping checksum over 900 slots: sum(frame) + sum(buttons) (notes/research/hidden_input.md §9.2)
        let mut calc_sum = 0u32;
        for i in 0..SOUNDSCOPE_MAX_EVENTS {
            let slot = 0x60 + i * 8;
            if slot + 8 <= slice.len() {
                let frame = u32::from_le_bytes(slice[slot..slot + 4].try_into().unwrap_or([0; 4]));
                let buttons = u16::from_le_bytes(slice[slot + 6..slot + 8].try_into().unwrap_or([0; 2]));
                calc_sum = calc_sum.wrapping_add(frame).wrapping_add(buttons as u32);
            }
        }
        let checksum_valid = calc_sum == checksum;

        let count = (recorded_count as usize).min(SOUNDSCOPE_MAX_EVENTS);
        let mut events = Vec::with_capacity(count);
        let mut current_preset = (initial_variant % 24) as u8;

        for i in 0..count {
            let slot = 0x60 + i * 8;
            if slot + 8 <= slice.len() {
                let field = u32::from_le_bytes(slice[slot..slot + 4].try_into().unwrap_or([0; 4]));
                let duration = u16::from_le_bytes(slice[slot + 4..slot + 6].try_into().unwrap_or([0; 2]));
                let buttons = u16::from_le_bytes(slice[slot + 6..slot + 8].try_into().unwrap_or([0; 2]));

                // Update preset selector (hidden_input.md §8.1/§8.2: Left 0x8000 / Right 0x2000 wrap modulo 24)
                if buttons & 0x8000 != 0 {
                    current_preset = (current_preset + 23) % 24;
                } else if buttons & 0x2000 != 0 {
                    current_preset = (current_preset + 1) % 24;
                }

                events.push(PerformanceEvent {
                    field,
                    duration,
                    buttons,
                    preset: Some(current_preset),
                });
            }
        }

        Ok(Self {
            version,
            checksum,
            calculated_checksum: calc_sum,
            checksum_valid,
            recorded_count,
            initial_scale,
            initial_variant,
            events,
        })
    }
}

/// A parsed PlayStation 1 memory card image (`Ps1Card`).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Ps1Card {
    /// Active, undeleted saves present on the card.
    pub saves: Vec<Ps1Save>,
    /// Broken frame numbers recorded in directory block frames 16..=35 (`0xFFFFFFFF` = none).
    pub broken_frames: Vec<u32>,
    /// Whether the header frame (Frame 0) checksum is valid.
    pub header_checksum_valid: bool,
    /// True if all 15 directory frames have valid checksums.
    pub directory_checksums_valid: bool,
    /// Raw 128 KiB card data.
    pub raw: Vec<u8>,
}

impl Ps1Card {
    /// Parses a PlayStation 1 memory card image from raw bytes.
    ///
    /// Accepts:
    /// - Raw 128 KiB images (`.mcr`, `.mcd`, `.mem`, DuckStation/ePSXe raw, exactly 131,072 bytes).
    /// - InterAct DexDrive `.gme` images (starts with `"123-456-STD"`, 3904-byte header before the 128 KiB card).
    ///
    /// Validates the `MC` header frame magic and computes frame checksums.
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let raw_slice = if bytes.starts_with(GME_MAGIC) {
            if bytes.len() < GME_HEADER_SIZE + PS1_CARD_SIZE {
                return Err(Error::Corrupt(
                    Format::MemoryCard,
                    format!("DexDrive .gme image is {} bytes, shorter than required {}", bytes.len(), GME_HEADER_SIZE + PS1_CARD_SIZE),
                ));
            }
            &bytes[GME_HEADER_SIZE..GME_HEADER_SIZE + PS1_CARD_SIZE]
        } else if bytes.len() == PS1_CARD_SIZE {
            bytes
        } else if bytes.len() > PS1_CARD_SIZE && bytes.starts_with(HEADER_FRAME_MAGIC) {
            &bytes[..PS1_CARD_SIZE]
        } else {
            return Err(Error::NotA(
                Format::MemoryCard,
                format!("expected 128 KiB raw PS1 card or .gme image, got {} bytes", bytes.len()),
            ));
        };

        // Frame 0 must begin with "MC"
        if raw_slice.len() < PS1_FRAME_SIZE || &raw_slice[..2] != HEADER_FRAME_MAGIC {
            return Err(Error::NotA(
                Format::MemoryCard,
                "missing 'MC' magic in PS1 memory card header frame".into(),
            ));
        }

        let header_checksum = frame_checksum(&raw_slice[..PS1_FRAME_SIZE]);
        let header_checksum_valid = header_checksum == raw_slice[127];

        // Checksum directory frames 1..=15
        let mut directory_checksums_valid = true;
        for i in 1..=15 {
            let f = &raw_slice[i * PS1_FRAME_SIZE..(i + 1) * PS1_FRAME_SIZE];
            if frame_checksum(f) != f[127] {
                directory_checksums_valid = false;
            }
        }

        // Broken frames list: frames 16..=35
        let mut broken_frames = Vec::new();
        for i in 16..=35 {
            let f = &raw_slice[i * PS1_FRAME_SIZE..(i + 1) * PS1_FRAME_SIZE];
            let val = u32::from_le_bytes(f[..4].try_into().unwrap_or([0xFF; 4]));
            if val != 0xFFFF_FFFF {
                broken_frames.push(val);
            }
        }

        // Parse active saves (state == 0x51)
        let mut saves = Vec::new();
        for i in 1..=15 {
            let f = &raw_slice[i * PS1_FRAME_SIZE..(i + 1) * PS1_FRAME_SIZE];
            let state = BlockState::from_byte(f[0]);
            if state == BlockState::InUseFirst {
                if let Some(save) = parse_save_chain(raw_slice, i) {
                    saves.push(save);
                }
            }
        }

        Ok(Self {
            saves,
            broken_frames,
            header_checksum_valid,
            directory_checksums_valid,
            raw: raw_slice.to_vec(),
        })
    }

    /// Evaluates deleted saves on the card as undelete candidates under the shell's
    /// L1+L2+R1+R2 shortcut (`notes/research/hidden_input.md` §6).
    ///
    /// # Important shell behavior
    /// The retail PS1 shell remembers only the single **last** deleted filename in that session
    /// (at RAM `0x80086208` on 1.0 J / `0x80090CF0` on 4.5 A) and passes it to BIOS B0 function
    /// 0x46 upon pressing the four-shoulder chord. A fresh boot clears this path.
    ///
    /// Each returned entry is modeled as a **candidate if this were the last deletion** in that session,
    /// checking whether ROM `0xBFC0AC78` would successfully restore its directory states to
    /// `0x51`/`0x52`/`0x53` or reject it due to block reuse, filename collisions, or broken chains.
    #[must_use]
    pub fn deleted(&self) -> Vec<Ps1UndeleteCandidate> {
        let mut candidates = Vec::new();

        // Collect all blocks currently used by live saves to detect block theft / collision
        let mut live_used_blocks = [false; 16];
        for save in &self.saves {
            for &blk in &save.blocks {
                if (blk as usize) < 16 {
                    live_used_blocks[blk as usize] = true;
                }
            }
        }

        for i in 1..=15 {
            let f = &self.raw[i * PS1_FRAME_SIZE..(i + 1) * PS1_FRAME_SIZE];
            let state = BlockState::from_byte(f[0]);
            if state == BlockState::DeletedFirst {
                let Some(save) = parse_save_chain(&self.raw, i) else {
                    continue;
                };

                // Check eligibility under B0:46 rules
                let mut recoverable = true;
                let mut failure_reason = None;

                // 1. Conflict with existing live filename
                if self.saves.iter().any(|s| s.filename == save.filename) {
                    recoverable = false;
                    failure_reason = Some(format!(
                        "live save with identical filename \"{}\" already exists on the card",
                        save.filename
                    ));
                }

                // 2. Block reuse by a live save
                if recoverable {
                    for &blk in &save.blocks {
                        if live_used_blocks[blk as usize] {
                            recoverable = false;
                            failure_reason = Some(format!(
                                "block {blk} in deleted chain is currently occupied by a live save"
                            ));
                            break;
                        }
                    }
                }

                // 3. Chain validation: check continuation states (0xA2 for middle, 0xA3 for last)
                if recoverable && save.blocks.len() > 1 {
                    for (idx, &blk) in save.blocks.iter().enumerate().skip(1) {
                        let is_last = idx == save.blocks.len() - 1;
                        let blk_f = &self.raw[(blk as usize) * PS1_FRAME_SIZE..((blk as usize) + 1) * PS1_FRAME_SIZE];
                        let blk_state = BlockState::from_byte(blk_f[0]);
                        let expected = if is_last { BlockState::DeletedLast } else { BlockState::DeletedMiddle };
                        if blk_state != expected {
                            recoverable = false;
                            failure_reason = Some(format!(
                                "block {blk} state is 0x{:02X}, expected 0x{:02X} ({:?})",
                                blk_f[0], expected.to_byte(), expected
                            ));
                            break;
                        }
                    }
                }

                // Compute state transitions: 0xA1 -> 0x51, 0xA2 -> 0x52, 0xA3 -> 0x53
                let mut state_transitions = Vec::with_capacity(save.blocks.len());
                for (idx, &blk) in save.blocks.iter().enumerate() {
                    let old_st = self.raw[(blk as usize) * PS1_FRAME_SIZE];
                    let new_st = if idx == 0 {
                        0x51
                    } else if idx == save.blocks.len() - 1 {
                        0x53
                    } else {
                        0x52
                    };
                    state_transitions.push((blk, old_st, new_st));
                }

                candidates.push(Ps1UndeleteCandidate {
                    save,
                    recoverable,
                    failure_reason,
                    state_transitions,
                });
            }
        }

        candidates
    }

    /// Finds and decodes all SoundScope performance scores (`BPLAYSTATION`) recorded on the card
    /// (`notes/research/hidden_input.md` §8–§9).
    #[must_use]
    pub fn soundscope_performances(&self) -> Vec<SoundScopePerformance> {
        let mut list = Vec::new();
        for save in &self.saves {
            if save.filename.starts_with(SOUNDSCOPE_FILENAME) {
                if let Ok(perf) = SoundScopePerformance::parse(&save.data) {
                    list.push(perf);
                }
            }
        }
        list
    }

    /// Returns a reference to a live save by exact filename if present.
    #[must_use]
    pub fn save_by_name(&self, filename: &str) -> Option<&Ps1Save> {
        self.saves.iter().find(|s| s.filename == filename)
    }

    /// Returns a reference to a live save matching a product code if present.
    #[must_use]
    pub fn save_by_product(&self, product: &str) -> Option<&Ps1Save> {
        self.saves.iter().find(|s| s.product.eq_ignore_ascii_case(product))
    }
}

/// Follows the directory block chain starting at block `start_block` (1..=15) and parses the save.
fn parse_save_chain(raw: &[u8], start_block: usize) -> Option<Ps1Save> {
    let f0 = &raw[start_block * PS1_FRAME_SIZE..(start_block + 1) * PS1_FRAME_SIZE];

    let file_size = u32::from_le_bytes(f0[4..8].try_into().unwrap_or([0; 4])) as usize;

    // Filename: 21 bytes at 0x0A..0x1F, NUL-terminated
    let fn_bytes = &f0[0x0A..0x1F];
    let fn_end = fn_bytes.iter().position(|&b| b == 0).unwrap_or(fn_bytes.len());
    let filename = String::from_utf8_lossy(&fn_bytes[..fn_end]).trim().to_string();

    let (region, product, name) = parse_filename(&filename);

    // Follow next block pointers
    let mut blocks = vec![start_block as u8];
    let mut curr = start_block;
    let mut visited = [false; 16];
    visited[start_block] = true;

    loop {
        let dir_frame = &raw[curr * PS1_FRAME_SIZE..(curr + 1) * PS1_FRAME_SIZE];
        let next = i16::from_le_bytes([dir_frame[8], dir_frame[9]]);
        if next == -1 || next as u16 == 0xFFFF {
            break;
        }
        if !(0..15).contains(&next) {
            break;
        }
        let next_blk = (next as usize) + 1;
        if visited[next_blk] {
            break; // loop guard
        }
        visited[next_blk] = true;
        blocks.push(next_blk as u8);
        curr = next_blk;
    }

    // Assemble payload bytes across all blocks
    let mut data = Vec::with_capacity(blocks.len() * PS1_BLOCK_SIZE);
    for &blk in &blocks {
        let blk_start = (blk as usize) * PS1_BLOCK_SIZE;
        if blk_start + PS1_BLOCK_SIZE <= raw.len() {
            data.extend_from_slice(&raw[blk_start..blk_start + PS1_BLOCK_SIZE]);
        }
    }

    // Parse title frame from Frame 0 of the first data block
    let mut title = String::new();
    let mut title_bytes = [0u8; 64];
    let mut icon_frames = Vec::new();
    let mut icon_flag = 0u8;

    if data.len() >= PS1_FRAME_SIZE && data.starts_with(TITLE_FRAME_MAGIC) {
        icon_flag = data[2];
        title_bytes.copy_from_slice(&data[4..68]);
        title = decode_sjis(&title_bytes);

        // 16-color CLUT at offset 0x60..0x80 (32 bytes = 16 16-bit entries)
        let mut clut = [[0u8; 4]; 16];
        for (i, entry) in clut.iter_mut().enumerate() {
            let o = 0x60 + i * 2;
            if o + 2 <= data.len() {
                let raw_val = u16::from_le_bytes([data[o], data[o + 1]]);
                let r = ((raw_val & 0x1F) as u32 * 255 / 31) as u8;
                let g = (((raw_val >> 5) & 0x1F) as u32 * 255 / 31) as u8;
                let b = (((raw_val >> 10) & 0x1F) as u32 * 255 / 31) as u8;
                // PS1 memory card CLUT: 0 is transparent, non-zero is opaque (ps1.rs, psx-spx)
                let a = if raw_val == 0 { 0 } else { 255 };
                *entry = [r, g, b, a];
            }
        }

        // Icon frames: 1 to 3 frames depending on icon_flag
        let frame_count = match icon_flag {
            0x11 => 1,
            0x12 => 2,
            0x13 => 3,
            _ => (icon_flag & 0x0F).clamp(1, 3) as usize,
        };

        for f in 1..=frame_count {
            let frame_start = f * PS1_FRAME_SIZE;
            if frame_start + PS1_FRAME_SIZE <= data.len() {
                let icon_data = &data[frame_start..frame_start + PS1_FRAME_SIZE];
                let mut rgba = [0u8; 16 * 16 * 4];
                for y in 0..16 {
                    for x in 0..16 {
                        let byte_idx = y * 8 + x / 2;
                        let byte = icon_data[byte_idx];
                        let clut_idx = if x % 2 == 0 { byte & 0x0F } else { byte >> 4 } as usize;
                        let color = clut[clut_idx.min(15)];
                        let px_idx = (y * 16 + x) * 4;
                        rgba[px_idx..px_idx + 4].copy_from_slice(&color);
                    }
                }
                icon_frames.push(rgba);
            }
        }
    }

    Some(Ps1Save {
        region,
        product,
        name,
        filename,
        title,
        title_bytes,
        blocks,
        icon_frames,
        icon_flag,
        file_size,
        data,
    })
}

/// Parses region, product, and save name from a 20-character filename string.
fn parse_filename(raw: &str) -> (Option<Region>, String, String) {
    if raw.len() >= 2 {
        let region = match &raw[..2] {
            "BA" => Some(Region::America),
            "BE" => Some(Region::Europe),
            "BI" => Some(Region::Japan),
            _ => None,
        };

        if let Some(r) = region {
            if raw.len() >= 12 {
                let product = raw[2..12].to_string();
                let name = raw[12..].to_string();
                return (Some(r), product, name);
            }
            return (Some(r), String::new(), raw[2..].to_string());
        }
    }

    // Try inferring region from product if formatted like SLUS/SLES/SLPS
    if raw.len() >= 10 {
        if let Some(r) = Region::from_title_id(raw) {
            return (Some(r), raw[..10].to_string(), raw[10..].to_string());
        }
    }

    (None, String::new(), raw.to_string())
}

impl fmt::Display for Ps1Card {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "PS1 Memory Card: {} active saves, {} broken sectors, header checksum: {}",
            self.saves.len(),
            self.broken_frames.len(),
            if self.header_checksum_valid { "OK" } else { "INVALID" }
        )
    }
}
