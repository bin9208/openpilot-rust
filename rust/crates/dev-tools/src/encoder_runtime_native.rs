//! CLI-private peer: each previous listener is joined before changing IPC prefix.
//! This module is intentionally not exposed as a concurrent library API.
use openpilot_dev_tools::{
    encoder_runtime_check::{self, Options, Peers},
    encoder_runtime_exercise::{self, Driver},
    encoder_runtime_process::{self, Cleanup},
    encoder_runtime_wire, encoder_v4l_check,
    seal::Result,
};
use openpilot_msgq::{
    Subscriber, VisionImage, VisionLayout, VisionMetadata, VisionServer, VisionStream,
};
use rustix::thread::{sched_getaffinity, sched_setaffinity, CpuSet};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs::{self, File},
    os::unix::{fs::MetadataExt, process::ExitStatusExt},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
fn pretty(path: &Path, value: &Value) -> Result<()> {
    fs::write(path, serde_json::to_string_pretty(value)? + "\n")?;
    Ok(())
}
fn unlink(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}
struct Peer {
    directory: PathBuf,
    prefix: String,
    shm: PathBuf,
    params: PathBuf,
    streams: Vec<(VisionStream, u32)>,
    services: Vec<&'static str>,
    sockets: Vec<Subscriber>,
    rows: BTreeMap<String, Vec<Value>>,
    raw: Option<File>,
    output: Option<File>,
    inputs: Vec<Value>,
    server: Option<VisionServer>,
    images: Vec<Vec<VisionImage>>,
    next: usize,
    process: Option<Child>,
    owns_shm: bool,
}
impl Peer {
    fn new(
        binary: &Path,
        directory: &Path,
        case: &str,
        native: &Path,
        cpus: &[usize],
    ) -> Result<Self> {
        fs::create_dir(directory)?;
        let prefix = format!(
            "encoder-runtime-{}-{}",
            std::process::id(),
            directory
                .file_name()
                .ok_or("missing peer name")?
                .to_string_lossy()
        );
        let shm = Path::new("/dev/shm").join(format!("msgq_{prefix}"));
        let params = directory.join("params").join(&prefix);
        let services = encoder_runtime_exercise::services(case);
        let streams = if case == "main-multi" {
            vec![
                (VisionStream::Road, 0),
                (VisionStream::Driver, 1),
                (VisionStream::WideRoad, 2),
            ]
        } else if case == "--youtube-wide" {
            vec![(VisionStream::WideRoad, 2)]
        } else {
            vec![(VisionStream::Road, 0)]
        };
        let mut peer = Self {
            directory: directory.into(),
            prefix,
            shm,
            params,
            streams,
            rows: services.iter().map(|s| ((*s).into(), vec![])).collect(),
            services,
            sockets: vec![],
            raw: None,
            output: None,
            inputs: vec![],
            server: None,
            images: vec![],
            next: 0,
            process: None,
            owns_shm: false,
        };
        fs::create_dir(&peer.shm)?;
        peer.owns_shm = true;
        fs::create_dir_all(&peer.params)?;
        for (key, value) in [
            ("RecordRoadCam", "2"),
            ("RecordFront", "1"),
            ("RecordAudio", "0"),
            ("CarrotVisionActive", "0"),
        ] {
            fs::write(peer.params.join(key), value)?;
        }
        // The CLI creates peers sequentially; drop joins the only owned listener.
        // Subscriber queues have no background threads. No prior peer survives here.
        std::env::set_var("OPENPILOT_PREFIX", &peer.prefix);
        for service in &peer.services {
            let capacity = match *service {
                "thumbnail" => 250 * 1024,
                "livestreamRoadEncodeData" => 2 * 1024 * 1024,
                _ => 10 * 1024 * 1024,
            };
            peer.sockets
                .push(Subscriber::for_runtime(service, false, capacity)?);
        }
        peer.raw = Some(File::create(directory.join("messages.bin"))?);
        peer.start_server()?;
        let mut command = Command::new(binary);
        let mut argv = vec![openpilot_dev_tools::python_fs::json_string(
            binary.as_os_str(),
        )?];
        if !case.starts_with("main") {
            command.arg(case);
            argv.push(serde_json::to_string(case)?);
        }
        let mut library = native.join("usr/lib/x86_64-linux-gnu").into_os_string();
        if let Some(previous) = std::env::var_os("LD_LIBRARY_PATH").filter(|v| !v.is_empty()) {
            library.push(":");
            library.push(previous);
        }
        command
            .env("PARAMS_ROOT", directory.join("params"))
            .env("LOGGERD_TEST", "1")
            .env("LOGGERD_SEGMENT_LENGTH", "4")
            .env("LOGPRINT", "debug")
            .env("DEBUG_ENCODER", if case == "main" { "1" } else { "0" })
            .env("LD_LIBRARY_PATH", library);
        let invocation = format!("{{\"argv\":[{}],\"binary_sha256\":{},\"fixture_cpus\":{},\"prefix\":{},\"streams\":{}}}",
            argv.join(","), serde_json::to_string(&openpilot_dev_tools::seal::digest(binary)?)?, serde_json::to_string(cpus)?,
            serde_json::to_string(&peer.prefix)?,serde_json::to_string(&peer.stream_ids())?);
        pretty(
            &directory.join("invocation.json"),
            &serde_json::from_str(&invocation)?,
        )?;
        peer.output = Some(File::create(directory.join("daemon.log"))?);
        command
            .stdout(Stdio::from(peer.output.as_ref().unwrap().try_clone()?))
            .stderr(Stdio::from(peer.output.as_ref().unwrap().try_clone()?));
        // Inherit affinity without an unsafe pre_exec callback. Restore only the
        // calling thread; the already-running listener's original affinity stays.
        let previous = sched_getaffinity(None)?;
        let mut selected = CpuSet::new();
        for cpu in cpus {
            selected.set(*cpu);
        }
        sched_setaffinity(None, &selected)?;
        let spawned = command.spawn();
        let restored = sched_setaffinity(None, &previous);
        peer.process = Some(spawned?);
        restored?;
        if let Err(error) = peer.await_initialized() {
            encoder_runtime_process::close(&mut peer)?;
            return Err(error);
        }
        Ok(peer)
    }
    fn stream_ids(&self) -> Vec<u32> {
        self.streams.iter().map(|(_, id)| *id).collect()
    }
    fn start_server(&mut self) -> Result<()> {
        let server = VisionServer::new("camerad")?;
        self.server = Some(server);
        for (stream, _) in &self.streams {
            self.images
                .push(self.server.as_ref().unwrap().create_stream(
                    *stream,
                    64,
                    VisionLayout {
                        width: 128,
                        height: 80,
                        stride: 160,
                        uv_offset: 12800,
                        len: 19200,
                    },
                )?);
        }
        self.server.as_ref().unwrap().start_listener()?;
        self.next = 0;
        Ok(())
    }
    fn check_alive(&mut self) -> Result<()> {
        if let Some(status) = self
            .process
            .as_mut()
            .ok_or("encoder process absent")?
            .try_wait()?
        {
            return Err(format!(
                "encoder exited {}: {}",
                status
                    .code()
                    .unwrap_or_else(|| -status.signal().unwrap_or(1)),
                self.directory.display()
            )
            .into());
        }
        Ok(())
    }
    fn await_initialized(&mut self) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(10);
        let expected = self.streams.len() * 64;
        loop {
            self.check_alive()?;
            let pid = self.process.as_ref().unwrap().id();
            let mappings = fs::read_to_string(format!("/proc/{pid}/maps"))?;
            let imported = mappings.matches("/dev/shm/msgq_visionbuf_").count();
            if imported >= expected
                && self
                    .services
                    .iter()
                    .all(|s| mappings.contains(self.shm.join(s).to_string_lossy().as_ref()))
            {
                fs::write(self.directory.join("maps.txt"), &mappings)?;
                let executable = PathBuf::from(format!("/proc/{pid}/exe"));
                let stat = fs::metadata(&executable)?;
                pretty(
                    &self.directory.join("executed-elf.json"),
                    &json!({"pid":pid,"sha256":openpilot_dev_tools::seal::digest(&executable)?,"device":stat.dev(),"inode":stat.ino(),"bytes":stat.len()}),
                )?;
                if mappings.contains("libpython") {
                    return Err("native encoder mapped a Python runtime".into());
                }
                fs::write(
                    self.directory.join("startup.json"),
                    format!("{{\"imported_buffers\": {imported}, \"expected\": {expected}}}\n"),
                )?;
                break;
            }
            if Instant::now() >= deadline {
                return Err(format!("encoder startup: {}", self.directory.display()).into());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        std::thread::sleep(Duration::from_millis(250));
        Driver::drain(self)
    }
    fn signal(&mut self, signal: rustix::process::Signal) -> Result<()> {
        self.check_alive()?;
        let pid =
            rustix::process::Pid::from_raw(i32::try_from(self.process.as_ref().unwrap().id())?)
                .ok_or("invalid owned pid")?;
        rustix::process::kill_process(pid, signal)?;
        Ok(())
    }
    fn release(&mut self) -> Result<()> {
        self.images.clear();
        self.server.take(); // Last Rc drops State and joins Listener.
        self.sockets.clear();
        if !self.owns_shm {
            return Ok(());
        }
        fs::remove_dir_all(&self.shm)?;
        unlink(&PathBuf::from(format!(
            "/tmp/{}_visionipc_camerad",
            self.prefix
        )))?;
        unlink(&PathBuf::from(format!("/tmp/logmessage{}", self.prefix)))?;
        self.owns_shm = false;
        Ok(())
    }
}
impl Driver for Peer {
    fn send(&mut self, frame: u32, pause: f64) -> Result<()> {
        for (index, (_, stream)) in self.streams.iter().enumerate() {
            let pixels = (0..19200)
                .map(|i| {
                    ((i * 17 + i / 160 * 13 + frame as usize * 7 + *stream as usize * 11) % 256)
                        as u8
                })
                .collect::<Vec<_>>();
            let image = &self.images[index][self.next % 64];
            image.write(0, &pixels)?;
            image.publish(VisionMetadata {
                width: 128,
                height: 80,
                stride: 160,
                uv_offset: 12800,
                len: 19200,
                frame_id: frame,
                timestamp_sof: 1_000_000_000 + u64::from(frame) * 50_000_000,
                timestamp_eof: 1_020_000_000 + u64::from(frame) * 50_000_000,
                valid: false,
                received: true,
                index: self.next % 64,
                fd: -1,
            })?;
        }
        self.next += 1;
        self.inputs.push(json!({"frame":frame,"active":fs::read_to_string(self.params.join("CarrotVisionActive"))?,"streams":self.stream_ids()}));
        std::thread::sleep(Duration::from_secs_f64(pause));
        self.check_alive()?;
        Driver::drain(self)
    }
    fn await_frame(&mut self, frame: u32) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(15);
        while !self
            .services
            .iter()
            .filter(|s| **s != "thumbnail")
            .all(|s| {
                self.rows[*s]
                    .last()
                    .is_some_and(|r| r["idx"]["frameId"].as_u64() == Some(u64::from(frame)))
            })
        {
            self.check_alive()?;
            Driver::drain(self)?;
            if Instant::now() >= deadline {
                return Err(format!("last frame {frame}: {}", self.directory.display()).into());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        Ok(())
    }
    fn set_active(&mut self, active: bool) -> Result<()> {
        fs::write(
            self.params.join("CarrotVisionActive"),
            if active { "1" } else { "0" },
        )?;
        Ok(())
    }
    fn stop(&mut self) -> Result<()> {
        self.signal(rustix::process::Signal::STOP)?;
        let deadline = Instant::now() + Duration::from_secs(2);
        let pid = self.process.as_ref().unwrap().id();
        while !fs::read_to_string(format!("/proc/{pid}/status"))?.contains("\nState:\tT") {
            if Instant::now() >= deadline {
                return Err("owned encoder did not stop".into());
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        Ok(())
    }
    fn resume(&mut self) -> Result<()> {
        self.signal(rustix::process::Signal::CONT)
    }
    fn restart(&mut self) -> Result<()> {
        self.images.clear();
        self.server.take();
        self.start_server()
    }
    fn drain(&mut self) -> Result<()> {
        for (index, service) in self.services.iter().enumerate() {
            while let Some(wire) = self.sockets[index].receive(Duration::ZERO)? {
                encoder_runtime_wire::drain(
                    service,
                    &[wire],
                    self.raw.as_mut().ok_or("raw capture closed")?,
                    self.rows.get_mut(*service).unwrap(),
                )?;
            }
        }
        Ok(())
    }
    fn rows(&self) -> &BTreeMap<String, Vec<Value>> {
        &self.rows
    }
}
impl Cleanup for Peer {
    fn shutdown(&mut self) -> Result<()> {
        encoder_runtime_process::stop_cleanly(
            self.process.as_mut().ok_or("missing process")?,
            &self.directory.join("shutdown.json"),
            Duration::from_secs(5),
        )
    }
    fn drain(&mut self) -> Result<()> {
        Driver::drain(self)
    }
    fn close_files(&mut self) -> Result<()> {
        self.raw.take();
        self.output.take();
        Ok(())
    }
    fn write_packets(&mut self) -> Result<()> {
        pretty(
            &self.directory.join("packets.json"),
            &serde_json::to_value(&self.rows)?,
        )
    }
    fn write_inputs(&mut self) -> Result<()> {
        pretty(
            &self.directory.join("inputs.json"),
            &serde_json::to_value(&self.inputs)?,
        )
    }
    fn release_ipc(&mut self) -> Result<()> {
        self.release()
    }
    fn outcome(&mut self) -> Result<()> {
        let status = self.process.as_mut().ok_or("missing process")?.wait()?;
        encoder_v4l_check::check_outcome(
            status
                .code()
                .unwrap_or_else(|| -status.signal().unwrap_or(1)),
            &fs::read_to_string(self.directory.join("daemon.log"))?,
            None,
        )
    }
}
impl Drop for Peer {
    fn drop(&mut self) {
        if let Some(child) = &mut self.process {
            if child.try_wait().ok().flatten().is_none() {
                let _ = child.kill();
            }
            let _ = child.wait();
        }
        // Includes constructor errors, before normal close is available.
        let _ = self.release();
    }
}
#[derive(Default)]
struct NativePeers {
    active: Option<Peer>,
    rows: BTreeMap<String, Value>,
}
impl Peers for NativePeers {
    fn create(
        &mut self,
        binary: &Path,
        directory: &Path,
        case: &str,
        native: &Path,
        cpus: &[usize],
    ) -> Result<()> {
        if self.active.is_some() {
            return Err("previous encoder peer is still active".into());
        }
        self.active = Some(Peer::new(binary, directory, case, native, cpus)?);
        Ok(())
    }
    fn exercise(&mut self, _kind: &str, case: &str) -> Result<()> {
        encoder_runtime_exercise::exercise(case, self.active.as_mut().ok_or("missing peer")?)
    }
    fn close(&mut self, kind: &str, _case: &str) -> Result<()> {
        let mut peer = self.active.take().ok_or("missing peer")?;
        let closed = encoder_runtime_process::close(&mut peer);
        self.rows
            .insert(kind.into(), serde_json::to_value(&peer.rows)?);
        closed
    }
    fn rows(&self, kind: &str) -> Result<Value> {
        self.rows
            .get(kind)
            .cloned()
            .ok_or_else(|| "missing peer rows".into())
    }
}
pub fn cli() -> Result<()> {
    use openpilot_dev_tools::argparse_scan::{scan_os, ArgumentError, OptionSpec};
    let parsed = scan_os(
        &std::env::args_os().skip(1).collect::<Vec<OsString>>(),
        &[
            OptionSpec::value("--source", true),
            OptionSpec::value("--rust", true),
            OptionSpec::value("--output", true),
            OptionSpec::value("--native", true),
            OptionSpec::append("--case"),
        ],
    )?;
    if parsed.help {
        println!("usage: check-encoder-runtime --source BINARY --rust BINARY --output DIRECTORY --native DIRECTORY [--case CASE]\nRun source and Rust encoderd over owned VisionIPC and native message queues");
        return Ok(());
    }
    let cases = parsed
        .append_values
        .get("--case")
        .into_iter()
        .flatten()
        .map(|s| {
            s.to_str()
                .filter(|s| encoder_runtime_check::CASES.contains(s))
                .map(str::to_owned)
                .ok_or_else(|| ArgumentError(format!("invalid case: {}", s.to_string_lossy())))
        })
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let options = Options {
        source: PathBuf::from(&parsed.values["--source"]),
        binary: PathBuf::from(&parsed.values["--rust"]),
        output: PathBuf::from(&parsed.values["--output"]),
        native: PathBuf::from(&parsed.values["--native"]),
        cases,
    };
    let allowed = sched_getaffinity(None)?;
    let cpus = (0..CpuSet::MAX_CPU)
        .filter(|cpu| allowed.is_set(*cpu))
        .take(2)
        .collect::<Vec<_>>();
    let free = openpilot_dev_tools::disk_space::inspect(&options.output, 0)?.free_bytes;
    encoder_runtime_check::execute(
        &options,
        &cpus,
        free,
        &mut NativePeers::default(),
        &mut std::io::stdout(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_peer_frame_wire_including_false_validity() {
        if std::env::var_os("ENCODER_PEER_WIRE_CHILD").is_none() {
            let status = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "native::tests::original_peer_frame_wire_including_false_validity",
                    "--nocapture",
                ])
                .env("ENCODER_PEER_WIRE_CHILD", "1")
                .status()
                .unwrap();
            assert!(status.success());
            return;
        }
        let temp = tempfile::tempdir().unwrap();
        let namespace = tempfile::Builder::new()
            .prefix("msgq_encoder-wire-")
            .tempdir_in("/dev/shm")
            .unwrap();
        let prefix = namespace
            .path()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .strip_prefix("msgq_")
            .unwrap()
            .to_owned();
        std::env::set_var("OPENPILOT_PREFIX", &prefix);
        let params = temp.path().join("params");
        fs::create_dir(&params).unwrap();
        fs::write(params.join("CarrotVisionActive"), "0").unwrap();
        let mut peer = Peer {
            directory: temp.path().into(),
            prefix,
            shm: namespace.path().into(),
            params,
            streams: vec![(VisionStream::Road, 0)],
            services: vec![],
            sockets: vec![],
            rows: BTreeMap::new(),
            raw: None,
            output: None,
            inputs: vec![],
            server: None,
            images: vec![],
            next: 0,
            process: Some(Command::new("/bin/sleep").arg("10").spawn().unwrap()),
            owns_shm: false,
        };
        peer.start_server().unwrap();
        let mut client =
            openpilot_msgq::VisionClient::new("camerad", VisionStream::Road, false).unwrap();
        assert!(client.connect().unwrap());
        peer.send(100, 0.).unwrap();
        let frame = client.receive(Duration::from_secs(1)).unwrap().unwrap();
        assert_eq!(frame.metadata().frame_id, 100);
        assert_eq!(frame.metadata().timestamp_sof, 6_000_000_000);
        assert_eq!(frame.metadata().timestamp_eof, 6_020_000_000);
        assert_eq!(frame.metadata().index, 0);
        let mut pixels = vec![0; 19200];
        frame.copy_into(&mut pixels).unwrap();
        assert_eq!(
            pixels,
            (0..19200)
                .map(|i| ((i * 17 + i / 160 * 13 + 700) % 256) as u8)
                .collect::<Vec<_>>()
        );
        // Literal original visionipc_pyx.pyx send sets False unconditionally.
        assert!(
            !frame.metadata().valid,
            "original Python peer sends valid=False"
        );
    }
}
