use super::*;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::io::{self, Read, Seek};
pub const SDL_WHEEL_SHA256: &str =
    "c47a6938de93fa610accd4969e638c2aebcb29b2fca518a84c3a39d91ab47116";
pub const SDL_TTF_SHA256: &str = "78cdad51f3cc3ada6932b1bb6e914b33798ab970a1e817763f22ddbfd97d0c57";
pub struct SdlOptions {
    pub wheel: PathBuf,
    pub ttf_source: PathBuf,
    pub cc: PathBuf,
    pub sdk: PathBuf,
    pub qemu: PathBuf,
    pub loader: PathBuf,
    pub agnos_libraries: PathBuf,
    pub output: PathBuf,
    pub source_oracle: Option<PathBuf>,
}
fn bytes_hash(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}
fn checked_digest(path: &Path, expected: &str) -> Result<()> {
    require(
        fs::metadata(path)?.len() <= 128 * 1024 * 1024 && digest(path)? == expected,
        format!("source digest mismatch: {}", path.display()),
    )
}
fn ttf_header(path: &Path) -> Result<Vec<u8>> {
    let mut file = File::open(path)?;
    let mut magic = [0; 6];
    file.read_exact(&mut magic)?;
    file.rewind()?;
    let reader: Box<dyn Read> = if magic[..2] == [0x1f, 0x8b] {
        Box::new(flate2::read::MultiGzDecoder::new(file))
    } else if magic == [0xfd, 0x37, 0x7a, 0x58, 0x5a, 0] {
        Box::new(xz2::read::XzDecoder::new_multi_decoder(file))
    } else {
        Box::new(file)
    };
    let mut archive = tar::Archive::new(reader);
    let mut found = None;
    let mut total = 0u64;
    for entry in archive.entries()?.raw(true) {
        let mut entry = entry?;
        total = total
            .checked_add(entry.size())
            .ok_or("SDL source size overflow")?;
        require(
            total <= 256 * 1024 * 1024,
            "SDL source exceeds extraction limit",
        )?;
        if entry.path()?.to_str() == Some("SDL2_ttf-2.20.1/SDL_ttf.h") {
            require(
                found.is_none()
                    && entry.header().entry_type().is_file()
                    && entry.size() > 0
                    && entry.size() <= 1024 * 1024,
                "invalid or duplicate pinned SDL_ttf header",
            )?;
            let mut data = Vec::new();
            entry.read_to_end(&mut data)?;
            found = Some(data);
        } else {
            io::copy(&mut entry, &mut io::sink())?;
        }
    }
    let mut reader = archive.into_inner();
    let mut padding = 0;
    let mut buffer = [0; 10240];
    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        padding += n;
        require(
            padding <= 10240 && buffer[..n].iter().all(|b| *b == 0),
            "unexpected SDL source archive tail",
        )?;
    }
    found.ok_or_else(|| "pinned SDL_ttf header absent".into())
}
fn selected_file(wheel: &mut zip::ZipArchive<File>, name: &str, limit: u64) -> Result<Vec<u8>> {
    let mut entry = wheel.by_name(name)?;
    require(
        !entry.is_dir()
            && entry.size() > 0
            && entry.size() <= limit
            && entry
                .unix_mode()
                .is_none_or(|mode| mode & 0o170000 == 0 || mode & 0o170000 == 0o100000),
        "unsafe SDL wheel member",
    )?;
    let expected = entry.size();
    let mut bytes = Vec::new();
    (&mut entry).take(expected + 1).read_to_end(&mut bytes)?;
    require(
        bytes.len() as u64 == expected,
        "SDL ZIP member size differs",
    )?;
    let mut extra = [0; 1];
    require(entry.read(&mut extra)? == 0, "SDL ZIP exceeds member size")?;
    Ok(bytes)
}
fn stage(options: &SdlOptions) -> Result<(PathBuf, PathBuf, PathBuf, PathBuf)> {
    let header = ttf_header(&options.ttf_source)?;
    let mut wheel = zip::ZipArchive::new(File::open(&options.wheel)?)?;
    require(
        crate::encoder_stage::zip_directory_count(&options.wheel, wheel.central_directory_start())?
            == wheel.len(),
        "duplicate SDL wheel member",
    )?;
    require(wheel.len() <= 100000, "SDL wheel member limit")?;
    let names: BTreeSet<String> = wheel
        .file_names()
        .map(|s| s.map(|v| v.into_owned()))
        .collect::<std::result::Result<_, _>>()?;
    let ttfs: Vec<_> = names
        .iter()
        .filter(|n| n.starts_with("pygame.libs/libSDL2_ttf-"))
        .cloned()
        .collect();
    require(
        ttfs.len() == 1,
        "missing or ambiguous pinned SDL_ttf provider",
    )?;
    let ttf_name = ttfs[0].clone();
    let temp = tempfile::Builder::new()
        .prefix(".sdl-providers-")
        .tempdir_in(&options.output)?;
    let mut pending = vec![ttf_name.clone()];
    let mut extracted = BTreeSet::new();
    let mut total = 0usize;
    while let Some(name) = pending.pop() {
        if !extracted.insert(name.clone()) {
            continue;
        }
        let relative = name
            .strip_prefix("pygame.libs/")
            .ok_or("invalid SDL library prefix")?;
        require(
            !relative.is_empty()
                && !relative.contains(['/', '\\', '\0'])
                && relative != "."
                && relative != "..",
            "unsafe SDL library path",
        )?;
        let data = selected_file(&mut wheel, &name, 256 * 1024 * 1024)?;
        total = total
            .checked_add(data.len())
            .ok_or("SDL payload overflow")?;
        require(
            total <= 256 * 1024 * 1024,
            "SDL native payload exceeds staging limit",
        )?;
        let path = temp.path().join(relative);
        fs::write(&path, data)?;
        for needed in crate::elf_audit::elf(&path)?.needed {
            let name = format!("pygame.libs/{needed}");
            if names.contains(&name) {
                pending.push(name);
            }
        }
    }
    let font_data = selected_file(&mut wheel, "pygame/freesansbold.ttf", 4 * 1024 * 1024)?;
    let sdls: Vec<_> = extracted
        .iter()
        .filter_map(|n| n.strip_prefix("pygame.libs/"))
        .filter(|n| n.starts_with("libSDL2-2-"))
        .map(String::from)
        .collect();
    require(sdls.len() == 1, "missing or ambiguous SDL2 provider")?;
    let library = options.output.join("lib");
    require(
        fs::symlink_metadata(&library).is_err_and(|e| e.kind() == io::ErrorKind::NotFound),
        "SDL library output already exists",
    )?;
    fs::rename(temp.path(), &library)?;
    let headers = options.output.join("include");
    fs::create_dir_all(&headers)?;
    fs::write(headers.join("SDL_ttf.h"), header)?;
    let font = options.output.join("freesansbold.ttf");
    fs::write(&font, font_data)?;
    let ttf = library.join(
        ttf_name
            .strip_prefix("pygame.libs/")
            .ok_or("missing ttf prefix")?,
    );
    Ok((library.join(&sdls[0]), ttf, font, headers))
}
pub fn sdl(options: &SdlOptions, runner: &mut impl Runner) -> Result<Value> {
    sdl_with_pins(options, runner, SDL_WHEEL_SHA256, SDL_TTF_SHA256)
}
pub(super) fn sdl_with_pins(
    options: &SdlOptions,
    runner: &mut impl Runner,
    wheel_sha: &str,
    ttf_sha: &str,
) -> Result<Value> {
    crate::package::disk_guard(&options.output, 1024 * 1024 * 1024)?;
    fs::create_dir_all(&options.output)?;
    checked_digest(&options.wheel, wheel_sha)?;
    checked_digest(&options.ttf_source, ttf_sha)?;
    let (sdl, ttf, font, headers) = stage(options)?;
    let probe = options.output.join("sdl-provider.elf");
    let command = build_probe(
        &options.cc,
        &fixture("runtime_sdl_provider.c"),
        &probe,
        &[
            options.sdk.join("usr/include/SDL2"),
            options.sdk.join("usr/include/aarch64-linux-gnu"),
            headers,
        ],
        runner,
    )?;
    let libraries = vec![
        options.output.join("lib"),
        options.agnos_libraries.clone(),
        options.sdk.join("usr/lib/aarch64-linux-gnu"),
    ];
    let audit = crate::elf_audit::audit(
        &[probe.clone(), ttf.clone(), options.loader.clone()],
        &libraries,
        183,
    )?;
    let mut value = json!({"wheel_sha256":wheel_sha,"ttf_source_sha256":ttf_sha,"build_command":command,"objects":audit["objects"],"roots":audit["roots"],"abi_compatible":audit["compatible"]});
    report(&options.output, &value)?;
    require(
        value["abi_compatible"] == true,
        "SDL target ABI closure failed",
    )?;
    let mut command = provider_prefix(&options.qemu, &options.loader, &libraries)?;
    command.extend([text(&probe)?, text(&sdl)?, text(&ttf)?, text(&font)?]);
    let mut spec = CommandSpec::new(command.clone(), Duration::from_secs(30));
    spec.env.insert("SDL_VIDEODRIVER".into(), "dummy".into());
    let result = runner.run(&spec)?;
    fs::write(options.output.join("pixels.raw"), &result.stdout)?;
    fs::write(options.output.join("probe.stderr"), &result.stderr)?;
    value["run"] = json!({"command":command,"exit_code":result.code,"details":String::from_utf8_lossy(&result.stderr),"pixel_bytes":result.stdout.len(),"pixel_sha256":bytes_hash(&result.stdout)});
    value["passed"] = json!(result.code == 0 && !result.stdout.is_empty());
    value["scope"]="native SDL2/SDL_ttf providers from pinned aarch64 wheel, no Python runtime files; AGNOS loader, QEMU dummy window and text rasterization".into();
    report(&options.output, &value)?;
    checked(&result, "SDL provider")?;
    require(value["passed"] == true, "SDL rendered no pixels")?;
    if let Some(oracle) = &options.source_oracle {
        let before = digest(oracle)?;
        let mut spec = CommandSpec::new(vec![text(oracle)?, text(&font)?], Duration::from_secs(30));
        spec.env
            .insert("PYGAME_HIDE_SUPPORT_PROMPT".into(), "1".into());
        let expected = runner.run(&spec)?;
        checked(&expected, "external source pixel oracle")?;
        require(
            digest(oracle)? == before,
            "external source oracle changed during comparison",
        )?;
        fs::write(options.output.join("source-pixels.raw"), &expected.stdout)?;
        let versions: Value = serde_json::from_slice(&expected.stderr)?;
        let equal = expected.stdout == result.stdout;
        value["source_pixel_comparison"] = json!({"bytes":expected.stdout.len(),"equal":equal,"sha256":bytes_hash(&expected.stdout),"versions":versions,"oracle":oracle,"oracle_sha256":before,"boundary":"explicit independent original-source executable; not bundled or maintained Python"});
        value["passed"] = equal.into();
        report(&options.output, &value)?;
        require(
            equal,
            "AArch64 SDL_ttf pixels differ from independent original oracle",
        )?;
    }
    Ok(
        json!({"objects":value["objects"].as_array().ok_or("missing objects")?.len(),"passed":true,"details":value["run"]["details"]}),
    )
}
