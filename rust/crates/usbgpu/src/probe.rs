use crate::{
    gpu::Gpu,
    kernel_graph::{Call, Copy, Description, Graph, Kernel, View},
    runtime_bus::RuntimeBus,
    Error,
};
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    version: u32,
    arch: String,
    seed: u32,
    buffers: Vec<u64>,
    copies: Vec<Copy>,
    kernels: Vec<Kernel>,
    calls: Vec<Call>,
    output: View,
    source_sha256: String,
}

pub fn architecture<B: RuntimeBus>(gpu: &Gpu<B>) -> String {
    let [major, minor, step] = gpu.properties.target;
    format!("gfx{major}{minor}{step}")
}

fn parse(descriptor: &[u8], arch: &str) -> Result<Manifest, Error> {
    if descriptor.len() > 4 << 20 {
        return Err(Error::Contract("GPU probe descriptor size limit"));
    }
    let manifest: Manifest = serde_json::from_slice(descriptor)?;
    if manifest.version != 1
        || manifest.arch != arch
        || manifest.seed != 42
        || manifest.buffers != [8, 8, 1 << 22]
        || manifest.calls.len() != 2
        || manifest.kernels.len() != 2
        || manifest.output.buffer != 2
        || manifest.output.offset != 0
        || manifest.output.bytes != Some(1 << 22)
        || manifest.source_sha256.len() != 64
    {
        return Err(Error::Contract("unsupported GPU probe descriptor"));
    }
    let key = manifest
        .copies
        .iter()
        .find(|copy| copy.buffer == 1 && copy.offset == 0)
        .ok_or(Error::Contract("GPU probe random key missing"))?;
    if key.data != [25, 17, 184, 20, 42, 0, 0, 0] {
        return Err(Error::Contract("GPU probe random key mismatch"));
    }
    Ok(manifest)
}

pub(crate) fn validate_descriptor(descriptor: &[u8], arch: &str) -> Result<(), Error> {
    parse(descriptor, arch)?;
    Ok(())
}

pub fn run<B: RuntimeBus>(
    gpu: &mut Gpu<B>,
    descriptor: &[u8],
    seed: u32,
) -> Result<Vec<u8>, Error> {
    let mut manifest = parse(descriptor, &architecture(gpu))?;
    let key = manifest
        .copies
        .iter_mut()
        .find(|copy| copy.buffer == 1 && copy.offset == 0)
        .ok_or(Error::Contract("GPU probe random key missing"))?;
    key.data[4..8].copy_from_slice(&seed.to_le_bytes());
    let graph = Graph::load(
        gpu,
        Description {
            buffers: manifest.buffers,
            kernels: manifest.kernels,
            copies: manifest.copies,
            calls: manifest.calls,
        },
        &BTreeMap::new(),
    )?;
    let output = graph.view(manifest.output)?;
    graph.run(gpu)?;
    let mut last = Vec::new();
    for _ in 0..8 {
        last = gpu.download(output, 1 << 22)?;
        if !last
            .chunks_exact(4)
            .all(|bytes| f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]).is_finite())
        {
            return Err(Error::Contract(
                "GPU probe returned nonfinite random values",
            ));
        }
    }
    Ok(last)
}
