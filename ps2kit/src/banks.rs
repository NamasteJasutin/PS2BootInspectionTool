//! Bank identity shared by the readers: which BIOS bank a program number addresses
//! ([`BankRef`]), a portable content key for any bank found in the caller's files ([`BankKey`],
//! [`fingerprint`]), and the true extents of SShd headers and SSsq sequences
//! (`notes/research/sound_inventory.md` §"Banks, sample sets, and exact pairing").

use crate::bytes::Bytes;
use crate::{Error, Format, Result};

/// Which caller-supplied instrument bank the program numbers address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BankRef {
    /// PS2 SShd boot bank.
    Ps2,
    /// PS2 logo SShd effect bank (`notes/ps2logo.md` §5).
    Ps2Logo,
    /// PS1 shell VAB, including the PS2's embedded PS1 shell.
    Ps1,
}

/// Portable bank dependency; equality uses the fingerprint, independently of its label.
#[derive(Debug, Clone, Eq)]
#[non_exhaustive]
pub struct BankKey {
    /// Hex content hash computed at runtime; empty denotes the legacy song default.
    pub fingerprint: String,
    /// Human label saved with a song so missing dependencies remain understandable.
    pub label: String,
}

impl PartialEq for BankKey {
    fn eq(&self, other: &Self) -> bool {
        self.fingerprint == other.fingerprint
    }
}

impl std::hash::Hash for BankKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.fingerprint.hash(state);
    }
}

impl Default for BankKey {
    fn default() -> Self {
        Self::new("", "Song default bank")
    }
}

impl BankKey {
    /// Creates a dependency; the song validator checks its bounded fingerprint and label.
    pub fn new(fingerprint: impl Into<String>, label: impl Into<String>) -> Self {
        Self { fingerprint: fingerprint.into(), label: label.into() }
    }
    /// Whether this is the default binding migrated from version-1/2 song files.
    pub fn is_default(&self) -> bool {
        self.fingerprint.is_empty()
    }
    /// The label a version-1/2 song file implies for its bank.
    pub fn legacy_label(bank: BankRef) -> &'static str {
        match bank {
            BankRef::Ps1 => "PS1 shell VAB",
            BankRef::Ps2 => "PS2 boot bank",
            BankRef::Ps2Logo => "PS2 logo bank",
        }
    }
    /// Checks a saved key: a 32-digit hex fingerprint (or the legacy default) and a bounded label.
    pub fn validate(&self) -> Result<()> {
        if (!self.is_default()
            && (self.fingerprint.len() != 32 || !self.fingerprint.bytes().all(|c| c.is_ascii_hexdigit())))
            || self.label.is_empty()
            || self.label.len() > 1024
        {
            return Err(Error::Corrupt(Format::Sequence, "invalid bank fingerprint or label".into()));
        }
        Ok(())
    }
}

/// A local content fingerprint: domain-separated, length-framed FNV-1a-128 over `parts`, as 32 hex
/// digits. Computed at run time from the caller's bytes; not a shipped hash catalogue and not a
/// security primitive.
pub fn fingerprint(domain: &str, parts: &[&[u8]]) -> String {
    let mut hash = 0x6c62272e07bb014262b821756295c58d_u128;
    for part in std::iter::once(domain.as_bytes()).chain(parts.iter().copied()) {
        for byte in (part.len() as u64).to_le_bytes().iter().chain(part) {
            hash ^= u128::from(*byte);
            hash = hash.wrapping_mul(0x0000000001000000000000000000013b);
        }
    }
    format!("{hash:032x}")
}

fn effect_count(header: &[u8]) -> Option<usize> {
    if header.len() < 0x30 {
        return None;
    }
    let table = header.u32(0x2C) as usize;
    let prefix = header.get(table..table.checked_add(8)?)?;
    let count = (prefix.u32(4) as usize).checked_add(1)?;
    (count <= 8192).then_some(count)
}

/// The byte length of an SShd header at the start of `data` (programs, tones and effect tables
/// bounded), or `None` when it is not a well-formed SShd header.
pub fn header_extent(data: &[u8]) -> Option<usize> {
    if data.len() < 0x30 || &data[12..16] != b"SShd" {
        return None;
    }
    let size = if data.u32(0) == u32::MAX {
        (data.u32(0x2C) as usize).checked_add(0x20)?.checked_add(effect_count(data)?.checked_mul(0x40)?)?
    } else {
        data.u32(0) as usize
    };
    let header = data.get(..size)?;
    if !(0x31..=0x10000).contains(&size) {
        return None;
    }
    let base = header.u32(0x10) as usize;
    if base != u32::MAX as usize {
        let max = header.get(base..base.checked_add(2)?)?.u16(0) as usize;
        if max > 127 || base.checked_add(2 + (max + 1) * 2)? > size {
            return None;
        }
        for p in 0..=max {
            let offset = header.u16(base + 2 + p * 2) as usize;
            if offset == 0xFFFF {
                continue;
            }
            let program = base.checked_add(offset)?;
            let prefix = header.get(program..program.checked_add(8)?)?;
            let tones = if prefix[0] == 255 { 0 } else { usize::from(prefix[0] & 127) + 1 };
            if program.checked_add(8 + tones * 16)? > size {
                return None;
            }
        }
    }
    if header.u32(0x2C) != u32::MAX
        && (header.u32(0x2C) as usize).checked_add(0x20 + effect_count(header)? * 0x40)? > size
    {
        return None;
    }
    Some(size)
}

/// The byte length of an SSsq sequence at the start of `data` up to its end-of-track event, or
/// `None` when malformed. SSsq is MIDI-like but has two-byte BPM events, one-byte coarse bends and
/// no initial delta; the true terminator extent is used, never allocation padding.
pub fn sequence_extent(data: &[u8]) -> Option<usize> {
    if data.len() <= 0x110 || &data[12..16] != b"SSsq" {
        return None;
    }
    let (mut pos, mut status) = (0x110, 0u8);
    for _ in 0..32768 {
        let byte = *data.get(pos)?;
        if byte & 128 != 0 {
            status = byte;
            pos += 1;
        }
        let size = match status {
            0x80..=0xBF => 2,
            0xC0..=0xCF | 0xE0..=0xEF => 1,
            0xFF => match data.get(pos)? {
                0x2F => {
                    if data.get(pos + 1)? != &0 {
                        return None;
                    }
                    return Some(pos + 2);
                }
                0x51 => 3,
                _ => return None,
            },
            _ => return None,
        };
        data.get(pos..pos.checked_add(size)?)?;
        pos += size;
        let mut finished = false;
        for _ in 0..4 {
            let byte = *data.get(pos)?;
            pos += 1;
            if byte & 128 == 0 {
                finished = true;
                break;
            }
        }
        if !finished {
            return None;
        }
    }
    None
}
