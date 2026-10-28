//! AIFF, AIFF-C, FLAC and MP3 import through symphonia (WAV goes through hound).

use crate::{ImportError, MonoClip, file_name, to_mono};
use std::path::Path;
use symphonia::core::codecs::audio::{AudioDecoderOptions, CODEC_ID_NULL_AUDIO};
use symphonia::core::errors::Error;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::{MediaSourceStream, MediaSourceStreamOptions};
use symphonia::core::meta::MetadataOptions;

/// The formats symphonia reads for us, recognised by their first bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Format {
    Aiff,
    Flac,
    Mp3,
}

impl Format {
    /// The format from a file's first four bytes; `None` for anything else (WAV and the rest).
    pub(crate) fn sniff(magic: &[u8; 4]) -> Option<Self> {
        match magic {
            b"FORM" => Some(Self::Aiff),
            b"fLaC" => Some(Self::Flac),
            // An ID3 tag, or an MPEG frame sync.
            [b'I', b'D', b'3', _] => Some(Self::Mp3),
            [0xFF, second, ..] if second & 0xE0 == 0xE0 => Some(Self::Mp3),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Aiff => "AIFF",
            Self::Flac => "FLAC",
            Self::Mp3 => "MP3",
        }
    }

    fn extension(self) -> &'static str {
        match self {
            Self::Aiff => "aiff",
            Self::Flac => "flac",
            Self::Mp3 => "mp3",
        }
    }
}

