use super::{stable_headers, HttpPeer, HttpResponse, OwnedServer, Pin, Result};
use rustix::process::Signal;
use serde_json::{json, Value};
use std::{collections::BTreeMap, ffi::OsString, fs, net::TcpListener, path::Path, time::Duration};

pub struct HttpPairConfig<'a> {
    pub repository: &'a Path,
    pub python: &'a Path,
    pub source_wrapper: &'a Path,
    pub source_server: &'a Path,
    pub candidate: &'a Path,
    pub output: &'a Path,
}

pub struct HttpFrozenConfig<'a> {
    pub repository: &'a Path,
    pub source_server: &'a Path,
    pub candidate: &'a Path,
    pub output: &'a Path,
}

fn port() -> Result<u16> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    Ok(listener.local_addr()?.port())
}

fn normalized(response: &HttpResponse) -> Value {
    let headers: BTreeMap<_, _> = stable_headers(&response.headers).into_iter().collect();
    json!({
        "status": response.status,
        "headers": headers,
        "body": base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &response.body),
        "interim": response.interim,
    })
}

fn record(output: &Path, rows: &[Value]) -> Result<()> {
    fs::write(output, serde_json::to_vec_pretty(rows)?)?;
    Ok(())
}

struct Case<'a> {
    name: &'a str,
    wire: Option<&'a [u8]>,
    method_path: Option<(&'a str, &'a str, &'a [u8])>,
    expected: u16,
}

fn compare(
    case: Case<'_>,
    peers: &[HttpPeer; 2],
    rows: &mut Vec<Value>,
    output: &Path,
) -> Result<()> {
    let replies = peers
        .iter()
        .map(|peer| match (case.wire, case.method_path) {
            (Some(wire), None) => peer.exchange(wire),
            (None, Some((method, path, body))) => peer.request(method, path, body),
            _ => Err("invalid recovery HTTP scenario".into()),
        })
        .collect::<Result<Vec<_>>>()?;
    let original = normalized(&replies[0]);
    let native = normalized(&replies[1]);
    rows.push(json!({"scenario": case.name, "original": original, "native": native}));
    record(output, rows)?;
    if original != native
        || replies
            .iter()
            .any(|response| response.status != case.expected)
    {
        return Err(format!("recovery HTTP pair mismatch: {}", case.name).into());
    }
    Ok(())
}

fn compare_frozen(
    case: Case<'_>,
    peer: &HttpPeer,
    source_rows: &[Value],
    rows: &mut Vec<Value>,
    output: &Path,
) -> Result<()> {
    let response = match (case.wire, case.method_path) {
        (Some(wire), None) => peer.exchange(wire)?,
        (None, Some((method, path, body))) => peer.request(method, path, body)?,
        _ => return Err("invalid frozen recovery HTTP scenario".into()),
    };
    let source = source_rows
        .get(rows.len())
        .ok_or("frozen recovery HTTP row absent")?;
    let native = normalized(&response);
    rows.push(json!({"scenario":case.name,"original":source["original"],"native":native}));
    record(output, rows)?;
    if source["scenario"] != case.name
        || source["original"] != native
        || response.status != case.expected
    {
        return Err(format!("frozen recovery HTTP mismatch: {}", case.name).into());
    }
    Ok(())
}

