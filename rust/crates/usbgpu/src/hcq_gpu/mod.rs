mod bus;
use crate::{
    asic::Asic,
    gpu::{Gpu, Options},
    gpu_memory::{Buffer, BufferOptions},
    hcq_model::{Allocation, Device, Request},
    hcq_vm::{Function, Host, Memory},
    kernel::ScratchPlan,
    Error,
};
pub use bus::{transfer, HcqBus};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

pub struct HcqGpu<B: HcqBus> {
    gpu: Gpu<B>,
    buffers: Vec<Option<Buffer>>,
    scratch: Option<(u64, Buffer)>,
    timeline: Option<Buffer>,
    cancelled: Arc<AtomicBool>,
    timeout: Duration,
    deadline: Duration,
    warp: Option<crate::warp::Warp>,
}
impl<B: HcqBus> HcqGpu<B> {
    pub fn new(asic: Asic<B>, cancelled: Arc<AtomicBool>) -> Result<Self, Error> {
        if !asic.hw.bus.custom_bridge() {
            return Err(Error::Contract(
                "pinned HCQ2 USB dispatcher requires custom bridge",
            ));
        }
        let gpu = Gpu::new(
            asic,
            Options {
                compute_ring_bytes: Some(1 << 20),
                ..Options::default()
            },
        )?;
        if gpu.properties.target != [12, 0, 0] {
            return Err(Error::Contract("pinned HCQ2 model requires gfx1200"));
        }
        let timeout = gpu.options.wait.timeout;
        let deadline = gpu.heap.asic.hw.bus.now() + timeout;
        Ok(Self {
            gpu,
            buffers: Vec::new(),
            scratch: None,
            timeline: None,
            cancelled,
            timeout,
            deadline,
            warp: None,
        })
    }
    pub fn load_warp(&mut self, descriptor: &[u8], output: Allocation) -> Result<(), Error> {
        if self.warp.is_some() {
            return Err(Error::Contract("HCQ warp already loaded"));
        }
        let buffer = self.owned(output)?;
        self.warp = Some(crate::warp::Warp::load(descriptor, &mut self.gpu, buffer)?);
        Ok(())
    }
    pub fn warp(&mut self, frames: &[u8], transforms: &[u8]) -> Result<(), Error> {
        self.gpu.refresh_compute_position()?;
        self.warp
            .as_ref()
            .ok_or(Error::Contract("HCQ warp is not loaded"))?
            .run(&mut self.gpu, frames, transforms)
    }
    fn owned(&self, allocation: Allocation) -> Result<Buffer, Error> {
        let index = usize::try_from(allocation.key)
            .map_err(|_| Error::Contract("HCQ GPU buffer key overflow"))?;
        let buffer = self
            .buffers
            .get(index)
            .and_then(|value| *value)
            .ok_or(Error::Contract("HCQ GPU buffer is not an owned allocation"))?;
        if buffer.address() != allocation.device || allocation.bytes > buffer.size() {
            return Err(Error::Contract("HCQ GPU allocation identity mismatch"));
        }
        Ok(buffer)
    }
    fn scratch(&mut self, bytes: u64) -> Result<Buffer, Error> {
        let bytes = bytes.max(128);
        if let Some((size, buffer)) = self.scratch {
            if bytes <= size {
                return Ok(buffer);
            }
        }
        let props = &self.gpu.properties;
        let plan = ScratchPlan::new(
            &self.gpu.heap.asic.hw.catalog,
            props.target[0],
            u32::try_from(bytes)
                .map_err(|_| Error::Contract("HCQ scratch private size overflow"))?,
            props.cu_count,
            props.slots_per_cu,
            props.shader_engines,
            props.xccs,
        )?;
        let buffer = self
            .gpu
            .heap
            .allocate_vram(plan.total_bytes, BufferOptions::default())?;
        self.scratch = Some((bytes, buffer));
        Ok(buffer)
    }
}
impl<B: HcqBus> Host for HcqGpu<B> {
    fn poll(&mut self) -> Result<(), Error> {
        if self.cancelled.load(Ordering::Relaxed) {
            return Err(Error::Cancelled);
        }
        if self.gpu.heap.asic.hw.bus.now() >= self.deadline {
            return Err(Error::Protocol("HCQ USB dispatcher timed out".into()));
        }
        Ok(())
    }
    fn call(
        &mut self,
        function: Function,
        args: &[u64],
        memory: &mut Memory,
    ) -> Result<u64, Error> {
        self.gpu.heap.asic.hw.bus.transfer(function, args, memory)
    }
}
impl<B: HcqBus> Device for HcqGpu<B> {
    fn dispatch_position(&mut self) -> Result<Option<u64>, Error> {
        if self.warp.is_some() {
            Ok(Some(self.gpu.refresh_compute_position()?))
        } else {
            Ok(None)
        }
    }
    fn bus_lock(&self) -> Option<crate::bus_lock::BusLock> {
        self.gpu.heap.asic.hw.bus.bus_lock()
    }
    fn allocate(&mut self, request: Request<'_>) -> Result<Allocation, Error> {
        if self.cancelled.load(Ordering::Relaxed) {
            return Err(Error::Cancelled);
        }
        let options = BufferOptions {
            host: false,
            uncached: request.uncached,
            cpu_access: request.cpu_access
                || request.host
                || (request.bytes <= 65536 && request.bytes.is_multiple_of(4)),
        };
        let (buffer, host, bytes) = match request.tag {
            Some("ring_compute_0") => {
                let buffer = self.gpu.hcq_queue().0;
                (
                    Some(buffer),
                    self.gpu
                        .heap
                        .asic
                        .hw
                        .bus
                        .pci_address(self.gpu.heap.cpu(buffer)?)?,
                    buffer.size(),
                )
            }
            Some("write_ptr_compute_0") => {
                let buffer = self.gpu.hcq_queue().1;
                (
                    Some(buffer),
                    self.gpu
                        .heap
                        .asic
                        .hw
                        .bus
                        .pci_address(self.gpu.heap.cpu(buffer)?)?,
                    buffer.size(),
                )
            }
            Some("doorbell_compute_0") => (
                None,
                self.gpu
                    .heap
                    .asic
                    .hw
                    .bus
                    .doorbell_address(self.gpu.hcq_queue().2)?,
                8,
            ),
            Some("scratch") => {
                let buffer = self.scratch(request.elements)?;
                (Some(buffer), 0, buffer.size())
            }
            Some(
                "program" | "cmdbuf_compute_0" | "kernargs_compute_0" | "timeline" | "slots"
                | "usb_vram",
            )
            | None => {
                let buffer = self.gpu.heap.allocate_vram(request.bytes, options)?;
                if request.tag == Some("timeline") {
                    self.timeline = Some(buffer);
                }
                let host = if options.cpu_access {
                    self.gpu
                        .heap
                        .asic
                        .hw
                        .bus
                        .pci_address(self.gpu.heap.cpu(buffer)?)?
                } else {
                    0
                };
                (Some(buffer), host, request.bytes)
            }
            Some(_) => return Err(Error::Contract("unsupported HCQ GPU placeholder")),
        };
        let key = u64::try_from(self.buffers.len())
            .map_err(|_| Error::Contract("HCQ GPU buffer count overflow"))?;
        self.buffers.push(buffer);
        Ok(Allocation {
            key,
            device: buffer.map_or(host, |buffer| buffer.address()),
            host,
            bytes,
        })
    }
    fn write(&mut self, allocation: Allocation, offset: u64, data: &[u8]) -> Result<(), Error> {
        let view = self.owned(allocation)?.view(
            offset,
            u64::try_from(data.len())
                .map_err(|_| Error::Contract("HCQ GPU write size overflow"))?,
        )?;
        if self.gpu.heap.cpu(view).is_ok() {
            self.gpu.heap.write(view, data)
        } else {
            self.gpu.upload(view, data)
        }
    }
    fn read(&mut self, allocation: Allocation, data: &mut [u8]) -> Result<(), Error> {
        let buffer = self.owned(allocation)?;
        data.copy_from_slice(&self.gpu.download(buffer, data.len())?);
        Ok(())
    }
    fn synchronize(&mut self) -> Result<(), Error> {
        self.gpu.synchronize()?;
        if let Some(buffer) = self.timeline {
            let value = self.gpu.heap.read_scalar(buffer.view(8, 8)?, 8)?;
            let started = self.gpu.heap.asic.hw.bus.now();
            while self.gpu.heap.read_scalar(buffer.view(0, 8)?, 8)? < value {
                if self.cancelled.load(Ordering::Relaxed) {
                    return Err(Error::Cancelled);
                }
                if self.gpu.heap.asic.hw.bus.now().saturating_sub(started) >= self.timeout {
                    return Err(Error::Protocol("HCQ model signal wait timed out".into()));
                }
                self.gpu.heap.asic.hw.bus.sleep(Duration::from_micros(100));
            }
        }
        self.deadline = self.gpu.heap.asic.hw.bus.now() + self.timeout;
        Ok(())
    }
}
