use super::super::decoder::DecodedFrame;
use super::*;
use serde_json::json;
use std::{sync::mpsc, time::Instant};
struct Probe {
    trace: Arc<Mutex<Vec<String>>>,
    entered: mpsc::SyncSender<()>,
    release: Arc<Mutex<mpsc::Receiver<()>>>,
}
impl Decoder for Probe {
    fn decode(
        &mut self,
        packet: &[u8],
        sequence: u64,
        width: i64,
        height: i64,
    ) -> Result<Option<DecodedFrame>, String> {
        self.trace
            .lock()
            .unwrap()
            .push(format!("decode:{sequence}:{width}:{height}:{packet:?}"));
        if sequence == 0 {
            self.entered.send(()).unwrap();
            self.release.lock().unwrap().recv().unwrap();
        }
        if sequence == 99 {
            return Err("controlled decode failure".into());
        }
        Ok(Some(DecodedFrame {
            width,
            height,
            planes: None,
            strides: None,
            hardware: None,
            sequence: None,
        }))
    }
    fn reset(&mut self) -> Result<(), String> {
        self.trace.lock().unwrap().push("reset".into());
        Ok(())
    }
}
impl Drop for Probe {
    fn drop(&mut self) {
        self.trace.lock().unwrap().push("close".into());
    }
}
fn request(key: &str, sequence: u64, epoch: u64, keyframe: bool, width: i64) -> Request {
    Request {
        key: key.into(),
        sequence,
        epoch,
        keyframe,
        width,
        height: 2,
        payload: vec![u8::try_from(sequence).unwrap()],
        config: vec![7],
        reset: false,
    }
}
fn wait(mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while !ready() {
        assert!(
            Instant::now() < deadline,
            "controlled decoder event deadline"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn results(worker: &mut DecodeWorker, count: usize) -> Vec<serde_json::Value> {
    wait(|| worker.shared.state.lock().unwrap().results.len() == count);
    worker
        .poll()
        .into_iter()
        .map(|result| json!([result.frame.key, result.frame.sequence, result.epoch]))
        .collect()
}
#[test]
fn actual_worker_overflow_keyframe_epoch_and_failure_recovery() {
    let trace = Arc::new(Mutex::new(Vec::new()));
    let factory_trace = Arc::clone(&trace);
    let (entered, first) = mpsc::sync_channel(1);
    let (resume, release) = mpsc::channel();
    let release = Arc::new(Mutex::new(release));
    let factory: Factory = Arc::new(move || {
        factory_trace.lock().unwrap().push("create".into());
        Ok(Box::new(Probe {
            trace: Arc::clone(&factory_trace),
            entered: entered.clone(),
            release: Arc::clone(&release),
        }))
    });
    let mut worker = DecodeWorker::new(factory);
    worker.submit(request("a", 0, 0, false, 2));
    first.recv_timeout(Duration::from_secs(2)).unwrap();
    for sequence in 1..=60 {
        worker.submit(request("a", sequence, 0, false, 2));
    }
    worker.submit(request("b", 7, 0, false, 2));
    worker.submit(request("a", 61, 0, false, 2));
    worker.submit(request("a", 62, 0, false, 2));
    assert_eq!(worker.dropped_requests(), 62);
    worker.submit(request("a", 80, 0, true, 2));
    resume.send(()).unwrap();
    wait(|| {
        trace
            .lock()
            .unwrap()
            .iter()
            .any(|line| line.starts_with("decode:80:"))
    });
    let initial = results(&mut worker, 2);
    worker.submit(request("a", 90, 1, true, 2));
    let epoch = results(&mut worker, 1);
    worker.submit(request("a", 91, 1, true, 4));
    let changed = results(&mut worker, 1);
    worker.submit(request("a", 99, 1, false, 4));
    wait(|| worker.shared.state.lock().unwrap().waiting.contains("a"));
    worker.submit(request("a", 100, 1, false, 4));
    worker.submit(request("a", 101, 1, true, 4));
    let recovery = results(&mut worker, 1);
    assert_eq!(worker.dropped_requests(), 63);
    assert!(worker.close());
    assert!(worker.close());
    let record = json!({"initial":initial,"epoch":epoch,"changed":changed,"recovery":recovery,"dropped":worker.dropped_requests(),"trace":*trace.lock().unwrap()});
    assert_eq!(record["initial"], json!([["a", 80, 0], ["b", 7, 0]]));
    assert_eq!(record["recovery"], json!([["a", 101, 1]]));
    if let Ok(path) = std::env::var("CLUSTER_MEDIA_STRESS_EVIDENCE") {
        std::fs::write(path, serde_json::to_vec(&record).unwrap()).unwrap();
    }
}
