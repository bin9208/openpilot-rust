use super::{custom, Brand, CarMatch, Catalog, Ecu, Error, Firmware, Live, MatchOptions};
use crate::query::Target;
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn live(versions: &[Firmware], brand: Brand) -> Live {
    let mut result = Live::new();
    for version in versions {
        if version.brand == brand.as_str() && !version.logging {
            let subaddress = (version.sub_address != 0).then_some(version.sub_address);
            result
                .entry(Target(version.address, subaddress))
                .or_default()
                .insert(version.fw_version.clone());
        }
    }
    result
}

pub(super) fn exact(
    catalog: &Catalog,
    live: &Live,
    brand: Brand,
) -> Result<BTreeSet<String>, Error> {
    let config = catalog
        .brands
        .iter()
        .find(|config| config.brand == brand)
        .ok_or(Error::Brand)?;
    let mut candidates = BTreeSet::new();
    for model in catalog.models.iter().filter(|model| model.brand == brand) {
        let compatible = model.firmware.iter().all(|expected| {
            let found = live.get(&expected.target());
            if found.is_none_or(BTreeSet::is_empty) {
                if config
                    .nonessential
                    .iter()
                    .any(|(ecu, models)| *ecu == expected.ecu && models.contains(&model.name))
                {
                    return true;
                }
                if !expected.ecu.essential() {
                    return true;
                }
            }
            expected.ecu == Ecu::Debug
                || found.is_some_and(|versions| {
                    versions
                        .iter()
                        .any(|version| expected.versions.contains(version))
                })
        });
        if compatible {
            candidates.insert(model.name.clone());
        }
    }
    Ok(candidates)
}

pub(super) fn fuzzy(catalog: &Catalog, live: &Live, brand: Brand) -> BTreeSet<String> {
    let mut lookup: BTreeMap<(Target, &[u8]), Vec<&str>> = BTreeMap::new();
    for model in catalog.models.iter().filter(|model| model.brand == brand) {
        for expected in model
            .firmware
            .iter()
            .filter(|expected| !expected.ecu.fuzzy_shared())
        {
            for version in &expected.versions {
                lookup
                    .entry((expected.target(), version))
                    .or_default()
                    .push(&model.name);
            }
        }
    }
    let mut candidate = None;
    let mut matched = BTreeSet::new();
    for (target, versions) in live {
        for version in versions {
            if let Some(models) = lookup.get(&(*target, version.as_slice())) {
                if models.len() == 1 {
                    matched.insert(*target);
                    match candidate {
                        None => candidate = Some(models[0]),
                        Some(known) => {
                            if known != models[0] {
                                return BTreeSet::new();
                            }
                        }
                    }
                }
            }
        }
    }
    match candidate {
        Some(name) if matched.len() >= 2 => BTreeSet::from([name.to_owned()]),
        Some(_) | None => BTreeSet::new(),
    }
}

pub(super) fn match_car(
    catalog: &Catalog,
    versions: &[Firmware],
    vin: &str,
    options: MatchOptions,
) -> Result<CarMatch, Error> {
    let phases = [(true, options.exact), (false, options.fuzzy)];
    for (is_exact, enabled) in phases {
        if !enabled {
            continue;
        }
        let mut matches = BTreeSet::new();
        for config in &catalog.brands {
            let versions = live(versions, config.brand);
            if is_exact {
                matches.extend(exact(catalog, &versions, config.brand)?);
            } else {
                matches.extend(fuzzy(catalog, &versions, config.brand));
                if matches.is_empty() && config.fuzzy {
                    matches.extend(custom::matches(catalog, &versions, vin, config.brand));
                }
            }
        }
        if !matches.is_empty() {
            return Ok(CarMatch {
                exact: is_exact,
                candidates: matches,
            });
        }
    }
    Ok(CarMatch {
        exact: true,
        candidates: BTreeSet::new(),
    })
}
