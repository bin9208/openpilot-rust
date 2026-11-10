//! Carrot tools/jetlink/setup_gadget.sh: provision the Jetlink FunctionFS gadget.
use crate::Error;

pub const NAME: &str = "jetlink";
pub const GADGET: &str = "/sys/kernel/config/usb_gadget/jetlink";
pub const FFS_MOUNT: &str = "/dev/ffs-jetlink";
use std::{
    fs,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
    process::Command,
};
struct Paths {
    config: PathBuf,
    udc: PathBuf,
    mount: PathBuf,
    serial: PathBuf,
    handoff_flag: PathBuf,
    road: PathBuf,
    data_role: PathBuf,
    tici: PathBuf,
}
impl Paths {
    fn device() -> Self {
        Self {
            config: "/sys/kernel/config".into(),
            udc: "/sys/class/udc".into(),
            mount: FFS_MOUNT.into(),
            serial: "/proc/device-tree/serial-number".into(),
            handoff_flag: "/data/jetlink-adb-handoff".into(),
            road: "/data/params/d".into(),
            data_role: "/sys/class/usbpd/usbpd0/current_dr".into(),
            tici: "/TICI".into(),
        }
    }
}
trait Platform {
    fn mounted(&self, path: &Path) -> Result<bool, Error>;
    fn mount(
        &self,
        kind: &str,
        source: &str,
        path: &Path,
        options: Option<&str>,
    ) -> Result<(), Error>;
    fn comma_ids(&self) -> Result<(u32, u32), Error>;
    fn own_udc(&self, path: &Path, uid: u32) -> Result<(), Error>;
}
struct Linux;
impl Platform for Linux {
    fn mounted(&self, path: &Path) -> Result<bool, Error> {
        Ok(Command::new("mountpoint")
            .arg("-q")
            .arg(path)
            .status()?
            .success())
    }
    fn mount(
        &self,
        kind: &str,
        source: &str,
        path: &Path,
        options: Option<&str>,
    ) -> Result<(), Error> {
        let mut command = Command::new("mount");
        command.args(["-t", kind]);
        if let Some(options) = options {
            command.args(["-o", options]);
        }
        if !command.arg(source).arg(path).status()?.success() {
            return Err(Error::Contract("USB mount failed"));
        }
        Ok(())
    }
    fn comma_ids(&self) -> Result<(u32, u32), Error> {
        fn id(flag: &str) -> Result<u32, Error> {
            let output = Command::new("id").args([flag, "comma"]).output()?;
            if !output.status.success() {
                return Err(Error::Contract("comma account missing"));
            }
            String::from_utf8_lossy(&output.stdout)
                .trim()
                .parse()
                .map_err(|_| Error::Contract("invalid comma account id"))
        }
        Ok((id("-u")?, id("-g")?))
    }
    fn own_udc(&self, path: &Path, uid: u32) -> Result<(), Error> {
        rustix::fs::chown(path, Some(rustix::process::Uid::from_raw(uid)), None)
            .map_err(std::io::Error::from)?;
        Ok(())
    }
}
fn value(path: &Path) -> String {
    fs::read_to_string(path)
        .unwrap_or_default()
        .trim_end_matches('\n')
        .to_owned()
}
pub fn egpu_present(usb: &Path) -> Result<bool, Error> {
    for entry in fs::read_dir(usb)? {
        let path = entry?.path();
        if matches!(value(&path.join("idVendor")).as_str(), "add1" | "3801")
            && value(&path.join("idProduct")) == "0001"
        {
            return Ok(true);
        }
    }
    Ok(false)
}
fn setup(paths: &Paths, platform: &impl Platform) -> Result<(), Error> {
    if !platform.mounted(&paths.config)? {
        platform.mount("configfs", "none", &paths.config, None)?;
    }
    let gadgets = paths.config.join("usb_gadget");
    if !gadgets.is_dir() || !paths.udc.is_dir() {
        return Err(Error::Contract("configfs/USB controller unavailable"));
    }
    for entry in fs::read_dir(&gadgets)? {
        let entry = entry?.path();
        let owner = entry
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let controller = entry.join("UDC");
        if owner != NAME && controller.exists() && !value(&controller).is_empty() {
            if owner == "g1" {
                // Only the explicitly opted-in default ADB gadget is handed over.
                match adb_handoff(paths, &value(&controller)) {
                    Ok(()) => continue,
                    Err(reason) => return Err(Error::Native(reason)),
                }
            }
            return Err(Error::Native(format!(
                "USB gadget '{}' already holds the device controller ({})",
                controller
                    .parent()
                    .and_then(Path::file_name)
                    .unwrap_or_default()
                    .to_string_lossy(),
                value(&controller),
            )));
        }
    }
    let gadget = gadgets.join(NAME);
    fs::create_dir_all(&gadget)?;
    for (name, value) in [
        ("idVendor", "0x1209"),
        ("idProduct", "0x0001"),
        ("bcdUSB", "0x0320"),
        ("bcdDevice", "0x0100"),
        ("bDeviceClass", "0"),
    ] {
        fs::write(gadget.join(name), format!("{value}\n"))?;
    }
    let strings = gadget.join("strings/0x409");
    let configuration = gadget.join("configs/c.1");
    fs::create_dir_all(&strings)?;
    fs::create_dir_all(configuration.join("strings/0x409"))?;
    fs::write(strings.join("manufacturer"), "zoompilot\n")?;
    fs::write(strings.join("product"), "jetlink\n")?;
    let serial = fs::read(&paths.serial)
        .map(|bytes| String::from_utf8_lossy(&bytes).replace('\0', ""))
        .unwrap_or_else(|_| "0001".into());
    fs::write(strings.join("serialnumber"), format!("{}\n", serial.trim()))?;
    fs::write(
        configuration.join("strings/0x409/configuration"),
        "jetlink inference link\n",
    )?;
    fs::write(configuration.join("bmAttributes"), "0xC0\n")?;
    fs::write(configuration.join("MaxPower"), "8\n")?;
    let function = gadget.join(format!("functions/ffs.{NAME}"));
    fs::create_dir_all(&function)?;
    let link = configuration.join(format!("ffs.{NAME}"));
    if !link.is_symlink() {
        symlink(&function, link)?;
    }
    fs::create_dir_all(&paths.mount)?;
    let (uid, gid) = platform.comma_ids()?;
    if !platform.mounted(&paths.mount)? {
        platform.mount(
            "functionfs",
            NAME,
            &paths.mount,
            Some(&format!("uid={uid},gid={gid}")),
        )?;
    }
    if !paths.mount.join("ep0").exists() {
        return Err(Error::Contract("FunctionFS ep0 unavailable"));
    }
    platform.own_udc(&gadget.join("UDC"), uid)?;
    Ok(())
}
/// Carrot tools/jetlink/adb_handoff.py: release the default ADB gadget only
/// when /data/jetlink-adb-handoff exists, the car is offroad and the port is in
/// device role. Returns the bracketed reason the web card shows on refusal.
fn adb_handoff(paths: &Paths, controller: &str) -> Result<(), String> {
    use std::os::unix::fs::MetadataExt;
    let deny = |code: &str, detail: String| Err(format!("[adb-handoff:{code}] {detail}"));
    let Ok(flag) = fs::symlink_metadata(&paths.handoff_flag) else {
        return deny(
            "opt_in_missing",
            format!(
                "ADB handoff permission is missing ({})",
                paths.handoff_flag.display()
            ),
        );
    };
    if controller != "a600000.dwc3" {
        return deny(
            "controller",
            format!("Unsupported controller {controller:?}"),
        );
    }
    if !flag.is_file()
        || flag.nlink() != 1
        || flag.uid() != rustix::process::geteuid().as_raw()
        || flag.mode() & 0o022 != 0
    {
        return deny(
            "opt_in_invalid",
            format!(
                "Unsafe permission marker: uid={}, mode={:04o}",
                flag.uid(),
                flag.mode() & 0o7777
            ),
        );
    }
    if !paths.tici.is_file() {
        return deny(
            "platform",
            "Required /TICI platform marker is missing".into(),
        );
    }
    let offroad = value(&paths.road.join("IsOffroad"));
    let onroad = value(&paths.road.join("IsOnroad"));
    if offroad.trim() != "1" || onroad.trim() != "0" {
        return deny(
            "offroad",
            format!("Offroad required: IsOffroad={offroad:?}, IsOnroad={onroad:?}"),
        );
    }
    let role = value(&paths.data_role);
    if role.trim() != "ufp" {
        return deny(
            "usb_role",
            format!("USB device role required: current_dr={role:?}"),
        );
    }
    let adb = paths.config.join("usb_gadget/g1");
    let (vendor, product) = (
        value(&adb.join("idVendor")).to_lowercase(),
        value(&adb.join("idProduct")).to_lowercase(),
    );
    if !matches!(vendor.trim(), "0x04d8" | "04d8") || !matches!(product.trim(), "0x1234" | "1234") {
        return deny(
            "gadget_identity",
            format!("Expected ADB 04d8:1234, got {vendor}/{product}"),
        );
    }
    if fs::write(adb.join("UDC"), "\n").is_err() {
        return deny("write_failed", "g1 UDC unbind write failed".into());
    }
    if !value(&adb.join("UDC")).trim().is_empty() {
        return deny(
            "still_bound",
            "g1 UDC remains bound after the unbind write".into(),
        );
    }
    Ok(())
}
pub fn provision_device() -> Result<(), Error> {
    if !rustix::process::geteuid().is_root() {
        return Err(Error::Contract("gadget provisioning requires root"));
    }
    setup(&Paths::device(), &Linux)
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    #[derive(Default)]
    struct OsFixture {
        calls: RefCell<Vec<String>>,
    }
    impl Platform for OsFixture {
        fn mounted(&self, _: &Path) -> Result<bool, Error> {
            Ok(true)
        }
        fn mount(&self, _: &str, _: &str, _: &Path, _: Option<&str>) -> Result<(), Error> {
            panic!("fixtures must never mount")
        }
        fn comma_ids(&self) -> Result<(u32, u32), Error> {
            Ok((1000, 1000))
        }
        fn own_udc(&self, path: &Path, uid: u32) -> Result<(), Error> {
            self.calls
                .borrow_mut()
                .push(format!("{}:{uid}", path.display()));
            Ok(())
        }
    }
    fn fixture(root: &Path) -> Paths {
        let paths = Paths {
            config: root.join("config"),
            udc: root.join("udc"),
            mount: root.join("ffs"),
            serial: root.join("serial"),
            handoff_flag: root.join("handoff"),
            road: root.join("params"),
            data_role: root.join("dr"),
            tici: root.join("TICI"),
        };
        for path in [
            &paths.config.join("usb_gadget"),
            &paths.udc,
            &paths.mount,
            &paths.road,
        ] {
            fs::create_dir_all(path).unwrap();
        }
        fs::write(&paths.serial, "abc123\0").unwrap();
        fs::write(paths.mount.join("ep0"), "").unwrap();
        paths
    }
    #[test]
    fn writes_original_attributes_without_binding_the_controller() {
        let dir = tempfile::tempdir().unwrap();
        let paths = fixture(dir.path());
        let os = OsFixture::default();
        setup(&paths, &os).unwrap();
        let gadget = paths.config.join("usb_gadget/jetlink");
        assert_eq!(fs::read(gadget.join("idVendor")).unwrap(), b"0x1209\n");
        assert_eq!(
            fs::read(gadget.join("strings/0x409/manufacturer")).unwrap(),
            b"zoompilot\n"
        );
        assert_eq!(
            fs::read(gadget.join("strings/0x409/serialnumber")).unwrap(),
            b"abc123\n"
        );
        assert!(gadget.join("configs/c.1/ffs.jetlink").is_symlink());
        assert!(!gadget.join("UDC").exists());
        assert_eq!(os.calls.borrow().len(), 1);
    }
    #[test]
    fn refuses_bound_adb_without_opt_in_and_hands_over_with_it() {
        let dir = tempfile::tempdir().unwrap();
        let paths = fixture(dir.path());
        let adb = paths.config.join("usb_gadget/g1");
        fs::create_dir(&adb).unwrap();
        fs::write(adb.join("UDC"), "a600000.dwc3\n").unwrap();
        fs::write(adb.join("idVendor"), "0x04d8").unwrap();
        fs::write(adb.join("idProduct"), "0x1234").unwrap();
        let error = setup(&paths, &OsFixture::default())
            .unwrap_err()
            .to_string();
        assert!(error.contains("opt_in_missing"), "{error}");
        assert!(!paths.config.join("usb_gadget/jetlink").exists());
        fs::write(&paths.handoff_flag, "").unwrap();
        fs::set_permissions(
            &paths.handoff_flag,
            std::os::unix::fs::PermissionsExt::from_mode(0o644),
        )
        .unwrap();
        fs::write(&paths.tici, "").unwrap();
        fs::write(paths.road.join("IsOffroad"), "1").unwrap();
        fs::write(paths.road.join("IsOnroad"), "0").unwrap();
        fs::write(&paths.data_role, "ufp").unwrap();
        setup(&paths, &OsFixture::default()).unwrap();
        assert_eq!(fs::read_to_string(adb.join("UDC")).unwrap().trim(), "");
    }
    #[test]
    fn refuses_another_bound_gadget_before_mutation() {
        let dir = tempfile::tempdir().unwrap();
        let paths = fixture(dir.path());
        let other = paths.config.join("usb_gadget/other");
        fs::create_dir(&other).unwrap();
        fs::write(other.join("UDC"), "owned\n").unwrap();
        let os = OsFixture::default();
        assert!(setup(&paths, &os).is_err());
        assert!(!paths.config.join("usb_gadget/jetlink").exists());
        assert!(os.calls.borrow().is_empty());
    }
}
