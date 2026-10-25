//! Real HTTP source-reference checks using independently captured urllib outputs.
use openpilot_dev_tools::{
    qa_process::{CommandSpec, NativeRunner, Runner},
    timed_fixtures::Fixture,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    path::Path,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

struct Server {
    endpoint: String,
    requests: Arc<Mutex<Vec<Value>>>,
    stop: Arc<AtomicBool>,
    owner: Option<thread::JoinHandle<()>>,
}
impl Server {
    fn new(mode: &str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!("http://{}/geo", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(vec![]));
        let captured = Arc::clone(&requests);
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = Arc::clone(&stop);
        let mode = mode.to_owned();
        let owner = thread::spawn(move || {
            let mut workers = vec![];
            while !stopping.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let mode = mode.clone();
                        let captured = Arc::clone(&captured);
                        workers.push(thread::spawn(move || {
                            let _ = response(stream, &mode, &captured);
                        }));
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2))
                    }
                    Err(e) => panic!("HTTP fixture accept: {e}"),
                }
            }
            for worker in workers {
                worker.join().unwrap();
            }
        });
        Self {
            endpoint,
            requests,
            stop,
            owner: Some(owner),
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.owner.take().unwrap().join().unwrap();
    }
}
fn response(
    mut stream: TcpStream,
    mode: &str,
    captures: &Mutex<Vec<Value>>,
) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let path = line.split_whitespace().nth(1).unwrap_or("").to_owned();
    let mut headers = std::collections::HashMap::new();
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 || line == "\r\n" || line == "\n" {
            break;
        }
        if let Some((key, value)) = line.split_once(':') {
            headers.insert(key.to_ascii_lowercase(), value.trim().to_owned());
        }
    }
    captures.lock().unwrap().push(json!({"path":path,"agent":headers.get("user-agent"),"encoding":headers.get("accept-encoding")}));
    let redirect = if mode.starts_with("redirect-") && mode != "redirect-loop" {
        let index = path
            .strip_prefix("/r")
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(0);
        let limit = mode
            .strip_prefix("redirect-")
            .unwrap()
            .parse::<usize>()
            .unwrap();
        (index < limit).then(|| format!("/r{}", index + 1))
    } else if mode == "redirect-loop" {
        Some("/again".into())
    } else if mode == "redirect" && path != "/final" {
        Some("/final".into())
    } else {
        None
    };
    if let Some(target) = redirect {
        write!(
            stream,
            "HTTP/1.0 301 Moved Permanently\r\nLocation: {target}\r\n\r\n"
        )?;
        return Ok(());
    }
    let body=match mode {
        "malformed"=>b"{invalid".to_vec(),
        "null-zone"=>br#"{"status": "success", "timezone": null}"#.to_vec(),
        "duplicate-status-valid"=>br#"{"status":"fail","status":"success","timezone":"Asia/Seoul"}"#.to_vec(),
        "duplicate-status-invalid"=>br#"{"status":"success","status":"fail","timezone":"Asia/Seoul"}"#.to_vec(),
        "duplicate-zone-valid"=>br#"{"status":"success","timezone":"missing","timezone":"Asia/Seoul"}"#.to_vec(),
        "duplicate-zone-invalid"=>br#"{"status":"success","timezone":"Asia/Seoul","timezone":"missing"}"#.to_vec(),
        "extra-nan"=>br#"{"status":"success","timezone":"Asia/Seoul","unused":NaN}"#.to_vec(),
        "extra-infinity"=>br#"{"status":"success","timezone":"Asia/Seoul","unused":Infinity}"#.to_vec(),
        "extra-negative-infinity"=>br#"{"status":"success","timezone":"Asia/Seoul","unused":-Infinity}"#.to_vec(),
        _=>serde_json::to_vec(&json!({"status":if mode=="failure-status" {"fail"} else {"success"},"timezone":if mode=="invalid-zone" {"missing"} else {"Asia/Seoul"}})).unwrap(),
    };
    if mode == "headers-late" || mode == "headers-stall" {
        thread::sleep(Duration::from_secs_f64(if mode == "headers-late" {
            5.05
        } else {
            5.6
        }));
    }
    write!(
        stream,
        "HTTP/1.0 {} OK\r\nContent-Length: {}\r\n\r\n",
        if mode == "http-error" { 503 } else { 200 },
        body.len()
    )?;
    if mode == "progress" {
        stream.write_all(&body[..1])?;
        stream.flush()?;
        thread::sleep(Duration::from_secs_f64(2.8));
        stream.write_all(&body[1..2])?;
        stream.flush()?;
        thread::sleep(Duration::from_secs_f64(2.8));
        stream.write_all(&body[2..])?;
    } else {
        if mode == "body-late" || mode == "body-stall" {
            thread::sleep(Duration::from_secs_f64(if mode == "body-late" {
                5.05
            } else {
                5.6
            }));
        }
        stream.write_all(&body)?;
    }
    stream.flush()
}