/// Runs the unchanged Python server implementation beside the candidate on
/// owned loopback listeners. Every expected HTTP result comes from source wire
/// observations, with explicit status assertions from the original controller.
pub fn run_http_pair(config: &HttpPairConfig<'_>) -> Result<()> {
    fs::create_dir(config.output)?;
    let source_pin = Pin::open(config.source_server, 4 * 1024 * 1024)?;
    let wrapper_pin = Pin::open(config.source_wrapper, 1024 * 1024)?;
    let candidate_pin = Pin::open(config.candidate, 256 * 1024 * 1024)?;
    let token_asset = Pin::open(
        &config
            .repository
            .join("openpilot/selfdrive/carrot/web/css/tokens.css"),
        1024 * 1024,
    )?;
    let original_dir = config.output.join("source");
    let native_dir = config.output.join("native");
    fs::create_dir(&original_dir)?;
    fs::create_dir(&native_dir)?;
    let ports = [port()?, port()?];
    if ports[0] == ports[1] {
        return Err("recovery HTTP fixture ports collided".into());
    }
    let original_argv = vec![
        OsString::from(config.python),
        OsString::from("-P"),
        OsString::from(config.source_wrapper),
        OsString::from("--port"),
        OsString::from(ports[0].to_string()),
        OsString::from("--output"),
        OsString::from(&original_dir),
    ];
    let native_argv = vec![
        OsString::from(config.candidate),
        OsString::from("--host"),
        OsString::from("127.0.0.1"),
        OsString::from("--port"),
        OsString::from(ports[1].to_string()),
    ];
    let mut original = OwnedServer::spawn(
        &original_argv,
        config.repository,
        &original_dir.join("process.log"),
        &[],
        &[],
        true,
    )?;
    let mut native = OwnedServer::spawn(
        &native_argv,
        config.repository,
        &native_dir.join("process.log"),
        &[],
        &[],
        false,
    )?;
    let peers = [
        HttpPeer::new(ports[0], Duration::from_secs(2)),
        HttpPeer::new(ports[1], Duration::from_secs(2)),
    ];
    let mut rows = Vec::new();
    let result = (|| -> Result<()> {
        for peer in &peers {
            peer.wait_ready("/api/support_terminal/status", Duration::from_secs(5))?;
        }
        const ROUTES: &[(&str, &str, u16)] = &[
            ("GET", "/", 200),
            ("HEAD", "/", 200),
            ("GET", "/recovery.js", 200),
            ("GET", "/recovery-api.js", 200),
            ("GET", "/css/tokens.css", 200),
            ("HEAD", "/css/tokens.css", 200),
            ("GET", "/missing", 404),
            ("HEAD", "/missing", 404),
            ("GET", "/api/support_terminal/status", 200),
            ("HEAD", "/api/support_terminal/status", 200),
            ("GET", "/download/tmux.log", 404),
        ];
        for &(method, path, expected) in ROUTES {
            compare(
                Case {
                    name: path,
                    wire: None,
                    method_path: Some((method, path, &[])),
                    expected,
                },
                &peers,
                &mut rows,
                &config.output.join("http.json"),
            )?;
        }
        const BAD_REQUESTS: &[(&str, &[u8], u16)] = &[
            ("no-newline-request", &[b'G'; 65_537], 414),
            ("no-newline-header", b"GET / HTTP/1.1\r\nX-Test: ", 431),
            (
                "empty-content-length",
                b"POST /missing HTTP/1.1\r\nContent-Length: \r\nConnection: close\r\n\r\n",
                404,
            ),
            (
                "invalid-content-length",
                b"POST /missing HTTP/1.1\r\nContent-Length: invalid\r\nConnection: close\r\n\r\n",
                400,
            ),
            (
                "signed-content-length",
                b"POST /missing HTTP/1.1\r\nContent-Length: +2\r\nConnection: close\r\n\r\n{}",
                404,
            ),
            (
                "invalid-utf8",
                b"POST /missing HTTP/1.1\r\nContent-Length: 1\r\nConnection: close\r\n\r\n\xff",
                400,
            ),
            (
                "truncated-utf8",
                b"POST /missing HTTP/1.1\r\nContent-Length: 2\r\nConnection: close\r\n\r\n\xe2\x82",
                400,
            ),
            (
                "invalid-json",
                b"POST /missing HTTP/1.1\r\nContent-Length: 1\r\nConnection: close\r\n\r\n{",
                400,
            ),
        ];
        for &(name, wire, expected) in BAD_REQUESTS {
            if name == "no-newline-header" {
                let mut full = wire.to_vec();
                full.extend_from_slice(&[b'a'; 65_537]);
                compare(
                    Case {
                        name,
                        wire: Some(&full),
                        method_path: None,
                        expected,
                    },
                    &peers,
                    &mut rows,
                    &config.output.join("http.json"),
                )?;
                continue;
            }
            compare(
                Case {
                    name,
                    wire: Some(wire),
                    method_path: None,
                    expected,
                },
                &peers,
                &mut rows,
                &config.output.join("http.json"),
            )?;
        }
        for &(name, length, expected) in &[
            ("json-body-at-limit", 65_536_usize, 404),
            ("json-body-over-limit", 65_537_usize, 400),
        ] {
            let mut wire = format!(
                "POST /missing HTTP/1.1\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n"
            )
            .into_bytes();
            if length == 65_536 {
                wire.push(b'"');
                wire.extend_from_slice(&[b'a'; 65_534]);
                wire.push(b'"');
            }
            compare(
                Case {
                    name,
                    wire: Some(&wire),
                    method_path: None,
                    expected,
                },
                &peers,
                &mut rows,
                &config.output.join("http.json"),
            )?;
        }
        let head = b"POST /missing HTTP/1.1\r\nHost: owned\r\nContent-Length: 2\r\nExpect: 100-continue\r\nConnection: close\r\n\r\n";
        let replies = peers
            .iter()
            .map(|peer| peer.exchange_continue(head, b"{}"))
            .collect::<Result<Vec<_>>>()?;
        rows.push(json!({"scenario":"expect-continue", "original":normalized(&replies[0]), "native":normalized(&replies[1])}));
        record(&config.output.join("http.json"), &rows)?;
        if normalized(&replies[0]) != normalized(&replies[1])
            || replies
                .iter()
                .any(|row| row.status != 404 || row.interim != [100])
        {
            return Err("recovery HTTP 100 Continue mismatch".into());
        }
        for &(path, expected) in &[
            ("/api/support_terminal/stop", 200),
            ("/api/support_terminal/commands/missing/approve", 400),
            ("/api/support_terminal/commands/missing/reject", 400),
        ] {
            compare(
                Case {
                    name: path,
                    wire: None,
                    method_path: Some(("POST", path, b"{}")),
                    expected,
                },
                &peers,
                &mut rows,
                &config.output.join("http.json"),
            )?;
        }
        Ok(())
    })();
    let source_stop = original.source_eof();
    let native_stop = native.signal(Signal::TERM);
    let original_exit = original.finish(Duration::from_secs(5));
    let native_exit = native.finish(Duration::from_secs(5));
    fs::write(
        config.output.join("cleanup.json"),
        serde_json::to_vec_pretty(&json!({
            "original_exit": original_exit.as_ref().ok(), "native_exit": native_exit.as_ref().ok(),
            "source_eof": source_stop.is_ok(), "native_term": native_stop.is_ok(),
        }))?,
    )?;
    result?;
    source_stop?;
    native_stop?;
    if original_exit? != 0 || native_exit? != 0 {
        return Err("recovery HTTP server cleanup failed".into());
    }
    source_pin.verify()?;
    wrapper_pin.verify()?;
    candidate_pin.verify()?;
    token_asset.verify()?;
    let original_proof: Value =
        serde_json::from_slice(&fs::read(original_dir.join("source-proof.json"))?)?;
    if original_proof.get("sha256").and_then(Value::as_str) != Some(source_pin.digest())
        || original_proof.get("source").and_then(Value::as_str)
            != config.source_server.canonicalize()?.to_str()
    {
        return Err("unchanged recovery source proof mismatch".into());
    }
    fs::write(
        config.output.join("result.json"),
        serde_json::to_vec_pretty(&json!({
            "pass": true,
            "comparisons": rows.len(),
            "source_sha256": source_pin.digest(),
            "wrapper_sha256": wrapper_pin.digest(),
            "candidate_sha256": candidate_pin.digest(),
            "token_asset_sha256": token_asset.digest(),
            "scope": "bootstrap/static/parser and inactive support routes",
        }))?,
    )?;
    Ok(())
}

