//! Source-derived checkout policy. The candidate checkout-status crate is never imported.
use crate::seal::Result;
use openpilot_log_format::JsonValue;
use openpilot_process_supervision::CapturedCommand;
use serde_json::{json, Value};
use std::{
    ffi::OsString,
    fs,
    io::{BufRead, Read, Write},
    os::unix::{ffi::OsStringExt, process::ExitStatusExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

fn commit(value: String) -> Option<String> {
    (matches!(value.len(), 40 | 64) && value.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| value.to_ascii_lowercase())
}
pub fn parse_metadata(raw: &[u8]) -> Option<String> {
    let value = JsonValue::parse(std::str::from_utf8(raw).ok()?).ok()?;
    commit(value.get("openpilot")?.get("git_commit")?.to_utf8()?)
}
pub fn read_checkout_commit(repo: &Path, launcher: &Path) -> Option<String> {
    let git = match repo.join(".git").try_exists() {
        Ok(exists) => exists,
        Err(error) if matches!(error.raw_os_error(), Some(9 | 40)) => false,
        Err(_) => return None,
    };
    if !git {
        return parse_metadata(&fs::read(repo.join("build.json")).ok()?);
    }
    let command = CapturedCommand {
        launcher: launcher.into(),
        cwd: repo.into(),
        argv: [
            "git",
            "--no-optional-locks",
            "rev-parse",
            "--verify",
            "HEAD^{commit}",
        ]
        .into_iter()
        .map(Into::into)
        .collect(),
    };
    let result = command.capture(Duration::from_secs(1), None).ok()?;
    // Python text=True decodes BOTH streams, including failed Git stderr.
    std::str::from_utf8(&result.stderr).ok()?;
    let text = std::str::from_utf8(&result.stdout).ok()?;
    if !result.status.success() {
        return None;
    }
    commit(
        text.trim_matches(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c))
            .into(),
    )
}
pub struct Status {
    running: Option<String>,
    required: bool,
    candidate: Option<String>,
    next: f64,
}
impl Status {
    pub fn new(running: Option<String>) -> Self {
        Self {
            running,
            required: false,
            candidate: None,
            next: 0.0,
        }
    }
    pub fn running_commit(&self) -> Option<&str> {
        self.running.as_deref()
    }
    pub fn reboot_required(&self) -> bool {
        self.required
    }
    pub fn update_with(&mut self, now: f64, read: impl FnOnce() -> Option<String>) -> bool {
        if now < self.next {
            return self.required;
        }
        self.next = now + 5.0;
        let installed = read();
        let changed = self.running.is_some() && installed.is_some() && installed != self.running;
        self.required = changed && installed == self.candidate;
        self.candidate = if changed { installed } else { None };
        self.required
    }
}
fn path(value: &Value) -> Result<PathBuf> {
    let value = JsonValue::parse(&value.to_string())?;
    let openpilot_log_format::JsonView::Text(text) = value.view() else {
        return Err("path must be text".into());
    };
    let mut bytes = Vec::new();
    for &point in text {
        if (0xdc80..=0xdcff).contains(&point) {
            bytes.push((point - 0xdc00) as u8);
        } else {
            let c = char::from_u32(point).ok_or("unencodable path surrogate")?;
            bytes.extend_from_slice(c.encode_utf8(&mut [0; 4]).as_bytes());
        }
    }
    Ok(OsString::from_vec(bytes).into())
}
fn protocol() -> Result<()> {
    let stdin = std::io::stdin();
    let mut lines = stdin.lock().lines();
    let config: Value =
        serde_json::from_str(&lines.next().ok_or("missing checkout reference config")??)?;
    let repo = path(&config["repo"])?;
    let launcher = path(&config["launcher"])?;
    let mut state: Option<Status> = None;
    println!("{{\"ready\":true}}");
    std::io::stdout().flush()?;
    for line in lines {
        let request: Value = serde_json::from_str(&line?)?;
        let started = Instant::now();
        let mut current = None;
        let mut returned = None;
        match request["op"].as_str() {
            Some("read") => current = read_checkout_commit(&repo, &launcher),
            Some("capture") => state = Some(Status::new(read_checkout_commit(&repo, &launcher))),
            Some("update") => {
                let now =
                    f64::from_bits(request["now_bits"].as_u64().ok_or("missing raw f64 bits")?);
                returned = Some(
                    state
                        .as_mut()
                        .ok_or("capture status first")?
                        .update_with(now, || read_checkout_commit(&repo, &launcher)),
                );
            }
            _ => return Err("invalid checkout reference operation".into()),
        }
        println!(
            "{}",
            json!({"commit":current,"returned":returned,"elapsed":started.elapsed().as_secs_f64(),
            "running_commit":state.as_ref().and_then(Status::running_commit),
            "reboot_required":state.as_ref().is_some_and(Status::reboot_required)})
        );
        std::io::stdout().flush()?;
    }
    Ok(())
}
/// Independent direct exec, deliberately separate from the candidate launcher.
/// Invoked only in a private child, so closing inherited descriptors cannot
/// affect the checker or any other thread.
fn direct_exec(request: &Value, error_path: &Path) -> Result<()> {
    use std::ffi::CString;
    let execute = || -> std::io::Result<()> {
        let cwd = path(&request["cwd"]).map_err(|e| std::io::Error::other(e.to_string()))?;
        std::env::set_current_dir(cwd)?;
        let argv = request["argv"]
            .as_array()
            .ok_or_else(|| std::io::Error::other("argv missing"))?
            .iter()
            .map(|v| path(v).map(|v| v.into_os_string()))
            .collect::<Result<Vec<_>>>()
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        let args = argv
            .iter()
            .map(|v| CString::new(v.as_encoded_bytes()))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let program = argv
            .first()
            .ok_or_else(|| std::io::Error::other("empty argv"))?;
        let mut env: std::collections::BTreeMap<OsString, OsString> = std::env::vars_os().collect();
        if let Some(overlay) = request["environment"].as_array() {
            for entry in overlay {
                env.insert(
                    path(&entry[0])
                        .map_err(|e| std::io::Error::other(e.to_string()))?
                        .into_os_string(),
                    path(&entry[1])
                        .map_err(|e| std::io::Error::other(e.to_string()))?
                        .into_os_string(),
                );
            }
        }
        let environment = env
            .iter()
            .map(|(k, v)| {
                let mut entry = k.clone();
                entry.push("=");
                entry.push(v);
                CString::new(entry.as_encoded_bytes())
            })
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let descriptors = fs::read_dir("/proc/self/fd")?
            .filter_map(|v| v.ok()?.file_name().to_str()?.parse::<i32>().ok())
            .filter(|v| *v > 2)
            .collect::<Vec<_>>();
        for descriptor in descriptors {
            let _ = nix::unistd::close(descriptor);
        }
        let candidates = if program.as_encoded_bytes().contains(&b'/') {
            vec![PathBuf::from(program)]
        } else {
            std::env::split_paths(
                env.get(std::ffi::OsStr::new("PATH"))
                    .map(OsString::as_os_str)
                    .unwrap_or(std::ffi::OsStr::new("/bin:/usr/bin")),
            )
            .map(|p| p.join(program))
            .collect()
        };
        let mut first = None;
        let mut last = std::io::Error::from_raw_os_error(2);
        for candidate in candidates {
            let candidate = CString::new(candidate.as_os_str().as_encoded_bytes())?;
            let error = match nix::unistd::execve(&candidate, &args, &environment) {
                Ok(never) => match never {},
                Err(e) => std::io::Error::from(e),
            };
            if !matches!(error.raw_os_error(), Some(2 | 20)) && first.is_none() {
                first = Some(error);
            } else {
                last = error;
            }
        }
        Err(first.unwrap_or(last))
    };
    if let Err(error) = execute() {
        fs::write(
            error_path,
            serde_json::to_vec(&json!({"kind":"spawn_error","errno":error.raw_os_error()}))?,
        )?;
    }
    Ok(())
}
fn inherited() -> Result<()> {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input)?;
    let request: Value = serde_json::from_str(&input)?;
    let parent = std::env::var_os("PARAMS_COPY_PATH");
    let temp = tempfile::tempdir_in("/tmp")?;
    let config = temp.path().join("request.json");
    let error_path = temp.path().join("error.json");
    fs::write(&config, &input)?;
    let mut child = Command::new(std::env::current_exe()?)
        .arg("--direct-exec")
        .arg(&config)
        .arg(&error_path)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()?;
    let status = child.wait()?;
    let mut response = if error_path.exists() {
        serde_json::from_slice(&fs::read(&error_path)?)?
    } else {
        json!({"kind":"child_exit","code":status.code().unwrap_or_else(|| -status.signal().unwrap_or(0))})
    };
    response["parent_environment_before"] = json!(parent.as_ref().map(|v| v.to_string_lossy()));
    response["parent_environment_after"] =
        json!(std::env::var_os("PARAMS_COPY_PATH").map(|v| v.to_string_lossy().into_owned()));
    fs::write(path(&request["report"])?, serde_json::to_vec(&response)?)?;
    Ok(())
}
pub fn cli() -> Result<()> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    match args.first().and_then(|v| v.to_str()) {
        Some("--inherited") => inherited(),
        Some("--direct-exec") if args.len() == 3 => direct_exec(
            &serde_json::from_slice::<Value>(&fs::read(&args[1])?)?,
            Path::new(&args[2]),
        ),
        None => protocol(),
        _ => Err("invalid checkout reference arguments".into()),
    }
}