fn case(expected: &Value) -> Value {
    let mode = expected["mode"].as_str().unwrap();
    let temporary = tempfile::tempdir().unwrap();
    let mut fixture = Fixture::new(
        temporary.path(),
        "UTC",
        Path::new(env!("CARGO_BIN_EXE_timed-command-fixture")),
    )
    .unwrap();
    let server = Server::new(mode);
    fixture.config["endpoint"] = json!(server.endpoint);
    fixture.config["actions"] = json!([{"kind":"internet"}]);
    let mut spec = CommandSpec::new_os(
        vec![env!("CARGO_BIN_EXE_timed-source-reference").into()],
        Duration::from_secs(30),
    );
    fixture.configure(&mut spec);
    spec.input = Some(format!("{}\n", json!({"config":fixture.config,"loop":false})).into_bytes());
    spec.stdout_limit = usize::MAX;
    spec.stderr_limit = usize::MAX;
    let start = Instant::now();
    let result = NativeRunner.run(&spec).unwrap();
    let elapsed = start.elapsed().as_secs_f64();
    assert_eq!(
        result.code,
        0,
        "{mode}: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let result: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(result["errors"], json!([null]), "{mode}");
    assert_eq!(result["state"]["records"], json!([]), "{mode}");
    let requests = server.requests.lock().unwrap().clone();
    if mode == "progress" {
        assert!(elapsed > 5., "{mode}: {elapsed}");
    } else if mode.ends_with("late") || mode.ends_with("stall") {
        assert!(elapsed > 4.8 && elapsed < 6., "{mode}: {elapsed}");
    }
    json!({"mode":mode,"value":result["values"][0]["ok"],"requests":requests})
}

#[test]
fn all_22_original_real_http_requests_and_socket_timeout_boundaries() {
    let bytes = include_bytes!("fixtures/timed-tools/http-original.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        "e6fdd0897d7bee8c7a3f7712e3a8d61e8a9a7e1a259f9b67b8eaa8ba80204510"
    );
    let data: Value = serde_json::from_slice(bytes).unwrap();
    let cases = data["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 22);
    let next = AtomicUsize::new(0);
    let results = Mutex::new(vec![]);
    thread::scope(|scope| {
        let mut workers = vec![];
        for _ in 0..8 {
            workers.push(scope.spawn(|| loop {
                let index = next.fetch_add(1, Ordering::SeqCst);
                let Some(expected) = cases.get(index) else {
                    break;
                };
                let observed = case(expected);
                results.lock().unwrap().push((index, observed));
            }));
        }
        for worker in workers {
            worker.join().unwrap();
        }
    });
    let mut results = results.into_inner().unwrap();
    results.sort_by_key(|row| row.0);
    let expected = cases
        .iter()
        .map(|row| json!({"mode":row["mode"],"value":row["value"],"requests":row["requests"]}))
        .collect::<Vec<_>>();
    assert_eq!(
        results.into_iter().map(|row| row.1).collect::<Vec<_>>(),
        expected
    );
}
