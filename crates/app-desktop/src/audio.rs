//! Audio output through cpal: device lists with what each device offers, choosing a stream
//! configuration (sample rate, buffer size, sample format), noticing when the device goes away,
//! and a silent clock that keeps the engine running when there is no output at all.
//!
//! Device lookup by name with a fallback to the default device, and ranking sample formats so the
//! callback can convert them, follow the approach of SoundCraft's `playback/devices.rs`
//! (MIT OR Apache-2.0, https://github.com/storytold/soundcraft).

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use daw388_engine::{Engine, EngineHandle};
use daw388_ui::{DeviceInfo, OutputInfo, OutputRequest};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// Scratch room for one callback; larger device buffers are rendered in chunks.
const SCRATCH_FRAMES: usize = 8192;
/// Sample rates offered in the settings, when the device supports them.
const STANDARD_RATES: [u32; 6] = [44_100, 48_000, 88_200, 96_000, 176_400, 192_000];
/// Frames per block of the silent clock.
const SILENT_BLOCK: usize = 512;

pub fn device_name(device: &cpal::Device) -> Option<String> {
    device.description().ok().map(|d| d.name().to_owned())
}

/// Preference of the sample formats the callback converts: lower is better.
fn format_rank(format: cpal::SampleFormat) -> Option<u8> {
    match format {
        cpal::SampleFormat::F32 => Some(0),
        cpal::SampleFormat::I32 => Some(1),
        cpal::SampleFormat::I16 => Some(2),
        cpal::SampleFormat::U16 => Some(3),
        _ => None,
    }
}

/// What a device offers, for the settings window.
pub fn describe(device: &cpal::Device) -> Option<DeviceInfo> {
    let name = device_name(device)?;
    let ranges: Vec<_> = device.supported_output_configs().ok()?.filter(|c| format_rank(c.sample_format()).is_some()).collect();
    let sample_rates = STANDARD_RATES.iter().copied().filter(|&r| ranges.iter().any(|c| c.contains_rate(r))).collect();
    let mut buffer_range: Option<(u32, u32)> = None;
    for range in &ranges {
        if let cpal::SupportedBufferSize::Range { min, max } = *range.buffer_size() {
            buffer_range = Some(buffer_range.map_or((min, max), |(a, b)| (a.min(min), b.max(max))));
        }
    }
    Some(DeviceInfo { name, sample_rates, buffer_range })
}

/// The stream configuration for a requested sample rate: a usable format at that rate with the
/// default's channel count if possible (else the nearest count), or the device default when the
/// rate is not offered or none was asked for.
pub fn pick_config(
    ranges: &[cpal::SupportedStreamConfigRange],
    default: &cpal::SupportedStreamConfig,
    rate: Option<u32>,
) -> cpal::SupportedStreamConfig {
    let Some(rate) = rate.filter(|&r| r != default.sample_rate()) else { return *default };
    let channels = default.channels().max(2);
    ranges
        .iter()
        .filter(|c| c.contains_rate(rate))
        .filter_map(|c| format_rank(c.sample_format()).map(|rank| (rank, c)))
        .min_by_key(|(rank, c)| (c.channels().abs_diff(channels), *rank))
        .and_then(|(_, c)| (*c).try_with_sample_rate(rate))
        .unwrap_or(*default)
}

/// The buffer size to ask for: the requested one when the device's range allows it.
pub fn buffer_size(want: Option<u32>, supported: &cpal::SupportedBufferSize) -> cpal::BufferSize {
    match (want, supported) {
        (Some(n), cpal::SupportedBufferSize::Range { min, max }) if (*min..=*max).contains(&n) => cpal::BufferSize::Fixed(n),
        (Some(n), cpal::SupportedBufferSize::Unknown) if (16..=8192).contains(&n) => cpal::BufferSize::Fixed(n),
        _ => cpal::BufferSize::Default,
    }
}

/// What keeps sound coming: a cpal stream or the silent clock. Dropping it stops it.
pub enum Output {
    Stream {
        // Held only to keep the stream running.
        _stream: cpal::Stream,
        lost: Arc<AtomicBool>,
    },
    Silent {
        // Held only to keep the clock running.
        _clock: SilentClock,
    },
}

