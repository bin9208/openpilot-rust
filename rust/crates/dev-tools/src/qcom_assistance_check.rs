//! Owned loopback/file checks for the native assistance worker.
use crate::{argparse_scan::scan_os_with_positionals, python_fs, seal::Result};
use openpilot_log_format::JsonValue;
use rustix::process::{kill_process, Pid, Signal};
use sha2::{Digest, Sha256};
use std::{
    ffi::OsString,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    time::{Duration, Instant},
};
mod server;
const MODES: [&str; 4] = ["alternate", "http404", "oversize", "retry"];
const BODY: &[u8] = b"synthetic assistance";
const ALTERNATE: &[u8] = b"alternate replaces stale";
pub struct Options {
    pub binary: PathBuf,
    pub output: PathBuf,
}
pub fn parse(args: &[OsString]) -> Result<Option<Options>> {
    let parsed = scan_os_with_positionals(args, &[], 2)?;
    Ok((!parsed.help).then(|| Options {
        binary: parsed.positionals[0].clone().into(),
        output: parsed.positionals[1].clone().into(),
    }))
}
/// The retired checker may be absent; a present copy must retain its provenance.
pub fn validate_source(root: &Path) -> Result<()> {
    let path = root.join("rust/tools/check_qcomgps_assistance.py");
    match fs::symlink_metadata(&path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
        Ok(_) => (),
    }
    let hash = format!("{:x}", Sha256::digest(fs::read(path)?));
    require(
        hash == "6fb7f5f62070d99d2c7443ec97467619b96d46e2031cba8da750d123caf53805",
        "assistance checker source changed",
    )
}
pub fn configuration(root: &Path, port: u16, mode: &str) -> Result<JsonValue> {
    require(MODES.contains(&mode), "unknown assistance scenario")?;
    let path = |name: &str| python_fs::json_string(root.join(name).as_os_str());
    // Filesystem strings retain Unix surrogateescape codepoints through JSON.
    Ok(JsonValue::parse(&format!(
        concat!(
            "{{\"at\":{{\"path\":{},\"lock\":{}}},\"diagnostic\":{},",
            "\"nmea\":{},\"root\":{},\"assistance\":{},",
            "\"assistance_url\":\"http://127.0.0.1:{}/assist\",\"alternate\":{},",
            "\"mmcli\":{},\"systemd\":{},\"cold_start\":false}}"
        ),
        path("unused-at")?,
        path("unused-lock")?,
        path("unused-diag")?,
        path("unused-nmea")?,
        python_fs::json_string(root.as_os_str())?,
        path("assist")?,
        port,
        if mode == "alternate" {
            path("alternate")?
        } else {
            "null".into()
        },
        path("unused-mmcli")?,
        path("unused-systemd")?,
    ))?)
}
fn require(condition: bool, message: &'static str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}
/// Preserve the original artifact assertions, not just the summary's byte count.
pub fn validate_artifacts(root: &Path, mode: &str, attempts: &[String], code: i32) -> Result<()> {
    require(code == 0, "assistance worker failed")?;
    let path = root.join("assist");
    match mode {
        "alternate" => require(
            fs::read(path)? == fs::read(root.join("alternate"))? && attempts.is_empty(),
            "alternate assistance content or HTTP requests differ",
        ),
        "oversize" => require(
            !path.exists() && fs::metadata(root.join("assist.download"))?.len() > 100_000,
            "oversized assistance file lifecycle differs",
        ),
        "http404" | "retry" => require(
            fs::read(path)? == BODY && attempts.len() == if mode == "retry" { 2 } else { 1 },
            "downloaded assistance content or retry count differs",
        ),
        _ => Err("unknown assistance scenario".into()),
    }
}
struct OwnedChild(Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}
fn wait(child: &mut Child) -> Result<ExitStatus> {
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        require(
            start.elapsed() < Duration::from_secs(3),
            "assistance worker shutdown timeout",
        )?;
        std::thread::sleep(Duration::from_millis(2));
    }
}
fn scenario(binary: &Path, output: &Path, mode: &str) -> Result<()> {
    fs::create_dir_all(output)?;
    let temporary = tempfile::Builder::new()
        .prefix("qcom-assistance-")
        .tempdir()?;
    let root = temporary.path();
    let server = server::Server::start(mode)?;
    let path = root.join("assist");
    fs::write(&path, b"stale removed on worker startup")?;
    fs::write(root.join("alternate"), ALTERNATE)?;
    let config = configuration(root, server.port, mode)?.to_json()?;
    let log = fs::File::create(output.join("worker.log"))?;
    let mut child = OwnedChild(
        Command::new(binary)
            .arg("--assistance-worker")
            .arg(config)
            .stdout(Stdio::from(log.try_clone()?))
            .stderr(Stdio::from(log))
            .spawn()?,
    );
    let status = if mode == "alternate" {
        wait(&mut child.0)?
    } else {
        let start = Instant::now();
        loop {
            require(
                child.0.try_wait()?.is_none(),
                "assistance worker exited before completion",
            )?;
            let complete = if mode == "oversize" {
                fs::read_to_string(output.join("worker.log"))?.contains("larger than expected")
            } else {
                path.exists() && fs::read(&path)? == BODY
            };
            if complete {
                break;
            }
            require(
                start.elapsed() < Duration::from_secs(15),
                "assistance deadline exceeded",
            )?;
            std::thread::sleep(Duration::from_millis(20));
        }
        kill_process(Pid::from_child(&child.0), Signal::USR1)?;
        wait(&mut child.0)?
    };
    let attempts = server.finish()?;
    validate_artifacts(root, mode, &attempts, status.code().unwrap_or(-1))?;
    let final_bytes = if path.exists() {
        Some(fs::metadata(path)?.len())
    } else {
        None
    };
    let result = serde_json::json!({"mode":mode,"attempts":attempts,
        "returncode":status.code(),"final_bytes":final_bytes,"pass":true});
    fs::write(
        output.join("result.json"),
        format!("{}\n", serde_json::to_string_pretty(&result)?),
    )?;
    Ok(())
}
fn repository_root() -> PathBuf {
    std::env::var_os("OPENPILOT_RUST_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."))
}
pub fn run(binary: &Path, output: &Path) -> Result<()> {
    validate_source(&repository_root())?;
    for mode in MODES {
        scenario(binary, &output.join(mode), mode)?;
        println!("PASS: {mode}");
        io::stdout().flush()?;
    }
    Ok(())
}
pub fn cli() -> Result<()> {
    match parse(&std::env::args_os().skip(1).collect::<Vec<_>>())? {
        Some(options) => run(&options.binary, &options.output),
        None => {
            println!("usage: check-qcomgps-assistance [-h] binary output\n\nNative assistance worker file/HTTP behavior with private files and loopback only.\n\npositional arguments:\n  binary\n  output\n\noptions:\n  -h, --help  show this help message and exit");
            Ok(())
        }
    }
}
