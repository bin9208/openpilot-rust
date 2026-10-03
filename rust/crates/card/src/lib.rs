//! Native card startup, CAN-driven core and explicit vehicle interface registry.
pub mod async_params;
pub mod brands;
pub mod can_wire;
pub mod core;
pub mod cruise;
pub mod ecu;
pub mod fingerprint;
pub mod firmware;
pub mod firmware_query;
pub mod identification;
pub mod isotp;
pub mod query;
mod query_io;
pub mod registry;
#[cfg(feature = "native")]
pub mod runtime;
pub mod startup;
pub mod state_helpers;
pub mod toggle;
pub mod vehicle_params;
pub mod vin;
pub mod xiaoge;
