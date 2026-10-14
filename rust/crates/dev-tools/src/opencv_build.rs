//! Native pinned OpenCV preparation and three provider build profiles.
use crate::{
    disk_space,
    qa_process::{CommandSpec, Runner},
    seal::{digest, Result},
};
use flate2::read::GzDecoder;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::Write,
    os::unix::fs::symlink,
    path::{Component, Path, PathBuf},
    time::Duration,
};
pub const REVISION: &str = "fe38fc608f6acb8b68953438a62305d8318f4fcd";
pub const ARCHIVE_SHA256: &str = "6a7508554941c1a698c243b2212b2985ce59a65a3e0348d53c2158607d801e61";
pub const PATCH_SHA256: &str = "7d17439697d9786b43b2a44dc9eecea34381adb6d2f14c6892266b6699a49aec";
pub const PATCHED: [(&str, &str, &str); 2] = [
    (
        "modules/core/include/opencv2/core/hal/intrin_sse.hpp",
        "9ea43239afc6dd6915c37715074d8d6351a055b3890aa71fb644864dcb9ececd",
        "dcd1c37f0c3817c176953e5c15037497954e1ff523467e3379fcb1e0d3d31ffb",
    ),
    (
        "modules/imgproc/src/drawing.cpp",
        "feb12ebea7b08419289a46d043afeee665b77217d1faed89d86ad30d2d8af0e0",
        "bed85690b61fa5060708fa6860062c60f2bc4b5f5d5a738748aa3c0df033f99f",
    ),
];
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Profile {
    Xiaoge,
    Webcam,
    Runtime,
}
impl Profile {
    pub fn libraries(self) -> &'static [&'static str] {
        match self {
            Self::Xiaoge => &["core", "imgproc", "dnn"],
            Self::Webcam => &["core", "imgproc", "imgcodecs", "videoio"],
            Self::Runtime => &["core", "imgproc", "imgcodecs", "videoio", "dnn"],
        }
    }
}
#[derive(Clone, Debug)]
pub struct Providers {
    pub ffmpeg: PathBuf,
    pub sdk: PathBuf,
    pub cross: PathBuf,
    pub compiler_root: PathBuf,
}
#[derive(Clone, Debug)]
pub struct Options {
    pub profile: Profile,
    pub root: PathBuf,
    pub output: PathBuf,
    pub source: Option<PathBuf>,
    pub archive: Option<PathBuf>,
    pub prepare_only: bool,
    pub toolchain: Option<PathBuf>,
    pub sanitizers: bool,
    pub providers: Option<Providers>,
    pub growth: u64,
}
pub fn write_json(path: &Path, value: &impl serde::Serialize) -> Result<()> {
    let mut file = File::create(path)?;
    serde_json::to_writer_pretty(&mut file, value)?;
    file.write_all(b"\n")?;
    Ok(())
}
fn check(ok: bool, message: &str) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(message.into())
    }
}
fn text(path: &Path) -> Result<String> {
    Ok(path.to_str().ok_or("non-UTF8 OpenCV path")?.to_owned())
}
pub fn safe_relative(path: &Path) -> Result<()> {
    check(
        !path.as_os_str().is_empty()
            && path
                .components()
                .all(|part| matches!(part, Component::Normal(_))),
        "unsafe OpenCV artifact path",
    )
}
pub fn phase(
    options: &Options,
    name: &str,
    argv: Vec<String>,
    env: &BTreeMap<String, String>,
    runner: &mut impl Runner,
) -> Result<Vec<u8>> {
    let space = disk_space::inspect(&options.output, options.growth)?;
    space.require()?;
    write_json(&options.output.join(format!("{name}.command.json")), &argv)?;
    write_json(
        &options.output.join(format!("{name}.space.json")),
        &json!({"free_bytes":space.free_bytes,"growth_bytes":options.growth}),
    )?;
    let mut command = CommandSpec::new(argv, Duration::from_secs(7200));
    command.env = env.clone();
    command.stdout_limit = 64 * 1024 * 1024;
    command.stderr_limit = 64 * 1024 * 1024;
    let result = runner.run(&command);
    let log = options.output.join(format!("{name}.log"));
    match result {
        Ok(captured) => {
            let mut file = File::create(log)?;
            file.write_all(&captured.stdout)?;
            file.write_all(&captured.stderr)?;
            write_json(
                &options.output.join(format!("{name}.exit.json")),
                &json!({"code":captured.code}),
            )?;
            check(
                captured.code == 0,
                &format!("OpenCV {name} failed with exit {}", captured.code),
            )?;
            Ok([captured.stdout, captured.stderr].concat())
        }
        Err(error) => {
            if let Some(crate::qa_process::CaptureError::Timeout { stdout, stderr, .. }) =
                error.downcast_ref::<crate::qa_process::CaptureError>()
            {
                let mut file = File::create(log)?;
                file.write_all(stdout)?;
                file.write_all(stderr)?;
            }
            write_json(
                &options.output.join(format!("{name}.exit.json")),
                &json!({"error":error.to_string()}),
            )?;
            Err(error)
        }
    }
}
pub fn source_hashes(source: &Path) -> Result<BTreeMap<String, String>> {
    fn walk(root: &Path, current: &Path, found: &mut BTreeMap<String, String>) -> Result<()> {
        for entry in fs::read_dir(current)? {
            let entry = entry?;
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)?;
            if metadata.file_type().is_symlink() {
                check(
                    !path.is_dir(),
                    "linked OpenCV source directory is unsupported",
                )?;
                check(
                    path.canonicalize()?.starts_with(root.canonicalize()?),
                    "OpenCV source link escapes source",
                )?;
            }
            if path.is_dir() {
                walk(root, &path, found)?;
            } else if path.is_file() {
                found.insert(text(path.strip_prefix(root)?)?, digest(&path)?);
            } else {
                return Err("unsupported OpenCV source node".into());
            }
        }
        Ok(())
    }
    let mut found = BTreeMap::new();
    walk(source, source, &mut found)?;
    Ok(found)
}
pub fn verify_source(source: &Path) -> Result<()> {
    for (name, _, patched) in PATCHED {
        check(
            digest(&source.join(name))? == patched,
            "pinned OpenCV hardening output differs",
        )?;
    }
    let identity: Value = serde_json::from_reader(File::open(
        source
            .parent()
            .ok_or("source parent missing")?
            .join("source.json"),
    )?)?;
    check(
        identity["revision"] == REVISION
            && identity["external_hardening_patch_sha256"] == PATCH_SHA256
            && identity["sha256"] == ARCHIVE_SHA256,
        "OpenCV source identity differs from pinned hardened provider",
    )?;
    if let Some(expected) = identity
        .get("native_source_files_sha256")
        .and_then(Value::as_str)
    {
        let manifest = source
            .parent()
            .ok_or("source parent missing")?
            .join("source-files.json");
        check(
            digest(&manifest)? == expected,
            "OpenCV prepared source manifest changed",
        )?;
        let files: BTreeMap<String, String> = serde_json::from_reader(File::open(manifest)?)?;
        check(
            source_hashes(source)? == files,
            "OpenCV prepared source tree changed",
        )?;
    }
    Ok(())
}
fn extract(archive: &Path, output: &Path) -> Result<PathBuf> {
    let prefix = format!("opencv-{REVISION}");
    let temporary = tempfile::tempdir_in(output)?;
    let mut package = tar::Archive::new(GzDecoder::new(File::open(archive)?));
    let mut seen = BTreeSet::new();
    let mut total = 0_u64;
    for entry in package.entries()? {
        let mut entry = entry?;
        // GitHub codeload includes a global PAX metadata record, not a filesystem node.
        if entry.header().entry_type().is_pax_global_extensions() {
            continue;
        }
        let path = entry.path()?.into_owned();
        safe_relative(&path)?;
        check(
            path.components()
                .next()
                .is_some_and(|part| part.as_os_str() == prefix.as_str()),
            &format!("OpenCV archive has wrong source root: {}", path.display()),
        )?;
        check(seen.insert(path.clone()), "duplicate OpenCV archive member")?;
        let kind = entry.header().entry_type();
        check(
            kind.is_file() || kind.is_dir(),
            "unsupported linked OpenCV archive member",
        )?;
        total = total
            .checked_add(entry.size())
            .ok_or("OpenCV archive size overflow")?;
        check(
            total <= 2 * 1024 * 1024 * 1024 && seen.len() <= 100000,
            "oversized OpenCV source archive",
        )?;
        check(
            entry.unpack_in(temporary.path())?,
            "OpenCV archive escaped extraction root",
        )?;
    }
    let source = output.join(&prefix);
    check(!source.exists(), "OpenCV source already exists")?;
    fs::rename(temporary.path().join(prefix), &source)?;
    Ok(source)
}
fn harden(source: &Path, options: &Options, runner: &mut impl Runner) -> Result<()> {
    let patch = options
        .root
        .join("rust/crates/opencv-runtime/native/opencv-defined-access.patch");
    check(
        digest(&patch)? == PATCH_SHA256,
        "OpenCV hardening patch identity differs",
    )?;
    let original = PATCHED
        .iter()
        .map(|(name, expected, _)| Ok(digest(&source.join(name))? == *expected))
        .collect::<Result<Vec<_>>>()?;
    if original.iter().all(|value| *value) {
        phase(
            options,
            "hardening",
            vec![
                "patch".into(),
                "--batch".into(),
                "--forward".into(),
                "-p1".into(),
                "-d".into(),
                text(source)?,
                "-i".into(),
                text(&patch)?,
            ],
            &BTreeMap::new(),
            runner,
        )?;
    }
    for (name, _, patched) in PATCHED {
        check(
            digest(&source.join(name))? == patched,
            "pinned OpenCV hardening input/output differs",
        )?;
    }
    fs::copy(patch, options.output.join("opencv-defined-access.patch"))?;
    let files: BTreeMap<_, _> = PATCHED
        .iter()
        .map(|(name, original, patched)| (*name, json!({"original":original,"patched":patched})))
        .collect();
    write_json(
        &options.output.join("hardening.json"),
        &json!({"patch_sha256":PATCH_SHA256,"files":files}),
    )
}
pub fn prepare(options: &Options, runner: &mut impl Runner) -> Result<PathBuf> {
    let space = disk_space::inspect(&options.output, options.growth)?;
    space.require()?;
    fs::create_dir_all(&options.output)?;
    let archive = options
        .archive
        .clone()
        .unwrap_or_else(|| options.output.join("opencv.tar.gz"));
    let url = format!("https://codeload.github.com/opencv/opencv/tar.gz/{REVISION}");
    if !archive.exists() {
        let parent = archive.parent().ok_or("archive parent missing")?;
        fs::create_dir_all(parent)?;
        let temporary = tempfile::NamedTempFile::new_in(parent)?;
        phase(
            options,
            "download",
            vec![
                "curl".into(),
                "--retry".into(),
                "3".into(),
                "--connect-timeout".into(),
                "20".into(),
                "--max-time".into(),
                "180".into(),
                "--max-filesize".into(),
                (256 * 1024 * 1024).to_string(),
                "-fsSL".into(),
                url.clone(),
                "-o".into(),
                text(temporary.path())?,
            ],
            &BTreeMap::new(),
            runner,
        )?;
        check(
            digest(temporary.path())? == ARCHIVE_SHA256,
            "OpenCV archive SHA256 differs",
        )?;
        temporary.persist_noclobber(&archive)?;
    }
    check(
        digest(&archive)? == ARCHIVE_SHA256,
        "OpenCV archive SHA256 differs",
    )?;
    let source = options.output.join(format!("opencv-{REVISION}"));
    if fs::symlink_metadata(&source).is_ok() {
        check(
            source.is_dir() && !source.is_symlink(),
            "OpenCV prepared source conflicts with an existing node",
        )?;
    } else {
        extract(&archive, &options.output)?;
    }
    let previous = options.output.join("source.json");
    if previous.is_file() {
        let identity: Value = serde_json::from_reader(File::open(previous)?)?;
        if identity.get("native_source_files_sha256").is_some() {
            verify_source(&source)?;
        }
    }
    harden(&source, options, runner)?;
    write_json(
        &options.output.join("source-files.json"),
        &source_hashes(&source)?,
    )?;
    let source_files_sha256 = digest(&options.output.join("source-files.json"))?;
    write_json(
        &options.output.join("source.json"),
        &json!({"revision":REVISION,"sha256":ARCHIVE_SHA256,"url":url,
        "external_hardening_patch_sha256":PATCH_SHA256,"native_source_files_sha256":source_files_sha256,"free_before":space.free_bytes,
        "python_reference_revision":"b4c5ec4042f097e2a5b386b9d413ec7333d0a184","reference_difference":"only modules/python/test/test_fitline.py and test_mser.py; native source identical"}),
    )?;
    Ok(source)
}

