//! Native controllers for check_loggerd_flush.py and check_loggerd_io.py.
//! Dedicated parent owns workers, detached strace children and diagnostic IPC.
use super::{
    fixtures::{self, Fixtures},
    owned::Owner,
    peer::{Evidence, Peer, Settings},
    process::Process,
    providers::Providers,
    request, require, save,
    scenarios::ENCODERS,
    validation, Request, Result, IDLE, WORKER,
};
use crate::argparse_scan::{scan_os_required_first, ArgumentError, OptionSpec};
use openpilot_cereal::log_capnp::{encode_index, event};
use openpilot_msgq::Subscriber;
use serde_json::{json, Value};
use std::{
    fs,
    os::unix::fs::{symlink, DirBuilderExt},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
#[path = "io_trace.rs"]
pub mod trace;

/// Same full-schema raw fixture as the original controller. Only the ambient
/// new_message logMonoTime is sampled freshly; every payload/index field stays.
pub fn raw_packet(frame: Option<u32>) -> Result<Vec<u8>> {
    let mut message = capnp::message::Builder::new_default();
    let now = rustix::time::clock_gettime(rustix::time::ClockId::Boottime);
    {
        let mut root = message.init_root::<event::Builder<'_>>();
        root.set_log_mono_time(now.tv_sec as u64 * 1_000_000_000 + now.tv_nsec as u64);
        root.set_valid(false);
        let mut data = root.init_road_encode_data();
        data.set_width(160);
        data.set_height(120);
        data.set_header(if frame.is_some() {
            b"header"
        } else {
            b"fixture-header"
        });
        let payload = if frame.is_some() {
            vec![b'X'; 8192]
        } else {
            b"fixture-payload".to_vec()
        };
        data.set_data(&payload);
        let mut index = data.get_idx()?;
        index.set_type(encode_index::Type::FullHEVC);
        index.set_flags(8);
        if let Some(frame) = frame {
            index.set_frame_id(frame);
            index.set_timestamp_eof((u64::from(frame) + 1) * 50_000_000);
        } else {
            index.set_timestamp_eof(1_000_000);
        }
    }
    Ok(capnp::serialize::write_message_to_words(&message))
}

/// Source encoder can buffer H264 packets (the historical160 input frames
/// yielded110 packets). Feed unchanged actual producer commands until160 real
/// packets exist, with a320-command bound. Never pad/repeat/synthesize a fixture.
fn capture_qroad(binary: &Path, output: &Path) -> Result<Value> {
    fs::create_dir_all(output)?;
    let service = "qRoadEncodeData";
    let mut subscriber = Subscriber::for_runtime(service, false, super::peer::capacity(service)?)?;
    let prefix = std::env::var("OPENPILOT_PREFIX")?;
    fs::create_dir_all(output.join("params").join(prefix))?;
    let mut command = std::process::Command::new(binary);
    command
        .arg("qroad")
        .env("PARAMS_ROOT", output.join("params"));
    let mut producer = Process::start(&mut command, output, Duration::from_secs(30))?;
    require(
        producer.line()? == b"ready",
        "source qroad producer readiness differs",
    )?;
    while subscriber.receive(Duration::ZERO)?.is_some() {}
    let mut packets = Vec::new();
    let mut commands = 0;
    while packets.len() < 160 && commands < 320 {
        commands += 1;
        producer.write(format!("frame {commands}\n").as_bytes())?;
        require(
            producer.line()? == b"ok",
            "source qroad frame acknowledgement differs",
        )?;
        while let Some(packet) = subscriber.receive(Duration::ZERO)? {
            packets.push(packet);
        }
    }
    producer.write(b"quit\n")?;
    require(
        producer.wait(Duration::from_secs(20))? == 0,
        "source qroad producer failed",
    )?;
    while let Some(packet) = subscriber.receive(Duration::ZERO)? {
        packets.push(packet);
    }
    require(
        packets.len() == 160,
        "exact640 IO gate needs160 actual qroad packets; preserve producer evidence",
    )?;
    let mut index = Vec::new();
    for (number, packet) in packets.iter().enumerate() {
        let path = format!("{number:04}.capnp");
        fs::write(output.join(&path), packet)?;
        index.push(json!({"path":path,"sha256":super::hash(packet)}));
    }
    let result = json!({"producer_sha256":crate::seal::digest(binary)?,"input_frames":commands,"packets":index,"required_packets":160,"padding":false});
    save(&output.join("index.json"), &result)?;
    Ok(result)
}

fn indices(logs: &[Value], part: usize, service: &str) -> Result<Vec<Value>> {
    Ok(logs
        .get(part)
        .and_then(|v| v["rlog"].as_array())
        .ok_or("IO segment/rlog absent")?
        .iter()
        .filter_map(|v| v.get(service).cloned())
        .collect())
}
fn frames(rows: &[Value]) -> Result<Vec<u64>> {
    rows.iter()
        .map(|v| {
            v["frameId"]
                .as_u64()
                .ok_or_else(|| "IO index frameId absent".into())
        })
        .collect()
}

/// Fail closed on dropped/duplicated/reordered indices or missing real rotation.
pub fn check_indices(logs: &[Value], mode: &str) -> Result<Value> {
    match mode {
        "raw" => {
            require(logs.len() == 1, "raw failure unexpectedly rotated")?;
            let rows = indices(logs, 0, "roadEncodeIdx")?;
            require(
                frames(&rows)? == [0, 1, 2],
                "raw ENOSPC indices must be exactly0..2",
            )?;
            Ok(json!({"indices":rows}))
        }
        "remux" => {
            require(logs.len() == 2, "ENOSPC remux must rotate exactly once")?;
            let first = indices(logs, 0, "qRoadEncodeIdx")?;
            let next = indices(logs, 1, "qRoadEncodeIdx")?;
            require(
                frames(&first)? == (0..640).collect::<Vec<_>>(),
                "remux ENOSPC indices must be exactly0..639",
            )?;
            require(
                frames(&next)? == [640, 641],
                "rotated remux indices must be640,641",
            )?;
            let first_log = logs[0]["rlog"]
                .as_array()
                .ok_or("first remux rlog absent")?;
            let next_log = logs[1]["rlog"]
                .as_array()
                .ok_or("rotated remux rlog absent")?;
            require(
                first_log
                    .last()
                    .is_some_and(|v| v["sentinel"]["type"] == "endOfSegment")
                    && next_log
                        .iter()
                        .any(|v| v["sentinel"]["type"] == "startOfSegment"),
                "remux rotation sentinels absent",
            )?;
            Ok(json!({"indices":first,"rotation_indices":next,"segments":2}))
        }
        _ => Err("unknown IO index mode".into()),
    }
}

fn missing(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
        Ok(_) => Err(format!("writer lock was not removed: {}", path.display()).into()),
    }
}

