use super::{ecus, Error, StartupIo};
use crate::{
    ecu::{self, EcuAddress, ScanConfig},
    firmware::{Brand, Catalog},
    isotp,
};

#[derive(Default)]
struct Group {
    parallel: Vec<EcuAddress>,
    serial: Vec<Vec<EcuAddress>>,
}

impl Catalog {
    pub fn present_ecus(
        &self,
        pandas: usize,
        io: &mut impl StartupIo,
    ) -> Result<Vec<EcuAddress>, Error> {
        let mut groups = [Group::default(), Group::default()];
        let mut responses = Vec::new();
        for config in &self.brands {
            for request in &config.requests {
                if usize::from(request.bus) >= pandas.saturating_mul(4) {
                    continue;
                }
                let group = &mut groups[usize::from(!request.obd_multiplexing)];
                for (ecu, target) in ecus(self, config) {
                    if !request.whitelist.is_empty() && !request.whitelist.contains(&ecu) {
                        continue;
                    }
                    let address = EcuAddress(target.0, target.1, request.bus);
                    if target.1.is_none() {
                        if !group.parallel.contains(&address) {
                            group.parallel.push(address);
                        }
                    } else if !group.serial.contains(&vec![address]) {
                        group.serial.push(vec![address]);
                    }
                    let mapped = isotp::rx_address(target.0, request.offset)?
                        .ok_or(isotp::Error::Address(target.0))?;
                    let mapped =
                        u32::try_from(mapped).map_err(|_| isotp::Error::Address(target.0))?;
                    let response = EcuAddress(mapped, target.1, request.bus);
                    if !responses.contains(&response) {
                        responses.push(response);
                    }
                }
            }
        }
        let mut result = Vec::new();
        for (enabled, mut group) in [true, false].into_iter().zip(groups) {
            io.set_obd_multiplexing(enabled)?;
            group.serial.insert(0, group.parallel);
            for queries in group.serial {
                for response in ecu::scan(
                    ScanConfig {
                        queries: &queries,
                        responses: &responses,
                        timeout: 0.1,
                    },
                    io,
                ) {
                    if !result.contains(&response) {
                        result.push(response);
                    }
                }
            }
        }
        Ok(result)
    }

    pub fn brand_matches(
        &self,
        present: &[EcuAddress],
    ) -> Result<Vec<(Brand, usize, usize)>, Error> {
        let mut result = Vec::new();
        for config in &self.brands {
            let mut responses = Vec::new();
            for request in &config.requests {
                for (ecu, target) in ecus(self, config) {
                    if !request.whitelist.is_empty() && !request.whitelist.contains(&ecu) {
                        continue;
                    }
                    let response = (
                        isotp::rx_address(target.0, request.offset)?
                            .ok_or(isotp::Error::Address(target.0))?,
                        target.1,
                    );
                    if !responses.contains(&response) {
                        responses.push(response);
                    }
                }
            }
            let found = responses
                .iter()
                .filter(|(address, subaddress)| {
                    present
                        .iter()
                        .any(|present| i64::from(present.0) == *address && present.1 == *subaddress)
                })
                .count();
            result.push((config.brand, found, responses.len()));
        }
        Ok(result)
    }
}
