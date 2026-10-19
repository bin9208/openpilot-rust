//! USB-C input-current policy from zoompilot/jetlink, MIT, Zeph Leggett (2026).
use std::{fs, io, path::Path};

pub const VOTER: &str = "/sys/kernel/debug/pmic-votable/USB_ICL";
pub const POWER_ROLE: &str = "/sys/class/usbpd/usbpd0/current_pr";

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
        wanted: bool,
        power_role: Option<&str>,
        mut run: impl FnMut(Action) -> io::Result<()>,
    ) -> io::Result<()> {
        if self.settled == Some(wanted) || (!wanted && power_role != Some("none")) {
            return Ok(());
        }
        let result = run(if wanted { Action::Cut } else { Action::Restore });
        self.cut = wanted && result.is_ok();
        // The upstream owner reports a failed helper once, rather than spawning
        // sudo again every500ms. A later setting transition permits another try.
        self.settled = Some(wanted);
        result
    }
}

pub fn boot_wanted(value: Option<&[u8]>) -> bool {
    let Some(mut value) = value else { return false };
    // Bash command substitution in cmd_boot removes trailing newlines only.
    while let Some(rest) = value.strip_suffix(b"\n") {
        value = rest;
    }
    !value.is_empty() && value != b"0"
}

pub fn write_voter(
    action: Action,
    mut write: impl FnMut(&str, &[u8]) -> io::Result<()>,
) -> io::Result<()> {
    match action {
        Action::Cut => {
            write("force_val", b"0\n")?;
            write("force_active", b"1\n")
        }
        Action::Restore => {
            write("force_active", b"0\n")?;
            write("force_val", b"0\n")
        }
    }
}

/// The privileged callers supply no user-controlled sysfs path.
pub fn apply(action: Action) -> io::Result<()> {
    write_voter(action, |name, value| {
        fs::write(Path::new(VOTER).join(name), value)
    })
}
