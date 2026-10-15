//! Build unchanged Panda C++ oracles with native command ownership and evidence.
use crate::{
    disk_space,
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

struct Options {
    output: PathBuf,
    json11: PathBuf,
    capnp: Option<PathBuf>,
    usb: Option<PathBuf>,
    compiler: String,
    sanitize: bool,
    mode: String,
}
impl Options {
    fn parse(family: &str, args: &[String]) -> Result<Self> {
        if !["protocol", "usb", "spi", "peripheral", "runtime-fixture"].contains(&family) {
            return Err("unknown Panda build family".into());
        }
        let mut values = BTreeMap::new();
        let mut mode = None;
        let mut sanitize = false;
        let mut i = 0;
        while i < args.len() {
            let key = args[i].as_str();
            i += 1;
            if key == "--sanitize" && family != "runtime-fixture" {
                sanitize = true;
                continue;
            }
            if ["--safety", "--device", "--state", "--can-io"].contains(&key)
                && family == "protocol"
            {
                if mode.replace(key[2..].to_owned()).is_some() {
                    return Err("Panda protocol modes are mutually exclusive".into());
                }
                continue;
            }
            let allowed = matches!(key, "--output" | "--json11-prefix")
                || key == "--capnp-prefix" && family == "protocol"
                || key == "--libusb-include" && matches!(family, "usb" | "runtime-fixture")
                || key == "--compiler" && matches!(family, "usb" | "spi" | "runtime-fixture");
            if !allowed {
                return Err(format!("unknown argument: {key}").into());
            }
            let value = args
                .get(i)
                .ok_or_else(|| format!("{key} requires a value"))?
                .clone();
            i += 1;
            values.insert(key.to_owned(), value);
        }
        let mut take = |key: &str| -> Result<String> {
            values
                .remove(key)
                .ok_or_else(|| format!("{key} required").into())
        };
        let output = absolute(&PathBuf::from(take("--output")?))?;
        let json11 = PathBuf::from(take("--json11-prefix")?);
        let capnp = if family == "protocol" {
            Some(PathBuf::from(take("--capnp-prefix")?))
        } else {
            None
        };
        let usb = if matches!(family, "usb" | "runtime-fixture") {
            Some(PathBuf::from(take("--libusb-include")?))
        } else {
            None
        };
        Ok(Self {
            output,
            json11,
            capnp,
            usb,
            compiler: values
                .remove("--compiler")
                .unwrap_or_else(|| "clang++".into()),
            sanitize,
            mode: mode.unwrap_or_else(|| "protocol".into()),
        })
    }
}
fn absolute(path: &Path) -> Result<PathBuf> {
    crate::path_resolve::resolve_non_strict(path)
}
fn s(path: impl AsRef<Path>) -> String {
    path.as_ref().to_string_lossy().into_owned()
}
fn hash(path: &Path) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(fs::read(path)?)))
}
fn write_json(path: &Path, value: &Value) -> Result<()> {
    fs::write(path, format!("{}\n", serde_json::to_string_pretty(value)?))?;
    Ok(())
}
fn hashes(root: &Path, files: &[PathBuf], relative: bool) -> Result<Value> {
    let mut result = serde_json::Map::new();
    for file in files {
        result.insert(
            s(if relative {
                file.strip_prefix(root)?
            } else {
                file
            }),
            json!(hash(file)?),
        );
    }
    Ok(Value::Object(result))
}
fn paths(root: &Path, names: &[&str]) -> Vec<PathBuf> {
    names.iter().map(|name| root.join(name)).collect()
}
fn strings(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|p| (*p).into()).collect()
}
fn includes(command: &mut Vec<String>, items: &[PathBuf]) {
    command.extend(items.iter().map(|p| format!("-I{}", p.display())));
}
fn sanitize(command: &mut Vec<String>, enabled: bool) {
    if enabled {
        command.splice(
            1..1,
            strings(&["-fsanitize=address,undefined", "-fno-omit-frame-pointer"]),
        );
    }
}
fn sorted_extension(path: &Path, extension: &str) -> Result<Vec<PathBuf>> {
    let mut result = fs::read_dir(path)?
        .map(|e| e.map(|e| e.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    result.retain(|p| p.extension().is_some_and(|e| e == extension));
    result.sort();
    Ok(result)
}
fn copy_tree(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let target = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}
fn extract(text: &str, start: &str, end: &str) -> Result<String> {
    let tail = text
        .split_once(start)
        .ok_or_else(|| format!("missing source anchor: {start}"))?
        .1;
    let body = tail
        .split_once(end)
        .ok_or_else(|| format!("missing source anchor: {end}"))?
        .0;
    Ok(format!("{start}{body}"))
}
fn captured(
    runner: &mut dyn Runner,
    command: Vec<String>,
    file: &Path,
    records: &mut Vec<Value>,
    shape: &str,
    list: bool,
) -> Result<()> {
    let result = runner.run(&CommandSpec::new(command.clone(), Duration::from_secs(300)))?;
    let record = if shape == "build" {
        json!({"command":command,"exit":result.code,"stdout":String::from_utf8_lossy(&result.stdout),"stderr":String::from_utf8_lossy(&result.stderr)})
    } else {
        json!({"argv":command,"exit_code":result.code,"stdout":String::from_utf8_lossy(&result.stdout),"stderr":String::from_utf8_lossy(&result.stderr)})
    };
    records.push(record.clone());
    write_json(file, &if list { json!(records) } else { record })?;
    if result.code != 0 {
        return Err(format!("Panda source compiler exited {}", result.code).into());
    }
    Ok(())
}
/// `free_bytes` is supplied by the caller's disk inspection, keeping admission testable.
/// Owned partial directories and failure logs remain available, matching the old tools.
pub fn run(
    root: &Path,
    family: &str,
    args: &[String],
    runner: &mut dyn Runner,
    free_bytes: u64,
) -> Result<()> {
    let options = Options::parse(family, args)?;
    let o = &options;
    fs::create_dir_all(o.output.parent().ok_or("output parent missing")?)?;
    fs::create_dir(&o.output)?;
    disk_space::Space::new(
        free_bytes,
        if family == "peripheral" {
            100 * 1024 * 1024
        } else {
            256 * 1024 * 1024
        },
    )?
    .require()?;
    match family {
        "protocol" => protocol(root, o, runner),
        "peripheral" => peripheral(root, o, runner),
        _ => simple(root, family, o, runner),
    }
}
fn simple(root: &Path, family: &str, o: &Options, runner: &mut dyn Runner) -> Result<()> {
    let usb = family == "usb";
    let spi = family == "spi";
    let name = if usb {
        "pandad-usb-source"
    } else if spi {
        "pandad-spi-source"
    } else {
        "libusb-1.0.so.0"
    };
    let binary = o.output.join(name);
    let mut sources = if usb {
        paths(
            root,
            &[
                "rust/tools/pandad_usb_source.cc",
                "openpilot/selfdrive/pandad/panda_comms.cc",
            ],
        )
    } else if spi {
        paths(
            root,
            &[
                "rust/tools/pandad_spi_source.cc",
                "openpilot/selfdrive/pandad/spi.cc",
                "openpilot/common/util.cc",
            ],
        )
    } else {
        paths(root, &["rust/tools/pandad_runtime_usb.cc"])
    };
    let mut command = vec![o.compiler.clone()];
    command.extend(strings(&["-std=c++17", "-O1", "-g"]));
    if usb || spi {
        command.extend(strings(&[
            "-ffunction-sections",
            "-fdata-sections",
            "-Wl,--gc-sections",
        ]));
    } else {
        command.extend(strings(&[
            "-shared",
            "-fPIC",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-Wl,-soname,libusb-1.0.so.0",
        ]));
    }
    if usb {
        command.push("-Wl,--wrap=clock_gettime".into());
    }
    if !usb {
        includes(&mut command, &[root.to_path_buf()]);
    }
    if usb || spi {
        includes(&mut command, &[root.join("openpilot")]);
    }
    includes(&mut command, &[o.json11.join("include")]);
    if let Some(include) = &o.usb {
        includes(&mut command, std::slice::from_ref(include));
    }
    command.extend(sources.iter().map(s));
    command.push(s(o.json11.join("lib/libjson11.a")));
    command.extend(strings(&["-pthread", "-o"]));
    command.push(s(&binary));
    if spi {
        command.extend(
            [
                "clock_gettime",
                "open",
                "stat",
                "close",
                "flock",
                "ioctl",
                "sched_yield",
                "usleep",
            ]
            .map(|name| format!("-Wl,--wrap={name}")),
        );
    }
    sanitize(&mut command, o.sanitize);
    let mut records = vec![];
    captured(
        runner,
        command,
        &o.output.join("command.json"),
        &mut records,
        "command",
        usb,
    )?;
    let fixture = o.output.join("libpanda_usb_fixture.so");
    if usb {
        let mut shared = vec![o.compiler.clone()];
        shared.extend(strings(&[
            "-std=c++17",
            "-O1",
            "-g",
            "-shared",
            "-fPIC",
            "-DPANDA_USB_ABI_ONLY",
        ]));
        includes(
            &mut shared,
            &[
                root.join("openpilot"),
                o.json11.join("include"),
                o.usb.clone().ok_or("USB include required")?,
            ],
        );
        shared.extend([s(&sources[0]), s(o.json11.join("lib/libjson11.a"))]);
        shared.extend(strings(&["-pthread", "-o"]));
        shared.push(s(&fixture));
        sanitize(&mut shared, o.sanitize);
        captured(
            runner,
            shared,
            &o.output.join("command.json"),
            &mut records,
            "command",
            true,
        )?;
        sources.extend(paths(
            root,
            &[
                "openpilot/selfdrive/pandad/panda_comms.h",
                "openpilot/common/swaglog.h",
                "openpilot/common/timing.h",
            ],
        ));
        sources.push(
            o.usb
                .as_ref()
                .ok_or("USB include required")?
                .join("libusb-1.0/libusb.h"),
        );
    } else if spi {
        sources.extend(paths(
            root,
            &[
                "openpilot/selfdrive/pandad/panda_comms.h",
                "openpilot/common/util.h",
                "openpilot/common/timing.h",
                "openpilot/common/swaglog.h",
                "panda/board/comms_definitions.h",
            ],
        ));
    } else {
        sources.push(root.join("panda/board/health.h"));
        sources.push(
            o.usb
                .as_ref()
                .ok_or("USB include required")?
                .join("libusb-1.0/libusb.h"),
        );
    }
    let report = if usb {
        json!({"binary":s(&binary),"sha256":hash(&binary)?,"fixture":s(&fixture),"fixture_sha256":hash(&fixture)?,"sources":hashes(root,&sources,false)?,"sanitize":o.sanitize,"scope":"unchanged C++ USB lifecycle and retry policies with scripted libusb ABI and clock; no physical USB"})
    } else if spi {
        json!({"binary":s(&binary),"sha256":hash(&binary)?,"sources":hashes(root,&sources,false)?,"sanitize":o.sanitize,"scope":"unchanged SPI and utility implementation; scripted Linux syscalls only; no hardware access"})
    } else {
        json!({"result":"PASS","binary":s(&binary),"sha256":hash(&binary)?,"source_sha256":hashes(root,&sources,false)?,"scope":"owned virtual libusb device, captured control/bulk calls, no physical USB delegation"})
    };
    write_json(&o.output.join("manifest.json"), &report)?;
    println!("{}", serde_json::to_string(&report)?);
    Ok(())
}
fn peripheral(root: &Path, o: &Options, runner: &mut dyn Runner) -> Result<()> {
    let source = root.join("openpilot/selfdrive/pandad/pandad.cc");
    let fixture = root.join("rust/tools/pandad_peripheral_source.cc");
    let text = fs::read_to_string(&source)?;
    let body = extract(
        &text,
        "void process_peripheral_state(",
        "\nvoid log_panda_serial(",
    )?;
    let constants = text
        .lines()
        .filter(|line| {
            [
                "#define MAX_IR_PANDA_VAL ",
                "#define CUTOFF_IL ",
                "#define SATURATE_IL ",
            ]
            .iter()
            .any(|prefix| line.starts_with(prefix))
        })
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(
        o.output.join("pandad_peripheral_body.inc"),
        format!("{constants}\n\n{body}"),
    )?;
    let prefix = absolute(&o.json11)?;
    let binary = o.output.join("pandad-peripheral-source");
    let mut command = strings(&["clang++", "-std=c++17", "-O1", "-g"]);
    includes(
        &mut command,
        &[
            root.join("openpilot"),
            o.output.clone(),
            prefix.join("include"),
        ],
    );
    command.extend([
        s(&fixture),
        s(prefix.join("lib/libjson11.a")),
        "-o".into(),
        s(&binary),
    ]);
    sanitize(&mut command, o.sanitize);
    captured(
        runner,
        command,
        &o.output.join("build.json"),
        &mut vec![],
        "build",
        false,
    )?;
    let report = json!({"scope":"unchanged process_peripheral_state body and original C++ FirstOrderFilter/map_val; recorded message/Params/hardware boundaries","binary":s(&binary),"binary_sha256":hash(&binary)?,"body_sha256":format!("{:x}",Sha256::digest(body.as_bytes())),"sanitize":o.sanitize,"sources":hashes(root,&[source,fixture,root.join("openpilot/common/util.h")],true)?});
    write_json(&o.output.join("manifest.json"), &report)?;
    println!("{}", serde_json::to_string(&report)?);
    Ok(())
}
fn protocol(root: &Path, o: &Options, runner: &mut dyn Runner) -> Result<()> {
    let schema = o.output.join("schema");
    let generated = o.output.join("cereal/gen/cpp");
    fs::create_dir(&schema)?;
    fs::create_dir_all(&generated)?;
    let mut sources = paths(
        root,
        &[
            "rust/tools/pandad_protocol_source.cc",
            "openpilot/selfdrive/pandad/panda.cc",
            "openpilot/selfdrive/pandad/panda.h",
            "openpilot/selfdrive/pandad/spi_alert.h",
        ],
    );
    let mut extra = vec![];
    if o.mode == "safety" {
        sources[0] = root.join("rust/tools/pandad_safety_source.cc");
        extra = paths(
            root,
            &[
                "openpilot/selfdrive/pandad/panda_safety.cc",
                "openpilot/common/params.cc",
                "openpilot/common/util.cc",
            ],
        );
        sources.extend(extra.clone());
        sources.extend(paths(
            root,
            &[
                "openpilot/selfdrive/pandad/pandad.h",
                "openpilot/common/params.h",
                "openpilot/common/params_keys.h",
                "openpilot/cereal/messaging/messaging.h",
            ],
        ));
    }
    sources.extend(paths(
        root,
        &[
            "openpilot/selfdrive/pandad/panda_comms.h",
            "panda/board/health.h",
            "panda/board/can.h",
            "openpilot/common/swaglog.h",
            "openpilot/common/timing.h",
            "openpilot/common/util.h",
        ],
    ));
    if matches!(o.mode.as_str(), "device" | "state" | "can-io") {
        sources[0] = root.join(format!(
            "rust/tools/pandad_{}_source.cc",
            o.mode.replace('-', "_")
        ));
    }
    if o.mode == "state" {
        let original = root.join("openpilot/selfdrive/pandad/pandad.cc");
        let text = fs::read_to_string(&original)?;
        let fill = extract(
            &text,
            "void fill_panda_state(",
            "\nvoid send_peripheral_state(",
        )?;
        let process = extract(
            &text,
            "std::optional<bool> process_panda_state(",
            "\nvoid process_peripheral_state(",
        )?;
        fs::write(
            o.output.join("pandad_state_body.inc"),
            format!("{fill}\n{process}"),
        )?;
        sources.push(original);
    }
    for name in ["log", "custom", "deprecated"] {
        let path = root.join(format!("openpilot/cereal/{name}.capnp"));
        fs::copy(&path, schema.join(format!("{name}.capnp")))?;
        sources.push(path);
    }
    let car = root.join("opendbc_repo/opendbc/car/car.capnp");
    fs::copy(&car, schema.join("car.capnp"))?;
    sources.push(car);
    copy_tree(
        &root.join("openpilot/cereal/include"),
        &schema.join("include"),
    )?;
    let mut records = vec![];
    let mut command = strings(&["capnp", "compile"]);
    command.extend([
        format!("-I{}", schema.display()),
        format!("--src-prefix={}", schema.display()),
        format!("-oc++:{}", generated.display()),
    ]);
    command.extend(sorted_extension(&schema, "capnp")?.iter().map(s));
    captured(
        runner,
        command,
        &o.output.join("commands.json"),
        &mut records,
        "command",
        true,
    )?;
    let capnp = o.capnp.as_ref().ok_or("capnp prefix required")?;
    let library = capnp.join("lib/x86_64-linux-gnu");
    let binary = o.output.join("pandad-protocol-source");
    let mut command = strings(&[
        "clang++",
        "-std=c++17",
        "-O1",
        "-g",
        "-ffunction-sections",
        "-fdata-sections",
        "-Wl,--gc-sections",
    ]);
    includes(
        &mut command,
        &[
            root.to_path_buf(),
            root.join("openpilot"),
            root.join("msgq_repo"),
            o.output.clone(),
            generated.clone(),
            capnp.join("include"),
            o.json11.join("include"),
        ],
    );
    command.extend([s(&sources[0]), s(&sources[1])]);
    command.extend(extra.iter().map(s));
    command.extend(sorted_extension(&generated, "c++")?.iter().map(s));
    command.extend([
        s(o.json11.join("lib/libjson11.a")),
        format!("-L{}", library.display()),
        format!("-Wl,-rpath,{}", library.display()),
    ]);
    command.extend(strings(&["-lcapnp", "-lkj", "-pthread", "-o"]));
    command.push(s(&binary));
    sanitize(&mut command, o.sanitize);
    if o.mode == "state" {
        command.insert(1, "-Wl,--wrap=clock_gettime".into());
    }
    captured(
        runner,
        command,
        &o.output.join("commands.json"),
        &mut records,
        "command",
        true,
    )?;
    let scope=match o.mode.as_str() {
        "can-io"=>"unchanged Panda bulk CAN send/receive methods, codec, health gate and reset; recorded I/O",
        "state"=>"unchanged Panda state/CAN-state fill, publication and reconnect bodies with original Panda methods; recorded I/O",
        "device"=>"unchanged Panda control/read methods and packed health structures; recorded transport",
        "safety"=>"unchanged PandaSafety, native Params and control command methods; fixture transport and logging sinks",
        _=>"unchanged original CAN pack/unpack and SPI alert methods; fixture only replaces transport reset and logging sinks"
    };
    let report = json!({"binary":s(&binary),"binary_sha256":hash(&binary)?,"sources":hashes(root,&sources,true)?,"scope":scope,"sanitize":o.sanitize});
    write_json(&o.output.join("manifest.json"), &report)?;
    println!("{}", serde_json::to_string(&report)?);
    Ok(())
}
/// CLI shared by the five build entrypoints. `--root` adds explicit checkout selection.
pub fn cli(family: &str) -> Result<()> {
    let mut args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        let name = if family == "runtime-fixture" {
            "build-pandad-runtime-fixture".to_owned()
        } else {
            format!("build-pandad-{family}-source")
        };
        let extra = match family {
            "protocol" => {
                " --capnp-prefix PREFIX [--sanitize] [--safety | --device | --state | --can-io]"
            }
            "usb" => " --libusb-include INCLUDE [--compiler COMPILER] [--sanitize]",
            "spi" => " [--compiler COMPILER] [--sanitize]",
            "peripheral" => " [--sanitize]",
            "runtime-fixture" => " --libusb-include INCLUDE [--compiler COMPILER]",
            _ => return Err("unknown Panda build family".into()),
        };
        println!("Usage: {name} --output OUTPUT --json11-prefix PREFIX [--root ROOT]{extra}");
        return Ok(());
    }
    let mut root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    if let Some(at) = args.iter().position(|a| a == "--root") {
        if at + 1 >= args.len() {
            return Err("--root requires value".into());
        }
        root = PathBuf::from(args.remove(at + 1));
        args.remove(at);
    }
    let root = root.canonicalize()?;
    let options = Options::parse(family, &args)?;
    let space = disk_space::inspect(&options.output, 256 * 1024 * 1024)?;
    run(&root, family, &args, &mut NativeRunner, space.free_bytes)
}
