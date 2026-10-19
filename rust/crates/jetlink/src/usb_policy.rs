//! Native port of zoompilot/jetlink's fixed USB-PD, UDC and VM operations (MIT).
use std::{
    fs::{self, OpenOptions},
    io::{self, Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::Path,
};

pub const UDC_MODE: &str = "/sys/devices/platform/soc/a600000.ssusb/mode";
pub const DATA_ROLE: &str = "/sys/class/usbpd/usbpd0/current_dr";
const STATE: &str = "/dev/shm/jetlink-sysctl-prev";
const VM: &[(&str, &str)] = &[
    ("vm.dirty_bytes", "16777216"),
    ("vm.dirty_background_bytes", "8388608"),
    ("vm.extra_free_kbytes", "32768"),
    ("net.core.wmem_max", "4194304"),
    ("net.core.rmem_max", "4194304"),
];

fn put(root: &Path, path: &str, value: &str) -> io::Result<()> {
    fs::write(
        root.join(path.trim_start_matches('/')),
        format!("{value}\n"),
    )
}
fn all(ops: impl IntoIterator<Item = io::Result<()>>) -> io::Result<()> {
    let mut failure = None;
    for result in ops {
        if let Err(error) = result {
            eprintln!("Jetlink USB policy: {error}");
            failure = Some(error);
        }
    }
    failure.map_or(Ok(()), Err)
}
fn sysctl(key: &str) -> String {
    format!("/proc/sys/{}", key.replace('.', "/"))
}
fn valid_key(key: &str) -> bool {
    key.strip_prefix("vm.")
        .or_else(|| key.strip_prefix("net.core."))
        .is_some_and(|name| {
            !name.is_empty() && name.bytes().all(|c| c.is_ascii_lowercase() || c == b'_')
        })
}
fn state_file(root: &Path) -> io::Result<fs::File> {
    // Shared-memory records must be ours, never a link planted for a root helper.
    let file = OpenOptions::new()
        .read(true)
        .append(true)
        .create(true)
        .mode(0o644)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(root.join(STATE.trim_start_matches('/')))?;
    let metadata = file.metadata()?;
    if !metadata.is_file()
        || metadata.uid() != rustix::process::geteuid().as_raw()
        || metadata.mode() & 0o022 != 0
    {
        return Err(io::Error::other(
            "invalid Jetlink sysctl snapshot owner/type/mode",
        ));
    }
    Ok(file)
}
fn vm(root: &Path, apply: bool) -> io::Result<()> {
    let state = root.join(STATE.trim_start_matches('/'));
    if !apply && !state.try_exists()? {
        return Ok(());
    }
    let mut file = state_file(root)?;
    let mut had = String::new();
    file.read_to_string(&mut had)?;
    if apply {
        let mut record = String::new();
        for &(key, _) in VM {
            let ratio = key
                .strip_suffix("_bytes")
                .map(|base| format!("{base}_ratio"));
            if had.lines().any(|line| {
                line.split_once('=')
                    .is_some_and(|(old, _)| old == key || ratio.as_deref() == Some(old))
            }) {
                continue;
            }
            let read = |key: &str| {
                fs::read_to_string(root.join(sysctl(key).trim_start_matches('/')))
                    .map(|v| v.trim().to_owned())
            };
            let mut saved_key = key.to_owned();
            let mut value = read(key);
            if value.as_deref().is_ok_and(|v| v == "0") {
                if let Some(ratio) = ratio {
                    saved_key = ratio;
                    value = read(&saved_key);
                }
            }
            match value {
                Ok(value) if !value.is_empty() => {
                    record.push_str(&format!("{saved_key}={value}\n"))
                }
                _ => eprintln!("Jetlink: could not read {saved_key}"),
            }
        }
        // Save stock values before changing any key. Refuse a destructive update
        // if the native helper cannot preserve the restoration record.
        file.write_all(record.as_bytes())?;
        file.sync_all()?;
        all(VM
            .iter()
            .map(|&(key, value)| put(root, &sysctl(key), value)))
    } else {
        let result = all(had.lines().map(|line| {
            let (key, value) = line
                .split_once('=')
                .ok_or_else(|| io::Error::other("invalid sysctl snapshot line"))?;
            if !valid_key(key) || value.is_empty() || !value.bytes().all(|v| v.is_ascii_digit()) {
                return Err(io::Error::other("invalid sysctl snapshot key/value"));
            }
            put(root, &sysctl(key), value)
        }));
        fs::remove_file(state)?;
        result
    }
}

pub fn apply(action: &str) -> io::Result<()> {
    apply_at(Path::new("/"), action)
}
fn apply_at(root: &Path, action: &str) -> io::Result<()> {
    let voter = "/sys/kernel/debug/pmic-votable/DISABLE_POWER_ROLE_SWITCH";
    match action {
        "port-hold" => {
            put(root, &format!("{voter}/force_val"), "1")?;
            put(root, &format!("{voter}/force_active"), "1")
        }
        "port-off" => {
            put(root, &format!("{voter}/force_active"), "0")?;
            put(root, &format!("{voter}/force_val"), "0")
        }
        "port-device" => put(
            root,
            "/sys/class/dual_role_usb/otg_default/data_role",
            "device",
        ),
        "port-reset" => put(root, "/sys/class/usbpd/usbpd0/hard_reset", "1"),
        "port-source" => put(
            root,
            "/sys/class/dual_role_usb/otg_default/power_role",
            "source",
        ),
        "port-sink" => put(
            root,
            "/sys/class/dual_role_usb/otg_default/power_role",
            "sink",
        ),
        "udc-start" => put(root, UDC_MODE, "peripheral"),
        "udc-stop" => put(root, UDC_MODE, "none"),
        "udc-apply" | "udc-restore" => {
            let (compliance, log) = if action == "udc-apply" {
                ("Y", "1")
            } else {
                ("N", "0")
            };
            all([
                put(
                    root,
                    "/sys/module/policy_engine/parameters/usb_compliance_mode",
                    compliance,
                ),
                put(
                    root,
                    "/sys/devices/platform/soc/a600000.ssusb/usb_compliance_mode",
                    compliance,
                ),
                put(root, "/sys/kernel/debug/ipc_logging/f_fs/log_disable", log),
            ])
        }
        "vm-apply" => vm(root, true),
        "vm-restore" => vm(root, false),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "unknown fixed USB policy action",
        )),
    }
}

