use openpilot_cluster::monitors::{find_git_root, GitCommands, NativeGitCommands, RefreshWorker};
use openpilot_cluster::monitors::{valid_address, CoreHistory, ThreadCpuSample};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread,
    time::{Duration, Instant},
};

fn sample(start: i64, ticks: i64, core: i64) -> ThreadCpuSample {
    ThreadCpuSample {
        key: (42, 43, start),
        pid: 42,
        process_name: "cluster".into(),
        cpu_time_ticks: ticks,
        processor: core,
    }
}

#[test]
fn addresses_reject_unusable_hosts_and_keep_canonical_scoped_ipv6() {
    for input in [
        "",
        "0.0.0.0",
        "127.1.2.3",
        "169.254.8.9",
        "::",
        "::1",
        "fe80::123%eth0",
        "::ffff:127.0.0.1",
        "::ffff:0.0.0.0",
        "::ffff:169.254.1.1",
        "192.168.001.2",
        "host",
    ] {
        assert_eq!(valid_address(input.as_bytes()), None, "{input}");
    }
    for (input, expected) in [
        (" 192.0.2.1 ", "192.0.2.1"),
        ("2001:0DB8::AB%eth0", "2001:db8::ab%eth0"),
        ("::ffff:192.0.2.1", "::ffff:192.0.2.1"),
    ] {
        assert_eq!(valid_address(input.as_bytes()).as_deref(), Some(expected));
    }
}

#[test]
fn core_usage_baseline_pid_reuse_migration_and_zero_delta_match_source() {
    let mut history = CoreHistory::new(100);
    assert!(history.observe(&[sample(1, 10, 6)], 1.0).text.is_none());
    assert_eq!(
        history.observe(&[sample(1, 11, 7)], 3.0).text.as_deref(),
        Some("[7(1)]")
    );
    assert_eq!(
        history.observe(&[sample(2, 1000, 7)], 4.0).text.as_deref(),
        Some("[]")
    );
    assert_eq!(
        history.observe(&[sample(2, 999, 7)], 5.0).text.as_deref(),
        Some("[]")
    );
    assert_eq!(
        history.observe(&[sample(2, 1004, -1)], 6.0).text.as_deref(),
        Some("[]")
    );
}

#[test]
fn refresh_returns_cached_state_coalesces_requests_and_joins_once() {
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let closed = Arc::new(AtomicBool::new(false));
    let observed = Arc::clone(&closed);
    let mut count = 0;
    let mut worker = RefreshWorker::new("cluster-monitor-test", 1.0, 0, move || {
        count += 1;
        started_tx
            .send((count, thread::current().name().map(str::to_owned)))
            .unwrap();
        release_rx.recv().unwrap();
        observed.store(true, Ordering::Release);
        Ok(count)
    });
    assert_eq!(worker.request(0.0).unwrap(), 0);
    assert_eq!(
        started_rx.recv_timeout(Duration::from_secs(2)).unwrap(),
        (1, Some("cluster-monitor-test".into()))
    );
    for now in [0.1, 0.9, 1.0, 2.0] {
        assert_eq!(worker.request(now).unwrap(), 0);
    }
    release_tx.send(()).unwrap();
    assert_eq!(
        started_rx.recv_timeout(Duration::from_secs(2)).unwrap().0,
        2
    );
    release_tx.send(()).unwrap();
    worker.close().unwrap();
    worker.close().unwrap();
    assert!(closed.load(Ordering::Acquire));
    assert_eq!(worker.request(100.0).unwrap(), 2);
}

#[test]
fn blocked_refresh_close_and_drop_return_within_source_one_second_bound() {
    for explicit_close in [true, false] {
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let mut worker = RefreshWorker::new("blocked-monitor-test", 1.0, 0, move || {
            started_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            Ok(1)
        });
        worker.request(0.0).unwrap();
        started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let closer = thread::spawn(move || {
            let started = Instant::now();
            if explicit_close {
                worker.close().unwrap();
            }
            drop(worker);
            done_tx.send(started.elapsed()).unwrap();
        });
        let result = done_rx.recv_timeout(Duration::from_millis(1500));
        release_tx.send(()).unwrap();
        closer.join().unwrap();
        let elapsed = result.expect("close blocked past the source timeout");
        assert!(elapsed >= Duration::from_millis(900), "{elapsed:?}");
        assert!(elapsed < Duration::from_millis(1500), "{elapsed:?}");
    }
}

