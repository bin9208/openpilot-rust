use super::{limits::TorqueLimits, parameters::setting_int, Error};
use openpilot_params::Params;

pub struct Settings {
    pub limits: TorqueLimits,
    original: [f64; 2],
    delta: [f64; 4],
    pub max_angle_frames: i32,
    pub soft_hold_mode: u8,
    pub haptic: i32,
    pub spam: [i32; 3],
    pub speed_from_pcm: i32,
    pub debug: i32,
    pub camera: i32,
    pub corner: i32,
    pub paddle: i32,
    pub ldws: bool,
}

impl Settings {
    pub fn new(limits: TorqueLimits, params: &Params) -> Result<Self, Error> {
        Ok(Self {
            original: [limits.delta_up, limits.delta_down],
            delta: [
                limits.delta_up,
                limits.delta_down,
                limits.delta_up,
                limits.delta_down,
            ],
            limits,
            max_angle_frames: 89,
            soft_hold_mode: 2,
            haptic: 0,
            spam: [8, 30, 1],
            speed_from_pcm: 0,
            debug: 0,
            camera: setting_int(params, "HyundaiCameraSCC")?,
            corner: 0,
            paddle: setting_int(params, "PaddleMode")?,
            ldws: params.get_bool("IsLdwsCar")?,
        })
    }

    pub fn refresh(&mut self, params: &Params) -> Result<(), Error> {
        self.max_angle_frames = setting_int(params, "MaxAngleFrames")?;
        let max = setting_int(params, "CustomSteerMax")?;
        if max > 0 {
            self.limits.max = f64::from(max);
        }
        let up = setting_int(params, "CustomSteerDeltaUp")?;
        let down = setting_int(params, "CustomSteerDeltaDown")?;
        self.delta[0] = if up > 0 {
            f64::from(up)
        } else {
            self.original[0]
        };
        self.delta[1] = if down > 0 {
            f64::from(down)
        } else {
            self.original[1]
        };
        let up_lc = setting_int(params, "CustomSteerDeltaUpLC")?;
        let down_lc = setting_int(params, "CustomSteerDeltaDownLC")?;
        self.delta[2] = if up_lc > 0 {
            f64::from(up_lc)
        } else {
            self.delta[0]
        };
        self.delta[3] = if down_lc > 0 {
            f64::from(down_lc)
        } else {
            self.delta[1]
        };
        self.soft_hold_mode = if setting_int(params, "AutoCruiseControl")? > 1 {
            1
        } else {
            2
        };
        self.haptic = setting_int(params, "HapticFeedbackWhenSpeedCamera")?;
        self.spam = [
            setting_int(params, "CruiseButtonTest1")?,
            setting_int(params, "CruiseButtonTest2")?,
            setting_int(params, "CruiseButtonTest3")?,
        ];
        self.speed_from_pcm = setting_int(params, "SpeedFromPCM")?;
        self.debug = setting_int(params, "CanfdDebug")?;
        self.camera = setting_int(params, "HyundaiCameraSCC")?;
        self.corner = setting_int(params, "EnableCornerRadar")?;
        self.paddle = setting_int(params, "PaddleMode")?;
        Ok(())
    }

    pub fn lane_change(&mut self, desire: i16) {
        let index = if matches!(desire, 3 | 4) { 2 } else { 0 };
        self.limits.delta_up = self.delta[index];
        self.limits.delta_down = self.delta[index + 1];
    }
}
