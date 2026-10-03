use openpilot_card::firmware::{platform_codes, Brand, Catalog, Firmware, MatchOptions};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    io::{self, Read},
};

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Case {
    Match {
        versions: Vec<Firmware>,
        vin: String,
        exact: bool,
        fuzzy: bool,
    },
    Select {
        name: String,
    },
    Codes {
        brand: Brand,
        versions: Vec<Vec<u8>>,
    },
}
#[derive(Serialize)]
#[serde(untagged)]
enum Output {
    Match(openpilot_card::firmware::CarMatch),
    Select(Option<String>),
    Codes(BTreeSet<(Vec<u8>, Option<Vec<u8>>)>),
}
fn trace(case: Case, catalog: &Catalog) -> Result<Output, openpilot_card::firmware::Error> {
    Ok(match case {
        Case::Match {
            versions,
            vin,
            exact,
            fuzzy,
        } => Output::Match(catalog.match_car(&versions, &vin, MatchOptions { exact, fuzzy })?),
        Case::Select { name } => {
            Output::Select(catalog.selected_platform(&name).map(str::to_owned))
        }
        Case::Codes { brand, versions } => Output::Codes(platform_codes(brand, &versions)),
    })
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
