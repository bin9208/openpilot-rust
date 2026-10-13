use crate::{
    gpu::{Gpu, Program},
    gpu_memory::{Buffer, BufferOptions},
    runtime_bus::RuntimeBus,
    Error,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct View {
    pub buffer: usize,
    pub offset: u64,
    #[serde(default)]
    pub bytes: Option<u64>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Kernel {
    pub name: String,
    pub elf: Vec<u8>,
    pub sha256: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Copy {
    pub buffer: usize,
    pub offset: u64,
    pub data: Vec<u8>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Call {
    pub kernel: usize,
    pub buffers: Vec<View>,
    pub values: Vec<u32>,
    pub global: [u32; 3],
    pub local: [u32; 3],
}
pub(crate) struct Graph {
    buffers: Vec<Buffer>,
    programs: Vec<Program>,
    calls: Vec<Call>,
}
pub(crate) struct Description {
    pub buffers: Vec<u64>,
    pub kernels: Vec<Kernel>,
    pub copies: Vec<Copy>,
    pub calls: Vec<Call>,
}
impl Graph {
    pub fn load<B: RuntimeBus>(
        gpu: &mut Gpu<B>,
        data: Description,
        bindings: &BTreeMap<usize, Buffer>,
    ) -> Result<Self, Error> {
        let Description {
            buffers: sizes,
            kernels,
            copies,
            calls,
        } = data;
        if sizes.is_empty()
            || sizes.len() > 64
            || kernels.is_empty()
            || kernels.len() > 64
            || copies.len() > 64
            || calls.len() > 64
        {
            return Err(Error::Contract("compiled GPU graph size limit"));
        }
        let mut total = 0u64;
        let mut buffers = Vec::new();
        for (index, size) in sizes.iter().copied().enumerate() {
            total = total
                .checked_add(size)
                .ok_or(Error::Contract("GPU graph allocation overflow"))?;
            if size == 0 || total > 64 << 20 {
                return Err(Error::Contract("GPU graph allocation limit"));
            }
            buffers.push(match bindings.get(&index) {
                Some(buffer) => buffer.view(0, size)?,
                None => gpu
                    .allocate(size, BufferOptions::default())?
                    .view(0, size)?,
            });
        }
        for copy in &copies {
            let buffer = buffers
                .get(copy.buffer)
                .ok_or(Error::Contract("GPU graph copy buffer missing"))?;
            gpu.upload(
                buffer.view(
                    copy.offset,
                    u64::try_from(copy.data.len())
                        .map_err(|_| Error::Contract("GPU copy size overflow"))?,
                )?,
                &copy.data,
            )?;
        }
        let mut programs = Vec::new();
        for kernel in &kernels {
            if kernel.name.is_empty()
                || format!("{:x}", Sha256::digest(&kernel.elf)) != kernel.sha256
            {
                return Err(Error::Contract("GPU graph kernel checksum mismatch"));
            }
            programs.push(gpu.load_program(&kernel.elf)?);
        }
        Ok(Self {
            buffers,
            programs,
            calls,
        })
    }
    pub fn view(&self, view: View) -> Result<Buffer, Error> {
        let buffer = self
            .buffers
            .get(view.buffer)
            .ok_or(Error::Contract("GPU graph buffer missing"))?;
        let bytes = view.bytes.unwrap_or(
            buffer
                .size()
                .checked_sub(view.offset)
                .ok_or(Error::Contract("GPU graph view offset"))?,
        );
        buffer.view(view.offset, bytes)
    }
    pub fn run<B: RuntimeBus>(&self, gpu: &mut Gpu<B>) -> Result<(), Error> {
        for call in &self.calls {
            let program = self
                .programs
                .get(call.kernel)
                .ok_or(Error::Contract("GPU graph kernel missing"))?;
            let buffers = call
                .buffers
                .iter()
                .map(|view| self.view(*view))
                .collect::<Result<Vec<_>, _>>()?;
            gpu.execute(program, &buffers, &call.values, call.global, call.local)?;
        }
        gpu.synchronize()
    }
}
