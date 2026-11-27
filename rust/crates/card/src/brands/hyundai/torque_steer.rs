//! `TorqueSteerTest`: the LFA1 torque request (0x12A `LFA`, `LKA_MODE` 2,
//! `STEER_REQ`, `TORQUE_REQUEST`) on an angle-control car, parked.
//!
//! Angle cars (LFA2, `LFA_ALT` 0xCB) stop at ~176 deg: the EPS holds the
//! angle request there and faults past ~180 (joongyu01 10-29). The owner's
//! IONIQ 5 PE EPS reports `MDPS_LkaPlgInSta` = 1 ("the LKA function of MDPS
//! is plugged in", 0xEA bits 46..47) with `MDPS_LkaToiActvSta` never set,
//! because the stock camera never asks for torque there (00000cf1). This asks
//! whether the torque interface still steers, which has no 176 deg limit.
//!
//! The setting is the torque (units of `TORQUE_REQUEST`, + left, |v| <= 270,
//! the CAN-FD torque max). One run per setting edge: ramp up 2 per frame (the
//! CAN-FD torque rate), hold 3 s, ramp down 3 per frame, then the normal
//! frames again; the setting is cleared after each run. Runs in P, or in D/R
//! held by the brake or AutoHold, standing, not engaged. Stops at once on
//! leaving that, the driver's hand, or after 10 s; ramps down early when the
//! wheel passes 400 deg. `TorqueSteerTestCopy` = 1 sends the stock camera's
//! 0x12A with only the torque fields changed instead of a fresh torque frame.
//! panda does not gate 0x12A torque (`tx = false` is commented out in
//! safety_hyundai_canfd.h), so this guard is the card's alone.
use openpilot_params::Params;

pub const KEY: &str = "TorqueSteerTest";
pub const COPY_KEY: &str = "TorqueSteerTestCopy";
pub const LIMIT: f64 = 270.;
const UP: f64 = 2.;
const DOWN: f64 = 3.;
const HOLD: f64 = 3.;
const MAX_RUN: f64 = 10.;
const ANGLE_GUARD: f64 = 400.;
const _: () = assert!(LIMIT / UP * 0.01 + HOLD + LIMIT / DOWN * 0.01 < MAX_RUN);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Off,
    Up,
    Hold,
    Down,
}

impl Stage {
    pub fn name(self) -> &'static str {
        match self {
            Stage::Off => "off",
            Stage::Up => "up",
            Stage::Hold => "hold",
            Stage::Down => "down",
        }
    }
}

/// What the test asks for this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Request {
    pub torque: f64,
    pub steer_req: bool,
    pub copy_stock: bool,
}

/// Inputs read from carState / carControl each frame.
#[derive(Clone, Copy, Debug)]
pub struct Observed {
    /// Parked or held standing, not engaged, CAN valid.
    pub ready: bool,
    pub pressed: bool,
    pub angle: f64,
}

pub struct TorqueSteer {
    setting: i32,
    settings_at: f64,
    copy_stock: bool,
    pub stage: Stage,
    started: f64,
    stage_at: f64,
    start_angle: f64,
    peak_angle: f64,
    target: f64,
    pub command: f64,
    log_at: f64,
    pub logs: Vec<crate::core::VehicleLog>,
}

impl Default for TorqueSteer {
    fn default() -> Self {
        Self {
            setting: 0,
            settings_at: f64::NEG_INFINITY,
            copy_stock: false,
            stage: Stage::Off,
            started: 0.,
            stage_at: 0.,
            start_angle: 0.,
            peak_angle: 0.,
            target: 0.,
            command: 0.,
            log_at: 0.,
            logs: Vec::new(),
        }
    }
}

fn toward(from: f64, to: f64, step: f64) -> f64 {
    from + (to - from).clamp(-step, step)
}

impl TorqueSteer {
    pub fn active(&self) -> bool {
        self.stage != Stage::Off
    }

    fn log(&mut self, message: String) {
        self.logs.push(crate::core::VehicleLog {
            level: crate::query::DiagnosticLevel::Info,
            message,
        });
    }

