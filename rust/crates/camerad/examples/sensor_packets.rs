use openpilot_camerad::{
    isp::{
        acquire, bps,
        bps_packet::{self, BpsConfig, BpsMemory, BpsRequest},
        ife_packet::{self, IfeConfig, IfeMemory, IfeRequest},
        BufferRef, FrameBuffers,
    },
    packet,
    sensor::{Register, SensorKind},
    sensor_packets,
};
use serde::Deserialize;
use serde_json::json;
use std::{
    error::Error,
    io::{self, BufRead, Write},
};

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Input {
    Probe {
        sensor: u8,
        port: usize,
    },
    Poke {
        request: i32,
    },
    I2c {
        words: bool,
        opcode: u32,
        registers: Vec<[u32; 2]>,
    },
    Csiphy,
    Layout,
    Acquire {
        sensor: u8,
        raw: bool,
        phy: u32,
        width: u32,
        height: u32,
        handle: i32,
        size: u32,
    },
    BpsTables {
        sensor: u8,
    },
    Bps {
        sensor: u8,
        slot: u32,
        request: i32,
        width: u32,
        height: u32,
    },
    Ife {
        sensor: u8,
        raw: bool,
        vignetting: bool,
        slot: u32,
        request: i32,
        initial: bool,
        width: u32,
        height: u32,
    },
}

fn sensor_kind(sensor: u8) -> Result<SensorKind, Box<dyn Error>> {
    match sensor {
        1 => Ok(SensorKind::Ar0231),
        2 => Ok(SensorKind::Ox03c10),
        3 => Ok(SensorKind::Os04c10),
        _ => Err("unknown sensor".into()),
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut output = io::BufWriter::new(io::stdout().lock());
    for line in io::stdin().lock().lines() {
        let value = match serde_json::from_str::<Input>(&line?)? {
            Input::Probe { sensor, port } => {
                let sensor = sensor_kind(sensor)?;
                let probe = sensor_packets::probe(sensor, port, 2, 3)?;
                json!([
                    probe.packet.bytes(),
                    probe.info.as_slice(),
                    probe.power.as_slice()
                ])
            }
            Input::Poke { request } => json!([sensor_packets::poke(request)?.bytes()]),
            Input::I2c {
                words,
                opcode,
                registers,
            } => {
                let registers: Vec<_> = registers
                    .into_iter()
                    .map(|[address, value]| Register(address, value))
                    .collect();
                let (packet, data) = sensor_packets::i2c(&registers, opcode, words, 2)?;
                json!([packet.bytes(), data])
            }
            Input::Csiphy => {
                let (packet, data) = sensor_packets::csiphy(2)?;
                json!([packet.bytes(), data])
            }
            Input::Layout => json!([
                packet::PACKET_SIZE,
                packet::PAYLOAD_OFFSET,
                packet::COMMAND_SIZE,
                packet::IO_SIZE,
                packet::PATCH_SIZE
            ]),
            Input::Acquire {
                sensor,
                raw,
                phy,
                width,
                height,
                handle,
                size,
            } => {
                let sensor = sensor_kind(sensor)?.config();
                json!([
                    acquire::ife_port(sensor, phy, raw, width, height).as_slice(),
                    acquire::bps_resource(sensor, handle, size, width, height).as_slice()
                ])
            }
            Input::BpsTables { sensor } => {
                let sensor = sensor_kind(sensor)?;
                let tables = bps::lookup_tables(sensor);
                let linear: Vec<_> = bps::linearization(sensor.config())
                    .into_iter()
                    .flat_map(u32::to_le_bytes)
                    .collect();
                json!([
                    tables.config.as_slice(),
                    tables.settings.as_slice(),
                    tables.striping.as_slice(),
                    linear
                ])
            }
            Input::Bps {
                sensor,
                slot,
                request,
                width,
                height,
            } => {
                let buffer = |handle, size: u32| BufferRef {
                    handle,
                    size,
                    aligned_size: (size + 31) & !31,
                };
                let config = BpsConfig {
                    width,
                    height,
                    device: 17,
                };
                let memory = BpsMemory {
                    command: buffer(110, 464),
                    program: buffer(111, 0x1000),
                    striping_command: buffer(112, 0xcfe0),
                    settings: buffer(113, 684),
                    striping: buffer(114, 0xc58),
                    gamma: buffer(115, 256),
                    linearization: buffer(116, 144),
                    full_resolution: buffer(117, 1),
                };
                let buffers = FrameBuffers {
                    raw: 200 + slot as i32,
                    yuv: 300 + slot as i32,
                    ife_fence: 400 + slot as i32,
                    bps_fence: 500 + slot as i32,
                };
                let update = BpsRequest {
                    slot,
                    request,
                    generic_handle: 2,
                };
                let built = bps_packet::build(
                    sensor_kind(sensor)?.config(),
                    &config,
                    &memory,
                    buffers,
                    update,
                )?;
                json!([
                    built.packet.bytes(),
                    built.generic.as_slice(),
                    built.command.as_slice(),
                    built.program.bytes
                ])
            }
            Input::Ife {
                sensor,
                raw,
                vignetting,
                slot,
                request,
                initial,
                width,
                height,
            } => {
                let config = IfeConfig {
                    raw,
                    vignetting,
                    width,
                    height,
                };
                let memory = IfeMemory {
                    command: BufferRef {
                        handle: 100,
                        size: 67984,
                        aligned_size: 68000,
                    },
                    gamma: BufferRef {
                        handle: 101,
                        size: 256,
                        aligned_size: 256,
                    },
                    linearization: BufferRef {
                        handle: 102,
                        size: 144,
                        aligned_size: 160,
                    },
                    vignetting: BufferRef {
                        handle: 103,
                        size: 884,
                        aligned_size: 896,
                    },
                };
                let buffers = FrameBuffers {
                    raw: 200 + slot as i32,
                    yuv: 300 + slot as i32,
                    ife_fence: 400 + slot as i32,
                    bps_fence: 0,
                };
                let update = IfeRequest {
                    slot,
                    request,
                    initial,
                    generic_handle: 2,
                };
                let built = ife_packet::build(
                    sensor_kind(sensor)?.config(),
                    &config,
                    &memory,
                    buffers,
                    update,
                )?;
                json!([
                    built.packet.bytes(),
                    built.generic.as_slice(),
                    built.program.bytes
                ])
            }
        };
        serde_json::to_writer(&mut output, &value)?;
        writeln!(output)?;
    }
    output.flush()?;
    Ok(())
}
