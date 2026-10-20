//! Atomic livePose cursor gate with selected child-only FD mapping for paramsd.
use command_fds::{CommandFdExt, FdMapping};
use rustix::{
    event::{poll, PollFd, PollFlags, Timespec},
    pipe::{pipe_with, PipeFlags},
};
use std::{
    fs::{self, File},
    io::{Read, Write},
    os::fd::{AsFd, AsRawFd, BorrowedFd},
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Report {
    pub completed_waits_before_input: u32,
    pub input_queued_before_release: bool,
    pub later_wait_returns: &'static str,
    pub batches_queued_before_release: u32,
    pub exact_committed_pose_cursor: bool,
}

impl Default for Report {
    fn default() -> Self {
        Self {
            completed_waits_before_input: 1,
            input_queued_before_release: false,
            later_wait_returns: "diagnostic only; not an inferred empty-update count",
            batches_queued_before_release: 0,
            exact_committed_pose_cursor: false,
        }
    }
}

pub struct StartupGate {
    queue: PathBuf,
    output: PathBuf,
    ready_read: File,
    ready_write: File,
    release_read: File,
    release_write: File,
    report: Report,
}

impl StartupGate {
    pub fn new(queue: &Path, output: &Path) -> Result<Self> {
        // Both ends remain CLOEXEC; attach maps only owned clones into the
        // selected child without making them process-wide inheritable.
        let (ready_read, ready_write) = pipe_with(PipeFlags::CLOEXEC)?;
        let (release_read, release_write) = pipe_with(PipeFlags::CLOEXEC)?;
        Ok(Self {
            queue: queue.to_owned(),
            output: output.to_owned(),
            ready_read: File::from(ready_read),
            ready_write: File::from(ready_write),
            release_read: File::from(release_read),
            release_write: File::from(release_write),
            report: Report::default(),
        })
    }

    pub fn ready_writer(&self) -> BorrowedFd<'_> {
        self.ready_write.as_fd()
    }

    pub fn release_reader(&self) -> BorrowedFd<'_> {
        self.release_read.as_fd()
    }

    pub fn report(&self) -> &Report {
        &self.report
    }

    /// Pass only the held gate ends to this child, preserving the C hook's
    /// separate ready-write and release-read descriptor contract. The mapping
    /// is scoped to `command.spawn()`; all original descriptors stay CLOEXEC.
    pub fn attach(
        &self,
        command: &mut Command,
        library: &Path,
        position: &Path,
        trace: &Path,
    ) -> Result<()> {
        command.fd_mappings(vec![
            FdMapping {
                parent_fd: self.ready_write.try_clone()?.into(),
                child_fd: self.ready_write.as_raw_fd(),
            },
            FdMapping {
                parent_fd: self.release_read.try_clone()?.into(),
                child_fd: self.release_read.as_raw_fd(),
            },
        ])?;
        command
            .env("LD_PRELOAD", library)
            .env("PARAMSD_PHASE_TRACE", trace)
            .env("PARAMSD_PHASE_POSITION", position)
            .env(
                "PARAMSD_PHASE_READY_FD",
                self.ready_write.as_raw_fd().to_string(),
            )
            .env(
                "PARAMSD_PHASE_RELEASE_FD",
                self.release_read.as_raw_fd().to_string(),
            );
        Ok(())
    }

    pub fn wait_batch(&mut self) -> Result<()> {
        let mut fds = [PollFd::new(&self.ready_read, PollFlags::IN)];
        let timeout = Timespec {
            tv_sec: 10,
            tv_nsec: 0,
        };
        if poll(&mut fds, Some(&timeout))? == 0 {
            return Err("native input-wait gate missing".into());
        }
        let mut token = [0];
        self.ready_read.read_exact(&mut token)?;
        if token != *b"R" {
            return Err("input gate ended before readiness".into());
        }
        Ok(())
    }

    fn pointer(&self) -> Result<u64> {
        let mut header = [0; 16];
        File::open(&self.queue)?.read_exact(&mut header)?;
        Ok(u64::from_le_bytes(header[8..16].try_into()?))
    }

    pub fn committed_cursor(previous: u64, packet_size: u64, capacity: u64) -> Result<u64> {
        let offset = previous & 0xffff_ffff;
        let mut cycle = previous >> 32;
        let total = packet_size
            .checked_add(15)
            .ok_or("pose packet size overflow")?
            & !7;
        if packet_size == 0
            || packet_size >= (capacity / 3).saturating_sub(16)
            || offset >= capacity
        {
            return Err("invalid pose packet or queue cursor".into());
        }
        let mut next_offset = offset;
        if capacity - offset <= total.checked_add(8).ok_or("pose cursor overflow")? {
            cycle = (cycle + 1) & 0xffff_ffff;
            next_offset = 0;
        }
        Ok((cycle << 32) | (next_offset + total))
    }

    pub fn release(&mut self, previous: u64, packet_size: u64) -> Result<()> {
        let capacity = self
            .queue
            .metadata()?
            .len()
            .checked_sub(123 * 8)
            .ok_or("livePose queue is smaller than its header")?;
        let committed = Self::committed_cursor(previous, packet_size, capacity)?;
        let deadline = Instant::now() + Duration::from_secs(2);
        while self.pointer()? != committed {
            if Instant::now() >= deadline {
                return Err(format!(
                    "livePose batch was not fully committed: previous={previous}, expected={committed}, actual={}",
                    self.pointer()?
                )
                .into());
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        self.report.input_queued_before_release = true;
        self.report.exact_committed_pose_cursor = true;
        self.report.batches_queued_before_release += 1;
        fs::write(
            self.output.join("startup-poll-gate.json"),
            serde_json::to_vec_pretty(&self.report)?,
        )?;
        self.release_write.write_all(b"R")?;
        Ok(())
    }

    pub fn finish(&mut self) -> Result<()> {
        self.release_write.write_all(b"S")?;
        Ok(())
    }
}
