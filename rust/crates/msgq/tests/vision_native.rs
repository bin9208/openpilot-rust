#![cfg(all(feature = "native-skip-miri", not(feature = "visionipc-ion")))]

mod support;

use openpilot_msgq::{VisionClient, VisionLayout, VisionMetadata, VisionServer, VisionStream};
use std::{
    env,
    io::{BufRead, BufReader, Write},
    process::Stdio,
    time::Duration,
};

const STREAMS: [VisionStream; 4] = [
    VisionStream::Road,
    VisionStream::Driver,
    VisionStream::WideRoad,
    VisionStream::Map,
];

fn line(output: &mut impl BufRead, expected: &str) {
    let mut actual = String::new();
    output.read_line(&mut actual).unwrap();
    assert_eq!(actual.trim(), expected);
}

fn ready(output: &mut impl BufRead) {
    let mut message = String::new();
    loop {
        message.clear();
        assert_ne!(
            output.read_line(&mut message).unwrap(),
            0,
            "client exited before its readiness event"
        );
        if message.trim() == "READY" {
            return;
        }
    }
}

fn rust_client() {
    let mut clients: Vec<_> = STREAMS
        .into_iter()
        .map(|stream| {
            let mut client = VisionClient::new("native194", stream, false).unwrap();
            assert!(client.connect().unwrap());
            client
        })
        .collect();
    println!("READY");
    for request in std::io::stdin().lock().lines() {
        let request = request.unwrap();
        if request == "stop" {
            println!("OK");
            return;
        }
        let id: u32 = request.strip_prefix("receive ").unwrap().parse().unwrap();
        for client in &mut clients {
            let frame = client.receive(Duration::from_secs(2)).unwrap().unwrap();
            let metadata = frame.metadata();
            assert_eq!(
                (
                    metadata.frame_id,
                    metadata.timestamp_sof,
                    metadata.timestamp_eof,
                    metadata.valid
                ),
                (
                    id,
                    u64::from(id) * 1000,
                    u64::from(id) * 1000 + 100,
                    id.is_multiple_of(2)
                )
            );
            let descriptor = frame.descriptor().unwrap();
            assert_eq!(
                (
                    descriptor.mmap_len,
                    descriptor.data_len,
                    descriptor.buffer_frame_id
                ),
                (104, 96, u64::from(id))
            );
            let mut bytes = [0; 96];
            frame.copy_into(&mut bytes).unwrap();
            assert_eq!(
                bytes,
                std::array::from_fn(|index| (index as u8).wrapping_add(id as u8))
            );
        }
        println!("OK");
    }
    panic!("missing stop request");
}

#[test]
fn native_server_interoperates_with_original_and_rust_process_clients() {
    if env::var_os("IPC194_RUST_CLIENT").is_some() {
        rust_client();
        return;
    }
    if env::var_os("IPC194_SERVER").is_none() {
        let namespace = tempfile::Builder::new()
            .prefix("msgq_rust-ipc194-")
            .tempdir_in("/dev/shm")
            .unwrap();
        let prefix = namespace
            .path()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .strip_prefix("msgq_")
            .unwrap();
        let status = support::command(env::current_exe().unwrap())
            .args([
                "--exact",
                "native_server_interoperates_with_original_and_rust_process_clients",
                "--nocapture",
            ])
            .env("IPC194_SERVER", "1")
            .env("OPENPILOT_PREFIX", prefix)
            .env_remove("CEREAL_FAKE")
            .status()
            .unwrap();
        assert!(status.success());
        return;
    }
    let before = std::fs::read_dir("/proc/self/fd").unwrap().count();
    let server = VisionServer::new("native194").unwrap();
    let layout = VisionLayout {
        width: 8,
        height: 4,
        stride: 16,
        uv_offset: 64,
        len: 96,
    };
    let images: Vec<_> = STREAMS
        .into_iter()
        .map(|stream| server.create_stream(stream, 4, layout).unwrap())
        .collect();
    server.start_listener().unwrap();
    assert_eq!(
        VisionClient::available_streams("native194").unwrap(),
        STREAMS
    );
    for native in [true, false] {
        let mut command = if native {
            let mut command = support::command(env!("NATIVE_VISION_PEER"));
            command.args(["client", "native194"]);
            command
        } else {
            let mut command = support::command(env::current_exe().unwrap());
            command
                .args([
                    "--exact",
                    "native_server_interoperates_with_original_and_rust_process_clients",
                    "--nocapture",
                    "--quiet",
                ])
                .env("IPC194_RUST_CLIENT", "1");
            command
        };
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut input = child.stdin.take().unwrap();
        let mut output = BufReader::new(child.stdout.take().unwrap());
        ready(&mut output);
        for id in [0_u32, 1, 254, u32::MAX] {
            let bytes: Vec<_> = (0_u8..96)
                .map(|value| value.wrapping_add(id as u8))
                .collect();
            for stream in &images {
                let image = &stream[id as usize % 4];
                image.write(0, &bytes).unwrap();
                image
                    .publish(VisionMetadata {
                        width: 0,
                        height: 0,
                        stride: 0,
                        uv_offset: 0,
                        len: 0,
                        frame_id: id,
                        timestamp_sof: u64::from(id) * 1000,
                        timestamp_eof: u64::from(id) * 1000 + 100,
                        valid: id.is_multiple_of(2),
                        received: false,
                        index: usize::MAX,
                        fd: -1,
                    })
                    .unwrap();
            }
            writeln!(input, "receive {id}").unwrap();
            input.flush().unwrap();
            line(&mut output, "OK");
        }
        writeln!(input, "stop").unwrap();
        input.flush().unwrap();
        line(&mut output, "OK");
        assert!(child.wait().unwrap().success());
    }
    drop(images);
    drop(server);
    assert_eq!(std::fs::read_dir("/proc/self/fd").unwrap().count(), before);
}
