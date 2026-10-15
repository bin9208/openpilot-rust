//! Source-compatible process exit snapshots and independent retry delivery.
//! Production socket/worker wiring is added only after the owned store gates.
pub mod store;
pub mod collector;
pub mod native;
#[cfg(test)]
mod tests;
