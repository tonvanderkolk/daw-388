//! Export: a high-quality resampler, WAV and AIFF writers with dither, and file names.

use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Resampler};
use std::fs;
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};

/// The file type of an export.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ExportFormat {
    #[default]
    Wav,
    Aiff,
}

impl ExportFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Wav => "wav",
            Self::Aiff => "aif",
        }
    }
}

/// Sample format of an export.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum BitDepth {
    Int16,
    #[default]
    Int24,
    Float32,
}

/// Resample with a windowed-sinc filter (for export; playback keeps the quicker Hermite).
/// The result has `round(len × to / from)` samples.
pub fn resample_hq(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || from == 0 || to == 0 || input.is_empty() {
        return input.to_vec();
    }
    let wanted = (input.len() as f64 * f64::from(to) / f64::from(from)).round() as usize;
    let Ok(mut resampler) = Fft::<f32>::new(from as usize, to as usize, 1024, 1, FixedSync::Both) else {
        return crate::resample(input, from, to);
    };
    let Ok(adapter) = InterleavedSlice::new(input, 1, input.len()) else {
        return crate::resample(input, from, to);
    };
    match resampler.process_all(&adapter, input.len(), None) {
        Ok(output) => {
            let mut samples = output.take_data();
            samples.resize(wanted, 0.0);
            samples
        }
        Err(_) => crate::resample(input, from, to),
    }
}

/// TPDF dither of ±1 LSB for 16-bit exports: two uniform values per sample, from a fixed seed so
/// an export is repeatable.
struct Dither(u32);

impl Dither {
    fn uniform(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x as f32 / u32::MAX as f32 - 0.5
    }

    fn next(&mut self) -> f32 {
        self.uniform() + self.uniform()
    }
}

/// Integer samples at `bits`, dithered at 16 bits, clamped to full scale.
fn to_int(samples: impl Iterator<Item = f32>, depth: BitDepth) -> Vec<i32> {
    let (scale, dither) = match depth {
        BitDepth::Int16 => (32_767.0f32, true),
        _ => (8_388_607.0f32, false),
    };
    let mut noise = Dither(0x2545_F491);
    samples
        .map(|x| {
            let lsb = x * scale + if dither { noise.next() } else { 0.0 };
            lsb.round().clamp(-scale - 1.0, scale) as i32
        })
        .collect()
}

/// Write `channels` (equal lengths) as WAV or AIFF; through a temporary file, so a failed or
/// cancelled export never leaves half a file.
pub fn write_audio(path: &Path, channels: &[Vec<f32>], rate: u32, format: ExportFormat, depth: BitDepth) -> io::Result<()> {
    let temporary = path.with_extension("part");
    let written = match format {
        ExportFormat::Wav => write_wav(&temporary, channels, rate, depth),
        ExportFormat::Aiff => write_aiff(&temporary, channels, rate, depth),
    };
    match written {
        Ok(()) => fs::rename(&temporary, path),
        Err(e) => {
            let _ = fs::remove_file(&temporary);
            Err(e)
        }
    }
}

fn interleaved(channels: &[Vec<f32>]) -> impl Iterator<Item = f32> + '_ {
    let frames = channels.iter().map(Vec::len).min().unwrap_or(0);
    (0..frames).flat_map(move |i| channels.iter().map(move |c| c[i]))
}

fn write_wav(path: &Path, channels: &[Vec<f32>], rate: u32, depth: BitDepth) -> io::Result<()> {
    let (bits_per_sample, sample_format) = match depth {
        BitDepth::Int16 => (16, hound::SampleFormat::Int),
        BitDepth::Int24 => (24, hound::SampleFormat::Int),
        BitDepth::Float32 => (32, hound::SampleFormat::Float),
    };
    let spec = hound::WavSpec { channels: channels.len() as u16, sample_rate: rate, bits_per_sample, sample_format };
    let mut writer = hound::WavWriter::create(path, spec).map_err(io::Error::other)?;
    if depth == BitDepth::Float32 {
        for x in interleaved(channels) {
            writer.write_sample(x).map_err(io::Error::other)?;
        }
    } else {
        for x in to_int(interleaved(channels), depth) {
            writer.write_sample(x).map_err(io::Error::other)?;
        }
    }
    writer.finalize().map_err(io::Error::other)
}

