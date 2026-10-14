use super::*;
use serde_json::json;
pub struct FfmpegOptions {
    pub qemu: PathBuf,
    pub loader: PathBuf,
    pub prefix: PathBuf,
    pub libraries: Vec<PathBuf>,
    pub output: PathBuf,
}
fn run(
    options: &FfmpegOptions,
    name: &str,
    program: &str,
    args: Vec<String>,
    value: &mut Value,
    runner: &mut impl Runner,
) -> Result<Vec<u8>> {
    let mut command = provider_prefix(&options.qemu, &options.loader, &options.libraries)?;
    command.push(text(&options.prefix.join("bin").join(program))?);
    command.extend(args);
    let result = runner.run(&CommandSpec::new(command.clone(), Duration::from_secs(120)))?;
    fs::write(
        options.output.join(format!("{name}.stdout")),
        &result.stdout,
    )?;
    fs::write(
        options.output.join(format!("{name}.stderr")),
        &result.stderr,
    )?;
    value["runs"]
        .as_array_mut()
        .ok_or("missing runs")?
        .push(json!({"name":name,"command":command,"exit_code":result.code}));
    report(&options.output, value)?;
    checked(&result, name)?;
    Ok(result.stdout)
}
pub fn ffmpeg(options: &FfmpegOptions, runner: &mut impl Runner) -> Result<Value> {
    crate::package::disk_guard(&options.output, 1024 * 1024 * 1024)?;
    fs::create_dir_all(&options.output)?;
    let roots = vec![
        options.prefix.join("bin/ffmpeg"),
        options.prefix.join("bin/ffprobe"),
        options.loader.clone(),
    ];
    let audit = crate::elf_audit::audit(&roots, &options.libraries, 183)?;
    let mut value = json!({"objects":audit["objects"],"roots":audit["roots"],"abi_compatible":audit["compatible"],"runs":[],"scope":"AArch64 QEMU execution with pinned AGNOS loader and explicit provider paths; no vehicle execution"});
    report(&options.output, &value)?;
    require(
        value["abi_compatible"] == true,
        "target provider ABI closure failed",
    )?;
    run(
        options,
        "version",
        "ffmpeg",
        vec!["-version".into()],
        &mut value,
        runner,
    )?;
    for (codec, extension) in [
        ("libx264", "mp4"),
        ("libvpx", "webm"),
        ("libvpx-vp9", "webm"),
    ] {
        let output = options.output.join(format!("{codec}.{extension}"));
        run(
            options,
            &format!("{codec}-encode"),
            "ffmpeg",
            [
                "-nostdin",
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=96x64:rate=4",
                "-frames:v",
                "4",
                "-threads",
                "1",
                "-c:v",
                codec,
                "-y",
            ]
            .into_iter()
            .map(String::from)
            .chain([text(&output)?])
            .collect(),
            &mut value,
            runner,
        )?;
        let stream = run(
            options,
            &format!("{codec}-probe"),
            "ffprobe",
            ["-v", "error", "-show_streams", "-of", "json"]
                .into_iter()
                .map(String::from)
                .chain([text(&output)?])
                .collect(),
            &mut value,
            runner,
        )?;
        let parsed: Value = serde_json::from_slice(&stream)?;
        let stream = parsed["streams"]
            .as_array()
            .and_then(|s| s.first())
            .ok_or("encoded stream metadata absent")?;
        require(
            stream["width"].as_u64() == Some(96) && stream["height"].as_u64() == Some(64),
            "encoded dimensions differ",
        )?;
        let decoded = run(
            options,
            &format!("{codec}-decode"),
            "ffmpeg",
            ["-nostdin", "-v", "error", "-i"]
                .into_iter()
                .map(String::from)
                .chain([text(&output)?])
                .chain(
                    [
                        "-frames:v",
                        "4",
                        "-threads",
                        "1",
                        "-pix_fmt",
                        "rgba",
                        "-f",
                        "rawvideo",
                        "pipe:1",
                    ]
                    .into_iter()
                    .map(String::from),
                )
                .collect(),
            &mut value,
            runner,
        )?;
        require(
            decoded.len() == 4 * 96 * 64 * 4,
            "decoded frame count differs",
        )?;
    }
    value["passed"] = true.into();
    report(&options.output, &value)?;
    Ok(
        json!({"abi_objects":value["objects"].as_array().ok_or("missing objects")?.len(),"execution_cases":value["runs"].as_array().ok_or("missing runs")?.len(),"passed":true}),
    )
}
