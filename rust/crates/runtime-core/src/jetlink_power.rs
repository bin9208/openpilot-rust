//! Carrot tools/jetlink/usb_power.py: root-only USB input-current guard.
//! Never changes USB roles and never restores draw; the kernel clears the
//! forced vote at reboot.
use std::{fs, io, path::Path};

pub const VOTER: &str = "/sys/kernel/debug/pmic-votable/USB_ICL";
pub const POWER_ROLE: &str = "/sys/class/usbpd/usbpd0/current_pr";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cut {
    Applied,
    /// The port was attached while the helper ran; nothing was written.
    Deferred,
}

fn read(path: &Path) -> io::Result<String> {
    Ok(fs::read_to_string(path)?.trim().to_owned())
}

/// Zero the USB input-current vote only while the port is empty, re-checking
/// around every write because process startup can overlap an attachment.
pub fn cut_input(role: &Path, voter: &Path) -> io::Result<Cut> {
    let empty = || read(role).map(|value| value == "none");
    if !empty()? {
        return Ok(Cut::Deferred);
    }
    let active = voter.join("force_active");
    let value = voter.join("force_val");
    let forced = read(&active)?;
    if forced == "1" && read(&value)? != "0" {
        return Err(io::Error::other(
            "USB_ICL has an existing nonzero forced vote",
        ));
    }
    if forced != "0" && forced != "1" {
        return Err(io::Error::other("unknown USB_ICL force_active value"));
    }
    if !value.is_file() {
        return Err(io::Error::other("USB_ICL force_val is missing"));
    }
    if !empty()? {
        return Ok(Cut::Deferred);
    }
    fs::write(&value, b"0\n")?;
    if !empty()? {
        return Ok(Cut::Deferred);
    }
    fs::write(&active, b"1\n")?;
    if read(&active)? != "1" || read(&value)? != "0" {
        return Err(io::Error::other(
            "USB_ICL zero-current vote did not read back",
        ));
    }
    Ok(Cut::Applied)
}

/// Carrot UsbInputPower: once the port has been empty for 1.5 s, cut input
/// current one time; never undo on retry, mode change or exit.
#[derive(Default, Debug)]
pub struct InputPower {
    empty_since: Option<f64>,
    pub finished: bool,
}

impl InputPower {
    pub const EMPTY_SECONDS: f64 = 1.5;

    /// `cut` returns Ok(true) when applied, Ok(false) when deferred.
    pub fn step(
        &mut self,
        now: f64,
        power_role: Option<&str>,
        cut: impl FnOnce() -> io::Result<bool>,
    ) {
        if self.finished {
            return;
        }
        if power_role != Some("none") {
            self.empty_since = None;
            return;
        }
        let since = *self.empty_since.get_or_insert(now);
        if now - since < Self::EMPTY_SECONDS {
            return;
        }
        match cut() {
            Ok(false) => {
                self.empty_since = None;
                return;
            }
            Ok(true) => eprintln!("Jetlink USB power: zero input-current vote verified; retained until reboot"),
            Err(error) => eprintln!(
                "Jetlink USB power: could not verify protection; no automatic retry until daemon restart: {error}"
            ),
        }
        self.finished = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cuts_only_on_an_empty_port_and_reads_back() {
        let root = tempfile::tempdir().unwrap();
        let role = root.path().join("role");
        let voter = root.path().join("voter");
        fs::create_dir(&voter).unwrap();
        fs::write(voter.join("force_active"), "0\n").unwrap();
        fs::write(voter.join("force_val"), "500000\n").unwrap();
        fs::write(&role, "sink\n").unwrap();
        assert_eq!(cut_input(&role, &voter).unwrap(), Cut::Deferred);
        fs::write(&role, "none\n").unwrap();
        assert_eq!(cut_input(&role, &voter).unwrap(), Cut::Applied);
        assert_eq!(
            fs::read_to_string(voter.join("force_active")).unwrap(),
            "1\n"
        );
    }

    #[test]
    fn debounces_and_finishes_once() {
        let mut power = InputPower::default();
        let mut calls = 0;
        power.step(0.0, Some("none"), || unreachable!());
        power.step(1.0, Some("sink"), || unreachable!());
        power.step(2.0, Some("none"), || unreachable!());
        power.step(3.6, Some("none"), || {
            calls += 1;
            Ok(true)
        });
        power.step(9.0, Some("none"), || unreachable!());
        assert_eq!(calls, 1);
        assert!(power.finished);
    }
}
