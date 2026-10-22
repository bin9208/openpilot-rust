//! Original ten-second per-socket deadlines, including eleven-second progress.
use super::{file, marked, require, Result};
use crate::qa_process::{CommandSpec, NativeRunner, Runner};
use serde_json::{json, Value};
use std::{
    fs,
    io::Write,
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

struct Peer {
    url: String,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<std::result::Result<Vec<Value>, String>>>,
}
fn reply(
    stream: &mut TcpStream,
    url: &str,
    stage: &str,
    mode: &str,
    request: &Value,
) -> Result<()> {
    let current = if request["method"] == "GET" {
        "get"
    } else {
        "put"
    };
    let active = stage.starts_with(current);
    let body = if current == "get" {
        serde_json::to_vec(&json!({"url":format!("{url}/put"),"headers":{}}))?
    } else {
        b"uploaded".to_vec()
    };
    let delay = Duration::from_secs(if mode == "late" { 11 } else { 12 });
    if active && stage.ends_with("headers") {
        thread::sleep(delay);
    }
    let result = (|| -> std::io::Result<()> {
        write!(
            stream,
            "HTTP/1.0 200 Fixture\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )?;
        if active && mode == "progress" {
            for part in [&body[..1], &body[1..2], &body[2..]] {
                stream.write_all(part)?;
                stream.flush()?;
                if part.len() == 1 {
                    thread::sleep(Duration::from_millis(5500));
                }
            }
        } else {
            if active && stage.ends_with("body") {
                thread::sleep(delay);
            }
            stream.write_all(&body)?;
            stream.flush()?;
        }
        Ok(())
    })();
    match result {
        Ok(()) => Ok(()),
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::BrokenPipe | std::io::ErrorKind::ConnectionReset
            ) =>
        {
            Ok(())
        }
        Err(error) => Err(error.into()),
    }
}
impl Peer {
    fn new(stage: &str, mode: &str) -> Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        listener.set_nonblocking(true)?;
        let url = format!("http://{}", listener.local_addr()?);
        let peer_url = url.clone();
        let stage = stage.to_owned();
        let mode = mode.to_owned();
        let stop = Arc::new(AtomicBool::new(false));
        let ended = Arc::clone(&stop);
        let handle = thread::spawn(move || {
            let mut rows = Vec::new();
            let start = Instant::now();
            while !ended.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let request = crate::uploader_reference_check::http::receive(&mut stream)
                            .map_err(|e| e.to_string())?;
                        let mut value = serde_json::to_value(request).map_err(|e| e.to_string())?;
                        value["time"] = json!(start.elapsed().as_secs_f64());
                        reply(&mut stream, &peer_url, &stage, &mode, &value)
                            .map_err(|e| e.to_string())?;
                        rows.push(value);
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2))
                    }
                    Err(error) => return Err(error.to_string()),
                }
            }
            Ok(rows)
        });
        Ok(Self {
            url,
            stop,
            thread: Some(handle),
        })
    }
    fn finish(mut self) -> Result<Vec<Value>> {
        self.stop.store(true, Ordering::Release);
        self.thread
            .take()
            .ok_or("timeout HTTP worker absent")?
            .join()
            .map_err(|_| "timeout HTTP worker panicked")?
            .map_err(Into::into)
    }
}
impl Drop for Peer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
}

