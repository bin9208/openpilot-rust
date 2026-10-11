use openpilot_usbgpu::{
    amd_bus::Bus,
    asic::{Asic, BootOptions},
    firmware::FirmwareSource,
    gpu::{Gpu, Options},
    local_artifact::Package,
    worker::Runtime,
    worker_local::NativeRuntime,
    Error,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    cell::Cell,
    collections::BTreeMap,
    io::{self, BufRead, Write},
    path::PathBuf,
    time::Duration,
};
include!("support/amd_rpc.rs");

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    entrypoint: String,
    inputs: BTreeMap<String, String>,
    outputs: BTreeMap<String, String>,
}
fn run() -> Result<Value, Error> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if ![3, 5, 6].contains(&args.len()) {
        return Err(Error::Contract(
            "expected model, assets and source sequence",
        ));
    }
    let package = Package::open(&PathBuf::from(&args[0]), &PathBuf::from(&args[1]), None)?;
    let camera = if args.len() >= 5 {
        [
            args[3]
                .parse()
                .map_err(|_| Error::Contract("fixture camera width"))?,
            args[4]
                .parse()
                .map_err(|_| Error::Contract("fixture camera height"))?,
        ]
    } else {
        [1344, 760]
    };
    let artifact = package.prepare(camera)?;
    let info = artifact.info.clone();
    let mut firmware = Source(artifact.assets.join("firmware"));
    let bus = Rpc {
        input: io::BufReader::new(io::stdin()),
        clock: Cell::new(0),
        custom: true,
    };
    let gpu = Gpu::new(
        Asic::boot(bus, &mut firmware, BootOptions::default())?,
        Options::default(),
    )?;
    let mut runtime = NativeRuntime::new(gpu, artifact)?;
    let sequence = PathBuf::from(&args[2]);
    let directory = sequence
        .parent()
        .ok_or(Error::Contract("sequence directory missing"))?;
    let entries: Vec<Entry> = serde_json::from_slice(&std::fs::read(&sequence)?)?;
    let mut packed = vec![0; info.input_bytes];
    let mut output = vec![0; info.output_count * 4];
    let mut index = 0;
    let mut calls = 0;
    while index < entries.len() {
        let prepare = &entries[index];
        if prepare.entrypoint != "prepare" {
            return Err(Error::Contract("source sequence order"));
        }
        for (target, source) in [
            ("img", "frame"),
            ("big_img", "big_frame"),
            ("tfm", "tfm"),
            ("big_tfm", "big_tfm"),
        ] {
            let bytes = std::fs::read(directory.join(&prepare.inputs[source]))?;
            let view = &info.layout[target];
            let expected = info.input(&packed, target)?.len();
            if bytes.len() != expected {
                return Err(Error::Contract("source frame length mismatch"));
            }
            packed[view.offset..view.offset + expected].copy_from_slice(&bytes);
        }
        let next = entries
            .get(index + 1)
            .filter(|entry| entry.entrypoint == "policy");
        let observed = if let Some(policy) = next {
            let scalars = std::fs::read(directory.join(&policy.inputs["packed_npy_inputs"]))?;
            let mut start = 0;
            for name in ["desire", "traffic_convention", "action_t"] {
                let bytes = info.input(&packed, name)?.len();
                let offset = info.layout[name].offset;
                packed[offset..offset + bytes].copy_from_slice(&scalars[start..start + bytes]);
                start += bytes;
            }
            runtime.run(&packed, &info, &mut output)?;
            index += 2;
            policy
        } else {
            if runtime.prepare(&packed, &info, &mut output)? {
                return Err(Error::Contract("split prepare published output"));
            }
            index += 1;
            prepare
        };
        for (name, path) in &observed.outputs {
            std::fs::write(
                directory.join(path),
                runtime.graph.read(&mut runtime.gpu, name)?,
            )?;
        }
        calls += 1;
    }
    let rejected = if args.len() == 6 {
        if args[5] != "--poison" {
            return Err(Error::Contract("unknown local fixture mode"));
        }
        let offset = info.layout["action_t"].offset;
        packed[offset..offset + 4].copy_from_slice(&f32::NAN.to_le_bytes());
        let failed = runtime.run(&packed, &info, &mut output).is_err();
        let stopped = runtime.run(&packed, &info, &mut output).is_err();
        if !failed || !stopped {
            return Err(Error::Contract(
                "local nonfinite failure did not stop runtime",
            ));
        }
        true
    } else {
        false
    };
    runtime.gpu.heap.asic.finish()?;
    Ok(
        json!({"calls":calls,"frame_size":info.frame_size,"camera":camera,"nonfinite_rejected":rejected}),
    )
}
fn main() {
    match run() {
        Ok(value) => println!("{}", json!({"done":value})),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
