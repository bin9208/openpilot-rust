use openpilot_card::{
    core::Vehicle,
    identification::{FingerprintSource, Identification},
    registry::{self, Interface},
    runtime::Common,
};
use openpilot_cereal::car_capnp::car_params;
use openpilot_params::Params;
use serde::Deserialize;
use std::{
    io::{self, Read},
    path::Path,
};

#[derive(Deserialize)]
struct Case {
    candidate: String,
    fingerprints: Vec<(u8, Vec<(u32, usize)>)>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<_> = std::env::args().collect();
    let dbc = Path::new(arguments.get(1).ok_or("missing DBC root")?);
    let assets = Path::new(arguments.get(2).ok_or("missing model assets")?);
    let numerics = Path::new(arguments.get(3).ok_or("missing numerical artifact")?);
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let cases: Vec<Case> = serde_json::from_str(&input)?;
    for (index, case) in cases.into_iter().enumerate() {
        println!("CASE {index}");
        let identification = Identification {
            candidate: case.candidate,
            observed: case.fingerprints,
            vin: "00000000000000000".into(),
            firmware: Vec::new(),
            source: FingerprintSource::Fixed,
            exact_match: true,
            cached: false,
            vin_rx_address: None,
            vin_rx_bus: None,
            ecu_responses: Vec::new(),
            fw_query_time: 0.,
            packets: 202,
        };
        let directory = tempfile::tempdir()?;
        let settings = Params::open(directory.path(), "d")?;
        let mut message = registry::parameters(&identification, &settings, false)?;
        for line in registry::parameter_diagnostics(
            &registry::ParamsInput {
                identification: &identification,
                settings: &settings,
                alpha_long: false,
            },
            &message,
            assets,
        )? {
            println!("{line}");
        }
        openpilot_card::startup::decorate(
            &identification,
            message.get_root::<car_params::Builder>()?,
        )?;
        let bytes = capnp::serialize::write_message_to_words(&message);
        let mut vehicle = Interface::new(registry::Setup {
            identification: &identification,
            params_bytes: &bytes,
            dbc_root: dbc,
            settings,
            now_ns: 0,
        })?;
        for line in vehicle
            .constructor_diagnostics()?
            .into_iter()
            .chain(vehicle.take_diagnostics())
        {
            println!("{line}");
        }
        let _common = Common::new(
            message.get_root_as_reader()?,
            &Params::open(directory.path(), "d")?,
            assets,
            numerics,
        )?;
        if message
            .get_root_as_reader::<car_params::Reader>()?
            .get_sec_oc_required()
        {
            let saved = Params::open(directory.path(), "d")?;
            saved.put("SecOCKey", b"000102030405060708090a0b0c0d0e0f")?;
            let prepared = openpilot_card::startup::prepare(
                &saved,
                &identification,
                message,
                vehicle.has_controller(),
                None,
            )?;
            let key = prepared.secoc_key.ok_or("prepared SecOC key missing")?;
            vehicle.set_secoc_key(&key)?;
            assert!(matches!(vehicle, Interface::Toyota(_)));
            assert_eq!(
                key,
                std::array::from_fn(|index| u8::try_from(index).unwrap())
            );
            assert!(prepared
                .params
                .get_root_as_reader::<car_params::Reader>()?
                .get_sec_oc_key_available());
        }
    }
    Ok(())
}
