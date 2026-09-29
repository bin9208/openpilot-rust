use openpilot_runtime_core::procfs::Collector;
use std::{fs, num::NonZeroU64, os::unix::fs::symlink, path::Path};
use tempfile::TempDir;

#[test]
fn pid_reuse_during_metadata_read_does_not_publish_mixed_identity() {
    use std::{io::Write, process::Command, thread};
    let root = fixture();
    let cmdline = root.path().join("123/cmdline");
    fs::remove_file(&cmdline).unwrap();
    assert!(Command::new("mkfifo")
        .arg(&cmdline)
        .status()
        .unwrap()
        .success());
    let mut reader = collector(root.path());
    thread::scope(|scope| {
        let snapshot = scope.spawn(move || reader.snapshot().unwrap());
        let mut writer = fs::OpenOptions::new().write(true).open(&cmdline).unwrap();
        fs::write(
            root.path().join("123/stat"),
            stat(123, "worker ) (a", 999, 2000),
        )
        .unwrap();
        writer.write_all(b"replacement\0").unwrap();
        drop(writer);
        assert!(snapshot.join().unwrap().processes.is_empty());
    });
}

fn stat(pid: u32, name: &str, start: u64, rss: i64) -> String {
    let mut fields = vec!["0".to_owned(); 50];
    for (index, value) in [
        (0, "S"),
        (1, "1"),
        (11, "125"),
        (12, "25"),
        (13, "-3"),
        (14, "4"),
        (15, "20"),
        (16, "-5"),
        (17, "3"),
        (20, "16777216"),
        (36, "6"),
    ] {
        fields[index] = value.to_owned();
    }
    fields[19] = start.to_string();
    fields[21] = rss.to_string();
    format!("{pid} ({name}) {}", fields.join(" "))
}

fn fixture() -> TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("stat"),
        "cpu 9 9 9 9 9 9 9\ncpu0 100 2 30 400 5 6 7 8 9 10\ncpu7 200 3 40 500 6 7 8\nintr 44\n",
    )
    .unwrap();
    fs::write(root.path().join("meminfo"), "MemTotal: 128 kB\nMemFree: 16 kB\nMemAvailable: 32 kB\nBuffers: 2 kB\nCached: 4 kB\nActive: 5 kB\nInactive: 6 kB\nShmem: 7 kB\n").unwrap();
    process(
        root.path(),
        123,
        "worker ) (a",
        500,
        2000,
        b"worker\0--test\0\xe1\x90\xff\0",
    );
    root
}

fn process(root: &Path, pid: u32, name: &str, start: u64, rss: i64, args: &[u8]) {
    let dir = root.join(pid.to_string());
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("stat"), stat(pid, name, start, rss)).unwrap();
    fs::write(dir.join("cmdline"), args).unwrap();
    fs::write(
        dir.join("smaps_rollup"),
        "Pss: 50 kB\nPss_Anon: 30 kB\nPss_Shmem: 10 kB\n",
    )
    .unwrap();
    if !dir.join("exe").is_symlink() {
        symlink("/synthetic/bin/worker", dir.join("exe")).unwrap();
    }
}

fn collector(root: &Path) -> Collector {
    Collector::new(
        root,
        NonZeroU64::new(100).unwrap(),
        NonZeroU64::new(4096).unwrap(),
    )
}

#[test]
fn snapshot_preserves_process_identity_counters_and_memory_units() {
    let root = fixture();
    let mut reader = collector(root.path());

    let snapshot = reader.snapshot().unwrap();

    assert_eq!(snapshot.cpu_times.len(), 2);
    assert_eq!(snapshot.cpu_times[1].cpu, 7);
    assert_eq!(snapshot.cpu_times[0].ticks, [100, 2, 30, 400, 5, 6, 7]);
    assert_eq!(snapshot.memory.total, 131072);
    assert_eq!(snapshot.memory.shared, 7168);
    let p = &snapshot.processes[0];
    assert_eq!(p.stat.name, "worker ) (a");
    assert_eq!(p.stat.children_user_ticks, -3);
    assert_eq!(p.stat.nice, -5);
    assert_eq!(p.resident_bytes, 8192000);
    assert_eq!(p.proportional.pss, 51200);
    assert_eq!(p.proportional.anon, 30720);
    assert_eq!(p.proportional.shared, 10240);
    assert_eq!(p.exe, "/synthetic/bin/worker");
    assert_eq!(p.cmdline, ["worker", "--test", "\u{fffd}\u{fffd}"]);
    assert!(snapshot.warnings.is_empty());
}

