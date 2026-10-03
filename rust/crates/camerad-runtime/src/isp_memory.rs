use openpilot_camera_kernel::{Allocation, AllocationOptions, Device, Master};
use openpilot_camerad::{
    isp::{bps, bps_packet::BpsMemory, ife_packet::IfeMemory, BufferRef},
    nv12::Nv12Layout,
    sensor::SensorKind,
};

use crate::isp_port::IspPortError;

pub(crate) struct CommandMemory<'device> {
    allocation: Allocation<'device>,
    descriptor: BufferRef,
}

impl<'device> CommandMemory<'device> {
    fn new(
        device: &'device Device,
        size: u32,
        alignment: u32,
        shared: bool,
        mmu: [i32; 2],
        count: u32,
    ) -> Result<Self, IspPortError> {
        let aligned_size =
            size.checked_add(alignment - 1).ok_or(IspPortError::Size)? & !(alignment - 1);
        let length = aligned_size.checked_mul(count).ok_or(IspPortError::Size)? as usize;
        let allocation = device.allocate(AllocationOptions {
            length,
            alignment,
            flags: if shared { 0x859 } else { 0x59 },
            mmu,
        })?;
        let descriptor = BufferRef {
            handle: allocation.handle() as i32,
            size,
            aligned_size,
        };
        Ok(Self {
            allocation,
            descriptor,
        })
    }

    pub(crate) fn reference(&self) -> BufferRef {
        self.descriptor
    }

    pub(crate) fn write(&mut self, offset: u32, data: &[u8]) -> Result<(), IspPortError> {
        Ok(self.allocation.write(offset as usize, data)?)
    }

    fn words(&mut self, offset: u32, values: &[u32]) -> Result<(), IspPortError> {
        let bytes: Vec<u8> = values
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect();
        self.write(offset, &bytes)
    }
}

fn reference(memory: Option<&CommandMemory<'_>>) -> BufferRef {
    memory.map_or(
        BufferRef {
            handle: 0,
            size: 0,
            aligned_size: 0,
        },
        CommandMemory::reference,
    )
}

pub(crate) struct IfeBuffers<'device> {
    vignetting: Option<CommandMemory<'device>>,
    linearization: Option<CommandMemory<'device>>,
    gamma: Option<CommandMemory<'device>>,
    pub(crate) command: CommandMemory<'device>,
}

impl<'device> IfeBuffers<'device> {
    pub(crate) fn new(
        master: &'device Master,
        kind: SensorKind,
        processed: bool,
        depth: u32,
    ) -> Result<Self, IspPortError> {
        let mmu = [master.mmu.device, master.mmu.cdm];
        let allocate =
            |size, count| CommandMemory::new(&master.request, size, 32, false, mmu, count);
        let command = allocate(67984, depth)?;
        let mut output = Self {
            command,
            gamma: None,
            linearization: None,
            vignetting: None,
        };
        if processed {
            let sensor = kind.config();
            let mut gamma = allocate(256, 3)?;
            for index in 0..3 {
                gamma.words(index * 256, sensor.gamma_lut_rgb)?;
            }
            output.gamma = Some(gamma);
            let mut linearization = allocate(144, 1)?;
            linearization.words(0, sensor.linearization_lut)?;
            output.linearization = Some(linearization);
            let mut vignetting = allocate(884, 2)?;
            for index in 0..2 {
                vignetting.words(index * 884, sensor.vignetting_lut)?;
            }
            output.vignetting = Some(vignetting);
        }
        Ok(output)
    }

    pub(crate) fn references(&self) -> IfeMemory {
        IfeMemory {
            command: self.command.reference(),
            gamma: reference(self.gamma.as_ref()),
            linearization: reference(self.linearization.as_ref()),
            vignetting: reference(self.vignetting.as_ref()),
        }
    }
}

pub(crate) struct BpsBuffers<'device> {
    full_resolution: Option<CommandMemory<'device>>,
    gamma: CommandMemory<'device>,
    linearization: CommandMemory<'device>,
    striping: CommandMemory<'device>,
    settings: CommandMemory<'device>,
    striping_command: CommandMemory<'device>,
    pub(crate) program: CommandMemory<'device>,
    pub(crate) command: CommandMemory<'device>,
}

impl<'device> BpsBuffers<'device> {
    pub(crate) fn new(
        master: &'device Master,
        kind: SensorKind,
        depth: u32,
    ) -> Result<Self, IspPortError> {
        let allocate = |size, alignment, count| {
            CommandMemory::new(
                &master.request,
                size,
                alignment,
                true,
                [master.mmu.icp, 0],
                count,
            )
        };
        let sensor = kind.config();
        let tables = bps::lookup_tables(kind);
        let command = allocate(464, 32, depth)?;
        let mut settings = allocate(684, 32, 1)?;
        settings.write(0, tables.settings)?;
        let program = allocate(4096, 32, 1)?;
        let mut striping = allocate(3160, 32, 1)?;
        striping.write(0, tables.striping)?;
        let striping_command = allocate(0xcfe0, 32, 1)?;
        let full_resolution = if sensor.out_scale > 1 {
            Some(allocate(
                Nv12Layout::new(sensor.frame_width, sensor.frame_height)?.size,
                4096,
                1,
            )?)
        } else {
            None
        };
        let mut linearization = allocate(144, 32, 1)?;
        linearization.words(0, &bps::linearization(sensor))?;
        let mut gamma = allocate(256, 32, 1)?;
        gamma.words(0, sensor.gamma_lut_rgb)?;
        Ok(Self {
            full_resolution,
            gamma,
            linearization,
            striping,
            settings,
            striping_command,
            program,
            command,
        })
    }

    pub(crate) fn references(&self) -> BpsMemory {
        BpsMemory {
            command: self.command.reference(),
            program: self.program.reference(),
            striping_command: self.striping_command.reference(),
            settings: self.settings.reference(),
            striping: self.striping.reference(),
            gamma: self.gamma.reference(),
            linearization: self.linearization.reference(),
            full_resolution: reference(self.full_resolution.as_ref()),
        }
    }
}
