//! The SPAS path of carrot's `tools/jetlink/blink_source.py` main loop, run
//! inside card at the worker's 20 Hz instead of as a Python process.
//!
//! In SPAS mode carrot's worker takes intents from three sources: explicit
//! Bluetooth remote taps, the navigation turn signal (`AutomaticSignals`) and
//! a confirmed voice lane change. Only the navigation turn signal has its
//! inputs in this port today; Bluetooth remote actions (A08) and voice lane
//! requests (A12) are not published yet, so those two produce nothing.
//! TBT, boundary and observation intents are off in SPAS mode in carrot too.
use super::{
    policy::{AutomaticInput, AutomaticSignals, Hold, Policy, PolicyInput},
    turn_journey::Travel,
    Side,
};
use openpilot_cereal::{
    car_capnp::car_state,
    log_capnp::{event, Desire, LaneChangeDirection},
};
use openpilot_messaging::state::{State as Subscribers, Topic};
use openpilot_params::Params;

/// What the worker hands the SPAS transport each cycle (`encode(...)`).
#[derive(Clone, Debug, Default)]
pub struct Record {
    pub stamp: f64,
    pub ok: bool,
    pub hold: Option<Hold>,
    /// Short taps not held (`events`), most recent last.
    pub events: Vec<(String, f64, Side)>,
    pub turn_phase: &'static str,
}

/// What the transport reports back (`worker.send(...)`).
#[derive(Clone, Debug, Default)]
pub struct Feedback {
    pub stamp: f64,
    pub accepted_id: Option<String>,
    pub available: bool,
    /// `"driver_override" | "ready" | "waiting"`.
    pub state: &'static str,
}

pub struct SpasWorker {
    policy: Policy,
    automatic: AutomaticSignals,
    travel: Travel,
    activated_id: Option<String>,
    next: f64,
    pub record: Option<Record>,
}

fn fresh_topic(topic: &Topic, now: f64, limit: f64) -> bool {
    topic.valid && topic.alive && (0. ..limit).contains(&(now - topic.receive_time))
}

fn gear_name(gear: car_state::GearShifter) -> &'static str {
    match gear {
        car_state::GearShifter::Park => "park",
        car_state::GearShifter::Neutral => "neutral",
        car_state::GearShifter::Drive => "drive",
        car_state::GearShifter::Unknown => "unknown",
        _ => "other",
    }
}

fn side(direction: LaneChangeDirection) -> Option<Side> {
    match direction {
        LaneChangeDirection::Left => Some(Side::Left),
        LaneChangeDirection::Right => Some(Side::Right),
        _ => None,
    }
}

fn desire_name(desire: Desire) -> &'static str {
    use Desire as D;
    match desire {
        D::TurnLeft => "turnLeft",
        D::TurnRight => "turnRight",
        _ => "other",
    }
}

impl Default for SpasWorker {
    fn default() -> Self {
        Self::new()
    }
}

impl SpasWorker {
    pub fn new() -> Self {
        Self {
            policy: Policy::new(),
            automatic: AutomaticSignals::new(),
            travel: Travel::default(),
            activated_id: None,
            next: 0.,
            record: None,
        }
    }

