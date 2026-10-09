mod load;
mod manifest;
mod storage;
use crate::{
    hcq_vm::{Execution, Host, Memory},
    Error,
};
use manifest::Manifest;
#[cfg(feature = "fixture-inspection")]
use sha2::Digest;
use storage::Bound;

pub(crate) fn validate_descriptor(bytes: &[u8], sha: &str, size: u64) -> Result<(), Error> {
    let manifest = Manifest::parse(bytes)?;
    if manifest.model_sha256 != sha || manifest.model_bytes != size {
        return Err(Error::Contract("native descriptor model identity mismatch"));
    }
    crate::hcq_vm::Program::parse(&serde_json::to_vec(&manifest.dispatcher)?)?;
    Ok(())
}

#[derive(Clone, Copy, Debug)]
pub struct Allocation {
    pub key: u64,
    pub device: u64,
    pub host: u64,
    pub bytes: u64,
}
pub struct Request<'a> {
    pub bytes: u64,
    pub host: bool,
    pub cpu_access: bool,
    pub uncached: bool,
    pub tag: Option<&'a str>,
    pub elements: u64,
}
pub trait Device: Host {
    fn dispatch_position(&mut self) -> Result<Option<u64>, Error> {
        Ok(None)
    }
    fn bus_lock(&self) -> Option<crate::bus_lock::BusLock> {
        None
    }
    fn allocate(&mut self, request: Request<'_>) -> Result<Allocation, Error>;
    fn write(&mut self, buffer: Allocation, offset: u64, data: &[u8]) -> Result<(), Error>;
    fn read(&mut self, buffer: Allocation, data: &mut [u8]) -> Result<(), Error>;
    fn synchronize(&mut self) -> Result<(), Error>;
}
pub struct Model<D: Device> {
    manifest: Manifest,
    device: D,
    memory: Memory,
    buffers: Vec<Bound>,
    bindings: Vec<Allocation>,
    execution: Execution,
    failed: bool,
}
impl<D: Device> Model<D> {
    fn attempt<T>(
        &mut self,
        action: impl FnOnce(&mut Self) -> Result<T, Error>,
    ) -> Result<T, Error> {
        if self.failed {
            return Err(Error::Contract("HCQ model stopped after failure"));
        }
        let result = action(self);
        self.failed = result.is_err();
        result
    }
    #[cfg(feature = "fixture-inspection")]
    pub fn fixture_link_snapshot(&mut self) -> Result<serde_json::Value, Error> {
        let mut rows = Vec::with_capacity(self.buffers.len());
        for (index, bound) in self.buffers.iter().copied().enumerate() {
            let bytes = match bound {
                Bound::Host { bytes, .. } => bytes,
                Bound::Device(allocation) => allocation.bytes,
            };
            let mut data = vec![
                0;
                usize::try_from(bytes)
                    .map_err(|_| Error::Contract("snapshot size overflow"))?
            ];
            match bound {
                Bound::Host { address, .. } => {
                    let size = data.len();
                    data.copy_from_slice(self.memory.read(address, size)?)
                }
                Bound::Device(allocation) => self.device.read(allocation, &mut data)?,
            }
            rows.push(serde_json::json!({"index":index, "bytes":bytes,
                "device":bound.address(0, manifest::Space::Device)?,
                "host":bound.address(0, manifest::Space::Host)?,
                "sha256":format!("{:x}", sha2::Sha256::digest(&data))}));
        }
        Ok(serde_json::json!({"buffers":rows}))
    }
    #[cfg(feature = "fixture-inspection")]
    pub fn fixture_snapshot(&self) -> Result<serde_json::Value, Error> {
        let parameters = self
            .manifest
            .parameters
            .iter()
            .map(|parameter| {
                let view = self.manifest.arguments[parameter.slot];
                let address = storage::buffer(&self.buffers, view)?
                    .address(view.offset, manifest::Space::Host)?;
                let bytes = self.memory.read(
                    address,
                    usize::try_from(parameter.bytes)
                        .map_err(|_| Error::Contract("HCQ snapshot parameter size"))?,
                )?;
                Ok(serde_json::json!({
                    "slot": parameter.slot,
                    "name": parameter.name,
                    "data": bytes,
                }))
            })
            .collect::<Result<Vec<_>, Error>>()?;
        let bindings = self
            .manifest
            .bindings
            .iter()
            .zip(&self.bindings)
            .map(|(spec, allocation)| {
                serde_json::json!({"name":spec.name, "output":spec.output, "alias":spec.alias,
                "address":allocation.device, "bytes":allocation.bytes})
            })
            .collect::<Vec<_>>();
        let buffers = self
            .buffers
            .iter()
            .copied()
            .enumerate()
            .map(|(index, bound)| {
                Ok(serde_json::json!({"index":index,
                "device":bound.address(0, manifest::Space::Device)?,
                "host":bound.address(0, manifest::Space::Host)?}))
            })
            .collect::<Result<Vec<_>, Error>>()?;
        Ok(serde_json::json!({"parameters":parameters,"bindings":bindings,"buffers":buffers}))
    }
    pub fn run(&mut self) -> Result<(), Error> {
        self.attempt(Self::run_inner)
    }
    fn run_inner(&mut self) -> Result<(), Error> {
        let lock = self.device.bus_lock();
        let _guard = lock.as_ref().map(|lock| lock.enter()).transpose()?;
        self.device.synchronize()?;
        if let Some(position) = self.device.dispatch_position()? {
            let index = self
                .manifest
                .buffers
                .iter()
                .position(|buffer| {
                    matches!(buffer,
                manifest::Buffer::Placeholder { tag, .. } if tag == "put_value_compute_0")
                })
                .ok_or(Error::Contract("HCQ compute position buffer missing"))?;
            self.buffers[index].write(
                0,
                &position.to_le_bytes(),
                &mut self.memory,
                &mut self.device,
            )?;
        }
        let table = &self.manifest.input_table;
        let view = *self
            .manifest
            .arguments
            .get(table.argument)
            .ok_or(Error::Contract("HCQ input table argument missing"))?;
        let buffer = storage::buffer(&self.buffers, view)?;
        for (index, entry) in table.entries.iter().enumerate() {
            let binding = self.bindings[entry.slot];
            let value = binding
                .device
                .checked_add(entry.offset)
                .ok_or(Error::Contract("HCQ input address overflow"))?;
            let offset = view
                .offset
                .checked_add(
                    u64::try_from(index)
                        .map_err(|_| Error::Contract("HCQ table offset overflow"))?
                        * 8,
                )
                .ok_or(Error::Contract("HCQ table offset overflow"))?;
            buffer.write(
                offset,
                &value.to_le_bytes(),
                &mut self.memory,
                &mut self.device,
            )?;
        }
        self.execution.run(&mut self.memory, &mut self.device)
    }
    pub fn write_input(&mut self, name: &str, bytes: &[u8]) -> Result<(), Error> {
        self.attempt(|model| model.write_input_inner(name, bytes))
    }
    fn write_input_inner(&mut self, name: &str, bytes: &[u8]) -> Result<(), Error> {
        let lock = self.device.bus_lock();
        let _guard = lock.as_ref().map(|lock| lock.enter()).transpose()?;
        let index = self.binding(name, false, bytes.len())?;
        self.device.synchronize()?;
        self.device.write(self.bindings[index], 0, bytes)
    }
    pub fn read_output(&mut self, name: &str, bytes: &mut [u8]) -> Result<(), Error> {
        self.attempt(|model| model.read_output_inner(name, bytes))
    }
    fn read_output_inner(&mut self, name: &str, bytes: &mut [u8]) -> Result<(), Error> {
        let lock = self.device.bus_lock();
        let _guard = lock.as_ref().map(|lock| lock.enter()).transpose()?;
        let index = self.binding(name, true, bytes.len())?;
        self.device.synchronize()?;
        self.device.read(self.bindings[index], bytes)
    }
    pub fn prepare_input(
        &mut self,
        name: &str,
        action: impl FnOnce(&mut D, Allocation) -> Result<(), Error>,
    ) -> Result<(), Error> {
        self.attempt(|model| {
            let index = model
                .manifest
                .bindings
                .iter()
                .position(|b| b.name == name && !b.output)
                .ok_or(Error::Contract("HCQ input binding missing"))?;
            let lock = model.device.bus_lock();
            let _guard = lock.as_ref().map(|lock| lock.enter()).transpose()?;
            model.device.synchronize()?;
            action(&mut model.device, model.bindings[index])
        })
    }
    pub fn output_size(&self, name: &str) -> Result<usize, Error> {
        let binding = self
            .manifest
            .bindings
            .iter()
            .find(|b| b.output && b.name == name)
            .ok_or(Error::Contract("HCQ output binding missing"))?;
        usize::try_from(binding.bytes).map_err(|_| Error::Contract("HCQ output size overflow"))
    }
    fn binding(&self, name: &str, output: bool, bytes: usize) -> Result<usize, Error> {
        self.manifest
            .bindings
            .iter()
            .position(|b| b.name == name && b.output == output && b.bytes == bytes as u64)
            .ok_or(Error::Contract("HCQ tensor name or byte size mismatch"))
    }
}
