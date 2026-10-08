//! Readers for PlayStation 2 BIOS dumps, PCSX2 memory cards and game disc images, plus a
//! re-implementation of what the console computes at start-up (the boot screen built from
//! the play history, the camera, the chime, the logo program's timing).
//!
//! Nothing from Sony is embedded: every table, texture, sound and bitmap is read from files
//! the user supplies at run time, and the crate is tested on CI with synthetic inputs only.
//!
//! | module | what it reads or does |
//! |---|---|
//! | [`rom`] | the ROMDIR container and the OSD LZ scheme |
//! | [`memcard`] | PCSX2 card images (with or without ECC) and folder cards |
//! | [`history`] | the play-history file (`B?DATA-SYSTEM/history`) |
//! | [`sectors`], [`disc`] | ISO / BIN+CUE sector access, `SYSTEM.CNF`, the boot ELF, the hand-off |
//! | [`logo`] | `rom0:PS2LOGO` assets and the logo bitmap on a disc's first sectors |
//! | [`sound`] | OSD sound bank, sequences and the SPU envelope, rendered to PCM |
//! | [`bios`], [`locate`] | the opening's data tables, found in any supported ROM by content |
//! | [`sim`] | towers, camera and timelines as functions of the frame number |
//! | [`ps1`] | the PS1 licence screen a PS2 shows for a PlayStation disc (`rom0:LOGO`, TMD, TIM, VAB) |
//!
//! ```no_run
//! use ps2kit::{memcard::MemoryCard, history::PlayHistory};
//! let card = MemoryCard::open("Mcd001.ps2")?;
//! for r in &PlayHistory::from_card(&card)?.records {
//!     if !r.is_empty() { println!("{} launched {} times, towers {:06b}", r.name, r.count, r.mask) }
//! }
//! # Ok::<(), ps2kit::Error>(())
//! ```
//!
//! Supported BIOS versions for [`bios`]/[`logo`]/[`sound`]: ROM 1.50–2.00 of the OSD
//! generation with `TEX*` assets (tested: 2.00 E, 1.60 E, 1.60 A, DTL-H30101 1.50 A);
//! ROM 1.00 is rejected with [`Error::UnsupportedVersion`]. The other readers do not depend
//! on the BIOS at all.
//!
//! The formats are documented in the `notes/` directory of the
//! [repository](https://github.com/NamasteJasutin/PS2BootInspectionTool).

#![warn(missing_docs)]

pub mod bios;
mod bytes;
pub mod disc;
pub mod history;
pub mod locate;
pub mod logo;
pub mod memcard;
pub mod ps1;
pub mod rom;
pub mod sectors;
pub mod sim;
pub mod sound;

/// The vector types used in the public API, re-exported from `glam` so that callers need not
/// match its version by hand.
pub use glam::{Vec2, Vec3, Vec4};

/// The television standard a console runs in; it sets the field rate and the picture height.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VideoMode {
    /// 60 Hz, 224-line fields (J, A, H and C consoles).
    Ntsc,
    /// 50 Hz, 256-line fields (E consoles).
    Pal,
}

impl VideoMode {
    /// Fields per second: 60 or 50.
    #[must_use]
    pub fn fps(self) -> f32 { if self == Self::Pal { 50.0 } else { 60.0 } }
    /// The opening integrates with a 1.2 step at 50 Hz so both modes take the same time.
    #[must_use]
    pub fn time_step(self) -> f32 { if self == Self::Pal { 1.2 } else { 1.0 } }
    /// Lines of one field: 224 or 256.
    #[must_use]
    pub fn field_height(self) -> f32 { if self == Self::Pal { 256.0 } else { 224.0 } }
    /// The console's vertical projection factor (line aspect) for this mode.
    #[must_use]
    pub fn aspect_y(self) -> f32 { if self == Self::Pal { 0.526271 } else { 0.457627 } }
}