#[test]
fn native_git_drains_both_pipes_preserves_replacement_and_times_out_its_group() {
    let root = tempfile::tempdir().unwrap();
    let program = root.path().join("git-provider");
    fs::write(&program, "#!/bin/sh\nif [ \"$3\" = pressure ]; then\n i=0; while [ $i -lt 3000 ]; do printf 'output\\n'; printf 'error\\n' >&2; i=$((i+1)); done\n printf '\\377\\r\\n'; exit 2\nfi\nsleep 30 &\nprintf '%s' \"$!\" > \"$2/child.pid\"\nwait\n").unwrap();
    fs::set_permissions(&program, fs::Permissions::from_mode(0o700)).unwrap();
    let cancelled = Arc::new(AtomicBool::new(false));
    let mut commands = NativeGitCommands::with_executable(program, Arc::clone(&cancelled));
    let output = commands.run(root.path(), &["pressure"], Duration::from_secs(3));
    assert_eq!(output.code, 2);
    assert_eq!(output.stdout.lines().count(), 3001);
    assert!(output.stdout.ends_with("\u{fffd}\n"));
    let started = Instant::now();
    assert_eq!(
        commands
            .run(root.path(), &["timeout"], Duration::from_millis(200))
            .code,
        124
    );
    assert!(started.elapsed() < Duration::from_secs(2));
    let pid = fs::read_to_string(root.path().join("child.pid")).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let stat = fs::read_to_string(format!("/proc/{pid}/stat"));
        if stat.as_ref().map_or(true, |value| {
            value
                .rsplit_once(") ")
                .is_some_and(|(_, fields)| fields.starts_with('Z'))
        }) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "owned Git descendant remained alive"
        );
        thread::sleep(Duration::from_millis(5));
    }
    cancelled.store(true, Ordering::Release);
    assert_eq!(
        commands
            .run(root.path(), &["pressure"], Duration::from_secs(1))
            .code,
        124
    );
}

#[test]
fn git_discovery_accepts_missing_descendant_and_worktree_head() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join(".git")).unwrap();
    assert_eq!(
        find_git_root(&root.path().join("missing/nested")),
        Some(root.path().to_owned())
    );
}

#[test]
fn network_worker_reads_owned_params_and_refreshes_only_at_deadline() {
    use openpilot_cluster::monitors::NetworkAddressProvider;
    let root = tempfile::tempdir().unwrap();
    let params = openpilot_params::Params::open(root.path(), "fixture").unwrap();
    let mut network = NetworkAddressProvider::new(Some(params.clone()), 2.0);
    for (now, value) in [(0.0, "192.0.2.253"), (2.0, "2001:db8::253")] {
        params.put("NetworkAddress", value.as_bytes()).unwrap();
        if now > 0.0 {
            assert_eq!(
                network.address(1.99).unwrap().as_deref(),
                Some("192.0.2.253")
            );
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        while network.address(now).unwrap().as_deref() != Some(value) {
            assert!(
                Instant::now() < deadline,
                "network worker did not publish Params address"
            );
            thread::sleep(Duration::from_millis(5));
        }
    }
    network.close().unwrap();
    assert_eq!(
        network.address(100.0).unwrap().as_deref(),
        Some("2001:db8::253")
    );
}

#[test]
fn native_git_status_reads_owned_remote_up_to_date_behind_and_deleted_branch() {
    use openpilot_cluster::{
        models::GitBranchStatusState,
        monitors::{read_git_status, GitBranchStatusProvider},
    };
    use std::{path::Path, process::Command};
    fn git(path: &Path, args: &[&str]) {
        let output = Command::new("git")
            .arg("-C")
            .arg(path)
            .args(args)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", "Cluster test")
            .env("GIT_AUTHOR_EMAIL", "cluster@example.invalid")
            .env("GIT_COMMITTER_NAME", "Cluster test")
            .env("GIT_COMMITTER_EMAIL", "cluster@example.invalid")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let root = tempfile::tempdir().unwrap();
    let author = root.path().join("author");
    let consumer = root.path().join("consumer");
    git(root.path(), &["init", "--bare", "remote"]);
    git(root.path(), &["init", "--initial-branch=dev", "author"]);
    fs::write(author.join("state"), "one").unwrap();
    git(&author, &["add", "state"]);
    git(&author, &["commit", "-m", "one"]);
    git(&author, &["remote", "add", "origin", "../remote"]);
    git(&author, &["push", "-u", "origin", "dev"]);
    git(
        root.path(),
        &["clone", "--branch", "dev", "remote", "consumer"],
    );
    let mut provider = GitBranchStatusProvider::new(&consumer, 60.0, 4.0, true);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let status = provider.status(0.0).unwrap();
        if status.state == GitBranchStatusState::Ok {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "git worker did not publish: {status:?}"
        );
        thread::sleep(Duration::from_millis(5));
    }
    provider.close().unwrap();
    fs::write(author.join("state"), "two").unwrap();
    git(&author, &["commit", "-am", "two"]);
    git(&author, &["push", "origin", "dev"]);
    let mut commands = NativeGitCommands::new(Arc::new(AtomicBool::new(false)));
    let status = read_git_status(Some(&consumer), &mut commands, Duration::from_secs(4));
    assert_eq!(status.state, GitBranchStatusState::Pull);
    assert_eq!(status.detail, "git pull 가능 +1");
    git(
        &root.path().join("remote"),
        &["update-ref", "-d", "refs/heads/dev"],
    );
    let status = read_git_status(Some(&consumer), &mut commands, Duration::from_secs(4));
    assert_eq!(status.state, GitBranchStatusState::Missing);
    assert_eq!(status.detail, "원격에서 삭제됨");
}
