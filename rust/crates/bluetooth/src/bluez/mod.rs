mod agent;
mod client;
mod digits;
mod error;
mod integer;
mod objects;
mod pairing;
mod peer;
pub mod policy;
mod state;
mod wire;

pub use client::{Action, Bluez, Snapshot, SnapshotReader};
pub use error::Error;
pub use objects::{Adapter, Device};
pub use state::{Pair, Prompt};

const AGENT: &str = "/org/carrot/BluetoothAgent";
const DEVICE: &str = "org.bluez.Device1";
const ADAPTER: &str = "org.bluez.Adapter1";
