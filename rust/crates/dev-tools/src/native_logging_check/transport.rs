use super::*;
struct RuntimeReceiver {
    // Drop closes libzmq's transferred FD and joins its context before the
    // retained listener and authenticated path owner are released.
    pull: zmq::Socket,
    _context: zmq::Context,
    _listener: std::os::unix::net::UnixListener,
    _leaf: lifecycle::Leaf,
}
fn bind_runtime(path: &Path) -> Result<RuntimeReceiver> {
    // Atomic bind rejects an occupied shared leaf. ZMQ_USE_FD prevents libzmq
    // from unlinking a foreign replacement between a precheck and its bind.
    let listener = std::os::unix::net::UnixListener::bind(path)?;
    let leaf = lifecycle::Leaf::from_listener(path, &listener)?;
    listener.set_nonblocking(true)?;
    let context = zmq::Context::new();
    let mut pull = context.socket(zmq::PULL)?;
    pull.set_linger(0)?;
    attach_listener(&mut pull, &listener, &format!("ipc://{}", path.display()))?;
    Ok(RuntimeReceiver {
        pull,
        _context: context,
        _listener: listener,
        _leaf: leaf,
    })
}
fn attach_listener(
    pull: &mut zmq::Socket,
    listener: &std::os::unix::net::UnixListener,
    endpoint: &str,
) -> Result<()> {
    use std::os::fd::{AsRawFd, IntoRawFd};
    let fd = listener.try_clone()?;
    let number = fd.as_raw_fd();
    // SAFETY: pull is a live libzmq socket; option receives exactly one C int.
    // The clone stays Rust-owned until bind succeeds. libzmq 4.3.4's
    // ipc_listener::set_local_address has no failure after assigning use_fd;
    // socket_base::bind returns success on that path. Failed binds therefore
    // leave the clone here; successful binds transfer exactly this clone.
    // This socket is never rebound or reused after an error (also in tests).
    #[allow(unsafe_code)] // Confined to this checked ZMQ_USE_FD FFI call.
    let status = unsafe {
        zmq_sys::zmq_setsockopt(
            pull.as_mut_ptr(),
            zmq_sys::ZMQ_USE_FD as i32,
            (&number as *const i32).cast(),
            std::mem::size_of_val(&number),
        )
    };
    require(status == 0, "runtime prebound FD option rejected")?;
    pull.bind(endpoint)?;
    let _transferred = fd.into_raw_fd();
    Ok(())
}
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
    lifecycle::run(&spec)
}
pub(super) fn check(source: &Path, native: &Path, root: &Path, output: &Path) -> Result<Value> {
    check_roles(&[(true, source), (false, native)], native, root, output)
}
pub(super) fn check_native(native: &Path, root: &Path, output: &Path) -> Result<Value> {
    check_roles(&[(false, native)], native, root, output)
}
fn check_roles(
    roles: &[(bool, &Path)],
    native: &Path,
    root: &Path,
    output: &Path,
) -> Result<Value> {
    let env = config();
    let mut report = json!({});
    for &(original, binary) in roles {
        let name = if original { "source" } else { "rust" };
        let absent = lifecycle::Directory::new(
            tempfile::Builder::new()
                .prefix("native-log-absent-")
                .tempdir_in("/tmp")?,
        );
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
    let root_temp = lifecycle::Directory::new(
        tempfile::Builder::new()
            .prefix("native-runtime-log-")
            .tempdir_in("/tmp")?,
    );
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
    let receiver = bind_runtime(&path)?;
    fs::write(
        output.join("runtime-endpoint.json"),
        serde_json::to_vec_pretty(
            &json!({"path":path,"private_marker":root_temp.path(),"ownership":"prebound FD + exact SO_COOKIE/UNIX_DIAG_VFS"}),
        )?,
    )?;
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
    let packet = peer.receive(&receiver.pull)?;
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
    // Preserve legacy report bytes/semantics; the one-role mode must not claim
    // that an original producer ran in this new execution.
    if roles.len() == 1 && !roles[0].0 {
        report["executed_roles"] = json!(["native"]);
        report["console_failure"] = json!("native IPC delivered and native producer returned successfully");
        report["signals"] = json!("native default SIGINT and SIGTERM terminate during backpressure");
        report["transport_error"] = json!("native returns a typed transport error; original invalid-connect drop is retained historical evidence");
    }
    fs::write(
        output.join("report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::{
        fs::{MetadataExt, symlink},
        net::{UnixDatagram, UnixListener},
    };
    #[test]
    fn prebound_zmq_rejects_occupied_nodes_without_unlinking() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("leaf");
        fs::write(&path, b"foreign").unwrap();
        assert!(bind_runtime(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"foreign");
        fs::remove_file(&path).unwrap();
        symlink("missing", &path).unwrap();
        assert!(bind_runtime(&path).is_err());
        assert_eq!(fs::read_link(&path).unwrap(), Path::new("missing"));
        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        let inode = fs::symlink_metadata(&path).unwrap().ino();
        assert!(bind_runtime(&path).is_err());
        assert_eq!(fs::symlink_metadata(&path).unwrap().ino(), inode);
        fs::remove_dir(&path).unwrap();
        for stream in [false, true] {
            let listener = stream.then(|| UnixListener::bind(&path).unwrap());
            let datagram = (!stream).then(|| UnixDatagram::bind(&path).unwrap());
            let inode = fs::symlink_metadata(&path).unwrap().ino();
            assert!(bind_runtime(&path).is_err());
            assert_eq!(fs::symlink_metadata(&path).unwrap().ino(), inode);
            drop((listener, datagram));
            fs::remove_file(&path).unwrap();
        }
    }
    #[test]
    fn actual_zmq_listener_preserves_foreign_regular_datagram_and_closed_stream_replacements() {
        for kind in [
            "regular",
            "datagram",
            "closed-stream",
            "symlink",
            "directory",
        ] {
            let root = tempfile::tempdir().unwrap();
            let path = root.path().join("leaf");
            let receiver = bind_runtime(&path).unwrap();
            fs::remove_file(&path).unwrap();
            match kind {
                "regular" => fs::write(&path, b"foreign").unwrap(),
                "datagram" => drop(UnixDatagram::bind(&path).unwrap()),
                "symlink" => symlink("missing", &path).unwrap(),
                "directory" => fs::create_dir(&path).unwrap(),
                _ => drop(UnixListener::bind(&path).unwrap()),
            }
            let inode = fs::symlink_metadata(&path).unwrap().ino();
            drop(receiver);
            assert_eq!(fs::symlink_metadata(&path).unwrap().ino(), inode);
        }
    }
    #[test]
    fn actual_zmq_listener_cleanup_removes_only_authenticated_original() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("leaf");
        let receiver = bind_runtime(&path).unwrap();
        drop(receiver);
        assert!(!path.exists());
    }
    fn descriptor_count(inode: u64) -> usize {
        let target = format!("socket:[{inode}]");
        fs::read_dir("/proc/self/fd")
            .unwrap()
            .filter_map(|entry| fs::read_link(entry.ok()?.path()).ok())
            .filter(|path| path.as_os_str() == std::ffi::OsStr::new(&target))
            .count()
    }
    #[test]
    fn prebound_fd_transfers_one_clone_and_closes_both_after_real_delivery() {
        use std::os::fd::AsFd;
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("leaf");
        let receiver = bind_runtime(&path).unwrap();
        let inode = rustix::fs::fstat(receiver._listener.as_fd())
            .unwrap()
            .st_ino;
        assert_eq!(descriptor_count(inode), 2);
        let context = zmq::Context::new();
        let push = context.socket(zmq::PUSH).unwrap();
        push.set_linger(0).unwrap();
        push.connect(&format!("ipc://{}", path.display())).unwrap();
        assert!(push.poll(zmq::POLLOUT, 1000).unwrap() > 0);
        push.send(&b"owned FD"[..], zmq::DONTWAIT).unwrap();
        assert!(receiver.pull.poll(zmq::POLLIN, 1000).unwrap() > 0);
        assert_eq!(
            receiver.pull.recv_bytes(zmq::DONTWAIT).unwrap(),
            b"owned FD"
        );
        drop(receiver);
        assert_eq!(descriptor_count(inode), 0);
        assert!(!path.exists());
    }
    #[test]
    fn rejected_bind_closes_only_the_untransferred_clone() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("leaf");
        let listener = UnixListener::bind(&path).unwrap();
        let leaf = lifecycle::Leaf::from_listener(&path, &listener).unwrap();
        listener.set_nonblocking(true).unwrap();
        let inode = rustix::fs::fstat(&listener).unwrap().st_ino;
        // Reject before transport selection, and inside IPC address resolution.
        for endpoint in [
            "invalid://endpoint".to_owned(),
            format!("ipc:///tmp/{}", "x".repeat(200)),
        ] {
            let context = zmq::Context::new();
            let mut pull = context.socket(zmq::PULL).unwrap();
            pull.set_linger(0).unwrap();
            assert!(attach_listener(&mut pull, &listener, &endpoint).is_err());
            assert_eq!(descriptor_count(inode), 1);
            drop(pull);
            drop(context);
            assert_eq!(descriptor_count(inode), 1);
        }
        drop(listener);
        drop(leaf);
        assert_eq!(descriptor_count(inode), 0);
        assert!(!path.exists());
    }
}
