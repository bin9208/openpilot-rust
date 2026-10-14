//! Real diagnostic exchanges over private PTYs; no modem/device access.
use openpilot_qcomgpsd::{framing, serial::Diagnostic, setup, Error};
use serialport::SerialPort;
use std::{
    io::{Read, Write},
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

fn ranges(counts: [u32; 16]) -> Vec<u8> {
    let mut response = vec![0; 3];
    for value in [1, 0].into_iter().chain(counts) {
        response.extend(value.to_le_bytes());
    }
    response
}

fn exchange(opcode: u8, response: Vec<u8>, types: &[u16]) -> (Result<(), Error>, Vec<Vec<u8>>) {
    let (mut master, slave) = serialport::TTYPort::pair().expect("private PTY");
    master.set_timeout(Duration::from_millis(20)).unwrap();
    let mut diagnostic = Diagnostic::open(Path::new(&slave.name().unwrap())).unwrap();
    let done = Arc::new(AtomicBool::new(false));
    let stop = Arc::new(AtomicBool::new(false));
    let peer_done = Arc::clone(&done);
    let peer_stop = Arc::clone(&stop);
    let peer = std::thread::spawn(move || {
        let mut frames = framing::Frames::default();
        let mut commands = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(3);
        while !peer_done.load(Ordering::Relaxed) && Instant::now() < deadline {
            let mut bytes = [0; 2048];
            match master.read(&mut bytes) {
                Ok(n) => frames.extend(&bytes[..n]),
                Err(e) if e.kind() == std::io::ErrorKind::TimedOut => continue,
                other => panic!("PTY read: {other:?}"),
            }
            while let Some((command, payload)) = frames.next_frame().unwrap() {
                assert_eq!(command, framing::LOG_CONFIG);
                let first = commands.is_empty();
                commands.push(payload);
                let reply = if first {
                    [vec![opcode], response.clone()].concat()
                } else {
                    vec![framing::LOG_CONFIG, 0, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0]
                };
                master.write_all(&framing::encode(&reply)).unwrap();
            }
        }
        peer_stop.store(true, Ordering::Relaxed);
        commands
    });
    let result = setup::logs(&mut diagnostic, types, &stop);
    done.store(true, Ordering::Relaxed);
    let commands = peer.join().unwrap();
    (result, commands)
}

#[test]
fn invalid_last_count_prevents_all_mask_commands() {
    let mut counts = [0; 16];
    counts[0] = 1;
    counts[15] = 4097;
    let (result, commands) = exchange(framing::LOG_CONFIG, ranges(counts), &[0, 0xffff]);
    assert!(
        result.is_err(),
        "out-of-range count was accepted; {} mask commands sent",
        commands.len() - 1
    );
    assert_eq!(
        commands,
        [vec![0, 0, 0, 1, 0, 0, 0]],
        "mask sent before full admission"
    );
}

#[test]
fn malformed_range_headers_and_truncation_never_send_masks() {
    let good = ranges([1; 16]);
    for length in 0..good.len() {
        let (result, commands) = exchange(framing::LOG_CONFIG, good[..length].to_vec(), &[]);
        assert!(result.is_err(), "length={length}");
        assert_eq!(commands.len(), 1, "length={length}");
    }
    for offset in [3, 7] {
        let mut bad = good.clone();
        bad[offset] ^= 1;
        let (result, commands) = exchange(framing::LOG_CONFIG, bad, &[]);
        assert!(result.is_err());
        assert_eq!(commands.len(), 1);
    }
    let (result, commands) = exchange(99, good, &[]);
    assert!(result.is_err(), "wrong response opcode accepted");
    assert_eq!(commands.len(), 1);
}

#[test]
fn valid_boundaries_preserve_original_mask_bytes_and_order() {
    // Independent original algorithm: equipment high four bits + item low twelve bits.
    let mut counts = [0; 16];
    counts[0] = 1;
    counts[1] = 9;
    counts[3] = 4095;
    counts[15] = 4096;
    let types = [0, 1, 0x1000, 0x1007, 0x1008, 0x1009, 0x3ffe, 0x3fff, 0xffff];
    let (result, commands) = exchange(framing::LOG_CONFIG, ranges(counts), &types);
    result.unwrap();
    let mut expected = vec![vec![0, 0, 0, 1, 0, 0, 0]];
    for (equipment, count) in counts.into_iter().enumerate().filter(|(_, n)| *n != 0) {
        let mut request = vec![0, 0, 0];
        for value in [3, u32::try_from(equipment).unwrap(), count] {
            request.extend(value.to_le_bytes());
        }
        let mut mask = vec![0_u8; usize::try_from(count.div_ceil(8)).unwrap()];
        for item in 0..count {
            let identifier = (u32::try_from(equipment).unwrap() << 12) | item;
            if types.iter().any(|typ| u32::from(*typ) == identifier) {
                mask[usize::try_from(item / 8).unwrap()] |= 1 << (item % 8);
            }
        }
        request.extend(mask);
        expected.push(request);
    }
    assert_eq!(commands, expected);
    // Captured by executing unchanged original modemdiag.py::setup_logs;
    // provenance hash, input ranges/types and all command bytes are retained.
    let original: serde_json::Value =
        serde_json::from_str(include_str!("data/log_masks_original.json")).unwrap();
    assert_eq!(serde_json::to_value(counts).unwrap(), original["counts"]);
    assert_eq!(serde_json::to_value(types).unwrap(), original["types"]);
    assert_eq!(
        serde_json::to_value(commands).unwrap(),
        original["commands"]
    );
}

#[test]
fn empty_ranges_emit_only_query() {
    let (result, commands) = exchange(framing::LOG_CONFIG, ranges([0; 16]), &framing::LOG_TYPES);
    result.unwrap();
    assert_eq!(commands, [vec![0, 0, 0, 1, 0, 0, 0]]);
}

#[test]
fn every_equipment_range_rejects_unrepresentable_counts_before_writing_masks() {
    for equipment in 0..16 {
        for invalid in [4097, u32::MAX] {
            let mut counts = [1; 16];
            counts[equipment] = invalid;
            let (result, commands) = exchange(framing::LOG_CONFIG, ranges(counts), &[]);
            assert!(result.is_err(), "equipment={equipment}, count={invalid}");
            assert_eq!(commands.len(), 1, "equipment={equipment}, count={invalid}");
        }
    }
}