impl Output {
    /// The device went away (or the system invalidated the stream) since it was opened.
    pub fn lost(&self) -> bool {
        match self {
            Self::Stream { lost, .. } => lost.load(Ordering::Relaxed),
            Self::Silent { .. } => false,
        }
    }
}

/// Build an output stream that renders the engine's mix as f32 and converts it to `T`.
fn build_stream<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    mut engine: Engine,
    lost: Arc<AtomicBool>,
) -> Result<cpal::Stream, cpal::Error>
where
    T: cpal::SizedSample + cpal::FromSample<f32>,
{
    let channels = usize::from(config.channels.max(1));
    let mut scratch = vec![0.0f32; SCRATCH_FRAMES * channels];
    device.build_output_stream(
        config,
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            for out in data.chunks_mut(scratch.len()) {
                let buffer = &mut scratch[..out.len()];
                engine.render(buffer, channels);
                for (o, s) in out.iter_mut().zip(buffer.iter()) {
                    *o = T::from_sample(*s);
                }
            }
        },
        move |e: cpal::Error| {
            // A device that is gone, or a stream the system says must be rebuilt, needs a new
            // stream; a rerouted default device and an xrun keep playing.
            let kind = e.kind();
            if kind == cpal::ErrorKind::DeviceNotAvailable || kind == cpal::ErrorKind::StreamInvalidated {
                lost.store(true, Ordering::Relaxed);
            }
            log::warn!("audio stream error: {e}");
        },
        None,
    )
}

/// The named output device, else the default one.
fn find_device(host: &cpal::Host, name: Option<&str>) -> Option<cpal::Device> {
    if let Some(want) = name {
        let found = host.output_devices().ok().and_then(|mut ds| ds.find(|d| device_name(d).as_deref() == Some(want)));
        if found.is_some() {
            return found;
        }
        log::warn!("audio device `{want}` is not available");
        return None;
    }
    host.default_output_device()
}

/// Open the requested device. Unlike the default-device fallback of earlier versions, a named
/// device that is missing is an error: the UI decides what to fall back to and says so.
pub fn open(host: &cpal::Host, request: &OutputRequest) -> Result<(EngineHandle, OutputInfo, Output), String> {
    let device = find_device(host, request.device.as_deref()).ok_or_else(|| match &request.device {
        Some(name) => format!("{name} niet gevonden"),
        None => "geen uitvoerapparaat gevonden".to_string(),
    })?;
    let name = device_name(&device).unwrap_or_else(|| "Audioapparaat".into());
    let default = device.default_output_config().map_err(|e| e.to_string())?;
    let ranges: Vec<_> = device.supported_output_configs().map(Iterator::collect).unwrap_or_default();
    let supported = pick_config(&ranges, &default, request.sample_rate);
    let format = supported.sample_format();
    let mut config = supported.config();
    config.buffer_size = buffer_size(request.buffer_frames, supported.buffer_size());
    let buffer_frames = match config.buffer_size {
        cpal::BufferSize::Fixed(n) => Some(n),
        cpal::BufferSize::Default => None,
    };
    let (handle, engine) = daw388_engine::create(config.sample_rate);
    let lost = Arc::new(AtomicBool::new(false));
    let built = match format {
        cpal::SampleFormat::F32 => build_stream::<f32>(&device, config, engine, Arc::clone(&lost)),
        cpal::SampleFormat::I32 => build_stream::<i32>(&device, config, engine, Arc::clone(&lost)),
        cpal::SampleFormat::I16 => build_stream::<i16>(&device, config, engine, Arc::clone(&lost)),
        cpal::SampleFormat::U16 => build_stream::<u16>(&device, config, engine, Arc::clone(&lost)),
        other => return Err(format!("sampleformaat {other} wordt nog niet ondersteund")),
    };
    let stream = built.map_err(|e| e.to_string())?;
    stream.play().map_err(|e| e.to_string())?;
    log::info!("audio output: {name}, {} Hz, buffer {:?}, {} channels, {format}", handle.sample_rate(), buffer_frames, config.channels);
    let info = OutputInfo { device: name, sample_rate: handle.sample_rate(), buffer_frames };
    Ok((handle, info, Output::Stream { _stream: stream, lost }))
}

