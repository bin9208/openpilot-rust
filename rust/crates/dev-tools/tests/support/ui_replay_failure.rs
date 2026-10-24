//! Failure-only original replay diagnostics. Never retry or replace replay errors.
use openpilot_dev_tools::{
    qa_process::{CaptureError, Captured, CommandSpec, Runner},
    seal::Result,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    fs,
    os::unix::{ffi::OsStrExt, fs::MetadataExt},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

const ENVIRONMENT: &[&str] = &[
    "DISPLAY",
    "XAUTHORITY",
    "LIBGL_ALWAYS_SOFTWARE",
    "GALLIUM_DRIVER",
    "LP_NUM_THREADS",
    "LP_NATIVE_VECTOR_WIDTH",
    "MESA_GL_VERSION_OVERRIDE",
    "MESA_GLSL_VERSION_OVERRIDE",
    "LANG",
    "LC_ALL",
    "LC_NUMERIC",
    "LD_LIBRARY_PATH",
    "STARTUP_UI_RAYLIB_ROOT",
    "STARTUP_UI_RAYLIB_LIBRARY",
    "UI_FORMS_RAW_HELPER",
    "UI_RENDER_RAW_HELPER",
    "OPENPILOT_RUST_ROOT",
];
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn file_hash(path: &Path) -> Option<String> {
    fs::read(path).ok().map(|bytes| hash(&bytes))
}
fn hex(value: &OsStr) -> String {
    value
        .as_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn os_json(value: &OsStr) -> Result<String> {
    openpilot_dev_tools::python_fs::json_string(value)
}
fn write_json(path: &Path, value: &Value) -> Result<()> {
    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}
fn environment(spec: &CommandSpec) -> BTreeMap<&'static str, Option<OsString>> {
    let mut result = ENVIRONMENT
        .iter()
        .map(|&key| (key, std::env::var_os(key)))
        .collect::<BTreeMap<_, _>>();
    for (key, value) in spec.effective_env() {
        if let Some(slot) = key.to_str().and_then(|key| result.get_mut(key)) {
            *slot = Some(value);
        }
    }
    for key in &spec.env_remove {
        if let Some(slot) = key.to_str().and_then(|key| result.get_mut(key)) {
            *slot = None;
        }
    }
    result
}
fn metadata(path: &Path) -> Value {
    match fs::metadata(path) {
        Ok(value) => {
            json!({"path_hex":hex(path.as_os_str()),"exists":true,"device":value.dev(),"inode":value.ino(),"mode":value.mode(),"bytes":value.len(),"modified_seconds":value.mtime(),"modified_nanos":value.mtime_nsec()})
        }
        Err(error) => {
            json!({"path_hex":hex(path.as_os_str()),"exists":false,"error":error.to_string(),"errno":error.raw_os_error()})
        }
    }
}
fn display_state(env: &BTreeMap<&str, Option<OsString>>) -> Value {
    let display = env.get("DISPLAY").and_then(Option::as_ref);
    let number = display
        .and_then(|v| v.to_str())
        .and_then(|v| v.strip_prefix(':'))
        .and_then(|v| v.split('.').next())
        .filter(|v| !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit()));
    let authority = env.get("XAUTHORITY").and_then(Option::as_ref);
    json!({
        "wall_nanos":SystemTime::now().duration_since(UNIX_EPOCH).ok().map(|d|d.as_nanos().to_string()),
        "display_hex":display.map(|v|hex(v)),
        "socket":number.map(|n|metadata(Path::new(&format!("/tmp/.X11-unix/X{n}")))),
        "lock":number.map(|n|metadata(Path::new(&format!("/tmp/.X{n}-lock")))),
        // Record only metadata. Never read/copy/hash Xauthority cookie contents.
        "xauthority":authority.map(|p|metadata(Path::new(p))),
    })
}

