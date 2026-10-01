use openpilot_modem::{at, config::Config, observe, runtime::Modem, state::State, Error};
use serde::Deserialize;
use serde_json::json;
use std::io;
#[derive(Deserialize)]
struct Request {
    config: Config,
    operations: Vec<Operation>,
}
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Operation {
    Step { state: State },
    Check { state: State },
    Poll,
    Identity,
    At { command: String },
    Routes { ip: String, peer: String },
    Dns { servers: Vec<String> },
    Sleep { ms: u64 },
    Kill,
    WaitExit,
}
fn main() -> Result<(), Error> {
    let request: Request = serde_json::from_reader(io::stdin())?;
    let mut modem = Modem::new(request.config);
    let mut rows = Vec::new();
    for operation in request.operations {
        let result = match operation {
            Operation::Step { state } => json!(modem.step(state)?),
            Operation::Check { state } => {
                modem.check_iccid(state);
                json!(null)
            }
            Operation::Poll => {
                observe::poll(&modem.config, &mut modem.snapshot, &mut modem.session);
                json!(null)
            }
            Operation::Identity => {
                observe::identity(&modem.config, &mut modem.snapshot);
                json!(null)
            }
            Operation::At { command } => json!(at::command(&modem.config, &command)),
            Operation::Routes { ip, peer } => {
                json!(modem.session.routes(&modem.config, &ip, &peer)?)
            }
            Operation::Dns { servers } => json!(modem.session.dns(&modem.config, &servers)?),
            Operation::Sleep { ms } => {
                std::thread::sleep(std::time::Duration::from_millis(ms));
                json!(null)
            }
            Operation::WaitExit => {
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
                while !modem.session.has_exited()? {
                    if std::time::Instant::now() >= deadline {
                        return Err(Error::Contract("PPP fixture exit timeout"));
                    }
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                json!(null)
            }
            Operation::Kill => {
                modem.session.kill(&modem.config)?;
                json!(null)
            }
        };
        rows.push(
            json!({"result": result, "snapshot": modem.snapshot, "fails": modem.session.fails}),
        );
    }
    serde_json::to_writer(io::stdout(), &rows)?;
    Ok(())
}
