use openpilot_ui_application::services::updater::{self, Binding, Request, Strategy};
use serde::Deserialize;
use std::io::{BufRead, Write};
#[derive(Deserialize)]
struct Process {
    name: String,
    pid: i32,
    running: bool,
}
#[derive(Deserialize)]
struct Input {
    expected: std::path::PathBuf,
    processes: Vec<Process>,
    request: Request,
    #[serde(default)]
    legacy: bool,
    #[serde(default)]
    hold: bool,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut stdin = std::io::stdin().lock();
    let mut text = String::new();
    stdin.read_line(&mut text)?;
    let input: Input = serde_json::from_str(&text)?;
    let mut message = capnp::message::Builder::new_default();
    let event = message.init_root::<openpilot_cereal::log_capnp::event::Builder>();
    let mut processes = event
        .init_manager_state()
        .init_processes(u32::try_from(input.processes.len())?);
    for (index, process) in input.processes.iter().enumerate() {
        let mut value = processes.reborrow().get(u32::try_from(index)?);
        value.set_name(process.name.as_str());
        value.set_pid(process.pid);
        value.set_running(process.running);
    }
    let event = message.get_root_as_reader::<openpilot_cereal::log_capnp::event::Reader>()?;
    let openpilot_cereal::log_capnp::event::Which::ManagerState(manager) = event.which()? else {
        return Err("managerState missing".into());
    };
    let binding = updater::bind(
        manager?,
        &input.expected,
        if input.legacy {
            Strategy::RecheckedPid
        } else {
            Strategy::PidfdWhenAvailable
        },
    )?;
    if input.hold {
        println!("bound");
        std::io::stdout().flush()?;
        text.clear();
        stdin.read_line(&mut text)?;
    }
    let outcome = match binding {
        Binding::Ready(target) => target.send(input.request)?,
        Binding::Unavailable(reason) => updater::Outcome::Unavailable(reason),
    };
    println!("{}", serde_json::to_string(&outcome)?);
    Ok(())
}
