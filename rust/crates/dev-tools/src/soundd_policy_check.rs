//! Soundd's six policy comparisons with an independent source-derived reference.
//! The original Params binding is verified for provenance, never loaded/executed.
use crate::{
    argparse_scan::{scan_os, OptionSpec},
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::Result,
    soundd_policy_reference as reference,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, Metadata},
    io::Read,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

pub struct Options {
    pub binary: PathBuf,
    pub binding: PathBuf,
    pub output: PathBuf,
}
fn identity(m: &Metadata) -> (u64, u64, u64, i64, i64, i64, i64) {
    (
        m.dev(),
        m.ino(),
        m.len(),
        m.mtime(),
        m.mtime_nsec(),
        m.ctime(),
        m.ctime_nsec(),
    )
}
struct Input {
    file: File,
    path: PathBuf,
    metadata: Metadata,
    hash: String,
}
impl Input {
    fn open(path: &Path) -> Result<Self> {
        let fd = rustix::fs::open(
            path,
            rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::NONBLOCK | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )?;
        let mut file = File::from(fd);
        let metadata = file.metadata()?;
        if !metadata.is_file() {
            return Err(format!("soundd input is not a regular file: {}", path.display()).into());
        }
        let mut digest = Sha256::new();
        let mut bytes = [0; 65536];
        loop {
            let n = file.read(&mut bytes)?;
            if n == 0 {
                break;
            }
            digest.update(&bytes[..n]);
        }
        let input = Self {
            file,
            path: path.to_owned(),
            metadata,
            hash: format!("{:x}", digest.finalize()),
        };
        input.verify()?;
        Ok(input)
    }
    fn verify(&self) -> Result<()> {
        if identity(&self.metadata) != identity(&self.file.metadata()?)
            || identity(&self.metadata) != identity(&fs::metadata(&self.path)?)
        {
            return Err(format!("soundd input changed: {}", self.path.display()).into());
        }
        Ok(())
    }
}
fn admission(options: &Options, repository: &Path) -> Result<(Vec<Input>, usize)> {
    let mut held = Vec::new();
    let source_pins = reference::sources(repository)?;
    for (name, pin) in source_pins {
        let input = Input::open(&repository.join(name))?;
        if input.hash != pin {
            return Err("soundd policy source changed during admission".into());
        }
        held.push(input);
    }
    let path = options.binding.with_file_name("provenance.json");
    let receipt = Input::open(&path)?;
    let provenance: Value = serde_json::from_slice(&fs::read(&path)?)?;
    let binding = Input::open(&options.binding)?;
    if Some(binding.hash.as_str()) != provenance["module_sha256"].as_str() {
        return Err("original Params binding hash differs from provenance".into());
    }
    let pins = provenance["sources"]
        .as_object()
        .ok_or("invalid original Params source provenance")?;
    for (name, pin) in pins {
        let source = Input::open(&repository.join(name))?;
        if Some(source.hash.as_str()) != pin.as_str() {
            return Err(format!("original Params source changed: {name}").into());
        }
        held.push(source);
    }
    held.push(receipt);
    held.push(binding);
    Ok((held, pins.len()))
}
fn bits(row: &Value) -> Result<Vec<u32>> {
    row["samples"]
        .as_array()
        .ok_or("soundd samples must be an array")?
        .iter()
        .map(|v| Ok((v.as_f64().ok_or("soundd sample must be numeric")? as f32).to_bits()))
        .collect()
}
fn close(a: f64, b: f64) -> bool {
    a == b
        || (a.is_finite()
            && b.is_finite()
            && (a - b).abs() <= 1e-12_f64.max(1e-12 * a.abs().max(b.abs())))
}
fn compare(
    index: usize,
    raw: &[Value],
    expected: &[Value],
    actual: &[Value],
) -> Result<(usize, usize, usize)> {
    if actual.len() != expected.len() || raw.len() != expected.len() {
        return Err(format!("soundd row count differs in case {index}").into());
    }
    let mut unchanged = 0;
    let mut corrected = 0;
    let mut frames = 0;
    for (step, ((raw, left), right)) in raw.iter().zip(expected).zip(actual).enumerate() {
        for key in [
            "alert",
            "volume",
            "adjust",
            "countdown",
            "timeout",
            "filter",
        ] {
            if raw["state"][key] != left["state"][key] {
                return Err(format!(
                    "independent playback changed non-frame state: {index}/{step}/{key}"
                )
                .into());
            }
        }
        let raw_bits = bits(raw)?;
        let left_bits = bits(left)?;
        if raw_bits.len() != left_bits.len() {
            return Err("independent playback sample lengths differ".into());
        }
        for (a, b) in raw_bits.iter().zip(&left_bits) {
            if a == b {
                unchanged += 1;
            } else {
                corrected += 1;
            }
        }
        frames += usize::from(raw["state"]["frame"] != left["state"]["frame"]);
        for key in ["alert", "frame", "adjust", "countdown", "timeout"] {
            if left["state"][key] != right["state"][key] {
                return Err(format!("soundd state differs: {index}/{step}/{key}").into());
            }
        }
        for key in ["volume", "filter"] {
            let a = left["state"][key]
                .as_f64()
                .ok_or("invalid reference scalar")?;
            let b = right["state"][key]
                .as_f64()
                .ok_or("invalid candidate scalar")?;
            if !close(a, b) {
                return Err(format!("soundd scalar differs: {index}/{step}/{key}").into());
            }
        }
        if left_bits != bits(right)? {
            return Err(format!("soundd sample bits differ: {index}/{step}").into());
        }
    }
    Ok((unchanged, corrected, frames))
}
fn write(path: &Path, value: &impl serde::Serialize) -> Result<()> {
    fs::write(path, serde_json::to_vec(value)?)?;
    Ok(())
}
pub fn run(options: &Options, repository: &Path, runner: &mut impl Runner) -> Result<Value> {
    // Invalidate only this invocation's receipt before admission can fail.
    fs::create_dir_all(&options.output)?;
    let summary = options.output.join("summary.json");
    match fs::remove_file(&summary) {
        Ok(()) => (),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
        Err(e) => return Err(e.into()),
    }
    let result = (|| {
        let source_roster = reference::sources(repository)?;
        let (mut held, binding_sources) = admission(options, repository)?;
        let mut unchanged = 0;
        let mut corrected = 0;
        let mut frame_rows = 0;
        let cases = [
            (48000, 1, false),
            (48000, 2, true),
            (12000, 1, false),
            (44100, 2, true),
            (0, 1, false),
            (0, 1, true),
        ];
        for (index, (rate, channels, tizi)) in cases.into_iter().enumerate() {
            let temporary = tempfile::Builder::new().prefix("sound-source-").tempdir()?;
            let assets = if rate == 0 {
                repository.join("openpilot/selfdrive/assets")
            } else {
                reference::create_assets(temporary.path(), rate, channels)?
            };
            // Bind the actual WAV inputs used by this scenario through comparison.
            let mut waves = Vec::new();
            for folder in ["sounds", "sounds_eng", "sounds_chs"] {
                for entry in fs::read_dir(assets.join(folder))? {
                    let path = entry?.path();
                    if path.extension().is_some_and(|e| e == "wav") {
                        waves.push(Input::open(&path)?);
                    }
                }
            }
            let mut config = json!({"assets":assets,"engage_volume":0.37,"language":"en","tizi":tizi,"steps":reference::steps()});
            if rate == 0 {
                config["basedir"] = json!(repository);
            }
            let raw = reference::reference(repository, &config, false)?;
            let intended = reference::reference(repository, &config, true)?;
            let mut command = CommandSpec::new_os(
                vec![options.binary.as_os_str().to_owned()],
                Duration::from_secs(30),
            );
            command.input = Some(serde_json::to_vec(&config)?);
            let output = runner.run(&command)?;
            if output.code != 0 {
                return Err(format!(
                    "soundd candidate exited {}: {}",
                    output.code,
                    String::from_utf8_lossy(&output.stderr)
                )
                .into());
            }
            let stdout = std::str::from_utf8(&output.stdout)?;
            let native: Vec<Value> = stdout
                .lines()
                .filter(|line| line.starts_with('{'))
                .map(serde_json::from_str)
                .collect::<std::result::Result<_, _>>()?;
            for (suffix, value) in [
                ("input.json", serde_json::to_value(&config)?),
                ("source.json", serde_json::to_value(&raw)?),
                ("intended.json", serde_json::to_value(&intended)?),
                ("native.json", serde_json::to_value(&native)?),
            ] {
                write(&options.output.join(format!("{index}-{suffix}")), &value)?;
            }
            fs::write(
                options.output.join(format!("{index}-stderr.log")),
                if output.stderr.is_empty() {
                    b"no stderr\n".as_slice()
                } else {
                    &output.stderr
                },
            )?;
            let (same, changed, frames) = compare(index, &raw, &intended, &native)?;
            unchanged += same;
            corrected += changed;
            frame_rows += frames;
            for wave in &waves {
                wave.verify()?;
            }
            // Shipped files remain bound until the final report. Synthetic files
            // are private and intentionally removed with their scenario TempDir.
            if rate == 0 {
                held.extend(waves);
            }
        }
        for input in &held {
            input.verify()?;
        }
        if reference::sources(repository)? != source_roster {
            return Err("soundd source roster changed during comparison".into());
        }
        let report = json!({"passed":true,"scenarios":6,"steps_per_scenario":reference::steps().len(),"sample_comparisons":unchanged+corrected,"original_unchanged_sample_comparisons":unchanged,"intentional_clip_boundary_corrected_samples":corrected,"intentional_clip_boundary_corrected_frame_rows":frame_rows,"all_nonplayback_state_source_exact":true,"binding_sources_verified":binding_sources,"audio_initialized":false,"live_source_sha256":source_roster,"historical_source_sha256":reference::historical_sources()});
        fs::write(&summary, serde_json::to_vec_pretty(&report)?)?;
        for input in &held {
            input.verify()?;
        }
        if reference::sources(repository)? != source_roster {
            return Err("soundd source roster changed during comparison".into());
        }
        Ok(report)
    })();
    if result.is_err() {
        let _ = fs::remove_file(summary);
    }
    result
}
struct Signals(Vec<signal_hook::SigId>);
impl Drop for Signals {
    fn drop(&mut self) {
        for id in self.0.drain(..) {
            signal_hook::low_level::unregister(id);
        }
    }
}
struct Controlled<'a> {
    stop: &'a AtomicBool,
    binary: Option<Input>,
}
impl Runner for Controlled<'_> {
    fn run(&mut self, spec: &CommandSpec) -> Result<crate::qa_process::Captured> {
        if self.binary.is_none() {
            let argv = spec.effective_argv();
            self.binary = Some(Input::open(Path::new(
                argv.first().ok_or("missing soundd executable")?,
            ))?);
        }
        NativeRunner.run_controlled(spec, Instant::now() + spec.timeout, self.stop)
    }
}
pub fn cli() -> Result<()> {
    let parsed = scan_os(
        &std::env::args_os().skip(1).collect::<Vec<_>>(),
        &[
            OptionSpec::value("--binary", true),
            OptionSpec::value("--binding", true),
            OptionSpec::value("--output", true),
        ],
    )?;
    if parsed.help {
        println!("usage: check-soundd-policy --binary BINARY --binding PARAMS_BINDING --output OUTPUT\nBinding is verified for original provenance; no Python binding or audio device is initialized.");
        return Ok(());
    }
    let options = Options {
        binary: PathBuf::from(&parsed.values["--binary"]),
        binding: PathBuf::from(&parsed.values["--binding"]),
        output: PathBuf::from(&parsed.values["--output"]),
    };
    let root = std::env::var_os("OPENPILOT_RUST_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."));
    let stop = Arc::new(AtomicBool::new(false));
    let mut signals = Signals(vec![]);
    for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
        signals
            .0
            .push(signal_hook::flag::register(signal, Arc::clone(&stop))?);
    }
    // Only the real CLI executes the candidate. Runner peers in unit tests may
    // deliberately use a nonexistent argv; they do not masquerade as ELF proof.
    let result = (|| {
        fs::create_dir_all(&options.output)?;
        let summary = options.output.join("summary.json");
        if let Err(e) = fs::remove_file(&summary) {
            if e.kind() != std::io::ErrorKind::NotFound {
                return Err(e.into());
            }
        }
        let mut runner = Controlled {
            stop: &stop,
            binary: None,
        };
        let report = run(&options, &root, &mut runner)?;
        runner
            .binary
            .as_ref()
            .ok_or("soundd executable was not observed")?
            .verify()?;
        if stop.load(Ordering::SeqCst) {
            return Err("soundd policy comparison interrupted".into());
        }
        println!("{}", serde_json::to_string(&report)?);
        Ok(())
    })();
    if result.is_err() || stop.load(Ordering::SeqCst) {
        let _ = fs::remove_file(options.output.join("summary.json"));
    }
    if stop.load(Ordering::SeqCst) {
        return Err("soundd policy comparison interrupted".into());
    }
    result
}