/// Runs the engine in real time without a sound card, so the transport, counter and meters keep
/// working; the audio goes nowhere.
pub struct SilentClock {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl SilentClock {
    pub fn start(sample_rate: u32) -> (EngineHandle, Self) {
        let (handle, mut engine) = daw388_engine::create(sample_rate);
        let stop = Arc::new(AtomicBool::new(false));
        let running = Arc::clone(&stop);
        let block = Duration::from_secs_f64(SILENT_BLOCK as f64 / f64::from(sample_rate.max(1)));
        let thread = std::thread::Builder::new()
            .name("daw388-silent-clock".into())
            .spawn(move || {
                let mut buffer = vec![0.0f32; SILENT_BLOCK * 2];
                let mut next = Instant::now();
                while !running.load(Ordering::Relaxed) {
                    engine.render(&mut buffer, 2);
                    next += block;
                    if let Some(wait) = next.checked_duration_since(Instant::now()) {
                        std::thread::sleep(wait);
                    } else {
                        // Fell behind (the machine slept): carry on from now.
                        next = Instant::now();
                    }
                }
            })
            .map_err(|e| log::error!("cannot start the silent clock: {e}"))
            .ok();
        (handle, Self { stop, thread })
    }
}

impl Drop for SilentClock {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cpal::{SampleFormat, SupportedBufferSize, SupportedStreamConfig, SupportedStreamConfigRange};

    fn range(channels: u16, min: u32, max: u32, format: SampleFormat) -> SupportedStreamConfigRange {
        SupportedStreamConfigRange::new(channels, min, max, SupportedBufferSize::Range { min: 32, max: 4096 }, format)
    }

    fn default_config() -> SupportedStreamConfig {
        SupportedStreamConfig::new(2, 44_100, SupportedBufferSize::Range { min: 32, max: 4096 }, SampleFormat::F32)
    }

    #[test]
    fn picks_the_requested_rate_in_the_best_format() {
        let ranges = [
            range(2, 44_100, 96_000, SampleFormat::I16),
            range(2, 44_100, 48_000, SampleFormat::F32),
            range(8, 44_100, 96_000, SampleFormat::F32),
        ];
        let picked = pick_config(&ranges, &default_config(), Some(48_000));
        assert_eq!((picked.sample_rate(), picked.channels(), picked.sample_format()), (48_000, 2, SampleFormat::F32));
        // 96 kHz only in I16 at two channels, or F32 at eight: the channel count wins.
        let picked = pick_config(&ranges, &default_config(), Some(96_000));
        assert_eq!((picked.sample_rate(), picked.channels(), picked.sample_format()), (96_000, 2, SampleFormat::I16));
    }

    #[test]
    fn falls_back_to_the_default_when_the_rate_is_not_offered() {
        let ranges = [range(2, 44_100, 48_000, SampleFormat::F32)];
        assert_eq!(pick_config(&ranges, &default_config(), Some(192_000)).sample_rate(), 44_100);
        assert_eq!(pick_config(&ranges, &default_config(), None).sample_rate(), 44_100);
    }

    #[test]
    fn buffer_size_only_when_the_device_allows_it() {
        let range = SupportedBufferSize::Range { min: 64, max: 1024 };
        assert_eq!(buffer_size(Some(128), &range), cpal::BufferSize::Fixed(128));
        assert_eq!(buffer_size(Some(32), &range), cpal::BufferSize::Default);
        assert_eq!(buffer_size(None, &range), cpal::BufferSize::Default);
    }

    #[test]
    fn silent_clock_runs_the_transport_in_real_time() {
        let (mut handle, clock) = SilentClock::start(48_000);
        handle.send(daw388_engine::Command::SetTrack(
            0,
            Some(Arc::new(daw388_engine::TrackAudio { name: "t".into(), samples: vec![0.1; 480_000] })),
        ));
        handle.play();
        std::thread::sleep(Duration::from_millis(200));
        let position = handle.position_seconds();
        assert!(position > 0.1 && position < 0.4, "about real time: {position}");
        drop(clock);
    }
}
