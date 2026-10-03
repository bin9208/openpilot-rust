use super::{ife, BufferRef, FrameBuffers, IspError, Program};
use crate::{
    nv12::Nv12Layout,
    packet::{u32_at, u64_at, CommandBuffer, IoConfig, Packet, Patch, Plane},
    sensor::SensorConfig,
};

pub struct IfeMemory {
    pub command: BufferRef,
    pub gamma: BufferRef,
    pub linearization: BufferRef,
    pub vignetting: BufferRef,
}

pub struct IfeConfig {
    pub raw: bool,
    pub vignetting: bool,
    pub width: u32,
    pub height: u32,
}

pub struct IfePacket {
    pub packet: Packet,
    pub program: Program,
    pub command_offset: u32,
    pub generic: [u8; 324],
}

pub struct IfeRequest {
    pub slot: u32,
    pub request: i32,
    pub initial: bool,
    pub generic_handle: i32,
}

pub fn build(
    sensor: &SensorConfig,
    config: &IfeConfig,
    memory: &IfeMemory,
    buffers: FrameBuffers,
    update: IfeRequest,
) -> Result<IfePacket, IspError> {
    let IfeRequest {
        slot,
        request,
        initial,
        generic_handle,
    } = update;
    let mut packet = Packet::new(
        0x0f00_0000 | u32::from(!initial),
        if initial { 1 } else { request as i64 as u64 },
        2,
        usize::from(!initial),
        10,
    )?;
    let program = if config.raw {
        Program::default()
    } else if initial {
        ife::initial(sensor, config.vignetting, config.width, config.height)?
    } else {
        ife::update(sensor, config.vignetting)?
    };
    let command_offset = memory.command.offset(slot)?;
    let length = u32::try_from(program.bytes.len()).map_err(|_| IspError::Offset)?;
    if length > memory.command.size {
        return Err(IspError::Offset);
    }
    packet.command(
        0,
        CommandBuffer {
            handle: memory.command.handle,
            offset: command_offset,
            size: memory.command.size,
            length,
            kind: 5,
            metadata: 3,
        },
    )?;
    packet.kmd(0, length);
    let generic = generic(config.raw);
    let offset = if initial { 0 } else { 0x60 };
    packet.command(
        1,
        CommandBuffer {
            handle: generic_handle,
            offset,
            size: 324,
            length: 324 - offset,
            kind: 9,
            metadata: 12,
        },
    )?;
    if !initial {
        packet.io(0, output(sensor, config, buffers)?)?;
    }
    for (index, offset) in program.patches.iter().enumerate() {
        let (source, source_offset) = match index {
            0 => (memory.linearization.handle, 0),
            1 | 2 => (
                memory.vignetting.handle,
                memory.vignetting.size * (index as u32 - 1),
            ),
            _ => (memory.gamma.handle, memory.gamma.size * (index as u32 - 3)),
        };
        packet.patch(Patch {
            destination: memory.command.handle,
            destination_offset: *offset,
            source,
            source_offset,
        })?;
    }
    Ok(IfePacket {
        packet,
        program,
        command_offset,
        generic,
    })
}

fn output(
    sensor: &SensorConfig,
    config: &IfeConfig,
    buffers: FrameBuffers,
) -> Result<IoConfig, IspError> {
    let mut io = IoConfig {
        fence: buffers.ife_fence,
        direction: 2,
        subsample_pattern: 1,
        framedrop_pattern: 1,
        ..IoConfig::default()
    };
    if config.raw {
        io.handles[0] = buffers.raw;
        io.planes[0] = Plane {
            width: sensor.frame_width,
            height: sensor.frame_height,
            stride: sensor.frame_stride,
            slice_height: sensor.frame_height + sensor.extra_height,
        };
        io.format = sensor.mipi_format;
        io.color_pattern = 5;
        io.bpp = if sensor.mipi_format == 3 { 10 } else { 12 };
        io.resource = 0x3006;
    } else {
        let layout = Nv12Layout::new(config.width, config.height)?;
        io.handles = [buffers.yuv, buffers.yuv, 0];
        io.planes[0] = Plane {
            width: config.width,
            height: config.height,
            stride: layout.stride,
            slice_height: layout.y_height,
        };
        io.planes[1] = Plane {
            width: config.width,
            height: config.height / 2,
            stride: layout.stride,
            slice_height: layout.uv_height,
        };
        io.offsets[1] = layout.stride * layout.y_height;
        io.format = 32;
        io.resource = 0x3000;
    }
    Ok(io)
}

fn generic(raw: bool) -> [u8; 324] {
    let mut data = [0; 324];
    for (offset, value) in [
        (0, 0x2000),
        (4, 1),
        (12, if raw { 0x3006 } else { 0x3000 }),
        (16, 1),
        (24, 1),
        (36, 0x3801),
        (40, 1),
        (44, 4),
        (96, 0xe002),
        (100, 1),
        (104, 4),
    ] {
        u32_at(&mut data, offset, value);
    }
    for offset in [48, 56, 64] {
        u64_at(&mut data, offset, 404_000_000);
    }
    for offset in [116, 124] {
        u64_at(&mut data, offset, 450_000_000);
    }
    for offset in [164, 172] {
        u64_at(&mut data, offset, 8_706_200_000);
    }
    data
}
