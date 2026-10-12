use openpilot_cluster::{
    h264::{Config, Control, Output, Software},
    usb::H264Chunk,
};
use std::{
    io,
    rc::Rc,
    sync::{Arc, Mutex},
};

#[derive(Default)]
struct Receipt {
    events: Vec<String>,
    bytes: Vec<u8>,
}

struct Recipient {
    confined: Rc<()>,
    receipt: Arc<Mutex<Receipt>>,
    fail: bool,
}

impl Output for Recipient {
    fn start(&mut self, requested: i64) -> io::Result<usize> {
        self.receipt
            .lock()
            .unwrap()
            .events
            .push(format!("start:{requested}"));
        Ok(137)
    }
    fn send(&mut self, chunk: H264Chunk<'_>) -> io::Result<()> {
        if self.fail {
            return Err(io::Error::other("owned recipient disconnected"));
        }
        assert!(!chunk.last);
        assert!(chunk.wait_for_ack);
        assert!(!chunk.require_ack);
        assert!(chunk.bytes.len() <= 137);
        assert_eq!(Rc::strong_count(&self.confined), 1);
        self.receipt
            .lock()
            .unwrap()
            .bytes
            .extend_from_slice(chunk.bytes);
        Ok(())
    }
    fn control(&mut self, value: Control) -> io::Result<()> {
        self.receipt
            .lock()
            .unwrap()
            .events
            .push(format!("{value:?}"));
        Ok(())
    }
    fn stop(&mut self) {
        self.receipt.lock().unwrap().events.push("stop".into());
    }
}

fn config() -> Config {
    Config {
        width: 32,
        height: 16,
        fps: 10,
        bitrate: "500k".into(),
        gop: 3,
        ffmpeg: "/usr/bin/ffmpeg".into(),
        encoder: "libx264".into(),
        requested_chunk_size: 137,
        wait_for_ack: true,
        soft_ack: true,
        dump: None,
    }
}

#[test]
fn real_child_outputs_annex_b_and_releases_thread_confined_recipient() {
    let receipt = Arc::new(Mutex::new(Receipt::default()));
    let worker_receipt = Arc::clone(&receipt);
    let mut pipeline = Software::start(config(), move || {
        Ok(Recipient {
            confined: Rc::new(()),
            receipt: worker_receipt,
            fail: false,
        })
    })
    .unwrap();
    let pid = pipeline.encoder_pid().unwrap();
    assert!(std::path::Path::new(&format!("/proc/{pid}")).exists());
    pipeline.control(Control::Brightness(61)).unwrap();
    assert!(pipeline.submit_rgba(&[0; 8], 32, 16).is_err());
    assert!(pipeline.submit_rgba(&[0; 2048], 16, 32).is_err());
    for index in 0..6_u8 {
        let mut rgba = vec![0; 32 * 16 * 4];
        for pixel in rgba.chunks_exact_mut(4) {
            pixel.copy_from_slice(&[index * 32, 75, 190, 255]);
        }
        pipeline.submit_rgba(&rgba, 32, 16).unwrap();
    }
    pipeline.close().unwrap();
    assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
    let receipt = receipt.lock().unwrap();
    assert_eq!(receipt.events, ["start:137", "Brightness(61)", "stop"]);
    assert!(receipt.bytes.windows(4).any(|bytes| bytes == [0, 0, 0, 1]));
    assert!(receipt.bytes.len() > 100);
    assert!(pipeline.submit_rgba(&[0; 2048], 32, 16).is_err());
}

#[test]
fn missing_executable_never_starts_usb_stream() {
    let mut settings = config();
    settings.ffmpeg = "/missing/cluster-ffmpeg".into();
    let result = Software::start(settings, || -> io::Result<Recipient> {
        panic!("recipient must not start before provider resolution")
    });
    assert!(result.is_err());
}

#[test]
fn output_failure_remains_visible_until_explicit_stream_cleanup() {
    let receipt = Arc::new(Mutex::new(Receipt::default()));
    let output_receipt = Arc::clone(&receipt);
    let mut pipeline = Software::start(config(), move || {
        Ok(Recipient {
            confined: Rc::new(()),
            receipt: output_receipt,
            fail: true,
        })
    })
    .unwrap();
    for _ in 0..80 {
        if pipeline.submit_rgba(&[75; 2048], 32, 16).is_err() {
            break;
        }
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while pipeline.snapshot().unwrap().error.is_none() {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(pipeline
        .check_error()
        .unwrap_err()
        .to_string()
        .contains("recipient disconnected"));
    assert_eq!(receipt.lock().unwrap().events, ["start:137"]);
    pipeline.close().unwrap();
    assert_eq!(receipt.lock().unwrap().events, ["start:137", "stop"]);
}

#[test]
fn encoder_exit_includes_actual_provider_stderr() {
    let receipt = Arc::new(Mutex::new(Receipt::default()));
    let mut settings = config();
    settings.encoder = "cluster-missing-encoder".into();
    let mut pipeline = Software::start(settings, move || {
        Ok(Recipient {
            confined: Rc::new(()),
            receipt,
            fail: false,
        })
    })
    .unwrap();
    for _ in 0..80 {
        if pipeline.submit_rgba(&[75; 2048], 32, 16).is_err() {
            break;
        }
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        if pipeline.check_error().is_err() {
            break;
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    pipeline.close().unwrap();
    assert!(pipeline
        .snapshot()
        .unwrap()
        .stderr_tail
        .iter()
        .any(|line| line.contains("cluster-missing-encoder")));
}
