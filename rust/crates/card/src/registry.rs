mod diagnostics;
mod params;
mod vehicle;
use crate::{
    brands::{
        body, chrysler, ford, gm, honda, hyundai, mazda, mock, nissan, psa, rivian, subaru, tesla,
        toyota, volkswagen,
    },
    core::Error,
    identification::Identification,
    vehicle_params,
};
use openpilot_params::Params;
pub use params::{parameter_diagnostics, parameters, parameters_logged, ParamsInput};
use std::path::Path;

pub enum Interface {
    Body(Box<body::Body>),
    Hyundai(Box<hyundai::Hyundai>),
    Mock(Box<mock::Mock>),
    Tesla(Box<tesla::Tesla>),
    Mazda(Box<mazda::Mazda>),
    Nissan(Box<nissan::Nissan>),
    Chrysler(Box<chrysler::Chrysler>),
    Rivian(Box<rivian::Rivian>),
    Ford(Box<ford::Ford>),
    Subaru(Box<subaru::Subaru>),
    Toyota(Box<toyota::Toyota>),
    Gm(Box<gm::Gm>),
    Honda(Box<honda::Honda>),
    Volkswagen(Box<volkswagen::Volkswagen>),
}

#[derive(serde::Serialize)]
pub struct ParserReadiness {
    pub counter: u32,
    pub pt_ready: bool,
    pub cam_ready: bool,
}

pub struct Setup<'a> {
    pub identification: &'a Identification,
    pub params_bytes: &'a [u8],
    pub dbc_root: &'a Path,
    pub settings: Params,
    pub now_ns: u64,
}

impl Interface {
    pub fn set_secoc_key(&mut self, key: &[u8; 16]) -> Result<(), Error> {
        if let Self::Toyota(vehicle) = self {
            vehicle.set_secoc_key(key)?;
        }
        Ok(())
    }
    pub fn constructor_diagnostics(&self) -> Result<Vec<String>, Error> {
        diagnostics::constructor(self)
    }
    pub fn parser_readiness(&self) -> Option<ParserReadiness> {
        match self {
            Self::Hyundai(vehicle) => Some(ParserReadiness {
                counter: vehicle.state.monitor.count,
                pt_ready: vehicle.state.inputs.pt.controls_ready,
                cam_ready: vehicle.state.inputs.cam.controls_ready,
            }),
            Self::Body(_)
            | Self::Mock(_)
            | Self::Tesla(_)
            | Self::Mazda(_)
            | Self::Nissan(_)
            | Self::Chrysler(_)
            | Self::Rivian(_)
            | Self::Ford(_)
            | Self::Subaru(_)
            | Self::Toyota(_)
            | Self::Gm(_)
            | Self::Honda(_)
            | Self::Volkswagen(_) => None,
        }
    }
    pub fn has_controller(&self) -> bool {
        match self {
            Self::Body(_)
            | Self::Hyundai(_)
            | Self::Tesla(_)
            | Self::Mazda(_)
            | Self::Nissan(_)
            | Self::Chrysler(_)
            | Self::Rivian(_)
            | Self::Ford(_)
            | Self::Subaru(_)
            | Self::Toyota(_)
            | Self::Gm(_)
            | Self::Honda(_)
            | Self::Volkswagen(_) => true,
            Self::Mock(_) => false,
        }
    }
    pub fn new(setup: Setup<'_>) -> Result<Self, Error> {
        let Setup {
            identification,
            params_bytes,
            dbc_root,
            settings,
            now_ns,
        } = setup;
        let platform = vehicle_params::platform(&identification.candidate)?;
        match platform.brand.as_str() {
            "body" => Ok(Self::Body(Box::new(body::Body::new(
                params_bytes,
                dbc_root,
                now_ns,
            )?))),
            "mock" => Ok(Self::Mock(Box::new(mock::Mock::new()?))),
            "hyundai" => Ok(Self::Hyundai(Box::new(hyundai::Hyundai::new(
                hyundai::Setup {
                    params_bytes,
                    dbc_root,
                    settings,
                    fingerprints: &identification.observed,
                    now_ns,
                },
            )?))),
            "tesla" => Ok(Self::Tesla(Box::new(tesla::Tesla::new(tesla::Setup {
                params_bytes,
                dbc_root,
                settings,
                fingerprints: &identification.observed,
                now_ns,
            })?))),
            "mazda" => Ok(Self::Mazda(Box::new(mazda::Mazda::new(mazda::Setup {
                params_bytes,
                dbc_root,
                settings,
                fingerprints: &identification.observed,
                now_ns,
            })?))),
            "nissan" => Ok(Self::Nissan(Box::new(nissan::Nissan::new(
                nissan::Setup {
                    params_bytes,
                    dbc_root,
                    settings,
                    fingerprints: &identification.observed,
                    now_ns,
                },
            )?))),
            "chrysler" => Ok(Self::Chrysler(Box::new(chrysler::Chrysler::new(
                chrysler::Setup {
                    params_bytes,
                    dbc_root,
                    settings,
                    fingerprints: &identification.observed,
                    now_ns,
                },
            )?))),
            "rivian" => Ok(Self::Rivian(Box::new(rivian::Rivian::new(
                rivian::Setup {
                    params_bytes,
                    dbc_root,
                    settings,
                    fingerprints: &identification.observed,
                    now_ns,
                },
            )?))),
            "ford" => Ok(Self::Ford(Box::new(ford::Ford::new(ford::Setup {
                params_bytes,
                dbc_root,
                settings,
                fingerprints: &identification.observed,
                now_ns,
            })?))),
            "subaru" => Ok(Self::Subaru(Box::new(subaru::Subaru::new(
                subaru::Setup {
                    params_bytes,
                    dbc_root,
                    settings,
                    fingerprints: &identification.observed,
                    now_ns,
                },
            )?))),
            "toyota" => Ok(Self::Toyota(Box::new(toyota::Toyota::new(
                toyota::Setup {
                    params_bytes,
                    dbc_root,
                    settings,
                    fingerprints: &identification.observed,
                    now_ns,
                },
            )?))),
            "gm" => Ok(Self::Gm(Box::new(gm::Gm::new(gm::Setup {
                params_bytes,
                dbc_root,
                settings,
                fingerprints: &identification.observed,
                now_ns,
            })?))),
            "honda" => Ok(Self::Honda(Box::new(honda::Honda::new(honda::Setup {
                params_bytes,
                dbc_root,
                settings,
                fingerprints: &identification.observed,
                now_ns,
            })?))),
            "volkswagen" => Ok(Self::Volkswagen(Box::new(volkswagen::Volkswagen::new(
                volkswagen::Setup {
                    params_bytes,
                    dbc_root,
                    settings,
                    fingerprints: &identification.observed,
                    now_ns,
                },
            )?))),
            "psa" => psa::Psa::new(psa::Setup {
                params_bytes,
                dbc_root,
                settings,
                fingerprints: &identification.observed,
                now_ns,
            })
            .map(|uninhabited| match uninhabited {})
            .map_err(Error::from),
            _ => Err(Error::UnsupportedVehicle {
                brand: platform.brand,
                candidate: identification.candidate.clone(),
            }),
        }
    }
    #[cfg(feature = "native")]
    pub fn connect_runtime(&mut self) -> Result<(), Error> {
        if let Self::Mock(vehicle) = self {
            vehicle.connect_runtime()?;
        }
        Ok(())
    }
}