/// An IEEE 754 80-bit extended number, as AIFF stores its sample rate.
fn extended(value: f64) -> [u8; 10] {
    let mut out = [0u8; 10];
    if value <= 0.0 {
        return out;
    }
    let exponent = value.log2().floor() as i32;
    let mantissa = (value / 2f64.powi(exponent) * 2f64.powi(63)) as u64;
    out[..2].copy_from_slice(&((exponent + 16_383) as u16).to_be_bytes());
    out[2..].copy_from_slice(&mantissa.to_be_bytes());
    out
}

fn write_aiff(path: &Path, channels: &[Vec<f32>], rate: u32, depth: BitDepth) -> io::Result<()> {
    let frames = channels.iter().map(Vec::len).min().unwrap_or(0) as u32;
    let mut sound = Vec::new();
    let bits: u16 = match depth {
        BitDepth::Int16 => {
            for x in to_int(interleaved(channels), depth) {
                sound.extend_from_slice(&(x as i16).to_be_bytes());
            }
            16
        }
        BitDepth::Int24 => {
            for x in to_int(interleaved(channels), depth) {
                sound.extend_from_slice(&x.to_be_bytes()[1..]);
            }
            24
        }
        BitDepth::Float32 => {
            for x in interleaved(channels) {
                sound.extend_from_slice(&x.to_be_bytes());
            }
            32
        }
    };
    let float = depth == BitDepth::Float32;
    let mut comm = Vec::new();
    comm.extend_from_slice(&(channels.len() as u16).to_be_bytes());
    comm.extend_from_slice(&frames.to_be_bytes());
    comm.extend_from_slice(&bits.to_be_bytes());
    comm.extend_from_slice(&extended(f64::from(rate)));
    if float {
        // AIFF-C: compression type and its name as a padded Pascal string.
        comm.extend_from_slice(b"fl32");
        let name = b"32-bit floating point";
        comm.push(name.len() as u8);
        comm.extend_from_slice(name);
        if (name.len() + 1) % 2 == 1 {
            comm.push(0);
        }
    }
    let mut chunks: Vec<(&[u8; 4], Vec<u8>)> = Vec::new();
    if float {
        chunks.push((b"FVER", 0xA280_5140u32.to_be_bytes().to_vec()));
    }
    chunks.push((b"COMM", comm));
    let mut ssnd = vec![0u8; 8];
    ssnd.extend_from_slice(&sound);
    chunks.push((b"SSND", ssnd));
    let body: usize = 4 + chunks.iter().map(|(_, data)| 8 + data.len() + data.len() % 2).sum::<usize>();
    let mut file = BufWriter::new(fs::File::create(path)?);
    file.write_all(b"FORM")?;
    file.write_all(&(body as u32).to_be_bytes())?;
    file.write_all(if float { b"AIFC" } else { b"AIFF" })?;
    for (id, data) in chunks {
        file.write_all(id)?;
        file.write_all(&(data.len() as u32).to_be_bytes())?;
        file.write_all(&data)?;
        if data.len() % 2 == 1 {
            file.write_all(&[0])?;
        }
    }
    file.flush()
}

/// `Projectnaam_mix` or `Projectnaam_01_Kick`, with characters a file system might refuse as `_`.
pub fn export_name(project: &str, track: Option<usize>, clip: Option<&str>) -> String {
    let name = match (track, clip) {
        (None, _) => format!("{project}_mix"),
        (Some(index), Some(clip)) if !clip.is_empty() => format!("{project}_{:02}_{clip}", index + 1),
        (Some(index), _) => format!("{project}_{:02}", index + 1),
    };
    name.chars().map(|c| if c.is_alphanumeric() || " -_.".contains(c) { c } else { '_' }).collect()
}

