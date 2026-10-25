//! Current source/Rust V4L host-driver comparisons; no hardware acceptance.
use crate::{
    qa_process::{CommandSpec, Runner},
    seal::Result,
};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};
#[derive(Clone)]
pub struct Options {
    pub root: PathBuf,
    pub output: PathBuf,
    pub binary: PathBuf,
    pub native: PathBuf,
    pub library_path: Option<OsString>,
    pub shm: PathBuf,
    pub pid: u32,
}
fn text(path: &Path) -> Result<String> {
    Ok(fs::read_to_string(path)?
        .replace("\r\n", "\n")
        .replace('\r', "\n"))
}
fn hash(path: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};
    let mut f = fs::File::open(path)?;
    let mut d = Sha256::new();
    let mut b = [0; 65536];
    loop {
        let n = f.read(&mut b)?;
        if n == 0 {
            break;
        }
        d.update(&b[..n]);
    }
    Ok(format!("{:x}", d.finalize()))
}
fn quote(s: &str) -> Result<String> {
    Ok(serde_json::to_string(s)?)
}
fn pretty(path: &Path, data: &str) -> Result<()> {
    let v = crate::control_tools_policy_check::json::parse_text(data)?;
    fs::write(
        path,
        crate::control_tools_policy_check::json::pretty(&v, 0)? + "\n",
    )?;
    Ok(())
}
fn argv_json(argv: &[OsString]) -> Result<String> {
    Ok(format!(
        "[{}]",
        argv.iter()
            .map(|v| crate::python_fs::json_string(v))
            .collect::<Result<Vec<_>>>()?
            .join(",")
    ))
}
pub fn check_outcome(status: i32, output: &str, expected: Option<(i32, &str)>) -> Result<()> {
    if [
        "AddressSanitizer",
        "LeakSanitizer",
        "MemorySanitizer",
        "ThreadSanitizer",
        "UndefinedBehaviorSanitizer",
        "runtime error:",
    ]
    .iter()
    .any(|s| output.contains(s))
    {
        return Err("encoder checker observed a sanitizer failure".into());
    }
    match expected {
        None if status != 0 => Err(format!("unexpected encoder status {status}").into()),
        Some((code, diagnostic)) if status != code || !output.contains(diagnostic) => Err(format!(
            "expected encoder status {code} and {diagnostic:?}, got status {status}: {output}"
        )
        .into()),
        _ => Ok(()),
    }
}
fn run(
    runner: &mut dyn Runner,
    argv: Vec<OsString>,
    log: &Path,
    env: &BTreeMap<OsString, OsString>,
    expected: Option<(i32, &str)>,
) -> Result<i32> {
    let mut spec = CommandSpec::new_os(argv, Duration::from_secs(30));
    spec.set_os_env(env.clone());
    spec.inherit_stdin = true;
    spec.disable_core_dump = true;
    let result = runner.run_to_file(&spec, log)?;
    check_outcome(result.code, &text(log)?, expected)?;
    Ok(result.code)
}
fn source(root: &Path, output: &Path) -> Result<String> {
    let fixture = root.join("rust/tools/encoder_oracle");
    let mut hashes = vec![];
    let mut extract = |name: &str, start: &str, end: Option<&str>| -> Result<String> {
        let p = root.join(name);
        hashes.push(format!("{}:{}", quote(name)?, quote(&hash(&p)?)?));
        let t = text(&p)?;
        let begin = t.find(start).ok_or("V4L source start marker missing")?;
        let end = match end {
            Some(end) => {
                begin
                    + t[begin..]
                        .find(end)
                        .ok_or("V4L source end marker missing")?
            }
            None => t.len(),
        };
        Ok(t[begin..end].to_owned())
    };
    let pieces = [
        text(&fixture.join("v4l_adapter.h"))?,
        extract(
            "msgq_repo/msgq/visionipc/visionbuf.h",
            "enum VisionStreamType",
            None,
        )?,
        extract(
            "msgq_repo/msgq/visionipc/visionbuf_ion.cc",
            "struct IonFileHandle",
            None,
        )?,
        extract(
            "openpilot/system/loggerd/loggerd.h",
            "struct EncoderSettings",
            Some("class EncoderInfo"),
        )?,
        text(&fixture.join("v4l_base.h"))?,
        extract(
            "openpilot/system/loggerd/encoder/encoder.cc",
            "VideoEncoder::VideoEncoder",
            Some("void VideoEncoder::publisher_publish"),
        )?,
        extract(
            "openpilot/system/loggerd/encoder/v4l_encoder.h",
            "class V4LEncoder",
            None,
        )?,
        extract(
            "openpilot/system/loggerd/encoder/v4l_encoder.cc",
            "static void dequeue_buffer",
            None,
        )?,
        text(&fixture.join("v4l_main.cc"))?,
    ];
    fs::write(output.join("source-oracle.cc"), pieces.join("\n"))?;
    Ok(format!("{{{}}}", hashes.join(",")))
}
#[derive(Clone, Copy)]
struct Case {
    profile: (&'static str, u32, u32),
    scenario: &'static str,
    failure: bool,
    width: u32,
}
fn cases() -> Vec<Case> {
    let mut cases = [
        ("main", 0, 0),
        ("main", 1, 0),
        ("main", 2, 0),
        ("main", 0, 1),
        ("--stream", 0, 0),
        ("--stream", 1, 0),
        ("--stream", 2, 0),
        ("--carrot-vision-road", 0, 0),
        ("--youtube-low", 0, 0),
        ("--youtube-medium", 0, 0),
        ("--youtube", 0, 0),
        ("--youtube-wide", 0, 0),
    ]
    .into_iter()
    .map(|profile| Case {
        profile,
        scenario: "normal",
        failure: false,
        width: 128,
    })
    .collect::<Vec<_>>();
    cases.push(Case {
        profile: ("main", 0, 0),
        scenario: "normal",
        failure: false,
        width: 1928,
    });
    for scenario in [
        "poll-eintr",
        "hold-seven",
        "output-unused-timestamp",
        "header-unused-timestamp",
        "eos-unused-timestamp",
    ] {
        cases.push(Case {
            profile: ("main", 0, 0),
            scenario,
            failure: false,
            width: 128,
        });
    }
    for scenario in ["slice-size-failure", "slice-mode-failure", "compat-failure"] {
        cases.push(Case {
            profile: ("--stream", 0, 0),
            scenario,
            failure: false,
            width: 128,
        });
    }
    for scenario in [
        "crop-unavailable",
        "crop-adjusted",
        "cbr-unavailable",
        "bitrate-adjusted",
    ] {
        cases.push(Case {
            profile: ("--youtube", 0, 0),
            scenario,
            failure: false,
            width: 128,
        });
    }
    for scenario in [
        "wrong-output",
        "cbr-wrong",
        "wrong-capability",
        "wrong-offset",
        "wrong-timestamp",
        "negative-frame-timestamp",
        "input-queue-failure",
    ] {
        cases.push(Case {
            profile: ("--youtube", 0, 0),
            scenario,
            failure: true,
            width: 128,
        });
    }
    cases
}
fn failure(scenario: &str, rust: bool) -> Result<(i32, &'static str)> {
    Ok(match (scenario, rust) {
        ("wrong-output", false) => (-6, "YouTube encoder output contract rejected"),
        ("wrong-output", true) => (1, "YouTube encoder output contract rejected"),
        ("cbr-wrong", false) => (-6, "V4L2 encoder did not retain CBR_CFR rate control"),
        ("cbr-wrong", true) => (1, "V4L2 encoder did not retain CBR_CFR rate control"),
        ("wrong-capability", false) => (
            -6,
            "strcmp((const char *)cap.driver, \"msm_vidc_driver\") == 0",
        ),
        ("wrong-capability", true) => (1, "source msm_vidc encoder capability assertion"),
        ("wrong-offset", false) => (-6, "v4l_buf.m.planes[0].data_offset == 0"),
        ("wrong-offset", true) => (-6, "source V4L zero data-offset assertion"),
        ("wrong-timestamp" | "negative-frame-timestamp", false) => {
            (-6, "extra.timestamp_eof/1000 == ts")
        }
        ("wrong-timestamp" | "negative-frame-timestamp", true) => {
            (-6, "source V4L timestamp synchronization assertion")
        }
        ("input-queue-failure", false) => (-6, "VIDIOC_QBUF failed"),
        ("input-queue-failure", true) => (-6, "Invalid argument (os error 22)"),
        _ => return Err("unknown V4L expected failure".into()),
    })
}
fn words(text: &str) -> Vec<&str> {
    text.split(|ch: char| ch.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&ch))
        .filter(|v| !v.is_empty())
        .collect()
}
fn validate_trace(name: &str, directory: &Path, trace: &[String]) -> Result<()> {
    let t = text(&directory.join("trace.tsv"))?;
    let lines = crate::camera_exposure_check::shared::splitlines(&t);
    if lines.len() != 24 {
        return Err(format!("{name}: expected 24 publications, got {}", lines.len()).into());
    }
    let mut counts = BTreeMap::new();
    for row in trace {
        let parts = words(row);
        let first = parts.first().ok_or("empty driver trace line")?;
        *counts.entry(*first).or_insert(0usize) += 1;
    }
    for (event, expected) in [
        ("ION_ALLOC", 6),
        ("ION_FREE", 6),
        ("CAPTURE_HEADER", 2),
        ("CAPTURE_FRAME", 24),
        ("CAPTURE_EOS", 2),
        ("STOP", 2),
        ("RETURN_INPUT", 24),
    ] {
        let actual = counts.get(event).copied().unwrap_or_default();
        if actual != expected {
            return Err(format!("{name}: {event} count {actual}, expected {expected}").into());
        }
    }
    let returned = trace
        .iter()
        .filter(|v| v.starts_with("QUEUE_INPUT"))
        .collect::<Vec<_>>();
    for segment in 0..2 {
        let first = words(
            returned
                .get(segment * 12)
                .ok_or("missing V4L rotation input")?,
        );
        if first.get(1) != Some(&"0") {
            return Err(format!("{name}: rotation did not restore source slot order").into());
        }
    }
    Ok(())
}
pub fn execute(
    options: &Options,
    runner: &mut dyn Runner,
    space: &mut dyn FnMut(&Path) -> Result<u64>,
    stdout: &mut dyn Write,
) -> Result<()> {
    let mut opts = options.clone();
    opts.output = crate::path_resolve::resolve_non_strict(&opts.output)?;
    let output = &opts.output;
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir(output)?;
    let sources = source(&opts.root, output)?;
    let fixture = opts.root.join("rust/tools/encoder_oracle");
    let library =
        crate::path_resolve::resolve_non_strict(&opts.native)?.join("usr/lib/x86_64-linux-gnu");
    let mut ld = library.into_os_string();
    if let Some(v) = opts.library_path.as_ref().filter(|v| !v.is_empty()) {
        ld.push(":");
        ld.push(v);
    }
    let mut env: BTreeMap<OsString, OsString> = [("LD_LIBRARY_PATH".into(), ld)].into();
    let includes = [
        opts.root.clone(),
        opts.root.join("openpilot"),
        opts.root.join("third_party/linux/include"),
    ]
    .map(|p| {
        let mut v = OsString::from("-I");
        v.push(p);
        v
    });
    let mut build = ["g++", "-std=c++20", "-O1", "-g0", "-pthread"]
        .map(OsString::from)
        .to_vec();
    build.extend(includes.clone());
    build.extend([
        output.join("source-oracle.cc").into_os_string(),
        "-o".into(),
        output.join("source-oracle").into_os_string(),
    ]);
    let mut driver = [
        "g++",
        "-std=c++20",
        "-O1",
        "-g0",
        "-pthread",
        "-shared",
        "-fPIC",
    ]
    .map(OsString::from)
    .to_vec();
    driver.extend(includes);
    driver.extend([
        fixture.join("fake_v4l.cc").into_os_string(),
        "-ldl".into(),
        "-o".into(),
        output.join("fake-v4l.so").into_os_string(),
    ]);
    let builds = [build, driver];
    for (i, command) in builds.iter().enumerate() {
        let free = space(output)?;
        let growth = 128 * 1024 * 1024u64;
        let floor = 0u64;
        pretty(
            &output.join(format!("build-{i}-space.json")),
            &format!("{{\"free\":{free},\"growth\":{growth},\"floor\":{floor}}}"),
        )?;
        if free < growth + floor {
            return Err("insufficient disk headroom".into());
        }
        run(
            runner,
            command.clone(),
            &output.join(format!("build-{i}.log")),
            &env,
            None,
        )?;
    }
    pretty(
        &output.join("build-commands.json"),
        &format!("[{},{}]", argv_json(&builds[0])?, argv_json(&builds[1])?),
    )?;
    env.insert(
        "LD_PRELOAD".into(),
        output.join("fake-v4l.so").into_os_string(),
    );
    let mut results = vec![];
    for (number, case) in cases().into_iter().enumerate() {
        env.insert("ENCODER_FAKE_CASE".into(), case.scenario.into());
        let mut returns = vec![];
        let mut configs = vec![];
        // Original resolves the candidate before entering either process for this case.
        let binary = crate::path_resolve::resolve_non_strict(&opts.binary)?;
        for (kind, binary) in [("source", output.join("source-oracle")), ("rust", binary)] {
            let name = format!("case-{number:02}-{}-{kind}", case.scenario);
            let directory = output.join(&name);
            let driver = output.join(format!("{name}-driver.tsv"));
            env.insert("ENCODER_FAKE_TRACE".into(), driver.clone().into_os_string());
            let prefix = format!("encv4l-{}-{number}-{kind}", opts.pid);
            env.insert("OPENPILOT_PREFIX".into(), prefix.clone().into());
            let command = vec![
                binary.into_os_string(),
                directory.clone().into_os_string(),
                case.profile.0.into(),
                case.profile.1.to_string().into(),
                case.profile.2.to_string().into(),
                case.width.to_string().into(),
                "80".into(),
                "12".into(),
            ];
            let shm = opts.shm.join(format!("msgq_{prefix}"));
            fs::create_dir(&shm)?;
            let expected = if case.failure {
                Some(failure(case.scenario, kind == "rust")?)
            } else {
                None
            };
            let result = run(
                runner,
                command,
                &output.join(format!("{name}.log")),
                &env,
                expected,
            );
            fs::remove_dir_all(&shm)?;
            let code = result?;
            returns.push(format!("{}:{code}", quote(kind)?));
            let t = text(&driver)?;
            let lines = crate::camera_exposure_check::shared::splitlines(&t)
                .into_iter()
                .map(str::to_owned)
                .collect::<Vec<_>>();
            if !case.failure {
                validate_trace(&name, &directory, &lines)?;
            }
            configs.push(
                lines
                    .into_iter()
                    .filter(|line| {
                        [
                            "QUERYCAP",
                            "FORMAT",
                            "FPS",
                            "CROP",
                            "CONTROL",
                            "READ_CONTROL",
                            "BUFFERS",
                            "STREAM",
                        ]
                        .iter()
                        .any(|p| line.starts_with(p))
                    })
                    .collect::<Vec<_>>(),
            );
        }
        if configs[0] != configs[1] {
            return Err(format!("case {number}: ioctl configuration differs").into());
        }
        if !case.failure
            && fs::read(output.join(format!(
                "case-{number:02}-{}-source/trace.tsv",
                case.scenario
            )))? != fs::read(
                output.join(format!("case-{number:02}-{}-rust/trace.tsv", case.scenario)),
            )?
        {
            return Err(format!("case {number}: encoded metadata/header/data differs").into());
        }
        results.push(format!("{{\"profile\":[{},{},{}],\"scenario\":{},\"width\":{},\"expected_failure\":{},\"returns\":{{{}}}}}",quote(case.profile.0)?,case.profile.1,case.profile.2,quote(case.scenario)?,case.width,case.failure,returns.join(",")));
    }
    pretty(&output.join("receipt.json"),&format!("{{\"status\":\"PASS\",\"scope\":\"host scripted ioctl/memory fixture, not VIDC hardware validation\",\"source_sha256\":{sources},\"rust_binary_sha256\":{},\"driver_sha256\":{},\"source_oracle_sha256\":{},\"results\":[{}]}}",quote(&hash(&crate::path_resolve::resolve_non_strict(&opts.binary)?)?)?,quote(&hash(&output.join("fake-v4l.so"))?)?,quote(&hash(&output.join("source-oracle"))?)?,results.join(",")))?;
    writeln!(
        stdout,
        "{{\"status\": \"PASS\", \"cases\": {}}}",
        results.len()
    )?;
    Ok(())
}
pub fn cli() -> Result<()> {
    use crate::argparse_scan::{scan_os, OptionSpec};
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let parsed = scan_os(
        &args,
        &[
            OptionSpec::value("--rust", true),
            OptionSpec::value("--output", true),
            OptionSpec::value("--native", true),
        ],
    )?;
    if parsed.help {
        println!("usage: check-encoder-v4l --rust BINARY --output DIRECTORY --native DIRECTORY\nCompare source and Rust V4L controls, queues and EOS using a host driver fixture");
        return Ok(());
    }
    let root = std::env::var_os("OPENPILOT_RUST_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."));
    let opts = Options {
        root: crate::path_resolve::resolve_non_strict(&root)?,
        output: PathBuf::from(&parsed.values["--output"]),
        binary: PathBuf::from(&parsed.values["--rust"]),
        native: PathBuf::from(&parsed.values["--native"]),
        library_path: std::env::var_os("LD_LIBRARY_PATH"),
        shm: "/dev/shm".into(),
        pid: std::process::id(),
    };
    execute(
        &opts,
        &mut crate::qa_process::NativeRunner,
        &mut |p| Ok(crate::disk_space::inspect(p, 0)?.free_bytes),
        &mut std::io::stdout(),
    )
}
