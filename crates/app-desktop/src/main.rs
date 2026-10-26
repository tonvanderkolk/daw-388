//! Desktop entry point of DAW 388: cpal audio output (CoreAudio on macOS, WASAPI on Windows),
//! native file dialogs and the eframe window.
//!
//! Device lookup by name with a fallback to the default device follows the approach of
//! SoundCraft's `playback/devices.rs` (MIT OR Apache-2.0, https://github.com/storytold/soundcraft).

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use daw388_engine::{Engine, EngineHandle};
use daw388_ui::{DawApp, Platform};
use std::path::PathBuf;

/// Scratch room for one callback; larger device buffers are rendered in chunks.
const SCRATCH_FRAMES: usize = 8192;

fn device_name(device: &cpal::Device) -> Option<String> {
    device.description().ok().map(|d| d.name().to_owned())
}

struct Desktop {
    host: cpal::Host,
    /// Kept alive while the output plays; dropping it closes the stream.
    stream: Option<cpal::Stream>,
}

impl Desktop {
    fn new() -> Self {
        Self { host: cpal::default_host(), stream: None }
    }

    /// The named output device, else the default one.
    fn find_device(&self, name: Option<&str>) -> Option<cpal::Device> {
        if let Some(want) = name {
            let found = self.host.output_devices().ok().and_then(|mut ds| ds.find(|d| device_name(d).as_deref() == Some(want)));
            if found.is_some() {
                return found;
            }
            log::warn!("audio device `{want}` is not available; using the default");
        }
        self.host.default_output_device()
    }
}

/// Build an output stream that renders the engine's mix as f32 and converts it to `T`.
fn build_stream<T>(device: &cpal::Device, config: cpal::StreamConfig, mut engine: Engine) -> Result<cpal::Stream, cpal::Error>
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
        |e: cpal::Error| log::warn!("audio stream error: {e}"),
        None,
    )
}

impl Platform for Desktop {
    fn output_devices(&mut self) -> Vec<String> {
        self.host.output_devices().map(|ds| ds.filter_map(|d| device_name(&d)).collect()).unwrap_or_default()
    }

    fn default_output_device(&mut self) -> Option<String> {
        self.host.default_output_device().as_ref().and_then(device_name)
    }

    fn open_output(&mut self, name: Option<&str>) -> Result<(EngineHandle, String), String> {
        self.stream = None;
        let device = self.find_device(name).ok_or("geen uitvoerapparaat gevonden")?;
        let opened = device_name(&device).unwrap_or_else(|| "Audioapparaat".into());
        let supported = device.default_output_config().map_err(|e| e.to_string())?;
        let format = supported.sample_format();
        let config = supported.config();
        let (handle, engine) = daw388_engine::create(config.sample_rate);
        let stream = match format {
            cpal::SampleFormat::F32 => build_stream::<f32>(&device, config, engine),
            cpal::SampleFormat::I16 => build_stream::<i16>(&device, config, engine),
            cpal::SampleFormat::I32 => build_stream::<i32>(&device, config, engine),
            cpal::SampleFormat::U16 => build_stream::<u16>(&device, config, engine),
            other => return Err(format!("sampleformaat {other} wordt nog niet ondersteund")),
        }
        .map_err(|e| e.to_string())?;
        stream.play().map_err(|e| e.to_string())?;
        log::info!("audio output: {opened}, {} Hz, {} channels, {format}", handle.sample_rate(), config.channels);
        self.stream = Some(stream);
        Ok((handle, opened))
    }

    fn pick_audio_files(&mut self, multiple: bool) -> Vec<PathBuf> {
        let dialog = rfd::FileDialog::new().add_filter("Audio (WAV, AIFF)", &daw388_ui::AUDIO_EXTENSIONS).set_title("Audio laden");
        if multiple { dialog.pick_files().unwrap_or_default() } else { dialog.pick_file().into_iter().collect() }
    }

    fn pick_project_to_open(&mut self) -> Option<PathBuf> {
        rfd::FileDialog::new().add_filter("DAW 388-project", &["json"]).set_title("Project openen (kies project.json)").pick_file()
    }

    fn pick_project_location(&mut self, suggested_name: &str) -> Option<PathBuf> {
        // The chosen name becomes the project folder; a typed extension is dropped.
        let path = rfd::FileDialog::new().set_title("Project opslaan als").set_file_name(suggested_name).save_file()?;
        Some(if path.extension().is_some() { path.with_extension("") } else { path })
    }
}

struct DesktopApp(DawApp);

impl eframe::App for DesktopApp {
    fn ui(&mut self, ui: &mut eframe::egui::Ui, _frame: &mut eframe::Frame) {
        self.0.ui(ui);
    }
}

fn main() -> eframe::Result {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info,symphonia=warn")).init();
    let options = eframe::NativeOptions {
        // winit's default macOS menu quits with Cmd+Q through `terminate:`, which ends the app
        // without a close request and so skips the "save changes?" question. Without that menu,
        // Cmd+Q reaches the UI, which asks first.
        #[cfg(target_os = "macos")]
        event_loop_builder: Some(Box::new(|builder| {
            use winit::platform::macos::EventLoopBuilderExtMacOS;
            builder.with_default_menu(false);
        })),
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("DAW 388")
            .with_inner_size([1440.0, 560.0])
            .with_min_inner_size([720.0, 480.0]),
        ..Default::default()
    };
    // A project folder or project.json on the command line opens it; audio files go onto tracks 1 to 8.
    let files: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    eframe::run_native(
        "DAW 388",
        options,
        Box::new(move |cc| {
            let mut app = DawApp::new(&cc.egui_ctx, Box::new(Desktop::new()));
            app.open_arguments(files);
            Ok(Box::new(DesktopApp(app)))
        }),
    )
}
