//! The console's play history: 21 records of 22 bytes in `B?DATA-SYSTEM/history`.

use crate::bytes::Bytes;
use crate::memcard::MemoryCard;
use crate::{Error, Result};

/// One slot of the play history. The layout is fixed (22 bytes), so the type is meant to be
/// constructed by callers too, e.g. to feed [`PlayHistory::new`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Record {
    /// Title ID (`SLES_530.64`); empty means an unused slot.
    pub name: String,
    /// Launch count, 1..=63.
    pub count: u8,
    /// Which of the title's six towers exist (bits 0-5).
    pub mask: u8,
    /// The bit that is still growing; 7 once the record is maxed out.
    pub index: u8,
    /// Last launch as `day | month << 5 | (year - 2000) << 9`; see the accessors.
    pub date: u16,
}

impl Record {
    /// True for an unused slot.
    #[must_use]
    pub fn is_empty(&self) -> bool { self.name.is_empty() }
    /// Day of month of the last launch (1..=31, or 0 when never set).
    #[must_use]
    pub fn day(&self) -> u16 { self.date & 31 }
    /// Month of the last launch (1..=12, or 0 when never set).
    #[must_use]
    pub fn month(&self) -> u16 { self.date >> 5 & 15 }
    /// Year of the last launch (2000 when never set).
    #[must_use]
    pub fn year(&self) -> u16 { 2000 + (self.date >> 9) }
}

/// Number of slots in the history file.
pub const RECORD_COUNT: usize = 21;
/// Bytes per slot in the history file.
pub const RECORD_SIZE: usize = 22;
/// The system folders, one per region, in which the BIOS keeps `history`.
pub const SYSTEM_FOLDERS: [&str; 4] = ["BIDATA-SYSTEM", "BADATA-SYSTEM", "BEDATA-SYSTEM", "BCDATA-SYSTEM"];

/// The 21 records of a history file; always exactly [`RECORD_COUNT`] long.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct PlayHistory {
    /// The slots in file order; unused ones are empty records.
    pub records: Vec<Record>,
    /// Where the table came from (system folder name), if it was read from a card.
    pub source: Option<String>,
}

impl PlayHistory {
    /// Builds a history from `records`, padding or cutting to [`RECORD_COUNT`].
    #[must_use]
    pub fn new(mut records: Vec<Record>, source: Option<String>) -> Self {
        records.resize(RECORD_COUNT, Record::default());
        Self { records, source }
    }

    /// Parses the raw `history` file; short files leave the remaining records empty.
    #[must_use]
    pub fn from_file(data: &[u8], source: Option<String>) -> Self {
        let mut recs = Vec::new();
        for i in 0..RECORD_COUNT {
            let o = i * RECORD_SIZE;
            if o + RECORD_SIZE > data.len() {
                break;
            }
            let r = &data[o..o + RECORD_SIZE];
            recs.push(Record { name: r.cstr(0, 16), count: r.u8(16), mask: r.u8(17), index: r.u8(18), date: r.u16(20) });
        }
        Self::new(recs, source)
    }

    /// Looks for `B?DATA-SYSTEM/history` in any region's system folder.
    pub fn from_card(card: &MemoryCard) -> Result<Self> {
        let root = card.list(&[])?;
        for folder in SYSTEM_FOLDERS {
            if root.iter().any(|e| e.name == folder && e.is_directory) {
                if let Ok(data) = card.read_file(&[folder, "history"]) {
                    return Ok(Self::from_file(&data, Some(folder.into())));
                }
            }
        }
        Err(Error::CardNotFound("B?DATA-SYSTEM/history (the BIOS creates it the first time it launches a disc itself)".into()))
    }

    /// A made-up history following the console's update rules: `launches[i]` is how often
    /// title `i` was started; `titles` optionally names them. Deterministic for a `seed`.
    #[must_use]
    pub fn synthetic(launches: &[u32], titles: &[String], seed: u64) -> Self {
        let mut rng = SplitMix(seed);
        let mut recs = Vec::new();
        for (i, &n) in launches.iter().take(RECORD_COUNT).enumerate() {
            if n == 0 {
                continue;
            }
            let name = titles.get(i).cloned().unwrap_or_else(|| format!("TEST_{:03}.{:02}", i / 100, i % 100));
            let mut r = Record { name, count: 1, mask: 1, index: 0, date: 0 };
            for _ in 1..n {
                if r.mask & 0x3F == 0x3F {
                    if r.count < 0x3F { r.count += 1 } else { r.index = 7 }
                } else {
                    r.count += 1;
                    if r.count >= 14 && (r.count - 14).is_multiple_of(10) {
                        let mut b;
                        loop {
                            b = (rng.next() % 6) as u8;
                            if r.mask & (1 << b) == 0 {
                                break;
                            }
                        }
                        r.index = b;
                        r.mask |= 1 << b;
                    }
                }
            }
            r.date = (1 + (i % 28) as u16) | 6 << 5 | 4 << 9;
            recs.push(r);
        }
        Self::new(recs, Some("synthetic".into()))
    }

    /// Title IDs of the games that have saves on a card (`BESLES-52541...` -> `SLES_525.41`).
    /// A card that cannot be listed yields an empty list.
    #[must_use]
    pub fn titles_with_saves(card: &MemoryCard) -> Vec<String> {
        let mut seen: Vec<String> = Vec::new();
        for e in card.list(&[]).unwrap_or_default() {
            let n = e.name.as_bytes();
            // The slices below are by byte: the prefix must be ASCII for them to be valid.
            if !e.is_directory || n.len() < 12 || !n[..12].is_ascii() || n[0] != b'B' || n[6] != b'-' || !n[7..12].iter().all(u8::is_ascii_digit) {
                continue;
            }
            let id = format!("{}_{}.{}", &e.name[2..6], &e.name[7..10], &e.name[10..12]);
            if !seen.contains(&id) {
                seen.push(id);
            }
        }
        seen
    }
}

/// SplitMix64, the generator behind [`PlayHistory::synthetic`]; the field is the state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplitMix(pub u64);

impl SplitMix {
    /// The next 64-bit output.
    // Named like `Iterator::next` without being one; renamed in 0.2 (see API_REVIEW.md).
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}