struct Standalone {
    binary: std::path::PathBuf,
    repository: std::path::PathBuf,
    pins: Vec<Pin>,
}
fn require_frozen_http_source(source: &Pin) -> Result<()> {
    let fixture = super::original_fixture::OriginalFixture::open()?;
    if source.digest()
        != fixture.original_input_sha256("openpilot/selfdrive/carrot/recovery/server.py")?
    {
        return Err("frozen HTTP original server source pin changed".into());
    }
    Ok(())
}
fn bounded_command(argv: Vec<OsString>, repository: &Path) -> Result<crate::qa_process::Captured> {
    use crate::qa_process::{CommandSpec, NativeRunner, Runner};
    let mut command = CommandSpec::new_os(argv, Duration::from_secs(5));
    command.cwd = Some(repository.to_owned());
    command.os_env = Some(
        [(
            OsString::from("OPENPILOT_ROOT"),
            repository.as_os_str().to_owned(),
        )]
        .into_iter()
        .collect(),
    );
    command.stdout_limit = 1024 * 1024;
    command.stderr_limit = 1024 * 1024;
    NativeRunner.run(&command)
}
fn require_runtime_dependencies(needed: &[String]) -> Result<()> {
    if needed.iter().any(|name| {
        ![
            "libstdc++.so.6",
            "libgcc_s.so.1",
            "libm.so.6",
            "libc.so.6",
            "ld-linux-x86-64.so.2",
        ]
        .contains(&name.as_str())
    }) {
        return Err("standalone recovery requires an unexpected runtime library".into());
    }
    Ok(())
}
fn standalone_package(config: &HttpFrozenConfig<'_>) -> Result<Standalone> {
    let package = config.output.join("minimal-host-package");
    let binary = package.join("bin/openpilot-carrot-recovery");
    let repository = package.join("repository");
    let helper = config.candidate.with_file_name("openpilot-process-child");
    let assets = config.repository.join("openpilot/selfdrive/carrot/web");
    let mut members = vec![
        (config.candidate.to_owned(), binary.clone()),
        (helper, package.join("bin/openpilot-process-child")),
    ];
    fn visit(
        source: &Path,
        target: &Path,
        members: &mut Vec<(std::path::PathBuf, std::path::PathBuf)>,
    ) -> Result<()> {
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            if entry.file_name() == "__pycache__"
                || entry.path().extension().is_some_and(|e| e == "py")
            {
                continue;
            }
            let kind = entry.file_type()?;
            if kind.is_dir() {
                visit(&entry.path(), &target.join(entry.file_name()), members)?;
            } else if kind.is_file() {
                members.push((entry.path(), target.join(entry.file_name())));
            } else {
                return Err("standalone web asset is not a regular file or directory".into());
            }
            if members.len() > 20_000 {
                return Err("standalone asset roster exceeds bound".into());
            }
        }
        Ok(())
    }
    visit(
        &assets,
        &repository.join("openpilot/selfdrive/carrot/web"),
        &mut members,
    )?;
    let mut pins = Vec::new();
    let mut growth = 25 * 1024 * 1024_u64;
    for (source, _) in &members {
        growth = growth
            .checked_add(fs::symlink_metadata(source)?.len())
            .ok_or("standalone size overflow")?;
        if growth > 2 * 1024 * 1024 * 1024 {
            return Err("standalone package exceeds bound".into());
        }
        pins.push(Pin::open(source, 256 * 1024 * 1024)?);
    }
    crate::disk_space::inspect(config.output, growth)?.require()?;
    fs::create_dir(&package)?;
    let mut copied = Vec::new();
    for ((source, target), pin) in members.iter().zip(&pins) {
        fs::create_dir_all(target.parent().ok_or("standalone target parent missing")?)?;
        fs::copy(source, target)?;
        let sealed = Pin::open(target, 256 * 1024 * 1024)?;
        if sealed.digest() != pin.digest() {
            return Err("standalone copy changed input".into());
        }
        pin.verify()?;
        copied.push(sealed);
    }
    pins.extend(copied);
    if package.join("bin/openpilot-carrot-server").try_exists()? {
        return Err("standalone package includes main web server".into());
    }
    let dynamic = bounded_command(
        vec![
            "/usr/bin/readelf".into(),
            "-d".into(),
            binary.as_os_str().to_owned(),
        ],
        &repository,
    )?;
    if dynamic.code != 0 {
        return Err("standalone ELF inspection failed".into());
    }
    let text = std::str::from_utf8(&dynamic.stdout)?;
    let needed = regex::Regex::new(r"\(NEEDED\).*\[(.*?)\]")?
        .captures_iter(text)
        .map(|c| c[1].to_owned())
        .collect::<Vec<_>>();
    require_runtime_dependencies(&needed)?;
    fs::write(
        config.output.join("standalone.json"),
        serde_json::to_vec_pretty(
            &json!({"needed":needed,"estimated_growth":growth,"main_web_absent":true,"python_runtime_files_absent":true,"executed_binary":binary,"executed_repository":repository}),
        )?,
    )?;
    Ok(Standalone {
        binary,
        repository,
        pins,
    })
}
fn standalone_cli(
    config: &HttpFrozenConfig<'_>,
    package: &Standalone,
    active_port: u16,
) -> Result<()> {
    let mut rows = Vec::new();
    for (name, args, success) in [
        ("help", vec!["--help".to_owned()], true),
        (
            "invalid_port",
            vec!["--port".to_owned(), "invalid".to_owned()],
            false,
        ),
        (
            "duplicate_listener",
            vec![
                "--host".to_owned(),
                "127.0.0.1".to_owned(),
                "--port".to_owned(),
                active_port.to_string(),
            ],
            false,
        ),
    ] {
        let mut argv = vec![package.binary.as_os_str().to_owned()];
        argv.extend(args.into_iter().map(OsString::from));
        let child = bounded_command(argv.clone(), &package.repository)?;
        rows.push(json!({"case":name,"argv":argv,"exit":child.code,"stdout_b64":base64::Engine::encode(&base64::engine::general_purpose::STANDARD,&child.stdout),"stderr_b64":base64::Engine::encode(&base64::engine::general_purpose::STANDARD,&child.stderr)}));
        fs::write(
            config.output.join("standalone-cli.json"),
            serde_json::to_vec_pretty(&rows)?,
        )?;
        if (child.code == 0) != success {
            return Err(format!("standalone CLI contract changed: {name}").into());
        }
    }
    Ok(())
}

