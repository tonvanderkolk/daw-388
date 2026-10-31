//! App-wide preferences (not per project): the audio output the user chose. Stored as
//! `settings.json` in the platform's configuration folder; a missing or unreadable file gives the
//! defaults.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    /// The input device to record from; `None`: no input (nothing is opened, so no microphone
    /// prompt until the user chooses one).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_device: Option<String>,
    /// Hear the input through the app while recording (off: the interface's direct monitoring).
    #[serde(default = "yes")]
    pub software_monitoring: bool,
    /// Extra latency compensation in milliseconds on top of what the device reports.
    #[serde(default)]
    pub latency_offset_ms: f32,
    /// The interface channels (from 1) for the AUX and EFF masters; `None`: not sent anywhere.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aux_output: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eff_output: Option<u16>,
    /// Projects opened or saved lately, newest first (File, Open Recent).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recent: Vec<std::path::PathBuf>,
    /// The compact console (View, Compact console); `None`: chosen at startup from the screen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compact_console: Option<bool>,
    /// Ask GitHub for a newer version at startup, at most once a day.
    #[serde(default = "yes")]
    pub check_updates: bool,
    /// When the last check was (seconds since 1970) and the latest version it found.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub update_checked_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_version: Option<String>,
}

/// How many projects File, Open Recent lists.
pub const RECENT: usize = 8;

impl Preferences {
    /// Put `dir` at the top of the recent projects, once, keeping at most [`RECENT`].
    pub fn remember(&mut self, dir: &Path) {
        self.recent.retain(|p| p != dir);
        self.recent.insert(0, dir.to_path_buf());
        self.recent.truncate(RECENT);
    }
}

fn yes() -> bool {
    true
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            output_device: None,
            sample_rate: None,
            buffer_frames: None,
            input_device: None,
            software_monitoring: true,
            latency_offset_ms: 0.0,
            aux_output: None,
            eff_output: None,
            recent: Vec::new(),
            compact_console: None,
            check_updates: true,
            update_checked_at: None,
            latest_version: None,
        }
    }
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
        let preferences = Preferences {
            output_device: Some("Scarlett 2i2".into()),
            sample_rate: Some(48_000),
            buffer_frames: Some(128),
            input_device: Some("Scarlett 2i2".into()),
            software_monitoring: false,
            latency_offset_ms: 1.5,
            aux_output: Some(3),
            eff_output: Some(4),
            recent: vec!["/music/Song.daw388".into()],
            compact_console: Some(true),
            check_updates: false,
            update_checked_at: Some(1_790_000_000),
            latest_version: Some("0.31.0".into()),
        };
        save(&path, &preferences).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(load(&path), preferences);
        fs::write(&path, "{ kapot").unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(load(&path), Preferences::default(), "a damaged file gives the defaults");
        // A file from before the input settings existed keeps monitoring on.
        fs::write(&path, r#"{"output_device":"X"}"#).unwrap_or_else(|e| panic!("{e}"));
        assert!(load(&path).software_monitoring);
        assert_eq!((load(&path).aux_output, load(&path).eff_output), (None, None), "no AUX or EFF output yet");
        assert_eq!(load(&path).compact_console, None, "the screen chooses the console");
        assert!(load(&path).check_updates, "checking for updates is on unless switched off");
    }

    #[test]
    fn recent_projects_are_newest_first_once_and_at_most_eight() {
        let mut preferences = Preferences::default();
        for n in 0..10 {
            preferences.remember(Path::new(&format!("/p/{n}.daw388")));
        }
        preferences.remember(Path::new("/p/5.daw388"));
        assert_eq!(preferences.recent.len(), RECENT);
        assert_eq!(preferences.recent[0], Path::new("/p/5.daw388"));
        assert_eq!(preferences.recent[1], Path::new("/p/9.daw388"));
        assert_eq!(preferences.recent.iter().filter(|p| p.ends_with("5.daw388")).count(), 1);
    }
}
