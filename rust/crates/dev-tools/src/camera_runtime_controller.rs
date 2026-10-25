//! Live controller phases shared by real IPC and independently captured peers.
use super::{ports, read_text, write_json, Options, SERVICES};
use crate::{
    camera_lifecycle_check::{result_value, values::*},
    camera_state_check::SchemaDecoder,
    control_tools_policy_check::json,
    python_fs,
    qa_process::CommandSpec,
    seal::Result,
};
use openpilot_log_format::{JsonValue, JsonView};
use std::{collections::BTreeMap, ffi::OsString, fmt, fs, path::Path, time::Duration};

pub trait Runtime {
    fn pid(&self) -> u32;
    fn inherited_environment(&self) -> Vec<(OsString, OsString)>;
    fn create_namespace(&mut self, path: &Path) -> Result<()>;
    fn prepare_schema(&mut self, _decoder: &mut dyn SchemaDecoder) -> Result<()> {
        Ok(())
    }
    fn subscribe(&mut self, service: &str, conflate: bool, segment_size: usize) -> Result<()>;
    fn vision_new(&mut self, name: &str, port: usize, conflate: bool) -> Result<()>;
    fn logger_socket(&mut self) -> Result<()>;
    fn logger_linger_zero(&mut self) -> Result<()>;
    fn logger_bind(&mut self, endpoint: &str) -> Result<()>;
    fn spawn(&mut self, spec: &CommandSpec, stdout: &Path, stderr: &Path) -> Result<()>;
    fn poll(&mut self) -> Result<Option<i32>>;
    fn signal(&mut self, signal: i32) -> Result<()>;
    fn kill(&mut self) -> Result<()>;
    fn wait(&mut self) -> Result<i32>;
    fn monotonic(&self) -> Duration;
    fn sleep(&mut self, duration: Duration);
    fn receive_message(&mut self, port: usize) -> Result<Option<Vec<u8>>>;
    fn vision_connected(&self, port: usize) -> bool;
    fn vision_connect(&mut self, port: usize) -> Result<bool>;
    fn receive_image(&mut self, port: usize) -> Result<Option<JsonValue>>;
    fn receive_log(&mut self) -> Result<Option<Vec<u8>>>;
    fn logger_close(&mut self) -> Result<()>;
    fn context_term(&mut self) -> Result<()>;
    fn remove_namespace(&mut self, path: &Path) -> Result<()>;
    fn unlink_endpoint(&mut self, path: &Path) -> Result<()>;
}
#[derive(Debug)]
struct Failure(String);
impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Failure {}
fn failure(representation: String) -> Box<dyn std::error::Error> {
    Box::new(Failure(representation))
}
fn simple(kind: &str, message: &str) -> Box<dyn std::error::Error> {
    failure(format!(
        "{kind}({})",
        repr(&JsonValue::text(message)).unwrap()
    ))
}
fn exception_repr(error: &(dyn std::error::Error + 'static)) -> Result<String> {
    if let Some(error) = error.downcast_ref::<Failure>() {
        Ok(error.0.clone())
    } else if let Some(error) = error.downcast_ref::<PythonError>() {
        Ok(format!(
            "{}({})",
            error.kind,
            repr(&JsonValue::text(&error.message))?
        ))
    } else {
        Ok(format!(
            "RuntimeError({})",
            repr(&JsonValue::text(&error.to_string()))?
        ))
    }
}
fn contextual(kind: &str, cause: Box<dyn std::error::Error>) -> Box<dyn std::error::Error> {
    if cause.is::<Failure>() || cause.is::<PythonError>() {
        cause
    } else {
        simple(kind, &cause.to_string())
    }
}
fn require(condition: bool, representation: String) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(failure(representation))
    }
}
fn update(environment: &mut Vec<(OsString, OsString)>, key: &str, value: OsString) {
    if let Some((_, previous)) = environment
        .iter_mut()
        .find(|(name, _)| name.as_os_str() == key)
    {
        *previous = value;
    } else {
        environment.push((key.into(), value));
    }
}
fn selected(key: &std::ffi::OsStr, prefixes: &[&[u8]]) -> bool {
    prefixes
        .iter()
        .any(|prefix| key.as_encoded_bytes().starts_with(prefix))
}
fn environment_json(environment: &[(OsString, OsString)]) -> Result<JsonValue> {
    json::parse_text(&format!(
        "{{{}}}",
        environment
            .iter()
            .filter(|(key, _)| selected(
                key,
                &[
                    b"CK_",
                    b"LD_",
                    b"ASAN_",
                    b"UBSAN_",
                    b"OPENPILOT_",
                    b"PARAMS_",
                    b"DISABLE_"
                ]
            ))
            .map(|(key, value)| Ok(format!(
                "{}: {}",
                python_fs::json_string(key)?,
                python_fs::json_string(value)?
            )))
            .collect::<Result<Vec<_>>>()?
            .join(", ")
    ))
}
#[derive(Default)]
struct Observations {
    messages: [Vec<JsonValue>; 3],
    images: [Vec<JsonValue>; 3],
    logs: Vec<JsonValue>,
    serialization_error: Option<String>,
}
impl Observations {
    fn receive(
        &mut self,
        platform: &mut dyn Runtime,
        enabled: &[usize],
        decoder: &mut dyn SchemaDecoder,
    ) -> Result<()> {
        for (port, service) in SERVICES.iter().enumerate() {
            let mut wires = vec![];
            let receive_error = loop {
                match platform.receive_message(port) {
                    Ok(Some(raw)) => wires.push(raw),
                    Ok(None) => break None,
                    Err(cause) => break Some(cause),
                }
            };
            let decoded = decoder.decode_many(&wires);
            let decoded_count = decoded.len();
            for (raw, decoded) in wires.iter().zip(decoded) {
                let decoded = decoded.map_err(|e| contextual("ValueError", e))?;
                let which = match decoded.value.view() {
                    JsonView::Object(fields) => fields.first().and_then(|(key, _)| {
                        key.iter()
                            .copied()
                            .map(char::from_u32)
                            .collect::<Option<String>>()
                    }),
                    _ => None,
                }
                .ok_or_else(|| simple("ValueError", "Event is not a struct"))?;
                require(
                    which == *service,
                    format!(
                        "AssertionError(({port}, {}))",
                        repr(&JsonValue::text(&which))?
                    ),
                )?;
                self.serialization_error = self
                    .serialization_error
                    .take()
                    .or(decoded.serialization_error);
                self.messages[port].push(object(&[
                    ("wire".into(), JsonValue::text(&hex_text(raw))),
                    ("event".into(), decoded.value),
                ])?);
            }
            require(
                decoded_count == wires.len(),
                "RuntimeError('schema decoder result count mismatch')".into(),
            )?;
            // Preserve the valid prefix before surfacing a later socket error.
            if let Some(cause) = receive_error {
                return Err(cause);
            }
        }
        for &port in enabled {
            if platform.vision_connected(port) {
                while let Some(frame) = platform.receive_image(port)? {
                    // Match the source projection order rather than a backend's map order.
                    self.images[port].push(object(
                        &[
                            "frame_id",
                            "buffer_frame_id",
                            "sof",
                            "eof",
                            "slot",
                            "width",
                            "height",
                            "stride",
                            "length",
                            "samples",
                        ]
                        .iter()
                        .map(|key| Ok(((*key).into(), get(&frame, key)?)))
                        .collect::<Result<Vec<_>>>()?,
                    )?);
                }
            }
        }
        while let Some(packet) = platform.receive_log()? {
            let payload = packet.get(1..).unwrap_or_default();
            let record = match std::str::from_utf8(payload) {
                Ok(text) => result_value(text, false)?,
                Err(e) => return Err(simple("UnicodeDecodeError", &e.to_string())),
            };
            let level = packet
                .first()
                .copied()
                .ok_or_else(|| simple("IndexError", "index out of range"))?;
            require(
                json::equal(&number(level), &get(&record, "levelnum")?),
                "AssertionError()".into(),
            )?;
            self.logs.push(record);
        }
        Ok(())
    }
    fn counts(&self) -> Result<JsonValue> {
        object(&[
            (
                "messages".into(),
                array(
                    &self
                        .messages
                        .iter()
                        .map(|rows| number(rows.len()))
                        .collect::<Vec<_>>(),
                )?,
            ),
            (
                "vision".into(),
                array(
                    &self
                        .images
                        .iter()
                        .map(|rows| number(rows.len()))
                        .collect::<Vec<_>>(),
                )?,
            ),
        ])
    }
    fn persist(&self, output: &Path) -> Result<()> {
        if let Some(error) = &self.serialization_error {
            return Err(simple("TypeError", error));
        }
        write_json(
            &output.join("messages.json"),
            &array(
                &self
                    .messages
                    .iter()
                    .map(|rows| array(rows))
                    .collect::<Result<Vec<_>>>()?,
            )?,
        )?;
        write_json(
            &output.join("vision.json"),
            &array(
                &self
                    .images
                    .iter()
                    .map(|rows| array(rows))
                    .collect::<Result<Vec<_>>>()?,
            )?,
        )?;
        write_json(&output.join("logs.json"), &array(&self.logs)?)?;
        Ok(())
    }
}
/// Drive one complete camera case. Verification follows only after successful
/// finalization; it is intentionally a separate operation, as in the source.
pub fn control(
    opt: &Options,
    platform: &mut dyn Runtime,
    decoder: &mut dyn SchemaDecoder,
) -> Result<()> {
    if opt.output.exists() {
        return Err(simple("FileExistsError", "File exists"));
    }
    fs::create_dir_all(&opt.output)?;
    let prefix = format!("camera_runtime_{}", platform.pid());
    let namespace = Path::new("/dev/shm").join(format!("msgq_{prefix}"));
    let endpoint_path = Path::new("/tmp").join(format!("logmessage{prefix}"));
    let endpoint = format!("ipc://{}", endpoint_path.display());
    platform.create_namespace(&namespace)?;
    if let Err(cause) = platform.prepare_schema(decoder) {
        platform.remove_namespace(&namespace)?;
        return Err(cause);
    }
    let mut logger = false;
    let mut bound = false;
    let mut in_try = false;
    let mut child = false;
    let mut exit = None;
    let mut sent_signal = None;
    let mut environment = platform.inherited_environment();
    update(&mut environment, "OPENPILOT_PREFIX", prefix.clone().into());
    environment.retain(|(key, _)| {
        !selected(
            key,
            &[
                b"CK_",
                b"SPECTRA_",
                b"DISABLE_",
                b"DEBUG_FRAMES",
                b"LOG_RAW_FRAMES",
                b"CTRL_EXP_FROM_PARAMS",
            ],
        )
    });
    let mut command = vec![opt.binary.as_os_str().to_owned()];
    let mut observations = Observations::default();
    let outcome = (|| -> Result<()> {
        let enabled = ports(&opt.case)?;
        match opt.case.as_str() {
            "disabled-road" => update(&mut environment, "DISABLE_ROAD", "".into()),
            "missing-road" => update(&mut environment, "CK_MISSING_PORT", "1".into()),
            "wait-failure" => {
                update(&mut environment, "CK_FAIL_OP", "sync:6".into());
                update(&mut environment, "CK_FAIL_SKIP", "8".into());
            }
            "poll-error" => {
                update(&mut environment, "CK_FAIL_OP", "poll".into());
                update(&mut environment, "CK_FAIL_SKIP", "6".into());
            }
            _ => {}
        }
        for (key, value) in [
            ("CK_SENSOR", OsString::from("21252")),
            ("CK_TRACE", opt.output.join("trace.jsonl").into_os_string()),
            ("CK_START_GATE", opt.output.join("go").into_os_string()),
            ("PARAMS_ROOT", opt.output.join("params").into_os_string()),
        ] {
            update(&mut environment, key, value);
        }
        if opt.case == "publication-order" {
            update(
                &mut environment,
                "CK_REGISTER_GATE",
                opt.output.join("register-go").into_os_string(),
            );
        }
        if opt.qemu.is_some() {
            require(opt.sysroot.is_some(), "AssertionError()".into())?;
            environment.retain(|(key, _)| key != "LD_PRELOAD" && key != "LD_LIBRARY_PATH");
        } else {
            let asan = "/usr/lib/llvm-18/lib/clang/18/lib/linux";
            let mut preload = OsString::from(format!("{asan}/libclang_rt.asan-x86_64.so:"));
            preload.push(&opt.fixture);
            for (key, value) in [
                ("LD_PRELOAD", preload),
                ("LD_LIBRARY_PATH", asan.into()),
                ("ASAN_OPTIONS", "detect_leaks=0:abort_on_error=1".into()),
                ("UBSAN_OPTIONS", "halt_on_error=1:print_stacktrace=1".into()),
            ] {
                update(&mut environment, key, value);
            }
        }
        let params = opt.output.join("params").join(&prefix);
        fs::create_dir_all(&params)?;
        fs::write(params.join("IsOffroad"), "1")?;
        for service in SERVICES {
            platform
                .subscribe(service, false, 250 * 1024)
                .map_err(|e| contextual("RuntimeError", e))?;
        }
        for &port in enabled {
            platform.vision_new("camerad", port, false)?;
        }
        platform.logger_socket()?;
        logger = true;
        platform.logger_linger_zero()?;
        platform
            .logger_bind(&endpoint)
            .map_err(|e| contextual("RuntimeError", e))?;
        bound = true;
        if let Some(qemu) = &opt.qemu {
            let mut preload = OsString::from("LD_PRELOAD=");
            preload.push(&opt.fixture);
            command = vec![
                qemu.as_os_str().to_owned(),
                "-L".into(),
                opt.sysroot.as_ref().unwrap().as_os_str().to_owned(),
                "-E".into(),
                preload,
                opt.binary.as_os_str().to_owned(),
            ];
        }
        in_try = true;
        fs::File::create(opt.output.join("stdout"))?;
        fs::File::create(opt.output.join("stderr"))?;
        let mut spec = CommandSpec::new_os(command.clone(), Duration::MAX);
        spec.inherit_stdin = true;
        spec.os_env = Some(environment.iter().cloned().collect::<BTreeMap<_, _>>());
        platform
            .spawn(
                &spec,
                &opt.output.join("stdout"),
                &opt.output.join("stderr"),
            )
            .map_err(|e| contextual("FileNotFoundError", e))?;
        child = true;
        let deadline = platform.monotonic() + Duration::from_secs(15);
        loop {
            observations.receive(platform, enabled, decoder)?;
            let trace = opt.output.join("trace.jsonl");
            if trace.exists() && read_text(&trace)?.contains("\"runtime-ready\"") {
                break;
            }
            exit = platform.poll()?;
            require(
                exit.is_none(),
                format!(
                    "AssertionError(('startup exit', {}))",
                    exit.unwrap_or_default()
                ),
            )?;
            require(
                platform.monotonic() < deadline,
                "AssertionError('startup timeout')".into(),
            )?;
            platform.sleep(Duration::from_millis(10));
        }
        for &port in enabled {
            while !platform.vision_connect(port)? {
                require(
                    platform.monotonic() < deadline,
                    format!("AssertionError(('VisionIPC connect timeout', {port}))"),
                )?;
                platform.sleep(Duration::from_millis(10));
            }
        }
        fs::write(opt.output.join("go"), "go")?;
        let deadline = platform.monotonic() + Duration::from_secs(10);
        loop {
            exit = platform.poll()?;
            if exit.is_some() {
                break;
            }
            observations.receive(platform, enabled, decoder)?;
            let barrier = opt.output.join("register-go");
            if opt.case == "publication-order"
                && !barrier.exists()
                && barrier.with_extension("ready").exists()
            {
                observations.receive(platform, enabled, decoder)?;
                let counts = observations.counts()?;
                write_json(&opt.output.join("publication-order.json"), &counts)?;
                require(
                    json::equal(
                        &counts,
                        &JsonValue::parse("{\"messages\":[0,0,0],\"vision\":[1,0,0]}")?,
                    ),
                    format!("AssertionError({})", repr(&counts)?),
                )?;
                fs::write(barrier, "release")?;
            }
            let signal = match opt.case.as_str() {
                "int" => Some(2),
                "pwr" => Some(30),
                "poll-error" => None,
                _ => Some(15),
            };
            if let Some(signal) = signal {
                if sent_signal.is_none()
                    && enabled.iter().all(|&port| {
                        observations.messages[port].len() >= 5
                            && observations.images[port].len() >= 5
                    })
                {
                    platform.signal(signal)?;
                    sent_signal = Some(signal);
                }
            }
            require(
                platform.monotonic() < deadline,
                "AssertionError('runtime timeout')".into(),
            )?;
            platform.sleep(Duration::from_millis(2));
        }
        observations.receive(platform, enabled, decoder)?;
        platform.sleep(Duration::from_millis(30));
        observations.receive(platform, enabled, decoder)?;
        require(
            exit == Some(0),
            format!(
                "AssertionError(('runtime exit', {}))",
                exit.unwrap_or_default()
            ),
        )?;
        Ok(())
    })();
    // Source finally runs only after try begins; native additionally releases its
    // own early-created resources if collector setup fails before that boundary.
    let finalize = (|| -> Result<()> {
        if in_try {
            if child {
                exit = platform.poll()?;
                if exit.is_none() {
                    platform.kill()?;
                    exit = Some(platform.wait()?);
                }
            }
            observations.persist(&opt.output)?;
            let failure = match &outcome {
                Ok(()) => null(),
                Err(cause) => JsonValue::text(&exception_repr(cause.as_ref())?),
            };
            write_json(
                &opt.output.join("run.json"),
                &object(&[
                    (
                        "command".into(),
                        json::parse_text(&crate::camera_exposure_check::shared::argv_json(
                            &command,
                        )?)?,
                    ),
                    ("environment".into(), environment_json(&environment)?),
                    ("signal".into(), sent_signal.map_or_else(null, number)),
                    ("exit".into(), exit.map_or_else(null, number)),
                    ("failure".into(), failure),
                ])?,
            )?;
        }
        Ok(())
    })();
    // These methods act only on resources this invocation successfully owned.
    let cleanup = (|| -> Result<()> {
        if logger {
            platform.logger_close()?;
            platform.context_term()?;
        }
        platform.remove_namespace(&namespace)?;
        if bound {
            platform.unlink_endpoint(&endpoint_path)?;
        }
        Ok(())
    })();
    finalize?;
    cleanup?;
    outcome
}