    /// Runs one worker cycle when 50 ms have passed; `record` keeps the last
    /// result otherwise, as the transport keeps the worker's latest message.
    pub fn update(
        &mut self,
        now: f64,
        cs: car_state::Reader<'_>,
        subs: &Subscribers,
        settings: &Params,
        feedback: &Feedback,
    ) -> Result<(), crate::core::Error> {
        if now < self.next {
            return Ok(());
        }
        self.next = now + 0.05;
        let topic = |name: &str| subs.topic(name).ok();
        let control = topic("carControl").and_then(|t| match t.event().ok()?.which().ok()? {
            event::Which::CarControl(v) => v.ok(),
            _ => None,
        });
        let selfdrive =
            topic("selfdriveState").and_then(|t| match t.event().ok()?.which().ok()? {
                event::Which::SelfdriveState(v) => v.ok(),
                _ => None,
            });
        let pandas_ok = topic("pandaStates").is_some_and(|t| {
            fresh_topic(t, now, 0.75)
                && matches!(t.event().ok().and_then(|e| e.which().ok()),
                    Some(event::Which::PandaStates(Ok(list))) if !list.is_empty())
        });
        let gear = gear_name(
            cs.get_gear_shifter()
                .unwrap_or(car_state::GearShifter::Unknown),
        );
        // vehicle_ready(spas=True): manual indicators work while driving or
        // engaged; unknown or stale CAN never does.
        let ok = ["carControl", "selfdriveState"]
            .iter()
            .all(|name| topic(name).is_some_and(|t| fresh_topic(t, now, 0.35)))
            && pandas_ok
            && cs.get_can_valid()
            && cs.get_v_ego().is_finite()
            && gear != "unknown";
        let mode =
            super::super::brands::hyundai::parameters::setting_int(settings, "AutoBlinkerControl")
                .unwrap_or(0);
        let mode = if (0..=2).contains(&mode) { mode } else { 0 };
        let auto_paused = cs.get_cruise_cancel_state();
        let carrot_limit = if self.automatic.direction.is_some() {
            1.0
        } else {
            0.35
        };
        let model = topic("modelV2").filter(|t| fresh_topic(t, now, 0.35));
        let carrot = topic("carrotMan").filter(|t| fresh_topic(t, now, carrot_limit));
        let auto_fresh = model.is_some() && carrot.is_some();
        let travelled = self.travel.update(now, f64::from(cs.get_v_ego()), ok);
        let always_lateral = settings.get_bool("AlwaysLateral").unwrap_or(false);
        let lateral_allowed = gear == "drive" && control.is_some_and(|cc| cc.get_lat_active())
            || (gear == "drive"
                && selfdrive.is_some_and(|ss| ss.get_active() || always_lateral)
                && cs.get_lat_enabled()
                && !cs.get_steer_fault_temporary()
                && !cs.get_steer_fault_permanent());
        // carrotNavi is not subscribed by card: navigation_cancelled() is false.
        let auto_valid = !auto_paused && auto_fresh && ok && lateral_allowed;
        let mut auto = None;
        let heading = topic("livePose")
            .filter(|t| fresh_topic(t, now, 0.35))
            .and_then(|t| match t.event().ok()?.which().ok()? {
                event::Which::LivePose(Ok(pose)) => {
                    let o = pose.get_orientation_n_e_d().ok()?;
                    let z = f64::from(o.get_z());
                    (pose.get_inputs_o_k()
                        && pose.get_sensors_o_k()
                        && pose.get_posenet_o_k()
                        && o.get_valid()
                        && z.is_finite())
                    .then_some(z)
                }
                _ => None,
            });
        if let (Some(model), Some(carrot), Some(ss)) = (model, carrot, selfdrive) {
            let meta = match model.event()?.which()? {
                event::Which::ModelV2(value) => value?.get_meta()?,
                _ => return Err(crate::core::Error::Event("modelV2")),
            };
            let cm = match carrot.event()?.which()? {
                event::Which::CarrotMan(value) => value?,
                _ => return Err(crate::core::Error::Event("carrotMan")),
            };
            let atc_type = cm.get_atc_type()?.to_str()?.to_owned();
            let lane = if meta.get_auto_turn_signal_lane_change()
                && control.is_some_and(|cc| cc.get_lat_active())
            {
                side(meta.get_auto_turn_signal()?)
            } else {
                None
            };
            auto = self.automatic.update(AutomaticInput {
                now,
                mode,
                valid: auto_valid,
                cue: ss.get_atc_turn_start_mono_time() as f64 / 1e9,
                direction: side(ss.get_atc_turn_direction()?),
                atc_type: &atc_type,
                desire: desire_name(meta.get_desire()?),
                lane,
                speed: f64::from(cs.get_v_ego()),
                angle: f64::from(cs.get_steering_angle_deg()),
                heading,
                distance: Some(f64::from(cm.get_x_dist_to_turn())),
                position: None,
                target: None,
                travel: travelled,
                use_travel: true,
                pressed: cs.get_steering_pressed(),
                torque: f64::from(cs.get_steering_torque()),
            });
        } else {
            self.automatic.update(AutomaticInput {
                now,
                mode,
                valid: false,
                cue: 0.,
                direction: None,
                atc_type: "",
                desire: "none",
                lane: None,
                speed: 0.,
                angle: 0.,
                heading: None,
                distance: None,
                position: None,
                target: None,
                travel: None,
                use_travel: true,
                pressed: false,
                torque: 0.,
            });
        }
        let model_fresh = model.is_some();
        let local_observation = auto.is_none()
            && self
                .policy
                .hold
                .as_ref()
                .is_none_or(|h| h.source == "observation");
        let policy_valid = if local_observation {
            !auto_paused && ok && model_fresh
        } else {
            auto_valid
        };
        let (taps, _) = self.policy.update(PolicyInput {
            now,
            mode,
            ok,
            commands: &[],
            speed: f64::from(cs.get_v_ego()),
            angle: f64::from(cs.get_steering_angle_deg()),
            gear,
            remote: false,
            automatic: auto,
            auto_valid: policy_valid,
        });
        let peer_fresh = (0. ..0.35).contains(&(now - feedback.stamp));
        let receiver_override = peer_fresh && feedback.state == "driver_override";
        let hold = self
            .policy
            .acknowledged(now, self.activated_id.as_deref(), receiver_override)
            .cloned();
        let acknowledged = hold.as_ref().is_some_and(|h| {
            peer_fresh
                && feedback.accepted_id.as_deref() == Some(h.id.as_str())
                && matches!(
                    feedback.state,
                    "ready" | "waiting" | "waiting_previous_signal"
                )
        });
        if !acknowledged {
            self.activated_id = None;
        } else if let Some(h) = &hold {
            let (left, right) = (cs.get_left_blinker(), cs.get_right_blinker());
            if (h.direction == Side::Left && left && !right)
                || (h.direction == Side::Right && right && !left)
            {
                self.activated_id = Some(h.id.clone());
            }
        }
        if feedback.state == "driver_override" {
            self.automatic.journey.active = None;
            self.automatic.journey.cancelled_until = now + 3.;
        }
        // wire_hold: observation and low-speed TBT holds travel as "lane".
        let hold = hold.map(|mut h| {
            if matches!(h.source.as_str(), "observation" | "tbt_display") {
                h.source = "lane".into();
            }
            h
        });
        self.record = Some(Record {
            stamp: now,
            ok,
            hold,
            events: taps
                .into_iter()
                .map(|c| (c.id, c.time, c.direction))
                .collect(),
            turn_phase: self.automatic.reason,
        });
        Ok(())
    }
}
