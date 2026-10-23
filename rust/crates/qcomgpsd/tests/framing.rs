use openpilot_qcomgpsd::framing::{crc16, decode, encode, Frames};
#[test]
fn ccitt_and_escaping_match_known_x25_frame() {
    assert_eq!(crc16(b"123456789"), 0x906e);
    let payload = [16, 0x7d, 0x7e, 0x00];
    let encoded = encode(&payload);
    assert!(encoded.windows(2).any(|value| value == [0x7d, 0x5d]));
    assert!(encoded.windows(2).any(|value| value == [0x7d, 0x5e]));
    assert_eq!(decode(&encoded).unwrap(), payload);
}
#[test]
fn fragmented_and_coalesced_frames_retain_order() {
    let mut frames = Frames::default();
    let first = encode(&[16, 1]);
    frames.extend(&first[..2]);
    assert!(frames.next_frame().unwrap().is_none());
    frames.extend(&first[2..]);
    frames.extend(&encode(&[115, 3]));
    assert_eq!(frames.next_frame().unwrap(), Some((16, vec![1])));
    assert_eq!(frames.next_frame().unwrap(), Some((115, vec![3])));
}
#[test]
fn bad_crc_and_empty_messages_terminate_the_frame_parser() {
    let mut corrupt = encode(&[16, 1, 2, 3]);
    corrupt[1] ^= 1;
    assert!(decode(&corrupt).is_err());
    let mut frames = Frames::default();
    frames.extend(&encode(&[]));
    assert!(frames.next_frame().is_err());
    assert!(decode(&[0x7e]).is_err());
}

#[test]
fn oversize_full_payload_api_rejects_after_crc_and_recovers_at_delimiter() {
    // A u16 log extent can describe at most 65535 + three outer-header bytes,
    // plus the opcode. Non-log consumers may accept a prefix of a larger frame,
    // but the full-payload API must never silently return truncated data.
    let mut message = vec![0x7e; 65_540];
    message[0] = 16;
    let encoded = encode(&message);
    let mut frames = Frames::default();
    for chunk in encoded[..encoded.len() - 1].chunks(257) {
        frames.extend(chunk);
        assert!(frames.next_frame().unwrap().is_none());
    }
    frames.extend(&[0x7e]);
    frames.extend(&encode(&[115, 3]));
    assert_eq!(
        frames.next_frame().unwrap_err().to_string(),
        "diagnostic frame exceeds retained payload"
    );
    assert_eq!(frames.next_frame().unwrap(), Some((115, vec![3])));

    let mut corrupt = encoded;
    corrupt[0] ^= 1;
    frames.extend(&corrupt);
    assert_eq!(
        frames.next_frame().unwrap_err().to_string(),
        "HDLC checksum mismatch"
    );
    frames.extend(&encode(&[39, 1]));
    assert_eq!(frames.next_frame().unwrap(), Some((39, vec![1])));
}

#[test]
fn largest_log_survives_worst_case_escaping_and_fragmentation() {
    let mut message = vec![0x7e; 65_539];
    message[0] = 16;
    // Real maximum DIAG_LOG: matching outer/inner extents, unknown log kind.
    message[1..8].copy_from_slice(&[0, 255, 255, 255, 255, 255, 255]);
    let encoded = encode(&message);
    let mut frames = Frames::default();
    // Odd chunks split escape pairs, including those near the retention bound.
    for chunk in encoded[..encoded.len() - 1].chunks(127) {
        frames.extend(chunk);
        assert!(frames.next_frame().unwrap().is_none());
    }
    frames.extend(&[0x7e]);
    let (opcode, body) = frames.next_frame().unwrap().unwrap();
    assert_eq!((opcode, &body[..]), (16, &message[1..]));
    assert_eq!(
        openpilot_qcomgpsd::decode::Log::parse(&body).unwrap().kind,
        0xffff
    );
}

#[test]
fn streaming_escaping_and_errors_match_whole_frame_decoder() {
    // Noncanonical escapes remain literal unless followed by 5d/5e. Include
    // escaped CRCs, dangling escape and empty messages.
    let mut cases = vec![
        vec![],
        vec![0x7e],
        vec![0, 0x7e],
        vec![0x7d, 0x7e],
        vec![0x7d, 0x5e, 0x7e],
        encode(&[]),
    ];
    for byte in 0..=255 {
        cases.push(encode(&[byte, 0x7d, 0x7e]));
        // First 7d is literal; the second pair decodes to 7e.
        let decoded = [byte, 0x7d, 0x7e];
        let mut raw = vec![byte, 0x7d, 0x7d, 0x5e];
        raw.extend(&encode(&decoded)[5..]);
        if byte != 0x7e && byte != 0x7d {
            cases.push(raw);
        }
    }
    for frame in cases.into_iter().filter(|frame| !frame.is_empty()) {
        // Use each byte boundary, exercising escaped data and checksums.
        let mut frames = Frames::default();
        for byte in &frame {
            frames.extend(&[*byte]);
        }
        let expected = decode(&frame).and_then(|mut payload| {
            if payload.is_empty() {
                Err(openpilot_qcomgpsd::Error::Protocol(
                    "empty diagnostic message",
                ))
            } else {
                let opcode = payload.remove(0);
                Ok(Some((opcode, payload)))
            }
        });
        assert_eq!(
            frames.next_frame().map_err(|e| e.to_string()),
            expected.map_err(|e| e.to_string()),
            "frame {frame:?}"
        );
    }
}
