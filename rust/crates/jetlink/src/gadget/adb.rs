//! Reversible handover of AGNOS's stock NCM+ADB gadget; never delete configfs functions.
use super::{value, Paths};
use crate::Error;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::Path,
    process::Command,
};
const WATCHER: &str = "adb-param-watcher.path";
const SERVICE: &str = "adb-param-watcher.service";
const ADB: &str = "adbd.service";
const CONTROLLER: &str = "a600000.dwc3";

pub(super) trait Services {
    fn active(&self, unit: &str) -> Result<bool, Error>;
    fn change(&self, action: &str, units: &[&str]) -> Result<(), Error>;
}
pub(super) struct Linux;
impl Services for Linux {
    fn active(&self, unit: &str) -> Result<bool, Error> {
        let status = Command::new("timeout")
            .args([
                "--kill-after=1s",
                "2s",
                "systemctl",
                "is-active",
                "--quiet",
                unit,
            ])
            .status()?;
        match status.code() {
            Some(0) => Ok(true),
            Some(3 | 4) => Ok(false),
            _ => Err(Error::Contract("cannot query AGNOS ADB owner")),
        }
    }
    fn change(&self, action: &str, units: &[&str]) -> Result<(), Error> {
        if !Command::new("timeout")
            .args(["--kill-after=1s", "6s", "systemctl", action])
            .args(units)
            .status()?
            .success()
        {
            return Err(Error::Contract("AGNOS ADB service handover failed"));
        }
        Ok(())
    }
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Record {
    controller: String,
    watcher: bool,
    adb: bool,
}
fn journal(paths: &Paths) -> Result<File, Error> {
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(&paths.state)?;
    let meta = file.metadata()?;
    if !meta.is_file()
        || meta.uid() != rustix::process::geteuid().as_raw()
        || meta.mode() & 0o077 != 0
        || meta.len() > 512
    {
        return Err(Error::Contract("invalid AGNOS ADB handover journal"));
    }
    rustix::fs::flock(&file, rustix::fs::FlockOperation::LockExclusive)
        .map_err(std::io::Error::from)?;
    Ok(file)
}
fn record(file: &mut File) -> Result<Option<Record>, Error> {
    file.seek(SeekFrom::Start(0))?;
    let mut raw = String::new();
    file.take(513).read_to_string(&mut raw)?;
    if raw.is_empty() {
        return Ok(None);
    }
    let saved: Record = serde_json::from_str(&raw)?;
    if saved.controller != CONTROLLER {
        return Err(Error::Contract("unknown saved ADB controller"));
    }
    Ok(Some(saved))
}
fn write_record(file: &mut File, saved: &Record) -> Result<(), Error> {
    file.seek(SeekFrom::Start(0))?;
    file.write_all(&serde_json::to_vec(saved)?)?;
    let length = file.stream_position()?;
    file.set_len(length)?;
    file.sync_all()?;
    Ok(())
}
/// Identity includes all configured function links, not merely a conventional name.
pub(super) fn stock(gadget: &Path) -> Result<bool, Error> {
    if gadget.file_name().is_none_or(|name| name != "g1")
        || !value(&gadget.join("idVendor")).eq_ignore_ascii_case("0x04d8")
        || value(&gadget.join("idProduct")) != "0x1234"
        || value(&gadget.join("strings/0x409/manufacturer")) != "comma.ai"
        || value(&gadget.join("configs/c.1/strings/0x409/configuration")) != "NCM+ADB"
    {
        return Ok(false);
    }
    let configs = gadget.join("configs");
    let mut names = Vec::new();
    for config in fs::read_dir(configs)? {
        let config = config?;
        if config.file_name() != "c.1" {
            return Ok(false);
        }
        for entry in fs::read_dir(config.path())? {
            let entry = entry?;
            if entry.file_type()?.is_symlink() {
                let name = entry.file_name();
                if name != "ffs.adb" && name != "ncm.0" {
                    return Ok(false);
                }
                if fs::canonicalize(entry.path())?
                    != fs::canonicalize(gadget.join("functions").join(&name))?
                {
                    return Ok(false);
                }
                names.push(name);
            }
        }
    }
    names.sort();
    Ok(names == ["ffs.adb", "ncm.0"])
}
fn free_for_restore(paths: &Paths) -> Result<(), Error> {
    for entry in fs::read_dir(paths.config.join("usb_gadget"))? {
        let path = entry?.path();
        let controller = value(&path.join("UDC"));
        if !controller.is_empty()
            && (path.file_name().is_none_or(|name| name != "g1")
                || controller != CONTROLLER
                || !stock(&path)?)
        {
            return Err(Error::Contract(
                "USB controller still owned; ADB restoration deferred",
            ));
        }
    }
    Ok(())
}
pub(super) fn claim(paths: &Paths, os: &impl Services) -> Result<(), Error> {
    let g1 = paths.config.join("usb_gadget/g1");
    let controller = value(&g1.join("UDC"));
    if (!controller.is_empty() && controller != CONTROLLER) || (g1.exists() && !stock(&g1)?) {
        return Err(Error::Contract("refusing non-stock USB gadget takeover"));
    }
    let mut file = journal(paths)?;
    if record(&mut file)?.is_none() {
        let watcher = os.active(WATCHER)?;
        let adb = os.active(ADB)?;
        if controller.is_empty() && !watcher && !adb {
            return Ok(());
        }
        let saved = Record {
            controller: CONTROLLER.into(),
            watcher,
            adb,
        };
        write_record(&mut file, &saved)?;
    }
    let result = (|| {
        os.change("stop", &[WATCHER, SERVICE, ADB])?;
        if g1.exists() && !stock(&g1)? {
            return Err(Error::Contract("stock ADB gadget changed during handover"));
        }
        let held = value(&g1.join("UDC"));
        if !held.is_empty() && held != CONTROLLER {
            return Err(Error::Contract("ADB controller changed during handover"));
        }
        // FunctionFS close can already unbind g1; a second unbind returns ENODEV.
        if !held.is_empty() {
            fs::write(g1.join("UDC"), b"\n")?;
        }
        Ok(())
    })();
    if let Err(error) = result {
        drop(file);
        if let Err(restore) = restore(paths, os) {
            return Err(Error::Native(format!("{error}; ADB restore: {restore}")));
        }
        return Err(error);
    }
    eprintln!("Jetlink temporarily owns stock AGNOS ADB USB controller");
    Ok(())
}
pub(super) fn restore(paths: &Paths, os: &impl Services) -> Result<(), Error> {
    if !paths.state.try_exists()? {
        return Ok(());
    }
    let mut file = journal(paths)?;
    let Some(saved) = record(&mut file)? else {
        return Ok(());
    };
    if ["UsbGpuActive", "UsbGpuLoading"]
        .iter()
        .any(|key| value(&paths.params.join(key)) == "1")
        || super::egpu_present(&paths.usb)?
    {
        return Err(Error::Contract("eGPU owns USB; ADB restoration deferred"));
    }
    free_for_restore(paths)?;
    let g1 = paths.config.join("usb_gadget/g1");
    if g1.exists() && !stock(&g1)? {
        return Err(Error::Contract(
            "stock ADB gadget changed before restoration",
        ));
    }
    // Use AGNOS's own script through its fixed systemd unit: it honors the current
    // AdbEnabled value and waits for adbd descriptors before binding the saved UDC.
    // No saved user setting is overwritten, including changes made during Jetlink use.
    // Arm first so a concurrent Params change cannot be missed between the
    // script's read and watcher startup. Reconciliation is idempotent.
    if saved.watcher {
        os.change("start", &[WATCHER])?;
    }
    if saved.adb || saved.watcher || value(&paths.params.join("AdbEnabled")) == "1" {
        if let Err(error) = os.change("start", &[SERVICE]) {
            // AGNOS19.8's Off script writes an empty UDC after stopping adbd,
            // which may already have unbound it. Accept that service failure
            // only after the current Off state is independently verified.
            let detached = !g1.exists() || fs::read_to_string(g1.join("UDC"))?.trim().is_empty();
            if value(&paths.params.join("AdbEnabled")) == "1"
                || !detached
                || os.active(SERVICE)?
                || os.active(ADB)?
            {
                return Err(error);
            }
            eprintln!("Jetlink verified ADB Off despite stock script unbind error");
        }
    } else if !value(&g1.join("UDC")).is_empty() {
        fs::write(g1.join("UDC"), b"\n")?;
    }
    file.set_len(0)?;
    file.sync_all()?;
    eprintln!("Jetlink returned stock AGNOS ADB USB ownership");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        cell::{Cell, RefCell},
        os::unix::fs::{symlink, PermissionsExt},
    };
    #[derive(Default)]
    struct Fake {
        calls: RefCell<Vec<String>>,
        fail_stop: bool,
        fail_service: Cell<bool>,
        service_busy: Cell<bool>,
        adb_stopped: Cell<bool>,
        detached_attribute: Option<std::path::PathBuf>,
    }
    impl Services for Fake {
        fn active(&self, unit: &str) -> Result<bool, Error> {
            Ok(match unit {
                ADB => !self.adb_stopped.get(),
                SERVICE => self.service_busy.get(),
                _ => true,
            })
        }
        fn change(&self, action: &str, units: &[&str]) -> Result<(), Error> {
            self.calls
                .borrow_mut()
                .push(format!("{action}:{}", units.join(",")));
            if self.fail_stop && action == "stop" {
                return Err(Error::Closed);
            }
            if action == "stop" && units.contains(&ADB) {
                self.adb_stopped.set(true);
            }
            if action == "start" && units.contains(&SERVICE) && self.fail_service.get() {
                return Err(Error::Contract(
                    "stock script failed after empty UDC unbind",
                ));
            }
            if action == "stop" {
                if let Some(path) = &self.detached_attribute {
                    fs::remove_file(path)?;
                    // Reading reports no held UDC, while another write would fail,
                    // modelling configfs ENODEV after FunctionFS already unbound.
                    fs::create_dir(path)?;
                }
            }
            Ok(())
        }
    }
    #[test]
    fn off_setting_accepts_stock_unbind_error_only_after_verification() {
        for kind in 0..6 {
            let dir = tempfile::tempdir().unwrap();
            let paths = fixture(dir.path());
            let os = Fake::default();
            claim(&paths, &os).unwrap();
            fs::write(
                paths.params.join("AdbEnabled"),
                if kind == 1 { "1" } else { "0" },
            )
            .unwrap();
            os.fail_service.set(true);
            os.service_busy.set(kind == 2);
            if kind == 3 {
                os.adb_stopped.set(false);
            }
            let udc = paths.config.join("usb_gadget/g1/UDC");
            match kind {
                4 => fs::write(&udc, CONTROLLER).unwrap(),
                5 => {
                    fs::remove_file(&udc).unwrap();
                    fs::create_dir(&udc).unwrap();
                }
                _ => {}
            }
            let result = restore(&paths, &os);
            if kind == 0 {
                result.unwrap();
                assert_eq!(fs::metadata(&paths.state).unwrap().len(), 0);
            } else {
                assert!(result.is_err());
                assert!(fs::metadata(&paths.state).unwrap().len() > 0);
            }
        }
    }
    fn fixture(root: &Path) -> Paths {
        let paths = super::super::tests::fixture(root);
        let g1 = paths.config.join("usb_gadget/g1");
        fs::create_dir_all(g1.join("configs/c.1/strings/0x409")).unwrap();
        fs::create_dir_all(g1.join("strings/0x409")).unwrap();
        fs::write(g1.join("idVendor"), "0x04D8\n").unwrap();
        fs::write(g1.join("idProduct"), "0x1234\n").unwrap();
        fs::write(g1.join("strings/0x409/manufacturer"), "comma.ai\n").unwrap();
        fs::write(
            g1.join("configs/c.1/strings/0x409/configuration"),
            "NCM+ADB\n",
        )
        .unwrap();
        for name in ["ncm.0", "ffs.adb"] {
            let function = g1.join("functions").join(name);
            fs::create_dir_all(&function).unwrap();
            symlink(function, g1.join("configs/c.1").join(name)).unwrap();
        }
        fs::write(g1.join("UDC"), format!("{CONTROLLER}\n")).unwrap();
        fs::write(paths.params.join("AdbEnabled"), "1").unwrap();
        paths
    }
    #[test]
    fn stock_handover_retains_restart_record_and_restores_current_setting() {
        let dir = tempfile::tempdir().unwrap();
        let paths = fixture(dir.path());
        let os = Fake::default();
        let udc = paths.config.join("usb_gadget/g1/UDC");
        claim(&paths, &os).unwrap();
        assert_eq!(value(&udc), "");
        let saved = fs::read(&paths.state).unwrap();
        claim(&paths, &os).unwrap();
        assert_eq!(fs::read(&paths.state).unwrap(), saved);
        assert_eq!(os.calls.borrow().len(), 2);
        fs::write(paths.params.join("AdbEnabled"), "0").unwrap();
        restore(&paths, &os).unwrap();
        assert_eq!(value(&paths.params.join("AdbEnabled")), "0");
        assert_eq!(fs::metadata(&paths.state).unwrap().len(), 0);
        assert_eq!(
            *os.calls.borrow(),
            [
                format!("stop:{WATCHER},{SERVICE},{ADB}"),
                format!("stop:{WATCHER},{SERVICE},{ADB}"),
                format!("start:{WATCHER}"),
                format!("start:{SERVICE}")
            ]
        );
        restore(&paths, &os).unwrap();
        assert_eq!(os.calls.borrow().len(), 4);
    }
    #[test]
    fn adbd_close_may_already_unbind_the_stock_controller() {
        let dir = tempfile::tempdir().unwrap();
        let paths = fixture(dir.path());
        let udc = paths.config.join("usb_gadget/g1/UDC");
        let os = Fake {
            detached_attribute: Some(udc.clone()),
            ..Fake::default()
        };
        claim(&paths, &os).unwrap();
        assert!(udc.is_dir());
        assert!(fs::metadata(&paths.state).unwrap().len() > 0);
    }
    #[test]
    fn detached_or_not_yet_created_g1_still_quiesces_the_adb_watcher() {
        for missing in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let paths = fixture(dir.path());
            let g1 = paths.config.join("usb_gadget/g1");
            if missing {
                fs::remove_dir_all(&g1).unwrap();
            } else {
                fs::write(g1.join("UDC"), "").unwrap();
            }
            fs::write(paths.params.join("AdbEnabled"), "0").unwrap();
            let os = Fake::default();
            claim(&paths, &os).unwrap();
            assert_eq!(
                os.calls.borrow()[0],
                format!("stop:{WATCHER},{SERVICE},{ADB}")
            );
            // A later enable is reconciled after the watcher is armed again.
            fs::write(paths.params.join("AdbEnabled"), "1").unwrap();
            restore(&paths, &os).unwrap();
            assert_eq!(os.calls.borrow()[1], format!("start:{WATCHER}"));
            assert_eq!(os.calls.borrow()[2], format!("start:{SERVICE}"));
            assert_eq!(value(&paths.params.join("AdbEnabled")), "1");
        }
    }
    #[test]
    fn rejects_modified_stock_or_foreign_links_without_service_mutation() {
        for kind in 0..4 {
            let dir = tempfile::tempdir().unwrap();
            let paths = fixture(dir.path());
            let os = Fake::default();
            let g1 = paths.config.join("usb_gadget/g1");
            match kind {
                0 => fs::write(g1.join("idVendor"), "0x1209").unwrap(),
                1 => fs::write(g1.join("UDC"), "foreign").unwrap(),
                2 => {
                    symlink(g1.join("functions/ncm.0"), g1.join("configs/c.1/foreign")).unwrap();
                }
                _ => {
                    fs::remove_file(g1.join("configs/c.1/ffs.adb")).unwrap();
                }
            }
            assert!(claim(&paths, &os).is_err());
            assert!(os.calls.borrow().is_empty());
            assert!(!paths.state.exists());
        }
    }
    #[test]
    fn setup_service_failure_rolls_back_and_busy_restore_retains_record() {
        let dir = tempfile::tempdir().unwrap();
        let paths = fixture(dir.path());
        let os = Fake {
            fail_stop: true,
            ..Fake::default()
        };
        assert!(claim(&paths, &os).is_err());
        assert!(os
            .calls
            .borrow()
            .iter()
            .any(|v| v == &format!("start:{SERVICE}")));
        assert_eq!(fs::metadata(&paths.state).unwrap().len(), 0);
        claim(&paths, &Fake::default()).unwrap();
        let own = paths.config.join("usb_gadget/carrot_jetlink");
        fs::create_dir_all(&own).unwrap();
        fs::write(own.join("UDC"), CONTROLLER).unwrap();
        assert!(restore(&paths, &Fake::default()).is_err());
        assert!(fs::metadata(&paths.state).unwrap().len() > 0);
        fs::write(own.join("UDC"), "").unwrap();
        restore(&paths, &Fake::default()).unwrap();
    }
    #[test]
    fn rejects_untrusted_journal_without_unbinding_stock() {
        for symlinked in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let paths = fixture(dir.path());
            if symlinked {
                symlink(&paths.machine_id, &paths.state).unwrap();
            } else {
                fs::write(&paths.state, "{}").unwrap();
                fs::set_permissions(&paths.state, fs::Permissions::from_mode(0o666)).unwrap();
            }
            assert!(claim(&paths, &Fake::default()).is_err());
            assert_eq!(value(&paths.config.join("usb_gadget/g1/UDC")), CONTROLLER);
        }
    }
}
