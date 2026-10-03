mod firmware;
mod git;
use crate::{
    fingerprint,
    identification::{CachedParams, Identification},
};
use capnp::message::{Builder, HeapAllocator};
pub use git::format_git_source;
use openpilot_cereal::car_capnp::car_params;
use openpilot_params::Params;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Params(#[from] openpilot_params::Error),
    #[error(transparent)]
    Schema(#[from] capnp::Error),
    #[error(transparent)]
    Enum(#[from] capnp::NotInSchema),
    #[error(transparent)]
    Utf8(#[from] std::str::Utf8Error),
    #[error(transparent)]
    Count(#[from] std::num::TryFromIntError),
    #[error("invalid hexadecimal SecOC key")]
    SecocHex,
}

pub struct Prepared {
    pub params: Builder<HeapAllocator>,
    pub bytes: Vec<u8>,
    pub secoc_key: Option<[u8; 16]>,
    pub controller_available: bool,
    pub warnings: Vec<&'static str>,
}

pub struct Preparation<'a> {
    pub settings: &'a Params,
    pub identification: &'a Identification,
    pub message: Builder<HeapAllocator>,
    pub has_controller: bool,
    pub user_key: Option<&'a str>,
}

pub fn save_identification(
    settings: &Params,
    identification: &Identification,
) -> Result<(), Error> {
    settings.put("CarName", identification.candidate.as_bytes())?;
    settings.put(
        "FingerPrints",
        fingerprint::source_repr(&identification.observed).as_bytes(),
    )?;
    Ok(())
}

pub fn cached_params(bytes: &[u8]) -> Result<CachedParams, Error> {
    let message = capnp::serialize::read_message(
        std::io::Cursor::new(bytes),
        capnp::message::ReaderOptions::new(),
    )?;
    let cp = message.get_root::<car_params::Reader>()?;
    Ok(CachedParams {
        brand: cp.get_brand()?.to_str()?.to_owned(),
        vin: cp.get_car_vin()?.to_str()?.to_owned(),
        firmware: firmware::read(cp.get_car_fw()?)?,
    })
}

pub fn prepare(
    settings: &Params,
    identification: &Identification,
    message: Builder<HeapAllocator>,
    has_controller: bool,
    user_key: Option<&str>,
) -> Result<Prepared, Error> {
    prepare_logged(
        Preparation {
            settings,
            identification,
            message,
            has_controller,
            user_key,
        },
        |_| {},
    )
}

pub fn prepare_logged(
    input: Preparation<'_>,
    mut warning: impl FnMut(&str),
) -> Result<Prepared, Error> {
    let Preparation {
        settings,
        identification,
        mut message,
        has_controller,
        user_key,
    } = input;
    let mut warnings = Vec::new();
    decorate(identification, message.get_root::<car_params::Builder>()?)?;
    let mut cp = message.get_root::<car_params::Builder>()?;
    settings.put_bool("FirmwareQueryDone", true)?;
    cp.set_alternative_experience(i16::from(!settings.get_bool("DisengageOnAccelerator")?));
    let controller_available = has_controller
        && settings.get_bool("OpenpilotEnabledToggle")?
        && !cp.reborrow_as_reader().get_dashcam_only();
    cp.set_passive(!controller_available || cp.reborrow_as_reader().get_dashcam_only());
    if cp.reborrow_as_reader().get_passive() {
        cp.reborrow()
            .init_safety_configs(1)
            .get(0)
            .set_safety_model(car_params::SafetyModel::NoOutput);
    }
    let secoc_key = if cp.reborrow_as_reader().get_sec_oc_required() {
        if let Some(key) = user_key
            .map(str::trim)
            .filter(|key| key.chars().count() == 32)
        {
            settings.put("SecOCKey", key.as_bytes())?;
        }
        match settings.get("SecOCKey")?.filter(|bytes| !bytes.is_empty()) {
            Some(key) => {
                let bytes = parse_hex(std::str::from_utf8(&key)?.trim())?;
                let key: Option<[u8; 16]> = bytes.try_into().ok();
                if key.is_some() {
                    cp.set_sec_oc_key_available(true);
                } else {
                    let message = "Saved SecOC key is invalid";
                    warnings.push(message);
                    warning(message);
                }
                key
            }
            None => None,
        }
    } else {
        None
    };
    if let Some(previous) = settings
        .get("CarParamsPersistent")?
        .filter(|bytes| !bytes.is_empty())
    {
        settings.put("CarParamsPrevRoute", &previous)?;
    }
    let bytes = capnp::serialize::write_message_to_words(&message);
    settings.put("CarParams", &bytes)?;
    Ok(Prepared {
        params: message,
        bytes,
        secoc_key,
        controller_available,
        warnings,
    })
}

/// Source get_car decorates CarParams before the brand constructor runs.
pub fn decorate(
    identification: &Identification,
    mut cp: car_params::Builder<'_>,
) -> Result<(), Error> {
    cp.set_car_vin(&identification.vin);
    cp.set_fingerprint_source(car_params::FingerprintSource::try_from(
        identification.source as u16,
    )?);
    cp.set_fuzzy_fingerprint(!identification.exact_match);
    firmware::write(
        cp.reborrow()
            .init_car_fw(u32::try_from(identification.firmware.len())?),
        &identification.firmware,
    )?;
    Ok(())
}

fn parse_hex(text: &str) -> Result<Vec<u8>, Error> {
    let mut bytes = Vec::new();
    let mut chars = text.bytes();
    while let Some(first) = chars.next() {
        if first.is_ascii_whitespace() {
            continue;
        }
        let second = chars.next().ok_or(Error::SecocHex)?;
        let high = char::from(first).to_digit(16).ok_or(Error::SecocHex)?;
        let low = char::from(second).to_digit(16).ok_or(Error::SecocHex)?;
        bytes.push(u8::try_from(high * 16 + low)?);
    }
    Ok(bytes)
}