pub(crate) fn load(path: &Path, format: Format) -> Result<MonoClip, ImportError> {
    let err = |e: &dyn std::fmt::Display| ImportError::Decode(format.name(), e.to_string());
    let file = std::fs::File::open(path).map_err(ImportError::Io)?;
    let stream = MediaSourceStream::new(Box::new(file), MediaSourceStreamOptions::default());
    let mut hint = Hint::new();
    hint.with_extension(format.extension());
    let mut reader =
        symphonia::default::get_probe().probe(&hint, stream, FormatOptions::default(), MetadataOptions::default()).map_err(|e| err(&e))?;

    let track = reader.default_track(TrackType::Audio).ok_or_else(|| err(&"geen audiospoor gevonden"))?;
    let track_id = track.id;
    let params = track
        .codec_params
        .as_ref()
        .and_then(|p| p.audio())
        .filter(|p| p.codec != CODEC_ID_NULL_AUDIO)
        .cloned()
        .ok_or_else(|| err(&"geen bruikbaar audioformaat"))?;
    let sample_rate = params.sample_rate.filter(|&r| r > 0).ok_or_else(|| err(&"onbekende samplerate"))?;
    let mut decoder = symphonia::default::get_codecs().make_audio_decoder(&params, &AudioDecoderOptions::default()).map_err(|e| err(&e))?;

    let mut interleaved: Vec<f32> = Vec::new();
    let mut scratch: Vec<f32> = Vec::new();
    let mut channels = 0usize;
    loop {
        let packet = match reader.next_packet() {
            Ok(Some(packet)) => packet,
            Ok(None) => break,
            // A file cut short still plays up to where it ends.
            Err(Error::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(err(&e)),
        };
        if packet.track_id != track_id {
            continue;
        }
        let decoded = decoder.decode(&packet).map_err(|e| err(&e))?;
        let planes = decoded.num_planes();
        if planes == 0 {
            continue;
        }
        channels = planes;
        let needed = decoded.frames() * planes;
        scratch.resize(needed, 0.0);
        decoded.copy_to_slice_interleaved(&mut scratch[..needed]);
        interleaved.extend_from_slice(&scratch[..needed]);
    }
    if interleaved.is_empty() {
        return Err(ImportError::Empty);
    }
    Ok(MonoClip { name: file_name(path), sample_rate, samples: to_mono(&interleaved, channels) })
}

#[cfg(test)]
mod tests {
    use crate::load_audio;
    use std::path::Path;

    /// The sample rate as an 80-bit IEEE extended float, as AIFF's COMM chunk stores it.
    fn extended(rate: u32) -> [u8; 10] {
        let exponent = 31 - rate.leading_zeros();
        let mantissa = u64::from(rate) << (63 - exponent);
        let mut out = [0u8; 10];
        out[..2].copy_from_slice(&((16_383 + exponent) as u16).to_be_bytes());
        out[2..].copy_from_slice(&mantissa.to_be_bytes());
        out
    }

    /// A minimal AIFF file with big-endian integer samples given as full-scale fractions.
    fn write_aiff(path: &Path, rate: u32, channels: u16, bits: u16, frames: &[Vec<f64>]) {
        let bytes_per = usize::from(bits / 8);
        let mut sound = vec![0u8; 8]; // offset and block size
        for frame in frames {
            for &v in frame {
                let max = (1i64 << (bits - 1)) as f64;
                let int = (v * max).round().clamp(-max, max - 1.0) as i64;
                sound.extend_from_slice(&int.to_be_bytes()[8 - bytes_per..]);
            }
        }
        let mut comm = Vec::new();
        comm.extend_from_slice(&channels.to_be_bytes());
        comm.extend_from_slice(&(frames.len() as u32).to_be_bytes());
        comm.extend_from_slice(&bits.to_be_bytes());
        comm.extend_from_slice(&extended(rate));
        let mut body = b"AIFF".to_vec();
        for (id, data) in [(b"COMM", &comm), (b"SSND", &sound)] {
            body.extend_from_slice(id);
            body.extend_from_slice(&(data.len() as u32).to_be_bytes());
            body.extend_from_slice(data);
        }
        let mut file = b"FORM".to_vec();
        file.extend_from_slice(&(body.len() as u32).to_be_bytes());
        file.extend_from_slice(&body);
        std::fs::write(path, file).unwrap_or_else(|e| panic!("{e}"));
    }

    #[test]
    fn stereo_16_bit_44_1_khz() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let path = dir.path().join("gitaar.aif");
        let frames: Vec<Vec<f64>> = (0..1_000).map(|_| vec![0.5, 0.0]).collect();
        write_aiff(&path, 44_100, 2, 16, &frames);
        let clip = load_audio(&path).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(clip.name, "gitaar");
        assert_eq!(clip.sample_rate, 44_100);
        assert_eq!(clip.samples.len(), 1_000);
        assert!((clip.samples[0] - 0.25).abs() < 1e-3, "{}", clip.samples[0]);
    }

    #[test]
    fn mono_24_bit_48_khz_with_any_extension() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        // Recognised by its contents, so a wrong extension does not matter.
        let path = dir.path().join("zang.wav");
        let frames: Vec<Vec<f64>> = (0..480).map(|n| vec![if n % 2 == 0 { -0.75 } else { 0.75 }]).collect();
        write_aiff(&path, 48_000, 1, 24, &frames);
        let clip = load_audio(&path).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(clip.sample_rate, 48_000);
        assert_eq!(clip.samples.len(), 480);
        assert!((clip.samples[0] + 0.75).abs() < 1e-4 && (clip.samples[1] - 0.75).abs() < 1e-4);
    }

    #[test]
    fn broken_files_give_an_error_not_a_crash() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let path = dir.path().join("kapot.aiff");
        std::fs::write(&path, b"FORM\0\0\0\x04AIFF").unwrap_or_else(|e| panic!("{e}"));
        assert!(load_audio(&path).is_err());
        let empty = dir.path().join("leeg.aif");
        std::fs::write(&empty, b"").unwrap_or_else(|e| panic!("{e}"));
        assert!(load_audio(&empty).is_err());
    }

    fn fixture(name: &str) -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures").join(name)
    }

    /// The frequency of a sine from its rising zero crossings.
    fn hz(samples: &[f32], rate: u32) -> f32 {
        let part = &samples[samples.len() / 10..samples.len() * 9 / 10];
        let crossings = part.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count();
        crossings as f32 * rate as f32 / part.len() as f32
    }

    #[test]
    fn flac_reads_as_mono_at_its_rate() {
        let clip = load_audio(&fixture("sine.flac")).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!((clip.name.as_str(), clip.sample_rate, clip.samples.len()), ("sine", 44_100, 22_050));
        assert!((hz(&clip.samples, 44_100) - 1_000.0).abs() < 10.0);
    }

    #[test]
    fn mp3_reads_as_mono_at_its_rate() {
        let clip = load_audio(&fixture("sine.mp3")).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(clip.sample_rate, 48_000);
        // MP3 adds a little silence at the start and end (encoder delay and padding).
        assert!(clip.samples.len() >= 24_000 && clip.samples.len() < 27_000, "{}", clip.samples.len());
        assert!((hz(&clip.samples, 48_000) - 1_000.0).abs() < 10.0);
    }

    #[test]
    fn formats_are_recognised_by_their_first_bytes() {
        use super::Format;
        assert_eq!(Format::sniff(b"FORM"), Some(Format::Aiff));
        assert_eq!(Format::sniff(b"fLaC"), Some(Format::Flac));
        assert_eq!(Format::sniff(b"ID3\x04"), Some(Format::Mp3));
        assert_eq!(Format::sniff(&[0xFF, 0xFB, 0x90, 0x00]), Some(Format::Mp3));
        assert_eq!(Format::sniff(b"RIFF"), None);
    }
}
