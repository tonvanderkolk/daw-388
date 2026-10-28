//! Recorded takes: their file names and writing them as 24-bit WAV files.

use std::path::{Path, PathBuf};

/// Write mono audio as a 24-bit WAV file (atomically: a temporary file, then renamed).
pub fn write_wav_24(path: &Path, sample_rate: u32, samples: &[f32]) -> Result<(), hound::Error> {
    let spec = hound::WavSpec { channels: 1, sample_rate, bits_per_sample: 24, sample_format: hound::SampleFormat::Int };
    let temporary = path.with_extension("wav.tmp");
    {
        let mut writer = hound::WavWriter::create(&temporary, spec)?;
        let scale = 8_388_607.0f32;
        for sample in samples {
            writer.write_sample((sample.clamp(-1.0, 1.0) * scale).round() as i32)?;
        }
        writer.finalize()?;
    }
    std::fs::rename(&temporary, path)?;
    Ok(())
}

/// A free file name for a new take of track `number` in `audio_dir`: `01_Kick_opname1.wav`,
/// `01_Kick_opname2.wav` and so on. `base` is the track's current name; an earlier
/// `_opnameN` suffix is dropped so takes do not pile up suffixes.
pub fn take_path(audio_dir: &Path, number: usize, base: Option<&str>) -> PathBuf {
    let base = base.map(strip_take_suffix).filter(|b| !b.is_empty());
    let stem = match base {
        Some(base) if base.starts_with(&format!("{number:02}_")) => base.to_string(),
        Some(base) => format!("{number:02}_{base}"),
        None => format!("{number:02}_Spoor{number}"),
    };
    (1..)
        .map(|n| audio_dir.join(format!("{stem}_opname{n}.wav")))
        .find(|p| !p.exists())
        .unwrap_or_else(|| audio_dir.join(format!("{stem}_opname.wav")))
}

fn strip_take_suffix(name: &str) -> &str {
    match name.rfind("_opname") {
        Some(at) if name[at + 7..].chars().all(|c| c.is_ascii_digit()) => &name[..at],
        _ => name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_24_bit_wav_that_reads_back() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let path = dir.path().join("t.wav");
        write_wav_24(&path, 48_000, &[0.5, -0.25, 2.0]).unwrap_or_else(|e| panic!("{e}"));
        let clip = crate::load_audio(&path).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(clip.sample_rate, 48_000);
        assert!((clip.samples[0] - 0.5).abs() < 1e-6 && (clip.samples[1] + 0.25).abs() < 1e-6);
        assert!((clip.samples[2] - 1.0).abs() < 1e-6, "clipped to full scale");
        let reader = hound::WavReader::open(&path).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(reader.spec().bits_per_sample, 24);
    }

    #[test]
    fn take_names_count_up_and_do_not_stack() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let first = take_path(dir.path(), 1, Some("01_Kick"));
        assert_eq!(first.file_name().and_then(|n| n.to_str()), Some("01_Kick_opname1.wav"));
        std::fs::write(&first, b"").unwrap_or_else(|e| panic!("{e}"));
        let second = take_path(dir.path(), 1, Some("01_Kick_opname1"));
        assert_eq!(second.file_name().and_then(|n| n.to_str()), Some("01_Kick_opname2.wav"));
        let fresh = take_path(dir.path(), 3, None);
        assert_eq!(fresh.file_name().and_then(|n| n.to_str()), Some("03_Spoor3_opname1.wav"));
        let named = take_path(dir.path(), 2, Some("Gitaar"));
        assert_eq!(named.file_name().and_then(|n| n.to_str()), Some("02_Gitaar_opname1.wav"));
    }
}