pub fn configure(
    profile: Profile,
    source: &Path,
    output: &Path,
    sanitizers: bool,
    toolchain: Option<&Path>,
    clang: bool,
) -> Result<Vec<String>> {
    let mut command = vec![
        "cmake".into(),
        "-S".into(),
        text(source)?,
        "-B".into(),
        text(&output.join("build"))?,
        format!(
            "-DCMAKE_BUILD_TYPE={}",
            if sanitizers {
                "RelWithDebInfo"
            } else {
                "Release"
            }
        ),
        format!("-DBUILD_LIST={}", profile.libraries().join(",")),
        format!(
            "-DOPENCV_DOWNLOAD_PATH={}",
            text(&output.join("downloads"))?
        ),
    ];
    command.extend(
        [
            "-DBUILD_SHARED_LIBS=ON",
            "-DCMAKE_POSITION_INDEPENDENT_CODE=ON",
            "-DBUILD_TESTS=OFF",
            "-DBUILD_PERF_TESTS=OFF",
            "-DBUILD_EXAMPLES=OFF",
            "-DBUILD_opencv_apps=OFF",
            "-DBUILD_opencv_python3=OFF",
            "-DOPENCV_PYTHON_SKIP_DETECTION=ON",
            "-DBUILD_opencv_python2=OFF",
            "-DBUILD_JAVA=OFF",
            "-DWITH_IPP=OFF",
            "-DWITH_OPENCL=OFF",
            "-DWITH_LAPACK=OFF",
            "-DWITH_TBB=OFF",
            "-DWITH_ITT=OFF",
        ]
        .map(String::from),
    );
    if profile == Profile::Xiaoge {
        command.extend(
            [
                "-DOPENCV_DNN_CUDA=OFF",
                "-DBUILD_PROTOBUF=ON",
                "-DWITH_PROTOBUF=ON",
            ]
            .map(String::from),
        );
    } else {
        command.extend(
            [
                "-DWITH_ADE=OFF",
                "-DWITH_GSTREAMER=OFF",
                "-DWITH_GTK=OFF",
                "-DWITH_QT=OFF",
                "-DWITH_1394=OFF",
                "-DWITH_OBSENSOR=OFF",
                "-DWITH_V4L=ON",
                "-DWITH_FFMPEG=ON",
                "-DWITH_JPEG=ON",
                "-DBUILD_JPEG=ON",
                "-DWITH_PNG=OFF",
                "-DWITH_TIFF=OFF",
                "-DWITH_WEBP=OFF",
                "-DWITH_OPENJPEG=OFF",
                "-DWITH_OPENEXR=OFF",
                "-DWITH_AVIF=OFF",
                "-DWITH_JPEGXL=OFF",
                "-DVIDEOIO_ENABLE_PLUGINS=OFF",
            ]
            .map(String::from),
        );
        if profile == Profile::Webcam {
            command.push("-DWITH_PROTOBUF=OFF".into());
        } else {
            command.extend(
                [
                    "-DBUILD_PROTOBUF=ON",
                    "-DWITH_PROTOBUF=ON",
                    "-DOPENCV_DNN_CUDA=OFF",
                    "-DCMAKE_SKIP_RPATH=ON",
                    "-DOPENCV_FFMPEG_ENABLE_LIBAVDEVICE=OFF",
                ]
                .map(String::from),
            );
            command.push(format!(
                "-DZLIB_INCLUDE_DIR:PATH={}",
                text(&output.join("target-include"))?
            ));
        }
    }
    command.push("-DCMAKE_INSTALL_LIBDIR=lib".into());
    command.push(format!(
        "-DCMAKE_INSTALL_PREFIX={}",
        text(&output.join("install"))?
    ));
    if let Some(toolchain) = toolchain {
        command.push(format!("-DCMAKE_TOOLCHAIN_FILE={}", text(toolchain)?));
    }
    if sanitizers {
        check(
            profile == Profile::Xiaoge,
            "sanitizers require the xiaoge profile",
        )?;
        let flags = "-fsanitize=address,undefined -fno-omit-frame-pointer";
        command.extend([
            format!("-DCMAKE_C_FLAGS={flags}"),
            format!("-DCMAKE_CXX_FLAGS={flags}"),
            format!(
                "-DCMAKE_SHARED_LINKER_FLAGS={flags}{}",
                if clang { " -shared-libasan" } else { "" }
            ),
        ]);
    }
    Ok(command)
}
fn cmake_string(path: &Path) -> Result<String> {
    Ok(text(path)?.replace('\\', "\\\\").replace('"', "\\\""))
}
fn toolchain(providers: &Providers, output: &Path) -> Result<PathBuf> {
    let roots = [&providers.ffmpeg, &providers.sdk, &providers.compiler_root]
        .into_iter()
        .map(|path| cmake_string(path))
        .collect::<Result<Vec<_>>>()?
        .join(";");
    let links = [
        providers.ffmpeg.join("lib"),
        providers.sdk.join("usr/lib/aarch64-linux-gnu"),
        providers.cross.join("agnos/libraries"),
    ]
    .iter()
    .map(|path| Ok(format!("-Wl,-rpath-link,{}", cmake_string(path)?)))
    .collect::<Result<Vec<_>>>()?
    .join(" ");
    let path = output.join("aarch64.cmake");
    fs::write(&path,format!("set(CMAKE_SYSTEM_NAME Linux)\nset(CMAKE_SYSTEM_PROCESSOR aarch64)\nset(CMAKE_C_COMPILER \"{}\")\nset(CMAKE_CXX_COMPILER \"{}\")\nset(CMAKE_FIND_ROOT_PATH \"{roots}\")\nset(CMAKE_FIND_ROOT_PATH_MODE_PROGRAM NEVER)\nset(CMAKE_FIND_ROOT_PATH_MODE_LIBRARY ONLY)\nset(CMAKE_FIND_ROOT_PATH_MODE_INCLUDE ONLY)\nset(CMAKE_FIND_ROOT_PATH_MODE_PACKAGE ONLY)\nset(CMAKE_SHARED_LINKER_FLAGS_INIT \"{links}\")\nset(CMAKE_EXE_LINKER_FLAGS_INIT \"{links}\")\n",cmake_string(&providers.cross.join("cc.sh"))?,cmake_string(&providers.cross.join("cxx.sh"))?))?;
    Ok(path)
}
fn runtime_providers(
    options: &Options,
    providers: &Providers,
    runner: &mut impl Runner,
) -> Result<(PathBuf, BTreeMap<String, String>)> {
    let include = options.output.join("target-include");
    let target = providers.sdk.join("usr/include").canonicalize()?;
    if fs::symlink_metadata(&include).is_ok() {
        check(
            include.is_symlink() && include.canonicalize()? == target,
            "target header alias conflicts with configured SDK",
        )?;
    } else {
        symlink(&target, &include)?;
    }
    let environment = BTreeMap::from([
        (
            "PKG_CONFIG_LIBDIR".into(),
            text(&providers.ffmpeg.join("lib/pkgconfig"))?,
        ),
        ("PKG_CONFIG_PATH".into(), "".into()),
        ("PKG_CONFIG_SYSROOT_DIR".into(), "".into()),
    ]);
    write_json(
        &options.output.join("pkg-config-environment.json"),
        &environment,
    )?;
    phase(
        options,
        "ffmpeg-versions",
        [
            "pkg-config",
            "--modversion",
            "libavcodec",
            "libavformat",
            "libavutil",
            "libswscale",
        ]
        .map(String::from)
        .to_vec(),
        &environment,
        runner,
    )?;
    Ok((toolchain(providers, &options.output)?, environment))
}
fn copy_license(source: &Path, licenses: &Path, name: &str, relative: &str) -> Result<()> {
    let from = source.join(relative);
    check(
        fs::metadata(&from)?.len() > 0,
        "empty OpenCV dependency license",
    )?;
    fs::copy(from, licenses.join(name))?;
    Ok(())
}
fn licenses(
    profile: Profile,
    source: &Path,
    output: &Path,
    before: &BTreeMap<String, String>,
) -> Result<()> {
    let licenses = output.join("licenses");
    fs::create_dir_all(&licenses)?;
    let mut files = vec![
        (
            if profile == Profile::Xiaoge {
                "opencv-Apache-2.0.txt"
            } else {
                "OpenCV-Apache-2.0.txt"
            },
            "LICENSE",
        ),
        ("zlib-License.txt", "3rdparty/zlib/LICENSE"),
    ];
    if profile != Profile::Webcam {
        files.extend([
            ("protobuf-BSD-3-Clause.txt", "3rdparty/protobuf/LICENSE"),
            (
                "flatbuffers-Apache-2.0.txt",
                "3rdparty/flatbuffers/LICENSE.txt",
            ),
        ]);
    }
    if profile != Profile::Xiaoge {
        files.push((
            "libjpeg-turbo-licenses.md",
            "3rdparty/libjpeg-turbo/LICENSE.md",
        ));
    }
    if profile == Profile::Runtime {
        files.extend([
            ("OpenCV-COPYRIGHT.txt", "COPYRIGHT"),
            (
                "libjpeg-turbo-README.ijg",
                "3rdparty/libjpeg-turbo/README.ijg",
            ),
        ]);
    }
    for (name, relative) in files {
        copy_license(source, &licenses, name, relative)?;
    }
    if profile == Profile::Runtime {
        let notice = fs::read_to_string(source.join("hal/carotene/src/common.hpp"))?;
        let end = notice
            .find("*/")
            .ok_or("missing carotene license comment")?
            + 2;
        fs::write(
            licenses.join("carotene-BSD-3-Clause.txt"),
            format!("{}\n", &notice[..end]),
        )?;
        let kleidicv =
            output.join("build/3rdparty/kleidicv/kleidicv-0.7.0/LICENSES/Apache-2.0.txt");
        fs::copy(&kleidicv, licenses.join("KleidiCV-Apache-2.0.txt"))?;
        let archive = output
            .join("downloads/kleidicv/e8f94e427bd78a745afa5c8cd073b416-kleidicv-0.7.0.tar.gz");
        write_json(
            &licenses.join("external-hal-provenance.json"),
            &json!({"carotene_notice_source":"hal/carotene/src/common.hpp","carotene_notice_source_sha256":before.get("hal/carotene/src/common.hpp").ok_or("missing carotene source hash")?,"kleidicv_version":"0.7.0","kleidicv_archive_sha256":digest(&archive)?,"kleidicv_license_sha256":digest(&kleidicv)?,"discovery":"Pinned OpenCV hal/kleidicv dependency recipe; recorded external ARM acceleration providers."}),
        )?;
    }
    Ok(())
}
fn files_under(root: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    if !root.is_dir() {
        return Err("OpenCV install directory missing".into());
    }
    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        if path.is_dir() {
            check(
                !path.is_symlink(),
                "OpenCV installed directory is a symlink",
            )?;
            files_under(&path, files)?;
        } else if path.is_file() {
            files.push(path);
        }
    }
    Ok(())
}
fn complete(options: &Options, source: &Path, before: &BTreeMap<String, String>) -> Result<Value> {
    check(
        &source_hashes(source)? == before,
        "external build changed its read-only source tree",
    )?;
    let output = &options.output;
    let mut installed = Vec::new();
    files_under(&output.join("install/lib"), &mut installed)?;
    let libraries: Vec<_> = installed
        .iter()
        .filter(|path| {
            path.file_name().is_some_and(|name| {
                let name = name.to_string_lossy();
                name.starts_with("libopencv_") && name.ends_with(".so.4.13.0")
            })
        })
        .cloned()
        .collect();
    let expected: BTreeSet<_> = options
        .profile
        .libraries()
        .iter()
        .map(|name| format!("libopencv_{name}.so.4.13.0"))
        .collect();
    let actual: BTreeSet<_> = libraries
        .iter()
        .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    check(actual == expected, "unexpected OpenCV provider libraries")?;
    licenses(options.profile, source, output, before)?;
    let mut retained: Vec<_> = installed
        .into_iter()
        .filter(|path| {
            path.file_name().is_some_and(|name| {
                name.to_string_lossy().starts_with("libopencv_")
                    && name.to_string_lossy().contains(".so")
            })
        })
        .collect();
    let mut headers = Vec::new();
    files_under(&output.join("install/include/opencv4"), &mut headers)?;
    retained.extend(headers.into_iter().filter(|path| {
        path.file_name()
            .is_some_and(|name| name.to_string_lossy().contains(".h"))
    }));
    files_under(&output.join("licenses"), &mut retained)?;
    retained.push(output.join("source-files.json"));
    if options.profile == Profile::Xiaoge {
        retained.extend([
            output.join("source.json"),
            output.join("hardening.json"),
            output.join("opencv-defined-access.patch"),
        ]);
        retained.extend(PATCHED.iter().map(|(name, _, _)| source.join(name)));
    }
    if options.profile == Profile::Runtime {
        let patch = options
            .root
            .join("rust/crates/opencv-runtime/native/opencv-defined-access.patch");
        check(
            digest(&patch)? == PATCH_SHA256,
            "hardening patch changed during build",
        )?;
        fs::copy(patch, output.join("opencv-defined-access.patch"))?;
        fs::copy(
            source
                .parent()
                .ok_or("source parent missing")?
                .join("source.json"),
            output.join("source.json"),
        )?;
        retained.extend([
            output.join("source.json"),
            output.join("opencv-defined-access.patch"),
            output.join("aarch64.cmake"),
        ]);
    }
    let mut hashes = BTreeMap::new();
    for path in retained {
        hashes.insert(text(path.strip_prefix(output)?)?, digest(&path)?);
    }
    let library_hashes: BTreeMap<_, _> = libraries
        .iter()
        .map(|path| {
            let relative = text(path.strip_prefix(output)?)?;
            Ok((
                relative.clone(),
                hashes.get(&relative).ok_or("missing library hash")?.clone(),
            ))
        })
        .collect::<Result<_>>()?;
    let manifest = hashes
        .iter()
        .map(|(path, hash)| format!("{hash}  {path}\n"))
        .collect::<String>();
    fs::write(output.join("native-libraries.sha256"), manifest)?;
    let receipt = json!({"status":if options.profile==Profile::Runtime{"BUILT"}else{"PASS"},"architecture":if options.profile==Profile::Runtime{Some("aarch64")}else{None},"source":source,"source_revision":REVISION,"source_archive_sha256":ARCHIVE_SHA256,"external_hardening_patch_sha256":PATCH_SHA256,"hardening_patch_sha256":PATCH_SHA256,"source_unchanged":true,"sanitizers":if options.sanitizers{Some("address,undefined")}else{None},"toolchain":options.toolchain,"libraries":library_hashes,"sealed":hashes,"limits":"External provider build only; ABI and real consumer validation are separate."});
    write_json(&output.join("receipt.json"), &receipt)?;
    Ok(receipt)
}
pub fn build(options: &Options, runner: &mut impl Runner) -> Result<Value> {
    disk_space::inspect(&options.output, options.growth)?.require()?;
    fs::create_dir_all(&options.output)?;
    let source = if options.profile == Profile::Xiaoge {
        prepare(options, runner)?
    } else {
        let source = options
            .source
            .as_ref()
            .ok_or("--source required")?
            .canonicalize()?;
        verify_source(&source)?;
        source
    };
    if options.prepare_only {
        check(
            options.profile == Profile::Xiaoge,
            "prepare-only requires xiaoge profile",
        )?;
        return Ok(json!({"status":"PREPARED","source":source,"source_revision":REVISION}));
    }
    verify_source(&source)?;
    let before = source_hashes(&source)?;
    write_json(&options.output.join("source-files.json"), &before)?;
    write_json(
        &options.output.join("receipt.json"),
        &json!({"status":"BUILDING"}),
    )?;
    let (toolchain, environment) = if options.profile == Profile::Runtime {
        let (path, env) = runtime_providers(
            options,
            options
                .providers
                .as_ref()
                .ok_or("runtime target providers missing")?,
            runner,
        )?;
        (Some(path), env)
    } else {
        (options.toolchain.clone(), BTreeMap::new())
    };
    let command = configure(
        options.profile,
        &source,
        &options.output,
        options.sanitizers,
        toolchain.as_deref(),
        std::env::var("CXX").is_ok_and(|value| value.contains("clang")),
    )?;
    let log = String::from_utf8(phase(options, "configure", command, &environment, runner)?)?;
    if options.profile != Profile::Xiaoge {
        let words = log.split_whitespace().collect::<Vec<_>>();
        check(
            words.windows(2).any(|pair| pair == ["FFMPEG:", "YES"])
                && words.windows(2).any(|pair| pair == ["v4l/v4l2:", "YES"]),
            "OpenCV configuration requires FFmpeg and V4L2 capture backends",
        )?;
    }
    if options.profile == Profile::Runtime {
        check(
            log.lines().any(|line| {
                line.split_once("To be built:").is_some_and(|(_, value)| {
                    value.split_whitespace().collect::<Vec<_>>()
                        == ["core", "dnn", "imgcodecs", "imgproc", "videoio"]
                })
            }),
            "combined OpenCV configuration requires precisely five libraries",
        )?;
        let flags = fs::read_to_string(
            options
                .output
                .join("build/modules/core/CMakeFiles/opencv_core.dir/flags.make"),
        )?;
        check(
            flags.contains(&text(&options.output.join("target-include"))?),
            "OpenCV omitted relocated target zlib headers",
        )?;
    }
    phase(
        options,
        "build",
        vec![
            "cmake".into(),
            "--build".into(),
            text(&options.output.join("build"))?,
            "--parallel".into(),
            "2".into(),
        ],
        &environment,
        runner,
    )?;
    phase(
        options,
        "install",
        vec![
            "cmake".into(),
            "--install".into(),
            text(&options.output.join("build"))?,
        ],
        &environment,
        runner,
    )?;
    complete(options, &source, &before)
}

