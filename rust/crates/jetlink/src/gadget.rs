//! Rust equivalent of the fixed offroad-only setup_gadget.sh provisioning policy.
use crate::Error;
use std::{
    fs,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
    process::Command,
};
struct Paths {
    config: PathBuf,
    usb: PathBuf,
    params: PathBuf,
    udc: PathBuf,
    mount: PathBuf,
    machine_id: PathBuf,
}
impl Paths {
    fn device() -> Self {
        Self {
            config: "/sys/kernel/config".into(),
            usb: "/sys/bus/usb/devices".into(),
            params: "/data/params/d".into(),
            udc: "/sys/class/udc".into(),
            mount: "/dev/ffs-carrot-jetlink".into(),
            machine_id: "/etc/machine-id".into(),
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
    if value(&paths.params.join("IsOffroad")) != "1" {
        return Err(Error::Contract("Jetlink setup requires offroad"));
    }
    if ["UsbGpuActive", "UsbGpuLoading"]
        .iter()
        .any(|key| value(&paths.params.join(key)) == "1")
        || egpu_present(&paths.usb)?
    {
        return Err(Error::Contract("eGPU owns USB"));
    }
    if !platform.mounted(&paths.config)? {
        platform.mount("configfs", "none", &paths.config, None)?;
    }
    let gadgets = paths.config.join("usb_gadget");
    if !gadgets.is_dir() || !paths.udc.is_dir() {
        return Err(Error::Contract("configfs/USB controller unavailable"));
    }
    for entry in fs::read_dir(&gadgets)? {
        let controller = entry?.path().join("UDC");
        if controller.exists() && !value(&controller).is_empty() {
            return Err(Error::Contract("a USB gadget already owns the controller"));
        }
    }
    let gadget = gadgets.join("carrot_jetlink");
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
    fs::write(strings.join("manufacturer"), "carrotpilot\n")?;
    fs::write(strings.join("product"), "jetlink\n")?;
    fs::write(strings.join("serialnumber"), fs::read(&paths.machine_id)?)?;
    fs::write(
        configuration.join("strings/0x409/configuration"),
        "Jetlink inference\n",
    )?;
    fs::write(configuration.join("bmAttributes"), "0xC0\n")?;
    fs::write(configuration.join("MaxPower"), "8\n")?;
    let function = gadget.join("functions/ffs.carrot_jetlink");
    fs::create_dir_all(&function)?;
    let link = configuration.join("ffs.carrot_jetlink");
    if !link.is_symlink() {
        symlink(&function, link)?;
    }
    fs::create_dir_all(&paths.mount)?;
    let (uid, gid) = platform.comma_ids()?;
    if !platform.mounted(&paths.mount)? {
        platform.mount(
            "functionfs",
            "carrot_jetlink",
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
            usb: root.join("usb"),
            params: root.join("params"),
            udc: root.join("udc"),
            mount: root.join("ffs"),
            machine_id: root.join("machine-id"),
        };
        for path in [
            &paths.config.join("usb_gadget"),
            &paths.usb,
            &paths.params,
            &paths.udc,
            &paths.mount,
        ] {
            fs::create_dir_all(path).unwrap();
        }
        fs::write(paths.params.join("IsOffroad"), "1").unwrap();
        fs::write(&paths.machine_id, "synthetic\n").unwrap();
        fs::write(paths.mount.join("ep0"), "").unwrap();
        paths
    }
    #[test]
    fn writes_original_attributes_without_binding_the_controller() {
        // Given ordinary directories and an OS-only fixture for privileged operations.
        let dir = tempfile::tempdir().unwrap();
        let paths = fixture(dir.path());
        let os = OsFixture::default();
        // When the real provisioning policy runs, then original attributes are written and binding is left to FunctionFS.
        setup(&paths, &os).unwrap();
        let gadget = paths.config.join("usb_gadget/carrot_jetlink");
        assert_eq!(fs::read(gadget.join("idVendor")).unwrap(), b"0x1209\n");
        assert_eq!(
            fs::read(gadget.join("configs/c.1/MaxPower")).unwrap(),
            b"8\n"
        );
        assert_eq!(
            fs::read(gadget.join("strings/0x409/serialnumber")).unwrap(),
            b"synthetic\n"
        );
        assert!(gadget.join("configs/c.1/ffs.carrot_jetlink").is_symlink());
        assert!(!gadget.join("UDC").exists());
        assert_eq!(os.calls.borrow().len(), 1);
    }
    #[test]
    fn refuses_onroad_egpu_and_existing_controller_ownership_before_mutation() {
        for kind in 0..4 {
            // Given a conflicting source condition in isolated files.
            let dir = tempfile::tempdir().unwrap();
            let paths = fixture(dir.path());
            match kind {
                0 => fs::write(paths.params.join("IsOffroad"), "0").unwrap(),
                1 => fs::write(paths.params.join("UsbGpuLoading"), "1").unwrap(),
                2 => {
                    let device = paths.usb.join("bridge");
                    fs::create_dir(&device).unwrap();
                    fs::write(device.join("idVendor"), "3801").unwrap();
                    fs::write(device.join("idProduct"), "0001").unwrap();
                }
                _ => {
                    let other = paths.config.join("usb_gadget/other");
                    fs::create_dir(&other).unwrap();
                    fs::write(other.join("UDC"), "owned\n").unwrap();
                }
            }
            // When setup is requested, then no Jetlink gadget directory or privileged operation appears.
            let os = OsFixture::default();
            assert!(setup(&paths, &os).is_err());
            assert!(!paths.config.join("usb_gadget/carrot_jetlink").exists());
            assert!(os.calls.borrow().is_empty());
        }
    }
}