fn scenario(root: &Path, trace: &Path, output: &Path, expected: &Value) -> Result<Value> {
    let stage = expected["stage"]
        .as_str()
        .ok_or("original timeout stage missing")?;
    let mode = expected["mode"]
        .as_str()
        .ok_or("original timeout mode missing")?;
    require(
        ["get-headers", "get-body", "put-headers", "put-body"].contains(&stage)
            && ["late", "stall", "progress"].contains(&mode),
        "unknown original timeout scope",
    )?;
    require(
        mode != "progress" || stage.ends_with("body"),
        "progress must target a response body",
    )?;
    fs::create_dir_all(output)?;
    let temp = tempfile::tempdir()?;
    let logs = temp.path().join("logs");
    file(&logs.join("route--0/qlog"), 128)?;
    let persist = temp.path().join("persist");
    let keys = crate::uploader_reference_check::http::Keys::new()?;
    keys.write(&persist, "id_rsa")?;
    let peer = Peer::new(stage, mode)?;
    let input = json!({"root":logs,"steps":1,"http":{"api_host":peer.url,"persist":persist,"version":root.join("openpilot/common/version.h")}});
    let mut command =
        CommandSpec::new_os(vec![trace.as_os_str().to_owned()], Duration::from_secs(25));
    let mut bytes = serde_json::to_vec(&input)?;
    bytes.push(b'\n');
    command.input = Some(bytes);
    command.cwd = Some(root.into());
    let start = Instant::now();
    let captured = NativeRunner.run(&command)?;
    let elapsed = start.elapsed().as_secs_f64();
    fs::write(output.join("stdout"), &captured.stdout)?;
    fs::write(output.join("stderr"), &captured.stderr)?;
    let requests = peer.finish()?;
    fs::write(
        output.join("http.json"),
        serde_json::to_vec_pretty(&requests)?,
    )?;
    require(captured.code == 0, "uploader timeout trace process failed")?;
    let actual: Value = serde_json::from_slice(&captured.stdout)?;
    require(
        actual == expected["result"],
        "native timeout decisions/events differ from immutable original",
    )?;
    let attribute = if marked(&logs.join("route--0/qlog")) {
        json!("31")
    } else {
        Value::Null
    };
    require(
        attribute == expected["attribute"],
        "native timeout xattr differs from original",
    )?;
    // Original request comparison drops only measured arrival time. The raw
    // actual headers/bodies and timing remain in the retained http.json.
    let project = |rows: &[Value]| {
        rows.iter()
            .map(|row| {
                let mut value = json!({"method":row["method"],"path":row["path"]});
                if row["method"] == "PUT" {
                    value["body_hex"] = row["body_hex"].clone();
                }
                value
            })
            .collect::<Vec<_>>()
    };
    require(
        project(&requests)
            == project(
                expected["requests"]
                    .as_array()
                    .ok_or("original timeout requests missing")?,
            ),
        "native timeout HTTP traffic differs from original",
    )?;
    let result = json!({"stage":stage,"mode":mode,"implementation":"rust","elapsed":elapsed,"result":actual,"attribute":attribute,"requests":requests,"original_result_equal":true,"original_http_equal":true});
    fs::write(
        output.join("result.json"),
        serde_json::to_vec_pretty(&result)?,
    )?;
    Ok(result)
}

pub(super) fn run(
    root: &Path,
    trace: &Path,
    output: &Path,
    expected: &[Value],
    late_only: bool,
) -> Result<Value> {
    require(expected.len() == 10, "original timeout roster differs")?;
    let mut selected = Vec::new();
    for row in expected {
        if !late_only || row["mode"] == "late" {
            selected.push(row.clone());
        }
    }
    require(
        selected.len() == if late_only { 4 } else { 10 },
        "selected timeout roster differs",
    )?;
    let mut records = Vec::new();
    for chunk in selected.chunks(8) {
        let mut workers = Vec::new();
        for row in chunk {
            let root = PathBuf::from(root);
            let trace = PathBuf::from(trace);
            let output = output.join(format!(
                "{}-{}-rust",
                row["stage"].as_str().ok_or("stage absent")?,
                row["mode"].as_str().ok_or("mode absent")?
            ));
            let row = row.clone();
            workers.push(thread::spawn(move || {
                scenario(&root, &trace, &output, &row).map_err(|e| e.to_string())
            }));
        }
        for worker in workers {
            records.push(
                worker
                    .join()
                    .map_err(|_| "native timeout scenario panicked")?
                    .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?,
            );
        }
    }
    let result = json!({"passed":true,"scenarios":records.len(),"executions":records.len(),"expected_origin":"frozen actual Python source observations; no fresh source traffic","records":records,"failures":[]});
    fs::create_dir_all(output)?;
    fs::write(
        output.join("report.json"),
        serde_json::to_vec_pretty(&result)?,
    )?;
    Ok(result)
}
