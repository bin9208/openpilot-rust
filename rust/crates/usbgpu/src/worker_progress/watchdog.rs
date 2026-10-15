//! Source stall reporting policy, sampled without per-frame file reads.
use super::{seconds, SIZE, STAGES};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

pub struct Watchdog {
    memory: Option<Reader>,
    path: Option<PathBuf>,
    reported: HashSet<u64>,
}
struct Reader(*mut std::ffi::c_void);
impl Reader {
    fn open(path: &Path) -> std::io::Result<Self> {
        let file = std::fs::File::open(path)?;
        if file.metadata()?.len() != SIZE as u64 {
            return Err(std::io::ErrorKind::InvalidData.into());
        }
        // SAFETY: the parent-owned progress file remains exactly 48 bytes until the
        // worker exits; this independent read-only mapping lives on the watchdog thread.
        let ptr = unsafe {
            rustix::mm::mmap(
                std::ptr::null_mut(),
                SIZE,
                rustix::mm::ProtFlags::READ,
                rustix::mm::MapFlags::SHARED,
                &file,
                0,
            )?
        };
        Ok(Self(ptr))
    }
    fn current(&self) -> Option<(usize, u64, f64)> {
        // SAFETY: the page-aligned live mapping contains six atomic u64 slots.
        let words = unsafe { std::slice::from_raw_parts(self.0.cast::<AtomicU64>(), 6) };
        let generation = u64::from_le(words[0].load(Ordering::SeqCst));
        if generation == 0 || !generation.is_multiple_of(2) {
            return None;
        }
        let stage = usize::try_from(u64::from_le(words[1].load(Ordering::SeqCst))).ok()?;
        let frame = u64::from_le(words[2].load(Ordering::SeqCst));
        let started = f64::from_bits(u64::from_le(words[3].load(Ordering::SeqCst)));
        if u64::from_le(words[0].load(Ordering::SeqCst)) != generation || stage >= STAGES.len() {
            return None;
        }
        Some((stage, frame, started))
    }
}
impl Drop for Reader {
    fn drop(&mut self) {
        // SAFETY: Reader solely owns this exact read-only mapping and its original length.
        let _ = unsafe { rustix::mm::munmap(self.0, SIZE) };
    }
}
impl Watchdog {
    pub fn new(path: Option<PathBuf>) -> Self {
        Self {
            memory: path.as_deref().and_then(|path| Reader::open(path).ok()),
            path,
            reported: HashSet::new(),
        }
    }
    fn observe(&mut self, stage: usize, frame: u64, age: f64) -> bool {
        frame > 0
            && matches!(
                STAGES[stage],
                "input_upload" | "warp" | "model_call" | "output_read"
            )
            && age > 0.075
            && self.reported.len() < 3
            && self.reported.insert(frame)
    }
    pub fn tick(&mut self) {
        let Some((stage, frame, started)) = self.memory.as_ref().and_then(Reader::current) else {
            return;
        };
        let age = seconds(rustix::time::ClockId::Monotonic) - started;
        if self.observe(stage, frame, age) {
            let worker = self
                .path
                .as_deref()
                .map(|path| super::snapshot(path, std::process::id(), Path::new("/proc")));
            eprintln!(
                "{}",
                serde_json::json!({"precompiledWorkerStall":{"stage":STAGES[stage],"frame":frame,"age_ms":(age*1000.).round_ties_even()},"nativeWorker":worker})
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_stall_report_is_strict75ms_and_limited_to_three_distinct_frames() {
        let mut watch = Watchdog::new(None);
        for stage in 0..STAGES.len() {
            assert!(!watch.observe(stage, 0, 1.));
            assert!(!watch.observe(stage, 1, 0.075));
        }
        for (frame, stage) in [(1, 6), (2, 7), (3, 8)] {
            assert!(watch.observe(stage, frame, 0.076));
            assert!(!watch.observe(9, frame, 1.));
        }
        assert!(!watch.observe(9, 4, 1.));
        let mut watch = Watchdog::new(None);
        for stage in [0, 1, 2, 3, 4, 5, 10, 11] {
            assert!(!watch.observe(stage, 1, 1.));
        }
    }
    #[test]
    fn readonly_mapping_reads_the_actual_worker_record_without_file_polls() {
        let file = tempfile::NamedTempFile::new().unwrap();
        super::super::initialize(file.path());
        super::super::next_frame();
        super::super::mark("model_call");
        let watch = Watchdog::new(Some(file.path().to_owned()));
        let (stage, frame, _) = watch.memory.as_ref().unwrap().current().unwrap();
        assert_eq!(STAGES[stage], "model_call");
        assert_eq!(frame, 1);
    }
}
