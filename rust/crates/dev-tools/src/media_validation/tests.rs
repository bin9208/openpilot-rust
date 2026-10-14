use super::*;
use serde_json::json;
use std::{io::Write, os::unix::fs::PermissionsExt, process::Command};
struct Fixture {
    temp: tempfile::TempDir,
    elf: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let elf = owned_elf(temp.path(), "owned.so", &[]);
        Self { temp, elf }
    }
    fn path(&self, name: &str) -> PathBuf {
        self.temp.path().join(name)
    }
    fn copy(&self, name: &str) -> PathBuf {
        let path = self.path(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::copy(&self.elf, &path).unwrap();
        path
    }
    fn runner(&self) -> OwnedRunner {
        OwnedRunner {
            elf: self.elf.clone(),
            commands: Vec::new(),
            fail_at: None,
            wrong_dimensions: false,
            short_frames: false,
            oracle_mismatch: false,
            empty_pixels: false,
        }
    }
}
// These are owned parser fixtures: host shared objects with the machine field
// changed to AArch64 solely for static tests. FakeRunner never executes them.
fn owned_elf(directory: &Path, name: &str, links: &[&str]) -> PathBuf {
    fs::create_dir_all(directory).unwrap();
    crate::package::disk_guard(directory, 16 * 1024 * 1024).unwrap();
    let source = directory.join(format!("{name}.c"));
    let code = if links.is_empty() {
        "int owned(void) { return 7; }"
    } else {
        "extern int owned(void); int dependent(void) { return owned(); }"
    };
    fs::write(&source, code).unwrap();
    let output = directory.join(name);
    let mut c = Command::new(std::env::var_os("CC").unwrap_or_else(|| "gcc".into()));
    c.args(["-shared", "-nostdlib", "-fPIC"])
        .arg(&source)
        .arg("-o")
        .arg(&output)
        .arg(format!("-Wl,-soname,{name}"));
    if !links.is_empty() {
        c.arg("-Wl,--no-as-needed")
            .arg(format!("-L{}", directory.display()));
        for link in links {
            c.arg(format!("-l:{link}"));
        }
    }
    let result = c.output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let mut bytes = fs::read(&output).unwrap();
    bytes[18..20].copy_from_slice(&183u16.to_le_bytes());
    fs::write(&output, bytes).unwrap();
    output
}
struct OwnedRunner {
    elf: PathBuf,
    commands: Vec<CommandSpec>,
    fail_at: Option<usize>,
    wrong_dimensions: bool,
    short_frames: bool,
    oracle_mismatch: bool,
    empty_pixels: bool,
}
impl Runner for OwnedRunner {
    fn run(&mut self, c: &CommandSpec) -> Result<Captured> {
        self.commands.push(c.clone());
        if self.fail_at == Some(self.commands.len()) {
            return Ok(Captured {
                code: 7,
                stdout: Vec::new(),
                stderr: b"owned failure".to_vec(),
            });
        }
        let mut result = Captured {
            code: 0,
            stdout: b"{\"resolved\":2}\r\n".to_vec(),
            stderr: Vec::new(),
        };
        if c.argv.iter().any(|a| a == "-std=c11") {
            let index = c.argv.iter().position(|a| a == "-o").unwrap();
            fs::copy(&self.elf, &c.argv[index + 1])?;
        } else if c.argv[0].ends_with("external-oracle") {
            result.stdout = if self.oracle_mismatch {
                b"different pixels".to_vec()
            } else {
                b"owned rgba fixture".to_vec()
            };
            result.stderr = b"{\"source\":\"owned original oracle fixture\"}".to_vec();
        } else if c.argv.iter().any(|a| a.ends_with("/ffprobe")) {
            result.stdout = serde_json::to_vec(
                &json!({"streams":[{"width":if self.wrong_dimensions{95}else{96},"height":64}]}),
            )?;
        } else if c.argv.last().is_some_and(|a| a == "pipe:1") {
            result.stdout = vec![
                7;
                if self.short_frames {
                    1
                } else {
                    4 * 96 * 64 * 4
                }
            ];
        } else if c.env.get("SDL_VIDEODRIVER").is_some_and(|s| s == "dummy") {
            result.stdout = if self.empty_pixels {
                Vec::new()
            } else {
                b"owned rgba fixture".to_vec()
            };
            result.stderr = b"owned provider versions".to_vec();
        }
        Ok(result)
    }
}
fn saved(output: &Path) -> Value {
    serde_json::from_slice(&fs::read(output.join("report.json")).unwrap()).unwrap()
}
fn dynamic_fixture(f: &Fixture) -> DynamicOptions {
    for name in ["libportaudio.so.2", "libusb-1.0.so.0", "librtmp.so.1"] {
        f.copy(&format!("sdk/usr/lib/aarch64-linux-gnu/{name}"));
    }
    for source in [
        "portaudio/src/native.rs",
        "panda-usb/src/api.rs",
        "usbgpu/src/native_usb/api.rs",
        "carrot-server/src/youtube_live/rtmp_api.rs",
    ] {
        let path = f.path(&format!("source/rust/crates/{source}"));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            path,
            "let _ = b\"OwnedSymbol\\0\"; let _ = b\"OtherSymbol\\0\";",
        )
        .unwrap();
    }
    DynamicOptions {
        root: f.path("source"),
        sdk: f.path("sdk"),
        agnos: f.path("agnos"),
        qemu: "owned-qemu".into(),
        probe: "owned-probe".into(),
        output: f.path("dynamic-proof"),
    }
}
#[test]
fn dynamic_preserves_all_current_source_symbols_commands_and_failure_records() {
    let f = Fixture::new();
    let options = dynamic_fixture(&f);
    let mut runner = f.runner();
    assert_eq!(dynamic(&options, &mut runner).unwrap()["passed"], true);
    assert_eq!(runner.commands.len(), 3);
    for c in &runner.commands {
        assert_eq!(c.timeout, Duration::from_secs(30));
        assert_eq!(&c.argv[c.argv.len() - 2..], ["OtherSymbol", "OwnedSymbol"]);
    }
    let report = saved(&options.output);
    assert_eq!(report["runs"][0]["stdout"], "{\"resolved\":2}\n");
    let mut runner = f.runner();
    runner.fail_at = Some(2);
    assert!(dynamic(&options, &mut runner).is_err());
    assert_eq!(runner.commands.len(), 3);
    assert_eq!(saved(&options.output)["passed"], false);
    fs::write(
        options.root.join("rust/crates/portaudio/src/native.rs"),
        "no current symbols",
    )
    .unwrap();
    let mut runner = f.runner();
    assert!(dynamic(&options, &mut runner).is_err());
    assert!(runner.commands.is_empty());
}
fn graphics_fixture(f: &Fixture) -> GraphicsOptions {
    let native = f.path("source/rust/crates/startup-ui/native");
    fs::create_dir_all(&native).unwrap();
    fs::write(native.join("raylib_loader.cc"),"const char *required[] = {\"InitWindow\",\"CloseWindow\"};\nFORWARD(int, BeginDrawing, (void))\n").unwrap();
    fs::write(native.join("gles.cc"), "LOAD(void, \"glClear\")\n").unwrap();
    fs::write(native.join("egl.cc"),"egl.symbol<void*>(\"eglGetProcAddress\")\ngles.symbol<void*>(\"glBindTexture\")\nextension<void*>(\"eglCreateImageKHR\")\n").unwrap();
    GraphicsOptions {
        source_root: f.path("source"),
        cc: "owned-cc".into(),
        qemu: "owned-qemu".into(),
        loader: f.path("agnos/loader"),
        plugin: f.copy("plugin/libraylib.so"),
        sdk: f.path("sdk"),
        agnos_libraries: f.path("agnos"),
        output: f.path("graphics-proof"),
    }
}
#[test]
fn graphics_preserves_build_contract_and_each_lookup_group() {
    let f = Fixture::new();
    let options = graphics_fixture(&f);
    let mut runner = f.runner();
    graphics(&options, &mut runner).unwrap();
    assert_eq!(runner.commands.len(), 5);
    assert!(runner.commands[0]
        .argv
        .iter()
        .any(|a| a.ends_with("runtime_library_probe.c")));
    assert_eq!(runner.commands[1].argv[6], "contract");
    assert!(runner.commands[1]
        .argv
        .iter()
        .any(|a| a == "comma-deps-raylib-6.0.0.1.post103"));
    assert!(runner.commands[1].argv.iter().any(|a| a == "BeginDrawing"));
    assert!(runner.commands[3].argv.iter().any(|a| a == "glBindTexture"));
    assert_eq!(saved(&options.output)["passed"], true);
    let mut runner = f.runner();
    runner.fail_at = Some(3);
    assert!(graphics(&options, &mut runner).is_err());
    assert_eq!(saved(&options.output)["runs"].as_array().unwrap().len(), 2);
    assert_eq!(saved(&options.output)["runs"][1]["exit_code"], 7);
}
fn ffmpeg_fixture(f: &Fixture) -> FfmpegOptions {
    f.copy("ffmpeg/bin/ffmpeg");
    f.copy("ffmpeg/bin/ffprobe");
    FfmpegOptions {
        qemu: "owned-qemu".into(),
        loader: f.copy("agnos/ld-linux-aarch64.so.1"),
        prefix: f.path("ffmpeg"),
        libraries: vec![f.path("agnos")],
        output: f.path("ffmpeg-proof"),
    }
}
#[test]
fn ffmpeg_keeps_all_three_codec_encode_probe_decode_cases_and_exact_frames() {
    let f = Fixture::new();
    let options = ffmpeg_fixture(&f);
    let mut runner = f.runner();
    assert_eq!(
        ffmpeg(&options, &mut runner).unwrap()["execution_cases"],
        10
    );
    assert_eq!(runner.commands.len(), 10);
    assert!(runner
        .commands
        .iter()
        .all(|c| c.timeout == Duration::from_secs(120)));
    for codec in ["libx264", "libvpx", "libvpx-vp9"] {
        assert!(runner
            .commands
            .iter()
            .any(|c| c.argv.iter().any(|a| a == codec)));
    }
    for dimensions in [true, false] {
        let mut runner = f.runner();
        runner.wrong_dimensions = dimensions;
        runner.short_frames = !dimensions;
        assert!(ffmpeg(&options, &mut runner).is_err());
        assert_eq!(
            saved(&options.output)["runs"].as_array().unwrap().len(),
            if dimensions { 3 } else { 4 }
        );
    }
}
fn sdl_fixture(f: &Fixture) -> SdlOptions {
    let libs = f.path("sdl-original");
    let sdl = owned_elf(&libs, "libSDL2-2-owned.so", &[]);
    let mut linkable = fs::read(&sdl).unwrap();
    let host_machine = if cfg!(target_arch = "aarch64") {
        183u16
    } else {
        62u16
    };
    linkable[18..20].copy_from_slice(&host_machine.to_le_bytes());
    fs::write(&sdl, &linkable).unwrap();
    let ttf = owned_elf(&libs, "libSDL2_ttf-owned.so", &["libSDL2-2-owned.so"]);
    linkable[18..20].copy_from_slice(&183u16.to_le_bytes());
    fs::write(&sdl, linkable).unwrap();
    let wheel = f.path("owned.whl");
    let mut zip = zip::ZipWriter::new(File::create(&wheel).unwrap());
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, bytes) in [
        ("pygame.libs/libSDL2-2-owned.so", fs::read(sdl).unwrap()),
        ("pygame.libs/libSDL2_ttf-owned.so", fs::read(ttf).unwrap()),
        ("pygame/freesansbold.ttf", b"owned font fixture".to_vec()),
        (
            "pygame/__init__.py",
            b"must never be extracted or executed".to_vec(),
        ),
    ] {
        zip.start_file(name, options).unwrap();
        zip.write_all(&bytes).unwrap();
    }
    zip.finish().unwrap();
    let source = f.path("ttf-source.tar.gz");
    let compressed = flate2::write::GzEncoder::new(
        File::create(&source).unwrap(),
        flate2::Compression::default(),
    );
    let mut archive = tar::Builder::new(compressed);
    let data = b"owned SDL_ttf header fixture";
    let mut h = tar::Header::new_ustar();
    h.set_path("SDL2_ttf-2.20.1/SDL_ttf.h").unwrap();
    h.set_mode(0o644);
    h.set_size(data.len() as u64);
    h.set_cksum();
    archive.append(&h, data.as_slice()).unwrap();
    archive.into_inner().unwrap().finish().unwrap();
    let oracle = f.path("external-oracle");
    fs::write(
        &oracle,
        b"owned independent original oracle executable fixture",
    )
    .unwrap();
    fs::set_permissions(&oracle, fs::Permissions::from_mode(0o755)).unwrap();
    SdlOptions {
        wheel,
        ttf_source: source,
        cc: "owned-cc".into(),
        sdk: f.path("sdk"),
        qemu: "owned-qemu".into(),
        loader: f.copy("agnos/ld-linux-aarch64.so.1"),
        agnos_libraries: f.path("agnos"),
        output: f.path("sdl-proof"),
        source_oracle: Some(oracle),
    }
}
#[test]
fn sdl_preserves_provider_closure_dummy_pixels_and_independent_oracle_equality() {
    for mismatch in [false, true] {
        let f = Fixture::new();
        let options = sdl_fixture(&f);
        let wheel = digest(&options.wheel).unwrap();
        let source = digest(&options.ttf_source).unwrap();
        let mut runner = f.runner();
        runner.oracle_mismatch = mismatch;
        let result = sdl::sdl_with_pins(&options, &mut runner, &wheel, &source);
        assert_eq!(result.is_ok(), !mismatch);
        assert_eq!(runner.commands.len(), 3);
        assert_eq!(runner.commands[1].env["SDL_VIDEODRIVER"], "dummy");
        let value = saved(&options.output);
        assert_eq!(value["source_pixel_comparison"]["equal"], !mismatch);
        assert_eq!(value["passed"], !mismatch);
        assert_eq!(
            value["source_pixel_comparison"]["oracle_sha256"],
            digest(options.source_oracle.as_ref().unwrap()).unwrap()
        );
        assert!(!options.output.join("pygame").exists());
        assert_eq!(
            fs::read(options.output.join("include/SDL_ttf.h")).unwrap(),
            b"owned SDL_ttf header fixture"
        );
    }
}
#[test]
fn sdl_public_pins_and_empty_pixels_fail_closed() {
    let f = Fixture::new();
    let options = sdl_fixture(&f);
    let mut runner = f.runner();
    assert!(sdl(&options, &mut runner).is_err());
    assert!(runner.commands.is_empty());
    assert!(!options.output.join("lib").exists());
    let mut runner = f.runner();
    runner.empty_pixels = true;
    let wheel = digest(&options.wheel).unwrap();
    let source = digest(&options.ttf_source).unwrap();
    assert!(sdl::sdl_with_pins(&options, &mut runner, &wheel, &source).is_err());
    assert_eq!(runner.commands.len(), 2);
    assert_eq!(saved(&options.output)["passed"], false);
}

