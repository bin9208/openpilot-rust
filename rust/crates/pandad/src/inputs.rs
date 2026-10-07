use crate::peripheral::{CameraSample, Input};
use openpilot_cereal::log_capnp::event;

pub struct Inputs {
    frame: u64,
    fan: Option<u16>,
    camera: Option<CameraSample>,
    enabled: bool,
    valid: bool,
    received: bool,
    receive_ns: u64,
    frequency: f64,
    simulation: bool,
}

impl Inputs {
    pub fn new(simulation: bool, frequency: f32) -> Self {
        Self {
            frame: 0,
            fan: None,
            camera: None,
            enabled: false,
            valid: false,
            received: false,
            receive_ns: 0,
            frequency: f64::from(frequency),
            simulation,
        }
    }

    pub fn update(&mut self, now_ns: u64, messages: &[Vec<u8>]) -> capnp::Result<()> {
        self.frame = self.frame.wrapping_add(1);
        if self.frame == u64::MAX {
            self.frame = 1;
        }
        self.fan = None;
        self.camera = None;
        for bytes in messages {
            let message = capnp::serialize::read_message_from_flat_slice(
                &mut bytes.as_slice(),
                capnp::message::ReaderOptions {
                    traversal_limit_in_words: None,
                    ..Default::default()
                },
            )?;
            let event = message.get_root::<event::Reader<'_>>()?;
            match event.which()? {
                event::Which::DeviceState(value) => {
                    self.fan = Some(value?.get_fan_speed_percent_desired());
                }
                event::Which::DriverCameraState(value) => {
                    let camera = value?;
                    self.camera = Some(CameraSample {
                        frame_id: camera.get_frame_id(),
                        integration_lines: camera.get_integ_lines(),
                        mono_time_ns: event.get_log_mono_time(),
                    });
                }
                event::Which::SelfdriveState(value) => {
                    self.enabled = value?.get_enabled();
                    self.valid = event.get_valid();
                    self.received = true;
                    self.receive_ns = now_ns;
                }
                _ => {
                    return Err(capnp::Error::failed(
                        "unexpected Panda input service".into(),
                    ));
                }
            }
        }
        Ok(())
    }

    pub fn peripheral(&self, now_ns: u64, fan_control: bool) -> Input {
        Input {
            frame: self.frame,
            now_ns,
            fan_speed: self.fan,
            camera: self.camera,
            fan_control,
        }
    }

    pub fn engaged(&self, now_ns: u64) -> bool {
        let alive = if self.simulation {
            self.received
        } else {
            self.frequency <= 1e-5
                || now_ns.wrapping_sub(self.receive_ns) as f64 * 1e-9 < 10.0 / self.frequency
        };
        self.enabled && self.valid && alive
    }
}
