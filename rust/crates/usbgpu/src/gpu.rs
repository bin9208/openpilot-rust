use crate::{
    asic::Asic,
    asic_gfx::ComputeRing,
    asic_sdma::CopyRing,
    gpu_memory::{Buffer, BufferOptions, Heap, SystemPool},
    kernel::{Kernel, ScratchPlan},
    packets::{ComputePackets, CopyPackets, Dispatch},
    queue::Ring,
    runtime_bus::{NativeRingIo, RuntimeBus},
    Error,
};
use serde::Serialize;
use std::{collections::BTreeMap, time::Duration};
#[derive(Clone, Debug, Serialize)]
pub struct Properties {
    pub target: [u8; 3],
    pub gc_version: [u8; 3],
    pub nbio_version: [u8; 3],
    pub sdma_version: [u8; 3],
    pub xccs: u8,
    pub cu_count: u32,
    pub shader_engines: u32,
    pub slots_per_cu: u32,
    pub waves_per_cu: u32,
    pub lds_kib: u32,
}
impl Properties {
    fn from_asic<B: RuntimeBus>(asic: &Asic<B>) -> Result<Self, Error> {
        let gc_version = asic.hw.gfx()?;
        let mut target = gc_version;
        if target == [9, 4, 3] {
            target = [9, 4, 2];
        }
        if target != [9, 4, 2] && target != [9, 5, 0] && !matches!(target[0], 11 | 12) {
            return Err(Error::Contract("unsupported AMD compute architecture"));
        }
        let get = |name| {
            asic.hw
                .discovery
                .gc_info
                .get(name)
                .copied()
                .ok_or_else(|| Error::Protocol(format!("missing GPU property {name}")))
        };
        let (cu_per_array, arrays_per_engine) = if asic.hw.discovery.gc_version[0] == 2 {
            (get("gc_num_cu_per_sh")?, get("gc_num_sh_per_se")?)
        } else {
            (
                2 * (get("gc_num_wgp0_per_sa")? + get("gc_num_wgp1_per_sa")?),
                get("gc_num_sa_per_se")?,
            )
        };
        let engines = get("gc_num_se")?;
        let xccs = asic.hw.gmc()?.xccs;
        let value = Self {
            target,
            gc_version,
            nbio_version: asic.hw.version(14)?,
            sdma_version: asic.hw.version(3)?,
            xccs,
            cu_count: u32::try_from(cu_per_array * arrays_per_engine * engines)
                .map_err(|_| Error::Contract("GPU CU count overflow"))?,
            shader_engines: engines as u32,
            slots_per_cu: get("gc_max_scratch_slots_per_cu")? as u32,
            waves_per_cu: (get("gc_max_waves_per_simd")? * 2) as u32,
            lds_kib: get("gc_lds_size")? as u32,
        };
        if value.xccs == 0 || value.xccs > 8 || value.cu_count == 0 || value.shader_engines == 0 {
            return Err(Error::Contract("invalid AMD compute geometry"));
        }
        Ok(value)
    }
}
#[derive(Clone, Copy)]
pub struct WaitPolicy {
    pub timeout: Duration,
    pub spin_milliseconds: u64,
    pub sleep: Duration,
}
impl Default for WaitPolicy {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            spin_milliseconds: 1,
            sleep: Duration::from_micros(100),
        }
    }
}
#[derive(Default)]
pub struct Options {
    pub aql: Option<bool>,
    pub disable_copy: bool,
    pub wait: WaitPolicy,
    pub waves_per_shader: u32,
    pub compute_ring_bytes: Option<u64>,
}
struct QueueBinding {
    ring: Ring,
    buffer: Buffer,
    gart: Buffer,
    aql_descriptor: Option<Vec<u8>>,
    read: Buffer,
    write: Buffer,
    doorbell: u32,
    compute: Option<ComputeRing>,
}
impl QueueBinding {
    fn create<B: RuntimeBus>(
        heap: &mut Heap<B>,
        pool: &mut SystemPool,
        properties: &Properties,
        copy: bool,
        aql: bool,
        ring_bytes: u64,
    ) -> Result<Self, Error> {
        let options = BufferOptions {
            host: false,
            uncached: true,
            cpu_access: true,
        };
        let buffer = heap.allocate(ring_bytes, options, pool)?;
        let gart = heap.allocate(256, options, pool)?;
        let catalog = &heap.asic.hw.catalog;
        let (_, read_offset) = catalog.field("struct_amd_queue_s", &["read_dispatch_id"])?;
        let (_, write_offset) = catalog.field("struct_amd_queue_s", &["write_dispatch_id"])?;
        let read = gart.view(read_offset as u64, 8)?;
        let write = gart.view(write_offset as u64, 8)?;
        let mut aql_descriptor = None;
        if aql && !copy {
            let record = "struct_amd_queue_s";
            let mut bytes = vec![0; catalog.layout(record)?.size];
            let flags = catalog.constant("hsa", "AMD_QUEUE_PROPERTIES_IS_PTR64")?
                | catalog.constant("hsa", "AMD_QUEUE_PROPERTIES_ENABLE_PROFILING")?;
            catalog.write(record, &["queue_properties"], &mut bytes, flags)?;
            catalog.write(
                record,
                &["read_dispatch_id_field_base_byte_offset"],
                &mut bytes,
                read_offset as u64,
            )?;
            catalog.write(
                record,
                &["max_cu_id"],
                &mut bytes,
                u64::from(properties.cu_count) * u64::from(properties.xccs) - 1,
            )?;
            catalog.write(
                record,
                &["max_wave_id"],
                &mut bytes,
                u64::from(properties.waves_per_cu) - 1,
            )?;
            heap.write(gart, &bytes)?;
            aql_descriptor = Some(bytes);
        }
        let (doorbell, compute) = if copy {
            (
                heap.asic.setup_copy_ring(CopyRing {
                    address: buffer.address(),
                    size: buffer.size(),
                    read_pointer: read.address(),
                    write_pointer: write.address(),
                    index: 0,
                })?,
                None,
            )
        } else {
            let eop = heap.allocate(4096, BufferOptions::default(), pool)?;
            let config = ComputeRing {
                address: buffer.address(),
                size: buffer.size(),
                read_pointer: read.address(),
                write_pointer: write.address(),
                eop: eop.address(),
                eop_size: eop.size(),
                index: u8::from(aql),
                aql,
            };
            if !aql {
                heap.cache(buffer)?;
            }
            (heap.asic.setup_compute_ring(config)?, Some(config))
        };
        Ok(Self {
            ring: Ring {
                virtual_address: buffer.address(),
                bytes: buffer.size() as usize,
                put: 0,
            },
            buffer,
            gart,
            aql_descriptor,
            read,
            write,
            doorbell,
            compute,
        })
    }
    fn compute<B: RuntimeBus>(
        &mut self,
        heap: &mut Heap<B>,
        words: &[u32],
        xccs: u8,
        bound: bool,
    ) -> Result<(), Error> {
        let ring = heap.cpu(self.buffer)?;
        let pointer = heap.cpu(self.write)?;
        self.ring.submit_compute(
            &mut NativeRingIo {
                bus: &mut heap.asic.hw.bus,
                ring,
                pointer,
                doorbell: u64::from(self.doorbell),
            },
            words,
            xccs > 1,
            bound,
        )
    }
    fn copy<B: RuntimeBus>(
        &mut self,
        heap: &mut Heap<B>,
        words: &[u32],
        sizes: &[usize],
        bound: bool,
    ) -> Result<(), Error> {
        let ring = heap.cpu(self.buffer)?;
        let pointer = heap.cpu(self.write)?;
        self.ring.submit_copy(
            &mut NativeRingIo {
                bus: &mut heap.asic.hw.bus,
                ring,
                pointer,
                doorbell: u64::from(self.doorbell),
            },
            words,
            sizes,
            bound,
        )
    }
    fn aql<B: RuntimeBus>(&mut self, heap: &mut Heap<B>, bytes: &[u8]) -> Result<(), Error> {
        let ring = heap.cpu(self.buffer)?;
        let pointer = heap.cpu(self.write)?;
        self.ring.submit_aql(
            &mut NativeRingIo {
                bus: &mut heap.asic.hw.bus,
                ring,
                pointer,
                doorbell: u64::from(self.doorbell),
            },
            bytes,
        )
    }
}
pub struct Program {
    pub buffer: Buffer,
    pub descriptor: crate::kernel::Descriptor,
}
pub struct Gpu<B: RuntimeBus> {
    pub heap: Heap<B>,
    pub properties: Properties,
    pub options: Options,
    pub system: SystemPool,
    pub staging: Buffer,
    pub completion: Buffer,
    pub timeline: Buffer,
    shadow_timeline: Buffer,
    pub next_timeline: u32,
    staging_timeline: u32,
    compute: QueueBinding,
    copy: Option<QueueBinding>,
    arguments: Buffer,
    argument_offset: u64,
    scratch: Buffer,
    scratch_plan: ScratchPlan,
    aql: bool,
    aql_indirect: Option<Buffer>,
    aql_indirect_offset: u64,
    registers: BTreeMap<String, crate::amd_metadata::Register>,
    error: Option<String>,
}
impl<B: RuntimeBus> Gpu<B> {
    pub fn new(asic: Asic<B>, options: Options) -> Result<Self, Error> {
        let properties = Properties::from_asic(&asic)?;
        let mut heap = Heap::new(asic);
        heap.asic.hw.bus.cache_doorbells()?;
        let staging = heap.map_system(0xf000, 0x200000, 256 << 10)?;
        let mut system = SystemPool {
            buffer: heap.map_system(0xa000, 0x820000, 4096)?,
            next: 0x800,
        };
        let completion = heap.map_system(0xb800, 0x822000, 4096)?;
        let aql = options.aql.unwrap_or(properties.xccs > 1);
        let aql_indirect = if aql {
            Some(heap.allocate(
                8192,
                BufferOptions {
                    uncached: true,
                    cpu_access: true,
                    host: false,
                },
                &mut system,
            )?)
        } else {
            None
        };
        let ring_bytes = options.compute_ring_bytes.unwrap_or(8192);
        if !ring_bytes.is_power_of_two() || !(8192..=16 << 20).contains(&ring_bytes) {
            return Err(Error::Contract("invalid AMD compute ring size"));
        }
        let compute =
            QueueBinding::create(&mut heap, &mut system, &properties, false, aql, ring_bytes)?;
        let copy = if options.disable_copy {
            None
        } else {
            match QueueBinding::create(&mut heap, &mut system, &properties, true, false, 512) {
                Ok(queue) => Some(queue),
                Err(Error::Io(_)) => None,
                Err(error) => return Err(error),
            }
        };
        let signals = heap.allocate(
            256,
            BufferOptions {
                host: true,
                uncached: true,
                cpu_access: true,
            },
            &mut system,
        )?;
        let timeline = signals.view(240, 16)?;
        heap.write_scalar(timeline, 8, 0)?;
        let shadow_timeline = signals.view(224, 16)?;
        heap.write_scalar(shadow_timeline, 8, 0)?;
        let arguments = heap.allocate(
            8192,
            BufferOptions {
                cpu_access: true,
                ..BufferOptions::default()
            },
            &mut system,
        )?;
        let scratch_plan = ScratchPlan::new(
            &heap.asic.hw.catalog,
            properties.target[0],
            128,
            properties.cu_count,
            properties.slots_per_cu,
            properties.shader_engines,
            properties.xccs,
        )?;
        let scratch = heap.allocate(
            scratch_plan.total_bytes,
            BufferOptions {
                cpu_access: copy.is_none(),
                ..BufferOptions::default()
            },
            &mut system,
        )?;
        let registers = heap
            .asic
            .hw
            .catalog
            .queue_registers(properties.gc_version, properties.nbio_version)?;
        let mut result = Self {
            heap,
            properties,
            options,
            system,
            staging,
            completion,
            timeline,
            shadow_timeline,
            next_timeline: 1,
            staging_timeline: 0,
            compute,
            copy,
            arguments,
            argument_offset: 0,
            scratch,
            scratch_plan,
            aql,
            aql_indirect,
            aql_indirect_offset: 0,
            registers,
            error: None,
        };
        result.update_aql_scratch()?;
        Ok(result)
    }
    fn update_aql_scratch(&mut self) -> Result<(), Error> {
        if self.aql {
            let bytes = self
                .compute
                .aql_descriptor
                .as_mut()
                .ok_or(Error::Contract("AQL descriptor missing"))?;
            self.scratch_plan.aql_descriptor(
                &self.heap.asic.hw.catalog,
                self.properties.target[0],
                self.scratch.address(),
                bytes,
            )?;
            self.heap.write(self.compute.gart, bytes)?;
        }
        Ok(())
    }
    pub fn allocate(&mut self, size: u64, mut options: BufferOptions) -> Result<Buffer, Error> {
        if self.copy.is_none() {
            options.cpu_access = true;
        }
        self.heap.allocate(size, options, &mut self.system)
    }
    pub(crate) fn hcq_queue(&self) -> (Buffer, Buffer, u32) {
        (
            self.compute.buffer,
            self.compute.write,
            self.compute.doorbell,
        )
    }
    pub(crate) fn refresh_compute_position(&mut self) -> Result<u64, Error> {
        let value = self.heap.read_scalar(self.compute.write, 8)?;
        self.compute.ring.put = value;
        Ok(value)
    }
    fn next_signal(&mut self) -> Result<u32, Error> {
        let value = self.next_timeline;
        self.next_timeline = self
            .next_timeline
            .checked_add(1)
            .ok_or(Error::Contract("GPU timeline exhausted"))?;
        Ok(value)
    }
    pub fn wait_signal(
        &mut self,
        signal: Buffer,
        value: u32,
        policy: WaitPolicy,
    ) -> Result<(), Error> {
        let mut start = self.heap.asic.hw.bus.now().as_millis();
        loop {
            let previous = self.heap.read_scalar(signal, 8)?;
            if previous >= u64::from(value) {
                return Ok(());
            }
            let now = self.heap.asic.hw.bus.now().as_millis();
            let elapsed = now.saturating_sub(start);
            if elapsed >= policy.timeout.as_millis() {
                if self.heap.read_scalar(signal, 8)? < u64::from(value) {
                    let last = self.heap.read_scalar(signal, 8)?;
                    return Err(Error::Protocol(format!(
                        "Wait timeout: {} ms! (the signal is not set to {value}, but {last})",
                        policy.timeout.as_millis()
                    )));
                }
                return Ok(());
            }
            if elapsed > u128::from(policy.spin_milliseconds) && !policy.sleep.is_zero() {
                self.heap.asic.hw.bus.sleep(policy.sleep);
            }
            if self.heap.read_scalar(signal, 8)? != previous {
                start = self.heap.asic.hw.bus.now().as_millis();
            }
        }
    }
    pub fn synchronize(&mut self) -> Result<(), Error> {
        if let Some(error) = &self.error {
            return Err(Error::Protocol(error.clone()));
        }
        if let Err(error) =
            self.wait_signal(self.timeline, self.next_timeline - 1, self.options.wait)
        {
            self.error = Some(error.to_string());
            self.heap.asic.handle_interrupts()?;
            if self.heap.asic.recover(true)? {
                self.compute.ring.put = 0;
                self.heap.write_scalar(self.compute.read, 8, 0)?;
                self.heap.write_scalar(self.compute.write, 8, 0)?;
                self.heap.asic.setup_compute_ring(
                    self.compute
                        .compute
                        .ok_or(Error::Contract("compute recovery configuration missing"))?,
                )?;
                self.heap
                    .write_scalar(self.timeline, 8, u64::from(self.next_timeline - 1))?;
                self.error = None;
            }
            return Err(Error::Protocol("Device hang detected".into()));
        }
        if self.next_timeline > 1 << 31 {
            std::mem::swap(&mut self.timeline, &mut self.shadow_timeline);
            self.next_timeline = 1;
            self.heap.write_scalar(self.timeline, 8, 0)?;
            self.staging_timeline = 0;
        }
        Ok(())
    }
    fn submit_copy_words(&mut self, words: &[u32], sizes: &[usize]) -> Result<(), Error> {
        let result = self
            .copy
            .as_mut()
            .ok_or(Error::Contract("SDMA queue unavailable"))?
            .copy(&mut self.heap, words, sizes, false);
        if let Err(error) = &result {
            self.error = Some(error.to_string());
        }
        result
    }
    pub fn upload(&mut self, destination: Buffer, bytes: &[u8]) -> Result<(), Error> {
        if bytes.len() as u64 > destination.size() {
            return Err(Error::Contract("upload exceeds GPU buffer"));
        }
        if self.copy.is_none() {
            self.synchronize()?;
            return self.heap.write(destination, bytes);
        }
        for (index, chunk) in bytes.chunks(self.staging.size() as usize).enumerate() {
            self.wait_signal(self.timeline, self.staging_timeline, self.options.wait)?;
            self.heap.write(self.staging, chunk)?;
            let value = self.next_signal()?;
            let mut queue = CopyPackets::new(
                &self.heap.asic.hw.catalog,
                self.properties.sdma_version[0],
                self.properties.target[0],
            )?;
            queue.max_copy_size = if self.properties.sdma_version[0] >= 5 {
                0x40000000
            } else {
                0x400000
            };
            queue.wait(self.timeline.address(), value - 1)?;
            queue.copy(
                destination.address() + index as u64 * self.staging.size(),
                self.staging.address(),
                chunk.len() as u64,
            )?;
            queue.signal(self.timeline.address(), value, true)?;
            let (words, sizes) = (queue.words, queue.command_sizes);
            self.submit_copy_words(&words, &sizes)?;
            self.staging_timeline = value;
        }
        Ok(())
    }
    pub fn download(&mut self, source: Buffer, size: usize) -> Result<Vec<u8>, Error> {
        if size as u64 > source.size() {
            return Err(Error::Contract("download exceeds GPU buffer"));
        }
        self.synchronize()?;
        if self.copy.is_none() {
            return self.heap.read(source, size);
        }
        let custom = self.heap.asic.hw.bus.custom_bridge();
        let chunk_size = if custom {
            self.staging.size() as usize
        } else {
            4096
        };
        let mut output = Vec::with_capacity(size);
        for offset in (0..size).step_by(chunk_size) {
            let size = (size - offset).min(chunk_size);
            if custom {
                self.heap.asic.hw.bus.arm_staging_read(size)?;
            }
            let value = self.next_signal()?;
            let mut queue = CopyPackets::new(
                &self.heap.asic.hw.catalog,
                self.properties.sdma_version[0],
                self.properties.target[0],
            )?;
            queue.wait(self.timeline.address(), value - 1)?;
            queue.copy(
                self.staging.address(),
                source.address() + offset as u64,
                size as u64,
            )?;
            if custom {
                queue.write(self.completion.address() + 12, 0, false)?;
            }
            queue.signal(self.timeline.address(), value, true)?;
            let (words, sizes) = (queue.words, queue.command_sizes);
            self.submit_copy_words(&words, &sizes)?;
            if !custom {
                self.wait_signal(self.timeline, value, self.options.wait)?;
            }
            output.extend_from_slice(&self.heap.read(self.staging, size)?);
        }
        Ok(output)
    }
    pub fn load_program(&mut self, elf: &[u8]) -> Result<Program, Error> {
        let kernel = Kernel::load(
            &self.heap.asic.hw.catalog,
            elf,
            self.properties.target[0],
            self.properties.lds_kib,
        )?;
        let buffer = self.allocate(kernel.image.len() as u64, BufferOptions::default())?;
        self.upload(buffer, &kernel.image)?;
        self.synchronize()?;
        self.ensure_scratch(kernel.descriptor.private_segment_size)?;
        Ok(Program {
            buffer,
            descriptor: kernel.descriptor,
        })
    }
    pub fn ensure_scratch(&mut self, size: u32) -> Result<(), Error> {
        if size <= self.scratch_plan.private_bytes {
            return Ok(());
        }
        let plan = ScratchPlan::new(
            &self.heap.asic.hw.catalog,
            self.properties.target[0],
            size,
            self.properties.cu_count,
            self.properties.slots_per_cu,
            self.properties.shader_engines,
            self.properties.xccs,
        )?;
        self.synchronize()?;
        let next = self.allocate(plan.total_bytes, BufferOptions::default())?;
        self.heap.free(self.scratch)?;
        self.scratch = next;
        self.scratch_plan = plan;
        self.update_aql_scratch()
    }
    pub fn release_program(&mut self, program: Program) -> Result<(), Error> {
        self.synchronize()?;
        self.heap.free(program.buffer)
    }
    fn submit_compute_words(&mut self, words: &[u32]) -> Result<(), Error> {
        let result = self
            .compute
            .compute(&mut self.heap, words, self.properties.xccs, false);
        if let Err(error) = &result {
            self.error = Some(error.to_string());
        }
        result
    }
    pub fn execute(
        &mut self,
        program: &Program,
        buffers: &[Buffer],
        values: &[u32],
        global: [u32; 3],
        local: [u32; 3],
    ) -> Result<(), Error> {
        let needed = program.descriptor.argument_allocation_size as u64;
        if needed > self.arguments.size() {
            return Err(Error::Contract("kernel arguments exceed USB argument ring"));
        }
        let mut offset = self.argument_offset.next_multiple_of(8);
        if offset + needed > self.arguments.size() {
            offset = 0;
        }
        let arguments = self.arguments.view(offset, needed)?;
        self.argument_offset = offset + needed;
        if buffers.len() * 8 + values.len() * 4 > program.descriptor.kernargs_segment_size as usize
        {
            return Err(Error::Contract("too many AMD kernel arguments"));
        }
        for (index, buffer) in buffers.iter().enumerate() {
            self.heap
                .write_scalar(arguments.view(index as u64 * 8, 8)?, 8, buffer.address())?;
        }
        for (index, value) in values.iter().enumerate() {
            self.heap.write_scalar(
                arguments.view(buffers.len() as u64 * 8 + index as u64 * 4, 4)?,
                4,
                u64::from(*value),
            )?;
        }
        if program.descriptor.dispatch_pointer {
            let bytes = program.descriptor.dispatch_packet(
                &self.heap.asic.hw.catalog,
                program.buffer.address(),
                arguments.address(),
                global,
                local,
                false,
            )?;
            self.heap.write(
                arguments.view(
                    u64::from(program.descriptor.kernargs_segment_size),
                    bytes.len() as u64,
                )?,
                &bytes,
            )?;
        }
        let value = self.next_signal()?;
        let mut queue = ComputePackets::new(
            &self.heap.asic.hw.catalog,
            &self.registers,
            self.properties.target[0],
            self.properties.xccs,
        );
        queue.wait(self.timeline.address(), value - 1, u32::MAX, 5)?;
        queue.memory_barrier(self.properties.nbio_version)?;
        if self.aql {
            let before = queue.words.clone();
            queue.words.clear();
            queue.signal(self.timeline.address(), value)?;
            let after = queue.words;
            let dispatch = program.descriptor.dispatch_packet(
                &self.heap.asic.hw.catalog,
                program.buffer.address(),
                arguments.address(),
                global,
                local,
                true,
            )?;
            return self.submit_aql_dispatch(&before, &dispatch, &after);
        }
        queue.dispatch(Dispatch {
            descriptor: &program.descriptor,
            program_base: program.buffer.address(),
            arguments: arguments.address(),
            global,
            local,
            scratch_address: self.scratch.address(),
            scratch_bytes: self.scratch.size(),
            tmpring: self.scratch_plan.tmpring,
            waves_per_shader: self.options.waves_per_shader,
        })?;
        queue.signal(self.timeline.address(), value)?;
        let words = queue.words;
        self.submit_compute_words(&words)
    }
    fn submit_aql_dispatch(
        &mut self,
        before: &[u32],
        dispatch: &[u8],
        after: &[u32],
    ) -> Result<(), Error> {
        let buffer = self
            .aql_indirect
            .ok_or(Error::Contract("AQL indirect allocation missing"))?;
        let count = before.len() + 1 + after.len();
        let size = count as u64 * 4;
        let mut offset = self.aql_indirect_offset.next_multiple_of(16);
        if offset + size > buffer.size() {
            offset = 0;
        }
        if size > buffer.size() {
            return Err(Error::Contract("AQL PM4 arguments exceed indirect buffer"));
        }
        self.aql_indirect_offset = offset + size;
        let storage = buffer.view(offset, size)?;
        let words = before
            .iter()
            .copied()
            .chain([0])
            .chain(after.iter().copied())
            .flat_map(u32::to_le_bytes)
            .collect::<Vec<_>>();
        self.heap.write(storage, &words)?;
        let catalog = &self.heap.asic.hw.catalog;
        let header = (1 << catalog.constant("hsa", "HSA_PACKET_HEADER_BARRIER")?)
            | (catalog.constant("hsa", "HSA_FENCE_SCOPE_SYSTEM")?
                << catalog.constant("hsa", "HSA_PACKET_HEADER_SCACQUIRE_FENCE_SCOPE")?)
            | (catalog.constant("hsa", "HSA_FENCE_SCOPE_SYSTEM")?
                << catalog.constant("hsa", "HSA_PACKET_HEADER_SCRELEASE_FENCE_SCOPE")?)
            | (catalog.constant("hsa", "HSA_PACKET_TYPE_VENDOR_SPECIFIC")?
                << catalog.constant("hsa", "HSA_PACKET_HEADER_TYPE")?)
            | 1 << 16;
        let packet = |address: u64, count: usize| {
            let pair = crate::packets::words64(address);
            let mut words = vec![
                header as u32,
                crate::packets::packet3(0x3f, 2),
                pair[0],
                pair[1],
                count as u32 | 1 << 23,
                10,
            ];
            words.resize(16, 0);
            words
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>()
        };
        let mut bytes = packet(storage.address(), before.len());
        bytes.extend_from_slice(dispatch);
        bytes.extend_from_slice(&packet(
            storage.address() + (before.len() as u64 + 1) * 4,
            after.len(),
        ));
        let result = self.compute.aql(&mut self.heap, &bytes);
        if let Err(error) = &result {
            self.error = Some(error.to_string());
        }
        result
    }
}
