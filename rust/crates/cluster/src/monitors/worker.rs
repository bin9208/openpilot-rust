use std::{
    io,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

type Refresh<T> = Box<dyn FnMut() -> io::Result<T> + Send>;

pub struct RefreshWorker<T> {
    name: String,
    interval: f64,
    next: f64,
    value: Arc<Mutex<T>>,
    closed: Arc<AtomicBool>,
    refresh: Option<Refresh<T>>,
    sender: Option<SyncSender<()>>,
    thread: Option<JoinHandle<()>>,
    completion: Option<Receiver<()>>,
}

impl<T: Clone + Send + 'static> RefreshWorker<T> {
    pub fn new(
        name: &str,
        interval: f64,
        initial: T,
        refresh: impl FnMut() -> io::Result<T> + Send + 'static,
    ) -> Self {
        Self {
            name: name.into(),
            interval: interval.max(0.1),
            next: 0.0,
            value: Arc::new(Mutex::new(initial)),
            closed: Arc::new(AtomicBool::new(false)),
            refresh: Some(Box::new(refresh)),
            sender: None,
            thread: None,
            completion: None,
        }
    }

    /// # Errors
    /// Returns an OS thread creation failure; refresh failures retain the cached value.
    pub fn request(&mut self, now: f64) -> io::Result<T> {
        if !self.closed.load(Ordering::Acquire) && now >= self.next {
            self.next = now + self.interval;
            if self.thread.is_none() {
                let (sender, receiver) = mpsc::sync_channel(1);
                let (finished, completion) = mpsc::channel::<()>();
                let shared = Arc::clone(&self.value);
                let closed = Arc::clone(&self.closed);
                let mut refresh = self
                    .refresh
                    .take()
                    .ok_or_else(|| io::Error::other("refresh worker unavailable"))?;
                let name = self.name.clone();
                self.thread = Some(thread::Builder::new().name(name.clone()).spawn(move || {
                    while receiver.recv().is_ok() {
                        if closed.load(Ordering::Acquire) {
                            break;
                        }
                        match refresh() {
                            Ok(value) => {
                                *shared
                                    .lock()
                                    .unwrap_or_else(std::sync::PoisonError::into_inner) = value;
                            }
                            Err(error) => eprintln!("{name} refresh failed: {error}"),
                        }
                    }
                    drop(finished);
                })?);
                self.sender = Some(sender);
                self.completion = Some(completion);
            }
            if let Some(sender) = &self.sender {
                let _ = sender.try_send(());
            }
        }
        Ok(self.snapshot())
    }

    #[must_use]
    pub fn snapshot(&self) -> T {
        self.value
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// # Errors
    /// Returns a worker panic if it exits within the source's one-second shutdown bound.
    pub fn close(&mut self) -> io::Result<()> {
        self.stop()
    }
}

impl<T> RefreshWorker<T> {
    fn stop(&mut self) -> io::Result<()> {
        self.closed.store(true, Ordering::Release);
        self.sender = None;
        if let Some(thread) = self.thread.take() {
            if thread.thread().id() == thread::current().id() {
                self.completion = None;
                return Ok(());
            }
            let timed_out = self.completion.take().is_some_and(|completion| {
                matches!(
                    completion.recv_timeout(Duration::from_secs(1)),
                    Err(mpsc::RecvTimeoutError::Timeout)
                )
            });
            if !timed_out {
                thread
                    .join()
                    .map_err(|_| io::Error::other("refresh worker panicked"))?;
            }
        }
        Ok(())
    }
}

impl<T> Drop for RefreshWorker<T> {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}
