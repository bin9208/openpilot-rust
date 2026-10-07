use crate::{context::Context, state::messages, Error};
use openpilot_cereal::log_capnp::{
    frame_data::ImageSensor, init_data::DeviceType, live_calibration_data::Status,
};
use openpilot_msgq::VisionStream;
use openpilot_ui_framework::geometry::Rect;
pub type Matrix = [[f64; 3]; 3];
const VIEW: Matrix = [[0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]];
#[derive(Clone, Copy)]
pub struct Intrinsic {
    pub width: f64,
    pub height: f64,
    pub focal: f64,
}
impl Intrinsic {
    fn matrix(self) -> Matrix {
        [
            [self.focal, 0.0, self.width / 2.0],
            [0.0, self.focal, self.height / 2.0],
            [0.0, 0.0, 1.0],
        ]
    }
}
pub fn multiply(a: Matrix, b: Matrix) -> Matrix {
    std::array::from_fn(|row| {
        std::array::from_fn(|col| {
            a[row][0] * b[0][col] + a[row][1] * b[1][col] + a[row][2] * b[2][col]
        })
    })
}
pub fn rotation([roll, pitch, yaw]: [f64; 3]) -> Matrix {
    let (sr, cr) = roll.sin_cos();
    let (sp, cp) = pitch.sin_cos();
    let (sy, cy) = yaw.sin_cos();
    let rx = [[1.0, 0.0, 0.0], [0.0, cr, -sr], [0.0, sr, cr]];
    let ry = [[cp, 0.0, sp], [0.0, 1.0, 0.0], [-sp, 0.0, cp]];
    let rz = [[cy, -sy, 0.0], [sy, cy, 0.0], [0.0, 0.0, 1.0]];
    multiply(multiply(rz, ry), rx)
}
type CachedMatrices = ((i64, f32, f32, VisionStream), (Matrix, Matrix));
pub struct Calibration {
    cameras: Option<[Intrinsic; 2]>,
    view: [Matrix; 2],
    device_position: String,
    calibration_frame: i64,
    cached: Option<CachedMatrices>,
}
impl Default for Calibration {
    fn default() -> Self {
        Self {
            cameras: None,
            view: [VIEW; 2],
            device_position: String::new(),
            calibration_frame: 0,
            cached: None,
        }
    }
}
impl Calibration {
    pub fn update(&mut self, context: &Context) -> Result<(), Error> {
        let sm = context.messages.borrow();
        if self.cameras.is_none()
            && sm.state.topic("roadCameraState")?.seen
            && sm.state.topic("deviceState")?.seen
        {
            let event = sm.state.topic("roadCameraState")?.event()?;
            let frame = match event.which()? {
                openpilot_cereal::log_capnp::event::Which::RoadCameraState(value) => value?,
                _ => return Err(Error::Contract("road calibration frame")),
            };
            let device = messages::device_state(&sm.state)?.get_device_type()?;
            let sensor = frame.get_sensor()?;
            let ar = [
                Intrinsic {
                    width: 1928.0,
                    height: 1208.0,
                    focal: 2648.0,
                },
                Intrinsic {
                    width: 1928.0,
                    height: 1208.0,
                    focal: 567.0,
                },
            ];
            self.cameras = Some(match (device, sensor) {
                (DeviceType::Neo, ImageSensor::Unknown) => [
                    Intrinsic {
                        width: 1164.0,
                        height: 874.0,
                        focal: 910.0,
                    },
                    Intrinsic {
                        width: 0.0,
                        height: 0.0,
                        focal: 0.0,
                    },
                ],
                (DeviceType::Tici | DeviceType::Pc, ImageSensor::Unknown)
                | (DeviceType::Unknown, ImageSensor::Ar0231 | ImageSensor::Ox03c10)
                | (
                    DeviceType::Tici | DeviceType::Tizi | DeviceType::Mici,
                    ImageSensor::Ar0231 | ImageSensor::Ox03c10,
                ) => ar,
                (DeviceType::Tici | DeviceType::Tizi | DeviceType::Mici, ImageSensor::Os04c10) => [
                    Intrinsic {
                        width: 1344.0,
                        height: 760.0,
                        focal: 1522.0 * 3.0 / 4.0,
                    },
                    Intrinsic {
                        width: 1344.0,
                        height: 760.0,
                        focal: 567.0 / 4.0 * 3.0,
                    },
                ],
                _ => return Err(Error::Contract("unknown device/camera calibration pair")),
            });
        }
        let topic = sm.state.topic("liveCalibration")?;
        self.calibration_frame = topic.receive_frame;
        if !(topic.updated && topic.valid) {
            return Ok(());
        }
        let event = topic.event()?;
        let calibration = match event.which()? {
            openpilot_cereal::log_capnp::event::Which::LiveCalibration(value) => value?,
            _ => return Err(Error::Contract("live calibration event")),
        };
        let rpy = calibration.get_rpy_calib()?;
        if rpy.len() != 3 || calibration.get_cal_status()? != Status::Calibrated {
            return Ok(());
        }
        let rpy = [rpy.get(0), rpy.get(1), rpy.get(2)].map(f64::from);
        if !context.big {
            let pitch = rpy[1].to_degrees();
            let yaw = rpy[2].to_degrees();
            let text = format!(
                "{:.1}° {} {:.1}° {}",
                pitch.abs(),
                if pitch > 0.0 { "v" } else { "^" },
                yaw.abs(),
                if yaw > 0.0 { "<" } else { ">" }
            );
            if text != self.device_position {
                context.params.put("DevicePosition", text.as_bytes())?;
                self.device_position = text;
            }
        }
        let device_from_calib = rotation(rpy);
        self.view[0] = multiply(VIEW, device_from_calib);
        let wide = calibration.get_wide_from_device_euler()?;
        if wide.len() == 3 {
            self.view[1] = multiply(
                multiply(
                    VIEW,
                    rotation([wide.get(0), wide.get(1), wide.get(2)].map(f64::from)),
                ),
                device_from_calib,
            );
        }
        Ok(())
    }
    pub fn matrices(
        &mut self,
        rect: Rect,
        stream: VisionStream,
        speed: f64,
        big: bool,
    ) -> Result<(Matrix, Matrix), Error> {
        use super::model_renderer::math;
        let key = (self.calibration_frame, rect.width, rect.height, stream);
        if big {
            if let Some((previous, matrices)) = self.cached {
                if previous == key {
                    return Ok(matrices);
                }
            }
        }
        let wide = stream == VisionStream::WideRoad;
        let index = usize::from(wide);
        let intrinsics = self.cameras.unwrap_or([
            Intrinsic {
                width: 1928.0,
                height: 1208.0,
                focal: 2648.0,
            },
            Intrinsic {
                width: 1928.0,
                height: 1208.0,
                focal: 567.0,
            },
        ])[index];
        let intrinsic = intrinsics.matrix();
        let zoom = if big {
            if wide {
                2.0
            } else {
                1.1
            }
        } else if wide {
            0.7 * 1.5
        } else {
            math::interp(speed, &[10.0, 30.0], &[0.8, 1.0])?
        };
        let calib = multiply(intrinsic, self.view[index]);
        let kep = calib.map(|row| row[0] * 1000.0);
        let [x, y, width, height] = [rect.x, rect.y, rect.width, rect.height].map(f64::from);
        let cx = intrinsic[0][2];
        let cy = intrinsic[1][2];
        let max_x = cx * zoom - width / 2.0 - 5.0;
        let max_y = cy * zoom - height / 2.0 - 5.0;
        let [offset_x, offset_y] = if kep[2].abs() > 1e-6 {
            [
                math::clip((kep[0] / kep[2] - cx) * zoom, -max_x, max_x),
                math::clip(
                    (kep[1] / kep[2] - cy) * zoom + if big { 0.0 } else { 20.0 },
                    -max_y,
                    max_y,
                ),
            ]
        } else {
            [0.0, 0.0]
        };
        let frame = [
            [zoom * 2.0 * cx / width, 0.0, -offset_x / width * 2.0],
            [0.0, zoom * 2.0 * cy / height, -offset_y / height * 2.0],
            [0.0, 0.0, 1.0],
        ];
        let video = [
            [zoom, 0.0, width / 2.0 + x - offset_x - cx * zoom],
            [0.0, zoom, height / 2.0 + y - offset_y - cy * zoom],
            [0.0, 0.0, 1.0],
        ];
        let result = (frame, multiply(video, calib));
        if big {
            self.cached = Some((key, result));
        }
        Ok(result)
    }
}
