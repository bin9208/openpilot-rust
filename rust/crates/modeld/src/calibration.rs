use crate::numpy_trig::trig;

type Matrix = [f64; 9];

#[derive(Clone, Copy)]
pub struct CalibrationUpdate<'a> {
    pub updated: bool,
    pub road_seen: bool,
    pub device_seen: bool,
    pub rpy: [f32; 3],
    pub calibrated: bool,
    pub yaw_trim_degrees: f64,
    pub device: &'a str,
    pub sensor: &'a str,
    pub main_wide: bool,
    pub use_extra: bool,
}

#[derive(Debug, thiserror::Error)]
#[error("unknown device/camera calibration")]
pub struct UnknownCamera;

#[derive(Default)]
pub struct DrivingCalibration {
    main: [f32; 9],
    extra: [f32; 9],
    seen: bool,
}

impl DrivingCalibration {
    pub fn main(&self) -> [f32; 9] {
        self.main
    }
    pub fn extra(&self) -> [f32; 9] {
        self.extra
    }
    pub fn seen(&self) -> bool {
        self.seen
    }

    pub fn update(&mut self, input: CalibrationUpdate<'_>) -> Result<bool, UnknownCamera> {
        if !input.updated || !input.road_seen || !input.device_seen {
            return Ok(false);
        }
        let (front, wide) = cameras(input.device, input.sensor)?;
        let mut rpy = input.rpy;
        let trim = if input.calibrated {
            input.yaw_trim_degrees
        } else {
            0.0
        };
        if trim != 0.0 {
            rpy[2] = (f64::from(rpy[2]) - trim.to_radians()) as f32;
        }
        self.main = warp(rpy, if input.main_wide { wide } else { front }, false);
        self.extra = warp(
            rpy,
            if input.use_extra || input.main_wide {
                wide
            } else {
                front
            },
            true,
        );
        self.seen = true;
        Ok(true)
    }
}

fn cameras(device: &str, sensor: &str) -> Result<([f64; 3], [f64; 3]), UnknownCamera> {
    let ar = ([1928.0, 1208.0, 2648.0], [1928.0, 1208.0, 567.0]);
    match (device, sensor) {
        ("neo", "unknown") => Ok(([1164.0, 874.0, 910.0], [0.0; 3])),
        ("tici" | "pc", "unknown") | ("unknown", "ar0231" | "ox03c10") => Ok(ar),
        ("tici" | "tizi" | "mici", "ar0231" | "ox03c10") => Ok(ar),
        ("tici" | "tizi" | "mici", "os04c10") => {
            Ok(([1344.0, 760.0, 1141.5], [1344.0, 760.0, 425.25]))
        }
        _ => Err(UnknownCamera),
    }
}

fn multiply(left: Matrix, right: Matrix) -> Matrix {
    std::array::from_fn(|index| {
        let row = (index / 3) * 3;
        let col = index % 3;
        (left[row] * right[col] + left[row + 1] * right[col + 3]) + left[row + 2] * right[col + 6]
    })
}

fn warp(rpy: [f32; 3], camera: [f64; 3], big: bool) -> [f32; 9] {
    let [sx, sy, sz] = rpy.map(|value| f64::from(trig(value, false)));
    let [cx, cy, cz] = rpy.map(|value| f64::from(trig(value, true)));
    let roll = [1.0, 0.0, 0.0, 0.0, cx, -sx, 0.0, sx, cx];
    let pitch = [cy, 0.0, sy, 0.0, 1.0, 0.0, -sy, 0.0, cy];
    let yaw = [cz, -sz, 0.0, sz, cz, 0.0, 0.0, 0.0, 1.0];
    let rotation = multiply(multiply(yaw, pitch), roll);
    let intrinsics = [
        camera[2],
        0.0,
        camera[0] / 2.0,
        0.0,
        camera[2],
        camera[1] / 2.0,
        0.0,
        0.0,
        1.0,
    ];
    let view = [0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0];
    let camera_from_calib = multiply(multiply(intrinsics, view), rotation);
    // Preserve float64 residuals from the original NumPy inverse before the final float32 cast.
    let inverse: [u64; 9] = if big {
        [
            0,
            0,
            0x3ff0_0000_0000_0000,
            0x3f62_0120_1201_2012,
            0x8000_0000_0000_0000,
            0xbfe2_0120_1201_2012,
            0xbbf3_aa69_5ee7_da03,
            0x3f62_0120_1201_2012,
            0xbfd5_5a22_6ef3_bc09,
        ]
    } else {
        [
            0,
            0x3c55_833a_1583_3a16,
            0x3ff0_0000_0000_0000,
            0x3f52_0120_1201_2012,
            0xbc38_3524_aa7e_493d,
            0xbfd2_0120_1201_2012,
            0xbbda_9e4d_bbe5_fbcf,
            0x3f52_0120_1201_2011,
            0xbfaa_c812_e794_dfb3,
        ]
    };
    multiply(camera_from_calib, inverse.map(f64::from_bits)).map(|value| value as f32)
}