pub fn read(path: &str) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty())
}
pub fn own_host_attached() -> bool {
    // Never interpret a different gadget's configured controller as ours.
    read("/sys/kernel/config/usb_gadget/carrot_jetlink/UDC")
        .filter(|name| !name.contains('/') && name != "." && name != "..")
        .and_then(|name| read(&format!("/sys/class/udc/{name}/state")))
        .is_some_and(|state| state == "configured")
}

#[derive(Default)]
pub struct Tuning {
    state: Option<bool>,
    restored: bool,
}
impl Tuning {
    pub fn update(&mut self, wanted: bool, mut run: impl FnMut(&str) -> bool) {
        if self.state == Some(wanted) {
            return;
        }
        let vm = run(if wanted { "vm-apply" } else { "vm-restore" });
        let udc = run(if wanted { "udc-apply" } else { "udc-restore" });
        self.restored = !wanted && vm && udc;
        self.state = Some(wanted);
    }
    pub fn finish(&mut self, wanted: bool, run: impl FnMut(&str) -> bool) {
        if !wanted && !self.restored {
            self.state = None;
            self.update(false, run);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("dev/shm")).unwrap();
        for (key, value) in [
            ("vm.dirty_bytes", "0"),
            ("vm.dirty_ratio", "20"),
            ("vm.dirty_background_bytes", "0"),
            ("vm.dirty_background_ratio", "10"),
            ("vm.extra_free_kbytes", "123"),
            ("net.core.wmem_max", "234"),
            ("net.core.rmem_max", "345"),
        ] {
            let path = root.path().join(sysctl(key).trim_start_matches('/'));
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, value).unwrap();
        }
        root
    }
    #[test]
    fn vm_preserves_first_stock_values_and_ratio_mode() {
        let root = fixture();
        apply_at(root.path(), "vm-apply").unwrap();
        let first = fs::read_to_string(root.path().join(STATE.trim_start_matches('/'))).unwrap();
        assert!(first.contains("vm.dirty_ratio=20\n"));
        assert!(!first.contains("vm.dirty_bytes="));
        apply_at(root.path(), "vm-apply").unwrap();
        assert_eq!(
            first,
            fs::read_to_string(root.path().join(STATE.trim_start_matches('/'))).unwrap()
        );
        apply_at(root.path(), "vm-restore").unwrap();
        assert_eq!(
            fs::read_to_string(root.path().join("proc/sys/vm/dirty_ratio")).unwrap(),
            "20\n"
        );
        assert_eq!(
            fs::read_to_string(root.path().join("proc/sys/net/core/wmem_max")).unwrap(),
            "234\n"
        );
        assert!(!root.path().join(STATE.trim_start_matches('/')).exists());
    }
    #[test]
    fn udc_attempts_other_knobs_when_one_is_missing() {
        let root = tempfile::tempdir().unwrap();
        let path = root
            .path()
            .join("sys/kernel/debug/ipc_logging/f_fs/log_disable");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        assert!(apply_at(root.path(), "udc-apply").is_err());
        assert_eq!(fs::read_to_string(path).unwrap(), "1\n");
    }
    #[test]
    fn snapshot_symlink_is_rejected() {
        let root = fixture();
        let target = root.path().join("victim");
        fs::write(&target, "untouched").unwrap();
        std::os::unix::fs::symlink(&target, root.path().join(STATE.trim_start_matches('/')))
            .unwrap();
        assert!(apply_at(root.path(), "vm-apply").is_err());
        assert_eq!(fs::read_to_string(target).unwrap(), "untouched");
    }
    #[test]
    fn tuning_is_once_per_owner_and_restores_prior_owner() {
        let mut tuning = Tuning::default();
        let mut calls = vec![];
        for _ in 0..1000 {
            tuning.update(true, |v| {
                calls.push(v.to_owned());
                true
            });
        }
        tuning.finish(true, |_| panic!("wanted link retains tuning"));
        assert_eq!(calls, ["vm-apply", "udc-apply"]);
        tuning.update(false, |v| {
            calls.push(v.to_owned());
            false
        });
        tuning.finish(false, |v| {
            calls.push(v.to_owned());
            true
        });
        assert_eq!(
            &calls[2..],
            ["vm-restore", "udc-restore", "vm-restore", "udc-restore"]
        );
        let mut fresh = Tuning::default();
        let mut prior = vec![];
        fresh.update(false, |v| {
            prior.push(v.to_owned());
            true
        });
        assert_eq!(prior, ["vm-restore", "udc-restore"]);
    }
}
