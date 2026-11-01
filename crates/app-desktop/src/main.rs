//! Desktop entry point of DAW 388: cpal audio output (CoreAudio on macOS, WASAPI on Windows),
//! native file dialogs, the preferences file and the eframe window.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
#[cfg(target_os = "macos")]
mod open_documents;

use audio::{Output, SilentClock};
use cpal::traits::HostTrait;
use daw388_engine::EngineHandle;
use daw388_ui::{DawApp, DeviceInfo, Fetch, OutputInfo, OutputRequest, Platform};
use std::path::{Path, PathBuf};

/// The PDF manual that comes with the app.
const MANUAL: &str = "DAW 388 Manual.pdf";

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
        let dialog = rfd::FileDialog::new().add_filter("Audio (WAV, AIFF)", &daw388_ui::AUDIO_EXTENSIONS).set_title("Load audio");
        if multiple { dialog.pick_files().unwrap_or_default() } else { dialog.pick_file().into_iter().collect() }
    }

    fn pick_project_to_open(&mut self) -> Option<PathBuf> {
        // A `.daw388` package once the app is installed (macOS then shows it as one document);
        // until then, or for projects from before 0.24.0, the project.json inside.
        rfd::FileDialog::new()
            .add_filter("DAW 388 project", &[daw388_project::session::PROJECT_EXTENSION, "json"])
            .set_title("Open project")
            .pick_file()
    }

    fn pick_project_location(&mut self, suggested_name: &str) -> Option<PathBuf> {
        // The chosen name becomes the project package, `Name.daw388`; a typed extension is replaced.
        let path = rfd::FileDialog::new().set_title("Save project as").set_file_name(suggested_name).save_file()?;
        Some(daw388_project::session::package_path(&path))
    }

    fn pick_folder(&mut self, title: &str) -> Option<PathBuf> {
        rfd::FileDialog::new().set_title(title).pick_folder()
    }

    fn reveal(&mut self, dir: &Path) {
        let opener = if cfg!(target_os = "windows") { "explorer" } else { "open" };
        if let Err(e) = std::process::Command::new(opener).arg(dir).spawn() {
            log::warn!("cannot show {}: {e}", dir.display());
        }
    }

    fn open_url(&mut self, url: &str) {
        // `explorer` hands a URL to the default browser on Windows, as `open` does on macOS.
        let opener = if cfg!(target_os = "windows") { "explorer" } else { "open" };
        if let Err(e) = std::process::Command::new(opener).arg(url).spawn() {
            log::warn!("cannot open {url}: {e}");
        }
    }

    fn trash(&mut self, paths: &[PathBuf]) -> Result<(), String> {
        // On macOS through its own file manager: the Finder way asks the user to let DAW 388
        // control the Finder.
        #[cfg(target_os = "macos")]
        let context = {
            use trash::macos::{DeleteMethod, TrashContextExtMacos};
            let mut context = trash::TrashContext::default();
            context.set_delete_method(DeleteMethod::NsFileManager);
            context
        };
        #[cfg(not(target_os = "macos"))]
        let context = trash::TrashContext::default();
        context.delete_all(paths).map_err(|e| e.to_string())
    }

    fn fetcher(&self) -> Option<Fetch> {
        Some(Box::new(|url| {
            let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(std::time::Duration::from_secs(10))).build().into();
            agent
                .get(url)
                .header("User-Agent", concat!("DAW388/", env!("CARGO_PKG_VERSION")))
                .header("Accept", "application/vnd.github+json")
                .call()
                .map_err(|e| e.to_string())?
                .body_mut()
                .read_to_string()
                .map_err(|e| e.to_string())
        }))
    }

    fn manual(&self) -> Option<PathBuf> {
        // In the app's Resources on macOS, next to the program on Windows (see build.yml).
        let exe = std::env::current_exe().ok()?;
        let dir = exe.parent()?;
        let path = if cfg!(target_os = "macos") { dir.join("../Resources").join(MANUAL) } else { dir.join(MANUAL) };
        path.is_file().then_some(path)
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
        std::env::temp_dir().join("daw388-takes")
    }
}

struct DesktopApp(DawApp);

impl eframe::App for DesktopApp {
    fn ui(&mut self, ui: &mut eframe::egui::Ui, _frame: &mut eframe::Frame) {
        #[cfg(target_os = "macos")]
        self.0.open_documents(open_documents::take());
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
            .with_min_inner_size([720.0, 480.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };
    // A project folder or project.json on the command line opens it; audio files go onto tracks 1 to 8.
    let files: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    // The Finder does not use the command line: a double-clicked project arrives as an open event.
    #[cfg(target_os = "macos")]
    open_documents::install();
    eframe::run_native(
        "DAW 388",
        options,
        Box::new(move |cc| {
            #[cfg(target_os = "macos")]
            open_documents::set_context(&cc.egui_ctx);
            let mut app = DawApp::new(&cc.egui_ctx, Box::new(Desktop::new()));
            app.open_arguments(files);
            Ok(Box::new(DesktopApp(app)))
        }),
    )
}
