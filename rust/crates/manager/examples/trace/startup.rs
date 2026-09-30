use super::{Error, Fixture, Parameters};
use openpilot_manager::initialization::Startup;
use openpilot_runtime_version::{BuildMetadata, JsonValue};
use serde_json::json;

impl Startup for Fixture {
    fn save_bootlog(&mut self) -> Result<(), Error> {
        self.record(json!(["bootlog"]));
        Ok(())
    }
    fn build_metadata(&mut self) -> Result<BuildMetadata, Error> {
        self.record(json!(["metadata"]));
        Ok(openpilot_runtime_version::build_metadata_from_dict(&JsonValue::parse(r#"{"channel":"release-tizi","openpilot":{"version":"fixture","git_commit":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","git_commit_date":"date","git_origin":"https://github.com/commaai/openpilot.git","build_style":"release"}}"#).unwrap())?)
    }
    fn checkout_status(&mut self) -> Result<(), Error> {
        self.record(json!(["checkout"]));
        Ok(())
    }
    fn serial(&mut self) -> Result<String, Error> {
        self.record(json!(["serial"]));
        Ok("fixture-serial".into())
    }
    fn register(&mut self) -> Result<String, Error> {
        self.record(json!(["register"]));
        if self.scenario == "registration_failure" {
            return Ok(String::new());
        }
        self.params.put("DongleId", b"fixture-id")?;
        Ok("fixture-id".into())
    }
    fn initialize_logging(&mut self, _: &BuildMetadata, _: &str) -> Result<(), Error> {
        self.record(json!(["logging"]));
        Ok(())
    }
    fn prepare_processes(&mut self) -> Result<(), Error> {
        self.record(json!(["prepare"]));
        Ok(())
    }
    fn supported_cars(&mut self, brand: &str) -> Result<Vec<String>, Error> {
        self.record(json!(["cars", brand]));
        if brand == "gm" {
            return Err(Error::Contract("fixture cars failure"));
        }
        Ok(vec!["Z car".into(), "A car".into()])
    }
    fn exception(&mut self, message: &str, _: &Error) -> Result<(), Error> {
        self.record(json!(["exception", message]));
        Ok(())
    }
    fn release_boot_lock(&mut self) -> Result<(), Error> {
        self.record(json!(["unlock"]));
        Ok(())
    }
}
