mod controller_fixture;
use controller_fixture::{
    effects::{Messages, Trace},
    Fixture, Request,
};
use openpilot_logging::producer::Factory;
use openpilot_params::Params;
use std::{
    io::{self, BufRead, Write},
    path::Path,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let [root, prefix, endpoint] = args.as_slice() else {
        return Err("expected Params root, prefix, logging endpoint".into());
    };
    let params = Params::open(Path::new(root), prefix)?;
    let factory = Factory::new(endpoint.clone())?;
    let mut logger = factory.logger();
    let mut trace = Trace {
        directory: Path::new(root).join(prefix),
        params: &params,
        logger: &mut logger,
        now: 0.0,
        streams: Vec::new(),
        effects: Vec::new(),
    };
    let mut fixture = Fixture::default();
    let mut stdout = io::stdout().lock();
    for line in io::stdin().lock().lines() {
        trace.effects.clear();
        let mut messages = Messages::default();
        let request: Request = serde_json::from_str(&line?)?;
        let result = fixture.dispatch(request, &mut trace, &mut messages);
        let error = result.err().map(|error| format!("{error:?}"));
        let mut response = serde_json::json!({"state":fixture.controller.as_ref().map(|controller|controller.snapshot()),"health":fixture.health(),"effects":trace.effects,"messages":messages,"error":error});
        controller_fixture::compress::buffers(&mut response);
        serde_json::to_writer(&mut stdout, &response)?;
        stdout.write_all(b"\n")?;
        stdout.flush()?;
    }
    Ok(())
}