#[test]
fn disappeared_malformed_and_mismatched_pid_records_are_skipped() {
    let root = fixture();
    for pid in [4, 5, 6] {
        fs::create_dir(root.path().join(pid.to_string())).unwrap();
    }
    fs::write(root.path().join("5/stat"), "5 (bad) S 1").unwrap();
    fs::write(root.path().join("6/stat"), stat(7, "wrong identity", 1, 3)).unwrap();

    let snapshot = collector(root.path()).snapshot().unwrap();

    assert_eq!(
        snapshot
            .processes
            .iter()
            .map(|p| p.stat.pid)
            .collect::<Vec<_>>(),
        [123]
    );
}

#[test]
fn negative_or_overflowing_resident_memory_does_not_wrap() {
    let root = fixture();
    process(root.path(), 4, "negative", 1, -1, b"x");
    process(root.path(), 5, "overflow", 2, i64::MAX, b"x");

    let snapshot = collector(root.path()).snapshot().unwrap();

    assert_eq!(
        snapshot
            .processes
            .iter()
            .map(|p| p.stat.pid)
            .collect::<Vec<_>>(),
        [123]
    );
}

#[test]
fn reused_pid_with_same_name_refreshes_metadata_and_smaps() {
    let root = fixture();
    let mut reader = collector(root.path());
    reader.snapshot().unwrap();
    process(root.path(), 123, "worker ) (a", 900, 2000, b"replacement\0");
    fs::write(root.path().join("123/smaps_rollup"), "Pss: 99 kB\n").unwrap();

    let snapshot = reader.snapshot().unwrap();

    assert_eq!(snapshot.processes[0].stat.start_ticks, 900);
    assert_eq!(snapshot.processes[0].cmdline, ["replacement"]);
    assert_eq!(snapshot.processes[0].proportional.pss, 101376);
}

#[test]
fn smaps_refreshes_on_the_twentieth_subsequent_cycle() {
    let root = fixture();
    let mut reader = collector(root.path());
    reader.snapshot().unwrap();
    fs::write(root.path().join("123/smaps_rollup"), "Pss: 99 kB\n").unwrap();

    let observed: Vec<_> = (0..20)
        .map(|_| reader.snapshot().unwrap().processes[0].proportional.pss)
        .collect();

    assert_eq!(&observed[..19], &[51200; 19]);
    assert_eq!(observed[19], 101376);
}

#[test]
fn exited_process_cache_is_evicted_even_if_identity_returns() {
    let root = fixture();
    let mut reader = collector(root.path());
    reader.snapshot().unwrap();
    fs::remove_dir_all(root.path().join("123")).unwrap();
    assert!(reader.snapshot().unwrap().processes.is_empty());
    process(root.path(), 123, "worker ) (a", 500, 2000, b"reappeared\0");

    let snapshot = reader.snapshot().unwrap();

    assert_eq!(snapshot.processes[0].cmdline, ["reappeared"]);
}

#[test]
fn per_mapping_smaps_fallback_sums_only_supported_counters() {
    let root = fixture();
    fs::remove_file(root.path().join("123/smaps_rollup")).unwrap();
    fs::write(root.path().join("123/smaps"), "map one\nPss: 3 kB\nPss_Anon: 2 kB\nPss_Dirty: 8 kB\nmap two\nPss: 4 kB\nPss_Shmem: 1 kB\n").unwrap();

    let snapshot = collector(root.path()).snapshot().unwrap();

    assert_eq!(snapshot.processes[0].proportional.pss, 7168);
    assert_eq!(snapshot.processes[0].proportional.anon, 2048);
    assert_eq!(snapshot.processes[0].proportional.shared, 1024);
}

#[test]
fn missing_system_files_leave_explicit_warnings_and_do_not_hide_processes() {
    let root = fixture();
    fs::remove_file(root.path().join("stat")).unwrap();
    fs::remove_file(root.path().join("meminfo")).unwrap();

    let snapshot = collector(root.path()).snapshot().unwrap();

    assert_eq!(snapshot.processes.len(), 1);
    assert!(snapshot.cpu_times.is_empty());
    assert_eq!(snapshot.memory.total, 0);
    assert_eq!(snapshot.warnings.len(), 2);
}

#[test]
fn small_processes_do_not_read_smaps() {
    let root = fixture();
    process(root.path(), 123, "small", 500, 1280, b"small\0");

    let snapshot = collector(root.path()).snapshot().unwrap();

    assert_eq!(snapshot.processes[0].proportional.pss, 0);
}
