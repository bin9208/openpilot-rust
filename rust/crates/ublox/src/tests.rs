use crate::{parser::Parser, Error};
use serde::Deserialize;

#[derive(Deserialize)]
struct Chunk {
    time: f64,
    bytes: Vec<u8>,
}

#[test]
fn source_checked_binary_frames_and_ephemeris_under_miri() -> Result<(), Error> {
    let chunks: Vec<Chunk> = serde_json::from_slice(include_bytes!("../tests/data/decoder.json"))?;
    let mut parser = Parser::default();
    let mut publications = 0;
    let mut frames = 0;
    for chunk in chunks {
        for frame in parser.framer.add_data(chunk.time, &chunk.bytes) {
            frames += 1;
            if let Ok(Some(packet)) = parser.parse_frame(&frame, 1234567890) {
                let message = capnp::serialize::read_message(
                    packet.bytes.as_slice(),
                    capnp::message::ReaderOptions::new(),
                )?;
                assert!(message
                    .get_root::<openpilot_cereal::log_capnp::event::Reader<'_>>()?
                    .get_valid());
                publications += 1;
            }
        }
    }
    assert_eq!((frames, publications), (57, 17));
    assert_eq!(parser.framer.buffer, b"\xb5\x62\x01\x07\xff\xffincomplete");
    Ok(())
}
