mod activity;
mod clicks;
mod config;
mod decoder;
mod engine;
mod events;
mod files;
mod gates;
mod holds;
mod input;
#[expect(
    unsafe_code,
    reason = "audited Linux evdev ioctl boundary with native ABI tests"
)]
mod input_kernel;
mod input_permissions;
mod journal;
mod learning;
mod token;
mod touch;
mod types;

pub use config::{Address, Config, ConfigError, Device, Name};
pub use decoder::Decoder;
pub use engine::{Engine, Reload};
pub use files::{atomic_json, atomic_value, Error};
pub use gates::{Gates, VehicleSnapshot};
pub use input::{decode_events, enumerate, Input, InputBatch, InputError};
pub use journal::{Channel, CommandWriter, Intent};
pub use types::{Action, Event, Mapping, Profile, Seconds, Token};
