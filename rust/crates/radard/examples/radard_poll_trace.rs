use capnp::{message::Builder, serialize};
use openpilot_cereal::car_capnp::car_params;
use openpilot_messaging::state::Poll;
use openpilot_radard::wire;
use serde::Deserialize;
#[derive(Deserialize)]
struct Case {
    brand: String,
    ext: u32,
    unavailable: bool,
    mode: i32,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cases: Vec<Case> = serde_json::from_reader(std::io::stdin().lock())?;
    let mut outputs = Vec::new();
    for case in cases {
        let mut message = Builder::new_default();
        let mut cp = message.init_root::<car_params::Builder<'_>>();
        cp.set_brand(case.brand.as_str());
        cp.set_ext_flags(case.ext);
        cp.set_radar_unavailable(case.unavailable);
        let options = wire::subscriptions(&serialize::write_message_to_words(&message), case.mode)?;
        outputs.push(match options.poll {
            Poll::One(name) => vec![name],
            Poll::Many(names) => names,
            Poll::All => return Err("unexpected all-services poll".into()),
        });
    }
    serde_json::to_writer(std::io::stdout().lock(), &outputs)?;
    Ok(())
}
