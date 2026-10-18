//! Owned peer IPC, Git fixtures and descriptor/descendant cleanup.
use super::{require, Result};
use command_fds::{CommandFdExt, FdMapping};
use rustix::process::{Pid, Signal, WaitId, WaitIdOptions};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    fs::{self, File},
    io::{Read, Write},
    os::{
        fd::AsRawFd,
        unix::{fs::symlink, process::CommandExt},
    },
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Command, Output, Stdio},
    time::{Duration, Instant},
};
#[derive(Clone)]
pub struct Binaries {
    pub trace: PathBuf,
    pub fixture: PathBuf,
    pub launcher: PathBuf,
    pub reference: PathBuf,
}
fn pid(child: &Child) -> Result<Pid> {
    Pid::from_raw(i32::try_from(child.id())?).ok_or_else(|| "invalid child PID".into())
}
fn exited(child: &Child) -> Result<bool> {
    Ok(rustix::process::waitid(
        WaitId::Pid(pid(child)?),
        WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
    )?
    .is_some())
}
fn terminate(child: &mut Child) -> Result<std::process::ExitStatus> {
    match rustix::process::kill_process_group(pid(child)?, Signal::KILL) {
        Ok(()) | Err(rustix::io::Errno::SRCH) => {}
        Err(e) => return Err(e.into()),
    }
    Ok(child.wait()?)
}
struct Owner(Option<Child>);
impl Drop for Owner {
    fn drop(&mut self) {
        if let Some(child) = &mut self.0 {
            let _ = terminate(child);
        }
    }
}
/// Child streams are drained concurrently; waitid(NOWAIT) reserves the process
/// group until all cleanup signals are complete, including exceptional paths.
pub fn capture(
    mut command: Command,
    input: Option<&[u8]>,
    marker: Option<&File>,
    timeout: Duration,
) -> Result<Output> {
    command
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::inherit()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    if let Some(marker) = marker {
        command.fd_mappings(vec![FdMapping {
            parent_fd: marker.try_clone()?.into(),
            child_fd: marker.as_raw_fd(),
        }])?;
    }
    let mut owner = Owner(Some(command.spawn()?));
    drop(command);
    let child = owner.0.as_mut().ok_or("capture child absent")?;
    if let Some(bytes) = input {
        let mut stdin = child.stdin.take().ok_or("capture stdin absent")?;
        stdin.write_all(bytes)?;
    }
    let mut stdout = child.stdout.take().ok_or("capture stdout absent")?;
    let mut stderr = child.stderr.take().ok_or("capture stderr absent")?;
    for fd in [
        &stdout as &dyn std::os::fd::AsFd,
        &stderr as &dyn std::os::fd::AsFd,
    ] {
        rustix::fs::fcntl_setfl(
            fd,
            rustix::fs::fcntl_getfl(fd)? | rustix::fs::OFlags::NONBLOCK,
        )?;
    }
    let end = Instant::now() + timeout;
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut out_eof = false;
    let mut err_eof = false;
    loop {
        fn drain(stream: &mut dyn Read, bytes: &mut Vec<u8>, eof: &mut bool) -> Result<()> {
            if *eof {
                return Ok(());
            }
            let mut chunk = [0; 32768];
            match stream.read(&mut chunk) {
                Ok(0) => *eof = true,
                Ok(n) => bytes.extend_from_slice(&chunk[..n]),
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                    ) => {}
                Err(e) => return Err(e.into()),
            };
            Ok(())
        }
        drain(&mut stdout, &mut out, &mut out_eof)?;
        drain(&mut stderr, &mut err, &mut err_eof)?;
        if exited(child)? && out_eof && err_eof {
            break;
        }
        require(Instant::now() < end, "checkout command timeout")?;
        std::thread::sleep(Duration::from_millis(1));
    }
    let status = terminate(child)?;
    owner.0.take();
    Ok(Output {
        status,
        stdout: out,
        stderr: err,
    })
}
pub struct Peer {
    pub binaries: Binaries,
    pub output: PathBuf,
    pub root: PathBuf,
    pub repo: PathBuf,
    pub control: PathBuf,
    pub bin: PathBuf,
    pub fallback: PathBuf,
    pub launches: PathBuf,
    pub held: File,
    pub env: BTreeMap<OsString, OsString>,
    temporary: tempfile::TempDir,
    child: Owner,
    input: Option<ChildStdin>,
    output_pipe: ChildStdout,
    pending: Vec<u8>,
    events: Vec<String>,
    closed: bool,
}
impl Peer {
    pub fn new(
        binaries: &Binaries,
        implementation: &str,
        output: &Path,
        fixture_mode: bool,
    ) -> Result<Self> {
        let output = output.join(implementation);
        fs::create_dir_all(output.parent().ok_or("output parent absent")?)?;
        fs::create_dir(&output)?;
        let temporary = tempfile::Builder::new()
            .prefix("checkout-status-")
            .tempdir_in("/tmp")?;
        let root = temporary.path().to_owned();
        let repo = root.join("repo");
        let control = root.join("control");
        let bin = root.join("bin");
        let fallback = root.join("fallback-bin");
        let launches = root.join("launch-descriptors");
        for p in [&repo, &control, &bin, &fallback, &launches] {
            fs::create_dir(p)?;
        }
        symlink(
            crate::path_resolve::resolve_non_strict(&binaries.fixture)?,
            bin.join("git"),
        )?;
        let held = File::create(root.join("held-descriptor"))?;
        let mut env = std::env::vars_os()
            .filter(|(k, _)| !k.as_encoded_bytes().starts_with(b"GIT_"))
            .collect::<BTreeMap<_, _>>();
        for (k, v) in [
            ("GIT_CONFIG_GLOBAL", "/dev/null"),
            ("GIT_CONFIG_SYSTEM", "/dev/null"),
            ("GIT_AUTHOR_NAME", "checkout fixture"),
            ("GIT_AUTHOR_EMAIL", "fixture@example.invalid"),
            ("GIT_COMMITTER_NAME", "checkout fixture"),
            ("GIT_COMMITTER_EMAIL", "fixture@example.invalid"),
            ("GIT_AUTHOR_DATE", "2000-01-01T00:00:00+00:00"),
            ("GIT_COMMITTER_DATE", "2000-01-01T00:00:00+00:00"),
            ("MANAGER_DAEMON", "checkout-parent"),
        ] {
            env.insert(k.into(), v.into());
        }
        env.insert(
            "CHECKOUT_GIT_FIXTURE".into(),
            control.clone().into_os_string(),
        );
        env.insert(
            "CHECKOUT_HELD_FD".into(),
            held.as_raw_fd().to_string().into(),
        );
        env.insert("TMPDIR".into(), launches.clone().into_os_string());
        env.insert(
            "PATH".into(),
            if fixture_mode {
                std::env::join_paths([&bin, &fallback])?
            } else {
                "/usr/bin:/bin".into()
            },
        );
        let binary = if implementation == "reference" {
            &binaries.reference
        } else {
            &binaries.trace
        };
        let binary = crate::path_resolve::resolve_non_strict(binary)?;
        let mut command = Command::new(&binary);
        command
            .env_clear()
            .envs(&env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(File::create(output.join("stderr.log"))?)
            .process_group(0);
        command.fd_mappings(vec![FdMapping {
            parent_fd: held.try_clone()?.into(),
            child_fd: held.as_raw_fd(),
        }])?;
        let mut child = Owner(Some(command.spawn()?));
        drop(command);
        let input = child
            .0
            .as_mut()
            .and_then(|c| c.stdin.take())
            .ok_or("peer stdin absent")?;
        let pipe = child
            .0
            .as_mut()
            .and_then(|c| c.stdout.take())
            .ok_or("peer stdout absent")?;
        rustix::fs::fcntl_setfl(
            &pipe,
            rustix::fs::fcntl_getfl(&pipe)? | rustix::fs::OFlags::NONBLOCK,
        )?;
        let environment = env
            .iter()
            .filter(|(key, _)| {
                let key = key.as_encoded_bytes();
                key.starts_with(b"GIT_")
                    || key.starts_with(b"CHECKOUT_")
                    || matches!(key, b"PATH" | b"MANAGER_DAEMON" | b"TMPDIR")
            })
            .map(|(key, value)| -> Result<String> {
                Ok(format!(
                    "{}:{}",
                    crate::python_fs::json_string(key)?,
                    crate::python_fs::json_string(value)?
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        fs::write(
            output.join("command.json"),
            format!(
                "{{\"command\":[{}],\"repo\":{},\"pid\":{},\"environment\":{{{}}}}}\n",
                crate::python_fs::json_string(binary.as_os_str())?,
                crate::python_fs::json_string(repo.as_os_str())?,
                child.0.as_ref().ok_or("peer child absent")?.id(),
                environment.join(",")
            ),
        )?;
        let mut peer = Self {
            binaries: binaries.clone(),
            output,
            root,
            repo,
            control,
            bin,
            fallback,
            launches,
            held,
            env,
            temporary,
            child,
            input: Some(input),
            output_pipe: pipe,
            pending: vec![],
            events: vec![],
            closed: false,
        };
        peer.send(&json!({"repo":peer.repo,"launcher":crate::path_resolve::resolve_non_strict(&binaries.launcher)?}))?;
        require(
            peer.read()? == json!({"ready":true}),
            "checkout peer did not report ready",
        )?;
        Ok(peer)
    }
    fn send(&mut self, request: &Value) -> Result<()> {
        let input = self.input.as_mut().ok_or("peer stdin closed")?;
        writeln!(input, "{request}")?;
        input.flush()?;
        Ok(())
    }
    fn read(&mut self) -> Result<Value> {
        let deadline = Instant::now() + Duration::from_secs(4);
        loop {
            if let Some(i) = self.pending.iter().position(|v| *v == b'\n') {
                let line = self.pending.drain(..=i).collect::<Vec<_>>();
                return Ok(serde_json::from_slice(&line)?);
            }
            let mut buffer = [0; 8192];
            match self.output_pipe.read(&mut buffer) {
                Ok(0) => {
                    return Err(format!(
                        "checkout peer EOF: {}",
                        String::from_utf8_lossy(&fs::read(self.output.join("stderr.log"))?)
                    )
                    .into())
                }
                Ok(n) => self.pending.extend_from_slice(&buffer[..n]),
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                    ) => {}
                Err(e) => return Err(e.into()),
            }
            require(Instant::now() < deadline, "checkout RPC timeout")?;
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    pub fn record(&mut self, value: Value) -> Result<()> {
        self.record_encoded(value.to_string())
    }
    fn record_encoded(&mut self, encoded: String) -> Result<()> {
        // Raw command filenames may contain surrogateescape strings which
        // serde_json::Value rejects. Preserve already encoded JSON verbatim.
        self.events.push(encoded);
        fs::write(
            self.output.join("transcript.json"),
            format!("[{}]\n", self.events.join(",\n")),
        )?;
        Ok(())
    }
    pub fn write(&mut self, raw: &[u8]) -> Result<()> {
        fs::write(self.repo.join("build.json"), raw)?;
        self.record(json!({"write":"build.json","hex":raw.iter().map(|b|format!("{b:02x}")).collect::<String>()}))
    }
    pub fn metadata(&mut self, commit: &str) -> Result<()> {
        let text = serde_json::to_string(&json!({"openpilot":{"git_commit":commit}}))?;
        self.write(
            openpilot_log_format::JsonValue::parse(&text)?
                .to_json()?
                .as_bytes(),
        )
    }
    pub fn op(&mut self, op: &str, now: f64) -> Result<Value> {
        let mut request = json!({"op":op});
        if op == "update" {
            request["now_bits"] = json!(now.to_bits());
        }
        self.send(&request)?;
        let response = self.read()?;
        let child_pid = self.child.0.as_ref().ok_or("peer child absent")?.id();
        let mut children = Vec::new();
        for task in fs::read_dir(format!("/proc/{child_pid}/task"))? {
            children.extend(
                fs::read_to_string(task?.path().join("children"))?
                    .split_whitespace()
                    .map(str::to_owned),
            );
        }
        let descriptors = fs::read_dir(&self.launches)?
            .map(|e| e.map(|v| v.path()))
            .collect::<std::io::Result<Vec<_>>>()?;
        self.record(json!({"request":request,"response":response,"children_after":children,"launch_descriptors_after":descriptors}))?;
        require(
            children.is_empty() && descriptors.is_empty(),
            "checkout child or launch descriptor leaked",
        )?;
        Ok(response)
    }
    pub fn fixture(&mut self, behavior: Value) -> Result<()> {
        fs::write(self.control.join("behavior.json"), format!("{behavior}\n"))?;
        self.record(json!({"fixture":behavior}))
    }
    fn rows(&self, name: &str) -> Result<Vec<Value>> {
        match fs::read_to_string(self.control.join(name)) {
            Ok(text) => Ok(text
                .lines()
                .map(serde_json::from_str)
                .collect::<std::result::Result<_, _>>()?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(vec![]),
            Err(e) => Err(e.into()),
        }
    }
    pub fn calls(&self) -> Result<Vec<Value>> {
        self.rows("calls.jsonl")
    }
    pub fn helper_calls(&self) -> Result<Vec<Value>> {
        self.rows("helper-calls.jsonl")
    }
    pub fn command(&self, executable: impl AsRef<OsStr>) -> Command {
        let mut c = Command::new(executable);
        c.env_clear().envs(&self.env);
        c
    }
    pub fn probe(
        &mut self,
        name: &str,
        executable: &Path,
        args: &[&str],
        request: &Value,
    ) -> Result<Output> {
        let mut command = self.command(executable);
        command.args(args);
        let input = serde_json::to_vec(request)?;
        let output = capture(
            command,
            Some(&input),
            Some(&self.held),
            Duration::from_secs(4),
        )?;
        let fields = json!({"probe":name,"request":request,"returncode":output.status.code(),"stdout":std::str::from_utf8(&output.stdout)?,"stderr":std::str::from_utf8(&output.stderr)?}).to_string();
        self.record_encoded(format!(
            "{{\"command\":[{}],{}",
            crate::python_fs::json_string(executable.as_os_str())?,
            &fields[1..]
        ))?;
        require(
            output.status.success(),
            format!(
                "checkout probe failed {name}: {}",
                String::from_utf8_lossy(&output.stderr)
            ),
        )?;
        let descriptors = if self.launches.is_dir() {
            fs::read_dir(&self.launches)?
                .map(|e| e.map(|v| v.path()))
                .collect::<std::io::Result<Vec<_>>>()?
        } else {
            vec![]
        };
        self.record(json!({"probe_cleanup":name,"launch_descriptors_after":descriptors}))?;
        require(descriptors.is_empty(), "checkout probe descriptor leaked")?;
        Ok(output)
    }
    pub fn git(&mut self, args: &[&str]) -> Result<String> {
        let mut command = self.command("/usr/bin/git");
        command
            .args(["-c", "maintenance.auto=false"])
            .args(args)
            .current_dir(&self.repo);
        let result = capture(command, None, None, Duration::from_secs(5))?;
        let argv = [
            vec!["/usr/bin/git", "-c", "maintenance.auto=false"],
            args.to_vec(),
        ]
        .concat();
        self.record(json!({"git_command":argv,"stdout":std::str::from_utf8(&result.stdout)?,"stderr":std::str::from_utf8(&result.stderr)?,"returncode":result.status.code()}))?;
        require(result.status.success(), "real Git fixture command failed")?;
        Ok(std::str::from_utf8(&result.stdout)?.trim().into())
    }
    pub fn process_id(&self) -> Result<u32> {
        Ok(self.child.0.as_ref().ok_or("peer child absent")?.id())
    }
    pub fn close(&mut self) -> Result<()> {
        if self.closed {
            return Ok(());
        }
        self.input.take();
        let child = self.child.0.as_mut().ok_or("peer child absent")?;
        let end = Instant::now() + Duration::from_secs(3);
        while !exited(child)? && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(2));
        }
        let status = terminate(child)?;
        self.child.0.take();
        self.closed = true;
        let calls = self.calls()?;
        let helpers = self.helper_calls()?;
        fs::write(
            self.output.join("git-calls.json"),
            serde_json::to_vec_pretty(&calls)?,
        )?;
        fs::write(
            self.output.join("helper-calls.json"),
            serde_json::to_vec_pretty(&helpers)?,
        )?;
        let mut cleanup = Vec::new();
        for call in calls.iter().chain(&helpers) {
            if let Some(raw) = call["pid"]
                .as_i64()
                .and_then(|p| i32::try_from(p).ok())
                .and_then(Pid::from_raw)
            {
                let _ = rustix::process::waitpid(Some(raw), rustix::process::WaitOptions::NOHANG);
                cleanup.push(json!({"pid":raw.as_raw_pid(),"exists_after":Path::new(&format!("/proc/{}",raw.as_raw_pid())).exists()}));
            }
        }
        let remaining =
            fs::read_to_string(format!("/proc/self/task/{}/children", std::process::id()))?;
        fs::write(
            self.output.join("cleanup.json"),
            serde_json::to_vec_pretty(
                &json!({"fixtures":cleanup,"remaining_children":remaining.split_whitespace().collect::<Vec<_>>()}),
            )?,
        )?;
        require(status.success(), "checkout peer did not exit successfully")?;
        require(
            cleanup.iter().all(|r| r["exists_after"] == false) && remaining.trim().is_empty(),
            "checkout descendant leaked",
        )?;
        let _ = &self.temporary;
        Ok(())
    }
}
impl Drop for Peer {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
