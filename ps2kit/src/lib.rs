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

pub mod bios;
pub mod bytes;
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

pub use glam::{Vec2, Vec3, Vec4};

/// Every failure the readers report. More variants may be added in minor versions.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("not a PS2 memory card: {0}")]
    NotACard(String),
    #[error("memory card is damaged: {0}")]
    CardCorrupt(String),
    #[error("not found on card: {0}")]
    CardNotFound(String),
    #[error("not a PS2 BIOS image: {0}")]
    NotABios(String),
    #[error("BIOS version {0} is not supported yet (data tables are located per version)")]
    UnsupportedVersion(String),
    #[error("BIOS module missing: {0}")]
    MissingModule(String),
    #[error("data is damaged: {0}")]
    Corrupt(String),
    #[error("sound bank: {0}")]
    Bank(String),
    #[error("sound sequence: {0}")]
    Sequence(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
