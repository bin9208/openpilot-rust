use super::{integer, Error};
use openpilot_can::Frame;

fn raw(address: u32, data: &[i32]) -> Result<Frame, Error> {
    Ok(Frame {
        address,
        bus: 1,
        data: data
            .iter()
            .map(|v| u8::try_from(*v).map_err(|_| Error::Numeric))
            .collect::<Result<Vec<_>, _>>()?,
    })
}
pub fn time_status(ticks: u64, counter: i32) -> Result<Frame, Error> {
    let mut data = vec![
        i32::try_from((ticks >> 20) & 255).map_err(|_| Error::Numeric)?,
        i32::try_from((ticks >> 12) & 255).map_err(|_| Error::Numeric)?,
        i32::try_from((ticks >> 4) & 255).map_err(|_| Error::Numeric)?,
        i32::try_from((ticks & 15) << 4).map_err(|_| Error::Numeric)? + (counter << 2),
    ];
    let checksum = (0x1000 - data.iter().sum::<i32>()) & 0xfff;
    data.extend([0x40 + (checksum >> 8), checksum & 255, 0x12]);
    raw(0xa1, &data)
}
pub fn steering_status(counter: i32) -> Result<Frame, Error> {
    let data = [counter << 6, 0xf0, 0x20, 0, 0, 0];
    let checksum = 0x60 + data.iter().sum::<i32>();
    raw(
        0x306,
        &[
            data[0],
            data[1],
            data[2],
            0,
            0,
            0,
            checksum >> 8,
            checksum & 255,
        ],
    )
}
pub fn speed_status(speed: f64, counter: i32) -> Result<Frame, Error> {
    let speed = integer(speed * 16.)? & 0xfff;
    let near = i32::from(speed <= 0x27);
    let far = 1 - near;
    let data = [8, speed >> 4, (speed & 15) << 4, 0, 0];
    let checksum = 0x62 + far + (counter << 2) + data.iter().sum::<i32>();
    raw(
        0x308,
        &[
            data[0],
            data[1],
            data[2],
            0,
            0,
            (counter << 5) + (far << 4) + (near << 3) + (checksum >> 8),
            checksum & 255,
        ],
    )
}
pub fn keepalive() -> [Frame; 2] {
    [
        Frame {
            address: 0x409,
            data: vec![0; 7],
            bus: 0,
        },
        Frame {
            address: 0x40a,
            data: vec![0; 7],
            bus: 0,
        },
    ]
}
