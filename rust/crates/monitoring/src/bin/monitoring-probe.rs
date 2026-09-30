use openpilot_monitoring::{DriverMonitoring, Input};
use serde::Deserialize;
use std::{
    error::Error,
    fs::File,
    io::{BufRead, BufReader, BufWriter, Write},
};

#[derive(Deserialize)]
struct Request {
    reset: bool,
    rhd_saved: bool,
    always_on: bool,
    too_distracted: bool,
    valid: bool,
    input: Input,
}
fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 3 {
        return Err("usage: monitoring-probe INPUT.jsonl STATE.jsonl PACKETS.bin".into());
    }
    let input = BufReader::new(File::open(&args[0])?);
    let mut state = BufWriter::new(File::create(&args[1])?);
    let mut packets = BufWriter::new(File::create(&args[2])?);
    let mut dm = DriverMonitoring::new(false, false, false);
    for (index, line) in input.lines().enumerate() {
        let request: Request = serde_json::from_str(&line?)?;
        if request.reset {
            dm =
                DriverMonitoring::new(request.rhd_saved, request.always_on, request.too_distracted);
        }
        dm.run_step(&request.input);
        serde_json::to_writer(&mut state, &dm)?;
        state.write_all(b"\n")?;
        packets.write_all(&dm.state_packet(request.valid, u64::try_from(index)?)?)?;
    }
    state.flush()?;
    packets.flush()?;
    Ok(())
}
