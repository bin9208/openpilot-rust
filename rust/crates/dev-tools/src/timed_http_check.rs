//! Real loopback handlers from check_timed_http.py, including progressing reads.
use super::*;
use std::{
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    thread,
    time::Instant,
};
struct Server {
    endpoint: String,
    requests: Arc<Mutex<Vec<Value>>>,
    stop: Arc<AtomicBool>,
    owner: Option<thread::JoinHandle<()>>,
}
impl Server {
    fn new(mode: &str) -> Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        listener.set_nonblocking(true)?;
        let endpoint = format!("http://{}/geo", listener.local_addr()?);
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
        Ok(Self {
            endpoint,
            requests,
            stop,
            owner: Some(owner),
        })
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
    let redirect = if matches!(mode, "redirect-10" | "redirect-11") {
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
            "HTTP/1.0 302 Found\r\nLocation: {target}\r\nContent-Length: 0\r\n\r\n"
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

const MODES: [&str; 22] = [
    "success",
    "redirect",
    "http-error",
    "failure-status",
    "invalid-zone",
    "null-zone",
    "malformed",
    "headers-late",
    "headers-stall",
    "body-late",
    "body-stall",
    "progress",
    "duplicate-status-valid",
    "duplicate-status-invalid",
    "duplicate-zone-valid",
    "duplicate-zone-invalid",
    "extra-nan",
    "extra-infinity",
    "extra-negative-infinity",
    "redirect-10",
    "redirect-11",
    "redirect-loop",
];
fn worker(
    options: &Options,
    mode: &str,
    implementation: &str,
    runner: &mut dyn Runner,
) -> Result<JsonValue> {
    fs::create_dir_all(&options.output)?;
    let directory = tempfile::tempdir()?;
    let mut fixture = Fixture::new(directory.path(), "UTC", &options.command)?;
    let server = Server::new(mode)?;
    fixture.config["endpoint"] = json!(server.endpoint);
    fixture.config["actions"] = json!([{"kind":"internet"}]);
    let start = Instant::now();
    let value = if implementation == "python" {
        let mut spec = CommandSpec::new_os(
            vec![options.reference.as_os_str().to_owned()],
            Duration::from_secs(30),
        );
        fixture.configure(&mut spec);
        spec.input =
            Some(format!("{}\n", json!({"config":fixture.config,"loop":false})).into_bytes());
        spec.stdout_limit = usize::MAX;
        spec.stderr_limit = usize::MAX;
        let captured = NativeRunner.run(&spec)?;
        require(captured.code == 0, "source HTTP reference failed")?;
        field(
            pyjson::sequence(&field(&pyjson::parse(&captured.stdout)?, "values")?)?
                .first()
                .ok_or("missing source HTTP result")?,
            "ok",
        )?
    } else {
        let (rows, records) = timed_fixtures::native(
            &fixture,
            &options.binary,
            &options.output.join("native.jsonl"),
            runner,
        )?;
        require(records.is_empty(), "unexpected HTTP logs")?;
        field(
            &field(rows.first().ok_or("missing HTTP result")?, "result")?,
            "ok",
        )?
    };
    let elapsed = start.elapsed().as_secs_f64();
    let requests = server
        .requests
        .lock()
        .map_err(|_| "HTTP capture poisoned")?
        .clone();
    let requests = requests
        .iter()
        .map(|row| {
            object(&[
                ("path", node(&row["path"])?),
                ("agent", node(&row["agent"])?),
                ("encoding", node(&row["encoding"])?),
            ])
        })
        .collect::<Result<Vec<_>>>()?;
    // Stop/join the real server before writing result.json, including delayed
    // response workers whose client closed at its five-second socket deadline.
    drop(server);
    let result = object(&[
        ("mode", JsonValue::text(mode)),
        ("implementation", JsonValue::text(implementation)),
        ("value", value),
        ("elapsed", node(&json!(elapsed))?),
        ("requests", array(&requests)?),
    ])?;
    write_json(&options.output.join("result.json"), &result)?;
    Ok(result)
}
pub(super) fn run(options: &Options, runner: &mut dyn Runner) -> Result<String> {
    if !options.worker.is_empty() {
        require(
            options.worker.len() == 2,
            "HTTP worker expects mode and implementation",
        )?;
        worker(options, &options.worker[0], &options.worker[1], runner)?;
        return Ok(String::new());
    }
    let next = AtomicUsize::new(0);
    let results = Mutex::new(vec![None; 44]);
    thread::scope(|scope| {
        for _ in 0..8 {
            let next = &next;
            let results = &results;
            scope.spawn(move || loop {
                let index = next.fetch_add(1, Ordering::Relaxed);
                if index >= 44 {
                    break;
                }
                let mode = MODES[index / 2];
                let implementation = if index.is_multiple_of(2) {
                    "python"
                } else {
                    "rust"
                };
                let mut child = options.clone();
                child.output = options.output.join(format!("{mode}-{implementation}"));
                let result = worker(&child, mode, implementation, &mut NativeRunner)
                    .map_err(|error| error.to_string());
                results.lock().expect("HTTP result mutex")[index] = Some(result);
            });
        }
    });
    let results = results
        .into_inner()
        .map_err(|_| "HTTP result mutex poisoned")?
        .into_iter()
        .map(|row| row.ok_or("missing HTTP worker result")?.map_err(Into::into))
        .collect::<Result<Vec<_>>>()?;
    for pair in results.chunks_exact(2) {
        let left = &pair[0];
        let right = &pair[1];
        require(
            pyjson::equal(&field(left, "value")?, &field(right, "value")?),
            "HTTP value differs",
        )?;
        require(
            pyjson::equal(&field(left, "requests")?, &field(right, "requests")?),
            "HTTP request differs",
        )?;
        let mode = field(left, "mode")?.to_utf8().ok_or("HTTP mode not text")?;
        let elapsed = |row: &JsonValue| -> Result<f64> {
            match field(row, "elapsed")?.view() {
                JsonView::Float(value) => Ok(value),
                _ => Err("HTTP elapsed not numeric".into()),
            }
        };
        if mode == "progress" {
            require(
                field(left, "value")?.text_eq("Asia/Seoul")
                    && elapsed(left)? > 5.
                    && elapsed(right)? > 5.,
                "HTTP progressing body did not retain real duration",
            )?;
        } else if mode.ends_with("late") || mode.ends_with("stall") {
            require(
                is_null(&field(left, "value")?)
                    && elapsed(left)? > 4.8
                    && elapsed(left)? < 6.
                    && elapsed(right)? > 4.8
                    && elapsed(right)? < 6.,
                "HTTP idle socket deadline changed",
            )?;
        }
    }
    write_json(
        &options.output.join("summary.json"),
        &object(&[
            ("passed", node(&json!(true))?),
            ("executions", node(&json!(results.len()))?),
            ("results", array(&results)?),
        ])?,
    )?;
    Ok(format!(
        "PASS: {} original urllib/native HTTP executions\n",
        results.len()
    ))
}
