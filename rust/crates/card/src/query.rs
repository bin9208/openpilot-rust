//! Parallel diagnostic queries from `opendbc/car/isotp_parallel_query.py`.
mod run;
use crate::isotp;
use openpilot_can::Frame;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct Target(pub u32, pub Option<u8>);
impl Target {
    pub const fn new(address: u32, subaddress: Option<u8>) -> Self {
        Self(address, subaddress)
    }
}

#[derive(Clone, Copy, Debug, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticLevel {
    Warning,
    Error,
    Exception,
}

pub struct QueryConfig<'a> {
    pub bus: u8,
    pub targets: &'a [Target],
    pub request: &'a [Vec<u8>],
    pub response: &'a [Vec<u8>],
    pub response_offset: i64,
    pub functional_addrs: &'a [u32],
    pub response_pending_timeout: f64,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Functional address should be defined in functional_addrs: {0:#x}")]
    FunctionalAddress(u32),
    #[error("list index out of range")]
    RequestIndex,
    #[error(transparent)]
    Transport(#[from] isotp::Error),
}
impl Error {
    pub const fn source_exception(&self) -> &'static str {
        match self {
            Self::FunctionalAddress(_) => "AssertionError",
            Self::RequestIndex => "IndexError",
            Self::Transport(error) => error.source_exception(),
        }
    }
}

/// The caller retains CAN authority and supplies non-conflated packet batches.
pub trait QueryIo {
    fn log(&mut self, _level: DiagnosticLevel, _message: &str) {}
    fn receive(&mut self, wait_for_one: bool) -> Result<Vec<Vec<Frame>>, isotp::Error>;
    fn send(&mut self, frames: &[Frame]) -> Result<(), isotp::Error>;
    fn sleep(&mut self, seconds: f64) -> Result<(), isotp::Error>;
    fn now(&mut self) -> f64;
}

pub struct ParallelQuery {
    bus: u8,
    addresses: Vec<(Target, i64)>,
    request: Vec<Vec<u8>>,
    response: Vec<Vec<u8>>,
    functional_addrs: Vec<u32>,
    pending_timeout: f64,
}

impl ParallelQuery {
    /// Keeps source target insertion order, replacing duplicate dictionary entries.
    /// # Errors
    /// Rejects functional physical targets and invalid response addresses.
    pub fn new(config: QueryConfig<'_>) -> Result<Self, Error> {
        let mut addresses = Vec::new();
        for target in config.targets {
            if matches!(target.0, 0x7df | 0x18db33f1) {
                return Err(Error::FunctionalAddress(target.0));
            }
            let address = isotp::rx_address(target.0, config.response_offset)?
                .ok_or(Error::FunctionalAddress(target.0))?;
            if !addresses.iter().any(|(known, _)| known == target) {
                addresses.push((*target, address));
            }
        }
        Ok(Self {
            bus: config.bus,
            addresses,
            request: config.request.to_vec(),
            response: config.response.to_vec(),
            functional_addrs: config.functional_addrs.to_vec(),
            pending_timeout: config.response_pending_timeout,
        })
    }
}
