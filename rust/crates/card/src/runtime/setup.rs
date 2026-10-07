use super::{io::monotonic_ns, Common, Error, NativeIo};
use crate::{
    core::{Card, StepIo, Vehicle},
    firmware::Catalog,
    identification::{Identification, IdentifyOptions},
    registry::{self, Interface},
    startup,
};
use openpilot_cereal::car_capnp::car_params;
use openpilot_params::Params;
use std::{
    fs,
    io::{BufRead, BufReader},
    path::Path,
};

pub struct Initialized {
    pub card: Card,
    pub vehicle: Interface,
    pub common: Common,
    pub identification: Identification,
    pub tail: crate::cruise::CruiseCarrot,
}

fn environment(name: &'static str) -> Result<String, Error> {
    match std::env::var(name) {
        Ok(value) => Ok(value),
        Err(std::env::VarError::NotPresent) => Ok(String::new()),
        Err(std::env::VarError::NotUnicode(_)) => Err(Error::Environment(name)),
    }
}

fn user_key(path: &Path) -> Option<String> {
    let file = fs::File::open(path).ok()?;
    BufReader::new(file).lines().next()?.ok()
}

pub fn initialize(
    io: &mut NativeIo,
    settings: Params,
    vehicle_settings: Params,
    dbc_root: &Path,
    assets: &Path,
    numerics: &Path,
) -> Result<Initialized, Error> {
    io.print("Waiting for CAN messages...")?;
    io.wait_for_can()?;
    let alpha_long = settings.get_bool("AlphaLongitudinalEnabled")?;
    let pandas = io.wait_for_pandas()?;
    let catalog = Catalog::load()?;
    let fixed = environment("FINGERPRINT")?;
    let selected = settings.get("CarSelected3")?;
    let selected = selected.as_deref().map(std::str::from_utf8).transpose()?;
    let options = IdentifyOptions {
        fixed_fingerprint: &fixed,
        selected_car: selected,
        skip_fw_query: !environment("SKIP_FW_QUERY")?.is_empty(),
        disable_fw_cache: !environment("DISABLE_FW_CACHE")?.is_empty(),
        pandas,
    };
    let cached_bytes = settings
        .get("CarParamsCache")?
        .filter(|bytes| !bytes.is_empty());
    let cache = if let Some(bytes) = &cached_bytes {
        let message = capnp::serialize::read_message(
            std::io::Cursor::new(bytes),
            capnp::message::ReaderOptions::new(),
        )?;
        message.get_root::<car_params::Reader>()?;
        if options.skip_fw_query
            || !fixed.is_empty()
            || selected
                .and_then(|name| catalog.selected_platform(name))
                .is_some()
        {
            None
        } else {
            Some(startup::cached_params(bytes)?)
        }
    } else {
        None
    };
    let identification = catalog.identify(options, cache.as_ref(), io)?;
    io.print(&format!("SelectedCar = {}", identification.candidate))?;
    startup::save_identification(&settings, &identification)?;
    let mut params = registry::parameters_logged(
        registry::ParamsInput {
            identification: &identification,
            settings: &settings,
            alpha_long,
        },
        io,
    )?;
    for line in registry::parameter_diagnostics(
        &registry::ParamsInput {
            identification: &identification,
            settings: &settings,
            alpha_long,
        },
        &params,
        assets,
    )? {
        io.print(&line)?;
    }
    startup::decorate(&identification, params.get_root::<car_params::Builder>()?)?;
    let remote = settings.get("GitRemote")?;
    let branch = settings.get("GitBranch")?;
    let date = settings
        .get("GitCommitDate")?
        .filter(|bytes| !bytes.is_empty());
    let remote = remote.as_deref().map(std::str::from_utf8).transpose()?;
    let branch = branch.as_deref().map(std::str::from_utf8).transpose()?;
    let date = date
        .as_deref()
        .map(std::str::from_utf8)
        .transpose()?
        .unwrap_or("None");
    io.print(&format!(
        "Carrot GitBranch = {}, {date}",
        startup::format_git_source(remote, branch)
    ))?;
    let bytes = capnp::serialize::write_message_to_words(&params);
    let mut vehicle = Interface::new(registry::Setup {
        identification: &identification,
        params_bytes: &bytes,
        dbc_root,
        settings: vehicle_settings,
        now_ns: monotonic_ns(),
    })?;
    for line in vehicle.constructor_diagnostics()? {
        io.print(&line)?;
    }
    vehicle.emit_diagnostics(io)?;
    let common = Common::new(
        params.get_root_as_reader::<car_params::Reader>()?,
        &settings,
        assets,
        numerics,
    )?;
    let key = if params
        .get_root_as_reader::<car_params::Reader>()?
        .get_sec_oc_required()
    {
        user_key(Path::new("/cache/params/SecOCKey"))
    } else {
        None
    };
    let has_controller = vehicle.has_controller();
    let prepared = startup::prepare_logged(
        startup::Preparation {
            settings: &settings,
            identification: &identification,
            message: params,
            has_controller,
            user_key: key.as_deref(),
        },
        |message| io.warning(message).unwrap_or_else(drop),
    )?;
    if let Some(key) = &prepared.secoc_key {
        vehicle.set_secoc_key(key)?;
    }
    io.put_nonblocking("CarParamsCache", &prepared.bytes)?;
    io.put_nonblocking("CarParamsPersistent", &prepared.bytes)?;
    vehicle.connect_runtime()?;
    let tail = crate::cruise::CruiseCarrot::new(
        prepared.params.get_root_as_reader::<car_params::Reader>()?,
        Params::for_runtime()?,
        Path::new("/dev/shm/carrot-bluetooth"),
        super::io::monotonic(),
    )?;
    let card = Card::new(
        prepared.params,
        settings,
        std::env::var_os("REPLAY").is_some(),
        has_controller,
    )?;
    Ok(Initialized {
        card,
        vehicle,
        common,
        identification,
        tail,
    })
}
