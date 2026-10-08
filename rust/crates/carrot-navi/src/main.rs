use openpilot_carrot_navi::native::{run, Options};

fn main() {
    let options = match Options::parse(std::env::args().skip(1)) {
        Ok(Some(options)) => options,
        Ok(None) => {
            println!("Carrot Navi WebSocket receiver\n--host HOST --port PORT --advertise-ip IP --no-beacon --map-theme {{auto,dark,light}} --map-type {{normal,satellite}} --no-cereal");
            return;
        }
        Err(error) => {
            eprintln!("{}: {error}", error.kind);
            std::process::exit(2);
        }
    };
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    };
    if let Err(error) = runtime.block_on(run(options)) {
        eprintln!("{}: {error}", error.kind);
        std::process::exit(1);
    }
}
