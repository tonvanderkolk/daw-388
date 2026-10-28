//! Desktop entry point of DAW 388: cpal audio output (CoreAudio on macOS, WASAPI on Windows),
//! native file dialogs, the preferences file and the eframe window.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;

use audio::{Output, SilentClock};
use cpal::traits::HostTrait;
use daw388_engine::EngineHandle;
use daw388_ui::{DawApp, DeviceInfo, OutputInfo, OutputRequest, Platform};
use std::path::PathBuf;

struct Desktop {
    host: cpal::Host,
    /// Kept alive while the output plays; dropping it closes the stream.
    output: Option<Output>,
}

impl Desktop {
    fn new() -> Self {
        Self { host: cpal::default_host(), output: None }
    }
}

impl Platform for Desktop {
    fn output_devices(&mut self) -> Vec<DeviceInfo> {
        self.host.output_devices().map(|ds| ds.filter_map(|d| audio::describe(&d, true)).collect()).unwrap_or_default()
    }

    fn input_devices(&mut self) -> Vec<DeviceInfo> {
        self.host.input_devices().map(|ds| ds.filter_map(|d| audio::describe(&d, false)).collect()).unwrap_or_default()
    }

    fn default_output_device(&mut self) -> Option<String> {
        self.host.default_output_device().as_ref().and_then(audio::device_name)
    }

    fn open_output(&mut self, request: &OutputRequest) -> Result<(EngineHandle, OutputInfo), String> {
        // Close the old stream first: some systems allow only one open stream per device.
        self.output = None;
        let (handle, info, output) = audio::open(&self.host, request)?;
        self.output = Some(output);
        Ok((handle, info))
    }

    fn open_silent(&mut self, sample_rate: u32) -> EngineHandle {
        self.output = None;
        let (handle, clock) = SilentClock::start(sample_rate);
        self.output = Some(Output::Silent { _clock: clock });
        handle
    }

    fn output_lost(&mut self) -> bool {
        self.output.as_ref().is_some_and(Output::lost)
    }

    fn preferences_path(&self) -> Option<PathBuf> {
        dirs::config_dir().map(|dir| dir.join("DAW 388").join("settings.json"))
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

    fn spawn(&mut self, job: Box<dyn FnOnce() + Send>) {
        if let Err(e) = std::thread::Builder::new().name("daw388-load".into()).spawn(job) {
            log::error!("cannot start a loader thread: {e}");
        }
    }

    fn spawn_service(&mut self, job: Box<dyn FnOnce() + Send>) {
        if let Err(e) = std::thread::Builder::new().name("daw388-recorder".into()).spawn(job) {
            log::error!("cannot start the recording writer: {e}");
        }
    }

    fn temp_dir(&self) -> PathBuf {
        std::env::temp_dir().join("daw388-opnames")
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
            .with_inner_size([1680.0, 950.0])
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
