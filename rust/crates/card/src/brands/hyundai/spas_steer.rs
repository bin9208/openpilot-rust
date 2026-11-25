//! `SpasSteerTest`: the parking ECU's SPAS1 steering request, parked, on a
//! car with no stock SPAS sender (the owner's IONIQ 5 PE HDA1, no APRK).
//!
//! The LKAS angle path stops at ~180 deg and faults past it
//! (joongyu01/carrot-wip-dev 21c0a2fd0, 10-29). The stock parking assist
//! turns the wheel to ±460 deg through SPAS1 (0x165): byte 3 0x4B, byte 11
//! the mode (3 idle, 4 ask, 5 active, echoed by the EPS in 0xEA byte 5),
//! bytes 12..13 the angle, 0.1 deg, + left (885d366f0). This asks whether
//! this EPS follows that request with no parking ECU fitted.
//!
//! Sequence (one run per setting edge): mode 4 at the current wheel angle for
//! 0.1 s, then mode 5 ramping to the target at 100 deg/s, 2 s hold, back to
//! the start angle, 0.2 s of mode 3. Stops at once on leaving P, moving, the
//! driver's hand or lever, a stock SPAS sender, or the setting going to 0
//! (then it ramps back first). panda (phase 13) admits only this shape.
use super::spas;
use openpilot_can::Frame;
use openpilot_cereal::car_capnp::car_state;
use openpilot_params::Params;

pub const KEY: &str = "SpasSteerTest";
pub const SPAS2_KEY: &str = "SpasSteerTestSpas2";
const PERIOD: f64 = 0.02;
const ASK: f64 = 0.1;
/// deg per 20 ms frame: 100 deg/s (panda admits 3).
const STEP: f64 = 2.0;
const HOLD: f64 = 2.0;
const IDLE_TAIL: f64 = 0.2;
/// panda ends phase 13 at 15 s; a full 450 deg run needs
/// 0.1 + 4.5 + 2 + 4.5 + 0.2 = 11.3 s.
const MAX_RUN: f64 = 14.0;
const _: () = assert!(ASK + 2. * (LIMIT / STEP * PERIOD) + HOLD + IDLE_TAIL < MAX_RUN);
const LIMIT: f64 = 450.;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Off,
    Ask,
    Out,
    Hold,
    Back,
    Tail,
}

impl Stage {
    pub fn name(self) -> &'static str {
        match self {
            Stage::Off => "off",
            Stage::Ask => "ask",
            Stage::Out => "out",
            Stage::Hold => "hold",
            Stage::Back => "back",
            Stage::Tail => "tail",
        }
    }
}

/// SPAS1 as the parking ECU steers.
pub fn steer_frame(counter: u8, mode: u8, angle_deg: f64, bus: u8) -> Frame {
    let mut data = vec![0u8; 24];
    data[2] = counter;
    data[3] = 0x4b;
    data[11] = (mode & 7) << 2;
    let raw = (angle_deg * 10.).round().clamp(-4600., 4600.) as i16;
    data[12..14].copy_from_slice(&raw.to_le_bytes());
    let crc = spas::checksum(0x165, &data).to_le_bytes();
    data[..2].copy_from_slice(&crc);
    Frame {
        address: 0x165,
        data,
        bus,
    }
}

fn toward(from: f64, to: f64) -> f64 {
    from + (to - from).clamp(-STEP, STEP)
}

pub struct SpasSteer {
    setting: i32,
    settings_at: f64,
    with_spas2: bool,
    pub stage: Stage,
    started: f64,
    stage_at: f64,
    start_angle: f64,
    target: f64,
    pub command: f64,
    next_tx: f64,
    counter: [u8; 2],
    pub reason: &'static str,
    report: Option<&'static str>,
}

impl Default for SpasSteer {
    fn default() -> Self {
        Self {
            setting: 0,
            settings_at: f64::NEG_INFINITY,
            with_spas2: false,
            stage: Stage::Off,
            started: 0.,
            stage_at: 0.,
            start_angle: 0.,
            target: 0.,
            command: 0.,
            next_tx: 0.,
            counter: [0; 2],
            reason: "idle",
            report: None,
        }
    }
}

