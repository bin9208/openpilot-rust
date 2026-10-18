use super::factory::Spec;
use crate::{
    h264::{Control, Output},
    usb::{Display, H264Chunk, ParamsRuntime},
};
use std::{
    io,
    sync::{mpsc, Arc, Mutex},
    thread::{self, JoinHandle},
    time::Duration,
};

enum Operation {
    Start(i64),
    Chunk {
        bytes: Vec<u8>,
        last: bool,
        wait: bool,
        require: bool,
    },
    Control(Control),
    Stop,
    Close,
}
struct Request {
    operation: Operation,
    response: mpsc::SyncSender<io::Result<usize>>,
}
#[derive(Clone)]
pub(super) struct Remote {
    requests: mpsc::Sender<Request>,
    dimensions: [u32; 2],
    preserve_references: bool,
}
impl Remote {
    fn call(&self, operation: Operation) -> io::Result<usize> {
        let (response, received) = mpsc::sync_channel(1);
        self.requests
            .send(Request {
                operation,
                response,
            })
            .map_err(io::Error::other)?;
        received.recv().map_err(io::Error::other)?
    }
}
impl Output for Remote {
    fn preserve_reference_frames(&self) -> bool {
        self.preserve_references
    }
    fn dimensions(&self) -> Option<[u32; 2]> {
        Some(self.dimensions)
    }
    fn start(&mut self, requested: i64) -> io::Result<usize> {
        self.call(Operation::Start(requested))
    }
    fn send(&mut self, chunk: H264Chunk<'_>) -> io::Result<()> {
        self.call(Operation::Chunk {
            bytes: chunk.bytes.to_vec(),
            last: chunk.last,
            wait: chunk.wait_for_ack,
            require: chunk.require_ack,
        })?;
        Ok(())
    }
    fn control(&mut self, control: Control) -> io::Result<()> {
        self.call(Operation::Control(control))?;
        Ok(())
    }
    fn stop(&mut self) {
        if let Err(error) = self.call(Operation::Stop) {
            eprintln!("H264 retained stream stop: {error}");
        }
    }
}
pub(super) enum Stream {
    Local(Box<Display<ParamsRuntime>>),
    Remote(Remote),
}
impl Output for Stream {
    fn preserve_reference_frames(&self) -> bool {
        match self {
            Self::Local(display) => display.preserve_reference_frames(),
            Self::Remote(remote) => remote.preserve_reference_frames(),
        }
    }
    fn dimensions(&self) -> Option<[u32; 2]> {
        match self {
            Self::Local(display) => Output::dimensions(display.as_ref()),
            Self::Remote(output) => output.dimensions(),
        }
    }
    fn start(&mut self, requested: i64) -> io::Result<usize> {
        match self {
            Self::Local(display) => Output::start(display.as_mut(), requested),
            Self::Remote(output) => output.start(requested),
        }
    }
    fn send(&mut self, chunk: H264Chunk<'_>) -> io::Result<()> {
        match self {
            Self::Local(display) => Output::send(display.as_mut(), chunk),
            Self::Remote(output) => output.send(chunk),
        }
    }
    fn control(&mut self, control: Control) -> io::Result<()> {
        match self {
            Self::Local(display) => Output::control(display.as_mut(), control),
            Self::Remote(output) => output.control(control),
        }
    }
    fn drain_profile_samples(&mut self) -> Vec<(String, f64)> {
        match self {
            Self::Local(display) => Output::drain_profile_samples(display.as_mut()),
            Self::Remote(_) => Vec::new(),
        }
    }
    fn stop(&mut self) {
        match self {
            Self::Local(display) => Output::stop(display.as_mut()),
            Self::Remote(output) => output.stop(),
        }
    }
}
pub(super) fn factory(
    spec: Spec,
    remote: Option<Remote>,
) -> impl FnOnce() -> io::Result<Stream> + Send + 'static {
    move || {
        remote.map_or_else(
            || spec.open().map(|display| Stream::Local(Box::new(display))),
            |remote| Ok(Stream::Remote(remote)),
        )
    }
}
pub(super) struct Transport {
    remote: Remote,
    thread: Option<JoinHandle<()>>,
    finished: mpsc::Receiver<()>,
    samples: Arc<Mutex<Vec<(String, f64)>>>,
}
impl Transport {
    pub fn open(spec: Spec) -> io::Result<Self> {
        let preserve_references = spec.wifi();
        let (requests, queue) = mpsc::channel();
        let (ready, received) = mpsc::sync_channel(1);
        let (done, finished) = mpsc::channel();
        let samples = Arc::new(Mutex::new(Vec::new()));
        let worker_samples = Arc::clone(&samples);
        let thread = thread::Builder::new()
            .name("cluster-h264-usb-owner".into())
            .spawn(move || {
                match spec.open() {
                    Err(error) => {
                        let _ = ready.send(Err(error));
                    }
                    Ok(mut display) => {
                        let _ = ready.send(Ok(display.dimensions()));
                        execute(&mut display, queue, &worker_samples);
                        display.close();
                    }
                }
                let _ = done.send(());
            })?;
        let dimensions = match received
            .recv()
            .map_err(io::Error::other)
            .and_then(|result| result)
        {
            Ok(dimensions) => dimensions,
            Err(error) => {
                drop(requests);
                let _ = thread.join();
                return Err(error);
            }
        };
        Ok(Self {
            remote: Remote {
                requests,
                dimensions,
                preserve_references,
            },
            thread: Some(thread),
            finished,
            samples,
        })
    }
    pub fn remote(&self) -> Remote {
        self.remote.clone()
    }
    pub fn samples(&self) -> io::Result<Vec<(String, f64)>> {
        Ok(std::mem::take(&mut *self.samples.lock().map_err(|_| {
            io::Error::other("H264 retained profiles poisoned")
        })?))
    }
    pub fn close(&mut self) -> io::Result<()> {
        if self.thread.is_none() {
            return Ok(());
        }
        let (response, _received) = mpsc::sync_channel(1);
        let _ = self.remote.requests.send(Request {
            operation: Operation::Close,
            response,
        });
        self.finished
            .recv_timeout(Duration::from_secs(3))
            .map_err(io::Error::other)?;
        self.thread.take().map_or(Ok(()), |thread| {
            thread
                .join()
                .map_err(|_| io::Error::other("H264 retained USB owner panicked"))
        })
    }
}
impl Drop for Transport {
    fn drop(&mut self) {
        if let Err(error) = self.close() {
            eprintln!("H264 retained USB close: {error}");
        }
    }
}
fn execute(
    display: &mut Display<ParamsRuntime>,
    queue: mpsc::Receiver<Request>,
    samples: &Mutex<Vec<(String, f64)>>,
) {
    let mut started = false;
    for request in queue {
        let close = matches!(request.operation, Operation::Close);
        let result = match request.operation {
            Operation::Start(requested) => {
                let result = Output::start(display, requested);
                started |= result.is_ok();
                result
            }
            Operation::Chunk {
                bytes,
                last,
                wait,
                require,
            } => Output::send(
                display,
                H264Chunk {
                    bytes: &bytes,
                    last,
                    wait_for_ack: wait,
                    require_ack: require,
                },
            )
            .map(|()| 0),
            Operation::Control(control) => Output::control(display, control).map(|()| 0),
            Operation::Stop | Operation::Close => {
                if started {
                    Output::stop(display);
                    started = false;
                }
                Ok(0)
            }
        };
        if let Ok(mut destination) = samples.lock() {
            destination.extend(
                display
                    .profile_samples()
                    .iter()
                    .map(|&(name, ms)| (name.into(), ms)),
            );
        }
        display.clear_profile_samples();
        let _ = request.response.send(result);
        if close {
            break;
        }
    }
    if started {
        Output::stop(display);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reference_retention_survives_the_actual_stream_and_remote_adapters() {
        for preserve_references in [false, true] {
            let (requests, _queue) = mpsc::channel();
            let remote = Remote {
                requests,
                dimensions: [1920, 462],
                preserve_references,
            };
            assert_eq!(remote.preserve_reference_frames(), preserve_references);
            assert_eq!(
                Stream::Remote(remote).preserve_reference_frames(),
                preserve_references
            );
        }
    }
}
