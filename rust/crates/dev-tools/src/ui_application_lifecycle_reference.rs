//! Independent original GuiApplication lifecycle reference; no candidate widgets.
use crate::{
    seal::Result,
    ui_application_lifecycle_loop::{
        Backend, Config, Event, Loop, LoopError, Mouse, MouseEvent, Outcome,
    },
    ui_application_lifecycle_raw::Raw,
    ui_application_lifecycle_recording::{
        dynamic_command, global_command, Encoder, GlobalOptions, SubmitError,
    },
    ui_forms_input::{hex, Canvas, Input, InputFrame, Rect},
    ui_forms_reference::controls,
};
use num_bigint::BigInt;
use std::{
    collections::{BTreeMap, VecDeque},
    ffi::{OsStr, OsString},
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
pub struct Options {
    pub root: PathBuf,
    pub mode: String,
    pub output: PathBuf,
    pub raw_helper: PathBuf,
    pub raylib_library: PathBuf,
}
/// Ordered owners mirror the original dictionaries; clear only after a complete
/// resource group unloads, so a failed callback leaves the original partial state.
pub struct Resources {
    pub ready: bool,
    pub textures: Vec<(String, String)>,
    pub fonts: Vec<(String, String)>,
    pub target: Option<String>,
    pub shader: Option<String>,
}
pub fn close_resources(
    resources: &mut Resources,
    call: &mut dyn FnMut(&str, Option<&str>) -> Result<()>,
) -> Result<()> {
    if !resources.ready {
        return Ok(());
    }
    for (_, value) in &resources.textures {
        call("texture", Some(value))?;
    }
    resources.textures.clear();
    for (_, value) in &resources.fonts {
        call("font", Some(value))?;
    }
    resources.fonts.clear();
    if let Some(value) = &resources.target {
        call("target", Some(value))?;
        resources.target = None;
    }
    if let Some(value) = &resources.shader {
        call("shader", Some(value))?;
        resources.shader = None;
    }
    call("ffmpeg", None)?;
    call("stop-recording", None)?;
    call("ffmpeg", None)?;
    call("window", None)?;
    resources.ready = false;
    Ok(())
}
/// Original touch overlay policy; the Canvas executes mechanical draw calls.
pub fn draw_touch_points(
    api: &mut dyn Canvas,
    history: &mut VecDeque<(f64, f64, f64)>,
    events: &[MouseEvent],
    now: f64,
) -> Result<()> {
    for event in events {
        if event.pressed {
            history.clear();
        }
        if history.len() == 140 {
            history.pop_front();
        }
        history.push_back((event.pos.x, event.pos.y, now));
    }
    while history.front().is_some_and(|v| now - v.2 > 3.) {
        history.pop_front();
    }
    if let Some(&(x, y, _)) = history.back() {
        api.call(&format!("circle {} {} 15 4281805286", x as i32, y as i32))?;
    }
    let count = history.len();
    for (index, (x, y, _)) in history.iter().enumerate() {
        let fraction = index as f64 / count as f64;
        let color = controls::color(
            (255. * (1.5 - fraction)).min(255.) as u8,
            (255. * (fraction + 0.5)).min(255.) as u8,
            50,
            255,
        );
        api.call(&format!("circle {} {} 5 {color}", *x as i32, *y as i32))?;
    }
    Ok(())
}
pub fn validate_source(root: &Path) -> Result<()> {
    use sha2::{Digest, Sha256};
    let manifest = include_bytes!("../tests/fixtures/ui-application-lifecycle/provenance.json");
    if format!("{:x}", Sha256::digest(manifest))
        != "84c211425182eeffd333474ae619cef7575b73e2cb1270cfa0cdbe4d2d6b8afd"
    {
        return Err("application original provenance changed; independently recapture".into());
    }
    let pins: serde_json::Value = serde_json::from_slice(manifest)?;
    for (name, expected) in pins["sources"]
        .as_object()
        .ok_or("application source pins missing")?
    {
        let bytes = match fs::read(root.join(name)) {
            Ok(bytes) => bytes,
            Err(error)
                if error.kind() == std::io::ErrorKind::NotFound
                    && matches!(
                        name.as_str(),
                        "rust/tools/check_ui_application.py"
                            | "rust/tools/ui_qa/application_source.py"
                    ) =>
            {
                continue
            }
            Err(error) => return Err(error.into()),
        };
        if format!("{:x}", Sha256::digest(bytes))
            != expected.as_str().ok_or("invalid application source pin")?
        {
            return Err(format!("application original source changed: {name}").into());
        }
    }
    let closure = include_bytes!("../tests/fixtures/ui-application-lifecycle/source-closure.json");
    if format!("{:x}", Sha256::digest(closure))
        != "de58975d68e6e186b87f041d20e8fd5c4390f096c4fb3876b556b98e8b036d9c"
    {
        return Err("application import/font provenance changed; independently recapture".into());
    }
    let pins: serde_json::Value = serde_json::from_slice(closure)?;
    for (name, hash) in pins["sources"]
        .as_object()
        .ok_or("application closure sources missing")?
    {
        if Some(format!("{:x}", Sha256::digest(fs::read(root.join(name))?)).as_str())
            != hash.as_str()
        {
            return Err(format!("application dependency changed: {name}").into());
        }
    }
    for name in pins["absent"]
        .as_array()
        .ok_or("application font fallbacks missing")?
    {
        if root
            .join(name.as_str().ok_or("application font fallback path")?)
            .try_exists()?
        {
            return Err("application source font fallback layout changed".into());
        }
    }
    // These bindings make the original's failed Params import succeed, changing
    // language/device initialization and font fallback behavior.
    for entry in fs::read_dir(root.join("openpilot/common"))? {
        let name = entry?.file_name();
        let name = name.to_string_lossy();
        if matches!(
            name.as_ref(),
            "params_pyx" | "params_pyx.py" | "params_pyx.pyc"
        ) || (name.starts_with("params_pyx.") && name.ends_with(".so"))
        {
            return Err("application Params import provider changed".into());
        }
    }
    let lock: toml::Value = toml::from_str(&fs::read_to_string(root.join("uv.lock"))?)?;
    let dependencies = lock["package"]
        .as_array()
        .ok_or("application dependency lock missing")?
        .iter()
        .filter(|package| {
            package
                .get("name")
                .and_then(toml::Value::as_str)
                .is_some_and(|name| matches!(name, "comma-deps-raylib" | "cffi" | "pycparser"))
        })
        .collect::<Vec<_>>();
    if serde_json::to_value(dependencies)? != pins["dependencies"] {
        return Err("application graphics dependencies changed".into());
    }
    Ok(())
}
struct Settings {
    startup: bool,
    vsync: bool,
    burn: bool,
    offscreen: bool,
    show_fps: bool,
    touches: bool,
    strict: bool,
    grid: BigInt,
    profile: BigInt,
    record: bool,
    output: PathBuf,
    global: GlobalOptions,
}
struct SignalRegistration(signal_hook::SigId);
impl Drop for SignalRegistration {
    fn drop(&mut self) {
        signal_hook::low_level::unregister(self.0);
    }
}
// Same POSIX pathlib normalization used by the camera source tools, without
// importing their optional schema/runtime dependency feature.
pub(crate) fn lexical_path(path: &Path) -> PathBuf {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    let bytes = path.as_os_str().as_bytes();
    let mut result = if bytes.starts_with(b"//") && !bytes.starts_with(b"///") {
        b"//".to_vec()
    } else if bytes.starts_with(b"/") {
        b"/".to_vec()
    } else {
        vec![]
    };
    for part in bytes
        .split(|byte| *byte == b'/')
        .filter(|part| !part.is_empty() && *part != b".")
    {
        if !result.is_empty() && !result.ends_with(b"/") {
            result.push(b'/');
        }
        result.extend_from_slice(part);
    }
    if result.is_empty() {
        result.push(b'.');
    }
    OsString::from_vec(result).into()
}
fn suffix(path: &Path, extension: &str) -> Result<PathBuf> {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    // pathlib preserves a ParentDir basename and treats a terminal dot as part
    // of the name in CPython 3.12; Rust's file_name omits that component.
    let path = lexical_path(path);
    let bytes = path.as_os_str().as_bytes();
    let start = bytes
        .iter()
        .rposition(|byte| *byte == b'/')
        .map_or(0, |at| at + 1);
    let name = &bytes[start..];
    if name.is_empty() || name == b"." {
        return Err("ValueError: path has an empty name".into());
    }
    let end = name
        .iter()
        .rposition(|byte| *byte == b'.')
        .filter(|at| *at > 0 && *at + 1 < name.len())
        .unwrap_or(name.len());
    let mut result = bytes[..start + end].to_vec();
    result.push(b'.');
    result.extend_from_slice(extension.as_bytes());
    Ok(OsString::from_vec(result).into())
}
fn read_integer(env: &BTreeMap<OsString, OsString>, key: &str, default: &str) -> Result<BigInt> {
    let value = env.get(OsStr::new(key)).map_or(Ok(default), |v| {
        v.to_str().ok_or("environment integer encoding")
    })?;
    let canonical = crate::stats_clock_check::integer(value)
        .map_err(|_| format!("ValueError: invalid literal for int() with base 10: {value:?}"))?;
    Ok(canonical.parse()?)
}
impl Settings {
    fn read(env: &BTreeMap<OsString, OsString>) -> Result<Self> {
        let one = |key| env.get(OsStr::new(key)).is_some_and(|value| value == "1");
        let grid = read_integer(env, "GRID", "0")?;
        let profile = read_integer(env, "PROFILE_RENDER", "0")?;
        let _stats = read_integer(env, "PROFILE_STATS", "100")?;
        let output = suffix(
            Path::new(
                env.get(OsStr::new("RECORD_OUTPUT"))
                    .map_or(OsStr::new("output"), OsString::as_os_str),
            ),
            "mp4",
        )?;
        let quality = read_integer(env, "RECORD_QUALITY", "23")?;
        let speed = read_integer(env, "RECORD_SPEED", "1")?;
        Ok(Self {
            startup: env.contains_key(OsStr::new("PROFILE_STARTUP")),
            vsync: one("ENABLE_VSYNC"),
            burn: env.contains_key(OsStr::new("BURN_IN")),
            offscreen: one("OFFSCREEN"),
            show_fps: one("SHOW_FPS"),
            touches: one("SHOW_TOUCHES"),
            strict: one("STRICT_MODE"),
            grid,
            profile,
            record: one("RECORD"),
            output,
            global: GlobalOptions {
                quality,
                speed,
                bitrate: env
                    .get(OsStr::new("RECORD_BITRATE"))
                    .cloned()
                    .unwrap_or_default(),
            },
        })
    }
}
struct Reference<'a> {
    options: &'a Options,
    settings: Settings,
    raw: Raw,
    output: &'a mut dyn Write,
    resources: Resources,
    input: Option<Input>,
    button: Option<controls::Button>,
    mouse: Mouse,
    events: Vec<MouseEvent>,
    history: VecDeque<(f64, f64, f64)>,
    strict_exit: bool,
    ticks: usize,
    close: bool,
    texture: bool,
    encoder: Option<Encoder>,
    recording: bool,
    record_start: f64,
    readback: tempfile::NamedTempFile,
    epoch: Instant,
    render_started: Instant,
    fps_log: Instant,
    interrupted: Arc<AtomicBool>,
}
impl Reference<'_> {
    fn call(&mut self, command: &str) -> Result<()> {
        self.raw.call(command)
    }
    fn close_encoder(&mut self) {
        if let Some(mut encoder) = self.encoder.take() {
            encoder.close();
        }
    }
    fn spawn_encoder(&mut self, args: Vec<OsString>, capacity: usize) -> Result<()> {
        let mut command = Command::new(&args[0]);
        command.args(&args[1..]);
        self.encoder = Some(Encoder::spawn(&mut command, capacity)?);
        Ok(())
    }
    fn start_recording(&mut self) -> Result<()> {
        if self.recording {
            return Ok(());
        }
        if !self.texture {
            self.call("target 536 240")?;
            self.texture = true;
            self.resources.target = Some("target".into());
        }
        let directory = suffix(&self.options.output, "videos")?;
        fs::create_dir_all(&directory)?;
        let path = directory.join(format!(
            "{}.mp4",
            chrono::Local::now().format("%Y%m%d-%H%M%S")
        ));
        self.close_encoder();
        self.spawn_encoder(dynamic_command(536, 240, 20, path.as_os_str()), 8)?;
        self.recording = true;
        self.record_start = self.epoch.elapsed().as_secs_f64();
        writeln!(self.output, "[REC] start -> {}", path.display())?;
        Ok(())
    }
    fn stop_recording(&mut self) -> Result<()> {
        if !self.recording {
            return Ok(());
        }
        self.recording = false;
        self.close_encoder();
        writeln!(self.output, "[REC] stop")?;
        Ok(())
    }
    fn init(&mut self) -> Result<()> {
        self.call(&format!(
            "flags {}",
            32 | if self.settings.vsync { 64 } else { 0 }
        ))?;
        self.call(&format!(
            "app-window 536 240 {}",
            hex("Source framework lifecycle")
        ))?;
        self.resources.ready = true;
        if self.settings.burn || self.settings.record {
            self.call("target 536 240")?;
            self.texture = true;
            self.resources.target = Some("target".into());
        }
        if self.settings.record {
            self.spawn_encoder(
                global_command(
                    536,
                    240,
                    20,
                    &self.settings.global,
                    self.settings.output.as_os_str(),
                ),
                60,
            )?;
        }
        self.call(&format!(
            "fps-limit {}",
            if self.settings.offscreen { 0 } else { 20 }
        ))?;
        self.load_fonts()?;
        if self.settings.burn {
            self.call(&format!(
                "shader {} {}",
                hex(include_str!("../tests/fixtures/ui-diagnostics/vertex.glsl")),
                hex(include_str!(
                    "../tests/fixtures/ui-diagnostics/fragment.glsl"
                ))
            ))?;
            self.resources.shader = Some("shader".into());
        }
        Ok(())
    }
    fn load_fonts(&mut self) -> Result<()> {
        let names = [
            "Inter-Medium",
            "Inter-Bold",
            "Inter-SemiBold",
            "Pretendard-SemiBold",
            "unifont",
            "Inter-Regular",
            "KaiGenGothicKR-Bold",
        ];
        for (id, name) in names.iter().enumerate() {
            let base = self
                .options
                .root
                .join("openpilot/selfdrive/assets/fonts")
                .join(name);
            let mut path = base.with_extension("fnt");
            if !path.exists() {
                for extension in ["ttf", "otf"] {
                    let candidate = base.with_extension(extension);
                    if candidate.exists() {
                        path = candidate;
                        break;
                    }
                }
            }
            let broad = matches!(id, 4 | 6);
            let points = (32..127)
                .chain(if broad { 0x3400..0x4dc0 } else { 0..0 })
                .chain(if broad { 0x4e00..0xa000 } else { 0..0 })
                .chain(if broad { 0xac00..0xd7a4 } else { 0..0 })
                .collect::<Vec<u32>>();
            let command = if path.extension() == Some(OsStr::new("fnt")) {
                "atlas"
            } else {
                "source"
            };
            let size = if id == 4 {
                16
            } else if id == 6 {
                48
            } else {
                200
            };
            let encoded = hex(path.to_str().ok_or("font path encoding")?);
            let args = points
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(" ");
            let valid = self.raw.request(&format!(
                "app-font {id} {command} {encoded} {size} {} {args}",
                points.len()
            ))? == "1";
            if id != 4 && valid {
                self.call(&format!("font-mipmaps {id}"))?;
            }
            self.resources
                .fonts
                .push((name.to_string(), id.to_string()));
        }
        self.call("font-select 0")?;
        Ok(())
    }
    fn close(&mut self) -> Result<()> {
        let mut resources = std::mem::replace(
            &mut self.resources,
            Resources {
                ready: false,
                textures: vec![],
                fonts: vec![],
                target: None,
                shader: None,
            },
        );
        let result = close_resources(&mut resources, &mut |kind, identity| match kind {
            "texture" | "font" => self.call(&format!(
                "app-unload {kind} {}",
                identity.ok_or("resource identity missing")?
            )),
            "target" | "shader" => self.call(&format!("app-unload {kind} 0")),
            "ffmpeg" => {
                self.close_encoder();
                Ok(())
            }
            "stop-recording" => self.stop_recording(),
            "window" => self.call("app-close-window"),
            _ => Err("application cleanup operation".into()),
        });
        self.resources = resources;
        result
    }
    fn draw_widgets(&mut self) -> Result<()> {
        let keyboard = self
            .raw
            .request("keyboard-input")?
            .split_whitespace()
            .map(str::parse::<i64>)
            .collect::<std::result::Result<Vec<_>, _>>()?;
        if keyboard.len() != 6 {
            return Err("application keyboard payload".into());
        }
        let down = [262, 263, 259, 261]
            .into_iter()
            .zip(&keyboard[2..])
            .filter_map(|(key, on)| (*on != 0).then_some(key))
            .collect::<Vec<_>>();
        let events=self.events.iter().map(|event|serde_json::json!({"slot":event.slot,"pos":{"x":event.pos.x,"y":event.pos.y},"pressed":event.pressed,"released":event.released,"down":event.down,"t":event.time})).collect::<Vec<_>>();
        self.call("font-select 0")?;
        self.input.as_mut().ok_or("Demo input missing")?.frame(
            &mut self.raw,
            InputFrame {
                rect: Rect {
                    x: 20.,
                    y: 30.,
                    width: 496.,
                    height: 100.,
                },
                now: self.epoch.elapsed().as_secs_f64(),
                scale: 1.16,
                key: keyboard[0],
                down: &down,
                character: u32::try_from(keyboard[1]).ok().filter(|v| *v != 0),
                events: &events,
                debug: self.settings.touches,
            },
        )?;
        if self.button.as_mut().ok_or("Demo button missing")?.render(
            &mut self.raw,
            Rect {
                x: 190.,
                y: 160.,
                width: 160.,
                height: 60.,
            },
            1.16,
            &events,
        )? {
            self.close = true;
        }
        controls::debug_rect(
            &mut self.raw,
            Rect {
                x: 0.,
                y: 0.,
                width: 536.,
                height: 240.,
            },
        )?;
        Ok(())
    }
    fn touches(&mut self) -> Result<()> {
        let now = self.epoch.elapsed().as_secs_f64();
        draw_touch_points(&mut self.raw, &mut self.history, &self.events, now)
    }
    fn event_inner(&mut self, event: Event) -> Result<()> {
        match event {
            Event::MousePoll => {
                let fields = self
                    .raw
                    .request("mouse-input")?
                    .split_whitespace()
                    .map(str::parse::<f64>)
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                if fields.len() != 6 {
                    return Err("application mouse payload".into());
                }
                self.events = self.mouse.poll(
                    [fields[0] != 0., fields[3] != 0.],
                    [(fields[1], fields[2]), (fields[4], fields[5])],
                    self.epoch.elapsed().as_secs_f64(),
                );
            }
            Event::MouseEvents => {}
            Event::PollInput => self.call("poll-input")?,
            Event::Sleep(seconds) => std::thread::sleep(Duration::from_secs_f64(seconds)),
            Event::Begin => self.call("begin")?,
            Event::TargetBegin => self.call("target-begin")?,
            Event::TargetEnd => self.call("target-end")?,
            Event::Clear => self.call("clear 4278190080")?,
            Event::MatrixPush => self.call("matrix-push")?,
            Event::MatrixPop => self.call("matrix-pop")?,
            Event::MatrixScale(scale) => self.call(&format!("matrix-scale-xy {scale} {scale}"))?,
            Event::Ticks => self.ticks += 1,
            Event::Widgets { .. } => self.draw_widgets()?,
            Event::ShaderBegin => self.call("shader-begin")?,
            Event::ShaderEnd => self.call("shader-end")?,
            Event::TargetDraw { .. } => self.call("target-draw")?,
            Event::ShowFps => self.call("fps")?,
            Event::ShowTouches => self.touches()?,
            Event::Grid => {
                let mut position = BigInt::from(0);
                while position <= BigInt::from(536) {
                    self.call(&format!(
                        "app-grid-line {position} 0 {position} 240 4282137660"
                    ))?;
                    position += &self.settings.grid;
                }
                let mut position = BigInt::from(0);
                while position <= BigInt::from(240) {
                    self.call(&format!(
                        "app-grid-line 0 {position} 536 {position} 4282137660"
                    ))?;
                    position += &self.settings.grid;
                }
            }
            Event::End => self.call("end")?,
            Event::RecordReadback => {
                self.call(&format!(
                    "app-record-read {}",
                    hex(self.readback.path().to_str().ok_or("recording temp path")?)
                ))?;
            }
            Event::RecordSubmit => {
                let frame = fs::read(self.readback.path())?;
                match self
                    .encoder
                    .as_ref()
                    .ok_or("recording queue missing")?
                    .submit(frame)
                {
                    Ok(()) => {}
                    Err(SubmitError::Full) => return Err("queue full".into()),
                    Err(SubmitError::Closed) => return Err("recording queue missing".into()),
                }
            }
            Event::RecordUnload => self.call("app-record-unload")?,
            Event::StopRecording => self.stop_recording()?,
            Event::StartRecording => self.start_recording()?,
            Event::MonitorFps => {
                let fps = self.raw.request("get-fps")?.parse::<i32>()?;
                if fps < 18 && self.fps_log.elapsed() >= Duration::from_secs(5) {
                    eprintln!("FPS dropped below 20: {fps}");
                    self.fps_log = Instant::now();
                }
                if self.settings.strict && fps < 10 {
                    self.close_encoder();
                    self.strict_exit = true;
                    return Err("strict application exit".into());
                }
            }
            Event::ProfileOutput(frames) => {
                let elapsed = self.render_started.elapsed().as_secs_f64() * 1000.;
                let average = elapsed / frames as f64;
                writeln!(self.output,"\n=== Render loop profile ===\nnative original application render loop\n\n\x1b[92mRendered {frames} frames in {elapsed:.1} ms\x1b[0m\n\x1b[92mAverage frame time: {average:.2} ms ({:.1} FPS)\x1b[0m",1000./average)?;
            }
        }
        Ok(())
    }
}
impl Backend for Reference<'_> {
    fn should_close(&mut self, _frame: usize) -> std::result::Result<bool, LoopError> {
        if self.interrupted.load(Ordering::Relaxed) {
            return Err(LoopError::Interrupted);
        }
        self.raw
            .request("window-close")
            .map(|v| v == "1")
            .map_err(|e| LoopError::Failure(e.to_string()))
    }
    fn close_requested(&self) -> bool {
        self.close
    }
    fn now(&self) -> f64 {
        self.epoch.elapsed().as_secs_f64()
    }
    fn recording(&self) -> Option<(f64, f64)> {
        self.recording.then_some((self.record_start, 60.))
    }
    fn event(&mut self, event: Event) -> std::result::Result<(), LoopError> {
        if self.interrupted.load(Ordering::Relaxed) {
            return Err(LoopError::Interrupted);
        }
        self.event_inner(event).map_err(|e| {
            if e.to_string() == "queue full" {
                LoopError::QueueFull
            } else {
                LoopError::Failure(e.to_string())
            }
        })
    }
}
pub fn run(
    options: &Options,
    environment: &BTreeMap<OsString, OsString>,
    output: &mut dyn Write,
) -> Result<()> {
    let options = Options {
        root: options.root.clone(),
        mode: options.mode.clone(),
        output: crate::path_resolve::resolve_non_strict(&options.output)?,
        raw_helper: options.raw_helper.clone(),
        raylib_library: options.raylib_library.clone(),
    };
    let options = &options;
    validate_source(&options.root)?;
    let settings = Settings::read(environment)?;
    let interrupted = Arc::new(AtomicBool::new(false));
    let _registration = SignalRegistration(signal_hook::flag::register(
        signal_hook::consts::SIGINT,
        Arc::clone(&interrupted),
    )?);
    let raw = Raw::open(&options.raw_helper, &options.raylib_library)?;
    let mut app = Reference {
        options,
        settings,
        raw,
        output,
        resources: Resources {
            ready: false,
            textures: vec![],
            fonts: vec![],
            target: None,
            shader: None,
        },
        input: None,
        button: None,
        mouse: Mouse::new(1.),
        events: vec![],
        history: VecDeque::new(),
        strict_exit: false,
        ticks: 0,
        close: false,
        texture: false,
        encoder: None,
        recording: false,
        record_start: 0.,
        readback: tempfile::NamedTempFile::new()?,
        epoch: Instant::now(),
        render_started: Instant::now(),
        fps_log: Instant::now(),
        interrupted,
    };
    let mut frames = 0;
    let mut skipped = 0;
    let result = (|| -> Result<()> {
        let startup = Instant::now();
        app.init()?;
        if app.settings.startup {
            writeln!(app.output,"\n=== Startup profile ===\nnative original application initialization\n\x1b[92mUI window ready in {:.1} ms\x1b[0m",startup.elapsed().as_secs_f64()*1000.)?;
            return Ok(());
        }
        app.input = Some(Input::new(255, false));
        controls::set_debug(app.settings.touches);
        let mut button = controls::Button::new("Close", false);
        button.size = 30.;
        button.font = 0;
        app.button = Some(button);
        if options.mode == "dynamic" {
            app.start_recording()?;
            app.start_recording()?;
        }
        let mut generator = Loop::new(Config {
            texture: app.texture,
            burn_in: app.settings.burn,
            show_fps: app.settings.show_fps,
            show_touches: app.settings.touches,
            grid: app.settings.grid > BigInt::from(0),
            global_record: app.settings.record,
            profile_frames: app.settings.profile.clone(),
            ..Config::default()
        });
        if options.mode == "paused" {
            generator.set_should_render(false);
        }
        app.render_started = Instant::now();
        let rendering = (|| -> Result<()> {
            loop {
                if app.interrupted.load(Ordering::Relaxed) {
                    app.close()?;
                    break;
                }
                let outcome = generator
                    .resume(&mut app)
                    .map_err(|error| format!("application render: {error:?}"))?;
                frames = generator.frame();
                match outcome {
                    Outcome::Stopped | Outcome::ProfileComplete => break,
                    Outcome::Yield(false) => {
                        skipped += 1;
                        if skipped >= 2 {
                            generator.set_should_render(true);
                        }
                    }
                    Outcome::Yield(true) => {
                        app.call(&format!(
                            "{} {}",
                            if app.texture {
                                "export-target"
                            } else {
                                "export"
                            },
                            hex(options.output.to_str().ok_or("output path encoding")?)
                        ))?;
                        if options.mode == "paused" && generator.frame() >= 3 {
                            generator.request_close();
                        }
                    }
                }
            }
            Ok(())
        })();
        frames = generator.frame();
        rendering
    })();
    if app.strict_exit {
        // Python os._exit skips finally, while OS teardown still releases the
        // in-process window. Reap our separate mechanical window owner as well.
        let _ = app.raw.close();
        return result;
    }
    if app.interrupted.load(Ordering::Relaxed) {
        app.close()?;
    }
    // Source finally ordering: dynamic stop twice, encoder close, state, close.
    let finalized = (|| -> Result<()> {
        if options.mode == "dynamic" {
            app.stop_recording()?;
            app.stop_recording()?;
        }
        app.close_encoder();
        let text = if let Some(input) = &app.input {
            input.text_json()?.to_json()?
        } else {
            "\"\"".to_string()
        };
        fs::write(
            suffix(&options.output, "json")?,
            format!(
                "{{\"frames\": {frames}, \"ticks\": {}, \"skipped\": {skipped}, \"text\": {text}}}",
                app.ticks
            ),
        )?;
        app.close()?;
        app.raw.close()?;
        Ok(())
    })();
    finalized?;
    result
}

