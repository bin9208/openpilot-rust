//! Native port of the owned USB-worker production daemon-loop checker.
//! These are protocol fixtures, not physical GPU or vehicle acceptance.
use crate::{
    driving_daemon_reference::event_dict,
    model_family_process::{
        car_params, held_tree, held_usb_assets, json_file, layout, protocol_path, publish_report,
        require, verify_tree, Cancellation, FailedCase, Held, Namespace, Process,
    },
    model_family_reference::Original,
    seal::Result,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::Read,
    os::unix::fs::{FileExt, MetadataExt},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
pub const MODEL_SHA: &str = "e758b96df27858ea97122d18554930d04f9f8bda417417074edfb3a72b008d0b";
pub const RUNTIME_SHA: &str = "a25b81e90d5259c27bc49b3a93ab499c1909a33e133cba974cf023cd94727eeb";
pub const FLAGS: [&str; 6] = [
    "UsbGpuPresent",
    "UsbGpuCompiled",
    "UsbGpuHardwareSeen",
    "UsbGpuLoading",
    "UsbGpuActive",
    "UsbGpuStartupFailed",
];
pub const RETIRED: [(&str, &str); 4] = [
    (
        "rust/tools/check_usbgpu_daemon.py",
        "a1a915df4f86ac072e6e91b43826568f44f59eda019057e98771c17e79744d2d",
    ),
    (
        "rust/tools/check_driver_daemon.py",
        "ea1266dc395dab662319bd3637ad90440989db3d2937756dbf07fbb993520f5b",
    ),
    (
        "rust/tools/model_logging_capture.py",
        "f00fa03c10597fce6c53931b45cf8867baded88a75948c664ffd075f3042535d",
    ),
    (
        "rust/tools/model_logging_reference.py",
        "ced29c79268f0df83bd9f741c8c840c6111521909592bfd7ad7b5602f0182910",
    ),
];
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mode {
    Disconnect,
    Retry,
    Cancel,
}
impl Mode {
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "disconnect" => Ok(Self::Disconnect),
            "retry" => Ok(Self::Retry),
            "cancel" => Ok(Self::Cancel),
            _ => Err("USB mode must be disconnect, retry or cancel".into()),
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::Disconnect => "disconnect",
            Self::Retry => "retry",
            Self::Cancel => "cancel",
        }
    }
}
pub struct Options {
    pub binary: PathBuf,
    pub worker: PathBuf,
    pub peer: PathBuf,
    pub catalog: PathBuf,
    pub model: PathBuf,
    pub runtime: PathBuf,
    pub installed: PathBuf,
    pub assets: PathBuf,
    pub output: PathBuf,
    pub original: PathBuf,
    pub original_sha256: String,
    pub root: PathBuf,
    pub expect_absent: bool,
    pub modes: Vec<Mode>,
}
fn flags(parameters: &Path) -> Result<Value> {
    let mut flags = BTreeMap::new();
    for name in FLAGS {
        let path = parameters.join(name);
        let value = match fs::read(path) {
            Ok(bytes) => bytes == b"1",
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
            Err(e) => return Err(e.into()),
        };
        flags.insert(name, value);
    }
    Ok(json!(flags))
}
fn lifecycle(path: &Path) -> Result<Vec<Value>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let held = Held::open(path, 1024 * 1024, false)?;
    let bytes = held.bytes(1024 * 1024)?;
    std::str::from_utf8(&bytes)?
        .lines()
        .map(|line| Ok(serde_json::from_str(line)?))
        .collect()
}
// These independent inodes belong to this checker. Case hardlinks may change
// their ctime/link count, but never their identity, mode, mtime, length or bytes.
struct Snapshot {
    path: PathBuf,
    file: File,
    identity: (u64, u64, u64, u32, i64, i64),
    sha: String,
}
impl Snapshot {
    fn identity(m: &fs::Metadata) -> (u64, u64, u64, u32, i64, i64) {
        (
            m.dev(),
            m.ino(),
            m.len(),
            m.mode(),
            m.mtime(),
            m.mtime_nsec(),
        )
    }
    fn hash(file: &File, length: u64) -> Result<String> {
        let mut hash = Sha256::new();
        let mut offset = 0;
        let mut buffer = [0; 65536];
        while offset < length {
            let n = file.read_at(&mut buffer[..(length - offset).min(65536) as usize], offset)?;
            require(n != 0, "USB owned snapshot truncated")?;
            hash.update(&buffer[..n]);
            offset += n as u64;
        }
        Ok(format!("{:x}", hash.finalize()))
    }
    fn create(source: &Path, target: &Path, original: &Held) -> Result<Self> {
        original.verify()?;
        let expected = original.pin();
        let length = expected["bytes"].as_u64().ok_or("USB input size missing")?;
        let input = File::from(rustix::fs::open(
            source,
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::NONBLOCK
                | rustix::fs::OFlags::NOFOLLOW
                | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )?);
        require(
            input.metadata()?.is_file(),
            "USB snapshot source must be regular",
        )?;
        let mut output = OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .open(target)?;
        let bytes = std::io::copy(&mut input.take(length + 1), &mut output)?;
        output.sync_all()?;
        require(bytes == length, "USB snapshot copy size changed")?;
        let identity = Self::identity(&output.metadata()?);
        let sha = Self::hash(&output, length)?;
        require(
            expected["sha256"].as_str() == Some(&sha),
            "USB snapshot differs from held original",
        )?;
        original.verify()?;
        let snapshot = Self {
            path: target.into(),
            file: output,
            identity,
            sha,
        };
        snapshot.verify()?;
        Ok(snapshot)
    }
    fn verify(&self) -> Result<()> {
        for parent in self.path.ancestors().skip(1) {
            require(
                fs::symlink_metadata(parent)?.is_dir(),
                "USB owned snapshot parent became an alias",
            )?;
        }
        let path = fs::symlink_metadata(&self.path)?;
        require(
            path.is_file()
                && !path.file_type().is_symlink()
                && Self::identity(&path) == self.identity
                && Self::identity(&self.file.metadata()?) == self.identity
                && Self::hash(&self.file, self.identity.2)? == self.sha
                && Self::identity(&self.file.metadata()?) == self.identity
                && Self::identity(&fs::symlink_metadata(&self.path)?) == self.identity,
            "USB owned snapshot identity or contents changed",
        )
    }
    fn report(&self) -> Value {
        json!({"path":self.path,"identity":self.identity,"sha256":self.sha,
        "ctime_and_link_count":"checker-owned case hardlinks may change these fields"})
    }
}
struct OwnedInputs {
    model: Snapshot,
    runtime: Snapshot,
}
impl OwnedInputs {
    fn link_case(&self, cache: &Path, target: &Path) -> Result<()> {
        fs::hard_link(
            &self.model.path,
            cache.join(format!("model-{}.pkl", &MODEL_SHA[..16])),
        )?;
        fs::hard_link(&self.model.path, target.join("model.pkl"))?;
        fs::hard_link(&self.runtime.path, target.join("runtime.tar.gz"))?;
        Ok(())
    }
    fn verify(&self) -> Result<()> {
        self.model.verify()?;
        self.runtime.verify()
    }
    fn report(&self) -> Value {
        json!({"model":self.model.report(),"runtime":self.runtime.report()})
    }
}
fn original_input_report(paths: [&Path; 2], held: &[Held]) -> Result<Value> {
    let mut rows = Vec::new();
    for (path, input) in paths.into_iter().zip(held) {
        input.verify()?;
        let m = fs::symlink_metadata(path)?;
        rows.push(json!({"path":path,"seal":input.pin(),"dev":m.dev(),"ino":m.ino(),
            "mode":m.mode(),"links":m.nlink(),"mtime":[m.mtime(),m.mtime_nsec()],"ctime":[m.ctime(),m.ctime_nsec()]}));
    }
    Ok(json!(rows))
}
fn setup(root: &Path, owned: &OwnedInputs, marker: &Value) -> Result<()> {
    crate::disk_space::inspect(root, 5120 * 1024 * 1024)?.require()?;
    fs::create_dir(root)?;
    let cache = root.join("cache");
    let target = cache.join("precompiled").join(MODEL_SHA);
    fs::create_dir_all(&target)?;
    fs::create_dir(root.join("models"))?;
    fs::create_dir(root.join("sysfs"))?;
    fs::write(
        cache.join("state.json"),
        serde_json::to_vec(
            &json!({"active":{"model_id":"owned-generic","filename":"model.pkl","size":776634338,"sha256":MODEL_SHA,"url":"https://owned.invalid/model.pkl"},"previous":null}),
        )?,
    )?;
    owned.link_case(&cache, &target)?;
    fs::write(target.join("installed.json"), serde_json::to_vec(marker)?)?;
    let decoder = flate2::read::GzDecoder::new(fs::File::open(&owned.runtime.path)?);
    let mut archive = tar::Archive::new(decoder);
    let mut selected = BTreeMap::new();
    for entry in archive.entries()? {
        let mut entry = entry?;
        let name = entry.path()?.to_string_lossy().into_owned();
        if ["examples/openpilot/compile_warp.py", "tinygrad/__init__.py"].contains(&name.as_str()) {
            require(
                entry.header().entry_type().is_file()
                    && entry.size() <= 1024 * 1024
                    && !selected.contains_key(&name),
                "USB runtime boundary fixture file changed",
            )?;
            let mut data = Vec::new();
            Read::by_ref(&mut entry)
                .take(1024 * 1024 + 1)
                .read_to_end(&mut data)?;
            require(
                data.len() <= 1024 * 1024,
                "USB runtime fixture source exceeds bound",
            )?;
            selected.insert(name, data);
        }
    }
    require(
        selected.len() == 2,
        "USB runtime fixture required entries missing",
    )?;
    for (name, data) in selected {
        let path = target
            .join(format!("runtime-{}", &RUNTIME_SHA[..16]))
            .join(name);
        fs::create_dir_all(path.parent().ok_or("USB runtime member parent missing")?)?;
        fs::write(path, data)?;
    }
    let device = root.join("sysfs/fixture");
    fs::create_dir(device.clone())?;
    for (name, value) in [
        ("idVendor", "add1"),
        ("idProduct", "0001"),
        ("speed", "5000.0"),
    ] {
        fs::write(device.join(name), value)?;
    }
    Ok(())
}
fn packet(path: &Path, topic: &str, frame: u64, mode: Mode) -> Result<Option<Value>> {
    let held = Held::open(path, 16 * 1024 * 1024, false)?;
    let event = event_dict(&held.bytes(16 * 1024 * 1024)?)?;
    let body = event
        .get(topic)
        .ok_or("USB daemon publication has wrong Cereal union")?;
    require(
        body["frameId"] == frame,
        "USB daemon publication frame identity changed",
    )?;
    if topic != "modelV2" {
        return Ok(None);
    }
    let hex = body["rawPredictions"]["bytes_hex"]
        .as_str()
        .ok_or("USB raw prediction bytes missing")?;
    require(
        !hex.is_empty() && hex.len().is_multiple_of(8),
        "USB raw prediction float buffer invalid",
    )?;
    let raw = hex
        .as_bytes()
        .chunks_exact(2)
        .map(|b| Ok(u8::from_str_radix(std::str::from_utf8(b)?, 16)?))
        .collect::<Result<Vec<_>>>()?;
    require(
        raw.chunks_exact(4)
            .all(|b| f32::from_le_bytes(b.try_into().expect("four-byte chunk")).is_finite()),
        "USB raw prediction is nonfinite",
    )?;
    if frame == 2 || mode == Mode::Retry {
        require(
            raw[..4] == ((frame - 1) as f32).to_le_bytes() && raw[4..].iter().all(|&b| b == 0),
            "USB owned protocol prediction bytes changed",
        )?;
    }
    Ok(Some(json!({"frame":frame,"raw_bytes":raw.len()})))
}
fn original_case(original: &Original, width: u64, mode: Mode) -> Result<Value> {
    let runs = original.report["runs"]
        .as_array()
        .ok_or("original USB daemon runs missing")?;
    require(runs.len() == 6, "original USB six-case roster changed")?;
    let selected = runs
        .iter()
        .filter(|r| r["camera"][0] == width && r["mode"] == mode.name())
        .collect::<Vec<_>>();
    require(
        selected.len() == 1
            && selected[0]["camera"] == json!([width, layout(width)?.0])
            && selected[0]["passed"] == true
            && selected[0]["workers_reaped_and_shared_removed"] == true,
        "original USB daemon case missing, duplicated or failed",
    )?;
    Ok(selected[0].clone())
}
pub fn run(options: &Options) -> Result<Value> {
    execute(options, |_| Ok(()))
}
fn execute(options: &Options, mut emit: impl FnMut(&Value) -> Result<()>) -> Result<Value> {
    let cancel = Cancellation::new()?;
    require(
        options.output.is_absolute(),
        "USB evidence output must be absolute",
    )?;
    let original = Original::open(
        &options.root,
        &options.original,
        &options.original_sha256,
        &RETIRED,
        options.expect_absent,
        &[
            "rust/tools/check_usbgpu_selection.py",
            "rust/tools/usbgpu_selection_source.py",
            "openpilot/selfdrive/modeld/modeld.py",
            "openpilot/system/camerad/cameras/nv12_info.py",
        ],
    )?;
    let inputs = [
        (&options.binary, 512 * 1024 * 1024, true),
        (&options.worker, 512 * 1024 * 1024, true),
        (&options.peer, 512 * 1024 * 1024, true),
        (&options.model, 800 * 1024 * 1024, false),
        (&options.runtime, 4 * 1024 * 1024, false),
        (&options.installed, 1024 * 1024, false),
    ]
    .into_iter()
    .map(|(p, n, e)| Held::open(p, n, e))
    .collect::<Result<Vec<_>>>()?;
    require(
        inputs[3].pin() == json!({"bytes":776634338u64,"sha256":MODEL_SHA})
            && inputs[4].pin() == json!({"bytes":2731184,"sha256":RUNTIME_SHA}),
        "USB genuine model/runtime identity changed",
    )?;
    let catalog = held_tree(&options.catalog)?;
    let assets = held_usb_assets(&options.assets, &original.manifest["assets_files"])?;
    original.catalog_sources(&options.catalog, &["driving"])?;
    let mut marker = json_file(&options.installed, 1024 * 1024)?;
    require(
        marker["pickle"]["sha256"] == MODEL_SHA,
        "USB marker model identity changed",
    )?;
    marker["catalog_url"] = json!("https://owned.invalid/precompiled.json");
    marker["pickle"]["url"] = json!("https://owned.invalid/model.pkl");
    marker["runtime"] =
        json!({"sha256":RUNTIME_SHA,"size":2731184,"url":"https://owned.invalid/runtime.tar.gz"});
    marker["runtime_directory"] = json!(format!("runtime-{}", &RUNTIME_SHA[..16]));
    fs::create_dir(&options.output)?;
    let source_before = original_input_report([&options.model, &options.runtime], &inputs[3..5])?;
    fs::write(
        options.output.join("source-inputs-before.json"),
        serde_json::to_vec_pretty(&source_before)?,
    )?;
    let growth = inputs[3].pin()["bytes"]
        .as_u64()
        .ok_or("model size missing")?
        + inputs[4].pin()["bytes"]
            .as_u64()
            .ok_or("runtime size missing")?;
    // Include filesystem block rounding and the owned directory/receipts.
    crate::disk_space::inspect(&options.output, growth + 64 * 1024)?.require()?;
    let copies = options.output.join("owned-usb-inputs");
    fs::create_dir(&copies)?;
    let owned = OwnedInputs {
        model: Snapshot::create(&options.model, &copies.join("model.pkl"), &inputs[3])?,
        runtime: Snapshot::create(&options.runtime, &copies.join("runtime.tar.gz"), &inputs[4])?,
    };
    fs::write(
        options.output.join("owned-inputs-before.json"),
        serde_json::to_vec_pretty(&owned.report())?,
    )?;
    let modes = if options.modes.is_empty() {
        vec![Mode::Disconnect, Mode::Retry, Mode::Cancel]
    } else {
        options.modes.clone()
    };
    let mut reports = Vec::new();
    for width in [1344, 1928] {
        for &mode in &modes {
            cancel.check()?;
            let expected = original_case(&original, width, mode)?;
            let destination = options.output.join(format!("{width}-{}", mode.name()));
            setup(&destination, &owned, &marker)?;
            let mut failure = FailedCase::new(
                destination.join("report.json"),
                json!({"camera":[width,layout(width)?.0],"mode":mode.name(),"passed":false}),
            );
            let namespace = Namespace::new()?;
            let params_root = destination.join("params");
            let params = params_root.join(namespace.prefix());
            fs::create_dir_all(&params)?;
            let lifecycle_path = destination.join("workers.jsonl");
            let mut environment = namespace.environment(&params_root);
            environment.insert("SEND_RAW_PRED".into(), "1".into());
            environment.insert(
                "USBGPU_WORKER_TEST_LOG".into(),
                destination.join("inputs.jsonl").into_os_string(),
            );
            environment.insert(
                "USBGPU_WORKER_TEST_LIFECYCLE_LOG".into(),
                lifecycle_path.clone().into_os_string(),
            );
            match mode {
                Mode::Disconnect => {
                    environment.insert("USBGPU_WORKER_TEST_DISCONNECT".into(), "1".into());
                }
                Mode::Retry => {
                    environment.insert(
                        "USBGPU_WORKER_TEST_RETRY_FILE".into(),
                        destination.join("attempts").into_os_string(),
                    );
                    environment.insert(
                        "USBGPU_WORKER_TEST_RETRY_ERROR".into(),
                        "PCIe power on failed".into(),
                    );
                }
                Mode::Cancel => {
                    environment.insert("USBGPU_WORKER_TEST_STARTUP_PAUSE".into(), "1".into());
                }
            }
            let (height, stride, _, uv, size) = layout(width)?;
            let peer_args = [
                width.to_string(),
                height.to_string(),
                stride.to_string(),
                uv.to_string(),
                size.to_string(),
                "dual".to_owned(),
            ]
            .map(OsString::from);
            let mut peer = Process::spawn(
                &options.peer,
                &peer_args,
                &environment,
                &["SIMULATION"],
                &destination.join("peer.log"),
            )?;
            peer.require_line(&["READY"], Duration::from_secs(60), &cancel)?;
            car_params(&params.join("CarParams"), true)?;
            for (name, value) in [
                ("VEgoStopping", "5"),
                ("CameraYawTrimDeg", "0"),
                ("UseWideCamera", "1"),
                ("SteerActuatorDelay", "0"),
                ("LatSmoothSec", "0"),
                ("LongActuatorDelay", "30"),
            ] {
                fs::write(params.join(name), value)?;
            }
            let argv = [
                options.catalog.as_os_str().to_owned(),
                destination.as_os_str().to_owned(),
                options.assets.as_os_str().to_owned(),
                options.worker.as_os_str().to_owned(),
                "2".into(),
            ];
            let mut daemon = Process::spawn(
                &options.binary,
                &argv,
                &environment,
                &["SIMULATION"],
                &destination.join("daemon.log"),
            )?;
            let mut commands = Vec::new();
            let mut packets = Vec::new();
            let mut initial = None;
            let final_state = if mode == Mode::Cancel {
                let deadline = Instant::now() + Duration::from_secs(30);
                while !lifecycle_path.exists() && Instant::now() < deadline {
                    cancel.check()?;
                    std::thread::sleep(Duration::from_millis(10));
                }
                require(
                    lifecycle_path.exists(),
                    "USB worker did not start before original 30s deadline",
                )?;
                daemon.signal(rustix::process::Signal::TERM)?;
                require(
                    daemon.wait(Duration::from_secs(5), &cancel)?.success(),
                    "USB cancelled daemon did not exit successfully",
                )?;
                let state = flags(&params)?;
                require(
                    state["UsbGpuLoading"] == false
                        && state["UsbGpuActive"] == false
                        && state["UsbGpuStartupFailed"] == false,
                    "USB startup cancellation left runtime flags set",
                )?;
                state
            } else {
                daemon.require_line(&["modeld got CarParams"], Duration::from_secs(60), &cancel)?;
                let state = flags(&params)?;
                require(
                    state["UsbGpuActive"] == true
                        && state["UsbGpuLoading"] == true
                        && state["UsbGpuCompiled"] == true,
                    "USB initial flags differ from original",
                )?;
                initial = Some(state);
                for topic in ["modelV2", "drivingModelData", "cameraOdometry"] {
                    let text = format!(
                        "receive {topic} 50 {}",
                        protocol_path(&destination.join("unexpected.bin"))?
                    );
                    let reply = peer.command(&text, &cancel)?;
                    commands.push(json!({"command":text,"response":reply}));
                    require(
                        reply == "TIMEOUT",
                        "USB daemon published before first frame",
                    )?;
                }
                let image = destination.join("frame.bin");
                fs::write(&image, vec![0; size as usize])?;
                let mut final_state = Value::Null;
                for frame in [2, 3] {
                    let text = format!(
                        "send {frame} {} {}",
                        protocol_path(&image)?,
                        protocol_path(&image)?
                    );
                    let reply = peer.command(&text, &cancel)?;
                    commands.push(json!({"command":text,"response":reply}));
                    require(reply == "OK", "USB frame send failed")?;
                    for topic in ["modelV2", "drivingModelData", "cameraOdometry"] {
                        let path = destination.join(format!("{topic}-{frame}.bin"));
                        let text = format!("receive {topic} 30000 {}", protocol_path(&path)?);
                        let reply = peer.command(&text, &cancel)?;
                        commands.push(json!({"command":text,"response":reply}));
                        require(
                            reply == "OK",
                            "USB publication missed original 30000ms deadline",
                        )?;
                        if let Some(value) = packet(&path, topic, frame, mode)? {
                            packets.push(value);
                        }
                    }
                    let deadline = Instant::now() + Duration::from_secs(2);
                    let mut state = flags(&params)?;
                    while state["UsbGpuLoading"] == true && Instant::now() < deadline {
                        cancel.check()?;
                        std::thread::sleep(Duration::from_millis(1));
                        state = flags(&params)?;
                    }
                    require(
                        state["UsbGpuLoading"] == false
                            && state["UsbGpuActive"] == json!(frame == 2 || mode == Mode::Retry)
                            && state["UsbGpuStartupFailed"]
                                == json!(frame == 3 && mode == Mode::Disconnect),
                        "USB frame/fallback flags differ from original",
                    )?;
                    final_state = state;
                }
                require(
                    daemon.wait(Duration::from_secs(5), &cancel)?.success(),
                    "USB two-frame daemon did not exit successfully",
                )?;
                if mode == Mode::Retry {
                    require(
                        fs::read_to_string(destination.join("attempts"))? == "2",
                        "USB retry attempt count differs from original",
                    )?;
                }
                final_state
            };
            require(
                final_state == expected["final"],
                "USB final flags differ from independent original report",
            )?;
            if let Some(initial) = &initial {
                require(
                    *initial == expected["initial"],
                    "USB initial flags differ from independent original report",
                )?;
            }
            require(
                json!(packets)
                    == expected
                        .get("packets")
                        .cloned()
                        .unwrap_or_else(|| json!([])),
                "USB packet shape differs from independent original report",
            )?;
            let response = peer.command("stop", &cancel)?;
            commands.push(json!({"command":"stop","response":response}));
            require(
                response == "OK" && peer.wait(Duration::from_secs(5), &cancel)?.success(),
                "USB peer stop failed",
            )?;
            let workers = lifecycle(&lifecycle_path)?;
            require(!workers.is_empty(), "USB worker lifecycle log missing")?;
            for worker in &workers {
                let pid = worker["pid"].as_u64().ok_or("USB worker PID missing")?;
                let shared = worker["shared"]
                    .as_str()
                    .ok_or("USB shared-file lifecycle path missing")?;
                require(
                    !Path::new(&format!("/proc/{pid}")).exists() && !Path::new(shared).exists(),
                    "USB worker or shared file remained after daemon exit",
                )?;
            }
            namespace.finish()?;
            let report = json!({"camera":[width,height],"mode":mode.name(),"passed":true,"initial":initial,"final":final_state,"packets":packets,"workers":workers,"workers_reaped_and_shared_removed":true,"commands":commands,"process_receipts":{"daemon":daemon.receipt(),"peer":peer.receipt()},"device_validation":false});
            publish_report(&destination.join("report.json"), &report, || {
                cancel.check()?;
                original.verify()
            })?;
            failure.accepted();
            emit(&json!({"camera":[width,height],"mode":mode.name(),"passed":true}))?;
            reports.push(report);
        }
    }
    owned.verify()?;
    let source_after = original_input_report([&options.model, &options.runtime], &inputs[3..5])?;
    require(
        source_before == source_after,
        "USB original input identity changed during cases",
    )?;
    fs::write(
        options.output.join("source-inputs-after.json"),
        serde_json::to_vec_pretty(&source_after)?,
    )?;
    fs::write(
        options.output.join("owned-inputs-after.json"),
        serde_json::to_vec_pretty(&owned.report())?,
    )?;
    let report = json!({"runs":reports,"device_validation":false,"original_manifest_sha256":options.original_sha256,"source_absence":options.expect_absent,"inputs":inputs.iter().map(Held::pin).collect::<Vec<_>>()});
    publish_report(&options.output.join("report.json"), &report, || {
        cancel.check()?;
        original.verify()?;
        owned.verify()?;
        for input in &inputs {
            input.verify()?;
        }
        verify_tree(&options.catalog, &catalog)?;
        verify_tree(&options.assets, &assets)?;
        original.catalog_sources(&options.catalog, &["driving"])
    })?;
    Ok(report)
}
pub fn cli() -> Result<()> {
    use crate::argparse_scan::{scan_os, OptionSpec};
    use std::io::Write;
    let parsed = scan_os(
        &std::env::args_os().skip(1).collect::<Vec<_>>(),
        &[
            OptionSpec::value("--binary", true),
            OptionSpec::value("--worker", true),
            OptionSpec::value("--peer", true),
            OptionSpec::value("--catalog", true),
            OptionSpec::value("--model", true),
            OptionSpec::value("--runtime", true),
            OptionSpec::value("--installed", true),
            OptionSpec::value("--assets", true),
            OptionSpec::value("--evidence", true),
            OptionSpec::append("--mode"),
            OptionSpec::value("--native-original", true),
            OptionSpec::value("--native-original-sha256", true),
            OptionSpec::value("--native-root", true),
            OptionSpec::value("--source-policy", false),
        ],
    )?;
    if parsed.help {
        std::io::stdout().write_all(b"Usage: check-usbgpu-daemon --binary ELF --worker ELF --peer ELF --catalog DIR --model PKL --runtime TAR --installed JSON --assets DIR --evidence DIR --native-original DIR --native-original-sha256 SHA --native-root DIR [--mode disconnect|retry|cancel] [--source-policy present|retired-model-family]\n")?;
        return Ok(());
    }
    let value = |n: &str| -> Result<&OsString> {
        parsed
            .values
            .get(n)
            .ok_or_else(|| "USB daemon CLI option missing".into())
    };
    let path = |n: &str| -> Result<PathBuf> {
        let p = PathBuf::from(value(n)?);
        Ok(if p.is_absolute() {
            p
        } else {
            std::env::current_dir()?.join(p)
        })
    };
    let policy = parsed
        .values
        .get("--source-policy")
        .map(|s| s.to_str().ok_or("USB source policy is not UTF-8"))
        .transpose()?
        .unwrap_or("present");
    require(
        matches!(policy, "present" | "retired-model-family"),
        "unknown USB source policy",
    )?;
    let modes = parsed
        .append_values
        .get("--mode")
        .map(|values| {
            values
                .iter()
                .map(|s| Mode::parse(s.to_str().ok_or("USB mode is not UTF-8")?))
                .collect::<Result<Vec<_>>>()
        })
        .transpose()?
        .unwrap_or_default();
    let options = Options {
        binary: path("--binary")?,
        worker: path("--worker")?,
        peer: path("--peer")?,
        catalog: path("--catalog")?,
        model: path("--model")?,
        runtime: path("--runtime")?,
        installed: path("--installed")?,
        assets: path("--assets")?,
        output: path("--evidence")?,
        original: path("--native-original")?,
        original_sha256: value("--native-original-sha256")?
            .to_str()
            .ok_or("USB original SHA is not UTF-8")?
            .to_owned(),
        root: path("--native-root")?,
        expect_absent: policy == "retired-model-family",
        modes,
    };
    let mut stdout = std::io::stdout().lock();
    execute(&options, |row| {
        serde_json::to_writer(&mut stdout, row)?;
        stdout.write_all(b"\n")?;
        stdout.flush()?;
        Ok(())
    })?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn case_hardlinks_preserve_original_inputs_and_snapshot_mutations_are_rejected() {
        let root = tempfile::tempdir().unwrap();
        let model = root.path().join("original-model");
        let runtime = root.path().join("original-runtime");
        fs::write(&model, b"synthetic model boundary bytes").unwrap();
        fs::write(&runtime, b"synthetic archive boundary bytes").unwrap();
        let held = [
            Held::open(&model, 1024, false).unwrap(),
            Held::open(&runtime, 1024, false).unwrap(),
        ];
        let before = original_input_report([&model, &runtime], &held).unwrap();
        let owned = OwnedInputs {
            model: Snapshot::create(&model, &root.path().join("owned-model"), &held[0]).unwrap(),
            runtime: Snapshot::create(&runtime, &root.path().join("owned-runtime"), &held[1])
                .unwrap(),
        };
        assert_ne!(
            fs::metadata(&model).unwrap().ino(),
            fs::metadata(&owned.model.path).unwrap().ino()
        );
        for case in ["first", "second"] {
            let cache = root.path().join(case);
            let target = cache.join("precompiled");
            fs::create_dir_all(&target).unwrap();
            owned.link_case(&cache, &target).unwrap();
            owned.verify().unwrap();
            assert_eq!(
                before,
                original_input_report([&model, &runtime], &held).unwrap()
            );
        }
        assert_eq!(fs::metadata(&model).unwrap().nlink(), 1);
        assert_eq!(fs::metadata(&runtime).unwrap().nlink(), 1);
        let alias = root.path().join("second/precompiled/model.pkl");
        let mut changed = fs::read(&alias).unwrap();
        changed[0] ^= 1;
        fs::write(alias, changed).unwrap();
        assert!(owned.verify().is_err());
        let runtime_alias = root.path().join("first/precompiled/runtime.tar.gz");
        let mut changed = fs::read(&runtime_alias).unwrap();
        changed[0] ^= 1;
        fs::write(runtime_alias, changed).unwrap();
        assert!(owned.runtime.verify().is_err());
        assert_eq!(
            before,
            original_input_report([&model, &runtime], &held).unwrap()
        );
    }
    #[test]
    fn raw_predictions_retain_original_finite_and_exact_protocol_checks() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("packet.bin");
        let encode = |raw: &[u8], frame: u32| {
            let mut message = capnp::message::Builder::new_default();
            let mut event = message.init_root::<openpilot_cereal::log_capnp::event::Builder<'_>>();
            let mut model = event.reborrow().init_model_v2();
            model.set_frame_id(frame);
            model.set_raw_predictions(raw);
            fs::write(&path, capnp::serialize::write_message_to_words(&message)).unwrap();
        };
        encode(&1f32.to_le_bytes(), 2);
        assert!(packet(&path, "modelV2", 2, Mode::Retry).is_ok());
        encode(&f32::NAN.to_le_bytes(), 2);
        assert!(packet(&path, "modelV2", 2, Mode::Retry).is_err());
        encode(&2f32.to_le_bytes(), 2);
        assert!(packet(&path, "modelV2", 2, Mode::Retry).is_err());
        encode(&1f32.to_le_bytes(), 2);
        assert!(packet(&path, "cameraOdometry", 2, Mode::Retry).is_err());
        assert!(packet(&path, "modelV2", 3, Mode::Retry).is_err());
        assert!(Mode::parse("normal").is_err());
    }
}
