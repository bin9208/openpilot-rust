use openpilot_runtime_core::procfs::{Collector, SamplingClock};
use std::{cell::Cell, fs, num::NonZeroU64, path::Path};

struct Clock {
    now: Cell<u64>,
    cpu: Cell<u64>,
    calls: Cell<u32>,
    cpu_cost: u64,
    wall_cost: u64,
}
impl Clock {
    fn new(now: u64) -> Self {
        Self {
            now: Cell::new(now),
            cpu: Cell::new(0),
            calls: Cell::new(0),
            cpu_cost: 0,
            wall_cost: 0,
        }
    }
    fn advance(&self, collector: &mut Collector) {
        self.now
            .set(self.now.get().max(collector.smaps_next_step_ns()));
        collector.smaps_step(self);
    }
}
impl SamplingClock for Clock {
    fn monotonic_ns(&self) -> u64 {
        self.now.get()
    }
    fn thread_cpu_ns(&self) -> u64 {
        let count = self.calls.get();
        self.calls.set(count + 1);
        if count % 2 == 1 {
            self.now.set(self.now.get() + self.wall_cost);
            self.cpu.set(self.cpu.get() + self.cpu_cost);
        }
        self.cpu.get()
    }
}
fn process(root: &Path, pid: u32, start: u64, data: &[u8]) {
    let dir = root.join(pid.to_string());
    fs::create_dir_all(&dir).unwrap();
    let mut fields = vec!["0".to_owned(); 50];
    fields[0] = "S".into();
    fields[19] = start.to_string();
    fields[21] = "2000".into();
    fs::write(
        dir.join("stat"),
        format!("{pid} (name with ) bracket) {}", fields.join(" ")),
    )
    .unwrap();
    fs::write(dir.join("smaps"), data).unwrap();
}
fn collector(root: &Path) -> Collector {
    Collector::new(
        root,
        NonZeroU64::new(100).unwrap(),
        NonZeroU64::new(4096).unwrap(),
    )
}
const START: u64 = 10_000_000_000;
const PAUSE: u64 = 5_000_000;
const DATA: &[u8] = b"Pss: 10 kB\nPss_Anon: 3 kB\nPss_Shmem: 2 kB\n";

#[test]
fn scan_needs_eof_and_retains_start_timestamp() {
    let root = tempfile::tempdir().unwrap();
    process(root.path(), 1, 123, DATA);
    let mut reader = collector(root.path());
    let clock = Clock::new(START);
    assert_eq!(
        reader.snapshot_at(START).unwrap().processes[0]
            .proportional
            .mono_time_ns,
        0
    );
    reader.smaps_step(&clock);
    assert_eq!(
        reader.snapshot_at(START).unwrap().processes[0]
            .proportional
            .pss,
        0
    );
    reader.smaps_step(&clock); // Same time cannot read EOF.
    assert_eq!(
        reader.snapshot_at(START).unwrap().processes[0]
            .proportional
            .pss,
        0
    );
    clock.advance(&mut reader);
    let snapshot = reader.snapshot_at(clock.now.get()).unwrap();
    let sample = &snapshot.processes[0].proportional;
    assert_eq!(
        (sample.pss, sample.anon, sample.shared, sample.mono_time_ns),
        (10240, 3072, 2048, START)
    );
}

#[test]
fn bounded_chunks_parse_split_lines_and_unterminated_final_line() {
    let root = tempfile::tempdir().unwrap();
    let mut data = vec![b'\n'; 32764];
    data.extend_from_slice(b"Pss: 10 kB\nPss_Anon: 3 kB\nPss_Shmem: 2 kB\nPss: 4 kB");
    process(root.path(), 1, 123, &data);
    let mut reader = collector(root.path());
    let clock = Clock::new(START);
    reader.snapshot_at(START).unwrap();
    for _ in 0..2 {
        clock.advance(&mut reader);
        assert_eq!(
            reader.snapshot_at(clock.now.get()).unwrap().processes[0]
                .proportional
                .pss,
            0
        );
    }
    clock.advance(&mut reader);
    assert_eq!(
        reader.snapshot_at(clock.now.get()).unwrap().processes[0]
            .proportional
            .pss,
        14 * 1024
    );
}

