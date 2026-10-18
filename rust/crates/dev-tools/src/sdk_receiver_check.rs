//! Native replacement for the original three-encoding HTTP receiver check.
use crate::sdk_receiver::{decode_envelope, Receiver, Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    net::TcpStream,
    path::Path,
    time::Duration,
};
pub fn run(output: &Path) -> Result<()> {
    let bytes = include_bytes!("../tests/fixtures/sdk-receiver/original-http.json");
    if format!("{:x}", Sha256::digest(bytes))
        != "910129d4cbb8786a7eaafde33bbff973c296b9f035057cf9d0e0919dca452a5c"
    {
        return Err("original HTTP wire fixture changed".into());
    }
    let original: Value = serde_json::from_slice(bytes)?;
    let mut receiver = Receiver::bind(Duration::from_secs(5))?;
    let mut rows = Vec::new();
    for row in original["http_cases"]
        .as_array()
        .ok_or("missing HTTP cases")?
    {
        let text = row["wire_hex"].as_str().ok_or("missing wire")?;
        let wire = text
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| Ok(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?))
            .collect::<Result<Vec<_>>>()?;
        let mut client = TcpStream::connect(receiver.address())?;
        client.set_read_timeout(Some(Duration::from_secs(5)))?;
        client.set_write_timeout(Some(Duration::from_secs(5)))?;
        write!(client, "POST /api/1/envelope/ HTTP/1.1\r\n")?;
        for (name, value) in row["headers"].as_object().ok_or("missing headers")? {
            let value = if name == "Host" {
                receiver.address().to_string()
            } else {
                value.as_str().ok_or("invalid header")?.to_owned()
            };
            write!(client, "{name}: {value}\r\n")?;
        }
        client.write_all(b"\r\n")?;
        client.write_all(&wire)?;
        let mut response = Vec::new();
        client.take(4096).read_to_end(&mut response)?;
        if !response.starts_with(b"HTTP/1.0 200 OK\r\n") || !response.ends_with(b"\r\n\r\n{}") {
            return Err("receiver did not acknowledge envelope".into());
        }
        let captured = receiver.receive(Duration::from_secs(5))?;
        let event: Value = serde_json::from_str(&captured.event.to_json()?)?;
        let header: Value = serde_json::from_str(&captured.header.to_json()?)?;
        if event != row["event"]
            || captured.raw != row["raw"]
            || captured.wire != wire
            || captured.content_encoding.as_deref() != row["content_encoding"].as_str()
        {
            return Err("original HTTP event or wire mismatch".into());
        }
        rows.push(json!({"event":event,"raw":captured.raw,"path":captured.path,"header":header,"auth":captured.auth,"wire_hex":text,"content_encoding":captured.content_encoding,"headers":captured.headers}));
    }
    receiver.require_empty()?;
    if decode_envelope(b"{}", Some("unsupported-fixture")).is_ok() {
        return Err("unsupported encoding was accepted".into());
    }
    receiver.close()?;
    let report = json!({"result":"PASS","http_cases":rows,"unsupported_rejected":true});
    fs::write(
        output,
        format!("{}\n", serde_json::to_string_pretty(&report)?),
    )?;
    Ok(())
}
pub fn cli() -> i32 {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let usage = "usage: check-sdk-receiver [-h] output";
    let mut positional = Vec::new();
    let mut options = true;
    let mut invalid = false;
    for arg in &args {
        if options && arg == "--" {
            options = false;
            continue;
        }
        if options
            && matches!(
                arg.to_str(),
                Some("-h" | "--h" | "--he" | "--hel" | "--help")
            )
        {
            println!("{usage}\n\npositional arguments:\n  output\n\noptions:\n  -h, --help  show this help message and exit");
            return 0;
        }
        if options && arg.as_encoded_bytes().starts_with(b"-") && arg != "-" {
            invalid = true;
        }
        positional.push(arg);
    }
    if invalid || positional.len() != 1 {
        eprintln!("{usage}\ncheck-sdk-receiver: error: expected one output path");
        return 2;
    }
    if let Err(error) = run(Path::new(positional[0])) {
        eprintln!("{error}");
        return 1;
    }
    println!("PASS: 3 actual HTTP envelope encodings; unsupported encoding rejected");
    0
}
