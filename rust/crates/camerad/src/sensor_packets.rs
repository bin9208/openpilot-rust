use crate::{
    packet::{u16_at, u32_at, u64_at, CommandBuffer, Packet, PacketError},
    sensor::{Register, SensorKind},
};

pub struct Probe {
    pub packet: Packet,
    pub info: [u8; 24],
    pub power: [u8; 196],
}

pub fn probe(
    sensor: SensorKind,
    port: usize,
    info_handle: i32,
    power_handle: i32,
) -> Result<Probe, PacketError> {
    let config = sensor.config();
    let mut packet = Packet::new(0x0100_0003, 0, 2, 0, 0)?;
    packet.command(
        0,
        CommandBuffer {
            handle: info_handle,
            size: 24,
            length: 24,
            kind: 10,
            ..CommandBuffer::default()
        },
    )?;
    packet.command(
        1,
        CommandBuffer {
            handle: power_handle,
            size: 196,
            length: 196,
            kind: 7,
            ..CommandBuffer::default()
        },
    )?;
    let mut info = [0; 24];
    u16_at(&mut info, 0, sensor.slave_address(port)? as u16);
    info[2..8].copy_from_slice(&[1, 4, 2, 2, 3, 1]);
    u32_at(&mut info, 8, config.probe_reg_addr);
    u32_at(&mut info, 12, config.probe_expected_data);
    u16_at(&mut info, 20, port as u16);
    let mut power = [0; 196];
    let mut offset = 0;
    for (kind, settings, wait) in [
        (2, &[(3, 0), (1, 0), (2, 0), (8, 0)][..], Some(1)),
        (2, &[(0, config.mclk_frequency)][..], Some(1)),
        (2, &[(8, 1)][..], Some(34)),
        (3, &[(0, 0)][..], Some(1)),
        (3, &[(8, 1)][..], Some(1)),
        (3, &[(8, 0)][..], Some(1)),
        (3, &[(2, 0), (1, 0), (3, 0)][..], None),
    ] {
        u16_at(&mut power, offset, settings.len() as u16);
        power[offset + 3] = kind;
        offset += 4;
        for (sequence, value) in settings {
            u16_at(&mut power, offset, *sequence);
            u32_at(&mut power, offset + 4, *value);
            offset += 12;
        }
        if let Some(delay) = wait {
            u16_at(&mut power, offset, delay);
            power[offset + 2..offset + 4].copy_from_slice(&[3, 9]);
            offset += 4;
        }
    }
    Ok(Probe {
        packet,
        info,
        power,
    })
}

pub fn poke(request: i32) -> Result<Packet, PacketError> {
    Packet::new(127, request as i64 as u64, 0, 0, 0)
}

pub fn i2c(
    registers: &[Register],
    opcode: u32,
    words: bool,
    handle: i32,
) -> Result<(Packet, Vec<u8>), PacketError> {
    let size = registers
        .len()
        .checked_mul(8)
        .and_then(|size| size.checked_add(8))
        .filter(|size| *size <= u32::MAX as usize)
        .ok_or(PacketError::Size)?;
    let mut data = vec![0; size];
    u16_at(&mut data, 0, registers.len() as u16);
    data[2..6].copy_from_slice(&[1, 5, if words { 2 } else { 1 }, 2]);
    for (index, Register(address, value)) in registers.iter().enumerate() {
        u32_at(&mut data, 8 + index * 8, *address);
        u32_at(&mut data, 12 + index * 8, *value);
    }
    let mut packet = Packet::new(opcode, 0, 1, 0, 0)?;
    packet.command(
        0,
        CommandBuffer {
            handle,
            size: size as u32,
            length: size as u32,
            kind: 7,
            ..CommandBuffer::default()
        },
    )?;
    Ok((packet, data))
}

pub fn csiphy(handle: i32) -> Result<(Packet, [u8; 24]), PacketError> {
    let mut data = [0; 24];
    u16_at(&mut data, 0, 0x1f);
    u16_at(&mut data, 2, 0x3210);
    data[6] = 4;
    u64_at(&mut data, 8, 33 * 200_000_000);
    u64_at(&mut data, 16, 48_000_000);
    let mut packet = Packet::new(0, 0, 1, 0, 0)?;
    packet.command(
        0,
        CommandBuffer {
            handle,
            size: 24,
            length: 24,
            kind: 9,
            ..CommandBuffer::default()
        },
    )?;
    Ok((packet, data))
}
