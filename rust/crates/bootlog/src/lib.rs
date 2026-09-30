//! Native port of system/loggerd/bootlog.cc; original MIT provenance retained.
pub mod snapshot;

use openpilot_cereal::log_capnp::event;
use openpilot_loggerd::{
    clock, diagnostics, metadata::Environment, raw_file::RawFile, writer::identifier,
};
use openpilot_logging::{log_site, record::Level};
use openpilot_params::Params;
use std::{
    collections::BTreeMap,
    fs,
    io::{self, Write},
    os::unix::{ffi::OsStrExt, fs::DirBuilderExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::SystemTime,
};

pub use openpilot_loggerd::Error;
pub const JOURNAL_COMMAND: &str =
    "[ -x \"$(command -v journalctl)\" ] && journalctl -o short-monotonic";

pub struct Inputs {
    pub pstore: PathBuf,
    pub launch_log: PathBuf,
}

impl Default for Inputs {
    fn default() -> Self {
        Self {
            pstore: "/sys/fs/pstore".into(),
            launch_log: "/tmp/launch_log".into(),
        }
    }
}

fn read(path: impl AsRef<Path>) -> Vec<u8> {
    fs::read(path).unwrap_or_default()
}

fn command_output() -> Vec<u8> {
    let Ok(output) = Command::new("/bin/sh")
        .args(["-c", JOURNAL_COMMAND])
        .stderr(Stdio::inherit())
        .output()
    else {
        return Vec::new();
    };
    // util::check_output appends each fgets(128) buffer as a NUL-terminated string.
    output
        .stdout
        .split_inclusive(|byte| *byte == b'\n')
        .flat_map(|line| line.chunks(127))
        .flat_map(|chunk| {
            chunk
                .split(|byte| *byte == 0)
                .next()
                .unwrap_or_default()
                .iter()
                .copied()
        })
        .collect()
}

pub fn boot_event(inputs: &Inputs) -> Result<Vec<u8>, Error> {
    let mut message = capnp::message::Builder::new_default();
    let mut event = message.init_root::<event::Builder>();
    event.set_log_mono_time(clock::now()?);
    event.set_valid(true);
    let mut boot = event.init_boot();
    boot.set_wall_time_nanos(u64::try_from(
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_err(|_| Error::Invalid("wall clock before epoch"))?
            .as_nanos(),
    )?);
    let mut files = BTreeMap::new();
    if let Ok(directory) = fs::read_dir(&inputs.pstore) {
        for entry in directory.flatten() {
            if entry.file_type().is_ok_and(|kind| !kind.is_dir()) {
                files.insert(entry.file_name().as_bytes().to_vec(), read(entry.path()));
            }
        }
    }
    let mut entries = boot
        .reborrow()
        .init_pstore()
        .init_entries(u32::try_from(files.len())?);
    for (index, (name, bytes)) in files.iter().enumerate() {
        let mut entry = entries.reborrow().get(u32::try_from(index)?);
        entry.set_key(capnp::text::Reader(name))?;
        entry.set_value(bytes.as_slice())?;
    }
    let mut command = boot.reborrow().init_commands().init_entries(1).get(0);
    command.set_key(JOURNAL_COMMAND)?;
    command.set_value(command_output().as_slice())?;
    let launch = read(&inputs.launch_log);
    boot.init_launch_log(u32::try_from(launch.len())?)
        .as_bytes_mut()
        .copy_from_slice(&launch);
    Ok(capnp::serialize::write_message_to_words(&message))
}

struct BootWriter {
    encoder: zstd::stream::write::Encoder<'static, RawFile>,
    pending: Vec<u8>,
}

impl BootWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.pending.extend_from_slice(bytes);
        if self.pending.len() >= zstd::zstd_safe::CCtx::in_size() {
            self.encoder.write_all(&self.pending)?;
            self.pending.clear();
        }
        Ok(())
    }

    fn finish(mut self) -> io::Result<()> {
        self.encoder.write_all(&self.pending)?;
        self.encoder.finish()?.finish_checked()
    }
}

pub fn capture(environment: &Environment, inputs: &Inputs) -> Result<PathBuf, Error> {
    let params = Params::for_runtime()?;
    let id = identifier(&params, "BootCount")?;
    let directory = environment.log_root.join("boot");
    let path = directory.join(format!("{id}.zst"));
    diagnostics::emit(
        log_site!(),
        Level::Warning,
        format!("bootlog to {}", path.display()),
    );
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o775)
        .create(directory)?;
    let mut file = BootWriter {
        encoder: zstd::stream::write::Encoder::new(RawFile::create(&path)?, 10)?,
        pending: Vec::new(),
    };
    file.write(&environment.init_data()?)?;
    file.write(&boot_event(inputs)?)?;
    // The original updates this before ZstdFileWriter's final compression/close.
    let _ = params.put("CurrentBootlog", id.as_bytes());
    file.finish()?;
    Ok(path)
}
