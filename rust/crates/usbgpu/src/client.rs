use crate::{
    worker::{Info, Input, Metadata},
    Error,
};
mod protocol;
use protocol::receive;
use rustix::process::{kill_process, Pid, Signal};
use std::{
    io::Write,
    os::unix::fs::FileExt,
    path::Path,
    process::{Child, Command, Stdio},
    sync::{atomic::AtomicBool, Arc},
    thread,
    time::{Duration, Instant},
};

pub struct Frame<'a> {
    pub main: &'a [u8],
    pub extra: &'a [u8],
    pub transforms: [[f32; 9]; 2],
    pub inputs: &'a [f32],
}
pub struct Client {
    child: Child,
    shared: tempfile::NamedTempFile,
    pub info: Info,
    packed: Vec<u8>,
    output: Vec<u8>,
    values: Vec<f32>,
    first: bool,
    failed: bool,
    cancelled: Arc<AtomicBool>,
}

pub struct Launch<'a> {
    pub worker: &'a Path,
    pub model: &'a Path,
    pub camera: [u32; 2],
    pub timeout: Duration,
    pub cancelled: Arc<AtomicBool>,
}

fn validate(info: &Info, camera: [u32; 2]) -> Result<(), Error> {
    let mut inputs = info
        .layout
        .iter()
        .filter(|(name, _)| ["desire", "traffic_convention", "action_t"].contains(&name.as_str()))
        .collect::<Vec<_>>();
    inputs.sort_by_key(|(_, view)| view.offset);
    let expected = Info::new(
        Metadata {
            model_sha256: String::new(),
            checkpoint: info.checkpoint.clone(),
            output_count: info.output_count,
            output_slices: info.output_slices.clone(),
            inputs: inputs
                .into_iter()
                .map(|(name, view)| Input {
                    name: name.clone(),
                    shape: view.shape.clone(),
                })
                .collect(),
        },
        camera,
    )?;
    if &expected != info {
        return Err(Error::Contract("worker ready layout mismatch"));
    }
    Ok(())
}

