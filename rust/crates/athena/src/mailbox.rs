use crate::Error;
use std::{
    collections::VecDeque,
    sync::{Condvar, Mutex},
    time::Duration,
};

pub struct Mailbox<T> {
    values: Mutex<VecDeque<T>>,
    available: Condvar,
}
impl<T> Default for Mailbox<T> {
    fn default() -> Self {
        Self {
            values: Mutex::new(VecDeque::new()),
            available: Condvar::new(),
        }
    }
}
impl<T> Mailbox<T> {
    pub fn put(&self, value: T) -> Result<(), Error> {
        self.values
            .lock()
            .map_err(|_| Error::Contract("mailbox poisoned"))?
            .push_back(value);
        self.available.notify_one();
        Ok(())
    }
    pub fn get(&self, timeout: Duration) -> Result<Option<T>, Error> {
        let mut values = self
            .values
            .lock()
            .map_err(|_| Error::Contract("mailbox poisoned"))?;
        if values.is_empty() && !timeout.is_zero() {
            values = self
                .available
                .wait_timeout_while(values, timeout, |values| values.is_empty())
                .map_err(|_| Error::Contract("mailbox poisoned"))?
                .0;
        }
        Ok(values.pop_front())
    }
}
