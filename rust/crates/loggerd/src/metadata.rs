use crate::{clock, Error};
use openpilot_cereal::log_capnp::{event, init_data::DeviceType};
use openpilot_params::Params;
use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    process::Command,
    time::SystemTime,
};

pub struct Environment {
    pub log_root: PathBuf,
    pub params_root: PathBuf,
    pub prefix: String,
    pub device: DeviceType,
}

impl Environment {
    pub fn read() -> Result<Self, Error> {
        let device = if Path::new("/TICI").is_file() {
            match String::from_utf8_lossy(&read_optional("/sys/firmware/devicetree/base/model"))
                .trim_matches(['\0', ' ', '\n'])
            {
                "comma tici" => DeviceType::Tici,
                "comma tizi" => DeviceType::Tizi,
                "comma mici" => DeviceType::Mici,
                _ => return Err(Error::Invalid("unknown hardware model")),
            }
        } else {
            DeviceType::Pc
        };
        let prefix = std::env::var("OPENPILOT_PREFIX").unwrap_or_default();
        let home = std::env::var_os("HOME").unwrap_or_default();
        let comma_home = PathBuf::from(home).join(format!(".comma{prefix}"));
        let log_root = std::env::var_os("LOG_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                if device == DeviceType::Pc {
                    comma_home.join("media/0/realdata")
                } else {
                    PathBuf::from("/data/media/0/realdata")
                }
            });
        let params_root = std::env::var_os("PARAMS_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                if device == DeviceType::Pc {
                    comma_home.join("params")
                } else {
                    PathBuf::from("/data/params")
                }
            });
        let prefix = std::env::var("OPENPILOT_PREFIX").unwrap_or_else(|_| "d".into());
        Ok(Self {
            log_root,
            params_root,
            prefix,
            device,
        })
    }

    pub fn init_data(&self) -> Result<Vec<u8>, Error> {
        let root = std::env::var_os("PARAMS_COPY_PATH")
            .filter(|path| !path.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| self.params_root.clone());
        if self.prefix.is_empty() {
            fs::create_dir_all(&root)?;
        } else {
            Params::open(&root, &self.prefix)?;
        }
        let lock = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o775)
            .open(root.join(".lock"))?;
        lock.lock()?;
        let mut params = BTreeMap::new();
        for entry in fs::read_dir(root.join(&self.prefix))? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                continue;
            }
            let key = entry
                .file_name()
                .into_string()
                .map_err(|_| Error::Invalid("non-UTF8 Params key"))?;
            params.insert(key, read_optional(entry.path()));
        }
        drop(lock);
        for key in [
            "GitCommit",
            "GitCommitDate",
            "GitBranch",
            "GitRemote",
            "DongleId",
        ] {
            params.entry(key.to_owned()).or_default();
        }
        let mut message = capnp::message::Builder::new_default();
        let mut event = message.init_root::<event::Builder>();
        event.set_valid(true);
        event.set_log_mono_time(clock::now()?);
        let mut init = event.init_init_data();
        init.set_wall_time_nanos(u64::try_from(
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map_err(|_| Error::Invalid("wall clock before epoch"))?
                .as_nanos(),
        )?);
        init.set_version(env!("LOGGERD_VERSION"));
        init.set_dirty(std::env::var_os("CLEAN").is_none());
        init.set_device_type(self.device);
        init.set_kernel_version(String::from_utf8_lossy(&read_optional("/proc/version")).as_ref());
        init.set_os_version(String::from_utf8_lossy(&read_optional("/VERSION")).as_ref());
        let args = read_optional("/proc/cmdline");
        let args = String::from_utf8_lossy(&args);
        let args: Vec<_> = args.split_ascii_whitespace().collect();
        let mut output = init.reborrow().init_kernel_args(u32::try_from(args.len())?);
        for (index, arg) in args.iter().enumerate() {
            output.set(u32::try_from(index)?, arg);
        }
        let text = |key: &str| String::from_utf8_lossy(params.get(key).map_or(&[], Vec::as_slice));
        init.set_git_commit(text("GitCommit").as_ref());
        init.set_git_commit_date(text("GitCommitDate").as_ref());
        init.set_git_branch(text("GitBranch").as_ref());
        init.set_git_remote(text("GitRemote").as_ref());
        init.set_dongle_id(text("DongleId").as_ref());
        init.set_git_src_commit(
            String::from_utf8_lossy(&read_optional("../../git_src_commit")).as_ref(),
        );
        init.set_git_src_commit_date(
            String::from_utf8_lossy(&read_optional("../../git_src_commit_date")).as_ref(),
        );
        init.set_passive(false);
        let mut entries = init
            .reborrow()
            .init_params()
            .init_entries(u32::try_from(params.len())?);
        for (index, (key, value)) in params.iter().enumerate() {
            let mut entry = entries.reborrow().get(u32::try_from(index)?);
            entry.set_key(key.as_str())?;
            if openpilot_params::metadata(key)
                .is_none_or(|info| info.flags & openpilot_params::DONT_LOG == 0)
            {
                entry.set_value(value.as_slice())?;
            }
        }
        let commands = hardware_commands(self.device);
        let mut entries = init
            .init_commands()
            .init_entries(u32::try_from(commands.len())?);
        for (index, (key, value)) in commands.iter().enumerate() {
            let mut entry = entries.reborrow().get(u32::try_from(index)?);
            entry.set_key(key.as_str())?;
            entry.set_value(value.as_slice())?;
        }
        Ok(capnp::serialize::write_message_to_words(&message))
    }
}