/// A path in `dir` for `stem.extension` that does not exist yet (`stem-2`, `stem-3`, ...).
pub fn unused_path(dir: &Path, stem: &str, extension: &str) -> PathBuf {
    let first = dir.join(format!("{stem}.{extension}"));
    if !first.exists() {
        return first;
    }
    (2..).map(|n| dir.join(format!("{stem}-{n}.{extension}"))).find(|p| !p.exists()).unwrap_or(first)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resampling_keeps_the_frequency_and_the_length() {
        let input: Vec<f32> = (0..44_100).map(|i| (2.0 * std::f32::consts::PI * 1_000.0 * i as f32 / 44_100.0).sin()).collect();
        let output = resample_hq(&input, 44_100, 48_000);
        assert_eq!(output.len(), 48_000);
        let crossings = output[1_000..47_000].windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count();
        let hz = crossings as f32 * 48_000.0 / 46_000.0;
        assert!((hz - 1_000.0).abs() < 10.0, "{hz}");
        let peak = output[1_000..47_000].iter().fold(0.0f32, |p, s| p.max(s.abs()));
        assert!((peak - 1.0).abs() < 0.02, "the level stays: {peak}");
        assert_eq!(resample_hq(&input, 44_100, 44_100), input);
    }

    fn ramp(frames: usize) -> Vec<f32> {
        (0..frames).map(|i| (i as f32 / frames as f32) * 1.6 - 0.8).collect()
    }

    #[test]
    fn wav_and_aiff_read_back_with_their_rate_and_length() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        for format in [ExportFormat::Wav, ExportFormat::Aiff] {
            for depth in [BitDepth::Int16, BitDepth::Int24, BitDepth::Float32] {
                let path = dir.path().join(format!("t-{depth:?}.{}", format.extension()));
                write_audio(&path, &[ramp(1_000)], 44_100, format, depth).unwrap_or_else(|e| panic!("{e}"));
                let clip = crate::load_audio(&path).unwrap_or_else(|e| panic!("{format:?} {depth:?}: {e}"));
                assert_eq!((clip.sample_rate, clip.samples.len()), (44_100, 1_000), "{format:?} {depth:?}");
                let worst = clip.samples.iter().zip(ramp(1_000)).map(|(a, b)| (a - b).abs()).fold(0.0, f32::max);
                assert!(worst < 2e-4, "{format:?} {depth:?}: {worst}");
                let stereo = dir.path().join(format!("s-{depth:?}.{}", format.extension()));
                write_audio(&stereo, &[ramp(500), ramp(500)], 96_000, format, depth).unwrap_or_else(|e| panic!("{e}"));
                let clip = crate::load_audio(&stereo).unwrap_or_else(|e| panic!("{e}"));
                assert_eq!((clip.sample_rate, clip.samples.len()), (96_000, 500));
            }
        }
        assert!(
            std::fs::read_dir(dir.path())
                .unwrap_or_else(|e| panic!("{e}"))
                .all(|e| { e.map(|e| e.path().extension().is_none_or(|x| x != "part")).unwrap_or(false) })
        );
    }

    #[test]
    fn only_16_bits_are_dithered() {
        let constant = [0.4 / 32_767.0; 2_000];
        let sixteen = to_int(constant.iter().copied(), BitDepth::Int16);
        assert!(sixteen.iter().any(|s| *s != sixteen[0]), "TPDF dither varies");
        let twenty_four = to_int(constant.iter().copied(), BitDepth::Int24);
        assert!(twenty_four.iter().all(|s| *s == twenty_four[0]));
        assert_eq!(to_int([2.0, -2.0].into_iter(), BitDepth::Int24), vec![8_388_607, -8_388_608], "clamped");
    }

    #[test]
    fn export_names_are_safe_and_unused() {
        assert_eq!(export_name("Mijn liedje", None, None), "Mijn liedje_mix");
        assert_eq!(export_name("Demo", Some(0), Some("Kick/Snare")), "Demo_01_Kick_Snare");
        assert_eq!(export_name("Demo", Some(7), None), "Demo_08");
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(unused_path(dir.path(), "a", "wav"), dir.path().join("a.wav"));
        std::fs::write(dir.path().join("a.wav"), b"x").unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(unused_path(dir.path(), "a", "wav"), dir.path().join("a-2.wav"));
    }
}