pub(super) fn worker(request: &Request) -> Result<Value> {
    if request.name == "capture-qroad" {
        return capture_qroad(&request.binary, &request.output);
    }
    if request.name == "capture-road" {
        let packets = fixtures::capture(&request.binary, &request.output, "road")?;
        return Ok(json!({"packets":packets.len()}));
    }
    let asan = request.name == "remux-asan";
    let mode = if asan { "remux" } else { request.name.as_str() };
    require(
        ["flush", "raw", "remux"].contains(&mode),
        "unknown IO worker case",
    )?;
    let mut settings = Settings::new(if mode == "remux" {
        &ENCODERS
    } else {
        &["roadEncodeData"]
    });
    if mode == "remux" {
        settings
            .parameters
            .insert("RecordRoadCam".into(), "0".into());
        settings.parameters.insert("RecordFront".into(), "0".into());
    }
    let trace = request.output.join("syscalls.log");
    if !asan {
        let tracer = request.runner.first().ok_or("IO strace provider missing")?;
        settings.runner = vec![
            tracer.clone(),
            "-D".into(),
            "-f".into(),
            "-yy".into(),
            "-s".into(),
            "96".into(),
            "-e".into(),
            "trace=write,fsync,fdatasync,close,execve,execveat".into(),
            "-o".into(),
            trace.to_str().ok_or("trace path is not UTF-8")?.into(),
        ];
    } else {
        require(
            request.runner.is_empty(),
            "ASan remux must not be traced; LeakSanitizer requires no ptrace",
        )?;
        require(
            std::env::var("ASAN_OPTIONS").as_deref()
                == Ok("detect_leaks=1:halt_on_error=1:abort_on_error=1"),
            "ASan remux requires unchanged full sanitizer policy",
        )?;
    }
    let mut peer = Peer::start(&request.binary, &request.output, &settings)?;
    match mode {
        "flush" => peer.send("roadEncodeData", &raw_packet(None)?)?,
        "raw" => {
            symlink("/dev/full", peer.segment(0).join("fcamera.hevc"))?;
            for frame in 0..3 {
                peer.send("roadEncodeData", &raw_packet(Some(frame))?)?;
            }
        }
        "remux" => {
            let fixtures = Fixtures::load(&request.fixtures)?;
            require(
                fixtures.qroad.len() == 160,
                "remux fixture must contain160 actual source packets",
            )?;
            symlink("/dev/full", peer.segment(0).join("qcamera.ts"))?;
            for service in &ENCODERS[..3] {
                peer.send(
                    service,
                    &fixtures::encoded(
                        fixtures.packet("roadEncodeData", 0)?,
                        service,
                        0,
                        0,
                        Some(true),
                    )?,
                )?;
            }
            for frame in 0..640 {
                peer.send(
                    "qRoadEncodeData",
                    &fixtures::encoded(
                        &fixtures.qroad[frame as usize % 160],
                        "qRoadEncodeData",
                        0,
                        frame,
                        None,
                    )?,
                )?;
            }
            for service in &ENCODERS[..3] {
                peer.send(
                    service,
                    &fixtures::encoded(
                        fixtures.packet("roadEncodeData", 0)?,
                        service,
                        1,
                        1,
                        Some(true),
                    )?,
                )?;
            }
            for frame in [640, 641] {
                peer.send(
                    "qRoadEncodeData",
                    &fixtures::encoded(
                        &fixtures.qroad[0],
                        "qRoadEncodeData",
                        1,
                        frame,
                        Some(true),
                    )?,
                )?;
            }
        }
        _ => unreachable!(),
    }
    let logs = peer.stop(rustix::process::Signal::TERM)?;
    let report = match mode {
        "flush" => {
            require(
                fs::read(peer.segment(0).join("fcamera.hevc"))? == b"fixture-headerfixture-payload",
                "raw flush file bytes differ",
            )?;
            json!({"log_syncs":[],"route_finalized":true,"raw_bytes_exact":true})
        }
        "raw" => {
            missing(&peer.segment(0).join("fcamera.hevc.lock"))?;
            let indices = check_indices(&logs, mode)?;
            json!({"exit":0,"indices":indices["indices"],"video_lock_removed":true,"enospc_observed":true})
        }
        "remux" => {
            let indices = check_indices(&logs, mode)?;
            missing(&peer.segment(0).join("qcamera.ts.lock"))?;
            missing(&peer.segment(1).join("qcamera.ts.lock"))?;
            json!({"indices":640,"exit":0,"packet_warning":true,"trailer_enospc":true,"avio_close_enospc":true,"full_indices":indices})
        }
        _ => unreachable!(),
    };
    // -D detaches the tracer, not the logger. Wait for actual trace completion
    // before the parent's subreaper cohort cleanup may signal adopted children.
    let until = Instant::now() + Duration::from_secs(2);
    let syscall = if asan {
        json!({"coverage":"untraced-asan-remux","leak_detection":true})
    } else {
        loop {
            let text = fs::read_to_string(&trace)?;
            match trace::audit(&text, &request.output, &peer.route, mode) {
                Ok(audit) => break audit,
                Err(error) if Instant::now() >= until => return Err(error),
                Err(_) => std::thread::sleep(Duration::from_millis(10)),
            }
        }
    };
    Ok(json!({"report":report,"syscalls":syscall}))
}

