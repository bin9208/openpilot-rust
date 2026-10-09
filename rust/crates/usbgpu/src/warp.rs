use crate::{
    gpu::Gpu,
    gpu_memory::Buffer,
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
    camera: [u32; 2],
    frame_size: u64,
    buffers: Vec<u64>,
    copies: Vec<Copy>,
    kernels: Vec<Kernel>,
    calls: Vec<Call>,
    inputs: BTreeMap<String, View>,
    output: View,
    source_sha256: String,
}
pub struct Warp {
    camera: [u32; 2],
    inputs: BTreeMap<String, View>,
    graph: Graph,
}
impl Warp {
    pub fn load<B: RuntimeBus>(
        descriptor: &[u8],
        gpu: &mut Gpu<B>,
        output: Buffer,
    ) -> Result<Self, Error> {
        if descriptor.len() > 4 << 20 {
            return Err(Error::Contract("warp descriptor size limit"));
        }
        let manifest: Manifest = serde_json::from_slice(descriptor)?;
        if manifest.version != 1
            || manifest.arch != "gfx1200"
            || gpu.properties.target != [12, 0, 0]
            || ![[1344, 760], [1928, 1208]].contains(&manifest.camera)
            || manifest.buffers.is_empty()
            || manifest.buffers.len() > 64
            || manifest.kernels.is_empty()
            || manifest.kernels.len() > 64
            || manifest.calls.len() > 64
            || manifest.copies.len() > 64
            || manifest.source_sha256.len() != 64
            || manifest.output.offset != 0
            || manifest.output.bytes != Some(393216)
            || output.size() < 393216
        {
            return Err(Error::Contract("unsupported warp descriptor"));
        }
        let [width, height] = manifest.camera;
        let expected = u64::from(width.next_multiple_of(128))
            * u64::from(height.next_multiple_of(32) + (height / 2).next_multiple_of(16));
        if manifest.frame_size != expected
            || manifest.inputs.len() != 2
            || manifest.inputs.get("frames").and_then(|v| v.bytes) != Some(expected * 2)
            || manifest.inputs.get("transforms").and_then(|v| v.bytes) != Some(72)
            || manifest.buffers.get(manifest.output.buffer) != Some(&393216)
        {
            return Err(Error::Contract("warp input/output layout mismatch"));
        }
        let graph = Graph::load(
            gpu,
            Description {
                buffers: manifest.buffers,
                kernels: manifest.kernels,
                copies: manifest.copies,
                calls: manifest.calls,
            },
            &BTreeMap::from([(manifest.output.buffer, output)]),
        )?;
        Ok(Self {
            camera: manifest.camera,
            inputs: manifest.inputs,
            graph,
        })
    }
    pub fn camera(&self) -> [u32; 2] {
        self.camera
    }
    pub fn run<B: RuntimeBus>(
        &self,
        gpu: &mut Gpu<B>,
        frames: &[u8],
        transforms: &[u8],
    ) -> Result<(), Error> {
        for (name, bytes) in [("frames", frames), ("transforms", transforms)] {
            let view = self
                .inputs
                .get(name)
                .ok_or(Error::Contract("warp input missing"))?;
            if view.bytes != Some(bytes.len() as u64) {
                return Err(Error::Contract("warp input byte size mismatch"));
            }
            gpu.upload(self.graph.view(*view)?, bytes)?;
        }
        self.graph.run(gpu)
    }
}
