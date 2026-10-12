#![cfg(feature = "native")]

#[test]
fn malformed_access_unit_recreates_decoder_before_later_input() {
    let mut decoder = openpilot_encoderd::native::decoder::SoftwareDecoder::new().unwrap();
    assert!(decoder.decode(b"malformed access unit").unwrap().is_none());
    decoder.reset().unwrap();
    assert!(decoder.decode(b"\0\0\0\x01\x09\x10").unwrap().is_none());
    println!("native FFmpeg malformed packets reset without a frame or process failure");
}
