//! Portable core of the PS2 Boot Inspection Tool: readers for the user's own BIOS dump,
//! memory card and disc image, and the simulation of what the console shows at start-up.
//! Nothing from Sony is embedded; everything is derived from the user's files at run time.

pub mod bios;
pub mod bytes;
pub mod disc;
pub mod history;
pub mod logo;
pub mod memcard;
pub mod rom;
pub mod sectors;
pub mod sim;
pub mod sound;

pub use glam::{Vec2, Vec3, Vec4};

#[derive(Debug, thiserror::Error)]
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
