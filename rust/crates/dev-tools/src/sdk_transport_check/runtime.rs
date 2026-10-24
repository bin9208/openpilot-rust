//! Real candidate HTTP events and independent source policy, with owned teardown.
use super::{expected, field, verify, verify_dirty};
use crate::{
    qa_process::{CommandSpec, NativeRunner},
    sdk_receiver::{Captured, Receiver},
    seal::Result,
    tombstone_reference_check::{Options, Pair},
    version_reference_check::{self, process::Cancellation},
};
use openpilot_log_format::{JsonValue as J, JsonView};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::Path,
    sync::mpsc::RecvTimeoutError,
    time::{Duration, Instant},
};
fn object(fields: &[(&str, J)]) -> Result<J> {
    let fields = fields
        .iter()
        .map(|(key, value)| Ok(format!("{}:{}", J::text(key).to_json()?, value.to_json()?)))
        .collect::<Result<Vec<_>>>()?;
    Ok(J::parse(&format!("{{{}}}", fields.join(",")))?)
}
fn array(values: &[J]) -> Result<J> {
    Ok(J::parse(&format!(
        "[{}]",
        values
            .iter()
            .map(J::to_json)
            .collect::<std::result::Result<Vec<_>, _>>()?
            .join(",")
    ))?)
}
fn call(pair: &mut Pair, op: &str, fields: &[(&str, J)]) -> Result<J> {
    let mut request = vec![("op", J::text(op))];
    request.extend_from_slice(fields);
    pair.call(&object(&request)?, false)
}
fn capture(receiver: &Receiver, cancel: &Cancellation) -> Result<Captured> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        cancel.check()?;
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("SDK HTTP event timeout".into());
        }
        match receiver.receive(remaining.min(Duration::from_millis(20))) {
            Ok(value) => return Ok(value),
            Err(error)
                if error.downcast_ref::<RecvTimeoutError>() == Some(&RecvTimeoutError::Timeout) => {
            }
            Err(error) => return Err(error),
        }
    }
}
fn values() -> Result<Vec<J>> {
    let mut values = ["true", "null", "0", "NaN", "Infinity", "[true,17.5,null]"]
        .iter()
        .map(|v| J::parse(v).map_err(Into::into))
        .collect::<Result<Vec<_>>>()?;
    values[2] = J::parse(&format!("1{}", "0".repeat(100)))?;
    values.push(object(&[
        ("key", J::text("value")),
        (
            "list",
            J::parse(&format!(
                "[{}]",
                (0..20).map(|v| v.to_string()).collect::<Vec<_>>().join(",")
            ))?,
        ),
    ])?);
    values.extend([
        J::text(&"A".repeat(8192)),
        J::text(&"A".repeat(8193)),
        J::text(&"한".repeat(2731)),
        J::text(&format!("{}😀tail", "A".repeat(8188))),
        J::codepoints(vec![0xd800]).ok_or("invalid surrogate fixture")?,
        J::codepoints(
            std::iter::once(0xd800)
                .chain(std::iter::repeat_n(u32::from('X'), 8192))
                .collect(),
        )
        .ok_or("invalid surrogate fixture")?,
    ]);
    Ok(values)
}
fn git_fixture(path: &Path, options: &Options, cancel: &Cancellation) -> Result<()> {
    fs::remove_file(path.join("build.json"))?;
    fs::write(path.join("RELEASES.md"), b"fixture notes\n\nolder notes\n")?;
    let mut env = options.environment.clone();
    for (key, value) in [
        ("GIT_CONFIG_GLOBAL", "/dev/null"),
        ("GIT_CONFIG_SYSTEM", "/dev/null"),
        ("GIT_CONFIG_NOSYSTEM", "1"),
        ("GIT_AUTHOR_DATE", "2020-01-02T03:04:05+00:00"),
        ("GIT_COMMITTER_DATE", "2020-01-02T03:04:05+00:00"),
    ] {
        env.insert(key.into(), value.into());
    }
    for args in [
        &["init", "-b", "main"][..],
        &["config", "user.name", "Fixture"],
        &["config", "user.email", "fixture@example.invalid"],
        &["config", "core.hooksPath", "/dev/null"],
        &["config", "commit.gpgsign", "false"],
        &["add", "."],
        &["commit", "-m", "fixture"],
        &[
            "remote",
            "add",
            "origin",
            "git@github.com:commaai/openpilot.git",
        ],
    ] {
        let mut spec = CommandSpec::new_os(
            std::iter::once("git")
                .chain(args.iter().copied())
                .map(Into::into)
                .collect(),
            Duration::from_secs(10),
        );
        spec.cwd = Some(path.into());
        spec.set_os_env(env.clone());
        let result = NativeRunner.run_controlled(
            &spec,
            Instant::now() + Duration::from_secs(10),
            &cancel.flag(),
        )?;
        if result.code != 0 {
            return Err(format!(
                "SDK source Git fixture exited{}: {}",
                result.code,
                String::from_utf8_lossy(&result.stderr)
            )
            .into());
        }
    }
    Ok(())
}
type SourceHashes = std::collections::BTreeMap<String, String>;
fn seal(repository: &Path) -> Result<(SourceHashes, SourceHashes)> {
    let mut live = crate::tombstone_reference_check::sources(repository)?;
    let mut historical = crate::tombstone_source::historical_sources(repository)?;
    let name = "rust/tools/check_crash_sdk_transport.py";
    let expected = "8bf1281bbc182ecaba9a29b9e49aa3d250ec8ee1f43eca5861596cee475ed806";
    match fs::symlink_metadata(repository.join(name)) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            historical.insert(name.into(), expected.into());
        }
        Err(error) => return Err(error.into()),
        Ok(_) => {
            let actual = format!("{:x}", Sha256::digest(fs::read(repository.join(name))?));
            if actual != expected {
                return Err("original SDK transport schedule changed".into());
            }
            live.insert(name.into(), actual);
        }
    }
    Ok((live, historical))
}
pub fn run(repository: &Path, options: &Options) -> Result<J> {
    fs::create_dir_all(&options.output)?;
    match fs::remove_file(options.output.join("manifest.json")) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    let cancel = Cancellation::new()?;
    let result = run_inner(repository, options, &cancel);
    let signal = cancel.signal();
    if result.is_err() || signal.is_some() {
        match fs::remove_file(options.output.join("manifest.json")) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    if let Some(signal) = signal {
        return Err(version_reference_check::Interrupted(signal).into());
    }
    result
}
fn run_inner(repository: &Path, options: &Options, cancel: &Cancellation) -> Result<J> {
    let (source_pins, historical_pins) = seal(repository)?;
    let mut receiver =
        Receiver::bind(Duration::from_secs(5)).map_err(|e| e as Box<dyn std::error::Error>)?;
    let mut pair = Pair::new(repository, options, &options.output.join("policy"), cancel)?;
    pair.write(Path::new("base/build.json"),br#"{"channel":["nightly",true],"openpilot":{"git_origin":"github.com/commaai/openpilot","git_commit":{"opaque":7,"nested":[null,false]},"version":"ignored-in-release"}}"#,None)?;
    call(
        &mut pair,
        "configure",
        &[
            ("base", J::text("$FIXTURE/base")),
            ("params", J::text("$FIXTURE/params")),
            ("pc", J::parse("false")?),
            ("device", J::text("tici")),
            ("dsn", J::text(&receiver.dsn())),
        ],
    )?;
    call(
        &mut pair,
        "init",
        &[("project", J::text("selfdrive_native"))],
    )?;
    let mut comparisons = Vec::new();
    for (index, value) in values()?.iter().enumerate() {
        call(
            &mut pair,
            "tag",
            &[
                ("key", J::text("unusual")),
                ("value", J::text(&value.to_json()?)),
            ],
        )?;
        let contents = if index % 2 == 0 {
            format!("contents\n{}", "B".repeat(9000))
        } else {
            "한".repeat(3000)
        };
        call(
            &mut pair,
            "tombstone",
            &[
                ("filename", J::text("fixture.crash")),
                ("message", J::text(&format!("message-{index}"))),
                ("contents", J::text(&contents)),
            ],
        )?;
        let event = capture(&receiver, cancel)?;
        verify(index, &event.event, event.auth.as_deref())?;
        fs::write(
            options.output.join(format!("envelope-{index}.txt")),
            event.raw,
        )?;
        comparisons.push(object(&[
            ("case", J::parse(&index.to_string())?),
            ("expected", expected(index)?),
            ("actual", event.event),
            ("passed", J::parse("true")?),
        ])?);
    }
    for path in pair.paths(Path::new("base")) {
        git_fixture(&path, options, cancel)?;
    }
    call(
        &mut pair,
        "init",
        &[("project", J::text("selfdrive_native"))],
    )?;
    let calls = field(
        pair.last_source_record()
            .ok_or("source init record absent")?,
        "calls",
    )?;
    call(
        &mut pair,
        "tombstone",
        &[
            ("filename", J::text("dirty.crash")),
            ("message", J::text("dirty-metadata")),
            ("contents", J::text("short")),
        ],
    )?;
    let dirty = capture(&receiver, cancel)?;
    verify_dirty(&dirty.event, &calls)?;
    fs::write(options.output.join("envelope-dirty.txt"), dirty.raw)?;
    comparisons.push(object(&[
        ("case", J::text("dirty-metadata")),
        ("source_calls", calls),
        ("actual", dirty.event),
        ("passed", J::parse("true")?),
    ])?);
    let missing = pair.root().join("native/actual-missing-file");
    let request = object(&[
        ("op", J::text("capture_io")),
        ("path", crate::python_fs::json_value(missing.as_os_str())),
    ])?;
    // Reporter and barrier use distinct PUSH sockets. A barrier alone cannot
    // establish delivery of capture_io's required crash record (#421).
    let (response, logs) = pair.raw_native(&request, 1)?;
    if !super::equal(&field(&response, "result")?, &J::parse("{\"value\":null}")?) {
        return Err("native I/O capture returned unexpected result".into());
    }
    let error = capture(&receiver, cancel)?;
    fs::write(
        options.output.join("native-error.json"),
        format!(
            "{}\n",
            version_reference_check::format_report(&object(&[
                ("request", request),
                ("response", response),
                ("event", error.event.clone()),
                ("logs", array(&logs)?)
            ])?)?
        ),
    )?;
    fs::write(options.output.join("envelope-native-error.txt"), &error.raw)?;
    let JsonView::Array(exceptions) = field(&field(&error.event, "exception")?, "values")?.view()
    else {
        return Err("native exception list absent".into());
    };
    let exception = exceptions.last().ok_or("empty native exception list")?;
    if !field(exception, "type")?
        .to_utf8()
        .is_some_and(|v| v.starts_with("std::io::"))
        || !field(exception, "value")?
            .to_utf8()
            .is_some_and(|v| v.contains("os error 2"))
    {
        return Err("native exception representation changed".into());
    }
    let extra = field(&error.event, "extra")?;
    if extra.get("rust_backtrace").is_none()
        || extra.get("tombstone").is_none()
        || logs.len() != 1
        || !field(&logs[0], "msg")?.text_eq("crash")
        || fs::read(pair.root().join("native/params/fixture/CarrotException"))? != b"exception"
    {
        return Err("native I/O capture omitted backtrace, log, or Params effect".into());
    }
    pair.finish()?;
    receiver
        .require_empty()
        .map_err(|e| e as Box<dyn std::error::Error>)?;
    receiver
        .close()
        .map_err(|e| e as Box<dyn std::error::Error>)?;
    // All accepted workers have joined. Check once more so a complete envelope
    // decoded during teardown cannot escape the pre-close empty-queue check.
    match receiver.receive(Duration::ZERO) {
        Err(error)
            if matches!(
                error.downcast_ref::<RecvTimeoutError>(),
                Some(RecvTimeoutError::Disconnected | RecvTimeoutError::Timeout)
            ) => {}
        Err(error) => return Err(error),
        Ok(_) => return Err("unexpected SDK envelope during teardown".into()),
    }
    cancel.check()?;
    if seal(repository)? != (source_pins.clone(), historical_pins.clone()) {
        return Err("SDK source policy changed during comparison".into());
    }
    fs::write(
        options.output.join("comparisons.json"),
        format!(
            "{}\n",
            version_reference_check::format_report(&array(&comparisons)?)?
        ),
    )?;
    let mut command = options
        .runner
        .iter()
        .map(|v| crate::python_fs::json_value(v))
        .collect::<Vec<_>>();
    command.push(crate::python_fs::json_value(options.binary.as_os_str()));
    let source_hashes = object(
        &source_pins
            .iter()
            .map(|(name, hash)| (name.as_str(), J::text(hash)))
            .collect::<Vec<_>>(),
    )?;
    let report=object(&[("result",J::text("PASS")),("events",J::parse("14")?),("native_io_exception_events",J::parse("1")?),("binary_sha256",J::text(&format!("{:x}",Sha256::digest(fs::read(&options.binary)?)))),("source_hashes",source_hashes),("historical_source_sha256",J::parse(&serde_json::to_string(&historical_pins)?)?),("transport",J::text("native sentry-rust0.49.3 HTTP to loopback; independent native source policy and pinned original sentry-sdk2.55.0 captured fields")),("preserved",J::parse(r#"["tag JSON types","original user/release/environment","extra values","max_value_length byte/codepoint fallback","_meta len/rem annotations"]"#)?),("excluded_sdk_internals",J::parse(r#"["SDK identity/platform","thread integration","native exception representation","retry/rate-limit/grouping internals"]"#)?),("command",array(&command)?)])?;
    fs::write(
        options.output.join("manifest.json"),
        format!("{}\n", version_reference_check::format_report(&report)?),
    )?;
    Ok(report)
}
