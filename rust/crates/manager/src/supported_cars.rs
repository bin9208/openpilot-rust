//! Build-time export of opendbc car documentation; see tools/generate_manager_cars.py.
use crate::Error;

pub fn names(brand: &str) -> Result<Vec<String>, Error> {
    let source = match brand {
        "hyundai" => include_str!("../data/cars/hyundai.json"),
        "gm" => include_str!("../data/cars/gm.json"),
        "toyota" => include_str!("../data/cars/toyota.json"),
        "mazda" => include_str!("../data/cars/mazda.json"),
        "ford" => include_str!("../data/cars/ford.json"),
        "volkswagen" => include_str!("../data/cars/volkswagen.json"),
        "tesla" => include_str!("../data/cars/tesla.json"),
        _ => return Err(Error::Contract("unsupported car documentation brand")),
    };
    Ok(serde_json::from_str(source)?)
}