impl Client {
    pub fn load(worker: &Path, model: &Path, camera: [u32; 2]) -> Result<Self, Error> {
        Self::launch(Launch {
            worker,
            model,
            camera,
            timeout: Duration::from_secs(110),
            cancelled: Arc::new(AtomicBool::new(false)),
        })
    }
    pub fn launch(launch: Launch<'_>) -> Result<Self, Error> {
        let command = Command::new(launch.worker);
        Self::from_command(command, launch)
    }
    pub fn launch_with_assets(
        launch: Launch<'_>,
        binding: &crate::worker_artifact::Binding,
    ) -> Result<Self, Error> {
        let mut command = Command::new(launch.worker);
        command
            .env("USBGPU_ASSETS_ROOT", binding.root())
            .env("USBGPU_ASSETS_MANIFEST_SHA256", binding.manifest_sha256());
        Self::from_command(command, launch)
    }
    pub fn load_command(command: Command, model: &Path, camera: [u32; 2]) -> Result<Self, Error> {
        Self::from_command(
            command,
            Launch {
                worker: Path::new(""),
                model,
                camera,
                timeout: Duration::from_secs(110),
                cancelled: Arc::new(AtomicBool::new(false)),
            },
        )
    }
    fn from_command(mut command: Command, launch: Launch<'_>) -> Result<Self, Error> {
        let Launch {
            model,
            camera,
            timeout,
            cancelled,
            ..
        } = launch;
        let shared = tempfile::NamedTempFile::new()?;
        let child = command
            .arg(model)
            .arg(shared.path())
            .args(camera.map(|dimension| dimension.to_string()))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;
        let placeholder = Info {
            size: 0,
            input_bytes: 0,
            output_count: 0,
            layout: Default::default(),
            input_shapes: Default::default(),
            output_slices: Default::default(),
            checkpoint: String::new(),
            frame_size: 0,
        };
        let mut client = Self {
            child,
            shared,
            info: placeholder,
            packed: Vec::new(),
            output: Vec::new(),
            values: Vec::new(),
            first: true,
            failed: false,
            cancelled,
        };
        let stdout = client
            .child
            .stdout
            .as_mut()
            .ok_or(Error::Contract("missing worker output"))?;
        let info: Info = serde_json::from_slice(&receive(stdout, timeout, &client.cancelled)?)?;
        validate(&info, camera)?;
        if client.shared.as_file().metadata()?.len() != info.size as u64 {
            return Err(Error::Contract("worker shared file length mismatch"));
        }
        client.packed = vec![0; info.input_bytes];
        client.output = vec![0; info.output_count * 4];
        client.values = vec![0.0; info.output_count];
        client.info = info;
        Ok(client)
    }
    pub fn run(&mut self, frame: Frame<'_>) -> Result<&[f32], Error> {
        if self.failed {
            return Err(Error::Contract("model worker stopped after failure"));
        }
        if let Err(error) = self.run_inner(frame) {
            self.failed = true;
            self.stop();
            return Err(error);
        }
        Ok(&self.values)
    }
    fn run_inner(&mut self, frame: Frame<'_>) -> Result<(), Error> {
        if frame.inputs.len() < 12
            || frame.main.len() != self.info.frame_size
            || frame.extra.len() != self.info.frame_size
        {
            return Err(Error::Contract("model worker input layout mismatch"));
        }
        for (target, value) in self.packed[..72]
            .chunks_exact_mut(4)
            .zip(frame.transforms.into_iter().flatten())
        {
            target.copy_from_slice(&value.to_le_bytes());
        }
        for (name, values) in [
            ("desire", &frame.inputs[..8]),
            ("traffic_convention", &frame.inputs[8..10]),
            ("action_t", &frame.inputs[10..12]),
        ] {
            let view = &self.info.layout[name];
            for (target, value) in self.packed[view.offset..view.offset + values.len() * 4]
                .chunks_exact_mut(4)
                .zip(values)
            {
                target.copy_from_slice(&value.to_le_bytes());
            }
        }
        for (name, bytes) in [("img", frame.main), ("big_img", frame.extra)] {
            let offset = self.info.layout[name].offset;
            self.packed[offset..offset + bytes.len()].copy_from_slice(bytes);
        }
        self.shared.as_file().write_all_at(&self.packed, 0)?;
        self.child
            .stdin
            .as_mut()
            .ok_or(Error::Contract("missing worker input"))?
            .write_all(b"r")?;
        let stdout = self
            .child
            .stdout
            .as_mut()
            .ok_or(Error::Contract("missing worker output"))?;
        if receive(
            stdout,
            Duration::from_secs(if self.first { 20 } else { 1 }),
            &self.cancelled,
        )? != b"1\n"
        {
            return Err(Error::Contract("invalid model worker response"));
        }
        self.first = false;
        self.shared
            .as_file()
            .read_exact_at(&mut self.output, self.info.input_bytes as u64)?;
        for (value, bytes) in self.values.iter_mut().zip(self.output.chunks_exact(4)) {
            *value = f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        }
        if !self.values.iter().all(|value| value.is_finite()) {
            return Err(Error::Contract("non-finite model output"));
        }
        Ok(())
    }
    pub fn raw_output(&self) -> &[u8] {
        &self.output
    }
    fn stop(&mut self) {
        if matches!(self.child.try_wait(), Ok(Some(_))) {
            return;
        }
        if let Some(pid) = i32::try_from(self.child.id()).ok().and_then(Pid::from_raw) {
            let _ = kill_process(pid, Signal::TERM);
        }
        let started = Instant::now();
        while started.elapsed() < Duration::from_secs(2) {
            if matches!(self.child.try_wait(), Ok(Some(_))) {
                return;
            }
            thread::sleep(Duration::from_millis(5));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
impl Drop for Client {
    fn drop(&mut self) {
        self.stop();
    }
}
