use super::*;
fn invalid(
    binary: &Path,
    root: &Path,
    env: &BTreeMap<OsString, OsString>,
    original: bool,
    text: &str,
) -> Result<crate::qa_process::Captured> {
    let mut argv = vec![binary.as_os_str().to_owned()];
    if !original {
        argv.push("invalid://endpoint".into());
    }
    let mut spec = CommandSpec::new_os(argv, Duration::from_secs(5));
    spec.cwd = Some(root.into());
    spec.set_os_env(env.clone());
    // CommandSpec inherits its caller. Explicitly remove every variable stripped
    // by the original controller; os_env then restores only selected fixtures.
    spec.env_remove = STRIPPED.iter().map(OsString::from).collect();
    spec.os_env
        .as_mut()
        .ok_or("missing transport environment")?
        .insert("NATIVE_LOG_ENDPOINT".into(), "invalid://endpoint".into());
    spec.input = Some(
        serde_json::to_vec(&json!({"op":"emit","level":40,"text":text}))?
            .into_iter()
            .chain([b'\n'])
            .collect(),
    );
    NativeRunner.run(&spec)
}
pub(super) fn check(source: &Path, native: &Path, root: &Path, output: &Path) -> Result<Value> {
    let env = config();
    let mut report = json!({});
    for (original, binary) in [(true, source), (false, native)] {
        let name = if original { "source" } else { "rust" };
        let absent = tempfile::Builder::new()
            .prefix("native-log-absent-")
            .tempdir_in("/tmp")?;
        let endpoint = format!("ipc://{}", absent.path().join("missing").display());
        let start = Instant::now();
        let mut peer = Peer::start(
            binary,
            root,
            &output.join(name).join("backpressure"),
            &endpoint,
            &env,
            original,
            false,
        )?;
        let started_peer = start.elapsed().as_secs_f64();
        let response = peer.command(json!({"op":"flood","count":5000}))?;
        let replied = start.elapsed().as_secs_f64();
        let sent = response["sent"].as_u64().ok_or("missing sent count")?;
        let dropped = response["dropped"]
            .as_u64()
            .ok_or("missing dropped count")?;
        require(
            sent > 0 && dropped > 0 && sent + dropped == 5000,
            "backpressure accounting differs",
        )?;
        peer.finish()?;
        let duration = start.elapsed().as_secs_f64();
        fs::write(
            output.join(name).join("backpressure/timing.json"),
            serde_json::to_vec_pretty(
                &json!({"start_peer_seconds":started_peer,"command_seconds":replied-started_peer,"finish_seconds":duration-replied,"send_and_shutdown_seconds":duration,"limit_seconds":2.0,"backpressure":response}),
            )?,
        )?;
        require(
            duration < 2.0,
            "backpressure send and shutdown exceeded bound",
        )?;
        report[name] = json!({"backpressure":response,"send_and_shutdown_seconds":duration});
        let receiver = Receiver::new()?;
        let mut peer = Peer::start(
            binary,
            root,
            &output.join(name).join("threads"),
            &receiver.endpoint,
            &env,
            original,
            false,
        )?;
        let response = peer.command(json!({"op":"threads","count":64}))?;
        require(
            response["sent"] == 64 && response["dropped"] == 0,
            "thread delivery differs",
        )?;
        let mut messages = std::collections::BTreeSet::new();
        for _ in 0..64 {
            let packet = peer.receive(&receiver.socket)?;
            let record: Value = serde_json::from_slice(&packet[1..])?;
            messages.insert(
                record["msg"]
                    .as_str()
                    .ok_or("thread message missing")?
                    .to_owned(),
            );
        }
        require(
            messages == (0..64).map(|i| format!("thread-{i}")).collect(),
            "thread messages differ",
        )?;
        receiver.empty()?;
        if !original {
            require(
                peer.command(json!({"op":"close"}))? == json!({"sent":0,"dropped":0,"filtered":0}),
                "native close response differs",
            )?;
        }
        peer.finish()?;
        let receiver = Receiver::new()?;
        let mut peer = Peer::start(
            binary,
            root,
            &output.join(name).join("console-failure"),
            &receiver.endpoint,
            &env,
            original,
            true,
        )?;
        require(
            peer.command(
                json!({"op":"emit","level":40,"text":"console failure remains nonfatal"}),
            )?["sent"]
                == 1,
            "console failure lost IPC",
        )?;
        let packet = peer.receive(&receiver.socket)?;
        let record: Value = serde_json::from_slice(&packet[1..])?;
        require(
            record["msg"] == "console failure remains nonfatal",
            "console-failure record differs",
        )?;
        peer.finish()?;
        for (kind, text) in [("empty", ""), ("nonempty", "invalid endpoint")] {
            let completed = invalid(binary, root, &env, original, text)?;
            if text.is_empty() {
                require(
                    completed.code == 0
                        && serde_json::from_slice::<Value>(&completed.stderr)?["sent"] == 0,
                    "empty invalid endpoint initialized",
                )?;
            } else if original {
                require(
                    completed.code == 0
                        && serde_json::from_slice::<Value>(&completed.stderr)?["dropped"] == 1,
                    "original invalid-connect behavior differs",
                )?;
            } else {
                require(
                    completed.code == 1
                        && String::from_utf8_lossy(&completed.stderr).contains("Transport("),
                    "native invalid-connect error differs",
                )?;
            }
            fs::write(
                output
                    .join(name)
                    .join(format!("{kind}-invalid-endpoint.json")),
                serde_json::to_vec_pretty(
                    &json!({"returncode":completed.code,"stderr":String::from_utf8(completed.stderr)?}),
                )?,
            )?;
        }
        for (signal, label) in [
            (rustix::process::Signal::INT, "SIGINT"),
            (rustix::process::Signal::TERM, "SIGTERM"),
        ] {
            let directory = output.join(name).join(label);
            let mut peer = Peer::start(binary, root, &directory, &endpoint, &env, original, false)?;
            peer.write(&json!({"op":"flood","count":10000000}))?;
            std::thread::sleep(Duration::from_millis(50));
            let started = Instant::now();
            let child = peer.child.as_ref().ok_or("signal producer missing")?;
            // No other operation reaps this direct child before this signal.
            rustix::process::kill_process(rustix::process::Pid::from_child(child), signal)?;
            let code = peer.wait(Duration::from_secs(2))?;
            require(code == -signal.as_raw(), "producer signal status differs")?;
            fs::write(
                directory.join("signal.json"),
                serde_json::to_vec_pretty(
                    &json!({"signal":label,"returncode":code,"seconds":started.elapsed().as_secs_f64()}),
                )?,
            )?;
        }
    }
    // Logger::for_runtime deliberately exercises its ordinary /tmp prefix. Bind
    // our leaf first and retain its private suffix through this sole child.
    let root_temp = tempfile::Builder::new()
        .prefix("native-runtime-log-")
        .tempdir_in("/tmp")?;
    let suffix = root_temp
        .path()
        .file_name()
        .ok_or("runtime suffix missing")?
        .to_str()
        .ok_or("runtime suffix not UTF-8")?;
    let prefix = format!("-{suffix}");
    let path = PathBuf::from(format!("/tmp/logmessage{prefix}"));
    require(
        fs::symlink_metadata(&path).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound),
        "runtime endpoint already exists",
    )?;
    let context = zmq::Context::new();
    let pull = context.socket(zmq::PULL)?;
    pull.set_linger(0)?;
    pull.bind(&format!("ipc://{}", path.display()))?;
    let _leaf = super::socket::SocketLeaf::claim(&path)?;
    let mut env = env;
    env.insert("OPENPILOT_PREFIX".into(), prefix.into());
    // Opt-in collector QA roots are unrelated to this normal-prefix contract.
    env.remove(std::ffi::OsStr::new("ROUTE_UPLOAD_QA_IPC_ROOT"));
    let mut peer = Peer::start(
        native,
        root,
        &output.join("runtime-prefix"),
        "runtime",
        &env,
        false,
        false,
    )?;
    require(
        peer.command(json!({"op":"emit","level":20,"text":"runtime endpoint"}))?["sent"] == 1,
        "runtime-prefix delivery failed",
    )?;
    let packet = peer.receive(&pull)?;
    let record: Value = serde_json::from_slice(&packet[1..])?;
    require(
        record["msg"] == "runtime endpoint",
        "runtime-prefix record differs",
    )?;
    require(
        peer.command(json!({"op":"close"}))?["sent"] == 0,
        "runtime-prefix close differs",
    )?;
    peer.finish()?;
    report["thread_messages_each"] = json!(64);
    report["console_failure"] = json!("IPC delivered and both producers returned successfully");
    report["empty"] = json!("invalid endpoint remains uninitialized");
    report["runtime_prefix"] = json!("received at normal prefixed runtime IPC");
    report["transport_error"]=json!("source ignores invalid connect then drops; Rust returns a typed transport error (caller must handle best effort)");
    report["signals"] =
        json!("source/native default SIGINT and SIGTERM terminate during backpressure");
    fs::write(
        output.join("report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    Ok(report)
}