/// A sales region, as the BIOS's `ROMVER` letter, a title ID's prefix and a PlayStation
/// licence sector encode it. `Display` gives the plain name (`Japan`, `America`, ...).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Region {
    /// `J` and `H` consoles; `SLPS`/`SCPS`/`SLPM`/`SCPM`/`SLKA` titles; the "Inc." licence line.
    Japan,
    /// `A` consoles; `SLUS`/`SCUS` titles; the "America" licence line.
    America,
    /// `E` consoles; `SLES`/`SCES` titles; the "Europe" licence line.
    Europe,
    /// `C` consoles.
    China,
    /// `SCAJ` titles (no console letter maps here: `H` consoles behave as Japanese ones).
    Asia,
}

impl Region {
    /// The region behind the fifth character of `ROMVER` (`0200EC20040614` → `E`): `J`/`H`
    /// Japan, `A` America, `E` Europe, `C` China; `None` for any other letter.
    #[must_use]
    pub fn from_romver_letter(letter: char) -> Option<Self> {
        Some(match letter {
            'J' | 'H' => Self::Japan,
            'A' => Self::America,
            'E' => Self::Europe,
            'C' => Self::China,
            _ => return None,
        })
    }

    /// The region a title ID's four-letter prefix implies (`SLES_530.64` → Europe); `None`
    /// for an unknown prefix.
    #[must_use]
    pub fn from_title_id(title_id: &str) -> Option<Self> {
        Some(match title_id.get(..4)? {
            "SLES" | "SCES" => Self::Europe,
            "SLUS" | "SCUS" => Self::America,
            "SLPS" | "SCPS" | "SLPM" | "SCPM" | "SLKA" => Self::Japan,
            "SCAJ" => Self::Asia,
            _ => return None,
        })
    }
}

impl std::fmt::Display for Region {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Japan => "Japan",
            Self::America => "America",
            Self::Europe => "Europe",
            Self::China => "China",
            Self::Asia => "Asia",
        })
    }
}

/// The kind of file or structure an [`Error`] is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Format {
    /// A PCSX2 memory card image or folder.
    MemoryCard,
    /// A BIOS dump or a module inside it.
    Bios,
    /// A disc image (ISO, BIN/CUE).
    DiscImage,
    /// An `SShd`/`pBAV` sound bank.
    SoundBank,
    /// An `SSsq` sound sequence.
    Sequence,
    /// A libgs TMD model.
    Tmd,
}

impl std::fmt::Display for Format {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::MemoryCard => "memory card",
            Self::Bios => "BIOS image",
            Self::DiscImage => "disc image",
            Self::SoundBank => "sound bank",
            Self::Sequence => "sound sequence",
            Self::Tmd => "TMD model",
        })
    }
}

/// Every failure the readers report. More variants may be added in minor versions.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The file is not of the expected format at all (no superblock, no ROMDIR table, ...).
    NotA(Format, String),
    /// A structure read from the file is truncated or inconsistent.
    Corrupt(Format, String),
    /// A named item is missing: a path on a card, a module in a BIOS.
    NotFound(Format, String),
    /// The BIOS is genuine but its OSD program is of a generation whose tables are not located.
    UnsupportedVersion(String),
    /// The underlying file could not be read.
    Io(#[from] std::io::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotA(Format::MemoryCard, s) => write!(f, "not a PS2 memory card: {s}"),
            Self::NotA(Format::Bios, s) => write!(f, "not a PS2 BIOS image: {s}"),
            Self::NotA(what, s) => write!(f, "not a {what}: {s}"),
            Self::Corrupt(Format::MemoryCard, s) => write!(f, "memory card is damaged: {s}"),
            Self::Corrupt(Format::SoundBank, s) => write!(f, "sound bank: {s}"),
            Self::Corrupt(Format::Sequence, s) => write!(f, "sound sequence: {s}"),
            Self::Corrupt(_, s) => write!(f, "data is damaged: {s}"),
            Self::NotFound(Format::MemoryCard, s) => write!(f, "not found on card: {s}"),
            Self::NotFound(Format::Bios, s) => write!(f, "BIOS module missing: {s}"),
            Self::NotFound(what, s) => write!(f, "not found in {what}: {s}"),
            Self::UnsupportedVersion(s) => write!(f, "BIOS version {s} is not supported yet (data tables are located per version)"),
            Self::Io(e) => e.fmt(f),
        }
    }
}

/// `std::result::Result` with this crate's [`Error`].
pub type Result<T> = std::result::Result<T, Error>;
