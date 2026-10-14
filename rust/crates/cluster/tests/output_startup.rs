use openpilot_cluster::{
    h264::{Config, Control, Output, Software},
    usb::H264Chunk,
};
use std::{
    io,
    sync::{Arc, Mutex},
};
struct OwnedOutput {
    records: Arc<Mutex<Vec<String>>>,
}
impl Output for OwnedOutput {
    fn dimensions(&self) -> Option<[u32; 2]> {
        Some([6, 4])
    }
    fn start(&mut self, _: i64) -> io::Result<usize> {
        self.records.lock().unwrap().push("stream".into());
        Ok(4096)
    }
    fn send(&mut self, chunk: H264Chunk<'_>) -> io::Result<()> {
        self.records
            .lock()
            .unwrap()
            .push(format!("chunk:{}", chunk.bytes.len()));
        Ok(())
    }
    fn control(&mut self, _: Control) -> io::Result<()> {
        Ok(())
    }
    fn stop(&mut self) {
        self.records.lock().unwrap().push("stop".into());
    }
}
fn config() -> Config {
    Config {
        width: 2,
        height: 2,
        fps: 10,
        bitrate: "800k".into(),
        gop: 10,
        ffmpeg: std::env::var_os("FFMPEG_BINARY")
            .unwrap_or_else(|| "/usr/bin/ffmpeg".into())
            .into(),
        encoder: "libx264".into(),
        requested_chunk_size: 4096,
        wait_for_ack: true,
        soft_ack: false,
        dump: None,
    }
}
#[test]
fn opened_worker_reports_dimensions_before_main_configures_actual_encoder() {
    let records = Arc::new(Mutex::new(Vec::new()));
    let owner = Arc::clone(&records);
    let main = Arc::clone(&records);
    let mut pipeline = Software::start_with(
        config(),
        move || {
            owner.lock().unwrap().push("open".into());
            Ok(OwnedOutput { records: owner })
        },
        move |config, dimensions| {
            assert_eq!(main.lock().unwrap().as_slice(), ["open"]);
            main.lock().unwrap().push("configure".into());
            [config.width, config.height] = dimensions.unwrap();
            Ok(())
        },
    )
    .unwrap();
    let pid = pipeline.encoder_pid().unwrap();
    let started = std::time::Instant::now();
    let command = loop {
        let command = std::fs::read(format!("/proc/{pid}/cmdline")).unwrap();
        if !command.is_empty() {
            break command;
        }
        assert!(
            started.elapsed().as_secs_f64() < 1.0,
            "encoder argv unavailable"
        );
        std::thread::yield_now();
    };
    assert!(
        command.windows(3).any(|value| value == b"6x4"),
        "actual encoder command: {command:?}"
    );
    for _ in 0..10 {
        pipeline.submit_rgba(&[128; 96], 6, 4).unwrap();
    }
    pipeline.close().unwrap();
    pipeline.close().unwrap();
    let snapshot = pipeline.snapshot().unwrap();
    assert!(snapshot.bytes_sent > 0);
    assert!(snapshot.error.is_none());
    assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
    let records = records.lock().unwrap();
    assert_eq!(&records[..3], ["open", "configure", "stream"]);
    assert_eq!(records.last().unwrap(), "stop");
    println!(
        "actual geometry6x4 encoder command {command:?}, startup {records:?}, bytes {} and child reaped",
        snapshot.bytes_sent
    );
}
#[cfg(feature = "native-media")]
#[test]
fn failed_native_encoder_initialization_never_negotiates_usb_stream() {
    use openpilot_cluster::h264::{vidc, Hardware};
    let records = Arc::new(Mutex::new(Vec::new()));
    let owner = Arc::clone(&records);
    let main = Arc::clone(&records);
    let error = Hardware::start_with(
        vidc::Config {
            width: 2,
            height: 2,
            device_path: "/dev/null".into(),
            ..vidc::Config::default()
        },
        config(),
        move || {
            owner.lock().unwrap().push("open".into());
            Ok(OwnedOutput { records: owner })
        },
        move |encoder, config, dimensions| {
            assert_eq!(main.lock().unwrap().as_slice(), ["open"]);
            main.lock().unwrap().push("configure".into());
            [config.width, config.height] = dimensions.unwrap();
            encoder.width = i32::try_from(config.width).unwrap();
            encoder.height = i32::try_from(config.height).unwrap();
            Ok(())
        },
    )
    .err()
    .unwrap();
    assert!(error.to_string().contains("os error 25"));
    assert_eq!(records.lock().unwrap().as_slice(), ["open", "configure"]);
    println!("native startup preserves QUERYCAP failure before stream negotiation: {error}");
}
