//! Project files, audio import and export.
//!
//! Reading WAV and AIFF files into mono tracks and converting them to the device's sample rate, plus the
//! project folder (see [`session`]). Export follows in phase 7 (see ROADMAP.md).

mod aiff;
pub mod preferences;
pub mod session;
pub mod take;

pub use session::{Backup, OpenedProject, ProjectError, ProjectSettings, RepeatMode, Saved, TrackToSave, TransportSettings};

use std::fmt;
use std::path::Path;

/// A decoded audio file, downmixed to mono, at its own sample rate.
#[derive(Debug, Clone)]
pub struct MonoClip {
    pub name: String,
    pub sample_rate: u32,
    pub samples: Vec<f32>,
}

impl MonoClip {
    pub fn duration_seconds(&self) -> f64 {
        self.samples.len() as f64 / f64::from(self.sample_rate.max(1))
    }

    /// The samples at `rate`, resampled when needed.
    pub fn at_rate(&self, rate: u32) -> Vec<f32> {
        resample(&self.samples, self.sample_rate, rate)
    }
}

#[derive(Debug)]
pub enum ImportError {
    Wav(hound::Error),
    Aiff(String),
    Io(std::io::Error),
    Empty,
}

impl fmt::Display for ImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Wav(e) => write!(f, "WAV-bestand niet leesbaar: {e}"),
            Self::Aiff(e) => write!(f, "AIFF-bestand niet leesbaar: {e}"),
            Self::Io(e) => write!(f, "bestand niet leesbaar: {e}"),
            Self::Empty => write!(f, "het bestand bevat geen audio"),
        }
    }
}

impl std::error::Error for ImportError {}

impl From<hound::Error> for ImportError {
    fn from(e: hound::Error) -> Self {
        Self::Wav(e)
    }
}

/// File extensions the import dialog offers.
pub const AUDIO_EXTENSIONS: [&str; 5] = ["wav", "wave", "aif", "aiff", "aifc"];

/// Read a WAV or AIFF file and average its channels to mono. The format is recognised by the
/// file's first bytes, not its name.
pub fn load_audio(path: &Path) -> Result<MonoClip, ImportError> {
    let mut magic = [0u8; 4];
    {
        use std::io::Read;
        let mut file = std::fs::File::open(path).map_err(ImportError::Io)?;
        // A file shorter than four bytes is not audio; let the WAV reader report it.
        let _ = file.read_exact(&mut magic);
    }
    if &magic == b"FORM" { aiff::load(path) } else { load_wav(path) }
}

fn file_name(path: &Path) -> String {
    path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Average interleaved channels to one.
fn to_mono(interleaved: &[f32], channels: usize) -> Vec<f32> {
    let channels = channels.max(1);
    let gain = 1.0 / channels as f32;
    interleaved.chunks(channels).map(|frame| frame.iter().sum::<f32>() * gain).collect()
}

/// Read a WAV file (8 to 32-bit integer or 32-bit float) and average its channels to mono.
pub fn load_wav(path: &Path) -> Result<MonoClip, ImportError> {
    let reader = hound::WavReader::open(path)?;
    decode(reader, file_name(path))
}

fn decode<R: std::io::Read>(reader: hound::WavReader<R>, name: String) -> Result<MonoClip, ImportError> {
    let spec = reader.spec();
    let channels = usize::from(spec.channels.max(1));
    let interleaved: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.into_samples::<f32>().collect::<Result<_, _>>()?,
        hound::SampleFormat::Int => {
            let scale = 1.0 / (1u64 << (spec.bits_per_sample.clamp(1, 32) - 1)) as f32;
            reader.into_samples::<i32>().map(|s| s.map(|v| v as f32 * scale)).collect::<Result<_, _>>()?
        }
    };
    if interleaved.is_empty() {
        return Err(ImportError::Empty);
    }
    Ok(MonoClip { name, sample_rate: spec.sample_rate, samples: to_mono(&interleaved, channels) })
}

/// Convert between sample rates with 4-point Hermite interpolation. Good enough for monitoring;
/// a band-limited resampler can replace it for export (phase 7).
pub fn resample(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || from == 0 || to == 0 || input.is_empty() {
        return input.to_vec();
    }
    let step = f64::from(from) / f64::from(to);
    let len = ((input.len() as f64) / step).floor() as usize;
    let at = |i: isize| -> f32 { input.get(i.clamp(0, input.len() as isize - 1) as usize).copied().unwrap_or(0.0) };
    (0..len)
        .map(|n| {
            let pos = n as f64 * step;
            let i = pos.floor() as isize;
            let t = (pos - pos.floor()) as f32;
            let (y0, y1, y2, y3) = (at(i - 1), at(i), at(i + 1), at(i + 2));
            let c1 = 0.5 * (y2 - y0);
            let c2 = y0 - 2.5 * y1 + 2.0 * y2 - 0.5 * y3;
            let c3 = 0.5 * (y3 - y0) + 1.5 * (y1 - y2);
            ((c3 * t + c2) * t + c1) * t + y1
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn wav(spec: hound::WavSpec, write: impl FnOnce(&mut hound::WavWriter<&mut Cursor<Vec<u8>>>)) -> Vec<u8> {
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut writer = hound::WavWriter::new(&mut cursor, spec).unwrap_or_else(|e| panic!("{e}"));
            write(&mut writer);
            writer.finalize().unwrap_or_else(|e| panic!("{e}"));
        }
        cursor.into_inner()
    }

    fn read(bytes: Vec<u8>) -> MonoClip {
        let reader = hound::WavReader::new(Cursor::new(bytes)).unwrap_or_else(|e| panic!("{e}"));
        decode(reader, "t".into()).unwrap_or_else(|e| panic!("{e}"))
    }

    #[test]
    fn stereo_16_bit_is_averaged_to_mono() {
        let spec = hound::WavSpec { channels: 2, sample_rate: 44_100, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
        let clip = read(wav(spec, |w| {
            for _ in 0..10 {
                w.write_sample(16_384i16).unwrap_or_else(|e| panic!("{e}"));
                w.write_sample(0i16).unwrap_or_else(|e| panic!("{e}"));
            }
        }));
        assert_eq!(clip.sample_rate, 44_100);
        assert_eq!(clip.samples.len(), 10);
        assert!((clip.samples[0] - 0.25).abs() < 1e-4);
    }

    #[test]
    fn float_and_24_bit_files_are_read() {
        let float = hound::WavSpec { channels: 1, sample_rate: 48_000, bits_per_sample: 32, sample_format: hound::SampleFormat::Float };
        let clip = read(wav(float, |w| w.write_sample(-0.5f32).unwrap_or_else(|e| panic!("{e}"))));
        assert_eq!(clip.samples, vec![-0.5]);

        let int24 = hound::WavSpec { channels: 1, sample_rate: 96_000, bits_per_sample: 24, sample_format: hound::SampleFormat::Int };
        let clip = read(wav(int24, |w| w.write_sample(4_194_304i32).unwrap_or_else(|e| panic!("{e}"))));
        assert!((clip.samples[0] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn resampling_keeps_duration_and_shape() {
        let input: Vec<f32> = (0..44_100).map(|n| (n as f32 * 0.01).sin()).collect();
        let output = resample(&input, 44_100, 48_000);
        assert_eq!(output.len(), 48_000);
        // Sample 48 000 * 0.5 s lines up with input sample 22 050.
        assert!((output[24_000] - input[22_050]).abs() < 1e-3);
        assert_eq!(resample(&input, 48_000, 48_000).len(), input.len());
    }
}
