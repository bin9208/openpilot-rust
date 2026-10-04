mod engine;
pub use engine::Engine;

use crate::{
    batch::Ego,
    data::Data,
    decoder::{Config, Interface},
    Error,
};
use openpilot_can::Packet;
use serde::Serialize;

pub trait Io {
    fn monotonic_ns(&mut self) -> u64;
    fn create_interface(&mut self, config: &Config) -> Result<Interface, Error>;
    fn track_flip(&mut self) -> Result<bool, Error>;
    fn update(
        &mut self,
        state: &mut Interface,
        ego: Ego,
        packets: &[Packet],
    ) -> Result<Option<Data>, Error>;
    fn publish(&mut self, data: Data, valid: bool) -> Result<(), Error>;
    fn input_error(&mut self, reason: Reason) -> Result<(), Error>;
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Reason {
    Batch(crate::batch::Reason),
    CanTimeout,
    ProcessingTimeout,
    InputTimeout,
}

impl std::fmt::Display for Reason {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = match self {
            Self::Batch(crate::batch::Reason::StateOverflow) => "stateOverflow",
            Self::Batch(crate::batch::Reason::MissingBatchMetadata) => "missingBatchMetadata",
            Self::Batch(crate::batch::Reason::StaleEgoState) => "staleEgoState",
            Self::Batch(crate::batch::Reason::InvalidEmptyBatch) => "invalidEmptyBatch",
            Self::Batch(crate::batch::Reason::InvalidBatchMetadata) => "invalidBatchMetadata",
            Self::Batch(crate::batch::Reason::MissingCanPacket) => "missingCanPacket",
            Self::CanTimeout => "canTimeout",
            Self::ProcessingTimeout => "processingTimeout",
            Self::InputTimeout => "inputTimeout",
        };
        formatter.write_str(text)
    }
}

#[derive(Default, Serialize)]
pub struct Metrics {
    pub processed_batches: u64,
    #[serde(serialize_with = "crate::scalar::serialize_float")]
    pub input_age_ms: f64,
    pub invalid: bool,
    pub pending_states: usize,
    pub pending_can: usize,
}