pub struct Evidence<'a> {
    pub spec: &'a CommandSpec,
    pub trace: &'a Value,
    pub capture: &'a str,
    pub root: &'a Path,
    pub directory: &'a Path,
    pub output: Option<PathBuf>,
}
struct Failure<'a> {
    phase: &'a str,
    exit: Option<i32>,
    stdout: &'a [u8],
    stderr: &'a [u8],
    error: &'a str,
}
impl Evidence<'_> {
    pub fn checked<T>(
        &self,
        runner: &mut dyn Runner,
        validate: impl FnOnce(&Captured) -> Result<T>,
    ) -> Result<(Captured, T)> {
        let env = environment(self.spec);
        let before = self.output.as_ref().map(|_| display_state(&env));
        let captured = match runner.run(self.spec) {
            Ok(value) => value,
            Err(error) => {
                let (stdout, stderr) = match error.downcast_ref::<CaptureError>() {
                    Some(CaptureError::Timeout { stdout, stderr, .. }) => {
                        (stdout.as_slice(), stderr.as_slice())
                    }
                    _ => (b"".as_slice(), b"".as_slice()),
                };
                self.failure(
                    &env,
                    before.as_ref(),
                    Failure {
                        phase: "transport",
                        exit: None,
                        stdout,
                        stderr,
                        error: &error.to_string(),
                    },
                );
                return Err(error);
            }
        };
        match validate(&captured) {
            Ok(value) => Ok((captured, value)),
            Err(error) => {
                self.failure(
                    &env,
                    before.as_ref(),
                    Failure {
                        phase: "replay",
                        exit: Some(captured.code),
                        stdout: &captured.stdout,
                        stderr: &captured.stderr,
                        error: &error.to_string(),
                    },
                );
                Err(error)
            }
        }
    }
    fn failure(
        &self,
        env: &BTreeMap<&str, Option<OsString>>,
        before: Option<&Value>,
        failure: Failure<'_>,
    ) {
        if let Some(output) = &self.output {
            if let Err(retention_error) = self.persist(output, env, before, &failure) {
                eprintln!("could not retain original replay failure evidence: {retention_error}");
            }
        }
    }
    fn persist(
        &self,
        output: &Path,
        env: &BTreeMap<&str, Option<OsString>>,
        before: Option<&Value>,
        failure: &Failure<'_>,
    ) -> Result<()> {
        let Failure {
            phase,
            exit,
            stdout,
            stderr,
            error,
        } = failure;
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        fs::create_dir_all(output)?;
        let destination = output.join(format!(
            "original-replay-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&destination)?;
        fs::write(destination.join("stdout.log"), stdout)?;
        fs::write(destination.join("stderr.log"), stderr)?;
        fs::write(
            destination.join("commands.txt"),
            self.spec.input.as_deref().unwrap_or_default(),
        )?;
        fs::write(destination.join("original-draw-capture.json"), self.capture)?;
        write_json(&destination.join("original-trace.json"), self.trace)?;
        fs::write(
            destination.join("input-provenance.json"),
            include_bytes!("../fixtures/ui-forms/input-provenance.json"),
        )?;
        let argv = self.spec.effective_argv();
        write_json(
            &destination.join("execution.json"),
            &json!({
                "phase":phase,"exit":exit,"error":error,"timeout_millis":self.spec.timeout.as_millis(),
                "helper_sha256":argv.first().and_then(|p|file_hash(Path::new(p))),
                "library_sha256":argv.get(1).and_then(|p|file_hash(Path::new(p))),
                "capture_sha256":hash(self.capture.as_bytes()),
                "commands_sha256":hash(self.spec.input.as_deref().unwrap_or_default()),
                "raw_helper_source_sha256":file_hash(&self.root.join("rust/crates/dev-tools/tests/fixtures/ui-forms/raw-raylib.cc")),
                "cargo_lock_sha256":file_hash(&self.root.join("rust/Cargo.lock")),
                "cwd_hex":self.spec.cwd.as_ref().map(|p|hex(p.as_os_str())),
            }),
        )?;
        let args = argv
            .iter()
            .map(|arg| os_json(arg))
            .collect::<Result<Vec<_>>>()?
            .join(",");
        fs::write(destination.join("argv.json"), format!("[{args}]\n"))?;
        let variables = env
            .iter()
            .map(|(key, value)| {
                Ok(format!(
                    "{}:{}",
                    os_json(OsStr::new(key))?,
                    value
                        .as_ref()
                        .map(|v| os_json(v))
                        .transpose()?
                        .unwrap_or_else(|| "null".into())
                ))
            })
            .collect::<Result<Vec<_>>>()?
            .join(",");
        fs::write(
            destination.join("environment.json"),
            format!("{{{variables}}}\n"),
        )?;
        write_json(
            &destination.join("before-display.json"),
            before.unwrap_or(&Value::Null),
        )?;
        write_json(&destination.join("after-display.json"), &display_state(env))?;
        let artifacts = destination.join("artifacts");
        fs::create_dir(&artifacts)?;
        for file in fs::read_dir(self.directory)? {
            let file = file?;
            if file.file_type()?.is_file()
                && file
                    .path()
                    .extension()
                    .is_some_and(|v| v == "json" || v == "png")
            {
                fs::copy(file.path(), artifacts.join(file.file_name()))?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openpilot_dev_tools::qa_process::NativeRunner;
    use std::{fmt, time::Duration};

    fn destination(root: &Path) -> PathBuf {
        let rows = fs::read_dir(root)
            .unwrap()
            .map(|v| v.unwrap().path())
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), 1);
        rows[0].clone()
    }
    #[derive(Debug)]
    struct OriginalError;
    impl fmt::Display for OriginalError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("original validation error")
        }
    }
    impl std::error::Error for OriginalError {}

    #[test]
    fn success_writes_no_evidence_and_retention_io_error_keeps_original_error() {
        let temp = tempfile::tempdir().unwrap();
        let retained = temp.path().join("retained");
        fs::write(&retained, b"unrelated output file").unwrap();
        let marker = temp.path().join("count");
        let mut spec = CommandSpec::new(
            vec![
                "/bin/sh".into(),
                "-c".into(),
                "printf x >> \"$COUNT\"; printf stdout; printf stderr >&2".into(),
            ],
            Duration::from_secs(3),
        );
        spec.set_os_env(
            [(OsString::from("COUNT"), marker.into_os_string())]
                .into_iter()
                .collect(),
        );
        let trace = json!({"name":"controlled-success"});
        let evidence = Evidence {
            spec: &spec,
            trace: &trace,
            capture: "{}",
            root: temp.path(),
            directory: temp.path(),
            output: Some(retained.clone()),
        };
        let (captured, value) = evidence.checked(&mut NativeRunner, |_| Ok(23)).unwrap();
        assert_eq!(
            (captured.code, captured.stdout, captured.stderr, value),
            (0, b"stdout".to_vec(), b"stderr".to_vec(), 23)
        );
        assert_eq!(fs::read(&retained).unwrap(), b"unrelated output file");
        let error = evidence
            .checked::<()>(&mut NativeRunner, |_| Err(Box::new(OriginalError)))
            .unwrap_err();
        assert!(error.is::<OriginalError>());
        assert_eq!(error.to_string(), "original validation error");
        assert_eq!(fs::read(&retained).unwrap(), b"unrelated output file");
        assert_eq!(fs::read(temp.path().join("count")).unwrap(), b"xx");
    }

    #[test]
    fn spawn_failure_keeps_io_error_and_does_not_call_validator() {
        let temp = tempfile::tempdir().unwrap();
        let spec = CommandSpec::new_os(
            vec![temp.path().join("absent-helper").into_os_string()],
            Duration::from_secs(3),
        );
        let trace = json!({"name":"controlled-spawn-failure"});
        let evidence = Evidence {
            spec: &spec,
            trace: &trace,
            capture: "{}",
            root: temp.path(),
            directory: temp.path(),
            output: Some(temp.path().join("retained")),
        };
        let error = evidence
            .checked::<()>(&mut NativeRunner, |_| {
                panic!("spawn failure must not validate")
            })
            .unwrap_err();
        assert_eq!(
            error.downcast_ref::<std::io::Error>().unwrap().kind(),
            std::io::ErrorKind::NotFound
        );
        let saved = destination(&temp.path().join("retained"));
        assert!(fs::read(saved.join("stdout.log")).unwrap().is_empty());
        assert!(fs::read(saved.join("stderr.log")).unwrap().is_empty());
        let execution: Value =
            serde_json::from_slice(&fs::read(saved.join("execution.json")).unwrap()).unwrap();
        assert_eq!(execution["phase"], "transport");
        assert_eq!(execution["exit"], Value::Null);
        assert_eq!(execution["error"], error.to_string());
    }

    #[test]
    fn timeout_retains_only_the_partial_bytes_exposed_by_runner() {
        let temp = tempfile::tempdir().unwrap();
        let mut spec=CommandSpec::new(vec!["/bin/sh".into(),"-c".into(),"printf x >> \"$COUNT\"; printf 'partial\\000'; printf diagnostic >&2; exec /bin/sleep 10".into()],Duration::from_millis(250));
        spec.set_os_env(
            [(
                OsString::from("COUNT"),
                temp.path().join("count").into_os_string(),
            )]
            .into_iter()
            .collect(),
        );
        let trace = json!({"name":"controlled-timeout"});
        let evidence = Evidence {
            spec: &spec,
            trace: &trace,
            capture: "{}",
            root: temp.path(),
            directory: temp.path(),
            output: Some(temp.path().join("retained")),
        };
        let error = evidence
            .checked::<()>(&mut NativeRunner, |_| panic!("timeout must not validate"))
            .unwrap_err();
        let CaptureError::Timeout {
            timeout,
            stdout,
            stderr,
        } = error.downcast_ref::<CaptureError>().unwrap()
        else {
            panic!("expected original timeout type")
        };
        assert_eq!(*timeout, Duration::from_millis(250));
        assert_eq!(stdout, b"partial\0");
        assert_eq!(stderr, b"diagnostic");
        let saved = destination(&temp.path().join("retained"));
        assert_eq!(fs::read(saved.join("stdout.log")).unwrap(), *stdout);
        assert_eq!(fs::read(saved.join("stderr.log")).unwrap(), *stderr);
        assert_eq!(fs::read(temp.path().join("count")).unwrap(), b"x");
    }
}
