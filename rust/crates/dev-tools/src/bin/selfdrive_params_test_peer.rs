//! Controlled test transport peer. Fixture replies are test-only, never source evidence.
use flate2::read::GzDecoder;
use serde_json::{json, Value};
use std::{
    env, fs,
    io::{self, BufRead, Read, Write},
    path::Path,
    process,
};
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        process::exit(1);
    }
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args = env::args().collect::<Vec<_>>();
    if args.len() != 4 {
        return Err("missing peer arguments".into());
    }
    let mut bytes = Vec::new();
    GzDecoder::new(
        include_bytes!("../../tests/fixtures/selfdrive-params/original.json.gz").as_slice(),
    )
    .read_to_end(&mut bytes)?;
    let corpus: Value = serde_json::from_slice(&bytes)?;
    let expected = corpus["expected"].as_array().ok_or("missing expected")?;
    let directory = Path::new(&args[1]).join("fixture");
    fs::create_dir_all(&directory)?;
    let mode = env::var("SELFDRIVE_PARAMS_PEER_MODE").unwrap_or_default();
    if mode == "wrong-ready" {
        println!("wrong");
        return Ok(());
    }
    if mode == "eof-ready" {
        print!("ready");
        io::stdout().flush()?;
        return Ok(());
    }
    let context = zmq::Context::new();
    let socket = context.socket(zmq::PUSH)?;
    socket.set_linger(100)?;
    socket.connect(&args[3])?;
    let ending = if mode == "cr-lines" { "\r\n" } else { "\n" };
    print!("ready{ending}");
    io::stdout().flush()?;
    for (index, line) in io::stdin().lock().lines().enumerate() {
        let request: Value = serde_json::from_str(&line?)?;
        let want = &expected[index];
        if mode == "bad-json" {
            println!("not json");
            break;
        }
        for message in want["messages"].as_array().unwrap() {
            let mut record =
                json!({"msg":message,"levelnum":30,"level":"WARNING","metadata":"normal"});
            if mode == "warning-float-surrogate" {
                record["levelnum"] = json!(30.0);
            }
            if mode == "missing-levelnum" {
                record.as_object_mut().unwrap().remove("levelnum");
            }
            if mode == "wrong-level" {
                record["level"] = json!("ERROR");
            }
            let mut text = record.to_string();
            if matches!(
                mode.as_str(),
                "warning-float-surrogate" | "raw-surrogate-metadata"
            ) {
                text = text.replace("\"normal\"", "\"\\ud800\"");
            }
            let mut data = if mode == "utf16-packets" {
                let mut bytes = vec![255, 254];
                for unit in text.encode_utf16() {
                    bytes.extend_from_slice(&unit.to_le_bytes());
                }
                bytes
            } else {
                text.into_bytes()
            };
            if mode == "raw-surrogate-metadata" {
                let escaped = b"\\ud800";
                let offset = data
                    .windows(escaped.len())
                    .position(|p| p == escaped)
                    .unwrap();
                data.splice(offset..offset + escaped.len(), [0xed, 0xa0, 0x80]);
            }
            let mut packet = vec![if mode == "wrong-severity" { 31 } else { 30 }];
            packet.extend(data);
            socket.send(packet, 0)?;
        }
        let mut marker = json!({"msg":request["marker"],"levelnum":10,"level":"DEBUG"});
        if mode == "marker-float" {
            marker["levelnum"] = json!(10.0);
        }
        if mode == "marker-bool" {
            marker["levelnum"] = json!(true);
        }
        let mut packet = vec![if mode == "marker-bool" { 1 } else { 10 }];
        packet.extend(marker.to_string().as_bytes());
        socket.send(packet, 0)?;
        let value = if mode == "wrong-value" {
            json!({"value":"wrong"})
        } else {
            want["value"].clone()
        };
        let end = if mode == "final-no-newline" && index == 46 {
            ""
        } else {
            ending
        };
        print!("{value}{end}");
        io::stdout().flush()?;
        if (mode == "final-no-newline" && index == 46)
            || mode == "wrong-value"
            || (matches!(
                mode.as_str(),
                "missing-levelnum" | "wrong-severity" | "wrong-level"
            ) && !want["messages"].as_array().unwrap().is_empty())
        {
            break;
        }
    }
    drop(socket);
    drop(context);
    if mode == "nonzero-exit" {
        process::exit(7);
    }
    Ok(())
}
