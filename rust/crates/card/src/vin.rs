use crate::{
    isotp::rx_address,
    query::{ParallelQuery, QueryConfig, QueryIo, Target},
};
use serde::Serialize;

pub const UNKNOWN: &str = "00000000000000000";
pub const STANDARD_ADDRESSES: &[u32] = &[0x7e0, 0x7e2, 0x760, 0x7c6, 0x18da10f1, 0x18da0ef1];

pub fn valid(value: &str) -> bool {
    value.len() == 17
        && value.bytes().all(
            |byte| matches!(byte, b'A'..=b'H' | b'J'..=b'N' | b'P' | b'R'..=b'Z' | b'0'..=b'9'),
        )
}

pub fn decode(mut value: &[u8]) -> Option<String> {
    while value.first().is_some_and(|byte| matches!(byte, 0 | 255)) {
        value = &value[1..];
    }
    while value.last().is_some_and(|byte| matches!(byte, 0 | 255)) {
        value = &value[..value.len() - 1];
    }
    if value.first() == Some(&0x11) {
        value = &value[1..value.len().min(18)];
    }
    if !value.is_ascii() {
        return None;
    }
    let text = std::str::from_utf8(value).ok()?;
    let result = text
        .trim_matches(|character: char| {
            character.is_ascii_whitespace() || ('\x1c'..='\x1f').contains(&character)
        })
        .to_ascii_uppercase();
    valid(&result).then_some(result)
}

pub struct VinConfig<'a> {
    pub buses: &'a [u8],
    pub timeout: f64,
    pub retry: usize,
}

#[derive(Debug, Serialize)]
pub struct VinResult {
    pub address: Option<i64>,
    pub bus: Option<u8>,
    pub vin: String,
}

struct Request {
    request: &'static [u8],
    response: &'static [u8],
    buses: &'static [u8],
    addresses: &'static [u32],
    functional: &'static [u32],
    offset: i64,
}
const REQUESTS: &[Request] = &[
    Request {
        request: &[0x22, 0xf1, 0x90],
        response: &[0x62, 0xf1, 0x90],
        buses: &[0, 1],
        addresses: STANDARD_ADDRESSES,
        functional: &[0x7df, 0x18db33f1],
        offset: 8,
    },
    Request {
        request: &[0x09, 0x02],
        response: &[0x49, 0x02, 0x01],
        buses: &[0, 1],
        addresses: STANDARD_ADDRESSES,
        functional: &[0x7df, 0x18db33f1],
        offset: 8,
    },
    Request {
        request: &[0x1a, 0x90],
        response: &[0x5a, 0x90],
        buses: &[0],
        addresses: &[0x24b],
        functional: &[],
        offset: 0x400,
    },
    Request {
        request: &[0x21, 0x81],
        response: &[0x61, 0x81],
        buses: &[0],
        addresses: &[0x797],
        functional: &[],
        offset: 3,
    },
    Request {
        request: &[0x22, 0xf1, 0x90],
        response: &[0x62, 0xf1, 0x90],
        buses: &[0],
        addresses: &[0x74f],
        functional: &[],
        offset: 0x6a,
    },
    Request {
        request: &[0x22, 0xf1, 0x90],
        response: &[0x62, 0xf1, 0x90],
        buses: &[0],
        addresses: &[0x733],
        functional: &[],
        offset: 0x40,
    },
];

pub fn query(config: VinConfig<'_>, io: &mut impl QueryIo) -> VinResult {
    for _ in 0..config.retry {
        for bus in config.buses {
            for request in REQUESTS {
                if !request.buses.contains(bus) {
                    continue;
                }
                let targets: Vec<_> = if request.functional.is_empty() {
                    request
                        .addresses
                        .iter()
                        .map(|address| Target(*address, None))
                        .collect()
                } else {
                    (0x700..0x800)
                        .filter(|address| *address != 0x7df)
                        .chain((0x18da00f1..0x18db00f1).step_by(0x100))
                        .map(|address| Target(address, None))
                        .collect()
                };
                let attempt = ParallelQuery::new(QueryConfig {
                    bus: *bus,
                    targets: &targets,
                    request: &[request.request.to_vec()],
                    response: &[request.response.to_vec()],
                    response_offset: request.offset,
                    functional_addrs: request.functional,
                    response_pending_timeout: 10.,
                })
                .and_then(|mut query| query.get_data(config.timeout, 60., io));
                let results = match attempt {
                    Ok(results) => results,
                    Err(_) => continue,
                };
                for address in request.addresses {
                    if let Some((_, data)) = results
                        .iter()
                        .find(|(target, _)| *target == Target(*address, None))
                    {
                        if let Some(vin) = decode(data) {
                            match rx_address(*address, request.offset) {
                                Ok(address) => {
                                    return VinResult {
                                        address,
                                        bus: Some(*bus),
                                        vin,
                                    }
                                }
                                Err(_) => continue,
                            }
                        }
                    }
                }
            }
        }
    }
    VinResult {
        address: None,
        bus: None,
        vin: UNKNOWN.to_owned(),
    }
}
