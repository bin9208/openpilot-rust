//! Original application checker policy with native process/X11/log boundaries.
use crate::{
    control_tools_policy_check::json,
    qa_process::{CommandSpec, Runner},
    seal::Result,
};
use openpilot_log_format::{JsonValue, JsonView};
use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};
#[derive(Clone)]
pub struct Options {
    pub root: PathBuf,
    pub binary: PathBuf,
    pub reference: PathBuf,
    pub output: PathBuf,
    pub display: OsString,
    pub pid: u32,
}
pub trait Backend: Runner {
    fn bind_logs(&mut self, address: &str) -> Result<()>;
    fn log_packet(&mut self) -> Result<Option<Vec<u8>>>;
    fn close_logs(&mut self) -> Result<()>;
    fn open_display(&mut self, display: &OsStr) -> Result<()>;
    fn window(&mut self, title: &str) -> Result<Option<u64>>;
    fn focus(&mut self, window: u64) -> Result<()>;
    fn sync(&mut self) -> Result<()>;
    fn key(&mut self, character: char, down: bool) -> Result<()>;
    fn motion(&mut self, window: u64, x: i32, y: i32) -> Result<()>;
    fn button(&mut self, down: bool) -> Result<()>;
    fn close_display(&mut self) -> Result<()>;
    fn protocol_errors(&mut self) -> Result<bool>;
    fn start_live(&mut self, command: &CommandSpec, log: &Path) -> Result<u32>;
    fn poll_live(&mut self) -> Result<Option<i32>>;
    /// None timeout waits indefinitely; a returned None denotes a timed wait.
    fn wait_live(&mut self, timeout: Option<Duration>) -> Result<Option<i32>>;
    fn kill_live(&mut self) -> Result<()>;
    fn identity(&mut self, pid: u32) -> Result<(PathBuf, String)>;
    fn now(&mut self) -> f64;
    fn sleep(&mut self, seconds: f64);
}
fn need(value: bool, message: &str) -> Result<()> {
    if value {
        Ok(())
    } else {
        Err(message.to_owned().into())
    }
}
fn field(value: &JsonValue, name: &str) -> Result<JsonValue> {
    value
        .get(name)
        .ok_or_else(|| format!("application result missing {name}").into())
}
fn eq_number(value: &JsonValue, number: i32) -> Result<bool> {
    Ok(json::equal(value, &JsonValue::parse(&number.to_string())?))
}
fn positive(value: &JsonValue) -> Result<bool> {
    Ok(match value.view() {
        JsonView::Bool(v) => v,
        JsonView::Integer(v) => v.parse::<num_bigint::BigInt>()? > num_bigint::BigInt::from(0),
        JsonView::Float(v) => v > 0.,
        _ => return Err("application frames must compare with zero".into()),
    })
}
fn truth(value: &JsonValue) -> bool {
    match value.view() {
        JsonView::Null => false,
        JsonView::Bool(v) => v,
        JsonView::Integer(v) => v
            .parse::<num_bigint::BigInt>()
            .is_ok_and(|v| v != num_bigint::BigInt::from(0)),
        JsonView::Float(v) => v != 0.,
        JsonView::Text(v) => !v.is_empty(),
        JsonView::Array(v) => !v.is_empty(),
        JsonView::Object(v) => !v.is_empty(),
    }
}
fn object_with_scenario(name: &str, value: &JsonValue) -> Result<JsonValue> {
    let JsonView::Object(fields) = value.view() else {
        return Err("application result is not an object".into());
    };
    let mut rows = vec![(JsonValue::text("scenario"), JsonValue::text(name))];
    for (key, value) in fields {
        let key = JsonValue::codepoints(key.to_vec()).ok_or("JSON key")?;
        if key.text_eq("scenario") {
            rows[0].1 = value;
        } else {
            rows.push((key, value));
        }
    }
    JsonValue::parse(&format!(
        "{{{}}}",
        rows.iter()
            .map(|(k, v)| Ok(format!("{}:{}", k.to_json()?, v.to_json()?)))
            .collect::<Result<Vec<_>>>()?
            .join(",")
    ))
    .map_err(Into::into)
}
fn pretty(value: &JsonValue) -> Result<String> {
    json::pretty_with_limit(value, 0, 996)
}
fn read_state(path: &Path) -> Result<JsonValue> {
    json::parse_text(
        &fs::read_to_string(path)?
            .replace("\r\n", "\n")
            .replace('\r', "\n"),
    )
}
fn drain(backend: &mut dyn Backend, records: &mut Vec<JsonValue>) -> Result<()> {
    while let Some(packet) = backend.log_packet()? {
        let (level, payload) = packet.split_first().ok_or("empty application log packet")?;
        let record = json::parse(payload)?;
        need(
            eq_number(&field(&record, "levelnum")?, i32::from(*level))?,
            "application log level does not match prefix",
        )?;
        records.push(record);
    }
    Ok(())
}
fn spec(argv: Vec<OsString>, env: BTreeMap<OsString, OsString>, timeout: Duration) -> CommandSpec {
    let mut spec = CommandSpec::new_os(argv, timeout);
    spec.set_os_env(env);
    spec.inherit_stdin = true;
    spec.stdout_limit = usize::MAX;
    spec.stderr_limit = usize::MAX;
    spec
}
fn base_env(options: &Options) -> BTreeMap<OsString, OsString> {
    [
        ("DISPLAY".into(), options.display.clone()),
        (
            "PARAMS_ROOT".into(),
            options.output.join("params").into_os_string(),
        ),
        (
            "OPENPILOT_PREFIX".into(),
            format!("ui-framework-125-{}", options.pid).into(),
        ),
    ]
    .into()
}
fn run_case(
    options: &Options,
    backend: &mut dyn Backend,
    records: &mut Vec<JsonValue>,
    results: &mut Vec<JsonValue>,
    name: &str,
    mode: &str,
    extra: BTreeMap<OsString, OsString>,
) -> Result<JsonValue> {
    let image = options.output.join(format!("{name}.png"));
    let mut env = base_env(options);
    env.extend(extra.clone());
    let command = spec(
        vec![
            options.binary.clone().into_os_string(),
            options.root.clone().into_os_string(),
            mode.into(),
            image.clone().into_os_string(),
        ],
        env,
        Duration::from_secs(45),
    );
    need(
        backend
            .run_to_file(&command, &options.output.join(format!("{name}.log")))?
            .code
            == 0,
        "native application child failed",
    )?;
    drain(backend, records)?;
    let result = read_state(&image.with_extension("json"))?;
    results.push(object_with_scenario(name, &result)?);
    let source = options.output.join(format!("source-{name}.png"));
    let mut env = base_env(options);
    env.extend(extra);
    if env.contains_key(OsStr::new("RECORD_OUTPUT")) {
        env.insert(
            "RECORD_OUTPUT".into(),
            options.output.join("source-record.mp4").into_os_string(),
        );
    }
    env.insert("PYTHONPATH".into(), options.root.clone().into_os_string());
    let command = spec(
        vec![
            options.reference.clone().into_os_string(),
            mode.into(),
            source.clone().into_os_string(),
        ],
        env,
        Duration::from_secs(45),
    );
    need(
        backend
            .run_to_file(&command, &options.output.join(format!("source-{name}.log")))?
            .code
            == 0,
        "source application child failed",
    )?;
    let expected = read_state(&source.with_extension("json"))?;
    let JsonView::Object(fields) = expected.view() else {
        return Err("source application result is not an object".into());
    };
    for (key, value) in fields {
        let JsonView::Object(actual) = result.view() else {
            return Err("native application result is not an object".into());
        };
        let actual = actual
            .into_iter()
            .find(|(name, _)| *name == key)
            .map(|(_, value)| value)
            .ok_or("native application result key missing")?;
        need(
            json::equal(&actual, &value),
            "source/native application state differs",
        )?;
    }
    if positive(&field(&result, "frames")?)? {
        need(
            rgb_equal(&image, &source)?,
            "source/native application pixels differ",
        )?;
    }
    Ok(result)
}
// Pillow convert('RGB') ignores alpha; ImageChops uses the common rectangle when
// image dimensions differ. PNG is the real screenshot format, independent of its suffix.
fn rgb(path: &Path) -> Result<(usize, usize, Vec<u8>)> {
    let bytes = fs::read(path)?;
    if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        let image = image::load_from_memory(&bytes)?.to_rgb8();
        return Ok((
            image.width() as usize,
            image.height() as usize,
            image.into_raw(),
        ));
    }
    let mut decoder = png::Decoder::new_with_limits(
        std::io::Cursor::new(bytes),
        png::Limits { bytes: usize::MAX },
    );
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info()?;
    let mut buffer = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buffer)?;
    let width = info.width as usize;
    let height = info.height as usize;
    crate::image_array::pixel_warning(width, height)?;
    let channels = info.color_type.samples();
    let depth = if info.bit_depth == png::BitDepth::Sixteen {
        2
    } else {
        1
    };
    let mut rgb = Vec::with_capacity(width * height * 3);
    for row in buffer[..info.buffer_size()].chunks(info.line_size) {
        for pixel in row.chunks_exact(channels * depth).take(width) {
            let channel = |index: usize| pixel[index * depth];
            let triplet = match info.color_type {
                png::ColorType::Rgb | png::ColorType::Rgba => [channel(0), channel(1), channel(2)],
                png::ColorType::Grayscale | png::ColorType::GrayscaleAlpha => {
                    let gray = if info.bit_depth == png::BitDepth::Sixteen
                        && info.color_type == png::ColorType::Grayscale
                    {
                        u16::from_be_bytes([pixel[0], pixel[1]]).min(255) as u8
                    } else {
                        channel(0)
                    };
                    [gray; 3]
                }
                _ => return Err("application PNG palette expansion failed".into()),
            };
            rgb.extend(triplet);
        }
    }
    Ok((width, height, rgb))
}
fn rgb_equal(left: &Path, right: &Path) -> Result<bool> {
    let (lw, lh, l) = rgb(left)?;
    let (rw, rh, r) = rgb(right)?;
    for y in 0..lh.min(rh) {
        for x in 0..lw.min(rw) {
            if l[(y * lw + x) * 3..(y * lw + x) * 3 + 3]
                != r[(y * rw + x) * 3..(y * rw + x) * 3 + 3]
            {
                return Ok(false);
            }
        }
    }
    Ok(true)
}
fn probe(
    options: &Options,
    backend: &mut dyn Backend,
    path: &Path,
    dynamic: bool,
) -> Result<JsonValue> {
    let entries = if dynamic {
        "stream=nb_frames"
    } else {
        "stream=width,height,nb_frames,r_frame_rate"
    };
    let mut command = [
        "ffprobe",
        "-v",
        "error",
        "-select_streams",
        "v:0",
        "-show_entries",
        entries,
        "-of",
        "json",
    ]
    .map(OsString::from)
    .to_vec();
    command.push(path.as_os_str().to_owned());
    let captured = backend.run(&spec(command, BTreeMap::new(), Duration::MAX))?;
    need(captured.code == 0, "application ffprobe failed")?;
    let text = String::from_utf8(captured.stdout)?
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    fs::write(
        options.output.join(if dynamic {
            "dynamic-ffprobe.json"
        } else {
            "record-ffprobe.json"
        }),
        &text,
    )?;
    let result = json::parse_text(&text)?;
    let streams = json::sequence(&field(&result, "streams")?)?;
    streams
        .into_iter()
        .next()
        .ok_or("ffprobe stream missing".into())
}
pub fn check(options: &Options, backend: &mut dyn Backend, printed: &mut dyn Write) -> Result<()> {
    let mut options = options.clone();
    options.binary = crate::ui_application_lifecycle_reference::lexical_path(&options.binary);
    options.output = crate::ui_application_lifecycle_reference::lexical_path(&options.output);
    let options = &options;
    fs::create_dir_all(&options.output)?;
    backend.bind_logs(&format!(
        "ipc:///tmp/logmessageui-framework-125-{}",
        options.pid
    ))?;
    let mut records = vec![];
    let mut results = vec![];
    let normal = |frames: &str| {
        BTreeMap::from([
            ("PROFILE_RENDER".into(), OsString::from(frames)),
            ("OFFSCREEN".into(), "1".into()),
        ])
    };
    let result = run_case(
        options,
        backend,
        &mut records,
        &mut results,
        "profile",
        "profile",
        normal("6"),
    )?;
    need(
        eq_number(&field(&result, "frames")?, 6)?,
        "profile frame count",
    )?;
    let result = run_case(
        options,
        backend,
        &mut records,
        &mut results,
        "startup-profile",
        "profile",
        [
            ("PROFILE_STARTUP".into(), "1".into()),
            ("OFFSCREEN".into(), "1".into()),
        ]
        .into(),
    )?;
    need(
        eq_number(&field(&result, "frames")?, 0)?,
        "startup frame count",
    )?;
    let result = run_case(
        options,
        backend,
        &mut records,
        &mut results,
        "paused",
        "paused",
        [("OFFSCREEN".into(), "1".into())].into(),
    )?;
    need(
        eq_number(&field(&result, "frames")?, 4)? && eq_number(&field(&result, "skipped")?, 2)?,
        "paused counts",
    )?;
    let record = options.output.join("record.mp4");
    let mut recording = normal("9");
    recording.extend([
        ("RECORD".into(), "1".into()),
        ("RECORD_OUTPUT".into(), record.clone().into_os_string()),
    ]);
    let result = run_case(
        options,
        backend,
        &mut records,
        &mut results,
        "record",
        "record",
        recording,
    )?;
    need(
        eq_number(&field(&result, "frames")?, 9)?,
        "record frame count",
    )?;
    let stream = probe(options, backend, &record, false)?;
    need(
        field(&stream, "nb_frames")?.text_eq("3")
            && eq_number(&field(&stream, "width")?, 536)?
            && eq_number(&field(&stream, "height")?, 240)?,
        "global recording metadata",
    )?;
    run_case(
        options,
        backend,
        &mut records,
        &mut results,
        "dynamic",
        "dynamic",
        normal("9"),
    )?;
    let videos = crate::python_glob::glob(
        &options.root,
        &options.output.join("dynamic.videos"),
        "*.mp4",
    )?;
    need(videos.len() == 1, "dynamic recording video count")?;
    let stream = probe(options, backend, &videos[0], true)?;
    need(
        field(&stream, "nb_frames")?.text_eq("3"),
        "dynamic recording frame count",
    )?;
    backend.open_display(&options.display)?;
    let image = options.output.join("live.png");
    let command = spec(
        vec![
            options.binary.clone().into_os_string(),
            options.root.clone().into_os_string(),
            "live".into(),
            image.clone().into_os_string(),
        ],
        base_env(options),
        Duration::MAX,
    );
    let pid = backend.start_live(&command, &options.output.join("live.log"))?;
    let interacted = (|| -> Result<()> {
        let deadline = backend.now() + 20.;
        let mut window = None;
        while backend.now() < deadline {
            need(
                backend.poll_live()?.is_none(),
                "native application exited before interaction",
            )?;
            window = backend.window("Rust framework lifecycle")?;
            if window.is_some() {
                break;
            }
            backend.sleep(0.05);
        }
        let window = window.ok_or("native application window missing")?;
        let (exe, mappings) = backend.identity(pid)?;
        need(
            exe == crate::path_resolve::resolve_non_strict(&options.binary)?
                && !mappings.to_lowercase().contains("libpython"),
            "native executable identity or mappings",
        )?;
        fs::write(
            options.output.join("live-native-process.json"),
            pretty(&JsonValue::parse(&format!(
                "{{\"pid\":{pid},\"exe\":{},\"python_library_mapped\":false}}",
                crate::python_fs::json_string(exe.as_os_str())?
            ))?)?,
        )?;
        while !image.exists() && backend.now() < deadline {
            backend.sleep(0.05);
        }
        need(image.exists(), "application did not render its first frame")?;
        backend.focus(window)?;
        backend.sync()?;
        backend.sleep(0.15);
        for ch in "native".chars() {
            backend.key(ch, true)?;
            backend.sync()?;
            backend.sleep(0.12);
            backend.key(ch, false)?;
            backend.sync()?;
            backend.sleep(0.08);
        }
        let mut attempts = 0;
        let deadline = backend.now() + 15.;
        while backend.poll_live()?.is_none() && backend.now() < deadline {
            attempts += 1;
            backend.focus(window)?;
            backend.motion(window, 270, 190)?;
            backend.sync()?;
            backend.sleep(0.1);
            backend.button(true)?;
            backend.sync()?;
            backend.sleep(0.25);
            backend.button(false)?;
            backend.sync()?;
            if backend
                .wait_live(Some(Duration::from_millis(500)))?
                .is_some()
            {
                break;
            }
        }
        need(
            backend.poll_live()? == Some(0),
            "application did not close after real clicks",
        )?;
        fs::write(
            options.output.join("live-click-attempts.json"),
            format!("{{\"attempts\": {attempts}}}"),
        )?;
        Ok(())
    })();
    if backend.poll_live()?.is_none() {
        backend.kill_live()?;
        backend.wait_live(None)?;
    }
    interacted?;
    backend.close_display()?;
    need(
        !backend.protocol_errors()?,
        "application X11 protocol errors",
    )?;
    let live = read_state(&image.with_extension("json"))?;
    need(
        field(&live, "text")?.text_eq("native") && truth(&field(&live, "closed")?),
        "live text or closed state",
    )?;
    results.push(object_with_scenario("live", &live)?);
    for result in &results {
        need(
            json::equal(&field(result, "ticks")?, &field(result, "frames")?),
            "duplicate tick counts",
        )?;
    }
    drain(backend, &mut records)?;
    let mut raylib = false;
    for record in &records {
        let text = field(record, "msg")?;
        let JsonView::Text(text) = text.view() else {
            return Err("application log message type".into());
        };
        if text.starts_with(&"raylib: ".chars().map(u32::from).collect::<Vec<_>>()) {
            raylib = true;
            break;
        }
    }
    need(raylib, "application raylib log missing")?;
    for record in &records {
        need(
            {
                let path = field(record, "pathname")?;
                let JsonView::Text(path) = path.view() else {
                    return Err("application log pathname type".into());
                };
                path.ends_with(&"logging.rs".chars().map(u32::from).collect::<Vec<_>>())
            },
            "application log pathname",
        )?;
    }
    let array = |rows: &[JsonValue]| -> Result<JsonValue> {
        JsonValue::parse(&format!(
            "[{}]",
            rows.iter()
                .map(JsonValue::to_json)
                .collect::<std::result::Result<Vec<_>, _>>()?
                .join(",")
        ))
        .map_err(Into::into)
    };
    fs::write(
        options.output.join("native-log-transport.json"),
        pretty(&array(&records)?)?,
    )?;
    backend.close_logs()?;
    let result = pretty(&array(&results)?)?;
    fs::write(options.output.join("results.json"), &result)?;
    writeln!(printed, "{result}")?;
    writeln!(printed,"PASS: actual native XTest keyboard/click, duplicate tick suppression, pause, profile and global/dynamic recording shutdown")?;
    Ok(())
}

#[cfg(feature = "ui-application-lifecycle-native")]
#[path = "ui_application_lifecycle_native.rs"]
mod native;
#[cfg(feature = "ui-application-lifecycle-native")]
pub use native::{cli, NativeBackend};
