mod presence;
use crate::{
    firmware::{Brand, BrandConfig, Catalog, Ecu, Firmware, MatchOptions},
    isotp,
    query::{ParallelQuery, QueryConfig, QueryIo, Target},
};
use std::collections::BTreeSet;

pub trait StartupIo: QueryIo {
    fn identification_event(&mut self, _event: crate::identification::Event<'_>) {}
    fn set_obd_multiplexing(&mut self, enabled: bool) -> Result<(), isotp::Error>;
}
pub struct QueryOptions {
    pub brand: Option<Brand>,
    pub pandas: usize,
    pub timeout: f64,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Transport(#[from] isotp::Error),
    #[error(transparent)]
    Match(#[from] crate::firmware::Error),
    #[error("firmware query brand has no response ECUs")]
    EmptyBrand,
}

pub(super) fn ecus(catalog: &Catalog, config: &BrandConfig) -> Vec<(Ecu, Target)> {
    let mut result = BTreeSet::new();
    for expected in catalog
        .models
        .iter()
        .filter(|model| model.brand == config.brand)
        .flat_map(|model| &model.firmware)
        .chain(&config.extra)
    {
        result.insert((expected.ecu, expected.target()));
    }
    result.into_iter().collect()
}

impl Catalog {
    pub fn query_firmware(
        &self,
        options: QueryOptions,
        io: &mut impl StartupIo,
    ) -> Result<Vec<Firmware>, Error> {
        let mut parallel = Vec::new();
        let mut serial = Vec::new();
        let mut types = Vec::new();
        for config in self
            .brands
            .iter()
            .filter(|config| options.brand.is_none_or(|brand| config.brand == brand))
        {
            for (ecu, target) in ecus(self, config) {
                let key = (config.brand, target);
                if !types.iter().any(|(known, _)| *known == key) {
                    types.push((key, ecu));
                }
                if target.1.is_none() {
                    if !parallel.contains(&key) {
                        parallel.push(key);
                    }
                } else if !serial.contains(&vec![key]) {
                    serial.push(vec![key]);
                }
            }
        }
        serial.insert(0, parallel);
        let mut firmware = Vec::new();
        for group in serial {
            for chunk in group.chunks(128) {
                for config in self
                    .brands
                    .iter()
                    .filter(|config| options.brand.is_none_or(|brand| config.brand == brand))
                {
                    for request in &config.requests {
                        if usize::from(request.bus) >= options.pandas.saturating_mul(4) {
                            continue;
                        }
                        if request.bus % 4 == 1 {
                            io.set_obd_multiplexing(request.obd_multiplexing)?;
                        }
                        let targets: Vec<_> = chunk
                            .iter()
                            .filter(|(brand, target)| {
                                *brand == config.brand
                                    && (request.whitelist.is_empty()
                                        || types.iter().any(
                                            |((known_brand, known_target), ecu)| {
                                                *known_brand == *brand
                                                    && *known_target == *target
                                                    && request.whitelist.contains(ecu)
                                            },
                                        ))
                            })
                            .map(|(_, target)| *target)
                            .collect();
                        if targets.is_empty() {
                            continue;
                        }
                        let attempt = ParallelQuery::new(QueryConfig {
                            bus: request.bus,
                            targets: &targets,
                            request: &request.request,
                            response: &request.response,
                            response_offset: request.offset,
                            functional_addrs: &[],
                            response_pending_timeout: 10.,
                        })
                        .and_then(|mut query| query.get_data(options.timeout, 60., io));
                        let results = match attempt {
                            Ok(results) => results,
                            Err(_) => continue,
                        };
                        for (target, version) in results {
                            let ecu = types
                                .iter()
                                .find(|((brand, address), _)| {
                                    *brand == config.brand && *address == target
                                })
                                .map_or(Ecu::Unknown, |(_, ecu)| *ecu);
                            let response_address =
                                match isotp::rx_address(target.0, request.offset)? {
                                    Some(address) => match u32::try_from(address) {
                                        Ok(address) => address,
                                        Err(_) => break,
                                    },
                                    None => break,
                                };
                            firmware.push(Firmware {
                                ecu,
                                fw_version: version,
                                address: target.0,
                                response_address,
                                request: request.request.clone(),
                                brand: config.brand.as_str().to_owned(),
                                bus: request.bus,
                                logging: request.logging
                                    || config
                                        .extra
                                        .iter()
                                        .any(|extra| extra.ecu == ecu && extra.target() == target),
                                obd_multiplexing: request.obd_multiplexing,
                                sub_address: target.1.unwrap_or(0),
                            });
                        }
                    }
                }
            }
        }
        Ok(firmware)
    }

    pub fn query_ordered(
        &self,
        present: &[crate::ecu::EcuAddress],
        options: (&str, usize, f64),
        io: &mut impl StartupIo,
    ) -> Result<Vec<Firmware>, Error> {
        let (vin, pandas, timeout) = options;
        let scores = self.brand_matches(present)?;
        let mut ranked: Vec<_> = scores
            .into_iter()
            .enumerate()
            .map(|(index, (brand, found, total))| (brand, found, total, index))
            .collect();
        if ranked.iter().any(|(_, _, total, _)| *total == 0) {
            return Err(Error::EmptyBrand);
        }
        ranked.sort_by(|left, right| {
            right
                .1
                .cmp(&left.1)
                .then_with(|| (right.1 * left.2).cmp(&(left.1 * right.2)))
                .then_with(|| left.3.cmp(&right.3))
        });
        let mut result = Vec::new();
        for (brand, found, _, _) in ranked {
            if found == 0 {
                continue;
            }
            let firmware = self.query_firmware(
                QueryOptions {
                    brand: Some(brand),
                    pandas,
                    timeout,
                },
                io,
            )?;
            let matches = self.match_car(
                &firmware,
                vin,
                MatchOptions {
                    exact: true,
                    fuzzy: true,
                },
            )?;
            result.extend(firmware);
            if matches.candidates.len() == 1 {
                break;
            }
        }
        Ok(result)
    }
}
