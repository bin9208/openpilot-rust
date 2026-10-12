use super::{Bundle, Graph};
use crate::{gpu::Gpu, gpu_memory::BufferOptions, runtime_bus::RuntimeBus, Error};
use std::{
    io::{Read, Seek, SeekFrom},
    path::Path,
};

impl Graph {
    pub fn load<B: RuntimeBus>(root: &Path, gpu: &mut Gpu<B>) -> Result<Self, Error> {
        Self::from_bundle(Bundle::open(root)?, gpu)
    }
    pub fn from_bundle<B: RuntimeBus>(bundle: Bundle, gpu: &mut Gpu<B>) -> Result<Self, Error> {
        let Bundle {
            manifest,
            mut weights,
            kernels,
        } = bundle;
        if gpu.properties.target != [12, 0, 0] {
            return Err(Error::Contract("AMD model graph target mismatch"));
        }
        let mut buffers = Vec::with_capacity(manifest.allocations.len());
        let mut chunk = vec![0u8; 256 << 10];
        for spec in &manifest.allocations {
            let buffer = gpu.allocate(spec.bytes, BufferOptions::default())?;
            if let Some(offset) = spec.weight_offset {
                weights.seek(SeekFrom::Start(offset))?;
            }
            let mut offset = 0;
            while offset < spec.bytes {
                let size = usize::try_from(
                    (spec.bytes - offset).min(
                        u64::try_from(chunk.len())
                            .map_err(|_| Error::Contract("AMD chunk size overflow"))?,
                    ),
                )
                .map_err(|_| Error::Contract("AMD allocation size overflow"))?;
                let bytes = &mut chunk[..size];
                match spec.weight_offset {
                    Some(_) => weights.read_exact(bytes)?,
                    None => bytes.fill(0),
                }
                gpu.upload(
                    buffer.view(
                        offset,
                        u64::try_from(size)
                            .map_err(|_| Error::Contract("AMD chunk length overflow"))?,
                    )?,
                    bytes,
                )?;
                offset += u64::try_from(size)
                    .map_err(|_| Error::Contract("AMD chunk length overflow"))?;
            }
            buffers.push(buffer);
        }
        let mut programs = Vec::with_capacity(manifest.kernels.len());
        for bytes in kernels {
            programs.push(gpu.load_program(&bytes)?);
        }
        Ok(Self {
            manifest,
            buffers,
            programs,
            failed: false,
        })
    }
}
