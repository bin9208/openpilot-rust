use super::{Admission, Budget, Context, Detector};
use crate::{
    pixels::Rgb,
    runtime::{monotonic_ns, Error},
    transport,
};
use openpilot_cereal::log_capnp::{event, live_calibration_data};
use openpilot_messaging::{runtime::SubMaster, state::Options};
use openpilot_params::Params;
use serde_json::{json, Value};
use std::{path::Path, time::Duration};
pub const PATH: &str = "/dev/shm/carrot_lead_brake_lights.json";
const TOPICS: [&str; 4] = ["radarState", "modelV2", "carState", "liveCalibration"];
#[derive(Default)]
pub struct Runtime {
    output: Option<std::path::PathBuf>,
    params: Option<Params>,
    enabled: bool,
    next_parameter: f64,
    budget: Budget,
    subscriber: Option<SubMaster>,
    detector: Detector,
    session: String,
    next_log: f64,
    next_error: f64,
}
fn now() -> f64 {
    monotonic_ns() as f64 / 1e9
}
pub struct Frame<'a> {
    pub mode_enabled: bool,
    pub rgb: &'a Rgb,
    pub frame_id: u32,
    pub stamp: f64,
    pub session: &'a str,
    pub work_start: f64,
}
impl Runtime {
    pub fn update(
        &mut self,
        frame: Frame<'_>,
        event: &mut impl FnMut(&str, Value) -> Result<(), Error>,
    ) {
        if let Err(error) = self.update_inner(frame, event) {
            let now = now();
            self.detector.reset();
            self.budget.overrun(now);
            if now >= self.next_error {
                let _ = event("leadBrakeLightsFailed", json!({"error":error.to_string()}));
                self.next_error = now + 1.;
            }
        }
    }
    fn update_inner(
        &mut self,
        frame: Frame<'_>,
        event: &mut impl FnMut(&str, Value) -> Result<(), Error>,
    ) -> Result<(), Error> {
        let Frame {
            mode_enabled,
            rgb,
            frame_id,
            stamp,
            session,
            work_start,
        } = frame;
        let current = now();
        if mode_enabled && current >= self.next_parameter {
            self.enabled = (|| -> Result<bool, Error> {
                if self.params.is_none() {
                    self.params = Some(Params::for_runtime()?);
                }
                Ok(self
                    .params
                    .as_ref()
                    .ok_or("brake Params absent")?
                    .get_bool("LeadBrakeLights")?)
            })()
            .unwrap_or(false);
            self.next_parameter = current + 0.5;
        }
        let admission =
            self.budget
                .admit(mode_enabled && self.enabled, session, current, work_start);
        match admission {
            Admission::Off => {
                self.subscriber = None;
                self.detector.reset();
                self.session.clear();
                return Ok(());
            }
            Admission::Skip => return Ok(()),
            _ => {}
        }
        if self.session != session {
            self.detector.reset();
            self.session = session.into();
        }
        let mut result = json!({"state":"unknown","reason":"signal_budget","box":null});
        if admission == Admission::Work {
            if self.subscriber.is_none() {
                self.subscriber = Some(SubMaster::for_runtime(&TOPICS, Options::default())?);
            }
            let sub = self.subscriber.as_mut().ok_or("brake subscriber absent")?;
            sub.update(Duration::ZERO)?;
            let fresh = TOPICS.iter().all(|name| {
                sub.state.topic(name).is_ok_and(|t| {
                    t.seen
                        && t.valid
                        && (0. ..=if *name == "liveCalibration" { 2. } else { 0.25 })
                            .contains(&(current - t.log_mono_time as f64 / 1e9))
                })
            });
            if fresh {
                result = match context(sub) {
                    Ok(context) => self.detector.process(rgb, stamp, &context)?,
                    Err(_) => self.detector.unknown("invalid_context"),
                };
            } else {
                result = self.detector.unknown("stale_context");
            }
        } else {
            self.detector.reset();
        }
        let end = now();
        let elapsed = (end - current) * 1000.;
        if elapsed > 5. {
            result = self.detector.unknown("work_budget");
            self.budget.overrun(end);
        }
        let object = result
            .as_object_mut()
            .ok_or("brake result must be an object")?;
        object.extend(json!({"version":1,"frame_id":frame_id,"timestamp":stamp,"session":session,"work_ms":elapsed,"control_permission":false}).as_object().unwrap().clone());
        transport::write(
            self.output.as_deref().unwrap_or(Path::new(PATH)),
            &result,
            4096,
        )?;
        if current >= self.next_log {
            event("leadBrakeLights", result)?;
            self.next_log = current + 1.;
        }
        Ok(())
    }
}
fn context(sub: &SubMaster) -> Result<Context, Error> {
    macro_rules! read {
        ($name:literal,$variant:ident) => {
            match sub.state.topic($name)?.event()?.which()? {
                event::$variant(value) => value?,
                _ => return Err("brake topic variant mismatch".into()),
            }
        };
    }
    let radar = read!("radarState", RadarState);
    let lead = radar.get_lead_one()?;
    let model = read!("modelV2", ModelV2);
    let leads = model.get_leads_v3()?;
    if leads.is_empty() {
        return Err("brake vision lead absent".into());
    }
    let vision = leads.get(0);
    let x = vision.get_x()?;
    let y = vision.get_y()?;
    if x.is_empty() || y.is_empty() {
        return Err("brake vision position absent".into());
    }
    let car = read!("carState", CarState);
    let calibration = read!("liveCalibration", LiveCalibration);
    let rpy = calibration.get_rpy_calib()?;
    let height = calibration.get_height()?;
    if rpy.len() != 3 || height.is_empty() {
        return Err("brake calibration shape".into());
    }
    Ok(Context {
        lead: lead.get_status(),
        radar: lead.get_radar(),
        id: i64::from(lead.get_radar_track_id()),
        distance: f64::from(lead.get_d_rel()),
        lateral: -f64::from(lead.get_y_rel()),
        probability: f64::from(vision.get_prob()),
        vision_x: f64::from(x.get(0)),
        vision_y: f64::from(y.get(0)),
        yaw: f64::from(car.get_yaw_rate()),
        steer: f64::from(car.get_steering_angle_deg()),
        speed: f64::from(car.get_v_ego()),
        calibrated: calibration.get_cal_status()? == live_calibration_data::Status::Calibrated,
        height: f64::from(height.get(0)),
        rpy: [
            f64::from(rpy.get(0)),
            f64::from(rpy.get(1)),
            f64::from(rpy.get(2)),
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn off_never_creates_subscriptions_or_output_and_busy_never_decodes_images() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("result.json");
        let params = Params::open(&dir.path().join("params"), "brake-off").unwrap();
        params.put("LeadBrakeLights", b"0").unwrap();
        let mut worker = Runtime {
            params: Some(params),
            output: Some(path.clone()),
            ..Runtime::default()
        };
        // Invalid pixels prove these branches do not enter conversion/detection.
        let rgb = Rgb {
            width: 0,
            height: 0,
            data: vec![],
        };
        let mut events = Vec::new();
        for _ in 0..50_000 {
            worker.update(
                Frame {
                    mode_enabled: true,
                    rgb: &rgb,
                    frame_id: 1,
                    stamp: now(),
                    session: "s",
                    work_start: now(),
                },
                &mut |name, value| {
                    events.push((name.to_owned(), value));
                    Ok(())
                },
            );
        }
        assert!(events.is_empty());
        assert!(!path.exists());
        assert!(worker.subscriber.is_none());
        worker.enabled = true;
        worker.next_parameter = now() + 10.;
        worker.update(
            Frame {
                mode_enabled: true,
                rgb: &rgb,
                frame_id: 2,
                stamp: now(),
                session: "s",
                work_start: now() - 0.1,
            },
            &mut |_, _| Ok(()),
        );
        let value: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(value["state"], "unknown");
        assert_eq!(value["reason"], "signal_budget");
        assert_eq!(value["control_permission"], false);
        assert!(worker.subscriber.is_none());
        let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
        worker.update(
            Frame {
                mode_enabled: true,
                rgb: &rgb,
                frame_id: 3,
                stamp: now(),
                session: "s",
                work_start: now() - 0.1,
            },
            &mut |_, _| panic!("10 Hz budget ignored"),
        );
        assert_eq!(
            std::fs::metadata(&path).unwrap().modified().unwrap(),
            modified
        );
        worker.update(
            Frame {
                mode_enabled: false,
                rgb: &rgb,
                frame_id: 4,
                stamp: now(),
                session: "s",
                work_start: now(),
            },
            &mut |_, _| panic!("OFF must be lazy"),
        );
        assert!(worker.session.is_empty());
        assert!(worker.subscriber.is_none());
    }
}