fn absolute(path: PathBuf) -> Result<PathBuf> {
    let path = std::env::current_dir()?.join(path);
    if path.exists() {
        return Ok(path.canonicalize()?);
    }
    let parent = path.parent().ok_or("OpenCV output has no parent")?;
    let name = path.file_name().ok_or("OpenCV output has no filename")?;
    Ok(absolute(parent.to_owned())?.join(name))
}
pub fn cli(profile: Profile) -> Result<()> {
    let mut values = BTreeMap::new();
    let mut args = std::env::args_os().skip(1);
    let mut prepare_only = false;
    while let Some(argument) = args.next() {
        let key = argument.to_str().ok_or("non-UTF8 OpenCV option")?;
        match key {
            "--help" | "-h" => {
                println!("Build pinned native OpenCV: --output PATH [--archive FILE --prepare-only] [--source PATH] [--toolchain FILE --sanitizers address,undefined] [--growth-mib N] [--ffmpeg PREFIX --sdk SDK --cross ROOT --compiler-root ROOT] [--root CHECKOUT]");
                return Ok(());
            }
            "--prepare-only" => {
                check(!prepare_only, "duplicate prepare-only")?;
                prepare_only = true;
            }
            "--root" | "--output" | "--archive" | "--source" | "--toolchain" | "--sanitizers"
            | "--growth-mib" | "--ffmpeg" | "--sdk" | "--cross" | "--compiler-root" => {
                let value = args.next().ok_or("OpenCV option value missing")?;
                check(
                    values.insert(key.to_owned(), value).is_none(),
                    "duplicate OpenCV option",
                )?;
            }
            _ => return Err(format!("unknown OpenCV option: {key}").into()),
        }
    }
    let root = values
        .remove("--root")
        .map_or_else(
            || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.."),
            PathBuf::from,
        )
        .canonicalize()?;
    let output = absolute(values.remove("--output").ok_or("--output required")?.into())?;
    let sanitizers = match values.remove("--sanitizers") {
        None => false,
        Some(value) if value == "address,undefined" => true,
        Some(_) => return Err("only address,undefined sanitizers are supported".into()),
    };
    let growth = match values.remove("--growth-mib") {
        Some(value) => value
            .to_str()
            .ok_or("non-UTF8 growth")?
            .parse::<u64>()?
            .checked_mul(1024 * 1024)
            .ok_or("growth MiB overflow")?,
        None => {
            if profile == Profile::Runtime || sanitizers {
                3 * 1024 * 1024 * 1024
            } else {
                2 * 1024 * 1024 * 1024
            }
        }
    };
    let mut take_path = |key: &str| -> Result<Option<PathBuf>> {
        values
            .remove(key)
            .map(|path| absolute(path.into()))
            .transpose()
    };
    let archive = take_path("--archive")?;
    let source = take_path("--source")?;
    let toolchain = take_path("--toolchain")?;
    let providers = if profile == Profile::Runtime {
        Some(Providers {
            ffmpeg: take_path("--ffmpeg")?
                .ok_or("--ffmpeg required")?
                .canonicalize()?,
            sdk: take_path("--sdk")?
                .ok_or("--sdk required")?
                .canonicalize()?,
            cross: take_path("--cross")?
                .ok_or("--cross required")?
                .canonicalize()?,
            compiler_root: take_path("--compiler-root")?
                .ok_or("--compiler-root required")?
                .canonicalize()?,
        })
    } else {
        None
    };
    check(
        values.is_empty(),
        "provider options require the runtime profile",
    )?;
    let options = Options {
        profile,
        root,
        output,
        source,
        archive,
        prepare_only,
        toolchain,
        sanitizers,
        providers,
        growth,
    };
    let receipt = build(&options, &mut crate::qa_process::NativeRunner)?;
    println!("{}", serde_json::to_string(&receipt)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_profile_places_download_cache_outside_the_read_only_source() {
        let source = Path::new("/owned/source");
        let output = Path::new("/owned/output");
        for profile in [Profile::Xiaoge, Profile::Webcam, Profile::Runtime] {
            let command = configure(profile, source, output, false, None, false).unwrap();
            let caches: Vec<_> = command
                .iter()
                .filter_map(|argument| argument.strip_prefix("-DOPENCV_DOWNLOAD_PATH="))
                .collect();
            assert_eq!(caches, ["/owned/output/downloads"], "{profile:?}");
        }
    }

    #[test]
    fn source_additions_and_edits_still_fail_the_immutable_completion_gate() {
        for addition in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let source = directory.path().join("source");
            fs::create_dir(&source).unwrap();
            fs::write(source.join("owned.cpp"), "original").unwrap();
            let before = source_hashes(&source).unwrap();
            if addition {
                fs::create_dir(source.join(".cache")).unwrap();
                fs::write(source.join(".cache/.gitignore"), "*\n").unwrap();
            } else {
                fs::write(source.join("owned.cpp"), "modified").unwrap();
            }
            let options = Options {
                profile: Profile::Xiaoge,
                root: directory.path().into(),
                output: directory.path().join("output"),
                source: Some(source.clone()),
                archive: None,
                prepare_only: false,
                toolchain: None,
                sanitizers: false,
                providers: None,
                growth: 0,
            };
            assert!(complete(&options, &source, &before)
                .unwrap_err()
                .to_string()
                .contains("external build changed its read-only source tree"));
            assert!(!options.output.join("receipt.json").exists());
            assert_ne!(source_hashes(&source).unwrap(), before);
        }
    }

    #[test]
    fn profiles_preserve_native_python_disabled_and_target_provider_flags() {
        let source = Path::new("/owned/source");
        let output = Path::new("/owned/output");
        for profile in [Profile::Xiaoge, Profile::Webcam, Profile::Runtime] {
            let command = configure(profile, source, output, false, None, false).unwrap();
            for required in [
                "-DBUILD_SHARED_LIBS=ON",
                "-DBUILD_opencv_python3=OFF",
                "-DOPENCV_PYTHON_SKIP_DETECTION=ON",
                "-DBUILD_opencv_python2=OFF",
                "-DBUILD_TESTS=OFF",
                "-DWITH_OPENCL=OFF",
            ] {
                assert!(command.iter().any(|value| value == required));
            }
            assert!(command
                .iter()
                .any(|value| value == &format!("-DBUILD_LIST={}", profile.libraries().join(","))));
            if profile != Profile::Xiaoge {
                for value in [
                    "-DWITH_FFMPEG=ON",
                    "-DWITH_V4L=ON",
                    "-DBUILD_JPEG=ON",
                    "-DWITH_GSTREAMER=OFF",
                ] {
                    assert!(command.iter().any(|item| item == value));
                }
            }
            if profile == Profile::Webcam {
                assert!(command.iter().any(|value| value == "-DWITH_PROTOBUF=OFF"));
            } else {
                assert!(command.iter().any(|value| value == "-DWITH_PROTOBUF=ON"));
            }
        }
        let command = configure(
            Profile::Xiaoge,
            source,
            output,
            true,
            Some(Path::new("/owned/arm.cmake")),
            true,
        )
        .unwrap();
        assert!(command
            .iter()
            .any(|value| value == "-DCMAKE_BUILD_TYPE=RelWithDebInfo"));
        assert!(command
            .iter()
            .any(|value| value.contains("-shared-libasan")));
        assert!(command
            .iter()
            .any(|value| value == "-DCMAKE_TOOLCHAIN_FILE=/owned/arm.cmake"));
    }
    #[test]
    fn wrong_archive_and_unhardened_source_are_rejected() {
        let directory = tempfile::tempdir().unwrap();
        let archive = directory.path().join("bad.tar.gz");
        fs::write(&archive, b"bad archive").unwrap();
        let options = Options {
            profile: Profile::Xiaoge,
            root: directory.path().into(),
            output: directory.path().join("output"),
            source: None,
            archive: Some(archive),
            prepare_only: true,
            toolchain: None,
            sanitizers: false,
            providers: None,
            growth: 0,
        };
        struct Never;
        impl Runner for Never {
            fn run(&mut self, _: &CommandSpec) -> Result<crate::qa_process::Captured> {
                panic!("bad archive must never execute an external tool")
            }
        }
        assert!(prepare(&options, &mut Never)
            .unwrap_err()
            .to_string()
            .contains("SHA256"));
        assert!(!options.output.join(format!("opencv-{REVISION}")).exists());
        assert!(verify_source(directory.path()).is_err());
    }
    #[test]
    fn extraction_rejects_foreign_roots_and_links_without_committing_source() {
        for linked in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let archive = directory.path().join("archive.tar.gz");
            let encoder = flate2::write::GzEncoder::new(
                File::create(&archive).unwrap(),
                flate2::Compression::default(),
            );
            let mut builder = tar::Builder::new(encoder);
            let mut header = tar::Header::new_gnu();
            header.set_mode(0o644);
            header.set_size(0);
            let name = if linked {
                format!("opencv-{REVISION}/link")
            } else {
                "foreign/file".into()
            };
            if linked {
                header.set_entry_type(tar::EntryType::Symlink);
                header.set_link_name("/outside").unwrap();
            }
            header.set_cksum();
            builder
                .append_data(&mut header, name, std::io::empty())
                .unwrap();
            builder.into_inner().unwrap().finish().unwrap();
            assert!(extract(&archive, directory.path()).is_err());
            assert!(!directory.path().join(format!("opencv-{REVISION}")).exists());
            assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
        }
    }
    #[test]
    fn phase_failure_is_recorded_without_a_success_receipt() {
        let directory = tempfile::tempdir().unwrap();
        let options = Options {
            profile: Profile::Webcam,
            root: directory.path().into(),
            output: directory.path().into(),
            source: None,
            archive: None,
            prepare_only: false,
            toolchain: None,
            sanitizers: false,
            providers: None,
            growth: 0,
        };
        struct Fail;
        impl Runner for Fail {
            fn run(&mut self, command: &CommandSpec) -> Result<crate::qa_process::Captured> {
                assert_eq!(command.env["PKG_CONFIG_PATH"], "");
                Ok(crate::qa_process::Captured {
                    code: 7,
                    stdout: b"partial configure".to_vec(),
                    stderr: b"failure".to_vec(),
                })
            }
        }
        let env = BTreeMap::from([("PKG_CONFIG_PATH".into(), "".into())]);
        assert!(phase(&options, "configure", vec!["cmake".into()], &env, &mut Fail).is_err());
        assert!(fs::read_to_string(directory.path().join("configure.log"))
            .unwrap()
            .contains("partial configurefailure"));
        assert_eq!(
            serde_json::from_reader::<_, Value>(
                File::open(directory.path().join("configure.exit.json")).unwrap()
            )
            .unwrap()["code"],
            7
        );
        assert!(!directory.path().join("receipt.json").exists());
    }
}