#[test]
fn sdl_rejects_duplicate_linked_oversized_and_corrupt_wheel_records_before_compile() {
    use std::io::Read;
    for fault in ["duplicate", "symlink", "oversized", "crc", "traversal"] {
        let f = Fixture::new();
        let options = sdl_fixture(&f);
        let mut original = zip::ZipArchive::new(File::open(&options.wheel).unwrap()).unwrap();
        let mut records = Vec::new();
        for index in 0..original.len() {
            let mut entry = original.by_index(index).unwrap();
            let name = entry.name().unwrap().into_owned();
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).unwrap();
            records.push((name, bytes));
        }
        drop(original);
        let target = "pygame.libs/libSDL2_ttf-owned.so";
        if fault == "duplicate" {
            let bytes = records
                .iter()
                .find(|(name, _)| name == target)
                .unwrap()
                .1
                .clone();
            records.push(("pygame.libs/libSDL2_ttf-other.so".into(), bytes));
        }
        if fault == "traversal" {
            records
                .iter_mut()
                .find(|(name, _)| name == target)
                .unwrap()
                .0 = "pygame.libs/libSDL2_ttf-/../../outside".into();
        }
        let mut writer = zip::ZipWriter::new(File::create(&options.wheel).unwrap());
        let flags = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for (name, bytes) in records {
            writer.start_file(name, flags).unwrap();
            writer.write_all(&bytes).unwrap();
        }
        writer.finish().unwrap();
        let mut bytes = fs::read(&options.wheel).unwrap();
        if fault == "duplicate" {
            let old = b"pygame.libs/libSDL2_ttf-other.so";
            let mut cursor = 0;
            while let Some(index) = bytes[cursor..].windows(old.len()).position(|b| b == old) {
                let index = cursor + index;
                bytes[index..index + old.len()].copy_from_slice(target.as_bytes());
                cursor = index + old.len();
            }
        } else if fault == "crc" {
            let index = bytes.windows(4).position(|b| b == b"\x7fELF").unwrap();
            bytes[index + 1] = b'X';
        } else if fault == "symlink" || fault == "oversized" {
            let mut index = bytes.windows(4).position(|b| b == b"PK\x01\x02").unwrap();
            loop {
                let length = u16::from_le_bytes([bytes[index + 28], bytes[index + 29]]) as usize;
                if &bytes[index + 46..index + 46 + length] == target.as_bytes() {
                    if fault == "symlink" {
                        bytes[index + 38..index + 42]
                            .copy_from_slice(&(0o120777u32 << 16).to_le_bytes());
                    } else {
                        bytes[index + 24..index + 28]
                            .copy_from_slice(&(256 * 1024 * 1024 + 1u32).to_le_bytes());
                    }
                    break;
                }
                let extra = u16::from_le_bytes([bytes[index + 30], bytes[index + 31]]) as usize;
                let comment = u16::from_le_bytes([bytes[index + 32], bytes[index + 33]]) as usize;
                index += 46 + length + extra + comment;
            }
        }
        fs::write(&options.wheel, bytes).unwrap();
        let mut runner = f.runner();
        let result = sdl::sdl_with_pins(
            &options,
            &mut runner,
            &digest(&options.wheel).unwrap(),
            &digest(&options.ttf_source).unwrap(),
        );
        assert!(result.is_err(), "accepted {fault}");
        assert!(runner.commands.is_empty(), "compiled after {fault}");
        assert!(
            !options.output.join("lib").exists(),
            "published native payload after {fault}"
        );
        assert!(!f.path("outside").exists());
    }
}

