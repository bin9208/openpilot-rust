//! Only a captured kernel wait moves off the camera owner thread.
use std::{
    any::Any,
    io,
    os::fd::{AsFd, BorrowedFd, OwnedFd},
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    sync::mpsc::{self, Receiver, SyncSender},
    thread::{self, JoinHandle},
};

type Task = Box<dyn FnOnce() -> bool + Send>;
type Completion = Result<bool, Box<dyn Any + Send>>;

pub(crate) struct AsyncFrameWait {
    fd: OwnedFd,
    sender: Option<SyncSender<Task>>,
    result: Receiver<Completion>,
    worker: Option<JoinHandle<()>>,
    busy: bool,
}
impl AsyncFrameWait {
    pub fn new() -> io::Result<Self> {
        let fd = rustix::event::eventfd(
            0,
            rustix::event::EventfdFlags::CLOEXEC | rustix::event::EventfdFlags::NONBLOCK,
        )?;
        let wake = fd.try_clone()?;
        let (sender, tasks) = mpsc::sync_channel::<Task>(1);
        let (done, result) = mpsc::sync_channel(1);
        let worker = thread::Builder::new()
            .name("driver-fence".into())
            .spawn(move || {
                while let Ok(task) = tasks.recv() {
                    let result = catch_unwind(AssertUnwindSafe(task));
                    if done.send(result).is_err() {
                        break;
                    }
                    loop {
                        match rustix::io::write(&wake, &1_u64.to_ne_bytes()) {
                            Ok(8) => break,
                            Err(rustix::io::Errno::INTR) => continue,
                            // Without a wake the owner would retain a live fence forever.
                            _ => std::process::abort(),
                        }
                    }
                }
            })?;
        Ok(Self {
            fd,
            sender: Some(sender),
            result,
            worker: Some(worker),
            busy: false,
        })
    }
    pub fn fd(&self) -> BorrowedFd<'_> {
        self.fd.as_fd()
    }
    pub fn busy(&self) -> bool {
        self.busy
    }
    pub fn submit(&mut self, task: impl FnOnce() -> bool + Send + 'static) -> io::Result<()> {
        if self.busy {
            return Err(io::Error::other("camera frame wait already pending"));
        }
        self.sender
            .as_ref()
            .ok_or_else(|| io::Error::other("camera frame wait closed"))?
            .send(Box::new(task))
            .map_err(|_| io::Error::other("camera frame wait worker exited"))?;
        self.busy = true;
        Ok(())
    }
    pub fn take(&mut self) -> io::Result<Option<bool>> {
        let mut bytes = [0; 8];
        loop {
            match rustix::io::read(&self.fd, &mut bytes) {
                Ok(8) => break,
                Err(rustix::io::Errno::AGAIN) => return Ok(None),
                Err(rustix::io::Errno::INTR) => continue,
                Err(error) => return Err(error.into()),
                _ => return Err(io::Error::other("short camera frame wait eventfd read")),
            }
        }
        self.busy = false;
        match self
            .result
            .recv()
            .map_err(|_| io::Error::other("camera frame wait result missing"))?
        {
            Ok(result) => Ok(Some(result)),
            Err(panic) => resume_unwind(panic),
        }
    }
}
impl Drop for AsyncFrameWait {
    fn drop(&mut self) {
        self.sender.take();
        // Camera state/fences outlive the in-flight wait, even on owner failure.
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::mpsc, time::Duration};
    fn completed(worker: &AsyncFrameWait) {
        let device = openpilot_camera_kernel::Device::open("/dev/null").unwrap();
        let (poll, ready) = device.poll_frames(&[Some(worker.fd())], 5000);
        assert_eq!(poll.result.code, 1);
        assert_eq!(ready, [true]);
    }
    #[test]
    fn owner_continues_while_driver_blocks_and_completion_is_consumed_once() {
        let mut worker = AsyncFrameWait::new().unwrap();
        let owner = thread::current().id();
        let (entered, ready) = mpsc::channel();
        let (release, wait) = mpsc::channel();
        worker
            .submit(move || {
                assert_ne!(thread::current().id(), owner);
                entered.send(()).unwrap();
                wait.recv().unwrap();
                true
            })
            .unwrap();
        ready.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(worker.busy());
        assert_eq!(worker.take().unwrap(), None);
        assert!(worker.submit(|| false).is_err());
        // The owner can handle other streams without waiting on the driver.
        let mut road_frames = 0;
        for _ in 0..32 {
            road_frames += 1;
            assert!(worker.take().unwrap().is_none());
        }
        assert_eq!(road_frames, 32);
        release.send(()).unwrap();
        completed(&worker);
        assert_eq!(worker.take().unwrap(), Some(true));
        assert!(!worker.busy());
        assert_eq!(worker.take().unwrap(), None);
        worker.submit(|| false).unwrap();
        completed(&worker);
        assert_eq!(worker.take().unwrap(), Some(false));
    }
    #[test]
    fn one_result_per_request_for_500_reused_waits() {
        let mut worker = AsyncFrameWait::new().unwrap();
        for frame in 0..500 {
            worker.submit(move || frame % 3 != 0).unwrap();
            completed(&worker);
            assert_eq!(worker.take().unwrap(), Some(frame % 3 != 0));
            assert_eq!(worker.take().unwrap(), None);
        }
    }
    #[test]
    fn panic_crosses_to_owner_and_worker_accepts_the_next_wait() {
        let mut worker = AsyncFrameWait::new().unwrap();
        worker.submit(|| panic!("owned fence failure")).unwrap();
        completed(&worker);
        let error = catch_unwind(AssertUnwindSafe(|| worker.take())).unwrap_err();
        assert_eq!(error.downcast_ref::<&str>(), Some(&"owned fence failure"));
        assert!(!worker.busy());
        worker.submit(|| true).unwrap();
        completed(&worker);
        assert_eq!(worker.take().unwrap(), Some(true));
    }
    #[test]
    fn drop_joins_inflight_wait_before_camera_resources_can_be_destroyed() {
        let mut worker = AsyncFrameWait::new().unwrap();
        let (entered, ready) = mpsc::channel();
        let (release, wait) = mpsc::channel();
        let (dropped, finished) = mpsc::channel();
        worker
            .submit(move || {
                entered.send(()).unwrap();
                wait.recv().unwrap();
                true
            })
            .unwrap();
        ready.recv_timeout(Duration::from_secs(5)).unwrap();
        let owner = thread::spawn(move || {
            drop(worker);
            dropped.send(()).unwrap();
        });
        assert!(finished.recv_timeout(Duration::from_millis(20)).is_err());
        release.send(()).unwrap();
        finished.recv_timeout(Duration::from_secs(5)).unwrap();
        owner.join().unwrap();
    }
}