impl SpasSteer {
    pub fn active(&self) -> bool {
        self.stage != Stage::Off
    }

    fn enter(&mut self, stage: Stage, now: f64) {
        self.stage = stage;
        self.stage_at = now;
    }

    fn finish(&mut self, reason: &'static str, settings: &Params) {
        self.stage = Stage::Off;
        self.reason = reason;
        self.report = Some(reason);
        let _ = settings.put(KEY, b"0");
    }

    /// How the last run ended, once.
    pub fn take_report(&mut self) -> Option<&'static str> {
        self.report.take()
    }

    /// `ready`: parked (P, standing, not engaged), no stock SPAS sender, no
    /// driver lever. Returns this cycle's frames.
    pub fn update(
        &mut self,
        cs: car_state::Reader<'_>,
        ready: bool,
        now: f64,
        bus: u8,
        settings: &Params,
    ) -> Vec<Frame> {
        if now >= self.settings_at {
            let setting = super::parameters::setting_int(settings, KEY)
                .unwrap_or(0)
                .clamp(-LIMIT as i32, LIMIT as i32);
            self.with_spas2 = settings.get_bool(SPAS2_KEY).unwrap_or(false);
            if setting != self.setting && setting != 0 && !self.active() {
                if ready && !cs.get_steering_pressed() {
                    let angle = f64::from(cs.get_steering_angle_deg());
                    self.start_angle = angle;
                    self.command = angle;
                    self.target = f64::from(setting);
                    self.started = now;
                    self.next_tx = now;
                    self.reason = "running";
                    self.enter(Stage::Ask, now);
                } else {
                    self.finish("refused_not_parked", settings);
                }
            } else if setting == 0 && matches!(self.stage, Stage::Ask | Stage::Out | Stage::Hold) {
                self.enter(Stage::Back, now); // cancelled: straighten back first
            }
            self.setting = setting;
            self.settings_at = now + 0.1;
        }
        if !self.active() {
            return Vec::new();
        }
        if !ready {
            self.finish("stopped_not_parked", settings);
            return Vec::new();
        }
        if cs.get_steering_pressed() {
            self.finish("stopped_driver_torque", settings);
            return Vec::new();
        }
        if now - self.started >= MAX_RUN {
            self.finish("stopped_time", settings);
            return Vec::new();
        }
        if now < self.next_tx {
            return Vec::new();
        }
        self.next_tx = (self.next_tx + PERIOD).max(now + PERIOD * 0.75);
        let mode = match self.stage {
            Stage::Ask => {
                if now - self.stage_at >= ASK {
                    self.enter(Stage::Out, now);
                }
                4
            }
            Stage::Out => {
                self.command = toward(self.command, self.target);
                if (self.command - self.target).abs() < 1e-6 {
                    self.enter(Stage::Hold, now);
                }
                5
            }
            Stage::Hold => {
                if now - self.stage_at >= HOLD {
                    self.enter(Stage::Back, now);
                }
                5
            }
            Stage::Back => {
                self.command = toward(self.command, self.start_angle);
                if (self.command - self.start_angle).abs() < 1e-6 {
                    self.enter(Stage::Tail, now);
                }
                5
            }
            Stage::Tail => {
                if now - self.stage_at >= IDLE_TAIL {
                    self.finish("done", settings);
                    return Vec::new();
                }
                3
            }
            Stage::Off => return Vec::new(),
        };
        let mut frames = vec![steer_frame(self.counter[0], mode, self.command, bus)];
        self.counter[0] = self.counter[0].wrapping_add(1);
        if self.with_spas2 {
            frames.push(spas::frame(0x16a, self.counter[1], None, bus));
            self.counter[1] = self.counter[1].wrapping_add(1);
        }
        frames
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_layout_matches_the_parking_ecu() {
        let f = steer_frame(7, 5, -12.3, 0);
        assert_eq!(f.address, 0x165);
        assert_eq!(f.data.len(), 24);
        assert_eq!(f.data[2], 7);
        assert_eq!(f.data[3], 0x4b);
        assert_eq!(f.data[11], 0x14);
        assert_eq!(i16::from_le_bytes([f.data[12], f.data[13]]), -123);
        assert_eq!(
            u16::from_le_bytes([f.data[0], f.data[1]]),
            spas::checksum(0x165, &f.data)
        );
        for (i, b) in f.data.iter().enumerate().skip(3) {
            if ![3, 11, 12, 13].contains(&i) {
                assert_eq!(*b, 0, "byte {i}");
            }
        }
        assert_eq!(steer_frame(0, 4, 0., 0).data[11], 0x10);
        assert_eq!(steer_frame(0, 3, 0., 0).data[11], 0x0c);
    }

    fn run(target: i32, abort_at: Option<f64>) -> (Vec<(f64, Frame)>, &'static str, String) {
        let temp = tempfile::tempdir().unwrap();
        let settings = Params::open(temp.path(), "d").unwrap();
        let mut message = capnp::message::Builder::new_default();
        {
            let mut cs = message.init_root::<car_state::Builder<'_>>();
            cs.set_steering_angle_deg(10.);
        }
        let cs = message.get_root_as_reader::<car_state::Reader<'_>>().unwrap();
        let mut steer = SpasSteer::default();
        settings.put(KEY, target.to_string().as_bytes()).unwrap();
        let mut out = Vec::new();
        let mut now = 100.;
        while now < 120. {
            let ready = abort_at.is_none_or(|t| now < t);
            for f in steer.update(cs, ready, now, 0, &settings) {
                out.push((now, f));
            }
            now += 0.01;
        }
        let left = String::from_utf8(settings.get(KEY).unwrap().unwrap_or_default()).unwrap();
        (out, steer.reason, left)
    }

    #[test]
    fn full_run_is_what_panda_admits() {
        let (frames, reason, setting) = run(-300, None);
        assert_eq!(reason, "done");
        assert_eq!(setting, "0", "the trigger is cleared");
        let decoded: Vec<(f64, u8, f64)> = frames
            .iter()
            .map(|(t, f)| {
                (
                    *t,
                    f.data[11] >> 2,
                    f64::from(i16::from_le_bytes([f.data[12], f.data[13]])) / 10.,
                )
            })
            .collect();
        assert_eq!(decoded[0].1, 4, "starts by asking");
        assert!((decoded[0].2 - 10.).abs() < 1e-9, "at the current wheel angle");
        let asks = decoded.iter().take_while(|d| d.1 == 4).count();
        assert!((5..=7).contains(&asks), "0.1 s of mode 4: {asks}");
        assert!(decoded.iter().any(|d| (d.2 + 300.).abs() < 1e-9), "reaches the target");
        assert_eq!(decoded.last().unwrap().1, 3, "ends idle");
        assert!((decoded.last().unwrap().2 - 10.).abs() < 1e-9, "back to the start angle");
        for w in decoded.windows(2) {
            assert!((w[1].2 - w[0].2).abs() <= 3.0, "panda step");
            assert!(w[1].0 - w[0].0 >= 0.015 - 1e-9, "panda spacing");
        }
        assert!(decoded.last().unwrap().0 - decoded[0].0 < 15., "inside panda's 15 s");
        for (i, (_, f)) in frames.iter().enumerate() {
            assert_eq!(f.data[2], i as u8, "counter advances by one");
        }
    }

    #[test]
    fn leaving_park_stops_at_once() {
        let (frames, reason, setting) = run(200, Some(101.));
        assert_eq!(reason, "stopped_not_parked");
        assert_eq!(setting, "0");
        assert!(frames.iter().all(|(t, _)| *t < 101.));
    }

    #[test]
    fn ramp_stays_inside_the_panda_step() {
        let mut a = 0.;
        let mut steps = 0;
        while (a - 450f64).abs() > 1e-9 {
            let b = toward(a, 450.);
            assert!((b - a).abs() <= 3.0);
            a = b;
            steps += 1;
        }
        assert_eq!(steps, 225, "4.5 s at 50 Hz");
    }
}
