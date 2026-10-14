use openpilot_params::Params;
use std::{env, error::Error, hint::black_box, io, time::Instant};

fn main() -> Result<(), Box<dyn Error>> {
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    if arguments.len() != 2 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: read_cost VALUE_BYTES ITERATIONS",
        )
        .into());
    }
    let size = arguments[0].parse::<usize>()?;
    let iterations = arguments[1].parse::<u32>()?;
    if size > 1024 * 1024 || iterations == 0 || iterations > 1_000_000 {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "probe limits exceeded").into());
    }
    let root = tempfile::Builder::new()
        .prefix("params-read-cost-")
        .tempdir()?;
    let params = Params::open(root.path(), "d")?;
    let expected = [0, 1, 127, 255]
        .into_iter()
        .cycle()
        .take(size)
        .collect::<Vec<_>>();
    params.put("CarParams", &expected)?;
    let started = Instant::now();
    for _ in 0..iterations {
        let actual = params
            .get("CarParams")?
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "probe value disappeared"))?;
        if black_box(actual.as_slice()) != expected {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "probe value differs").into());
        }
    }
    println!(
        "value_bytes={size} iterations={iterations} elapsed_ns={}",
        started.elapsed().as_nanos()
    );
    Ok(())
}
