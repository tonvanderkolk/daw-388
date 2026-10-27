//! App-wide preferences (not per project): the audio output the user chose. Stored as
//! `settings.json` in the platform's configuration folder; a missing or unreadable file gives the
//! defaults.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Preferences {
    /// The chosen output device; `None` follows the system's default device.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_device: Option<String>,
    /// The chosen sample rate; `None` uses the device's own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sample_rate: Option<u32>,
    /// The chosen buffer size in frames; `None` uses the device's default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub buffer_frames: Option<u32>,
}

/// Read the preferences, or the defaults when there are none yet or the file is damaged.
pub fn load(path: &Path) -> Preferences {
    fs::read_to_string(path).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default()
}

/// Write the preferences, creating the folder when needed.
pub fn save(path: &Path, preferences: &Preferences) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let json = serde_json::to_string_pretty(preferences).map_err(std::io::Error::other)?;
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, json)?;
    fs::rename(&temporary, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_defaults() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let path = dir.path().join("DAW 388").join("settings.json");
        assert_eq!(load(&path), Preferences::default(), "no file yet");
        let preferences = Preferences { output_device: Some("Scarlett 2i2".into()), sample_rate: Some(48_000), buffer_frames: Some(128) };
        save(&path, &preferences).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(load(&path), preferences);
        fs::write(&path, "{ kapot").unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(load(&path), Preferences::default(), "a damaged file gives the defaults");
    }
}
