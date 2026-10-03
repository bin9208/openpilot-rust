use openpilot_camera_kernel::DoubleParseError;
use openpilot_camerad::requests::StressPoint;
use openpilot_camerad_runtime::{CameraError, FrameClock, SystemClock};
use serde_json::json;
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    sync::{OnceLock, atomic::{AtomicUsize, Ordering}},
};

static SAMPLES: OnceLock<Vec<(i64, i64)>> = OnceLock::new();
static TRACE: OnceLock<File> = OnceLock::new();
static POSITION: AtomicUsize = AtomicUsize::new(0);

fn sample() -> (i64, i64) {
    let index = POSITION.fetch_add(1, Ordering::Relaxed);
    let Some(value) = SAMPLES.get().and_then(|samples| samples.get(index)).copied() else {
        panic!("owned clock fixture exhausted at {index}");
    };
    let Some(mut trace) = TRACE.get() else {
        panic!("owned trace initialized before clock construction");
    };
    if let Err(error) = writeln!(trace, "{}", json!({"op":"clock", "id":7, "sec":value.0, "nsec":value.1})) {
        panic!("owned trace write: {error}");
    }
    value
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().nth(1).as_deref() == Some("real") {
        let mut clock = SystemClock::default();
        println!("{}", json!({"now":clock.now_ns()}));
        return Ok(());
    }
    let samples = std::env::var("STRESS_CLOCK")?
        .split(',')
        .map(|sample| {
            let (seconds, nanos) = sample.split_once(':').ok_or("seconds:nanoseconds required")?;
            Ok((seconds.parse()?, nanos.parse()?))
        })
        .collect::<Result<Vec<(i64, i64)>, Box<dyn std::error::Error>>>()?;
    SAMPLES.set(samples).map_err(|_| "clock fixture initialized twice")?;
    TRACE.set(OpenOptions::new().create(true).append(true).open(std::env::var("STRESS_TRACE")?)?)
        .map_err(|_| "trace initialized twice")?;
    let mut clock = SystemClock::with_boottime(sample);
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input)?;
    let mut results = Vec::new();
    let mut error = None;
    let mut error_step = None;
    for (index, line) in input.lines().enumerate() {
        let mut fields = line.split_whitespace();
        match fields.next() {
            Some("now") => results.push(json!({"now":clock.now_ns()})),
            Some("stress") => {
                let camera = fields.next().ok_or("camera required")?.parse()?;
                let point = match fields.next().ok_or("stress point required")?.parse::<u8>()? {
                    0 => StressPoint::SkipSof,
                    1 => StressPoint::SyncSleep,
                    2 => StressPoint::IfeWait,
                    3 => StressPoint::BpsWait,
                    _ => return Err("unknown stress point".into()),
                };
                clock.set_camera(camera);
                match clock.stress(point) {
                    Ok(triggered) => results.push(json!({"triggered":triggered})),
                    Err(CameraError::StressValue { source, .. }) => {
                        error = Some(match source {
                            DoubleParseError::InvalidArgument => "invalid_argument",
                            DoubleParseError::OutOfRange => "out_of_range",
                        });
                        error_step = Some(index);
                        break;
                    }
                    Err(other) => return Err(other.into()),
                }
            }
            _ => return Err("unknown fixture action".into()),
        }
    }
    println!("{}", json!({"results":results,"error":error,"error_step":error_step}));
    Ok(())
}
