//! Actual msgq acknowledgement must wait for a busy recipient to finish work.
use super::{fixtures, peer::capacity, process::Process, require, save, Result};
use openpilot_msgq::{Publisher, Subscriber};
use serde_json::{json, Value};
use std::{
    fs,
    io::Write,
    path::Path,
    process::Command,
    time::{Duration, Instant},
};
fn nanos() -> u64 {
    let time = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    time.tv_sec as u64 * 1_000_000_000 + time.tv_nsec as u64
}
pub(super) fn fixture(output: &Path) -> Result<()> {
    let mut subscriber = Subscriber::for_runtime("can", false, capacity("can")?)?;
    fs::write(output.join("ready"), b"ready")?;
    println!("ready");
    std::io::stdout().flush()?;
    let packet = subscriber
        .receive(Duration::from_secs(5))?
        .ok_or("input-completion fixture did not receive CAN")?;
    let received = nanos();
    let until = Instant::now() + Duration::from_millis(100);
    while Instant::now() < until {
        std::hint::spin_loop();
    }
    save(
        &output.join("processed.json"),
        &json!({"received":received,"completed":nanos(),"sha256":super::hash(&packet)}),
    )?;
    loop {
        let _ = subscriber.receive(Duration::ZERO)?;
        // Mirrors the source's next blocking receive without a Python process.
        std::thread::sleep(Duration::from_millis(10));
    }
}
pub(super) fn check(output: &Path) -> Result<Value> {
    fs::create_dir_all(output.parent().ok_or("input-completion parent absent")?)?;
    fs::create_dir(output)?;
    let mut publisher = Publisher::for_runtime("can", capacity("can")?)?;
    let mut command = Command::new(std::env::current_exe()?);
    command.arg(super::IDLE).arg(output);
    let mut process = Process::start(&mut command, output, Duration::from_secs(10))?;
    require(
        process.line()? == b"ready",
        "input-completion fixture readiness differs",
    )?;
    let packet = fixtures::event("can", 123)?;
    let started = nanos();
    publisher.send(&packet)?;
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        process.alive()?;
        if publisher.readers_caught_up() {
            break;
        }
        require(
            Instant::now() < deadline,
            "input-completion reader acknowledgement exceeded bound",
        )?;
        std::thread::sleep(Duration::from_millis(1));
    }
    super::peer::await_idle_pid(process.pid()?, || process.alive(), Duration::from_secs(10))?;
    let returned = nanos();
    let processed: Value = serde_json::from_slice(&fs::read(output.join("processed.json"))?)?;
    let exact = processed["sha256"] == super::hash(&packet);
    let completed = processed["completed"]
        .as_u64()
        .ok_or("input-completion finished time missing")?;
    let result = json!({"started":started,"send_returned":returned,"received":processed["received"],"completed":completed,"sha256":processed["sha256"],"processing_finished_before_send_return":completed<=returned,"packet_exact":exact});
    save(&output.join("result.json"), &result)?;
    require(
        exact && completed <= returned,
        "queue acknowledgement returned before exact packet processing completed",
    )?;
    rustix::process::kill_process(process.pid()?, rustix::process::Signal::TERM)?;
    let _ = process.wait(Duration::from_secs(5))?;
    Ok(result)
}
