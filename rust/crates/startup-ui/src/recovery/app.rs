//! Failed-startup display. The isolated worker owns every mutating action.
use super::{layout, touch::Touch, view::View};
use crate::{
    geometry::{Point, Rect},
    Error,
};
use openpilot_carrot_recovery::{
    actions::startup_worker::{self, Worker},
    config::Config,
};
use openpilot_startup_recovery::{
    logs,
    schedule::Schedule,
    touch_log::{TouchLog, Value},
};
use std::{
    env, fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

#[derive(Clone, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Paths {
    pub repository: PathBuf,
    pub data: PathBuf,
    pub temporary: PathBuf,
    pub system: PathBuf,
}
impl Default for Paths {
    fn default() -> Self {
        Self {
            repository: ["OPENPILOT_ROOT", "BASEDIR"]
                .into_iter()
                .find_map(env::var_os)
                .map(PathBuf::from)
                .unwrap_or_else(|| "/data/openpilot".into()),
            data: "/data".into(),
            temporary: "/tmp".into(),
            system: "/".into(),
        }
    }
}
pub struct Options {
    pub paths: Paths,
    pub reason: String,
    pub log: Option<PathBuf>,
    pub preview: Option<PathBuf>,
    pub capture_log: Option<PathBuf>,
    pub big: bool,
    pub headless: bool,
    pub preview_state: String,
}
impl Options {
    pub fn parse(arguments: impl IntoIterator<Item = std::ffi::OsString>) -> Result<Self, Error> {
        let mut result = Self {
            paths: Paths::default(),
            reason: "Startup failed".into(),
            log: None,
            preview: None,
            capture_log: None,
            big: false,
            headless: false,
            preview_state: "waiting".into(),
        };
        let mut args = arguments.into_iter();
        while let Some(argument) = args.next() {
            if argument == "--headless" {
                result.headless = true;
                continue;
            }
            if argument == "--big" {
                result.big = true;
                continue;
            }
            let value = args
                .next()
                .ok_or(Error::Contract("missing recovery argument value"))?;
            match argument.to_str() {
                Some("--reason") => {
                    result.reason = value
                        .into_string()
                        .map_err(|_| Error::Contract("invalid recovery reason"))?
                }
                Some("--log") => result.log = Some(value.into()),
                Some("--preview") => result.preview = Some(value.into()),
                Some("--capture-log") => result.capture_log = Some(value.into()),
                Some("--root") => result.paths.repository = value.into(),
                Some("--paths") => {
                    result.paths = serde_json::from_slice(&fs::read(PathBuf::from(value))?)
                        .map_err(|_| Error::Contract("invalid recovery paths"))?
                }
                Some("--preview-state") => {
                    result.preview_state = value
                        .into_string()
                        .map_err(|_| Error::Contract("invalid preview state"))?
                }
                _ => return Err(Error::Contract("unexpected recovery argument")),
            }
        }
        if result.capture_log.is_some() && (result.preview.is_some() || result.headless) {
            return Err(Error::Contract(
                "capture, preview and headless actions are exclusive",
            ));
        }
        if !layout::LABELS
            .iter()
            .any(|(state, _, _)| *state == result.preview_state)
        {
            return Err(Error::Contract("invalid preview state"));
        }
        Ok(result)
    }
}
fn monotonic() -> f64 {
    let now = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    now.tv_sec as f64 + now.tv_nsec as f64 / 1e9
}
pub fn device_size(system: &Path) -> (i32, i32) {
    let model =
        fs::read_to_string(system.join("sys/firmware/devicetree/base/model")).unwrap_or_default();
    // Path.read_text uses universal-newline decoding before the source strip.
    let model = model.replace("\r\n", "\n").replace('\r', "\n");
    match model.trim_matches(['\0', '\n']).to_lowercase().as_str() {
        "comma tici" | "comma tizi" | "comma c3" | "comma c3x" => (2160, 1080),
        _ => (536, 240),
    }
}
pub fn wake_display(system: &Path) {
    let panel = system.join("sys/class/backlight/panel0-backlight");
    let write = || -> Result<(), Box<dyn std::error::Error>> {
        fs::write(panel.join("bl_power"), "0")?;
        let openpilot_hardware_info::Number::Integer(maximum) =
            openpilot_hardware_info::parse_integer(&fs::read_to_string(
                panel.join("max_brightness"),
            )?)?
        else {
            return Ok(());
        };
        fs::write(panel.join("brightness"), (maximum >> 1usize).to_string())?;
        Ok(())
    };
    let _ = write();
}
fn backend(system: &Path) -> Result<String, Error> {
    if env::var_os("RAYLIB_PLATFORM").is_some_and(|v| !v.is_empty()) {
        return Err(Error::Contract(
            "RAYLIB_PLATFORM is obsolete; use RAYLIB_BACKEND",
        ));
    }
    if let Ok(value) = env::var("RAYLIB_BACKEND") {
        let value = value.trim_matches(crate::text::whitespace).to_lowercase();
        if !value.is_empty() {
            return if matches!(value.as_str(), "desktop" | "comma" | "headless") {
                Ok(value)
            } else {
                Err(Error::Contract("invalid RAYLIB_BACKEND"))
            };
        }
    }
    Ok(
        if system.join("AGNOS").exists() || system.join("TICI").exists() {
            "comma"
        } else if env::var_os("DISPLAY").is_none_or(|v| v.is_empty()) {
            "headless"
        } else {
            "desktop"
        }
        .into(),
    )
}
fn hit(areas: [[f32; 4]; 3], point: Point) -> &'static str {
    for (name, [x, y, width, height]) in ["rebuild", "update", "error"].into_iter().zip(areas) {
        if (Rect {
            x,
            y,
            width,
            height,
        })
        .contains(point)
        {
            return name;
        }
    }
    "outside"
}
/// Separate construction permits owned-tree GUI integration tests without a vehicle.
pub struct App {
    pub view: View,
    pub worker: Worker,
    touch: Touch,
    log: TouchLog,
    schedule: Schedule,
    comma: bool,
}
impl App {
    pub fn new(options: &Options) -> Result<Self, Error> {
        if env::var("CARROT_STARTUP_RECOVERY").as_deref() != Ok("1") {
            return Err(Error::Contract(
                "recovery requires the failed-startup launcher",
            ));
        }
        let backend = backend(&options.paths.system)?;
        let error = logs::read_error(options.log.as_deref());
        let dimensions = if options.big {
            (2160, 1080)
        } else {
            device_size(&options.paths.system)
        };
        let view = View::new(
            &options.paths.repository,
            dimensions,
            &options.reason,
            if error.is_empty() {
                &options.reason
            } else {
                &error
            },
            false,
            true,
        )?;
        let mut config = Config::original()?;
        config.repository = options.paths.repository.clone();
        config.launcher = options
            .paths
            .repository
            .join("rust/bin/openpilot-process-child");
        config.data = options.paths.data.join("carrot");
        config.params = options.paths.data.join("params");
        config.tmux_log = options.paths.data.join("media/tmux.log");
        let log_path = options.paths.data.join("startup_recovery_touch.jsonl");
        let worker = Worker::new(config).with_touch_log(log_path.clone());
        let touch = Touch::open(
            &options
                .paths
                .system
                .join("dev/input/by-path/platform-894000.i2c-event"),
        );
        let log = TouchLog::for_path(log_path);
        log.record(
            "start",
            true,
            &[
                ("backend", Value::text(&backend)),
                ("width", Value::integer(dimensions.0)),
                ("height", Value::integer(dimensions.1)),
                ("raw_touch", Value::Bool(touch.raw())),
            ],
        );
        let schedule = Schedule::new(monotonic());
        wake_display(&options.paths.system);
        Ok(Self {
            view,
            worker,
            touch,
            log,
            schedule,
            comma: backend == "comma",
        })
    }
    pub fn frame(&mut self, address: &str) -> Result<(), Error> {
        self.worker.poll()?;
        self.schedule
            .poll_with(self.worker.progress().state.busy(), monotonic, || {
                self.worker.start(true, false).map(|_| ())
            })?;
        let state = &self.worker.progress().state;
        let frame = self.view.frame(&state.status, &state.detail, address)?;
        self.view.renderer.raw_begin()?;
        let painted = self.view.paint(&frame);
        self.view.renderer.raw_end();
        painted?;
        if self.comma {
            self.view.renderer.poll_input();
        }
        let mouse = !self.touch.raw() && self.view.renderer.mouse_button_pressed(0);
        if self.touch.pressed(mouse) {
            let point = if self.touch.raw() {
                self.view.renderer.sample(0).0
            } else {
                self.view.renderer.mouse_position()
            };
            self.press(frame.areas, point)?;
        }
        Ok(())
    }
    pub fn press(&mut self, areas: [[f32; 4]; 3], point: Point) -> Result<(), Error> {
        let hit = hit(areas, point);
        self.log.record(
            "press",
            true,
            &[
                ("x", Value::Float(f64::from(point.x))),
                ("y", Value::Float(f64::from(point.y))),
                ("hit", Value::text(hit)),
                ("state", Value::text(&self.worker.progress().state.status)),
            ],
        );
        match hit {
            "rebuild" | "update" => {
                let accepted = self.worker.start(false, hit == "rebuild")?;
                self.log.record(
                    "action",
                    true,
                    &[
                        ("hit", Value::text(hit)),
                        ("accepted", Value::Bool(accepted)),
                        ("state", Value::text(&self.worker.progress().state.status)),
                    ],
                );
            }
            "error" => self.view.next_page(),
            _ => (),
        }
        Ok(())
    }
}
pub fn run() -> Result<(), Error> {
    let options = Options::parse(env::args_os().skip(1))?;
    if let Some(path) = &options.capture_log {
        logs::capture(
            &mut std::io::stdin().lock(),
            &mut std::io::stdout().lock(),
            |bytes| fs::write(path, bytes),
            monotonic,
        )?;
        return Ok(());
    }
    if let Some(path) = &options.preview {
        let error = logs::read_error(options.log.as_deref());
        let dimensions = if options.big {
            (2160, 1080)
        } else {
            device_size(&options.paths.system)
        };
        let mut view = View::new(
            &options.paths.repository,
            dimensions,
            &options.reason,
            if error.is_empty() {
                &options.reason
            } else {
                &error
            },
            true,
            true,
        )?;
        let detail = if options.preview_state == "waiting" {
            "No new commit. Waiting for a fix or network connection."
        } else {
            ""
        };
        view.preview(&options.preview_state, detail, "192.168.0.10:6999", path)?;
        return Ok(());
    }
    if env::var("CARROT_STARTUP_RECOVERY").as_deref() != Ok("1") {
        return Err(Error::Contract(
            "recovery requires the failed-startup launcher",
        ));
    }
    startup_worker::initialize_process()?;
    let stopped = Arc::new(AtomicBool::new(false));
    for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
        signal_hook::flag::register(signal, stopped.clone())?;
    }
    if options.headless {
        let mut config = Config::original()?;
        config.repository = options.paths.repository.clone();
        config.launcher = options
            .paths
            .repository
            .join("rust/bin/openpilot-process-child");
        let mut worker = Worker::new(config);
        worker.start(true, false)?;
        while worker.running() && !stopped.load(Ordering::Relaxed) {
            worker.poll()?;
            std::thread::sleep(Duration::from_millis(25));
        }
        worker.stop()?;
        return Ok(());
    }
    let mut app = App::new(&options)?;
    let address = crate::network::start()?;
    while !stopped.load(Ordering::Relaxed) && !app.view.renderer.should_close() {
        let address = address
            .lock()
            .map_err(|_| Error::Contract("IP monitor poisoned"))?
            .label(6999);
        app.frame(&address)?;
        std::thread::sleep(Duration::from_millis(1));
    }
    app.worker.stop()?;
    Ok(())
}

#[cfg(test)]
mod tests;