/// Replay the same 25 owned requests against one native server, checking each
/// complete stable response against independently captured original-only rows.
pub fn run_http_frozen(
    config: &HttpFrozenConfig<'_>,
    load_original: impl Fn(&str) -> Result<Value>,
) -> Result<()> {
    fs::create_dir(config.output)?;
    let source = Pin::open(config.source_server, 4 * 1024 * 1024)?;
    require_frozen_http_source(&source)?;
    let package = standalone_package(config)?;
    let candidate = Pin::open(config.candidate, 256 * 1024 * 1024)?;
    let token_asset = Pin::open(
        &config
            .repository
            .join("openpilot/selfdrive/carrot/web/css/tokens.css"),
        1024 * 1024,
    )?;
    let source_rows = load_original("projected-source/http/http.json")?;
    let source_rows = source_rows
        .as_array()
        .ok_or("frozen HTTP source rows absent")?;
    if source_rows.len() != 25 {
        return Err("frozen HTTP source roster changed".into());
    }
    let directory = config.output.join("native");
    fs::create_dir(&directory)?;
    let port = port()?;
    let argv = [
        OsString::from(&package.binary),
        OsString::from("--host"),
        OsString::from("127.0.0.1"),
        OsString::from("--port"),
        port.to_string().into(),
    ];
    let mut server = OwnedServer::spawn(
        &argv,
        &package.repository,
        &directory.join("process.log"),
        &[],
        &[],
        false,
    )?;
    let peer = HttpPeer::new(port, Duration::from_secs(2));
    let mut rows = Vec::with_capacity(source_rows.len());
    let result = (|| -> Result<()> {
        peer.wait_ready("/api/support_terminal/status", Duration::from_secs(5))?;
        standalone_cli(config, &package, port)?;
        const ROUTES: &[(&str, &str, u16)] = &[
            ("GET", "/", 200),
            ("HEAD", "/", 200),
            ("GET", "/recovery.js", 200),
            ("GET", "/recovery-api.js", 200),
            ("GET", "/css/tokens.css", 200),
            ("HEAD", "/css/tokens.css", 200),
            ("GET", "/missing", 404),
            ("HEAD", "/missing", 404),
            ("GET", "/api/support_terminal/status", 200),
            ("HEAD", "/api/support_terminal/status", 200),
            ("GET", "/download/tmux.log", 404),
        ];
        for &(method, path, expected) in ROUTES {
            compare_frozen(
                Case {
                    name: path,
                    wire: None,
                    method_path: Some((method, path, &[])),
                    expected,
                },
                &peer,
                source_rows,
                &mut rows,
                &config.output.join("http.json"),
            )?;
        }
        const BAD_REQUESTS: &[(&str, &[u8], u16)] = &[
            ("no-newline-request", &[b'G'; 65_537], 414),
            ("no-newline-header", b"GET / HTTP/1.1\r\nX-Test: ", 431),
            (
                "empty-content-length",
                b"POST /missing HTTP/1.1\r\nContent-Length: \r\nConnection: close\r\n\r\n",
                404,
            ),
            (
                "invalid-content-length",
                b"POST /missing HTTP/1.1\r\nContent-Length: invalid\r\nConnection: close\r\n\r\n",
                400,
            ),
            (
                "signed-content-length",
                b"POST /missing HTTP/1.1\r\nContent-Length: +2\r\nConnection: close\r\n\r\n{}",
                404,
            ),
            (
                "invalid-utf8",
                b"POST /missing HTTP/1.1\r\nContent-Length: 1\r\nConnection: close\r\n\r\n\xff",
                400,
            ),
            (
                "truncated-utf8",
                b"POST /missing HTTP/1.1\r\nContent-Length: 2\r\nConnection: close\r\n\r\n\xe2\x82",
                400,
            ),
            (
                "invalid-json",
                b"POST /missing HTTP/1.1\r\nContent-Length: 1\r\nConnection: close\r\n\r\n{",
                400,
            ),
        ];
        for &(name, wire, expected) in BAD_REQUESTS {
            if name == "no-newline-header" {
                let mut full = wire.to_vec();
                full.extend_from_slice(&[b'a'; 65_537]);
                compare_frozen(
                    Case {
                        name,
                        wire: Some(&full),
                        method_path: None,
                        expected,
                    },
                    &peer,
                    source_rows,
                    &mut rows,
                    &config.output.join("http.json"),
                )?;
                continue;
            }
            compare_frozen(
                Case {
                    name,
                    wire: Some(wire),
                    method_path: None,
                    expected,
                },
                &peer,
                source_rows,
                &mut rows,
                &config.output.join("http.json"),
            )?;
        }
        for &(name, length, expected) in &[
            ("json-body-at-limit", 65_536_usize, 404),
            ("json-body-over-limit", 65_537_usize, 400),
        ] {
            let mut wire = format!(
                "POST /missing HTTP/1.1\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n"
            )
            .into_bytes();
            if length == 65_536 {
                wire.push(b'"');
                wire.extend_from_slice(&[b'a'; 65_534]);
                wire.push(b'"');
            }
            compare_frozen(
                Case {
                    name,
                    wire: Some(&wire),
                    method_path: None,
                    expected,
                },
                &peer,
                source_rows,
                &mut rows,
                &config.output.join("http.json"),
            )?;
        }
        let head = b"POST /missing HTTP/1.1\r\nHost: owned\r\nContent-Length: 2\r\nExpect: 100-continue\r\nConnection: close\r\n\r\n";
        let response = peer.exchange_continue(head, b"{}")?;
        let original = source_rows
            .get(rows.len())
            .ok_or("frozen HTTP 100 Continue row absent")?;
        let native = normalized(&response);
        rows.push(
            json!({"scenario":"expect-continue","original":original["original"],"native":native}),
        );
        record(&config.output.join("http.json"), &rows)?;
        if original["scenario"] != "expect-continue"
            || original["original"] != native
            || response.status != 404
            || response.interim != [100]
        {
            return Err("frozen HTTP 100 Continue mismatch".into());
        }
        for &(path, expected) in &[
            ("/api/support_terminal/stop", 200),
            ("/api/support_terminal/commands/missing/approve", 400),
            ("/api/support_terminal/commands/missing/reject", 400),
        ] {
            compare_frozen(
                Case {
                    name: path,
                    wire: None,
                    method_path: Some(("POST", path, b"{}")),
                    expected,
                },
                &peer,
                source_rows,
                &mut rows,
                &config.output.join("http.json"),
            )?;
        }
        if rows.len() != source_rows.len() {
            return Err("frozen HTTP comparison count changed".into());
        }
        Ok(())
    })();
    let stop = server.signal(Signal::TERM);
    let exit = server.finish(Duration::from_secs(5));
    fs::write(
        config.output.join("cleanup.json"),
        serde_json::to_vec_pretty(
            &json!({"native_term":stop.is_ok(),"native_exit":exit.as_ref().ok()}),
        )?,
    )?;
    result?;
    stop?;
    if exit? != 0 {
        return Err("frozen native HTTP server cleanup failed".into());
    }
    source.verify()?;
    candidate.verify()?;
    token_asset.verify()?;
    for pin in &package.pins {
        pin.verify()?;
    }
    fs::write(
        config.output.join("result.json"),
        serde_json::to_vec_pretty(&json!({"pass":true,"comparisons":rows.len(),
            "frozen_original_compared":true,"source_sha256":source.digest(),
            "candidate_sha256":candidate.digest(),"token_asset_sha256":token_asset.digest()}))?,
    )?;
    Ok(())
}

#[cfg(test)]
mod frozen_admission_tests {
    use super::*;
    #[test]
    fn unexpected_runtime_and_changed_original_source_are_rejected() {
        assert!(require_runtime_dependencies(&["libpython3.12.so".into()]).is_err());
        assert!(require_runtime_dependencies(&["libc.so.6".into()]).is_ok());
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("server.py");
        fs::write(&source, b"changed source").unwrap();
        assert!(require_frozen_http_source(&Pin::open(&source, 1024).unwrap()).is_err());
    }
}
