use super::{
    corpus::Corpus, owned::Directory, process::Process, reference::snapshot, require, save,
    Options, Result,
};
use rustix::process::Signal;
use serde_json::{json, Value};
use std::{
    fs,
    io::Write,
    path::Path,
    process::Command,
    time::{Duration, Instant},
};
fn command(binary: &Path, root: &Path) -> Command {
    let mut cmd = Command::new(binary);
    cmd.env("LOG_ROOT", root)
        .env_remove("MANAGER_DAEMON")
        .env_remove("LD_PRELOAD")
        .env_remove("OPENPILOT_LOG_CONTEXT");
    cmd
}
fn call(cmd: &mut Command, output: &Path) -> Result<Value> {
    let mut child = Process::spawn(cmd, output)?;
    let code = child.finish()?;
    child.summary(code)
}
pub fn run(options: &Options, corpus: &Corpus, output: &Path) -> Result<Value> {
    let work = Directory::temporary()?;
    let absent = work.path.join("absent");
    let mut rows = Vec::new();
    for (name, signal) in [("sigint", Signal::INT), ("sigterm", Signal::TERM)] {
        let mut child = Process::spawn(&mut command(&options.binary, &absent), &output.join(name))?;
        while !child.ready {
            child.drain()?;
            require(!child.exited()?, "deleter exited before readiness")?;
            std::thread::sleep(Duration::from_millis(2));
        }
        std::thread::sleep(Duration::from_millis(50));
        let before = Instant::now();
        child.signal(signal)?;
        let code = child.finish()?;
        let elapsed = before.elapsed().as_secs_f64();
        require(
            code == 0 && elapsed < 0.3 && !absent.try_exists()?,
            "deleter idle shutdown or root ownership differs",
        )?;
        let mut row = child.summary(code)?;
        row["shutdown_seconds"] = json!(elapsed);
        row["case"] = json!(name);
        save(&output.join(name).join("report.json"), &row)?;
        rows.push(row);
    }
    for (index, args) in [
        ["--cycles", "0"].as_slice(),
        ["--cycles", "-1"].as_slice(),
        ["--unknown"].as_slice(),
    ]
    .iter()
    .enumerate()
    {
        let row = call(
            command(&options.binary, &absent).args(*args),
            &output.join(format!("invalid-{index}")),
        )?;
        require(
            row["exit"] == 1 && !absent.try_exists()?,
            "deleter invalid CLI changed",
        )?;
        rows.push(row);
    }
    let row = call(
        command(&options.binary, &absent).args(["--cycles", "1"]),
        &output.join("one-cycle"),
    )?;
    require(
        row["exit"] == 0 && !absent.try_exists()?,
        "deleter one-cycle missing root changed",
    )?;
    rows.push(row);
    let request = output.join("private-mount-request.json");
    let mut mount_options = options.clone();
    mount_options.output = output.join("mount-details");
    save(&request, &serde_json::to_value(&mount_options)?)?;
    let exe = std::env::current_exe()?;
    // Root-owned timeout bounds the elevated namespace even if its caller dies.
    let low = call(
        Command::new("/usr/bin/sudo")
            .args([
                "-n",
                "/usr/bin/timeout",
                "--signal=TERM",
                "--kill-after=2s",
                "15s",
                "/usr/bin/unshare",
                "--mount",
                "--fork",
                "--kill-child=SIGKILL",
            ])
            .arg(exe)
            .arg("--deleter-mount")
            .arg(&request),
        &output.join("mount"),
    )?;
    require(
        low["exit"] == 0,
        "native deleter private mount worker failed",
    )?;
    let raw = fs::read(output.join("mount/stdout.bin"))?;
    let actual: Value = serde_json::from_slice(&raw)?;
    require(
        actual["remaining"] == corpus.low_space_original["remaining"]
            && actual["passed"] == true
            && actual["low_space_cycles"] == 3,
        "native private low-space poststate differs from original",
    )?;
    Ok(
        json!({"passed":true,"idle_and_cli_cases":rows.len(),"idle_and_cli":rows,"low_space_cycles":3,"low_space":actual,"actual_original_low_space":corpus.low_space_original}),
    )
}
fn utility(args: &[&str], output: &Path) -> Result<()> {
    let (first, rest) = args.split_first().ok_or("missing mount utility")?;
    let result = call(Command::new(*first).args(rest), output)?;
    require(result["exit"] == 0, "private mount utility failed")
}
pub fn mount(options: &Options, corpus: &Corpus, output: &Path) -> Result<Value> {
    let work = Directory::temporary()?;
    let mount = work.path.join("mount");
    fs::create_dir(&mount)?;
    utility(
        &["/usr/bin/mount", "--make-rprivate", "/"],
        &output.join("private-propagation"),
    )?;
    let target = mount.to_str().ok_or("private mount path UTF8")?;
    utility(
        &[
            "/usr/bin/mount",
            "-t",
            "tmpfs",
            "-o",
            "size=16m",
            "tmpfs",
            target,
        ],
        &output.join("tmpfs-mount"),
    )?;
    let mounted = match rustix::fs::open(
        &mount,
        rustix::fs::OFlags::PATH
            | rustix::fs::OFlags::DIRECTORY
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    ) {
        Ok(fd) => fs::File::from(fd),
        Err(error) => {
            std::mem::forget(work);
            return Err(error.into());
        }
    };
    let result = (|| -> Result<Value> {
        let root = mount.join("rust");
        fs::create_dir(&root)?;
        for name in ["route--0", "route--1", "route--2", "route--3", "boot"] {
            fs::create_dir(root.join(name))?;
            fs::write(root.join(name).join("rlog.zst"), b"synthetic")?;
        }
        fs::write(root.join("route--0/inuse.lock"), [])?;
        let stats = rustix::fs::statvfs(&mount)?;
        let available = u128::from(stats.f_bavail) * u128::from(stats.f_frsize);
        require(
            available < 5 * 1024 * 1024 * 1024,
            "private tmpfs is not actually low space",
        )?;
        let row = call(
            command(&options.binary, &root).args(["--cycles", "3"]),
            &output.join("low-space-daemon"),
        )?;
        let intervals = row["deletion_intervals"]
            .as_array()
            .ok_or("deleter intervals missing")?;
        require(
            row["exit"] == 0
                && row["ready"] == true
                && intervals.len() == 2
                && intervals
                    .iter()
                    .all(|v| v.as_f64().is_some_and(|t| (0.09..0.5).contains(&t))),
            "native deleter real cadence or count changed",
        )?;
        let remaining = snapshot(&root)?;
        require(
            remaining == corpus.low_space_original["remaining"],
            "private native filesystem differs from original",
        )?;
        Ok(
            json!({"passed":true,"low_space_cycles":3,"available_bytes":available,"remaining":remaining,"daemon":row,"original_source_waits":corpus.low_space_original["source_waits"]}),
        )
    })();
    use std::os::unix::fs::MetadataExt;
    let identity = mounted
        .metadata()
        .and_then(|before| fs::symlink_metadata(&mount).map(|current| (before, current)));
    let (before, current) = match identity {
        Ok(pair) => pair,
        Err(error) => {
            std::mem::forget(work);
            return Err(error.into());
        }
    };
    if !current.is_dir()
        || current.file_type().is_symlink()
        || (before.dev(), before.ino()) != (current.dev(), current.ino())
    {
        // Preserve unknown replacement paths; namespace exit releases the old mount.
        std::mem::forget(work);
        return Err("private mount replaced; cleanup authority rejected".into());
    }
    let unmount = utility(&["/usr/bin/umount", target], &output.join("tmpfs-unmount"));
    if let Err(error) = unmount {
        std::mem::forget(work);
        return Err(error);
    }
    result
}
pub fn emit_observer_fixture(path: &Path) -> Result<()> {
    let start = Instant::now();
    let mut times = vec![0.0];
    let mut stderr = std::io::stderr().lock();
    stderr.write_all("deleter: ready\ndeleter: deleting 경로--0\n".as_bytes())?;
    stderr.flush()?;
    for index in [1, 2] {
        std::thread::sleep(Duration::from_millis(100));
        times.push(start.elapsed().as_secs_f64());
        writeln!(stderr, "deleter: deleting 경로--{index}")?;
        stderr.flush()?;
    }
    save(path, &json!(times))?;
    std::thread::sleep(Duration::from_millis(150));
    Ok(())
}
pub fn observer(output: &Path) -> Result<Value> {
    let emitted = output.join("emitted.json");
    let exe = std::env::current_exe()?;
    let row = call(
        Command::new(exe)
            .arg("--deleter-observer-fixture")
            .arg(&emitted),
        output,
    )?;
    let expected = "deleter: ready\ndeleter: deleting 경로--0\ndeleter: deleting 경로--1\ndeleter: deleting 경로--2\n";
    let emission: Vec<f64> = serde_json::from_slice(&fs::read(emitted)?)?;
    require(
        emission.len() == 3
            && emission
                .windows(2)
                .all(|p| (0.09..0.5).contains(&(p[1] - p[0]))),
        "observer fixture emission cadence differs",
    )?;
    require(
        row["exit"] == 0
            && row["ready"] == true
            && row["stderr"] == expected
            && row["deletion_intervals"].as_array().is_some_and(|v| {
                v.len() == 2
                    && v.iter()
                        .all(|t| t.as_f64().is_some_and(|v| (0.09..0.5).contains(&v)))
            }),
        "observer dropped merged readiness/first-delete line or changed cadence",
    )?;
    Ok(
        json!({"passed":true,"observer":row,"emitted_times":emission,"legacy_observer_regression_ported":true}),
    )
}
