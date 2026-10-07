use super::Interface;
use crate::core::Error;
use std::{
    collections::BTreeSet,
    sync::{Mutex, OnceLock},
};

pub(super) fn constructor(interface: &Interface) -> Result<Vec<String>, Error> {
    static LOADED: OnceLock<Mutex<BTreeSet<String>>> = OnceLock::new();
    let names: Vec<&str> = match interface {
        Interface::Body(_) => vec!["comma_body"],
        Interface::Mock(_) | Interface::Hyundai(_) => Vec::new(),
        Interface::Tesla(vehicle) => std::iter::once(vehicle.state.party.dbc.name.as_str())
            .chain(
                vehicle
                    .state
                    .vehicle
                    .iter()
                    .map(|parser| parser.dbc.name.as_str()),
            )
            .collect(),
        Interface::Mazda(vehicle) => vec![vehicle.state.pt.dbc.name.as_str()],
        Interface::Volkswagen(vehicle) => vec![vehicle.state.pt.dbc.name.as_str()],
        Interface::Nissan(vehicle) => vec![vehicle.state.pt.dbc.name.as_str()],
        Interface::Chrysler(vehicle) => vec![vehicle.state.pt.dbc.name.as_str()],
        Interface::Rivian(vehicle) => vec![vehicle.state.pt.dbc.name.as_str()],
        Interface::Ford(vehicle) => vec![vehicle.state.pt.dbc.name.as_str()],
        Interface::Subaru(vehicle) => vec![vehicle.state.pt.dbc.name.as_str()],
        Interface::Toyota(vehicle) => vec![vehicle.state.pt.dbc.name.as_str()],
        Interface::Gm(vehicle) => vec![
            vehicle.state.pt.dbc.name.as_str(),
            "gm_global_a_object",
            "gm_global_a_chassis",
        ],
        Interface::Honda(vehicle) => std::iter::once(vehicle.state.pt.dbc.name.as_str())
            .chain(
                vehicle
                    .state
                    .body
                    .iter()
                    .map(|parser| parser.dbc.name.as_str()),
            )
            .collect(),
    };
    let mut loaded = LOADED
        .get_or_init(Mutex::default)
        .lock()
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    Ok(names
        .into_iter()
        .filter(|name| loaded.insert((*name).to_owned()))
        .map(|name| format!("DBC: {name}"))
        .collect())
}
