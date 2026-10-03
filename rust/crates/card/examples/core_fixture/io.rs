use super::{Output, Publication, Step};
use openpilot_can::Frame;
use openpilot_card::{
    core::{Error, StepIo, SERVICES},
    firmware_query::StartupIo,
    isotp,
    query::QueryIo,
};
use openpilot_messaging::state::{Options, State};
use openpilot_params::Params;

pub struct Io {
    pub state: State,
    pub step: Step,
    pub output: Output,
    settings: Params,
}
impl Io {
    pub fn new(step: Step, settings: Params) -> Result<Self, Error> {
        Ok(Self {
            state: State::new(&SERVICES, Options::default())?,
            step,
            output: Output::default(),
            settings,
        })
    }
}
impl QueryIo for Io {
    fn receive(&mut self, _: bool) -> Result<Vec<Vec<Frame>>, isotp::Error> {
        Ok(vec![])
    }
    fn send(&mut self, _: &[Frame]) -> Result<(), isotp::Error> {
        Ok(())
    }
    fn sleep(&mut self, _: f64) -> Result<(), isotp::Error> {
        Ok(())
    }
    fn now(&mut self) -> f64 {
        self.step.now as f64 / 1e9
    }
}
impl StartupIo for Io {
    fn set_obd_multiplexing(&mut self, _: bool) -> Result<(), isotp::Error> {
        Ok(())
    }
}
impl StepIo for Io {
    fn put_nonblocking(&mut self, key: &str, bytes: &[u8]) -> Result<(), Error> {
        self.output
            .param_writes
            .push((key.to_owned(), bytes.to_vec()));
        self.settings.put(key, bytes)?;
        Ok(())
    }
    fn receive_can_raw(&mut self) -> Result<Vec<Vec<u8>>, Error> {
        Ok(self.step.can.clone())
    }
    fn update_subscribers(&mut self) -> Result<(), Error> {
        Ok(self
            .state
            .update(self.step.now as f64 / 1e9, &self.step.messages)?)
    }
    fn subscribers(&self) -> &State {
        &self.state
    }
    fn monotonic_ns(&mut self) -> u64 {
        self.step.now
    }
    fn thread_cpu_ns(&mut self) -> u64 {
        0
    }
    fn publish(&mut self, topic: &str, bytes: &[u8]) -> Result<(), Error> {
        self.output.publications.push(Publication {
            topic: topic.into(),
            wire: bytes.to_vec(),
        });
        Ok(())
    }
    fn warning(&mut self, message: &str) -> Result<(), Error> {
        self.output.warnings.push(message.into());
        Ok(())
    }
    fn diagnostics(&mut self, values: &[(&'static str, f64)]) -> Result<(), Error> {
        self.output.diagnostics = values.to_vec();
        Ok(())
    }
}