#[test]
fn processes_are_staggered_and_new_processes_are_not_starved() {
    let root = tempfile::tempdir().unwrap();
    for pid in 1..=45 {
        process(root.path(), pid, 123, DATA);
    }
    let mut reader = collector(root.path());
    let clock = Clock::new(START);
    reader.snapshot_at(START).unwrap();
    clock.advance(&mut reader);
    assert!(reader
        .snapshot_at(clock.now.get())
        .unwrap()
        .processes
        .iter()
        .all(|p| p.proportional.pss == 0));
    clock.advance(&mut reader);
    assert_eq!(
        reader
            .snapshot_at(clock.now.get())
            .unwrap()
            .processes
            .iter()
            .filter(|p| p.proportional.pss != 0)
            .count(),
        1
    );
    for _ in 0..88 {
        clock.advance(&mut reader);
    }
    let snapshot = reader.snapshot_at(clock.now.get()).unwrap();
    assert!(snapshot
        .processes
        .iter()
        .all(|p| p.proportional.pss == 10240));
    let stamps: std::collections::HashSet<_> = snapshot
        .processes
        .iter()
        .map(|p| p.proportional.mono_time_ns)
        .collect();
    assert_eq!(stamps.len(), 45);
    assert_eq!(clock.now.get(), START + 89 * PAUSE);
    process(root.path(), 46, 123, DATA);
    reader.snapshot_at(clock.now.get()).unwrap();
    clock.advance(&mut reader);
    clock.advance(&mut reader);
    assert_eq!(
        reader.snapshot_at(clock.now.get()).unwrap().processes[45]
            .proportional
            .pss,
        10240
    );
}

#[test]
fn refresh_uses_completion_time_and_stale_values_have_zero_timestamp() {
    let root = tempfile::tempdir().unwrap();
    process(root.path(), 1, 123, DATA);
    let mut reader = collector(root.path());
    let clock = Clock::new(START);
    reader.snapshot_at(START).unwrap();
    clock.advance(&mut reader);
    clock.advance(&mut reader);
    fs::write(root.path().join("1/smaps"), b"Pss: 99 kB\n").unwrap();
    clock.now.set(START + 40_000_000_000);
    reader.smaps_step(&clock);
    assert_eq!(
        reader.snapshot_at(clock.now.get()).unwrap().processes[0]
            .proportional
            .pss,
        10240
    );
    clock.now.set(START + PAUSE + 40_000_000_000);
    reader.smaps_step(&clock);
    clock.advance(&mut reader);
    let sample = reader.snapshot_at(clock.now.get()).unwrap().processes[0]
        .proportional
        .clone();
    assert_eq!(sample.pss, 99 * 1024);
    assert_eq!(sample.mono_time_ns, START + PAUSE + 40_000_000_000);
    let boundary = sample.mono_time_ns + 120_000_000_000;
    assert_eq!(
        reader.snapshot_at(boundary).unwrap().processes[0]
            .proportional
            .pss,
        99 * 1024
    );
    let expired = reader.snapshot_at(boundary + 1).unwrap();
    assert_eq!(expired.processes[0].proportional, Default::default());
}

#[test]
fn cpu_duty_pause_uses_post_read_wall_time_without_catchup() {
    let root = tempfile::tempdir().unwrap();
    process(root.path(), 1, 123, DATA);
    let mut reader = collector(root.path());
    let clock = Clock {
        cpu_cost: 20_000_000,
        wall_cost: 300_000_000,
        ..Clock::new(START)
    };
    reader.snapshot_at(START).unwrap();
    reader.smaps_step(&clock);
    assert_eq!(
        reader.smaps_next_step_ns(),
        START + 300_000_000 + 20_000_000 * 19
    );
    clock.now.set(START + 400_000_000);
    reader.smaps_step(&clock);
    assert_eq!(
        reader.snapshot_at(clock.now.get()).unwrap().processes[0]
            .proportional
            .pss,
        0
    );
}

#[test]
fn expired_incomplete_scan_releases_other_processes() {
    let root = tempfile::tempdir().unwrap();
    process(root.path(), 1, 123, DATA);
    process(root.path(), 2, 123, DATA);
    let mut reader = collector(root.path());
    let clock = Clock::new(START);
    reader.snapshot_at(START).unwrap();
    clock.advance(&mut reader);
    clock.now.set(START + 121_000_000_000);
    reader.smaps_step(&clock);
    clock.advance(&mut reader);
    clock.advance(&mut reader);
    let snapshot = reader.snapshot_at(clock.now.get()).unwrap();
    assert_eq!(snapshot.processes[0].proportional.pss, 0);
    assert_eq!(snapshot.processes[1].proportional.pss, 10240);
}

