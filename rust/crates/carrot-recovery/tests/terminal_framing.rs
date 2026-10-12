use openpilot_carrot_recovery::{
    websocket::{read_message, Sender},
    Value,
};
use std::{
    io::{Cursor, Read},
    net::{TcpListener, TcpStream},
    sync::Arc,
    thread,
    time::Duration,
};

fn pair() -> (Arc<Sender>, TcpStream) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let (server, _) = listener.accept().unwrap();
    (Sender::new(server).unwrap(), client)
}

fn masked(opcode: u8, fin: bool, payload: &[u8]) -> Vec<u8> {
    let mask = [3, 7, 11, 19];
    let mut wire = vec![
        opcode | if fin { 0x80 } else { 0 },
        0x80 | u8::try_from(payload.len()).unwrap(),
    ];
    wire.extend_from_slice(&mask);
    wire.extend(
        payload
            .iter()
            .enumerate()
            .map(|(index, byte)| byte ^ mask[index & 3]),
    );
    wire
}

fn frame(reader: &mut TcpStream) -> (u8, Vec<u8>) {
    let mut header = [0; 2];
    reader.read_exact(&mut header).unwrap();
    assert_eq!(header[0] & 0x80, 0x80);
    assert_eq!(header[1] & 0x80, 0);
    let length = match header[1] {
        126 => {
            let mut bytes = [0; 2];
            reader.read_exact(&mut bytes).unwrap();
            usize::from(u16::from_be_bytes(bytes))
        }
        127 => {
            let mut bytes = [0; 8];
            reader.read_exact(&mut bytes).unwrap();
            usize::try_from(u64::from_be_bytes(bytes)).unwrap()
        }
        length => usize::from(length),
    };
    let mut payload = vec![0; length];
    reader.read_exact(&mut payload).unwrap();
    (header[0] & 15, payload)
}

#[test]
fn fragmented_text_survives_interleaved_ping_and_pong() {
    // Given an owned connected peer and a fragmented masked message.
    let (sender, mut peer) = pair();
    let wire = [
        masked(1, false, b"first"),
        masked(9, true, b"probe"),
        masked(10, true, b"ignored"),
        masked(0, true, b"second"),
    ]
    .concat();
    // When the complete message is read.
    let message = read_message(&mut Cursor::new(wire), &sender).unwrap();
    // Then the ping is answered and only data frames make up the text.
    assert_eq!(message, "firstsecond");
    assert_eq!(frame(&mut peer), (10, b"probe".to_vec()));
    println!(
        "{{\"scenario\":\"fragment-ping-pong\",\"message\":\"firstsecond\",\"pong\":\"probe\"}}"
    );
}

#[test]
fn unmasked_binary_replaces_invalid_utf8_like_source() {
    // Given the source-compatible unmasked binary representation.
    let (sender, _peer) = pair();
    // When the message decoder receives an invalid UTF-8 sequence.
    let message = read_message(&mut Cursor::new([0x82, 3, b'a', 0xff, b'b']), &sender).unwrap();
    // Then a replacement character is retained in the decoded message.
    assert_eq!(message, "a\u{fffd}b");
    println!("{{\"scenario\":\"unmasked-binary-utf8\",\"replacement\":true}}");
}

#[test]
fn frame_lengths_cross_both_wire_encoding_boundaries() {
    // Given payloads on both RFC length encoding boundaries.
    let (sender, mut peer) = pair();
    let worker = thread::spawn(move || {
        for length in [0, 125, 126, 65535, 65536] {
            assert!(sender.send_frame(&vec![b'x'; length], 2));
        }
    });
    // When the peer reads each transmitted frame.
    let lengths: Vec<_> = (0..5)
        .map(|_| {
            let (opcode, bytes) = frame(&mut peer);
            assert_eq!(opcode, 2);
            assert!(bytes.iter().all(|byte| *byte == b'x'));
            bytes.len()
        })
        .collect();
    worker.join().unwrap();
    // Then every payload arrives with exact byte count.
    assert_eq!(lengths, [0, 125, 126, 65535, 65536]);
    println!("{{\"scenario\":\"frame-length-boundaries\",\"lengths\":[0,125,126,65535,65536]}}");
}

#[test]
fn concurrent_senders_keep_frames_atomic() {
    // Given four writers sharing one owned WebSocket connection.
    let (sender, mut peer) = pair();
    let workers: Vec<_> = (0..4)
        .map(|id| {
            let sender = Arc::clone(&sender);
            thread::spawn(move || {
                for _ in 0..16 {
                    assert!(sender.send_json(&Value::object([
                        ("writer", Value::integer(id)),
                        ("data", Value::text(&"x".repeat(512)))
                    ])));
                }
            })
        })
        .collect();
    // When the peer decodes all sixty-four frames.
    let mut counts = [0; 4];
    for _ in 0..64 {
        let (opcode, bytes) = frame(&mut peer);
        assert_eq!(opcode, 1);
        let value = Value::parse(&String::from_utf8(bytes).unwrap()).unwrap();
        let id: usize = value.get("writer").string().unwrap().parse().unwrap();
        counts[id] += 1;
        assert!(value.get("data").text_eq(&"x".repeat(512)));
    }
    for worker in workers {
        worker.join().unwrap();
    }
    // Then frame headers and payloads were never interleaved.
    assert_eq!(counts, [16; 4]);
    println!("{{\"scenario\":\"concurrent-frame-writers\",\"counts\":[16,16,16,16]}}");
}
