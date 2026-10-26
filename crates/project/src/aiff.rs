//! AIFF and AIFF-C import through symphonia (uncompressed PCM, 8 to 32-bit integer and float).

use crate::{ImportError, MonoClip, file_name, to_mono};
use std::path::Path;
use symphonia::core::codecs::audio::{AudioDecoderOptions, CODEC_ID_NULL_AUDIO};
use symphonia::core::errors::Error;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::{MediaSourceStream, MediaSourceStreamOptions};
use symphonia::core::meta::MetadataOptions;

fn err(e: impl std::fmt::Display) -> ImportError {
    ImportError::Aiff(e.to_string())
}

pub(crate) fn load(path: &Path) -> Result<MonoClip, ImportError> {
    let file = std::fs::File::open(path).map_err(ImportError::Io)?;
    let stream = MediaSourceStream::new(Box::new(file), MediaSourceStreamOptions::default());
    let mut hint = Hint::new();
    hint.with_extension("aiff");
    let mut reader =
        symphonia::default::get_probe().probe(&hint, stream, FormatOptions::default(), MetadataOptions::default()).map_err(err)?;

    let track = reader.default_track(TrackType::Audio).ok_or_else(|| err("geen audiospoor gevonden"))?;
    let track_id = track.id;
    let params = track
        .codec_params
        .as_ref()
        .and_then(|p| p.audio())
        .filter(|p| p.codec != CODEC_ID_NULL_AUDIO)
        .cloned()
        .ok_or_else(|| err("geen bruikbaar audioformaat"))?;
    let sample_rate = params.sample_rate.filter(|&r| r > 0).ok_or_else(|| err("onbekende samplerate"))?;
    let mut decoder = symphonia::default::get_codecs().make_audio_decoder(&params, &AudioDecoderOptions::default()).map_err(err)?;

    let mut interleaved: Vec<f32> = Vec::new();
    let mut scratch: Vec<f32> = Vec::new();
    let mut channels = 0usize;
    loop {
        let packet = match reader.next_packet() {
            Ok(Some(packet)) => packet,
            Ok(None) => break,
            // A file cut short still plays up to where it ends.
            Err(Error::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(err(e)),
        };
        if packet.track_id != track_id {
            continue;
        }
        let decoded = decoder.decode(&packet).map_err(err)?;
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
}
