//! Pinned FFmpeg 6.1.1 provider builds, derived from build_{loggerd,runtime}_ffmpeg.py.
use crate::{
    disk_space,
    qa_process::{CommandSpec, Runner},
    seal::{digest, Result},
};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
pub const VERSION: &str = "6.1.1";
pub const SOURCE_SHA256: &str = "8684f4b00f94b85461884c3719382f1261f0d9eb3d59640a1f4ac0873616f968";
pub const SOURCE_URL: &str = "https://ffmpeg.org/releases/ffmpeg-6.1.1.tar.xz";
const GROWTH: u64 = 3 * 1024 * 1024 * 1024;
#[derive(Clone, Debug)]
pub enum Profile {
    Logger {
        ar: PathBuf,
        ranlib: PathBuf,
    },
    Runtime {
        sdk: PathBuf,
        cxx: PathBuf,
        binutils: PathBuf,
        binutils_libraries: Option<PathBuf>,
    },
}
#[derive(Clone, Debug)]
pub struct Options {
    pub root: PathBuf,
    pub archive: Option<PathBuf>,
    pub cc: PathBuf,
    pub jobs: u8,
    pub profile: Profile,
    pub headers_only: bool,
}
fn text(path: &Path) -> Result<String> {
    Ok(path.to_str().ok_or("non-UTF8 FFmpeg path")?.to_owned())
}
fn check(value: bool, message: &str) -> Result<()> {
    if value {
        Ok(())
    } else {
        Err(message.into())
    }
}
fn write_json(path: &Path, value: &impl serde::Serialize) -> Result<()> {
    crate::opencv_build::write_json(path, value)
}

