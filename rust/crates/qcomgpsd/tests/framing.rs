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
