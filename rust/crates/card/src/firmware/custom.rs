use super::{codes, Brand, Catalog, Ecu, Expected, Live, Model};
use std::collections::BTreeSet;

fn coded(model: &Model, live: &Live) -> bool {
    if !model.fuzzy_allowed {
        return false;
    }
    model.firmware.iter().all(|expected| {
        let checked = match model.brand {
            Brand::Ford => matches!(
                expected.ecu,
                Ecu::Abs | Ecu::FwdCamera | Ecu::FwdRadar | Ecu::Eps
            ),
            Brand::Hyundai | Brand::Toyota => {
                matches!(expected.ecu, Ecu::FwdRadar | Ecu::FwdCamera | Ecu::Eps)
            }
            Brand::Body
            | Brand::Chrysler
            | Brand::Gm
            | Brand::Honda
            | Brand::Mazda
            | Brand::Mock
            | Brand::Nissan
            | Brand::Psa
            | Brand::Rivian
            | Brand::Subaru
            | Brand::Tesla
            | Brand::Volkswagen => false,
        };
        if !checked {
            return true;
        }
        let expected_codes = codes::extract(model.brand, expected.versions.iter());
        let found_codes = codes::extract(
            model.brand,
            live.get(&expected.target()).into_iter().flatten(),
        );
        if !found_codes
            .iter()
            .any(|(code, _)| expected_codes.iter().any(|(expected, _)| expected == code))
        {
            return false;
        }
        let check_date = model.brand == Brand::Ford
            || (model.brand == Brand::Hyundai && expected.ecu == Ecu::FwdCamera);
        if check_date {
            let dates: BTreeSet<_> = expected_codes
                .iter()
                .filter_map(|(_, date)| date.as_ref())
                .collect();
            let (Some(first), Some(last)) = (dates.first(), dates.last()) else {
                return false;
            };
            return found_codes
                .iter()
                .filter_map(|(_, date)| date.as_ref())
                .any(|date| first <= &date && &date <= last);
        }
        true
    })
}

fn volkswagen(catalog: &Catalog, model: &Model, live: &Live, vin: &str) -> bool {
    let expected: Vec<_> = model
        .firmware
        .iter()
        .filter(|expected| expected.ecu == Ecu::FwdRadar)
        .collect();
    if expected.is_empty() {
        return false;
    }
    let compatible = expected.iter().all(|expected| {
        let available: Vec<&Expected> = catalog
            .models
            .iter()
            .filter(|model| model.brand == Brand::Volkswagen)
            .flat_map(|model| &model.firmware)
            .filter(|other| other.ecu == expected.ecu && other.target() == expected.target())
            .collect();
        live.get(&expected.target()).is_some_and(|versions| {
            versions
                .iter()
                .any(|version| available.iter().any(|ecu| ecu.versions.contains(version)))
        })
    });
    let wmi: String = vin.chars().take(3).collect();
    let chassis: String = vin.chars().skip(6).take(2).collect();
    compatible && model.wmis.contains(&wmi) && model.chassis.contains(&chassis)
}

pub(super) fn matches(catalog: &Catalog, live: &Live, vin: &str, brand: Brand) -> BTreeSet<String> {
    catalog
        .models
        .iter()
        .filter(|model| model.brand == brand)
        .filter(|model| match brand {
            Brand::Ford | Brand::Hyundai | Brand::Toyota => coded(model, live),
            Brand::Volkswagen => volkswagen(catalog, model, live, vin),
            Brand::Rivian => {
                let wmi: String = vin.chars().take(3).collect();
                let line: String = vin.chars().skip(3).take(1).collect();
                let year: String = vin.chars().skip(9).take(1).collect();
                model.wmis.contains(&wmi)
                    && model.lines.contains(&line)
                    && model.years.contains(&year)
            }
            Brand::Body
            | Brand::Chrysler
            | Brand::Gm
            | Brand::Honda
            | Brand::Mazda
            | Brand::Mock
            | Brand::Nissan
            | Brand::Psa
            | Brand::Subaru
            | Brand::Tesla => false,
        })
        .map(|model| model.name.clone())
        .collect()
}
