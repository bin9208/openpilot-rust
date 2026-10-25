//! Actual public HTTP operations with real private Params and loopback upload peers.
//! The collector binds only after the response and all request owners finish.
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::{service::service_fn, Response};
use hyper_util::rt::TokioIo;
use openpilot_carrot_server::{config::Config, http::Application, params::Backend};
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{mpsc, Arc, Mutex},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
const WARNING: &str =
    "Failed to cast param CarName with value=b'\\xff' from type t=<ParamKeyType.STRING: 0>";

async fn upstream(
    listener: tokio::net::TcpListener,
    mode: String,
    receipts: Arc<Mutex<Vec<serde_json::Value>>>,
) {
    let health = mode.starts_with("health");
    for _ in 0..if health { 2 } else { 1 } {
        let (socket, _) = listener.accept().await.unwrap();
        let mode = mode.clone();
        let receipts = Arc::clone(&receipts);
        hyper::server::conn::http1::Builder::new()
            .keep_alive(false)
            .serve_connection(TokioIo::new(socket), service_fn(move |request: hyper::Request<hyper::body::Incoming>| {
                let mode = mode.clone();
                let receipts = Arc::clone(&receipts);
                async move {
                    let method = request.method().to_string();
                    let path = request.uri().path().to_owned();
                    let body = request.into_body().collect().await.unwrap().to_bytes();
                    let failure = mode.ends_with("failure");
                    let (status, payload) = if path == "/api/v1/health" {
                        assert_eq!(method, "GET");
                        assert!(body.is_empty());
                        (200, "{\"ok\":true}")
                    } else if path == "/api/v1/session" {
                        assert_eq!(method, "POST");
                        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
                        assert_eq!(value["carName"], "none");
                        assert_eq!(value["dongleId"], "owned-dongle");
                        assert_eq!(value["serial"], "owned-serial");
                        assert_eq!(value["purpose"], "test");
                        if failure { (500, "{\"ok\":false,\"error\":\"owned failure\"}") }
                        else { (200, "{\"ok\":true,\"token\":\"owned-token\"}") }
                    } else {
                        assert_eq!(path, "/webhook");
                        assert_eq!(method, "POST");
                        let text = String::from_utf8(body.to_vec()).unwrap();
                        assert!(text.contains("Carrot Vision"));
                        assert!(text.contains("Car name: none"));
                        assert!(text.contains("owned-dongle"));
                        assert!(text.contains("owned-serial"));
                        assert!(text.contains("owned diagnostic bundle"));
                        assert!(text.contains("capture.txt"));
                        if failure { (500, "owned failure") } else { (204, "") }
                    };
                    receipts.lock().unwrap().push(serde_json::json!({"method":method,"path":path,"body":String::from_utf8(body.to_vec()).unwrap()}));
                    Ok::<_, std::convert::Infallible>(Response::builder().status(status).header("content-type", "application/json").body(Full::new(Bytes::from(payload))).unwrap())
                }
            })).await.unwrap();
    }
}
fn application(root: &Path) -> Arc<Application> {
    let mut config = Config::at(
        &root.join("repository"),
        &root.join("data"),
        &root.join("data/settings.json"),
    );
    config.legacy_state = root.join("absent-legacy");
    config.params_backup = root.join("params-backup.json");
    fs::create_dir_all(&config.web).unwrap();
    fs::create_dir_all(&config.shared_assets).unwrap();
    fs::create_dir_all(&config.state).unwrap();
    let params = openpilot_params::Params::for_runtime_at(&root.join("params")).unwrap();
    params.put("CarName", b"\xff").unwrap();
    params.put("DongleId", b"owned-dongle").unwrap();
    params.put("HardwareSerial", b"owned-serial").unwrap();
    Application::new(config, Backend::native(params, root.join("data/state")))
}
async fn operation(root: &Path, mode: &str) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let receipts = Arc::new(Mutex::new(Vec::new()));
    let peer = tokio::spawn(upstream(listener, mode.to_owned(), Arc::clone(&receipts)));
    std::env::set_var("CARROT_WEB_UPLOAD_URL", &base);
    std::env::set_var(
        "CARROT_VISION_DIAG_DISCORD_WEBHOOK_URL",
        format!("{base}/webhook"),
    );
    let mut app = application(root);
    let health = mode.starts_with("health");
    if !health {
        use openpilot_carrot_server::{vision_diag, vision_test, youtube_test::CommandSpec};
        let spec = || CommandSpec {
            path: root.join("absent-child"),
            pattern: "owned-absent-child".into(),
            args: Vec::new(),
        };
        let vision = vision_test::Config {
            repository: root.join("repository"),
            state: root.join("vision-state.json"),
            log: root.join("vision.log"),
            params_root: Some(root.join("params")),
            launcher: root.join("absent-launcher"),
            runner: spec(),
            children: [
                ("camerad".into(), spec()),
                ("stream_encoderd".into(), spec()),
                ("webrtcd".into(), spec()),
            ],
            port: 0,
        };
        fs::create_dir_all(root.join("proc/net")).unwrap();
        Arc::get_mut(&mut app).unwrap().vision_diag = Some(vision_diag::Config {
            repository: root.join("repository"),
            proc_root: root.join("proc"),
            vision,
            params_available: true,
            params_root: Some(root.join("params")),
            launcher: root.join("absent-launcher"),
            process_matches: [
                ("camerad", "owned-camera".into()),
                ("stream_encoderd", "owned-stream".into()),
                ("encoderd", "owned-encoder".into()),
                ("webrtcd", "owned-webrtc".into()),
                ("carrot_web", "owned-web".into()),
            ],
        });
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let request_app = Arc::clone(&app);
    let server = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        hyper::server::conn::http1::Builder::new()
            .keep_alive(false)
            .serve_connection(
                TokioIo::new(socket),
                service_fn(move |request| {
                    let app = Arc::clone(&request_app);
                    async move {
                        let request = openpilot_carrot_server::http::decode_request(request);
                        let response = if health {
                            openpilot_carrot_server::dashcam::health_handle(
                                request,
                                Arc::clone(&app.dashcam_upload_health),
                            )
                            .await
                        } else {
                            openpilot_carrot_server::vision_diag::http::handle(request, app).await
                        };
                        Ok::<_, std::convert::Infallible>(response)
                    }
                }),
            )
            .await
            .unwrap();
    });
    let (path, body) = if health {
        ("/api/dashcam/upload/test", "{}")
    } else {
        ("/api/vision_diag/upload_discord", "{\"bundle\":\"owned diagnostic bundle\",\"filename\":\"capture.txt\",\"source\":\"owned-test\"}")
    };
    let mut socket = tokio::net::TcpStream::connect(address).await.unwrap();
    socket.write_all(format!("POST {path} HTTP/1.1\r\nHost: {address}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
    let mut bytes = Vec::new();
    socket.read_to_end(&mut bytes).await.unwrap();
    server.await.unwrap();
    peer.await.unwrap();
    let text = String::from_utf8(bytes.clone()).unwrap();
    let (headers, body) = text.split_once("\r\n\r\n").unwrap();
    let status: u16 = headers.split_whitespace().nth(1).unwrap().parse().unwrap();
    let value: serde_json::Value = serde_json::from_str(body).unwrap();
    let failure = mode.ends_with("failure");
    assert_eq!(
        status,
        if failure {
            if health {
                500
            } else {
                502
            }
        } else {
            200
        }
    );
    assert_eq!(value["ok"], !failure);
    if health {
        if failure {
            assert_eq!(value["error"], "upload session HTTP 500: owned failure");
        } else {
            assert_eq!(value["session"], "automatic");
            assert_eq!(value["target"], "web");
            assert_eq!(value["url"], base);
        }
    } else {
        assert_eq!(value["discord"]["configured"], true);
        assert_eq!(value["discord"]["filename"], "capture.txt");
        assert_eq!(value["discord"]["status"], if failure { 500 } else { 204 });
        if failure {
            assert_eq!(value["discord"]["error"], "owned failure");
        } else {
            assert_eq!(value["discord"]["source"], "owned-test");
        }
    }
    fs::write(root.join("response.http"), bytes).unwrap();
    fs::write(
        root.join("upstream.json"),
        serde_json::to_vec_pretty(&*receipts.lock().unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(receipts.lock().unwrap().len(), if health { 2 } else { 1 });
    drop(app); // All request/application owners finish before the delayed collector.
}
fn worker(root: &Path, mode: &str) {
    let _logs = openpilot_logging::producer::RuntimeLogScope;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        tokio::time::timeout(Duration::from_secs(12), operation(root, mode))
            .await
            .unwrap();
    });
    drop(runtime); // Join all blocking workers before operation completion marker.
    println!("COMPLETED");
    std::io::stdout().flush().unwrap();
    let mut ack = [0];
    std::io::stdin().read_exact(&mut ack).unwrap();
    assert_eq!(ack, [b'!']);
}
struct Owner {
    child: Child,
    reader: Option<JoinHandle<()>>,
    reaped: bool,
}
impl Drop for Owner {
    fn drop(&mut self) {
        if !self.reaped {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        if let Some(reader) = self.reader.take() {
            reader.join().unwrap();
        }
    }
}
struct FailureOutput<'a> {
    root: &'a Path,
    mode: &'a str,
}
impl Drop for FailureOutput<'_> {
    fn drop(&mut self) {
        if !thread::panicking() {
            return;
        }
        let Some(base) = std::env::var_os("CARROT_LOG_FAILURE_OUTPUT") else {
            return;
        };
        let retain = || -> std::io::Result<()> {
            let base = PathBuf::from(base);
            fs::create_dir_all(&base)?;
            let destination = base.join(self.mode);
            fs::create_dir(&destination)?;
            for name in ["stderr", "response.http", "upstream.json"] {
                let source = self.root.join(name);
                if source.is_file() {
                    fs::copy(source, destination.join(name))?;
                }
            }
            Ok(())
        };
        if let Err(error) = retain() {
            eprintln!("retain owned HTTP failure: {error}");
        }
    }
}
struct SocketPath(PathBuf);
impl Drop for SocketPath {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
fn check(mode: &str) {
    let root = tempfile::tempdir().unwrap();
    let _failure = FailureOutput {
        root: root.path(),
        mode,
    };
    let prefix = format!(
        "_carrotlog_{}_{}",
        std::process::id(),
        root.path()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .trim_start_matches('.')
    );
    let endpoint = SocketPath(format!("/tmp/logmessage{prefix}").into());
    fs::create_dir_all(root.path().join("repository")).unwrap();
    fs::create_dir_all(root.path().join("bin")).unwrap();
    let stderr = root.path().join("stderr");
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--worker", mode])
        .arg(root.path())
        .env("OPENPILOT_PREFIX", prefix)
        .env("OPENPILOT_ROOT", root.path().join("repository"))
        .env("CARROT_REPO_DIR", root.path().join("repository"))
        .env("CARROT_DATA_DIR", root.path().join("data"))
        .env("PATH", root.path().join("bin"))
        .env_remove("OPENPILOT_LOG_CONTEXT")
        .env_remove("MANAGER_DAEMON")
        .env_remove("LOGPRINT")
        .env_remove("CARROT_WEB_UPLOAD_TOKEN")
        .env_remove("CARROT_DISCORD_WEBHOOK_URL")
        .env_remove("DISCORD_WEBHOOK_URL")
        .env_remove("CARROT_DEVICE_SERIAL")
        .env_remove("DEVICE_SERIAL")
        .env_remove("SERIAL")
        .env_remove("HTTP_PROXY")
        .env_remove("HTTPS_PROXY")
        .env_remove("ALL_PROXY")
        .env_remove("http_proxy")
        .env_remove("https_proxy")
        .env_remove("all_proxy")
        .env("NO_PROXY", "127.0.0.1,localhost")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(fs::File::create(&stderr).unwrap());
    let mut owner = Owner {
        child: command.spawn().unwrap(),
        reader: None,
        reaped: false,
    };
    let stdout = owner.child.stdout.take().unwrap();
    let (send, ready) = mpsc::sync_channel(1);
    owner.reader = Some(thread::spawn(move || {
        let mut line = String::new();
        let result = BufReader::new(stdout).read_line(&mut line);
        let _ = send.send(result.map(|_| line));
    }));
    assert_eq!(
        ready
            .recv_timeout(Duration::from_secs(18))
            .unwrap()
            .unwrap(),
        "COMPLETED\n",
        "HTTP operation failed: {}",
        fs::read_to_string(&stderr).unwrap()
    );
    let context = zmq::Context::new();
    let collector = context.socket(zmq::PULL).unwrap();
    collector.set_linger(0).unwrap();
    collector.set_rcvtimeo(2000).unwrap();
    collector
        .bind(&format!("ipc://{}", endpoint.0.display()))
        .unwrap();
    let packet = collector.recv_bytes(0).unwrap_or_else(|error| {
        panic!(
            "{mode} metadata warning lost after HTTP completion: {error}; stderr={}",
            fs::read_to_string(&stderr).unwrap()
        )
    });
    assert_eq!(packet[0], 30);
    let value: serde_json::Value = serde_json::from_slice(&packet[1..]).unwrap();
    assert_eq!(value["msg"], WARNING);
    assert_eq!(value["levelnum"], 30);
    assert_eq!(value["name"], "swaglog");
    assert_eq!(value["process"], owner.child.id());
    assert!(value["pathname"]
        .as_str()
        .unwrap()
        .ends_with("params-typed/src/lib.rs"));
    assert!(value["funcName"]
        .as_str()
        .unwrap()
        .ends_with("::get_string"));
    assert_eq!(collector.recv_bytes(zmq::DONTWAIT), Err(zmq::Error::EAGAIN));
    owner.child.stdin.take().unwrap().write_all(b"!").unwrap();
    let began = Instant::now();
    loop {
        if let Some(status) = owner.child.try_wait().unwrap() {
            owner.reaped = true;
            assert!(status.success());
            break;
        }
        assert!(began.elapsed() < Duration::from_secs(3));
        thread::sleep(Duration::from_millis(5));
    }
    println!("PASS metadata HTTP log lifetime {mode}");
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).is_some_and(|a| a == "--worker") {
        worker(Path::new(&args[3]), &args[2]);
        return;
    }
    let mut failures = Vec::new();
    for mode in [
        "health-success",
        "health-failure",
        "vision-success",
        "vision-failure",
    ] {
        if std::panic::catch_unwind(|| check(mode)).is_err() {
            failures.push(mode);
        }
    }
    assert!(
        failures.is_empty(),
        "metadata HTTP lifetime failures: {failures:?}"
    );
}
