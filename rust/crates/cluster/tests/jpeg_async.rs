use openpilot_cluster::jpeg_async::{Frame, Output, Pipeline};
use std::{
    io,
    sync::{mpsc, Arc},
    time::Duration,
};
struct OwnedOutput {
    started: mpsc::Sender<Vec<u8>>,
    release: mpsc::Receiver<()>,
    fail: bool,
}
impl Output for OwnedOutput {
    fn clear_profile_samples(&mut self) {}
    fn encode_jpeg(&mut self, frame: &Frame) -> io::Result<Vec<u8>> {
        Ok(frame.rgba.to_vec())
    }
    fn send_jpeg(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.started
            .send(bytes.to_vec())
            .map_err(io::Error::other)?;
        self.release.recv().map_err(io::Error::other)?;
        if self.fail {
            Err(io::Error::other("owned recipient rejected JPEG"))
        } else {
            Ok(())
        }
    }
    fn profile_samples(&self) -> Vec<(String, f64)> {
        vec![("owned.sample".into(), 0.5)]
    }
}
fn frame(n: u8) -> Frame {
    Frame {
        rgba: Arc::from(vec![n; 4]),
        width: 1,
        height: 1,
    }
}
#[test]
fn latest_pending_replaces_old_frame_and_wakes_capacity() -> Result<(), Box<dyn std::error::Error>>
{
    let (sent, seen) = mpsc::channel();
    let (release, hold) = mpsc::channel();
    let mut pipeline = Pipeline::start(move || {
        Ok(OwnedOutput {
            started: sent,
            release: hold,
            fail: false,
        })
    })?;
    pipeline.submit_rgba(frame(1))?;
    assert_eq!(seen.recv_timeout(Duration::from_secs(1))?, vec![1; 4]);
    pipeline.submit_rgba(frame(2))?;
    pipeline.submit_rgba(frame(3))?;
    assert!(!pipeline.wait_for_capacity(Some(Duration::ZERO))?);
    release.send(())?;
    assert_eq!(seen.recv_timeout(Duration::from_secs(1))?, vec![3; 4]);
    assert!(pipeline.wait_for_capacity(Some(Duration::from_secs(1)))?);
    release.send(())?;
    assert!(pipeline.close()?);
    let samples = pipeline.profile_samples()?;
    assert_eq!(
        samples
            .iter()
            .filter(|(name, _)| name == "usb_async.encode_jpeg")
            .count(),
        2
    );
    assert_eq!(
        samples
            .iter()
            .filter(|(name, _)| name == "usb_async.send_jpeg")
            .count(),
        2
    );
    assert!(pipeline.profile_samples()?.is_empty());
    Ok(())
}
#[test]
fn failed_owned_recipient_preserves_failure_cause_and_wakes_waiter(
) -> Result<(), Box<dyn std::error::Error>> {
    let (sent, seen) = mpsc::channel();
    let (release, hold) = mpsc::channel();
    let mut pipeline = Pipeline::start(move || {
        Ok(OwnedOutput {
            started: sent,
            release: hold,
            fail: true,
        })
    })?;
    pipeline.submit_rgba(frame(1))?;
    seen.recv_timeout(Duration::from_secs(1))?;
    pipeline.submit_rgba(frame(2))?;
    release.send(())?;
    let error = pipeline
        .wait_for_capacity(Some(Duration::from_secs(1)))
        .unwrap_err();
    assert_eq!(error.to_string(), "asynchronous USB JPEG pipeline failed");
    assert!(std::error::Error::source(&error)
        .unwrap()
        .to_string()
        .contains("owned recipient rejected JPEG"));
    assert!(pipeline.submit_rgba(frame(3)).is_err());
    assert!(pipeline.close()?);
    Ok(())
}

#[test]
fn blocked_output_close_returns_at_source_three_second_bound_then_reaps(
) -> Result<(), Box<dyn std::error::Error>> {
    let (sent, seen) = mpsc::channel();
    let (release, hold) = mpsc::channel();
    let mut pipeline = Pipeline::start(move || {
        Ok(OwnedOutput {
            started: sent,
            release: hold,
            fail: false,
        })
    })?;
    pipeline.submit_rgba(frame(1))?;
    seen.recv_timeout(Duration::from_secs(1))?;
    let start = std::time::Instant::now();
    assert!(!pipeline.close()?);
    assert!((2.9..3.6).contains(&start.elapsed().as_secs_f64()));
    release.send(())?;
    assert!(pipeline.close()?);
    Ok(())
}
