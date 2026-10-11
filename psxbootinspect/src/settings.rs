//! The app's few remembered choices, in a plain `key = value` file in the user's settings folder
//! (`settings.txt` under `PSX Boot Inspection Tool/`). Today that is only where the Soundfonts
//! folder lives; nothing else is stored, and nothing leaves the machine.

use std::path::{Path, PathBuf};

/// The folder name the app uses under the platform's settings and data folders.
pub const APP_DIR: &str = "PSX Boot Inspection Tool";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Settings {
    /// Where banks extracted from the user's own discs are kept; `None` means the default.
    pub soundfonts_dir: Option<PathBuf>,
}

impl Settings {
    /// `~/Library/Application Support/PSX Boot Inspection Tool/settings.txt` on macOS,
    /// `~/.config/…` on Linux, `%APPDATA%\…` on Windows.
    fn path() -> Option<PathBuf> { dirs::config_dir().map(|d| d.join(APP_DIR).join("settings.txt")) }

    /// The saved settings, or the defaults when there are none (or they cannot be read).
    pub fn load() -> Self { Self::path().and_then(|p| std::fs::read_to_string(p).ok()).map(|t| Self::parse(&t)).unwrap_or_default() }

    fn parse(text: &str) -> Self {
        let mut s = Self::default();
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            match k.trim() {
                "soundfonts" if !v.trim().is_empty() => s.soundfonts_dir = Some(PathBuf::from(v.trim())),
                _ => {}
            }
        }
        s
    }

    fn to_text(&self) -> String {
        let mut out = String::from("# PSX Boot Inspection Tool settings\n");
        if let Some(d) = &self.soundfonts_dir { out += &format!("soundfonts = {}\n", d.display()) }
        out
    }

    pub fn save(&self) -> Result<(), String> {
        let p = Self::path().ok_or("no settings folder on this system")?;
        if let Some(dir) = p.parent() { std::fs::create_dir_all(dir).map_err(|e| e.to_string())? }
        std::fs::write(&p, self.to_text()).map_err(|e| format!("{}: {e}", p.display()))
    }

    /// The default Soundfonts folder: the platform's data folder (`~/Library/Application
    /// Support/…` on macOS, `~/.local/share/…` on Linux, `%APPDATA%\…` on Windows).
    pub fn default_soundfonts_dir() -> PathBuf {
        dirs::data_dir().unwrap_or_else(std::env::temp_dir).join(APP_DIR).join("Soundfonts")
    }

    /// The Soundfonts folder in use.
    pub fn soundfonts_dir(&self) -> PathBuf { self.soundfonts_dir.clone().unwrap_or_else(Self::default_soundfonts_dir) }
}

/// Opens a folder in the system's file manager (Finder, Explorer, the desktop's own).
pub fn reveal(dir: &Path) -> Result<(), String> {
    let program = if cfg!(target_os = "macos") { "open" } else if cfg!(windows) { "explorer" } else { "xdg-open" };
    std::process::Command::new(program).arg(dir).spawn().map(|_| ()).map_err(|e| format!("{program}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_unknown_keys() {
        let s = Settings { soundfonts_dir: Some(PathBuf::from("/a/b c/Soundfonts")) };
        assert_eq!(Settings::parse(&s.to_text()), s);
        assert_eq!(Settings::parse("# x\nother = 1\nsoundfonts =   \n"), Settings::default());
        assert!(Settings::default().soundfonts_dir().ends_with(Path::new(APP_DIR).join("Soundfonts")));
    }
}
