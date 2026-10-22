//! Explicit incomplete PSA runtime boundary; the generic DBC codec is available.
use crate::{core::Message, firmware::Firmware, vehicle_params};
use openpilot_cereal::car_capnp::car_params;
use openpilot_params::Params;
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Baseline(#[from] vehicle_params::Error),
    #[error(transparent)]
    Schema(#[from] capnp::Error),
    #[error(transparent)]
    Utf8(#[from] std::str::Utf8Error),
    #[error("unknown PSA platform: {0}")]
    Platform(String),
    #[error("PSA DBC loading failed at {path}: {source}")]
    DbcLoad {
        path: PathBuf,
        source: openpilot_can::Error,
    },
    #[error("pinned PSA source has no CarState method: {0}")]
    SourceMethod(&'static str),
    #[error(
        "PSA state/controller runtime is not ported; DBC availability does not admit the vehicle"
    )]
    RuntimeUnported,
}

pub struct ParamsInput<'a> {
    pub candidate: &'a str,
    pub fingerprints: &'a [(u8, Vec<(u32, usize)>)],
    pub firmware: &'a [Firmware],
    pub alpha_long: bool,
    pub settings: &'a Params,
}

pub fn parameters(input: ParamsInput<'_>) -> Result<Message, Error> {
    if input.candidate != "PSA_PEUGEOT_208" {
        return Err(Error::Platform(input.candidate.to_owned()));
    }
    // get_std_params reads torque before brand parameters or any Params writes.
    vehicle_params::baseline(input.candidate)?;
    Err(Error::RuntimeUnported)
}

pub struct Setup<'a> {
    pub params_bytes: &'a [u8],
    pub dbc_root: &'a Path,
    pub settings: Params,
    pub fingerprints: &'a [(u8, Vec<(u32, usize)>)],
    pub now_ns: u64,
}

pub enum Psa {}
impl Psa {
    pub fn new(input: Setup<'_>) -> Result<Self, Error> {
        let cp = capnp::serialize::read_message(
            &mut std::io::Cursor::new(input.params_bytes),
            capnp::message::ReaderOptions::new(),
        )?;
        let candidate = cp
            .get_root::<car_params::Reader>()?
            .get_car_fingerprint()?
            .to_str()?;
        if candidate != "PSA_PEUGEOT_208" {
            return Err(Error::Platform(candidate.to_owned()));
        }
        let path = input.dbc_root.join("psa_aee2010_r3.dbc");
        // Validate the asset without silently admitting the unimplemented runtime.
        openpilot_can::dbc::Dbc::load(&path).map_err(|error| Error::DbcLoad {
            path,
            source: error,
        })?;
        Err(Error::RuntimeUnported)
    }
}

#[derive(Default)]
pub struct State;
impl State {
    pub fn new() -> Result<Self, Error> {
        Ok(Self)
    }

    pub fn update(&self) -> Result<Message, Error> {
        // Python resolves this missing method before evaluating its CAN arguments.
        Err(Error::SourceMethod("parse_wheel_speeds"))
    }
}
