mod firmware_wire;
use firmware_wire::{Input, Io};
use openpilot_card::{
    ecu::EcuAddress,
    firmware::{Brand, Catalog, Firmware},
    firmware_query::QueryOptions,
};
use serde::{Deserialize, Serialize};
use std::io::{self, Read};

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Case {
    Firmware {
        brand: Option<Brand>,
        pandas: usize,
        timeout: f64,
        io: Input,
    },
    Presence {
        pandas: usize,
        io: Input,
    },
    BrandMatches {
        present: Vec<EcuAddress>,
    },
    Ordered {
        present: Vec<EcuAddress>,
        vin: String,
        pandas: usize,
        timeout: f64,
        io: Input,
    },
}
#[derive(Serialize)]
#[serde(untagged)]
enum ResultValue {
    Firmware(Vec<Firmware>),
    Presence(Vec<EcuAddress>),
    BrandMatches(Vec<(Brand, usize, usize)>),
}
#[derive(Serialize)]
struct Output {
    result: ResultValue,
    io: Io,
}
fn trace(case: Case, catalog: &Catalog) -> Result<Output, Box<dyn std::error::Error>> {
    let (result, io) = match case {
        Case::Firmware {
            brand,
            pandas,
            timeout,
            io,
        } => {
            let mut io = Io::from(io);
            let result = catalog.query_firmware(
                QueryOptions {
                    brand,
                    pandas,
                    timeout,
                },
                &mut io,
            )?;
            (ResultValue::Firmware(result), io)
        }
        Case::Presence { pandas, io } => {
            let mut io = Io::from(io);
            let mut result = catalog.present_ecus(pandas, &mut io)?;
            result.sort_unstable();
            (ResultValue::Presence(result), io)
        }
        Case::Ordered {
            present,
            vin,
            pandas,
            timeout,
            io,
        } => {
            let mut io = Io::from(io);
            let result = catalog.query_ordered(&present, (&vin, pandas, timeout), &mut io)?;
            (ResultValue::Firmware(result), io)
        }
        Case::BrandMatches { present } => (
            ResultValue::BrandMatches(catalog.brand_matches(&present)?),
            Io::from(Input::default()),
        ),
    };
    Ok(Output { result, io })
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let cases: Vec<Case> = serde_json::from_str(&input)?;
    let catalog = Catalog::load()?;
    let output: Vec<_> = cases
        .into_iter()
        .map(|case| trace(case, &catalog))
        .collect::<Result<_, _>>()?;
    let path = std::env::args().nth(1).ok_or("missing output path")?;
    std::fs::write(path, serde_json::to_vec(&output)?)?;
    Ok(())
}
