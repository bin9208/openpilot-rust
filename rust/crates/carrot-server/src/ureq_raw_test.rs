use serde::Serialize;
use std::{
    error::Error,
    io::{Read, Write},
    net::TcpListener,
    path::Path,
    time::Duration,
};

#[derive(Serialize)]
struct Observation {
    scenario: String,
    tracked: bool,
    decoded: bool,
    content_length: Option<u64>,
    raw_content_length: Option<u64>,
    completed_raw_chunk_bytes: Option<u64>,
    bytes: Vec<u8>,
    error_kind: Option<String>,
}

struct ChunkCase {
    name: &'static str,
    wire: &'static [u8],
    expected: &'static [u8],
    completed: u64,
    failed: bool,
}

fn observe(wire: Vec<u8>, tracked: bool, decoded: bool) -> Result<Observation, Box<dyn Error>> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let address = listener.local_addr()?;
    let peer = std::thread::spawn(move || -> std::io::Result<()> {
        let (mut stream, _) = listener.accept()?;
        stream.set_read_timeout(Some(Duration::from_secs(2)))?;
        let mut request = Vec::new();
        while !request.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            stream.read_exact(&mut byte)?;
            request.extend_from_slice(&byte);
        }
        stream.write_all(&wire)
    });
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .proxy(None)
        .timeout_global(Some(Duration::from_secs(2)))
        .build()
        .into();
    let mut response = agent.get(format!("http://{address}/owned")).call()?;
    let content_length = response.body().content_length();
    let raw_content_length = response.body().raw_content_length();
    if tracked {
        assert!(response.body_mut().track_raw_chunks());
    }
    let mut bytes = Vec::new();
    let read = if decoded {
        response.body_mut().as_reader().read_to_end(&mut bytes)
    } else {
        response.body_mut().as_raw_reader().read_to_end(&mut bytes)
    };
    peer.join().map_err(|_| "owned peer panicked")??;
    Ok(Observation {
        scenario: String::new(),
        tracked,
        decoded,
        content_length,
        raw_content_length,
        completed_raw_chunk_bytes: response.body().completed_raw_chunk_bytes(),
        bytes,
        error_kind: read.err().map(|error| format!("{:?}", error.kind())),
    })
}

fn record(name: &str, rows: &[Observation]) -> Result<(), Box<dyn Error>> {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../.omo/evidence/225-heartbeat/provider-controls");
    std::fs::create_dir_all(&directory)?;
    std::fs::write(
        directory.join(format!("{name}.json")),
        serde_json::to_vec_pretty(rows)?,
    )?;
    Ok(())
}

#[test]
fn ureq_opt_in_counts_only_complete_chunk_data() -> Result<(), Box<dyn Error>> {
    let headers = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n";
    let cases = [
        ChunkCase {
            name: "complete",
            wire: b"3\r\nabc\r\n2\r\nxy\r\n0\r\n\r\n",
            expected: b"abcxy",
            completed: 5,
            failed: false,
        },
        ChunkCase {
            name: "partial-later",
            wire: b"3\r\nabc\r\n4\r\nxy",
            expected: b"abcxy",
            completed: 3,
            failed: true,
        },
        ChunkCase {
            name: "missing-crlf",
            wire: b"3\r\nabc",
            expected: b"abc",
            completed: 3,
            failed: true,
        },
    ];
    let mut rows = Vec::new();
    for case in cases {
        for tracked in [false, true] {
            let wire = [headers.as_slice(), case.wire].concat();
            let mut observation = observe(wire, tracked, false)?;
            observation.scenario = case.name.into();
            assert_eq!(observation.bytes, case.expected);
            assert_eq!(observation.error_kind.is_some(), case.failed);
            assert_eq!(observation.content_length, None);
            assert_eq!(observation.raw_content_length, None);
            assert_eq!(
                observation.completed_raw_chunk_bytes,
                tracked.then_some(case.completed)
            );
            rows.push(observation);
        }
    }
    record("chunk-default-and-opt-in", &rows)
}

#[test]
fn ureq_default_readers_preserve_decoding_and_raw_bytes() -> Result<(), Box<dyn Error>> {
    let gzip = [
        31, 139, 8, 0, 0, 0, 0, 0, 2, 3, 203, 47, 207, 75, 77, 1, 0, 45, 83, 180, 59, 5, 0, 0, 0,
    ];
    let mut wire = format!(
        "HTTP/1.1 200 OK\r\nContent-Encoding: gzip\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        gzip.len()
    ).into_bytes();
    wire.extend_from_slice(&gzip);
    let mut rows = Vec::new();
    for decoded in [false, true] {
        let mut observation = observe(wire.clone(), false, decoded)?;
        observation.scenario = "gzip-default".into();
        assert_eq!(observation.content_length, None);
        assert_eq!(
            observation.raw_content_length,
            Some(u64::try_from(gzip.len())?)
        );
        assert_eq!(observation.completed_raw_chunk_bytes, None);
        assert_eq!(
            observation.bytes,
            if decoded {
                b"owned".to_vec()
            } else {
                gzip.to_vec()
            }
        );
        assert_eq!(observation.error_kind, None);
        rows.push(observation);
    }
    record("gzip-default-readers", &rows)
}
