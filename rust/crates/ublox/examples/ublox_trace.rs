use openpilot_ublox::{parser::Parser, Error};
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize)]
struct Chunk {
    time: f64,
    bytes: Vec<u8>,
}
fn main() -> Result<(), Error> {
    let chunks: Vec<Chunk> = serde_json::from_reader(std::io::stdin())?;
    let mut parser = Parser::default();
    let mut output = Vec::new();
    for chunk in chunks {
        let frames = parser.framer.add_data(chunk.time, &chunk.bytes);
        let mut results = Vec::new();
        for frame in &frames {
            match parser.parse_frame(frame, 1234567890) {
                Ok(Some(packet)) => {
                    results.push(json!({"packet":packet.bytes,"service":packet.service}))
                }
                Ok(None) => results.push(json!({"skip":true})),
                Err(_) => results.push(json!({"error":true})),
            }
        }
        output.push(json!({"frames":frames,"results":results,"buffer":parser.framer.buffer,"last_log_time":parser.framer.last_log_time}));
    }
    serde_json::to_writer(std::io::stdout(), &output)?;
    Ok(())
}