/// Required inherited remux diagnostics; full ordering/context/callsites are
/// checked separately through the existing shared diagnostic comparator.
pub fn check_remux_diagnostics(rows: &[Value]) -> Result<()> {
    require(
        rows.iter().any(|v| {
            v["msg"]
                .as_str()
                .is_some_and(|s| s.contains("ts encoder write issue len:"))
        }),
        "remux packet warning absent",
    )?;
    for message in ["av_write_trailer failed -28", "avio_closep failed -28"] {
        require(
            rows.iter()
                .any(|v| v["levelnum"] == 40 && v["msg"] == message),
            format!("required ENOSPC diagnostic absent: {message}"),
        )?;
    }
    Ok(())
}

/// Dedicated CLI, sharing the same worker protocol and ownership supervisor.
pub fn cli() -> Result<()> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args.len() == 2 && (args[0] == WORKER || args[0] == IDLE) {
        return super::cli();
    }
    let parsed = scan_os_required_first(
        &args,
        &[
            OptionSpec::value("--binary", true),
            OptionSpec::value("--original", true),
            OptionSpec::value("--output", true),
            OptionSpec::value("--fixtures", false),
            OptionSpec::value("--producer", false),
            OptionSpec::value("--root", false),
            OptionSpec::value("--mode", false),
            OptionSpec::flag("--remux-only", None),
            OptionSpec::flag("--asan-remux", None),
        ],
    )?;
    if parsed.help {
        println!("usage: check-loggerd-io --binary BINARY --original ORIGINAL --output OUTPUT [--fixtures FIXTURES | --producer PRODUCER] [--mode all|flush|raw|remux] [--remux-only] [--asan-remux] [--root ROOT]");
        return Ok(());
    }
    let mode = if let Some(value) = parsed.values.get("--mode") {
        value
            .to_str()
            .ok_or_else(|| Box::new(ArgumentError("IO mode must be UTF-8".into())))?
    } else {
        "all"
    };
    if !["all", "flush", "raw", "remux"].contains(&mode) {
        return Err(Box::new(ArgumentError("invalid IO mode".into())));
    }
    let mode = if parsed.values.contains_key("--remux-only") {
        if parsed.values.contains_key("--mode") {
            return Err(Box::new(ArgumentError(
                "--remux-only conflicts with --mode".into(),
            )));
        }
        "remux"
    } else {
        mode
    };
    let remux = mode == "all" || mode == "remux";
    let asan = parsed.values.contains_key("--asan-remux");
    if asan
        && (mode != "remux"
            || std::env::var("ASAN_OPTIONS").as_deref()
                != Ok("detect_leaks=1:halt_on_error=1:abort_on_error=1"))
    {
        return Err(Box::new(ArgumentError(
            "--asan-remux requires remux-only coverage and unchanged full ASAN_OPTIONS".into(),
        )));
    }
    if remux
        && !parsed.values.contains_key("--fixtures")
        && !parsed.values.contains_key("--producer")
    {
        return Err(Box::new(ArgumentError(
            "remux requires --fixtures or --producer".into(),
        )));
    }
    let native = PathBuf::from(&parsed.values["--binary"]).canonicalize()?;
    let source = PathBuf::from(&parsed.values["--original"]).canonicalize()?;
    let tracer = PathBuf::from("/usr/bin/strace").canonicalize()?;
    let output = std::path::absolute(Path::new(&parsed.values["--output"]))?;
    let root = parsed
        .values
        .get("--root")
        .map_or_else(
            || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.."),
            PathBuf::from,
        )
        .canonicalize()?;
    let producer = parsed
        .values
        .get("--producer")
        .map(|p| PathBuf::from(p).canonicalize())
        .transpose()?;
    let mut paths = vec![source.as_path(), native.as_path(), tracer.as_path()];
    if let Some(producer) = &producer {
        paths.push(producer.as_path());
    }
    fs::create_dir_all(output.parent().ok_or("IO evidence parent absent")?)?;
    fs::DirBuilder::new().mode(0o700).create(&output)?;
    let output = output.canonicalize()?;
    let mut providers = Providers::new(&paths, &output)?;
    let mut owner = Owner::new_for(&output.join("control"), "check-loggerd-io")?;
    let fixtures = if remux {
        if let Some(path) = parsed.values.get("--fixtures") {
            PathBuf::from(path).canonicalize()?
        } else {
            let fixtures = output.join("fixtures");
            fs::create_dir(&fixtures)?;
            for kind in ["road", "qroad"] {
                owner.run(
                    &[request(
                        "loggerd-io",
                        &format!("capture-{kind}"),
                        producer.as_ref().ok_or("producer absent")?,
                        &fixtures.join(kind),
                        Path::new(""),
                        &[],
                    )],
                    Duration::from_secs(300),
                )?;
            }
            fixtures
        }
    } else {
        PathBuf::new()
    };
    if remux {
        require(
            Fixtures::load(&fixtures)?.qroad.len() == 160,
            "640-index gate requires exactly160 qroad source packets",
        )?;
    }
    let runner = vec![tracer
        .to_str()
        .ok_or("strace provider path is not UTF-8")?
        .into()];
    let mut results = json!({});
    for current in ["flush", "raw", "remux"] {
        if mode != "all" && mode != current {
            continue;
        }
        let target = output.join(current);
        fs::create_dir(&target)?;
        let mut rows = Vec::new();
        for (implementation, binary) in [("original", &source), ("rust", &native)] {
            let directory = target.join(implementation);
            let result = owner
                .run(
                    &[request(
                        "loggerd-io",
                        if asan { "remux-asan" } else { current },
                        binary,
                        &directory,
                        &fixtures,
                        if asan { &[] } else { &runner },
                    )],
                    Duration::from_secs(300),
                )?
                .remove(0);
            if current == "remux" {
                check_remux_diagnostics(&serde_json::from_slice::<Vec<Value>>(&fs::read(
                    directory.join("diagnostics.json"),
                )?)?)?;
            }
            rows.push(result);
        }
        save(
            &target.join("pair.json"),
            &json!({"original":rows[0],"rust":rows[1]}),
        )?;
        require(
            rows[0] == rows[1],
            format!(
                "{current}: actual source/native indices, flush, writes or close behavior differs"
            ),
        )?;
        let diagnostics = validation::diagnostics(
            &root,
            &Evidence::load(&target.join("original"), true)?,
            &Evidence::load(&target.join("rust"), true)?,
        )?;
        results[current] = json!({"original":rows[0],"rust":rows[1],"diagnostics":diagnostics});
        save(&output.join("results.json"), &results)?;
    }
    owner.finish()?;
    providers.verify()?;
    results["result"] = json!("pass");
    save(&output.join("results.json"), &results)
}
