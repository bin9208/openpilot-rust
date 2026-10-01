use openpilot_portaudio::{Error, Stream};
use std::{path::PathBuf, time::Duration};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let library = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or("owned fixture library path required")?;
    let mut stream = Stream::load(
        &library,
        Box::new(|output, _| {
            output.fill(0.25);
            true
        }),
    )?;
    assert!(matches!(
        stream.start(),
        Err(Error::Contract("stream is not open"))
    ));
    assert!(matches!(
        stream.active(),
        Err(Error::Contract("stream is not open"))
    ));
    assert!(matches!(
        stream.device(),
        Err(Error::Contract("stream is not open"))
    ));
    stream.open()?;
    assert_eq!(stream.device()?, 7);
    stream.start()?;
    assert!(stream.active()?);
    std::thread::sleep(Duration::from_millis(100));
    drop(stream);
    println!("pre-open guards and opened stream lifecycle passed");
    Ok(())
}
