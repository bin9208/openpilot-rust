//! USB-C input-current policy from zoompilot/jetlink, MIT, Zeph Leggett (2026).
use std::io;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Cut,
    Restore,
}

#[derive(Default, Debug)]
pub struct Draw {
    pub cut: bool,
    pub settled: Option<bool>,
}

impl Draw {
    pub fn update(
        &mut self,
        _wanted: bool,
        _power_role: Option<&str>,
        _run: impl FnMut(Action) -> io::Result<()>,
    ) -> io::Result<()> {
        Ok(())
    }
}

pub fn boot_wanted(_value: Option<&[u8]>) -> bool {
    false
}
