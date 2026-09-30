use openpilot_logging::{
    producer::{Delivery, Logger},
    record::Record,
    site::Site,
    Error,
};
use std::{cell::RefCell, rc::Rc};

#[derive(Clone)]
pub struct ProcessLog(Rc<RefCell<Logger>>);

impl ProcessLog {
    pub fn new(logger: Logger) -> Self {
        Self(Rc::new(RefCell::new(logger)))
    }

    pub(crate) fn emit(&self, site: Site, record: Record) -> Result<Delivery, Error> {
        self.0
            .try_borrow_mut()
            .map_err(|_| Error::Contract("process log is already borrowed"))?
            .emit(site, record)
    }
}
