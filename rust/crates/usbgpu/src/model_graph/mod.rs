mod bundle;
mod load;
mod manifest;
pub use bundle::Bundle;
#[cfg(test)]
mod tests;

use crate::{
    gpu::{Gpu, Program},
    gpu_memory::Buffer,
    runtime_bus::RuntimeBus,
    Error,
};
use manifest::{Binding, Call, Manifest};

pub struct Graph {
    manifest: Manifest,
    buffers: Vec<Buffer>,
    programs: Vec<Program>,
    failed: bool,
}
impl Graph {
    fn view(&self, index: usize) -> Result<Buffer, Error> {
        let view = self
            .manifest
            .views
            .get(index)
            .ok_or(Error::Contract("AMD graph view missing"))?;
        self.buffers
            .get(view.allocation)
            .ok_or(Error::Contract("AMD graph buffer missing"))?
            .view(view.offset, view.bytes)
    }
    fn binding(&self, bindings: &[Binding], name: &str) -> Result<Buffer, Error> {
        let binding = bindings
            .iter()
            .find(|binding| binding.name == name)
            .ok_or(Error::Contract("AMD graph binding missing"))?;
        self.view(binding.view)
    }
    pub fn write<B: RuntimeBus>(
        &mut self,
        gpu: &mut Gpu<B>,
        name: &str,
        bytes: &[u8],
    ) -> Result<(), Error> {
        if self.failed {
            return Err(Error::Contract("AMD graph stopped after failure"));
        }
        let result = (|| {
            let buffer = self.binding(&self.manifest.inputs, name)?;
            if buffer.size()
                != u64::try_from(bytes.len())
                    .map_err(|_| Error::Contract("AMD input size overflow"))?
            {
                return Err(Error::Contract("AMD graph input length mismatch"));
            }
            gpu.upload(buffer, bytes)
        })();
        self.failed = result.is_err();
        result
    }
    pub fn read<B: RuntimeBus>(&mut self, gpu: &mut Gpu<B>, name: &str) -> Result<Vec<u8>, Error> {
        if self.failed {
            return Err(Error::Contract("AMD graph stopped after failure"));
        }
        let result = (|| {
            let buffer = self.binding(&self.manifest.outputs, name)?;
            gpu.download(
                buffer,
                usize::try_from(buffer.size())
                    .map_err(|_| Error::Contract("AMD output size overflow"))?,
            )
        })();
        self.failed = result.is_err();
        result
    }
    pub fn run<B: RuntimeBus>(&mut self, gpu: &mut Gpu<B>, entry: &str) -> Result<(), Error> {
        if self.failed {
            return Err(Error::Contract("AMD graph stopped after failure"));
        }
        let result = (|| {
            for call in &self.manifest.calls[self.manifest.entry(entry)?] {
                match call {
                    Call::Kernel {
                        kernel,
                        views,
                        scalars,
                        global,
                        local,
                    } => {
                        let buffers = views
                            .iter()
                            .map(|view| self.view(*view))
                            .collect::<Result<Vec<_>, _>>()?;
                        gpu.execute(&self.programs[*kernel], &buffers, scalars, *global, *local)?;
                    }
                    Call::Copy {
                        source,
                        destination,
                    } => {
                        let source = self.view(*source)?;
                        let data = gpu.download(
                            source,
                            usize::try_from(source.size())
                                .map_err(|_| Error::Contract("AMD copy size overflow"))?,
                        )?;
                        gpu.upload(self.view(*destination)?, &data)?;
                    }
                }
            }
            gpu.synchronize()
        })();
        self.failed = result.is_err();
        result
    }
}