#[test]
fn sdl_ttf_header_must_be_unique_regular_and_bounded() {
    for fault in ["duplicate", "symlink", "oversized"] {
        let f = Fixture::new();
        let options = sdl_fixture(&f);
        let compressed = flate2::write::GzEncoder::new(
            File::create(&options.ttf_source).unwrap(),
            flate2::Compression::default(),
        );
        let mut archive = tar::Builder::new(compressed);
        for _ in 0..if fault == "duplicate" { 2 } else { 1 } {
            let mut h = tar::Header::new_ustar();
            h.set_path("SDL2_ttf-2.20.1/SDL_ttf.h").unwrap();
            h.set_mode(0o644);
            if fault == "symlink" {
                h.set_entry_type(tar::EntryType::Symlink);
                h.set_link_name("../outside").unwrap();
                h.set_size(0);
                h.set_cksum();
                archive.append(&h, std::io::empty()).unwrap();
            } else {
                let size = if fault == "oversized" {
                    1024 * 1024 + 1
                } else {
                    4
                };
                h.set_size(size);
                h.set_cksum();
                archive
                    .append(&h, std::io::Read::take(std::io::repeat(b'x'), size))
                    .unwrap();
            }
        }
        archive.into_inner().unwrap().finish().unwrap();
        let mut runner = f.runner();
        assert!(
            sdl::sdl_with_pins(
                &options,
                &mut runner,
                &digest(&options.wheel).unwrap(),
                &digest(&options.ttf_source).unwrap()
            )
            .is_err(),
            "accepted {fault}"
        );
        assert!(runner.commands.is_empty());
        assert!(!options.output.join("lib").exists());
    }
}
