use openpilot_carrot_navi::native::{run, Options};

#[derive(Debug, thiserror::Error)]
#[error("{kind}: {0}", kind = .0.kind)]
struct EntryError(#[from] openpilot_carrot_navi::Error);

fn main() {
    let mut exit_code = 1;
    let result = openpilot_managed_entry::runtime(
        "openpilot.selfdrive.carrot.carrot_navi",
        "carrot_navi",
        || {
            let options = Options::parse(std::env::args().skip(1)).map_err(|error| {
                exit_code = 2;
                openpilot_managed_entry::StepError::raised(EntryError(error))
            })?;
            let Some(options) = options else {
                println!("Carrot Navi WebSocket receiver\n--host HOST --port PORT --advertise-ip IP --no-beacon --map-theme {{auto,dark,light}} --map-type {{normal,satellite}} --no-cereal");
                return Ok(());
            };
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(openpilot_managed_entry::StepError::raised)?;
            runtime
                .block_on(run(options))
                .map_err(|error| openpilot_managed_entry::StepError::raised(EntryError(error)))
        },
    );
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(exit_code);
    }
}
