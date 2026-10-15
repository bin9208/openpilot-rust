use crate::Error;
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom},
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};
#[derive(Clone)]
pub(crate) struct Paths {
    pub gadget: PathBuf,
    pub net: PathBuf,
    pub state: PathBuf,
    pub leases: PathBuf,
    pub proc: PathBuf,
}
impl Paths {
    pub fn device() -> Self {
        Self {
            gadget: "/sys/kernel/config/usb_gadget/carrot_jetlink".into(),
            net: "/sys/class/net".into(),
            state: "/dev/shm/jetlink-mobile".into(),
            leases: "/dev/shm/jetlink-mobile-leases".into(),
            proc: "/proc".into(),
        }
    }
    pub fn function(&self) -> PathBuf {
        self.gadget.join("functions/ncm.jetlink")
    }
    pub fn link(&self) -> PathBuf {
        self.gadget.join("configs/c.1/ncm.jetlink")
    }
    pub fn locked(&self, uid: u32) -> Result<File, Error> {
        fs::create_dir_all(&self.state)?;
        let meta = fs::symlink_metadata(&self.state)?;
        if !meta.is_dir() || meta.file_type().is_symlink() || meta.uid() != uid {
            return Err(Error::Contract(
                "mobile state must be root-owned directory without symlink",
            ));
        }
        fs::set_permissions(&self.state, fs::Permissions::from_mode(0o700))?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.state.join("lock"))?;
        rustix::fs::flock(&lock, rustix::fs::FlockOperation::LockExclusive)
            .map_err(std::io::Error::from)?;
        Ok(lock)
    }
    pub fn owned(&self) -> Result<(), Error> {
        if !self.state.join("ncm-owned").is_file() || !self.function().is_dir() {
            return Err(Error::Contract("NCM is not owned by native mobile helper"));
        }
        Ok(())
    }
    pub fn detached(&self) -> Result<(), Error> {
        if self.gadget.join("UDC").exists() && !text(&self.gadget.join("UDC"))?.is_empty() {
            return Err(Error::Contract(
                "unbind ep0 owner before changing mobile descriptors",
            ));
        }
        Ok(())
    }
    pub fn interface(&self) -> Result<(String, u32), Error> {
        self.owned()?;
        let name = text(&self.function().join("ifname"))?;
        if !valid_interface(&name) {
            return Err(Error::Contract("invalid NCM interface name"));
        }
        let index = text(&self.net.join(&name).join("ifindex"))?
            .parse::<u32>()
            .map_err(|_| Error::Contract("invalid NCM interface index"))?;
        if index == 0 {
            return Err(Error::Contract("invalid NCM interface index"));
        }
        Ok((name, index))
    }
}
pub(crate) fn text(path: &Path) -> Result<String, Error> {
    Ok(fs::read_to_string(path)?.trim().to_owned())
}
pub(crate) fn remove(path: &Path) -> Result<(), Error> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}
pub(crate) fn valid_interface(name: &str) -> bool {
    (1..=15).contains(&name.len())
        && name
            .bytes()
            .all(|v| v.is_ascii_alphanumeric() || b"_.:-".contains(&v))
}
pub(crate) struct Output {
    pub code: i32,
    pub stdout: String,
}
pub(crate) trait Commands {
    fn run(&self, program: &str, args: &[String]) -> Result<Output, Error>;
}
pub(crate) struct Linux;
impl Commands for Linux {
    fn run(&self, program: &str, args: &[String]) -> Result<Output, Error> {
        let mut output = tempfile::tempfile()?;
        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(output.try_clone()?)
            .stderr(output.try_clone()?)
            .spawn()?;
        let until = Instant::now() + Duration::from_secs(15);
        let status = loop {
            if let Some(status) = child.try_wait()? {
                break status;
            }
            if Instant::now() >= until {
                child.kill()?;
                child.wait()?;
                return Err(Error::Deadline);
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        output.seek(SeekFrom::Start(0))?;
        let mut bytes = Vec::new();
        output.take(1 << 20).read_to_end(&mut bytes)?;
        Ok(Output {
            code: status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&bytes).into_owned(),
        })
    }
}
pub(crate) fn run(os: &impl Commands, program: &str, args: &[&str]) -> Result<Output, Error> {
    os.run(
        program,
        &args.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>(),
    )
}
pub(crate) fn checked(os: &impl Commands, program: &str, args: &[&str]) -> Result<String, Error> {
    let output = run(os, program, args)?;
    if output.code != 0 {
        return Err(Error::Native(format!(
            "mobile {program}: {}",
            output.stdout.chars().take(1024).collect::<String>()
        )));
    }
    Ok(output.stdout)
}
