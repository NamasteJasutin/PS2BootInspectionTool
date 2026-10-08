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
//! let card = MemoryCard::open(std::path::Path::new("Mcd001.ps2"))?;
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
// TODO(0.2): `disc`, `ps1` and `sim` are exempt from `missing_docs` until their public items
// are documented (listed in API_REVIEW.md); drop the `allow`s with that change.
#[allow(missing_docs)]
pub mod disc;
pub mod history;
pub mod locate;
pub mod logo;
pub mod memcard;
#[allow(missing_docs)]
pub mod ps1;
pub mod rom;
pub mod sectors;
#[allow(missing_docs)]
pub mod sim;
pub mod sound;

/// The vector types used in the public API, re-exported from `glam` so that callers need not
/// match its version by hand.
pub use glam::{Vec2, Vec3, Vec4};

/// Every failure the readers report. More variants may be added in minor versions.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The file has no memory-card superblock (unformatted, or not a card at all).
    #[error("not a PS2 memory card: {0}")]
    NotACard(String),
    /// The card's filesystem is inconsistent (bad geometry, FAT loop, chain ends early).
    #[error("memory card is damaged: {0}")]
    CardCorrupt(String),
    /// The requested path does not exist on the card.
    #[error("not found on card: {0}")]
    CardNotFound(String),
    /// The file has no ROMDIR table, so it is not a BIOS dump.
    #[error("not a PS2 BIOS image: {0}")]
    NotABios(String),
    /// The BIOS is genuine but its OSD program is of a generation whose tables are not located.
    #[error("BIOS version {0} is not supported yet (data tables are located per version)")]
    UnsupportedVersion(String),
    /// The ROMDIR has no module of that name.
    #[error("BIOS module missing: {0}")]
    MissingModule(String),
    /// A structure read from a file is truncated or inconsistent.
    #[error("data is damaged: {0}")]
    Corrupt(String),
    /// A sound bank (`SShd` / `pBAV`) is malformed.
    #[error("sound bank: {0}")]
    Bank(String),
    /// A sound sequence (`SSsq`) is malformed.
    #[error("sound sequence: {0}")]
    Sequence(String),
    /// The underlying file could not be read.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// `std::result::Result` with this crate's [`Error`].
pub type Result<T> = std::result::Result<T, Error>;
