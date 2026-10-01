use openpilot_athena::{state::Stop, Error};
use std::io::{self, Write};
fn main() -> Result<(), Error> {
    let stop = Stop::default();
    signal_hook::flag::register(signal_hook::consts::SIGTERM, stop.signal_flag())?;
    let context = zmq::Context::new();
    let socket = context
        .socket(zmq::PULL)
        .map_err(openpilot_logging::Error::from)?;
    socket
        .set_rcvtimeo(100)
        .map_err(openpilot_logging::Error::from)?;
    socket
        .bind(&format!(
            "ipc:///tmp/logmessage{}",
            std::env::var("OPENPILOT_PREFIX").unwrap_or_default()
        ))
        .map_err(openpilot_logging::Error::from)?;
    println!("READY");
    io::stdout().flush()?;
    while !stop.requested() {
        match socket.recv_bytes(0) {
            Ok(bytes) => {
                println!("{}", std::str::from_utf8(&bytes[1..])?);
                io::stdout().flush()?;
            }
            Err(zmq::Error::EAGAIN | zmq::Error::EINTR) => {}
            Err(error) => return Err(openpilot_logging::Error::from(error).into()),
        }
    }
    Ok(())
}