fn read_optional(path: impl AsRef<Path>) -> Vec<u8> {
    fs::read(path).unwrap_or_default()
}

fn output(program: &str, args: &[&str]) -> Vec<u8> {
    match Command::new(program).args(args).output() {
        Ok(output) => output.stdout,
        Err(error) => {
            eprintln!("loggerd: init command {program}: {error}");
            Vec::new()
        }
    }
}

fn hardware_commands(device: DeviceType) -> Vec<(String, Vec<u8>)> {
    let mut commands = vec![("df -h".into(), output("df", &["-h"]))];
    if device != DeviceType::Pc {
        let mut hardware = BTreeMap::new();
        hardware.insert("/BUILD".to_owned(), read_optional("/BUILD"));
        hardware.insert(
            "lsblk".into(),
            output("lsblk", &["-o", "NAME,SIZE,STATE,VENDOR,MODEL,REV,SERIAL"]),
        );
        hardware.insert(
            "SOM ID".into(),
            read_optional("/sys/devices/platform/vendor/vendor:gpio-som-id/som_id"),
        );
        let boot = output("abctl", &["--boot_slot"]);
        hardware.insert(
            "boot slot".into(),
            boot.split(|byte| *byte == b'\n')
                .next()
                .unwrap_or_default()
                .to_vec(),
        );
        let mut temperature = read_optional("/dev/disk/by-partlabel/ssd");
        while temperature
            .last()
            .is_some_and(|byte| matches!(byte, 0 | b'\r' | b'\n'))
        {
            temperature.pop();
        }
        hardware.insert("boot temp".into(), temperature);
        for part in ["xbl", "abl", "aop", "devcfg", "xbl_config"] {
            for slot in ["a", "b"] {
                let part = format!("{part}_{slot}");
                let hash = output("sha256sum", &[&format!("/dev/disk/by-partlabel/{part}")]);
                hardware.insert(
                    part,
                    hash.split(|byte| *byte == b' ')
                        .next()
                        .unwrap_or_default()
                        .to_vec(),
                );
            }
        }
        commands.extend(hardware);
    }
    commands.push((
        "loggerd implementation".into(),
        b"openpilot-loggerd Rust".to_vec(),
    ));
    commands.push((
        "loggerd source commit".into(),
        env!("LOGGERD_SOURCE_COMMIT").as_bytes().to_vec(),
    ));
    commands.push((
        "loggerd source tree".into(),
        env!("LOGGERD_SOURCE_TREE").as_bytes().to_vec(),
    ));
    commands
}
