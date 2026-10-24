//! Original camera packing, allocation, histogram and timing fixture/comparator.
use crate::camera_exposure_check::shared;
use crate::{
    camera_sensor_check::{command, lexical, Random},
    path_resolve::resolve_non_strict,
    python_fs,
    qa_process::{NativeRunner, Runner},
    seal::Result,
};
use openpilot_log_format::JsonValue;
use std::{ffi::OsStr, fs, io::Write, os::unix::ffi::OsStrExt, path::Path};
pub struct Inputs {
    pub source: Vec<u8>,
    pub native: String,
    pairs: Vec<(Vec<u8>, String)>,
    raw_text: bool,
}
fn add(pairs: &mut Vec<(Vec<u8>, String)>, source: String, native: String) {
    pairs.push((source.into_bytes(), native));
}
fn numbers(values: &[u64]) -> String {
    values
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}
fn randbytes(rng: &mut Random, count: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(count);
    for _ in 0..count / 4 {
        out.extend_from_slice(&(rng.getrandbits(32) as u32).to_le_bytes());
    }
    let remaining = count % 4;
    if remaining != 0 {
        out.extend_from_slice(&rng.getrandbits((remaining * 8) as u32).to_le_bytes()[..remaining]);
    }
    out
}
pub fn generate_inputs(output: &Path) -> Result<Inputs> {
    let output = lexical(output);
    fs::create_dir_all(&output)?;
    let fixtures = output.join("fixtures");
    fs::create_dir_all(&fixtures)?;
    let mut rng = Random::new(1872);
    let mut pairs = vec![];
    for index in 0..5000 {
        let mut sof = 1000000000u64 + index * 50000000;
        if index % 29 == 0 {
            sof -= 200000000;
        }
        let received = sof + [0, 1, 30000000, 75000000, 75000001, 200000000][rng.below(6) as usize];
        add(
            &mut pairs,
            format!("timing {sof} {received}"),
            format!("{{\"op\": \"timing\", \"sof\": {sof}, \"received\": {received}}}"),
        );
    }
    for (sof, received) in [
        (0, 0),
        (0, 200000000),
        (u64::MAX, 0),
        (0, u64::MAX),
        (1, 2),
        (1000000000000, 1000000000000),
        (1000000000000, 1000100000000),
    ] {
        add(
            &mut pairs,
            format!("timing {sof} {received}"),
            format!("{{\"op\": \"timing\", \"sof\": {sof}, \"received\": {received}}}"),
        );
    }
    for width in [0, 1, 127, 128, 129, 511, 512, 513, 1344, 1928, 2688, 4096] {
        for height in [0, 1, 31, 32, 33, 63, 64, 65, 760, 1208, 1520, 3072] {
            add(
                &mut pairs,
                format!("nv12 {width} {height}"),
                format!("{{\"op\": \"nv12\", \"width\": {width}, \"height\": {height}}}"),
            );
        }
    }
    for _ in 0..1500 {
        let width = 1 + rng.below(4096);
        let height = 1 + rng.below(3072);
        add(
            &mut pairs,
            format!("nv12 {width} {height}"),
            format!("{{\"op\": \"nv12\", \"width\": {width}, \"height\": {height}}}"),
        );
    }
    for length in [0, 1, 2, 255, 256, 65535, 65536, 65537, u32::MAX as u64] {
        for address in [0, 1, 0xffffff, 0x1000000, u32::MAX as u64] {
            for (selector, opcode) in [(0, 10), (1, 11), (255, 255), (12, 1)] {
                add(&mut pairs,format!("dmi {length} {address} {selector} {opcode}"),format!("{{\"op\": \"dmi\", \"length\": {length}, \"address\": {address}, \"selector\": {selector}, \"opcode\": {opcode}}}"));
            }
        }
    }
    for count in (0..130).chain([255, 256, 257, 1023, 1024, 1025, 65535, 65536, 65537]) {
        let values = (0..count)
            .map(|_| rng.below(1u64 << 32))
            .collect::<Vec<_>>();
        let address = rng.below(1u64 << 32);
        let source = values
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(" ");
        let values = numbers(&values);
        for op in ["cont", "random"] {
            let field = if op == "cont" {
                format!(", \"address\": {address}")
            } else {
                String::new()
            };
            add(
                &mut pairs,
                format!("{op} {address} {count} {source}"),
                format!("{{\"op\": \"{op}\", \"values\": [{values}]{field}}}"),
            );
        }
    }

    let mut images = vec![];
    for a in 0..3 {
        for b in 0..3 {
            for c in 0..3 {
                for d in 0..3 {
                    let mut remaining = 160;
                    let mut rows = vec![];
                    for split in [a, b, c, d] {
                        let count = split * remaining / 3;
                        rows.push(count);
                        remaining -= count;
                    }
                    rows.push(remaining);
                    let pixels = [0, 24, 48, 96, 235]
                        .into_iter()
                        .zip(rows)
                        .flat_map(|(value, row)| std::iter::repeat_n(value, row * 240))
                        .collect::<Vec<u8>>();
                    images.push((240, pixels, [0, 0, 239, 159], 1, 1));
                }
            }
        }
    }
    for _ in 0..100 {
        let width = 1 + rng.below(127);
        let height = 1 + rng.below(95);
        let pixels = randbytes(&mut rng, (width * height) as usize);
        let x = rng.below(width);
        let y = rng.below(height);
        let region = [x, y, rng.below(width - x + 1), rng.below(height - y + 1)];
        let xskip = 1 + rng.below(4);
        let yskip = 1 + rng.below(4);
        images.push((width, pixels, region, xskip, yskip));
    }
    images.push((1, vec![0x11], [0, 0, 1, 1], 1, 1));
    images.push((4, vec![0, 1, 200, 255], [0, 0, 4, 1], 1, 1));
    let mut raw_text = false;
    for (index, (width, pixels, region, xskip, yskip)) in images.into_iter().enumerate() {
        let path = fixtures.join(format!("image-{index}.bin"));
        fs::write(&path, pixels)?;
        let path = resolve_non_strict(&path)?;
        raw_text |= path.to_str().is_none();
        let mut source = format!(
            "luminance {width} {} {xskip} {yskip} \"",
            region
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(" ")
        )
        .into_bytes();
        source.extend_from_slice(path.as_os_str().as_bytes());
        source.push(b'"');
        let native=format!("{{\"op\": \"luminance\", \"width\": {width}, \"region\": [{}], \"x_skip\": {xskip}, \"y_skip\": {yskip}, \"file\": {}}}",numbers(&region),python_fs::json_string(path.as_os_str())?);
        pairs.push((source, native));
    }
    let mut source = vec![];
    let mut native = String::new();
    for (a, b) in &pairs {
        source.extend_from_slice(a);
        source.push(b'\n');
        native.push_str(b);
        native.push('\n');
    }
    Ok(Inputs {
        source,
        native,
        pairs,
        raw_text,
    })
}
fn source_json(source: &[u8]) -> Result<String> {
    python_fs::json_string(OsStr::from_bytes(source))
}
fn source_encoding_probe(runner: &mut dyn Runner, argv: &[std::ffi::OsString]) -> Result<()> {
    // subprocess.Popen starts before text-mode stdin encoding. Preserve the
    // original child's observable start, sending no bytes when encoding fails.
    let mut spec = crate::qa_process::CommandSpec::new_os(argv.to_vec(), std::time::Duration::MAX);
    spec.input = Some(vec![]);
    spec.stdout_limit = usize::MAX;
    spec.stderr_limit = usize::MAX;
    let _ = runner.run(&spec)?;
    Err("camera source input contains an unencodable surrogate".into())
}
pub fn check(
    reference: &Path,
    binary: &Path,
    output: &Path,
    source_runner: &str,
    native_runner: &str,
    runner: &mut dyn Runner,
    printed: &mut dyn Write,
) -> Result<()> {
    check_os(
        reference,
        binary,
        output,
        OsStr::new(source_runner),
        OsStr::new(native_runner),
        runner,
        printed,
    )
}
fn check_os(
    reference: &Path,
    binary: &Path,
    output: &Path,
    source_runner: &OsStr,
    native_runner: &OsStr,
    runner: &mut dyn Runner,
    printed: &mut dyn Write,
) -> Result<()> {
    let reference = lexical(reference);
    let binary = lexical(binary);
    let output = lexical(output);
    let input = generate_inputs(&output)?;
    let source_command = command(source_runner, &reference)?;
    if input.raw_text {
        return source_encoding_probe(runner, &source_command);
    }
    let source = shared::execute(runner, &source_command, &input.source)?;
    let native_command = command(native_runner, &binary)?;
    let native = shared::execute(runner, &native_command, input.native.as_bytes())?;
    let source_lines = shared::splitlines(&source);
    let native_lines = shared::splitlines(&native);
    if source_lines.len() != native_lines.len() || source_lines.len() != input.pairs.len() {
        return Err("camera misc output cardinality differs".into());
    }
    for (index, (left, right)) in source_lines.into_iter().zip(native_lines).enumerate() {
        shared::compare_row(
            left,
            right,
            &format!("case[{index}]"),
            |a, b, error| {
                Ok(format!(
                    "{{\"input\": [{}, {}], \"source\": {}, \"native\": {}, \"error\": {error}}}",
                    source_json(&input.pairs[index].0)?,
                    input.pairs[index].1,
                    a.to_json()?,
                    b.to_json()?
                ))
            },
            &output,
        )?;
    }
    let mut counts: Vec<(&str, usize)> = vec![];
    for (_, native) in &input.pairs {
        let name = match native.split('"').nth(3).ok_or("misc op missing")? {
            "timing" => "timing",
            "nv12" => "nv12",
            "dmi" => "dmi",
            "cont" => "cont",
            "random" => "random",
            "luminance" => "luminance",
            _ => return Err("unexpected misc op".into()),
        };
        if let Some((_, count)) = counts.iter_mut().find(|(key, _)| *key == name) {
            *count += 1;
        } else {
            counts.push((name, 1));
        }
    }
    let counts = counts
        .into_iter()
        .map(|(name, count)| format!("\"{name}\": {count}"))
        .collect::<Vec<_>>()
        .join(", ");
    let source_hash = shared::sha(&fs::read(reference)?);
    let native_hash = shared::sha(&fs::read(binary)?);
    let report=JsonValue::parse(&format!("{{\"status\": \"PASS\", \"counts\": {{{counts}}}, \"source_command\": {}, \"native_command\": {}, \"source_sha256\": \"{source_hash}\", \"native_sha256\": \"{native_hash}\", \"source_input_sha256\": \"{}\", \"native_input_sha256\": \"{}\", \"limits\": \"synthetic host/explicit emulator only; no hardware, source C++ histogram and packing unmodified\"}}",shared::argv_json(&source_command)?,shared::argv_json(&native_command)?,shared::sha(&input.source),shared::sha(input.native.as_bytes())))?;
    shared::write_report(&output, &report, printed)
}
pub fn cli() -> Result<()> {
    let options = shared::options(false)?;
    if options.help {
        println!("usage: check-camerad-misc --reference PATH --binary PATH --output PATH");
        return Ok(());
    }
    check_os(
        Path::new(&options.values["--reference"]),
        Path::new(&options.values["--binary"]),
        Path::new(&options.values["--output"]),
        shared::prefix(&options, "--reference-runner"),
        shared::prefix(&options, "--runner"),
        &mut NativeRunner,
        &mut std::io::stdout(),
    )
}
