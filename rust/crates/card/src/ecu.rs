use crate::{
    isotp,
    query::{ParallelQuery, QueryConfig, QueryIo, Target},
};
use openpilot_can::Frame;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct EcuAddress(pub u32, pub Option<u8>, pub u8);

pub struct ScanConfig<'a> {
    pub queries: &'a [EcuAddress],
    pub responses: &'a [EcuAddress],
    pub timeout: f64,
}

pub fn tester_present(target: EcuAddress) -> Frame {
    let mut data = Vec::with_capacity(8);
    if let Some(subaddress) = target.1 {
        data.push(subaddress);
    }
    data.extend_from_slice(&[2, 0x3e, 0]);
    data.resize(8, 0);
    Frame {
        address: target.0,
        data,
        bus: target.2,
    }
}

pub fn is_tester_response(frame: &Frame, subaddress: Option<u8>) -> bool {
    let offset = usize::from(subaddress.is_some());
    if !(3 + offset..=8).contains(&frame.data.len()) || !(1..=7).contains(&frame.data[offset]) {
        return false;
    }
    frame.data[offset + 1] == 0x7e
        || (frame.data[offset + 1] == 0x7f && frame.data[offset + 2] == 0x3e)
}

pub fn scan(config: ScanConfig<'_>, io: &mut impl QueryIo) -> Vec<EcuAddress> {
    let mut responses = Vec::new();
    let mut attempt = || -> Result<(), isotp::Error> {
        let mut queries = config.queries.to_vec();
        queries.sort_unstable();
        queries.dedup();
        let messages: Vec<_> = queries.into_iter().map(tester_present).collect();
        io.receive(false)?;
        io.send(&messages)?;
        let start = io.now();
        while io.now() - start < config.timeout {
            for packet in io.receive(true)? {
                for frame in packet {
                    let Some(first) = frame.data.first() else {
                        continue;
                    };
                    let subaddress =
                        if config
                            .responses
                            .contains(&EcuAddress(frame.address, None, frame.bus))
                        {
                            None
                        } else {
                            Some(*first)
                        };
                    let address = EcuAddress(frame.address, subaddress, frame.bus);
                    if config.responses.contains(&address)
                        && is_tester_response(&frame, subaddress)
                        && !responses.contains(&address)
                    {
                        responses.push(address);
                    }
                }
            }
        }
        Ok(())
    };
    match attempt() {
        Ok(()) | Err(_) => responses,
    }
}

pub struct DisableConfig<'a> {
    pub target: EcuAddress,
    pub communication_request: &'a [u8],
    pub timeout: f64,
    pub retry: usize,
}

pub fn disable(config: DisableConfig<'_>, io: &mut impl QueryIo) -> bool {
    use crate::query::DiagnosticLevel;
    let targets = [Target(config.target.0, config.target.1)];
    io.log(
        DiagnosticLevel::Warning,
        &format!(
            "ecu disable ('{:#x}', {}) ...",
            config.target.0,
            config
                .target
                .1
                .map(|value| value.to_string())
                .unwrap_or_else(|| "None".into())
        ),
    );
    for retry in 0..config.retry {
        let mut attempt = || -> Result<bool, crate::query::Error> {
            let mut session = ParallelQuery::new(QueryConfig {
                bus: config.target.2,
                targets: &targets,
                request: &[vec![0x10, 3]],
                response: &[vec![0x50, 3]],
                response_offset: 8,
                functional_addrs: &[],
                response_pending_timeout: 10.,
            })?;
            if session.get_data(config.timeout, 60., io)?.is_empty() {
                return Ok(false);
            }
            io.log(
                DiagnosticLevel::Warning,
                "communication control disable tx/rx ...",
            );
            let mut communication = ParallelQuery::new(QueryConfig {
                bus: config.target.2,
                targets: &targets,
                request: &[config.communication_request.to_vec()],
                response: &[vec![]],
                response_offset: 8,
                functional_addrs: &[],
                response_pending_timeout: 10.,
            })?;
            communication.get_data(0., 60., io)?;
            Ok(true)
        };
        match attempt() {
            Ok(result) => {
                if result {
                    io.log(DiagnosticLevel::Warning, "ecu disabled");
                    return true;
                }
            }
            Err(_) => io.log(DiagnosticLevel::Exception, "ecu disable exception"),
        }
        io.log(
            DiagnosticLevel::Error,
            &format!("ecu disable retry ({}) ...", retry + 1),
        );
    }
    io.log(DiagnosticLevel::Error, "ecu disable failed");
    false
}
