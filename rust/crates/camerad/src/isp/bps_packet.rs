use super::{bps, BufferRef, FrameBuffers, IspError, Program};
use crate::{
    nv12::Nv12Layout,
    packet::{u32_at, u64_at, CommandBuffer, IoConfig, Packet, Patch, Plane},
    sensor::SensorConfig,
};

pub struct BpsMemory {
    pub command: BufferRef,
    pub program: BufferRef,
    pub striping_command: BufferRef,
    pub settings: BufferRef,
    pub striping: BufferRef,
    pub gamma: BufferRef,
    pub linearization: BufferRef,
    pub full_resolution: BufferRef,
}

pub struct BpsConfig {
    pub width: u32,
    pub height: u32,
    pub device: i32,
}

pub struct BpsRequest {
    pub slot: u32,
    pub request: i32,
    pub generic_handle: i32,
}

pub struct BpsPacket {
    pub packet: Packet,
    pub command: [u8; 224],
    pub command_offset: u32,
    pub program: Program,
    pub generic: [u8; 36],
}

pub fn build(
    sensor: &SensorConfig,
    config: &BpsConfig,
    memory: &BpsMemory,
    buffers: FrameBuffers,
    update: BpsRequest,
) -> Result<BpsPacket, IspError> {
    let downscale = sensor.out_scale > 1;
    let mut packet = Packet::new(
        0x1000_0001,
        update.request as i64 as u64,
        2,
        if downscale { 3 } else { 2 },
        if downscale { 14 } else { 12 },
    )?;
    let program = bps::program(sensor)?;
    let command_offset = memory.command.offset(update.slot)?;
    let mut command = [0; 224];
    u32_at(&mut command, 164, memory.striping_command.size);
    u64_at(&mut command, 184, config.device as i64 as u64);
    u32_at(&mut command, 196, 1);
    u32_at(
        &mut command,
        204,
        ((program.bytes.len() as u32 - 1) & 0x000f_ffff) << 12,
    );
    u32_at(&mut command, 208, 20);
    packet.command(
        0,
        CommandBuffer {
            handle: memory.command.handle,
            offset: command_offset,
            size: 224,
            length: 224,
            kind: 8,
            metadata: 0,
        },
    )?;
    let mut generic = [0; 36];
    u32_at(&mut generic, 0, 0x2001);
    u64_at(&mut generic, 4, 0x01fca058);
    u32_at(
        &mut generic,
        12,
        sensor.frame_width.wrapping_mul(sensor.frame_height),
    );
    u64_at(&mut generic, 20, 0x38512180);
    u64_at(&mut generic, 28, 0x38512180);
    packet.command(
        1,
        CommandBuffer {
            handle: update.generic_handle,
            size: 36,
            length: 36,
            kind: 9,
            metadata: 1,
            ..CommandBuffer::default()
        },
    )?;
    packet.io(0, input(sensor, buffers))?;
    let output = output(
        config.width,
        config.height,
        buffers.yuv,
        buffers.bps_fence,
        if downscale { 7 } else { 1 },
    )?;
    let output_offset = output.offsets[1];
    packet.io(1, output)?;
    let full_offset = if downscale {
        let full = output_frame(sensor, memory, buffers)?;
        let offset = full.offsets[1];
        packet.io(2, full)?;
        offset
    } else {
        0
    };
    for (index, offset) in program.patches.iter().enumerate() {
        packet.patch(Patch {
            destination: memory.program.handle,
            destination_offset: *offset,
            source: if index == 0 {
                memory.linearization.handle
            } else {
                memory.gamma.handle
            },
            source_offset: 0,
        })?;
    }
    let mut patch = |offset: u32, source: i32, source_offset: u32| {
        packet.patch(Patch {
            destination: memory.command.handle,
            destination_offset: command_offset
                .checked_add(offset)
                .ok_or(crate::packet::PacketError::Size)?,
            source,
            source_offset,
        })
    };
    patch(0, buffers.raw, 0)?;
    if downscale {
        patch(16, memory.full_resolution.handle, 0)?;
        patch(20, memory.full_resolution.handle, full_offset)?;
        patch(112, buffers.yuv, 0)?;
        patch(116, buffers.yuv, output_offset)?;
    } else {
        patch(16, buffers.yuv, 0)?;
        patch(20, buffers.yuv, output_offset)?;
    }
    patch(168, memory.settings.handle, 0)?;
    patch(176, memory.command.handle, 192)?;
    patch(200, memory.program.handle, 0)?;
    patch(172, memory.striping.handle, 0)?;
    patch(160, memory.striping_command.handle, 0)?;
    Ok(BpsPacket {
        packet,
        command,
        command_offset,
        program,
        generic,
    })
}

fn input(sensor: &SensorConfig, buffers: FrameBuffers) -> IoConfig {
    let height = sensor.frame_height + sensor.extra_height;
    IoConfig {
        handles: [buffers.raw, 0, 0],
        planes: [
            Plane {
                width: sensor.frame_width,
                height,
                stride: sensor.frame_stride,
                slice_height: height,
            },
            Plane::default(),
            Plane::default(),
        ],
        format: sensor.mipi_format,
        color_pattern: 5,
        bpp: if sensor.mipi_format == 3 { 10 } else { 12 },
        fence: buffers.ife_fence,
        direction: 1,
        subsample_pattern: 1,
        framedrop_pattern: 1,
        ..IoConfig::default()
    }
}

fn output(
    width: u32,
    height: u32,
    handle: i32,
    fence: i32,
    resource: u32,
) -> Result<IoConfig, IspError> {
    let layout = Nv12Layout::new(width, height)?;
    let uv = layout
        .stride
        .checked_mul(layout.y_height)
        .and_then(|size| size.checked_add(4095))
        .ok_or(IspError::Offset)?
        & !4095;
    Ok(IoConfig {
        handles: [handle, handle, 0],
        offsets: [0, uv, 0],
        planes: [
            Plane {
                width,
                height,
                stride: layout.stride,
                slice_height: layout.y_height,
            },
            Plane {
                width,
                height: height / 2,
                stride: layout.stride,
                slice_height: layout.uv_height,
            },
            Plane::default(),
        ],
        format: 32,
        color_space: 1,
        resource,
        fence,
        direction: 2,
        subsample_pattern: 1,
        framedrop_pattern: 1,
        ..IoConfig::default()
    })
}

fn output_frame(
    sensor: &SensorConfig,
    memory: &BpsMemory,
    buffers: FrameBuffers,
) -> Result<IoConfig, IspError> {
    output(
        sensor.frame_width,
        sensor.frame_height,
        memory.full_resolution.handle,
        buffers.bps_fence,
        1,
    )
}
