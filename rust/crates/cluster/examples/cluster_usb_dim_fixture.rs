use openpilot_cluster::autorun::usb::Usb;
use openpilot_params::Params;
use std::{
    io::{self, Read},
    path::PathBuf,
};

#[derive(serde::Deserialize)]
struct Case {
    root: PathBuf,
    repository: PathBuf,
    lock: PathBuf,
    product: u16,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let case: Case = serde_json::from_str(&input)?;
    let params = Params::open(&case.root, "fixture")?;
    let mut usb = Usb::new(&case.repository, &case.lock);
    let dim = usb.dim(case.product, "output disabled");
    drop(usb);
    println!(
        "{}",
        serde_json::json!({"dim":dim,"closed":params.get_bool("ClusterHudConnected")?})
    );
    Ok(())
}