#[test]
fn pid_reuse_during_scan_and_after_cache_never_inherits_pss() {
    for stage in ["before_open", "during_scan", "after_cache"] {
        let root = tempfile::tempdir().unwrap();
        process(root.path(), 1, 123, DATA);
        let mut reader = collector(root.path());
        let clock = Clock::new(START);
        reader.snapshot_at(START).unwrap();
        if stage != "before_open" {
            clock.advance(&mut reader);
        }
        if stage == "after_cache" {
            clock.advance(&mut reader);
        }
        process(root.path(), 1, 456, DATA);
        if stage == "during_scan" {
            clock.advance(&mut reader);
        } // EOF identity check without eligibility refresh.
        let snapshot = reader.snapshot_at(clock.now.get()).unwrap();
        assert_eq!(snapshot.processes[0].proportional, Default::default());
    }
}

#[test]
fn malformed_or_failed_rescan_keeps_only_fresh_completed_cache() {
    for data in [b"Pss: bad kB\n".as_slice(), b"Pss:\n", &vec![b'x'; 98304]] {
        let root = tempfile::tempdir().unwrap();
        process(root.path(), 1, 123, DATA);
        let mut reader = collector(root.path());
        let clock = Clock::new(START);
        reader.snapshot_at(START).unwrap();
        clock.advance(&mut reader);
        clock.advance(&mut reader);
        fs::write(root.path().join("1/smaps"), data).unwrap();
        clock.now.set(START + PAUSE + 40_000_000_000);
        reader.smaps_step(&clock);
        clock.advance(&mut reader);
        assert_eq!(
            reader.snapshot_at(clock.now.get()).unwrap().processes[0]
                .proportional
                .pss,
            10240
        );
        assert_eq!(
            reader
                .snapshot_at(START + 121_000_000_000)
                .unwrap()
                .processes[0]
                .proportional,
            Default::default()
        );
    }
}

#[test]
fn missing_scan_file_does_not_retry_until_due() {
    let root = tempfile::tempdir().unwrap();
    process(root.path(), 1, 123, DATA);
    fs::remove_file(root.path().join("1/smaps")).unwrap();
    let mut reader = collector(root.path());
    let clock = Clock::new(START);
    reader.snapshot_at(START).unwrap();
    reader.smaps_step(&clock);
    fs::write(root.path().join("1/smaps"), DATA).unwrap();
    clock.advance(&mut reader);
    clock.advance(&mut reader);
    assert_eq!(
        reader.snapshot_at(clock.now.get()).unwrap().processes[0].proportional,
        Default::default()
    );
}

#[test]
fn exited_and_small_processes_drop_completed_cache() {
    for small in [false, true] {
        let root = tempfile::tempdir().unwrap();
        process(root.path(), 1, 123, DATA);
        let mut reader = collector(root.path());
        let clock = Clock::new(START);
        reader.snapshot_at(START).unwrap();
        clock.advance(&mut reader);
        clock.advance(&mut reader);
        if small {
            let stat = fs::read_to_string(root.path().join("1/stat")).unwrap();
            let (prefix, fields) = stat.rsplit_once(')').unwrap();
            let mut fields: Vec<_> = fields.split_whitespace().map(str::to_owned).collect();
            fields[21] = "1280".into();
            fs::write(
                root.path().join("1/stat"),
                format!("{prefix}) {}", fields.join(" ")),
            )
            .unwrap();
            assert_eq!(
                reader.snapshot_at(clock.now.get()).unwrap().processes[0].proportional,
                Default::default()
            );
        } else {
            fs::remove_dir_all(root.path().join("1")).unwrap();
            assert!(reader
                .snapshot_at(clock.now.get())
                .unwrap()
                .processes
                .is_empty());
        }
        process(root.path(), 1, 123, DATA);
        assert_eq!(
            reader.snapshot_at(clock.now.get()).unwrap().processes[0].proportional,
            Default::default()
        );
    }
}

#[test]
fn each_process_selects_rollup_or_fallback_independently() {
    let root = tempfile::tempdir().unwrap();
    process(root.path(), 1, 123, DATA);
    process(root.path(), 2, 123, DATA);
    fs::write(
        root.path().join("1/smaps_rollup"),
        b"Pss: 20 kB\nPss_Anon: 8 kB\nPss_Shmem: 7 kB\n",
    )
    .unwrap();
    let mut reader = collector(root.path());
    let clock = Clock::new(START);
    reader.snapshot_at(START).unwrap();
    for _ in 0..4 {
        clock.advance(&mut reader);
    }
    let snapshot = reader.snapshot_at(clock.now.get()).unwrap();
    assert_eq!(snapshot.processes[0].proportional.anon, 8 * 1024);
    assert_eq!(snapshot.processes[0].proportional.shared, 7 * 1024);
    assert_eq!(snapshot.processes[1].proportional.pss, 10 * 1024);
}
