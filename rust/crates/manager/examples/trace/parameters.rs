use super::{Error, Parameters, RefCell, Trace};
use serde_json::json;

pub(super) struct TracedParams {
    pub(super) inner: openpilot_params::Params,
    pub(super) trace: Trace,
    pub(super) logger: RefCell<openpilot_logging::producer::Logger>,
    pub(super) capture_logging: bool,
}
impl Parameters for TracedParams {
    fn cast_failed(&self, info: &openpilot_params::KeyInfo, value: &[u8]) -> Result<(), Error> {
        if self.capture_logging {
            self.trace
                .borrow_mut()
                .push(json!(["cast_failed", info.name]));
        }
        openpilot_manager::diagnostics::cast_failed(&mut self.logger.borrow_mut(), info, value)
    }

    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, Error> {
        Parameters::get(&self.inner, key)
    }
    fn put(&self, key: &str, value: &[u8]) -> Result<(), Error> {
        if [
            "IsOnroad",
            "IsOffroad",
            "RecordAudio",
            "LastManagerExitReason",
        ]
        .contains(&key)
        {
            self.trace
                .borrow_mut()
                .push(json!(["put", key, String::from_utf8_lossy(value)]));
        }
        Parameters::put(&self.inner, key, value)
    }
    fn clear(&self, flags: u32) -> Result<(), Error> {
        self.trace.borrow_mut().push(json!(["clear", flags]));
        Parameters::clear(&self.inner, flags)
    }
}