pub fn configure(options: &Options, source: &Path, prefix: &Path) -> Result<Vec<String>> {
    check(
        (1..=2).contains(&options.jobs),
        "FFmpeg jobs must be 1 or 2",
    )?;
    let mut args = vec![
        text(&source.join("configure"))?,
        format!("--prefix={}", text(prefix)?),
        "--arch=aarch64".into(),
        "--target-os=linux".into(),
        "--enable-cross-compile".into(),
        format!("--cc={}", text(&options.cc)?),
    ];
    match &options.profile {
        Profile::Logger { ar, ranlib } => {
            args.extend([
                format!("--ar={}", text(ar)?),
                format!("--ranlib={}", text(ranlib)?),
            ]);
            args.extend(
                [
                    "--enable-static",
                    "--disable-shared",
                    "--enable-pic",
                    "--disable-autodetect",
                    "--disable-everything",
                    "--disable-debug",
                    "--disable-doc",
                    "--disable-programs",
                    "--disable-avdevice",
                    "--disable-avfilter",
                    "--disable-swresample",
                    "--disable-swscale",
                    "--disable-postproc",
                    "--enable-avcodec",
                    "--enable-avformat",
                    "--enable-avutil",
                    "--enable-encoder=aac,ffvhuff",
                    "--enable-muxer=mpegts,matroska",
                    "--enable-protocol=file",
                    "--enable-parser=h264",
                    "--enable-bsf=h264_mp4toannexb,hevc_mp4toannexb,aac_adtstoasc",
                ]
                .map(String::from),
            );
        }
        Profile::Runtime {
            sdk, cxx, binutils, ..
        } => {
            args.push(format!("--cxx={}", text(cxx)?));
            for tool in ["ar", "ranlib", "nm", "strip"] {
                args.push(format!(
                    "--{tool}={}",
                    text(&binutils.join(format!("aarch64-linux-gnu-{tool}")))?
                ));
            }
            args.extend(
                [
                    "--enable-shared",
                    "--disable-static",
                    "--enable-pic",
                    "--disable-debug",
                    "--disable-doc",
                    "--disable-ffplay",
                    "--disable-autodetect",
                    "--enable-gpl",
                    "--enable-version3",
                    "--enable-libx264",
                    "--enable-libvpx",
                    "--enable-openssl",
                    "--enable-zlib",
                ]
                .map(String::from),
            );
            let libraries = sdk.join("usr/lib/aarch64-linux-gnu");
            args.extend([
                format!(
                    "--extra-cflags=-I{} -I{}",
                    text(&sdk.join("usr/include"))?,
                    text(&sdk.join("usr/include/aarch64-linux-gnu"))?
                ),
                format!(
                    "--extra-ldflags=-L{} -Wl,-rpath-link,{}",
                    text(&libraries)?,
                    text(&libraries)?
                ),
            ]);
        }
    }
    Ok(args)
}
fn environment(profile: &Profile) -> Result<BTreeMap<String, String>> {
    let mut env = BTreeMap::new();
    if let Profile::Runtime {
        sdk,
        binutils_libraries,
        ..
    } = profile
    {
        env.insert("PKG_CONFIG_SYSROOT_DIR".into(), text(sdk)?);
        env.insert(
            "PKG_CONFIG_LIBDIR".into(),
            text(&sdk.join("usr/lib/aarch64-linux-gnu/pkgconfig"))?,
        );
        env.insert("PKG_CONFIG_PATH".into(), String::new());
        if let Some(path) = binutils_libraries {
            let mut search = vec![path.clone()];
            if let Some(existing) = std::env::var_os("LD_LIBRARY_PATH") {
                search.extend(std::env::split_paths(&existing));
            }
            env.insert(
                "LD_LIBRARY_PATH".into(),
                std::env::join_paths(search)?
                    .into_string()
                    .map_err(|_| "non-UTF8 native binutils library path")?,
            );
        }
    }
    Ok(env)
}
fn phase(
    root: &Path,
    work: &Path,
    name: &str,
    mut command: CommandSpec,
    runner: &mut impl Runner,
) -> Result<Value> {
    let space = disk_space::inspect(root, GROWTH)?;
    space.require()?;
    let argv = command.argv.clone();
    command.cwd = Some(work.into());
    command.stdout_limit = 64 * 1024 * 1024;
    command.stderr_limit = 16 * 1024 * 1024;
    write_json(&root.join(format!("{name}.command.json")), &command.argv)?;
    let started = Instant::now();
    let result = runner.run(&command);
    match result {
        Ok(result) => {
            let mut combined = fs::OpenOptions::new()
                .append(true)
                .create(true)
                .open(root.join("build.log"))?;
            combined.write_all(&result.stdout)?;
            combined.write_all(&result.stderr)?;
            fs::write(root.join(format!("{name}.stdout.log")), result.stdout)?;
            fs::write(root.join(format!("{name}.stderr.log")), result.stderr)?;
            let record = json!({"argv":argv,"exit_code":result.code,"seconds":started.elapsed().as_secs_f64(),"available_bytes":space.free_bytes});
            write_json(&root.join(format!("{name}.result.json")), &record)?;
            check(
                result.code == 0,
                &format!("FFmpeg {name} failed with exit {}", result.code),
            )?;
            Ok(record)
        }
        Err(error) => {
            if let Some(crate::qa_process::CaptureError::Timeout { stdout, stderr, .. }) =
                error.downcast_ref::<crate::qa_process::CaptureError>()
            {
                fs::write(root.join(format!("{name}.stdout.log")), stdout)?;
                fs::write(root.join(format!("{name}.stderr.log")), stderr)?;
            }
            write_json(
                &root.join(format!("{name}.result.json")),
                &json!({"argv":argv,"error":error.to_string()}),
            )?;
            Err(error)
        }
    }
}
fn prepare(root: &Path, archive: &Path, expected: &str) -> Result<PathBuf> {
    let metadata = fs::metadata(archive)?;
    check(
        metadata.is_file() && metadata.len() <= 32 * 1024 * 1024,
        "FFmpeg source archive exceeds limit or is not a regular file",
    )?;
    disk_space::inspect(root, GROWTH)?.require()?;
    let temporary = tempfile::tempdir_in(root)?;
    let mut frozen = tempfile::NamedTempFile::new_in(root)?;
    let copied = io::copy(
        &mut fs::File::open(archive)?.take(32 * 1024 * 1024 + 1),
        &mut frozen,
    )?;
    check(
        copied <= 32 * 1024 * 1024
            && copied == metadata.len()
            && digest(frozen.path())? == expected,
        "FFmpeg source checksum mismatch",
    )?;
    crate::encoder_stage::extract_source(frozen.path(), temporary.path(), VERSION)?;
    let fresh = temporary.path().join(format!("ffmpeg-{VERSION}"));
    let source = root.join(format!("ffmpeg-{VERSION}"));
    if source.exists() || source.is_symlink() {
        check(
            !source.is_symlink() && source.is_dir(),
            "FFmpeg source is not an owned directory",
        )?;
        check(
            crate::opencv_build::source_hashes(&fresh)?
                == crate::opencv_build::source_hashes(&source)?,
            "FFmpeg cached source differs from pinned archive",
        )?;
    } else {
        fs::rename(fresh, &source)?;
    }
    Ok(source)
}
pub fn build(options: &Options, runner: &mut impl Runner) -> Result<Value> {
    build_with_pin(options, runner, SOURCE_SHA256)
}
fn build_with_pin(options: &Options, runner: &mut impl Runner, expected: &str) -> Result<Value> {
    check(
        (1..=2).contains(&options.jobs),
        "FFmpeg jobs must be 1 or 2",
    )?;
    disk_space::inspect(&options.root, GROWTH)?.require()?;
    fs::create_dir_all(&options.root)?;
    let root = options.root.canonicalize()?;
    let archive = match &options.archive {
        Some(path) => path.canonicalize()?,
        None => {
            let archive = root.join(format!("ffmpeg-{VERSION}.tar.xz"));
            if !archive.exists() {
                let download = tempfile::NamedTempFile::new_in(&root)?;
                phase(
                    &root,
                    &root,
                    "download",
                    CommandSpec::new(
                        vec![
                            "curl".into(),
                            "--fail".into(),
                            "--location".into(),
                            "--retry".into(),
                            "3".into(),
                            "--connect-timeout".into(),
                            "15".into(),
                            "--max-time".into(),
                            "180".into(),
                            "--max-filesize".into(),
                            (32 * 1024 * 1024).to_string(),
                            SOURCE_URL.into(),
                            "--output".into(),
                            text(download.path())?,
                        ],
                        Duration::from_secs(600),
                    ),
                    runner,
                )?;
                check(
                    digest(download.path())? == expected,
                    "FFmpeg download checksum mismatch",
                )?;
                download.persist(&archive)?;
            }
            archive
        }
    };
    let source = prepare(&root, &archive, expected)?;
    let before = crate::opencv_build::source_hashes(&source)?;
    let prefix = root.join("install");
    let work = root.join("build");
    check(
        !prefix.is_symlink() && !work.is_symlink(),
        "FFmpeg output directories must not be symlinks",
    )?;
    fs::create_dir_all(&work)?;
    fs::write(work.join("target-architecture.mak"), "include ffbuild/config.mak\n.PHONY: verify-target\nverify-target:\n\t$(CC) $(CPPFLAGS) $(CFLAGS) -dM -E -x c /dev/null\n")?;
    let mut effective = options.clone();
    if let Profile::Runtime {
        sdk,
        cxx,
        binutils,
        binutils_libraries,
    } = &mut effective.profile
    {
        effective.cc = options.cc.canonicalize()?;
        *sdk = sdk.canonicalize()?;
        *cxx = cxx.canonicalize()?;
        *binutils = binutils.canonicalize()?;
        if let Some(path) = binutils_libraries {
            *path = path.canonicalize()?;
        }
    }
    let env = environment(&effective.profile)?;
    let command = configure(&effective, &source, &prefix)?;
    write_json(
        &root.join("invocation.json"),
        &json!({"url":SOURCE_URL,"source_sha256":expected,"configure":command}),
    )?;
    fs::write(root.join("build.log"), b"")?;
    let compiler_hash = if effective.cc.is_file() {
        Some(digest(&effective.cc)?)
    } else {
        None
    };
    let mut record = json!({"source_version":VERSION,"source_sha256":expected,"url":SOURCE_URL,"license":if matches!(options.profile,Profile::Logger {..}) {"LGPL-2.1-or-later"} else {"GPL-3.0-or-later"},"profile":if matches!(options.profile,Profile::Logger {..}) {"logger-minimal-static"} else {"runtime-shared-gplv3"},"headers_only":options.headers_only,"compiler":{"command":effective.cc,"command_file_sha256":compiler_hash},"environment":env,"completed":false,"commands":[]});
    let mut steps = vec![("configure", command, Duration::from_secs(120))];
    steps.push((
        "target-architecture",
        vec![
            "make".into(),
            "--no-print-directory".into(),
            "-s".into(),
            "-f".into(),
            "target-architecture.mak".into(),
            "verify-target".into(),
        ],
        Duration::from_secs(120),
    ));
    if options.headers_only {
        steps.push((
            "install-headers",
            vec!["make".into(), "install-headers".into()],
            Duration::from_secs(120),
        ));
    } else {
        steps.extend([
            (
                "make",
                vec!["make".into(), format!("-j{}", options.jobs)],
                Duration::from_secs(3600),
            ),
            (
                "install",
                vec!["make".into(), "install".into()],
                Duration::from_secs(120),
            ),
        ]);
    }
    for (name, argv, timeout) in steps {
        let mut command = CommandSpec::new(argv, timeout);
        command.env = env.clone();
        let step = phase(&root, &work, name, command, runner);
        match step {
            Ok(step) => record["commands"]
                .as_array_mut()
                .ok_or("missing FFmpeg commands")?
                .push(step),
            Err(error) => {
                record["error"] = error.to_string().into();
                write_json(&root.join("build-record.json"), &record)?;
                return Err(error);
            }
        }
        write_json(&root.join("build-record.json"), &record)?;
        if name == "target-architecture" {
            let macros = fs::read_to_string(root.join("target-architecture.stdout.log"))?;
            verify_target(&macros)?;
            record["target_predefines_sha256"] =
                digest(&root.join("target-architecture.stdout.log"))?.into();
        }
    }
    check(
        before == crate::opencv_build::source_hashes(&source)?,
        "FFmpeg source changed during build",
    )?;
    let headers = verify_headers(&source, &work, &prefix)?;
    let license = prefix.join("share/runtime-provider");
    fs::create_dir_all(&license)?;
    let names = if matches!(options.profile, Profile::Runtime { .. }) {
        vec!["COPYING.GPLv3", "LICENSE.md"]
    } else {
        vec!["COPYING.LGPLv2.1", "LICENSE.md"]
    };
    for name in names {
        fs::copy(source.join(name), license.join(name))?;
    }
    record["headers"] = serde_json::to_value(headers)?;
    record["files"] = serde_json::to_value(crate::opencv_build::source_hashes(&prefix)?)?;
    record["scope"] = if options.headers_only { "AArch64 target configure and public header installation only; no provider library or codec build" } else { "external FFmpeg provider build; not vehicle validation" }.into();
    record["completed"] = true.into();
    write_json(&root.join("build-record.json"), &record)?;
    Ok(json!({"prefix":prefix,"record":record}))
}
fn verify_target(macros: &str) -> Result<()> {
    let definitions: BTreeMap<_, _> = macros
        .lines()
        .filter_map(|line| line.strip_prefix("#define ")?.split_once(' '))
        .collect();
    check(
        definitions.get("__aarch64__") == Some(&"1"),
        "FFmpeg compiler is not an AArch64 target compiler",
    )?;
    let endian = definitions
        .get("__BYTE_ORDER__")
        .ok_or("target compiler omitted byte order")?;
    check(
        *endian == "__ORDER_LITTLE_ENDIAN__"
            || Some(endian) == definitions.get("__ORDER_LITTLE_ENDIAN__"),
        "FFmpeg target compiler is not little-endian",
    )
}
fn verify_headers(source: &Path, work: &Path, prefix: &Path) -> Result<BTreeMap<String, String>> {
    let include = prefix.join("include");
    let hashes = crate::opencv_build::source_hashes(&include)?;
    check(!hashes.is_empty(), "FFmpeg installed no public headers")?;
    for (relative, sha) in &hashes {
        check(
            relative.ends_with(".h"),
            "unexpected FFmpeg public include artifact",
        )?;
        let original = source.join(relative);
        let original = if matches!(
            relative.as_str(),
            "libavutil/avconfig.h" | "libavutil/ffversion.h"
        ) {
            work.join(relative)
        } else {
            original
        };
        check(
            digest(&original)? == *sha,
            "FFmpeg installed header differs from pinned source or target configuration",
        )?;
    }
    let config = fs::read_to_string(work.join("config.h"))?;
    let avconfig = fs::read_to_string(include.join("libavutil/avconfig.h"))?;
    for expected in ["#define ARCH_AARCH64 1", "#define HAVE_BIGENDIAN 0"] {
        check(
            config.lines().any(|line| line == expected),
            "FFmpeg configure architecture differs from AArch64 target",
        )?;
    }
    let fast = config
        .lines()
        .find_map(|line| line.strip_prefix("#define HAVE_FAST_UNALIGNED "))
        .ok_or("missing FFmpeg target unaligned configuration")?;
    check(
        matches!(fast, "0" | "1"),
        "invalid FFmpeg target unaligned configuration",
    )?;
    for expected in [
        "#define AV_HAVE_BIGENDIAN 0".to_owned(),
        format!("#define AV_HAVE_FAST_UNALIGNED {fast}"),
    ] {
        check(
            avconfig.lines().any(|line| line == expected),
            "FFmpeg public configuration differs from AArch64 target",
        )?;
    }
    Ok(hashes)
}
pub fn cli(runtime: bool) -> Result<()> {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if runtime
        && arguments
            .iter()
            .any(|arg| arg == "--prepare-released-headers")
    {
        return crate::native_ffmpeg_headers::cli(&arguments);
    }
    let mut values = BTreeMap::new();
    let mut args = arguments.into_iter();
    let mut headers_only = false;
    while let Some(key) = args.next() {
        if key == "--headers-only" {
            check(!headers_only, "duplicate --headers-only")?;
            headers_only = true;
            continue;
        }
        let allowed = if runtime {
            &[
                "--root",
                "--archive",
                "--sdk",
                "--cc",
                "--cxx",
                "--binutils",
                "--binutils-libraries",
                "--jobs",
            ][..]
        } else {
            &["--root", "--archive", "--cc", "--ar", "--ranlib", "--jobs"][..]
        };
        check(
            allowed.contains(&key.as_str()),
            "unknown FFmpeg build argument",
        )?;
        let value = args.next().ok_or("FFmpeg argument requires a value")?;
        check(
            values.insert(key, value).is_none(),
            "duplicate FFmpeg build argument",
        )?;
    }
    let get = |name: &str| -> Result<PathBuf> {
        Ok(values
            .get(name)
            .ok_or_else(|| format!("missing {name}"))?
            .into())
    };
    let profile = if runtime {
        Profile::Runtime {
            sdk: get("--sdk")?,
            cxx: get("--cxx")?,
            binutils: get("--binutils")?,
            binutils_libraries: values.get("--binutils-libraries").map(PathBuf::from),
        }
    } else {
        Profile::Logger {
            ar: get("--ar")?,
            ranlib: get("--ranlib")?,
        }
    };
    let options = Options {
        root: get("--root")?,
        archive: if runtime {
            Some(get("--archive")?)
        } else {
            values.get("--archive").map(PathBuf::from)
        },
        cc: get("--cc")?,
        jobs: values.get("--jobs").map_or(Ok(2), |value| value.parse())?,
        profile,
        headers_only,
    };
    println!("{}", build(&options, &mut crate::qa_process::NativeRunner)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::qa_process::Captured;
    fn options(root: &Path) -> Options {
        Options {
            root: root.into(),
            archive: None,
            cc: "/bin/true".into(),
            jobs: 2,
            profile: Profile::Logger {
                ar: "ar".into(),
                ranlib: "ranlib".into(),
            },
            headers_only: false,
        }
    }
    fn archive(path: &Path, members: &[(&str, &[u8])]) -> String {
        let output = fs::File::create(path).unwrap();
        let mut tar = tar::Builder::new(xz2::write::XzEncoder::new(output, 1));
        for (name, data) in members {
            let mut header = tar::Header::new_gnu();
            header.set_mode(0o755);
            header.set_size(data.len() as u64);
            header.set_cksum();
            tar.append_data(&mut header, format!("ffmpeg-{VERSION}/{name}"), *data)
                .unwrap();
        }
        tar.into_inner().unwrap().finish().unwrap();
        digest(path).unwrap()
    }
    struct BuildRunner {
        calls: Vec<CommandSpec>,
        fail: Option<&'static str>,
        bad_header: bool,
    }
    impl Runner for BuildRunner {
        fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
            self.calls.push(spec.clone());
            if spec.argv.iter().any(|arg| arg == "verify-target") {
                return Ok(Captured {code:0,stdout:b"#define __aarch64__ 1\n#define __BYTE_ORDER__ __ORDER_LITTLE_ENDIAN__\n#define __ORDER_LITTLE_ENDIAN__ 1234\n".to_vec(),stderr:vec![]});
            }
            let work = spec.cwd.as_ref().unwrap();
            let root = work.parent().unwrap();
            let install = root.join("install/include/libavutil");
            if spec.argv[0].ends_with("configure") {
                fs::create_dir_all(work.join("libavutil"))?;
                fs::write(work.join("config.h"),"#define ARCH_AARCH64 1\n#define HAVE_BIGENDIAN 0\n#define HAVE_FAST_UNALIGNED 1\n")?;
                fs::write(
                    work.join("libavutil/avconfig.h"),
                    "#define AV_HAVE_BIGENDIAN 0\n#define AV_HAVE_FAST_UNALIGNED 1\n",
                )?;
                fs::write(
                    work.join("libavutil/ffversion.h"),
                    "#define FFMPEG_VERSION \"6.1.1\"\n",
                )?;
            } else if spec
                .argv
                .get(1)
                .is_some_and(|v| v == "install" || v == "install-headers")
            {
                fs::create_dir_all(&install)?;
                fs::copy(
                    work.join("libavutil/avconfig.h"),
                    install.join("avconfig.h"),
                )?;
                fs::copy(
                    work.join("libavutil/ffversion.h"),
                    install.join("ffversion.h"),
                )?;
                fs::copy(
                    root.join(format!("ffmpeg-{VERSION}/libavutil/version.h")),
                    install.join("version.h"),
                )?;
                if self.bad_header {
                    fs::write(install.join("version.h"), "changed")?;
                }
            }
            Ok(Captured {
                code: if self
                    .fail
                    .is_some_and(|name| spec.argv.iter().any(|arg| arg == name))
                {
                    7
                } else {
                    0
                },
                stdout: b"build output".to_vec(),
                stderr: vec![],
            })
        }
    }
    fn fixture(temp: &Path) -> (Options, String) {
        let path = temp.join("source.tar.xz");
        let sha = archive(
            &path,
            &[
                ("configure", b"fixture configure"),
                ("libavutil/version.h", b"version fixture"),
                ("COPYING.LGPLv2.1", b"LGPL fixture"),
                ("COPYING.GPLv3", b"GPL fixture"),
                ("LICENSE.md", b"source license"),
            ],
        );
        let mut result = options(&temp.join("build-root"));
        result.archive = Some(path);
        (result, sha)
    }
    #[test]
    fn original_profiles_preserve_codec_flags_sdk_and_bounded_jobs() {
        let mut settings = options(Path::new("root"));
        let logger = configure(&settings, Path::new("source"), Path::new("prefix")).unwrap();
        for flag in [
            "--enable-static",
            "--disable-shared",
            "--enable-encoder=aac,ffvhuff",
            "--enable-bsf=h264_mp4toannexb,hevc_mp4toannexb,aac_adtstoasc",
            "--disable-swscale",
        ] {
            assert!(logger.iter().any(|s| s == flag));
        }
        assert!(!logger.iter().any(|s| s == "--enable-gpl"));
        settings.profile = Profile::Runtime {
            sdk: "sdk".into(),
            cxx: "c++".into(),
            binutils: "tools".into(),
            binutils_libraries: None,
        };
        let runtime = configure(&settings, Path::new("source"), Path::new("prefix")).unwrap();
        for flag in [
            "--enable-shared",
            "--disable-static",
            "--enable-gpl",
            "--enable-version3",
            "--enable-libx264",
            "--enable-libvpx",
            "--enable-openssl",
            "--enable-zlib",
            "--strip=tools/aarch64-linux-gnu-strip",
        ] {
            assert!(runtime.iter().any(|s| s == flag));
        }
        let env = environment(&settings.profile).unwrap();
        assert_eq!(env["PKG_CONFIG_SYSROOT_DIR"], "sdk");
        assert_eq!(env["PKG_CONFIG_PATH"], "");
        for invalid in [0, 3, 255] {
            settings.jobs = invalid;
            assert!(configure(&settings, Path::new("source"), Path::new("prefix")).is_err());
        }
    }
    #[test]
    fn header_only_installs_verified_source_and_actual_target_config_without_make_build() {
        let temp = tempfile::tempdir().unwrap();
        let (mut settings, sha) = fixture(temp.path());
        settings.headers_only = true;
        let mut runner = BuildRunner {
            calls: vec![],
            fail: None,
            bad_header: false,
        };
        let result = build_with_pin(&settings, &mut runner, &sha).unwrap();
        assert_eq!(runner.calls.len(), 3);
        assert_eq!(runner.calls[2].argv, ["make", "install-headers"]);
        assert_eq!(result["record"]["headers_only"], true);
        assert!(settings
            .root
            .join("install/share/runtime-provider/COPYING.LGPLv2.1")
            .is_file());
        assert!(!settings.root.join("install/lib").exists());
        assert_eq!(runner.calls[0].timeout, Duration::from_secs(120));
        assert!(runner
            .calls
            .iter()
            .all(|spec| spec.stdout_limit == 64 * 1024 * 1024));
        build_with_pin(&settings, &mut runner, &sha).unwrap();
    }
    #[test]
    fn build_failure_and_wrong_installed_headers_never_claim_success() {
        for (failure, bad_header) in [(Some("-j2"), false), (None, true)] {
            let temp = tempfile::tempdir().unwrap();
            let (settings, sha) = fixture(temp.path());
            let mut runner = BuildRunner {
                calls: vec![],
                fail: failure,
                bad_header,
            };
            assert!(build_with_pin(&settings, &mut runner, &sha).is_err());
            if failure.is_some() {
                assert_eq!(runner.calls.len(), 3);
                assert!(settings.root.join("make.result.json").is_file());
            }
        }
    }
    #[test]
    fn wrong_archive_and_modified_cached_source_are_rejected_before_commands() {
        let temp = tempfile::tempdir().unwrap();
        let (settings, sha) = fixture(temp.path());
        let mut runner = BuildRunner {
            calls: vec![],
            fail: None,
            bad_header: false,
        };
        assert!(build(&settings, &mut runner).is_err());
        assert!(runner.calls.is_empty());
        build_with_pin(&settings, &mut runner, &sha).unwrap();
        fs::write(
            settings.root.join(format!("ffmpeg-{VERSION}/configure")),
            "changed",
        )
        .unwrap();
        runner.calls.clear();
        assert!(build_with_pin(&settings, &mut runner, &sha).is_err());
        assert!(runner.calls.is_empty());
    }

    #[test]
    fn runtime_profile_records_exact_environment_license_and_target_probe() {
        let temp = tempfile::tempdir().unwrap();
        let (mut settings, sha) = fixture(temp.path());
        let sdk = temp.path().join("sdk");
        let binutils = temp.path().join("binutils");
        let host_libraries = temp.path().join("host-libraries");
        for path in [&sdk, &binutils, &host_libraries] {
            fs::create_dir(path).unwrap();
        }
        settings.profile = Profile::Runtime {
            sdk: sdk.clone(),
            cxx: "/bin/true".into(),
            binutils: binutils.clone(),
            binutils_libraries: Some(host_libraries.clone()),
        };
        let mut runner = BuildRunner {
            calls: vec![],
            fail: None,
            bad_header: false,
        };
        let result = build_with_pin(&settings, &mut runner, &sha).unwrap();
        assert_eq!(runner.calls.len(), 4);
        assert_eq!(runner.calls[2].argv, ["make", "-j2"]);
        assert_eq!(runner.calls[3].argv, ["make", "install"]);
        assert_eq!(runner.calls[2].timeout, Duration::from_secs(3600));
        assert_eq!(
            runner.calls[0].env["PKG_CONFIG_SYSROOT_DIR"],
            sdk.to_str().unwrap()
        );
        assert_eq!(runner.calls[0].env["PKG_CONFIG_PATH"], "");
        assert!(
            std::env::split_paths(&runner.calls[0].env["LD_LIBRARY_PATH"])
                .next()
                .unwrap()
                == host_libraries
        );
        assert_eq!(result["record"]["profile"], "runtime-shared-gplv3");
        assert!(settings
            .root
            .join("install/share/runtime-provider/COPYING.GPLv3")
            .is_file());
        let installed = settings.root.join("install/include/libavutil/avconfig.h");
        fs::write(
            &installed,
            "#define AV_HAVE_BIGENDIAN 1\n#define AV_HAVE_FAST_UNALIGNED 0\n",
        )
        .unwrap();
        assert!(verify_headers(
            &settings.root.join(format!("ffmpeg-{VERSION}")),
            &settings.root.join("build"),
            &settings.root.join("install")
        )
        .is_err());
    }
    #[test]
    fn host_and_big_endian_compiler_macros_fail_closed() {
        for macros in ["#define __x86_64__ 1\n#define __BYTE_ORDER__ __ORDER_LITTLE_ENDIAN__\n", "#define __aarch64__ 1\n#define __BYTE_ORDER__ __ORDER_BIG_ENDIAN__\n#define __ORDER_LITTLE_ENDIAN__ 1234\n"] {
            assert!(verify_target(macros).is_err());
        }
        verify_target("#define __aarch64__ 1\n#define __BYTE_ORDER__ 1234\n#define __ORDER_LITTLE_ENDIAN__ 1234\n").unwrap();
    }
}