    fn enter(&mut self, stage: Stage, now: f64, o: &Observed) {
        self.stage = stage;
        self.stage_at = now;
        self.log(format!(
            "TorqueSteerTest stage={} cmd={:.0} angle={:.1} start={:.1}",
            stage.name(),
            self.command,
            o.angle,
            self.start_angle
        ));
    }

    fn finish(&mut self, reason: &str, o: &Observed, settings: &Params) {
        self.stage = Stage::Off;
        self.command = 0.;
        let _ = settings.put(KEY, b"0");
        self.log(format!(
            "TorqueSteerTest end={reason} target={:.0} start={:.1} end_angle={:.1} peak_delta={:.1} copy={}",
            self.target,
            self.start_angle,
            o.angle,
            self.peak_angle - self.start_angle,
            self.copy_stock
        ));
    }

    /// One frame (100 Hz). `None`: send the normal frames.
    pub fn update(&mut self, o: Observed, now: f64, settings: &Params) -> Option<Request> {
        if now >= self.settings_at {
            let setting = super::parameters::setting_int(settings, KEY)
                .unwrap_or(0)
                .clamp(-LIMIT as i32, LIMIT as i32);
            if setting != self.setting && setting != 0 && !self.active() {
                self.copy_stock = settings.get_bool(COPY_KEY).unwrap_or(false);
                self.target = f64::from(setting);
                if o.ready && !o.pressed {
                    self.start_angle = o.angle;
                    self.peak_angle = o.angle;
                    self.command = 0.;
                    self.started = now;
                    self.log_at = now;
                    self.enter(Stage::Up, now, &o);
                } else {
                    self.finish("refused_not_parked", &o, settings);
                }
            } else if setting == 0 && matches!(self.stage, Stage::Up | Stage::Hold) {
                self.enter(Stage::Down, now, &o);
            }
            self.setting = setting;
            self.settings_at = now + 0.1;
        }
        if !self.active() {
            return None;
        }
        if (o.angle - self.start_angle).abs() > (self.peak_angle - self.start_angle).abs() {
            self.peak_angle = o.angle;
        }
        if !o.ready {
            self.finish("stopped_not_parked", &o, settings);
            return None;
        }
        if o.pressed {
            self.finish("stopped_driver_torque", &o, settings);
            return None;
        }
        if now - self.started >= MAX_RUN {
            self.finish("stopped_time", &o, settings);
            return None;
        }
        if o.angle.abs() >= ANGLE_GUARD && matches!(self.stage, Stage::Up | Stage::Hold) {
            self.enter(Stage::Down, now, &o);
        }
        match self.stage {
            Stage::Up => {
                self.command = toward(self.command, self.target, UP);
                if (self.command - self.target).abs() < 1e-9 {
                    self.enter(Stage::Hold, now, &o);
                }
            }
            Stage::Hold => {
                if now - self.stage_at >= HOLD {
                    self.enter(Stage::Down, now, &o);
                }
            }
            Stage::Down => {
                self.command = toward(self.command, 0., DOWN);
                if self.command.abs() < 1e-9 {
                    self.finish("done", &o, settings);
                    return None;
                }
            }
            Stage::Off => return None,
        }
        if now >= self.log_at {
            self.log_at = now + 0.25;
            self.log(format!(
                "TorqueSteerTest {} cmd={:.0} angle={:.1} delta={:.1}",
                self.stage.name(),
                self.command,
                o.angle,
                o.angle - self.start_angle
            ));
        }
        Some(Request {
            torque: self.command.round(),
            steer_req: true,
            copy_stock: self.copy_stock,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params() -> (tempfile::TempDir, Params) {
        let temp = tempfile::tempdir().unwrap();
        let settings = Params::open(temp.path(), "d").unwrap();
        (temp, settings)
    }

    fn run(
        target: i32,
        observe: impl Fn(f64) -> Observed,
    ) -> (Vec<(f64, Request)>, TorqueSteer, String) {
        let (_temp, settings) = params();
        settings.put(KEY, target.to_string().as_bytes()).unwrap();
        let mut test = TorqueSteer::default();
        let mut out = Vec::new();
        let mut now = 100.;
        while now < 115. {
            if let Some(r) = test.update(observe(now), now, &settings) {
                out.push((now, r));
            }
            now += 0.01;
        }
        let left = String::from_utf8(settings.get(KEY).unwrap().unwrap_or_default()).unwrap();
        (out, test, left)
    }

    fn parked(_: f64) -> Observed {
        Observed {
            ready: true,
            pressed: false,
            angle: 5.,
        }
    }

    fn ended(test: &TorqueSteer, reason: &str) -> bool {
        test.logs
            .iter()
            .any(|l| l.message.contains(&format!("end={reason}")))
    }

    #[test]
    fn full_run_ramps_holds_and_clears_the_trigger() {
        let (frames, test, left) = run(-150, parked);
        assert!(ended(&test, "done"));
        assert_eq!(left, "0");
        assert!(frames.iter().all(|(_, r)| r.steer_req && !r.copy_stock));
        assert!(frames.iter().any(|(_, r)| (r.torque + 150.).abs() < 1e-9));
        assert!(frames
            .iter()
            .all(|(_, r)| r.torque <= 0. && r.torque >= -150.));
        for w in frames.windows(2) {
            let d = w[1].1.torque - w[0].1.torque;
            assert!(d.abs() <= DOWN + 1e-9, "rate {d}");
            if w[1].1.torque.abs() > w[0].1.torque.abs() {
                assert!(d.abs() <= UP + 1e-9, "ramp up {d}");
            }
        }
        let span = frames.last().unwrap().0 - frames[0].0;
        assert!(span > HOLD && span < MAX_RUN, "{span}");
        assert!(test.command == 0. && !test.active());
    }

    #[test]
    fn clamps_to_the_canfd_torque_max() {
        let (frames, _, _) = run(999, parked);
        let peak = frames.iter().map(|(_, r)| r.torque).fold(0., f64::max);
        assert!((peak - LIMIT).abs() < 1e-9);
    }

    #[test]
    fn refused_unless_parked() {
        let (frames, test, left) = run(100, |_| Observed {
            ready: false,
            pressed: false,
            angle: 0.,
        });
        assert!(frames.is_empty());
        assert!(ended(&test, "refused_not_parked"));
        assert_eq!(left, "0");
    }

    #[test]
    fn driver_hand_or_leaving_park_cuts_at_once() {
        let (frames, test, _) = run(100, |t| Observed {
            ready: true,
            pressed: t >= 100.5,
            angle: 0.,
        });
        assert!(ended(&test, "stopped_driver_torque"));
        assert!(frames.iter().all(|(t, _)| *t < 100.5));
        let (frames, test, _) = run(100, |t| Observed {
            ready: t < 100.3,
            pressed: false,
            angle: 0.,
        });
        assert!(ended(&test, "stopped_not_parked"));
        assert!(frames.iter().all(|(t, _)| *t < 100.3));
    }

    #[test]
    fn angle_guard_ramps_down_early() {
        let (frames, test, _) = run(200, |t| Observed {
            ready: true,
            pressed: false,
            angle: if t >= 101. { 410. } else { 0. },
        });
        assert!(ended(&test, "done"));
        assert!(test
            .logs
            .iter()
            .any(|l| l.message.contains("peak_delta=410.0")));
        let after: Vec<_> = frames.iter().filter(|(t, _)| *t >= 101.).collect();
        for w in after.windows(2) {
            assert!(w[1].1.torque <= w[0].1.torque, "only down after the guard");
        }
    }

    #[test]
    fn copy_setting_is_read_per_run() {
        let (_temp, settings) = params();
        settings.put(COPY_KEY, b"1").unwrap();
        settings.put(KEY, b"50").unwrap();
        let mut test = TorqueSteer::default();
        let r = test.update(parked(0.), 100., &settings).unwrap();
        assert!(r.copy_stock && r.steer_req);
    }
}
