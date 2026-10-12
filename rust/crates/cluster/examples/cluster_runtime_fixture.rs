use capnp::{message::Builder, serialize};
use openpilot_cereal::log_capnp::event;
use openpilot_cluster::{
    runtime::{self, HelperConfig},
    runtime_args::RuntimeArgs,
};
use serde::Deserialize;
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};
#[derive(Deserialize)]
struct Request {
    root: PathBuf,
    args: Vec<String>,
    output: PathBuf,
    publish: bool,
}
fn publish(stop: &AtomicBool) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let capacity = |name| {
        openpilot_messaging::services::lookup(name)
            .map(|service| service.queue_size)
            .ok_or("service missing")
    };
    let mut car = openpilot_msgq::Publisher::with_capacity("carState", capacity("carState")?)?;
    let mut device =
        openpilot_msgq::Publisher::with_capacity("deviceState", capacity("deviceState")?)?;
    let mut index = 0_u32;
    while !stop.load(Ordering::Relaxed) {
        let mut message = Builder::new_default();
        let mut item = message.init_root::<event::Builder<'_>>();
        item.set_valid(true);
        let now = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
        let stamp = u64::try_from(now.tv_sec)? * 1_000_000_000 + u64::try_from(now.tv_nsec)?;
        item.set_log_mono_time(stamp);
        let mut state = item.init_car_state();
        state.set_v_ego(18.0);
        state.set_a_ego(0.2);
        state.set_steering_angle_deg(3.0);
        car.send(&serialize::write_message_to_words(&message))?;
        let mut message = Builder::new_default();
        let mut item = message.init_root::<event::Builder<'_>>();
        item.set_valid(true);
        item.set_log_mono_time(stamp);
        item.init_device_state().set_started(true);
        device.send(&serialize::write_message_to_words(&message))?;
        index += 1;
        thread::sleep(Duration::from_millis(20));
    }
    eprintln!("Owned live publisher sent {index} car/device pairs");
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let request: Request = serde_json::from_slice(&std::fs::read(
        std::env::args().nth(1).ok_or("request file missing")?,
    )?)?;
    let args = RuntimeArgs::parse_from(&request.root.join("openpilot"), &request.args)?;
    let program = std::env::current_exe()?
        .parent()
        .ok_or("executable parent missing")?
        .parent()
        .ok_or("target debug parent missing")?
        .join("openpilot-cluster");
    let helpers = HelperConfig {
        launcher: program.with_file_name("openpilot-process-child"),
        program,
        cwd: request.root.clone(),
    };
    let stop = Arc::new(AtomicBool::new(false));
    let publisher_stop = Arc::clone(&stop);
    let worker = request
        .publish
        .then(|| thread::spawn(move || publish(&publisher_stop)));
    let result = runtime::run(Path::new(&request.root), &args, &helpers);
    stop.store(true, Ordering::Relaxed);
    if let Some(worker) = worker {
        worker
            .join()
            .map_err(|_| "publisher panicked")?
            .map_err(|error| error.to_string())?;
    }
    std::fs::write(request.output, serde_json::to_vec(&result?)?)?;
    Ok(())
}
