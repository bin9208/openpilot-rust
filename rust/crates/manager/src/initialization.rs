use crate::{
    parameters::{set_defaults, Parameters},
    Error,
};
use openpilot_runtime_version::{BuildMetadata, JsonValue};
use std::{collections::BTreeMap, path::Path};

/// External startup boundaries retain source ordering and exception scope.
/// Implementations must call native bootlog/registration/reporting adapters.
pub trait Startup {
    fn save_bootlog(&mut self) -> Result<(), Error>;
    fn build_metadata(&mut self) -> Result<BuildMetadata, Error>;
    fn checkout_status(&mut self) -> Result<(), Error>;
    fn serial(&mut self) -> Result<String, Error>;
    fn register(&mut self) -> Result<String, Error>;
    fn initialize_logging(&mut self, metadata: &BuildMetadata, dongle: &str) -> Result<(), Error>;
    fn prepare_processes(&mut self) -> Result<(), Error>;
    fn supported_cars(&mut self, brand: &str) -> Result<Vec<String>, Error>;
    fn exception(&mut self, message: &str, error: &Error) -> Result<(), Error>;
    fn release_boot_lock(&mut self) -> Result<(), Error>;
}

pub fn initialize(
    params: &impl Parameters,
    startup: &mut impl Startup,
    paths: &InitPaths<'_>,
) -> Result<BTreeMap<String, String>, Error> {
    startup.save_bootlog()?;
    let metadata = startup.build_metadata()?;
    startup.checkout_status()?;
    for flags in [
        openpilot_params::CLEAR_ON_MANAGER_START,
        openpilot_params::CLEAR_ON_ONROAD_TRANSITION,
        openpilot_params::CLEAR_ON_OFFROAD_TRANSITION,
        openpilot_params::CLEAR_ON_IGNITION_ON,
    ] {
        params.clear(flags)?;
    }
    if metadata.release_channel() {
        params.clear(openpilot_params::DEVELOPMENT_ONLY)?;
    }
    if params.boolean("RecordFrontLock")? {
        params.put_bool("RecordFront", true)?;
    }
    set_defaults(params, false)?;
    let mut environment = BTreeMap::new();
    let wide = params
        .get("UseWideCamera")?
        .unwrap_or_else(|| b"1".to_vec())
        == b"1";
    if !wide {
        set_environment(&mut environment, "DISABLE_WIDE_ROAD", "1".into())?;
    }
    match std::fs::create_dir(paths.shm) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            println!("WARNING: failed to make {}", paths.shm.display());
        }
        Err(error) => return Err(error.into()),
    }
    let serial = startup.serial()?;
    for (key, value) in [
        ("Version", &metadata.openpilot.version),
        ("GitCommit", &metadata.openpilot.git_commit),
        ("GitCommitDate", &metadata.openpilot.git_commit_date),
        ("GitBranch", &metadata.channel),
        ("GitRemote", &metadata.openpilot.git_origin),
    ] {
        params.put(key, string(value)?.as_bytes())?;
    }
    params.put_bool("IsTestedBranch", metadata.tested_channel())?;
    params.put_bool("IsReleaseBranch", metadata.release_channel())?;
    params.put("HardwareSerial", serial.as_bytes())?;
    let dongle = startup.register()?;
    if dongle.is_empty() {
        return Err(Error::Contract("registration returned an empty identity"));
    }
    set_environment(&mut environment, "DONGLE_ID", dongle.clone())?;
    set_environment(
        &mut environment,
        "GIT_ORIGIN",
        string(&metadata.openpilot.git_normalized_origin()?)?,
    )?;
    set_environment(&mut environment, "GIT_BRANCH", string(&metadata.channel)?)?;
    set_environment(
        &mut environment,
        "GIT_COMMIT",
        string(&metadata.openpilot.git_commit)?,
    )?;
    if !metadata.openpilot.is_dirty {
        set_environment(&mut environment, "CLEAN", "1".into())?;
    }
    startup.initialize_logging(&metadata, &dongle)?;
    startup.prepare_processes()?;
    Ok(environment)
}
fn set_environment(
    environment: &mut BTreeMap<String, String>,
    key: &str,
    value: String,
) -> Result<(), Error> {
    if value.contains('\0') {
        return Err(Error::Contract("environment value contains NUL"));
    }
    std::env::set_var(key, &value);
    environment.insert(key.into(), value);
    Ok(())
}
fn string(value: &JsonValue) -> Result<String, Error> {
    value.to_utf8().ok_or(Error::Contract(
        "metadata Params/environment value is not UTF-8 STRING",
    ))
}
pub struct InitPaths<'a> {
    pub shm: &'a Path,
    pub params: &'a Path,
}

pub const SUPPORTED_CARS: &[(&str, &str)] = &[
    ("hyundai", "SupportedCars"),
    ("gm", "SupportedCars_gm"),
    ("toyota", "SupportedCars_toyota"),
    ("mazda", "SupportedCars_mazda"),
    ("ford", "SupportedCars_ford"),
    ("volkswagen", "SupportedCars_vw"),
    ("tesla", "SupportedCars_tesla"),
];

pub fn write_supported_cars(startup: &mut impl Startup, directory: &Path) -> Result<(), Error> {
    for &(brand, filename) in SUPPORTED_CARS {
        let outcome = (|| {
            let mut cars = startup.supported_cars(brand)?;
            cars.sort();
            let mut text = cars.join("\n");
            if !cars.is_empty() {
                text.push('\n');
            }
            std::fs::write(directory.join(filename), text)?;
            Ok(())
        })();
        if let Err(error) = outcome {
            startup.exception(
                &format!("failed to write {filename} from opendbc.car.{brand}.values"),
                &error,
            )?;
        }
    }
    Ok(())
}

pub fn initialize_main(
    params: &impl Parameters,
    startup: &mut impl Startup,
    paths: &InitPaths<'_>,
) -> Result<(), Error> {
    let result = initialize(params, startup, paths)
        .and_then(|_| write_supported_cars(startup, paths.params));
    startup.release_boot_lock()?;
    result.map(|_| ())
}
