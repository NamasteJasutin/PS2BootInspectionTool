//! The console's play history: 21 records of 22 bytes in `B?DATA-SYSTEM/history`.

use crate::bytes::Bytes;
use crate::memcard::MemoryCard;
use crate::{Error, Result};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Record {
    pub name: String, // title ID, empty = unused slot
    pub count: u8,    // launches, 1..=63
    pub mask: u8,     // which of the title's six towers exist (bits 0-5)
    pub index: u8,    // bit that is still growing; 7 = record maxed out
    pub date: u16,    // day | month << 5 | (year - 2000) << 9
}

impl Record {
    pub fn is_empty(&self) -> bool { self.name.is_empty() }
    pub fn day(&self) -> u16 { self.date & 31 }
    pub fn month(&self) -> u16 { self.date >> 5 & 15 }
    pub fn year(&self) -> u16 { 2000 + (self.date >> 9) }
}

pub const RECORD_COUNT: usize = 21;
pub const RECORD_SIZE: usize = 22;
pub const SYSTEM_FOLDERS: [&str; 4] = ["BIDATA-SYSTEM", "BADATA-SYSTEM", "BEDATA-SYSTEM", "BCDATA-SYSTEM"];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlayHistory {
    pub records: Vec<Record>,
    /// Where the table came from (system folder name), if it was read from a card.
    pub source: Option<String>,
}

impl PlayHistory {
    pub fn new(mut records: Vec<Record>, source: Option<String>) -> Self {
        records.resize(RECORD_COUNT, Record::default());
        Self { records, source }
    }

    /// Parses the raw `history` file; short files leave the remaining records empty.
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
    /// title `i` was started; `titles` optionally names them.
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
                    if r.count >= 14 && (r.count - 14) % 10 == 0 {
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
    pub fn titles_with_saves(card: &MemoryCard) -> Vec<String> {
        let mut seen: Vec<String> = Vec::new();
        for e in card.list(&[]).unwrap_or_default() {
            let n = e.name.as_bytes();
            if !e.is_directory || n.len() < 12 || n[0] != b'B' || n[6] != b'-' || !n[7..12].iter().all(u8::is_ascii_digit) {
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

pub struct SplitMix(pub u64);

impl SplitMix {
    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}