#[cfg(test)]
mod tests {
    #[test]
    fn original_record_output_pathlib_suffix_contract() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/ui-application-lifecycle/record-output-path-original.json"
        ))
        .unwrap();
        for case in fixture["cases"].as_array().unwrap() {
            let input = case["input"].as_str().unwrap();
            let actual = super::suffix(std::path::Path::new(input), "mp4");
            if let Some(expected) = case["output"].as_str() {
                assert_eq!(actual.unwrap(), std::path::Path::new(expected), "{input:?}");
            } else {
                assert!(actual.is_err(), "{input:?}");
            }
        }
    }
}

/// Positional source-helper CLI; extra arguments are ignored by the original.
pub fn cli() -> Result<()> {
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    let mode = arguments
        .first()
        .ok_or("application reference mode missing")?;
    let output = arguments
        .get(1)
        .ok_or("application reference output missing")?;
    // The original resolves output before importing/parsing application settings.
    let output = crate::path_resolve::resolve_non_strict(Path::new(output))?;
    let root = crate::ui_diagnostic_reference::source_root()?;
    let current = std::env::current_exe()?;
    let helper = std::env::var_os("UI_APPLICATION_RAW_HELPER")
        .map(PathBuf::from)
        .unwrap_or_else(|| current.parent().unwrap().join("ui-application-raw"));
    let library = std::env::var_os("STARTUP_UI_RAYLIB_LIBRARY")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("STARTUP_UI_RAYLIB_ROOT")
                .map(|v| PathBuf::from(v).join("lib/libopenpilot-raylib.so"))
        })
        .unwrap_or_else(|| "libopenpilot-raylib.so".into());
    run(
        &Options {
            root,
            mode: mode.to_str().ok_or("application mode encoding")?.into(),
            output,
            raw_helper: helper,
            raylib_library: library,
        },
        &std::env::vars_os().collect(),
        &mut std::io::stdout(),
    )
}
