use openpilot_webrtc::{
    cereal, schema,
    video::ipc::{Camera, CameraTrack},
    Error,
};
use std::{
    io::{self, Read, Write},
    time::{Duration, Instant},
};

fn main() -> Result<(), Error> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let mode = args
        .first()
        .ok_or(Error::Contract("missing boundary mode"))?;
    match mode.as_str() {
        "schema" => {
            let names: Vec<_> = args.iter().skip(1).map(String::as_str).collect();
            println!("{}", schema::services(&names)?);
        }
        "camera" => {
            let camera = Camera::parse(args.get(1).ok_or(Error::Contract("missing camera"))?)?;
            let carrot = args.get(2).is_some_and(|value| value == "carrot");
            let mut track = CameraTrack::for_runtime(camera, carrot)?;
            println!("READY");
            io::stdout().flush()?;
            let started = Instant::now();
            loop {
                if let Some(frame) = track.receive()? {
                    println!("{}", serde_json::to_string(&frame)?);
                    break;
                }
                if started.elapsed() > Duration::from_secs(5) {
                    return Err(Error::Contract("owned camera timeout"));
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        "outgoing" => {
            let mut bytes = Vec::new();
            io::stdin().read_to_end(&mut bytes)?;
            println!("{}", cereal::outgoing(&bytes)?);
        }
        "incoming" => {
            let mut text = String::new();
            io::stdin().read_to_string(&mut text)?;
            let (_, bytes) = cereal::incoming(&text, 240_000)?;
            io::stdout().write_all(&bytes)?;
        }
        _ => return Err(Error::Contract("unknown boundary mode")),
    }
    Ok(())
}
