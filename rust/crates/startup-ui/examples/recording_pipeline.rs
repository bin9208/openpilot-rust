//! Owned native encoder fixture plus a real ffmpeg cadence check; never a vehicle.
use openpilot_startup_ui::diagnostics::recording::{Encoding, Recorder};
use std::{
    io::Read,
    path::PathBuf,
    time::{Duration, Instant},
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().collect();
    if args.get(1).is_some_and(|v| v == "--writer")
        || PathBuf::from(&args[0])
            .file_name()
            .is_some_and(|v| v == "ffmpeg")
    {
        let gate = PathBuf::from(std::env::var_os("RECORDING_FIXTURE_GATE").ok_or("gate absent")?);
        let deadline = Instant::now() + Duration::from_secs(10);
        while !gate.exists() {
            if Instant::now() > deadline {
                return Err("writer gate timeout".into());
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        let mut bytes = Vec::new();
        std::io::stdin().read_to_end(&mut bytes)?;
        std::fs::write(args.last().ok_or("output absent")?, bytes.len().to_string())?;
        return Ok(());
    }
    let output = PathBuf::from(args.get(1).ok_or("output path required")?);
    let controlled = std::env::var_os("RECORDING_FIXTURE_GATE").map(PathBuf::from);
    let encoding = Encoding {
        width: 256,
        height: 256,
        fps: 20,
        capture_every: 3,
        speed: 1,
        quality: 23,
        bitrate: String::new(),
        preset: "ultrafast",
        capacity: 2,
    };
    let mut recorder = Recorder::start(&encoding, &output)?;
    let pixels = vec![77; 256 * 256 * 4];
    let deadline = Instant::now() + Duration::from_secs(10);
    if let Some(gate) = controlled {
        let mut captures = 0;
        while captures < 3 {
            if Instant::now() > deadline {
                return Err("writer did not dequeue its first frame".into());
            }
            if recorder.can_accept() {
                recorder.submit(pixels.clone())?;
                captures += 1;
            } else {
                std::thread::sleep(Duration::from_millis(2));
            }
        }
        assert!(!recorder.can_accept());
        // The sole producer sees a full queue before GPU readback is requested.
        for _ in 0..20 {
            if recorder.can_accept() {
                return Err("blocked consumer freed capacity".into());
            }
        }
        std::fs::write(gate, b"release")?;
        recorder.close()?;
        assert_eq!(
            std::fs::read_to_string(&output)?.parse::<usize>()?,
            captures * pixels.len()
        );
        assert_eq!(recorder.dropped, 0);
        println!("PASS bounded native writer queue; {captures} captures/drained frames,20 readbacks skipped");
    } else {
        for _ in 0..20 {
            while !recorder.can_accept() {
                if Instant::now() > deadline {
                    return Err("encoder capacity deadline".into());
                }
                std::thread::sleep(Duration::from_millis(2));
            }
            recorder.submit(pixels.clone())?;
        }
        recorder.close()?;
        assert_eq!(recorder.accepted, 20);
        assert_eq!(recorder.dropped, 0);
        println!("PASS20 real encoded inputs at source20/3 capture rate");
    }
    Ok(())
}
