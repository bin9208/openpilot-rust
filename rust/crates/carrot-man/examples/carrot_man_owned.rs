use openpilot_carrot_man::native::{actor, config::Config};
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    io::{self, Read},
    net::Ipv4Addr,
    path::PathBuf,
};

#[derive(Deserialize)]
struct Fixture {
    root: PathBuf,
    ports: [u16; 7],
    values: BTreeMap<String, String>,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let fixture: Fixture = serde_json::from_str(&input)?;
    let prefix = std::env::var("OPENPILOT_PREFIX").unwrap_or_else(|_| "d".into());
    let params_root = fixture.root.join("params");
    let params = openpilot_params::Params::open(&params_root, &prefix)?;
    for (key, value) in fixture.values {
        params.put(&key, value.as_bytes())?;
    }
    let data = fixture.root.join("data");
    std::fs::create_dir_all(data.join("media"))?;
    std::fs::create_dir_all(data.join("params/d"))?;
    let config = Config {
        params_root: Some(params_root),
        memory_root: fixture.root.join("params/memory"),
        prefix,
        bind: Ipv4Addr::LOCALHOST,
        udp_port: fixture.ports[0],
        tcp_port: fixture.ports[1],
        http_port: fixture.ports[2],
        route_port: fixture.ports[3],
        kisa_port: fixture.ports[4],
        command_port: fixture.ports[5],
        broadcast_port: fixture.ports[6],
        geos_library: std::env::var_os("CARROT_GEOS_LIBRARY").map(PathBuf::from),
        web_settings: fixture.root.join("web_settings.json"),
        repo_root: fixture.root.clone(),
        data_root: data,
    };
    actor::run(config)?;
    Ok(())
}
