//! Source-compatible process exit snapshots and independent retry delivery.
//! Production socket/worker wiring is added only after the owned store gates.
pub mod collector;
pub mod native;
pub mod store;
#[cfg(test)]
mod tests;
