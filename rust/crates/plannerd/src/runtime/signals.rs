//! Fresh observer inputs acquired at final Carrot owner phases.
use super::{Output, views::Views};
use crate::{
    Error,
    carrot::{PrepareRead, SignalSource, StartRead},
    model::Model,
    platform::Clock,
    signal::{
        context::Context as GeometryInput,
        geometry::Geometry,
        prepare,
        priority::Candidate,
        road_policy::RoadPolicy,
        transport::{self, Mode, ModeCache, Reader, Service, VehicleContext},
    },
};
use openpilot_cereal::{
    car_capnp::car_state::GearShifter,
    log_capnp::{self, event},
};
use openpilot_messaging::{
    runtime::SubMaster,
    state::{Options, State},
};
use openpilot_params::Params;
use std::{path::PathBuf, time::Duration};
/// Owned signal inputs for an embedded planner or isolated replay. Production
/// construction retains canonical Params, observation paths and subscriptions.
pub struct Inputs {
    pub params: Params,
    pub start: PathBuf,
    pub prepare: PathBuf,
    pub geometry: Option<SubMaster>,
}
pub struct RuntimeSignals {
    mode: ModeCache,
    params: Option<Params>,
    start: Reader,
    start_path: PathBuf,
    prepare: Reader,
    prepare_path: PathBuf,
    prepare_mode: Option<Mode>,
    mode_since: f64,
    extra: Option<SubMaster>,
    last_start: Option<serde_json::Value>,
    last_start_log: f64,
    last_prepare: Option<serde_json::Value>,
}
impl RuntimeSignals {
    pub fn new_at(mode: Mode, now: f64) -> Self {
        Self {
            mode: ModeCache::primed(mode, now),
            params: None,
            start: Reader::new(mode.enabled()),
            start_path: transport::START_PATH.into(),
            prepare: Reader::new(mode.enabled()),
            prepare_path: transport::PREPARE_PATH.into(),
            prepare_mode: None,
            mode_since: f64::NEG_INFINITY,
            extra: None,
            last_start: None,
            last_start_log: -1.,
            last_prepare: None,
        }
    }
    pub fn with_inputs(mut self, inputs: Inputs) -> Self {
        self.params = Some(inputs.params);
        self.start_path = inputs.start;
        self.prepare_path = inputs.prepare;
        self.extra = inputs.geometry;
        self
    }
    fn mode(&mut self, now: f64) -> Result<Mode, Error> {
        self.ensure_params()?;
        let params = self
            .params
            .as_ref()
            .ok_or(Error::Contract("missing signal params"))?;
        Ok(current_mode(&mut self.mode, params, now))
    }
    fn ensure_params(&mut self) -> Result<(), Error> {
        if self.params.is_none() {
            self.params = Some(Params::for_runtime()?);
        }
        Ok(())
    }
}
fn current_mode(cache: &mut ModeCache, params: &Params, now: f64) -> Mode {
    cache.read(now, || {
        params.get("SignalAssistMode").map(|bytes| {
            crate::native_parameters::integer(bytes.as_deref().unwrap_or_default())
                .map(i64::from)
                .unwrap_or(0)
        })
    })
}
fn service(state: &State, name: &str) -> Service {
    state.topic(name).map_or(Service::default(), |t| Service {
        seen: t.seen,
        valid: t.valid,
        alive: t.alive,
        receive_time: t.receive_time,
    })
}
fn vehicle(views: &Views<'_>) -> Result<VehicleContext, Error> {
    Ok(VehicleContext {
        services: [
            "carState",
            "modelV2",
            "radarState",
            "selfdriveState",
            "carControl",
        ]
        .map(|s| service(views.0, s)),
        can_valid: views.car()?.get_can_valid(),
        selfdrive_enabled: views.selfdrive()?.get_enabled(),
        longitudinal_active: views.control()?.get_long_active(),
        drive: views.car()?.get_gear_shifter()? == GearShifter::Drive,
    })
}
fn suspended(views: &Views<'_>, now: f64) -> Result<bool, Error> {
    let status = service(views.0, "carrotNavi");
    if !status.fresh(now, 2.) {
        return Ok(false);
    }
    let event = views.0.topic("carrotNavi")?.event()?;
    let navi = match event.which()? {
        event::CarrotNavi(value) => value?,
        _ => return Ok(false),
    };
    let lane = navi.get_lane_current()?;
    let lane_meta = lane.get_meta()?;
    let navigation = navi.get_navigation_status()?;
    Ok(RoadPolicy {
        service: status,
        schema_version: navi.get_schema_version(),
        connected: navi.get_connected(),
        session_id: navi.get_session_id()?.to_str()?,
        navigation_present: navigation.get_meta()?.get_present(),
        off_route: navigation.get_off_route(),
        lane_present: lane_meta.get_present(),
        road_category_valid: lane.get_road_category_valid(),
        road_category: u8::try_from(lane.get_road_category()).unwrap_or(u8::MAX),
        received_mono_ns: lane_meta.get_received_mono_time_nanos(),
    }
    .suspended(now))
}
#[derive(Default)]
struct ModelGeometry(Option<Geometry>);
impl prepare::Geometry for ModelGeometry {
    fn range(&mut self, bounds: [f64; 4], night: bool) -> Option<[f64; 2]> {
        let g = self.0.as_ref()?;
        if night {
            g.night_support(bounds)
        } else {
            g.housing_range(bounds, 3.5)
        }
    }
    fn alignment(&mut self, candidate: &Candidate) -> Option<[f64; 2]> {
        self.0
            .as_ref()?
            .alignment(candidate.bounds, candidate.distance)
    }
    fn night(&self) -> bool {
        self.0.is_some()
    }
}
pub struct Source<'a, 's, C: Clock, O: Output> {
    runtime: &'a mut RuntimeSignals,
    views: &'a Views<'s>,
    model: &'a Model,
    clock: &'a C,
    output: &'a mut O,
    geometry: ModelGeometry,
}
impl<'a, 's, C: Clock, O: Output> Source<'a, 's, C, O> {
    pub fn new(
        runtime: &'a mut RuntimeSignals,
        views: &'a Views<'s>,
        model: &'a Model,
        clock: &'a C,
        output: &'a mut O,
    ) -> Self {
        Self {
            runtime,
            views,
            model,
            clock,
            output,
            geometry: ModelGeometry::default(),
        }
    }
}
fn geometry(
    extra: &State,
    views: &Views<'_>,
    now: f64,
) -> Result<Option<crate::signal::context::GeometryContext>, Error> {
    let e = extra.topic("liveCalibration")?.event()?;
    let calibration = match e.which()? {
        event::LiveCalibration(v) => v?,
        _ => return Ok(None),
    };
    let e = extra.topic("roadCameraState")?.event()?;
    let camera = match e.which()? {
        event::RoadCameraState(v) => v?,
        _ => return Ok(None),
    };
    let e = extra.topic("deviceState")?.event()?;
    let device = match e.which()? {
        event::DeviceState(v) => v?,
        _ => return Ok(None),
    };
    let pose = views.pose()?;
    let angular = pose.get_angular_velocity_device()?;
    let device_type = match device.get_device_type()? {
        log_capnp::init_data::DeviceType::Tici => "tici",
        log_capnp::init_data::DeviceType::Tizi => "tizi",
        log_capnp::init_data::DeviceType::Mici => "mici",
        _ => "unsupported",
    };
    let sensor = match camera.get_sensor()? {
        log_capnp::frame_data::ImageSensor::Os04c10 => "os04c10",
        _ => "unsupported",
    };
    Ok(GeometryInput {
        calibration: service(extra, "liveCalibration"),
        camera: service(extra, "roadCameraState"),
        device: service(extra, "deviceState"),
        pose: service(views.0, "livePose"),
        calibrated: calibration.get_cal_status()?
            == log_capnp::live_calibration_data::Status::Calibrated,
        device_type: device_type.into(),
        sensor: sensor.into(),
        rpy: calibration.get_rpy_calib()?.iter().map(f64::from).collect(),
        pose_timestamp_ns: pose.get_timestamp(),
        inputs_ok: pose.get_inputs_o_k(),
        posenet_ok: pose.get_posenet_o_k(),
        sensors_ok: pose.get_sensors_o_k(),
        angular_valid: angular.get_valid(),
        orientation_valid: pose.get_orientation_n_e_d()?.get_valid(),
        angular: vec![
            f64::from(angular.get_x()),
            f64::from(angular.get_y()),
            f64::from(angular.get_z()),
        ],
        angular_std: vec![
            f64::from(angular.get_x_std()),
            f64::from(angular.get_y_std()),
            f64::from(angular.get_z_std()),
        ],
    }
    .geometry(now))
}
impl<C: Clock, O: Output> SignalSource for Source<'_, '_, C, O> {
    fn policy(&mut self) -> Result<bool, Error> {
        Ok(suspended(self.views, self.clock.monotonic()).unwrap_or(false))
    }
    fn start(&mut self) -> Result<StartRead, Error> {
        let now = self.clock.monotonic();
        self.runtime.ensure_params()?;
        let params = self
            .runtime
            .params
            .as_ref()
            .ok_or(Error::Contract("missing signal params"))?;
        let cache = &mut self.runtime.mode;
        let vehicle = vehicle(self.views)?;
        let (observation, context) = self.runtime.start.read(
            now,
            || current_mode(cache, params, self.clock.monotonic()).enabled(),
            &self.runtime.start_path,
            &vehicle,
        );
        #[cfg(feature = "signal-validation")]
        self.output.event(
            "signalStartAcquisition",
            serde_json::json!({
                "context":context,"observation":observation,"reader":self.runtime.start.snapshot(),
                "services":validation_services(self.views.0),
            }),
        )?;
        Ok(StartRead {
            context,
            observation,
        })
    }
    fn prepare(&mut self) -> Result<PrepareRead<'_>, Error> {
        let now = self.clock.monotonic();
        self.runtime.ensure_params()?;
        let params = self
            .runtime
            .params
            .as_ref()
            .ok_or(Error::Contract("missing signal params"))?;
        let cache = &mut self.runtime.mode;
        let vehicle = vehicle(self.views)?;
        let (mut observation, mut context) = self.runtime.prepare.read(
            now,
            || current_mode(cache, params, self.clock.monotonic()).enabled(),
            &self.runtime.prepare_path,
            &vehicle,
        );
        let mode = self.runtime.mode(self.clock.monotonic())?;
        let mode_changed = self.runtime.prepare_mode != Some(mode);
        if mode_changed {
            self.runtime.prepare_mode = Some(mode);
            self.runtime.mode_since = context.now;
        }
        if observation
            .as_ref()
            .and_then(|v| v.get("timestamp"))
            .and_then(transport::numeric)
            .is_none_or(|t| t < self.runtime.mode_since)
        {
            observation = None;
        }
        let mut turning = true;
        if self.runtime.prepare.enabled() {
            if self.runtime.extra.is_none() {
                self.runtime.extra = Some(SubMaster::for_runtime(
                    &["liveCalibration", "roadCameraState", "deviceState"],
                    Options::default(),
                )?);
            }
            let extra = self
                .runtime
                .extra
                .as_mut()
                .ok_or(Error::Contract("missing signal geometry subscriber"))?;
            extra.update(Duration::ZERO)?;
            context.now = self.clock.monotonic();
            context.valid =
                context.valid && vehicle.services.iter().all(|s| s.fresh(context.now, 0.25));
            let current = geometry(&extra.state, self.views, context.now)
                .ok()
                .flatten();
            if let Some(g) = current {
                turning = g.turning;
                self.geometry.0 = Some(Geometry {
                    focal: g.focal,
                    rpy: g.rpy,
                    edges: self.model.road_edges.clone(),
                    edge_stds: self.model.road_edge_stds.clone(),
                    lanes: self.model.lane_lines.clone(),
                    lane_probs: self.model.lane_line_probs.clone(),
                    lane_stds: self.model.lane_line_stds.clone(),
                });
            } else {
                context.valid = false;
            }
        } else {
            context.valid = false;
        }
        #[cfg(feature = "signal-validation")]
        self.output.event(
            "signalPrepareAcquisition",
            serde_json::json!({
                "read_now":now,"context":context,"observation":observation,
                "reader":self.runtime.prepare.snapshot(),"mode_since":self.runtime.mode_since,
                "actuate":mode.actuate(),"turning":turning,"mode_changed":mode_changed,
                "services":validation_services(self.views.0),
                "extra":self.runtime.extra.as_ref().map(|v|validation_services(&v.state)),
            }),
        )?;
        Ok(PrepareRead {
            context,
            observation,
            actuate: mode.actuate(),
            turning,
            mode_changed,
            geometry: &mut self.geometry,
        })
    }
    fn record_start(
        &mut self,
        decision: &crate::signal::start::Decision,
        speed: f64,
        distance: f64,
    ) -> Result<(), Error> {
        if !self.runtime.start.enabled() && self.runtime.last_start.is_none() {
            return Ok(());
        }
        let now = self.clock.monotonic();
        #[cfg(feature = "signal-validation")]
        self.output.event(
            "signalStartRecordInput",
            serde_json::json!({"now":now,"speed":speed,"model_stop_distance":distance}),
        )?;
        let key = serde_json::json!([
            decision.hold,
            decision.warn,
            decision.released,
            decision.reason,
            decision.green_confirmed
        ]);
        if self.runtime.last_start.as_ref() != Some(&key) || now - self.runtime.last_start_log >= 1.
        {
            let mut fields = serde_json::to_value(decision)?;
            fields["speed"] = serde_json::json!(speed);
            fields["model_stop_distance"] = serde_json::json!(distance);
            fields["enabled"] = serde_json::json!(self.runtime.start.enabled());
            self.output.event("signalStartGuard", fields)?;
            self.runtime.last_start = Some(key);
            self.runtime.last_start_log = now;
        }
        Ok(())
    }
    fn record_prepare(&mut self, decision: &crate::signal::prepare::Decision) -> Result<(), Error> {
        if !self.runtime.prepare.enabled() && self.runtime.last_prepare.is_none() {
            return Ok(());
        }
        let mut fields = serde_json::to_value(decision)?;
        let mode = self.runtime.prepare_mode.unwrap_or_default();
        fields["mode"] = serde_json::json!(match mode {
            Mode::Off => 0,
            Mode::Observe => 1,
            Mode::Prepare => 2,
        });
        fields["control_active"] = serde_json::json!(decision.active && mode.actuate());
        if self.runtime.last_prepare.as_ref() != Some(&fields) {
            self.output.event("signalStopPrepare", fields.clone())?;
            self.runtime.last_prepare = Some(fields);
        }
        Ok(())
    }
}

// This observation-only trace is compiled for isolated validation, never for
// production daemon selection. It preserves actual receipt times for source
// replay instead of replacing freshness with a synthetic latest value.
#[cfg(feature = "signal-validation")]
fn validation_services(state: &State) -> serde_json::Value {
    serde_json::json!(
        state
            .topics()
            .iter()
            .map(|topic| serde_json::json!({
                "name":topic.service.name,"seen":topic.seen,"valid":topic.valid,
                "alive":topic.alive,"receive_time":topic.receive_time,
            }))
            .collect::<Vec<_>>()
    )
}

#[cfg(test)]
mod constructor_tests {
    use super::*;
    #[test]
    fn fresh_mode_reads_use_source_decimal_prefix() {
        let root = tempfile::tempdir().unwrap();
        let params = Params::for_runtime_at(root.path()).unwrap();
        for (raw, expected) in [
            (b"  +1suffix".as_slice(), Mode::Observe),
            (b"2.5".as_slice(), Mode::Prepare),
            (b"1\0tail".as_slice(), Mode::Observe),
            (b"0x2".as_slice(), Mode::Off),
            (b"3".as_slice(), Mode::Off),
            (b"invalid".as_slice(), Mode::Off),
            (b"2147483648".as_slice(), Mode::Off),
        ] {
            params.put("SignalAssistMode", raw).unwrap();
            assert_eq!(
                current_mode(&mut ModeCache::default(), &params, 1.),
                expected,
                "{raw:?}"
            );
        }
    }
    #[test]
    fn injected_construction_time_primes_shared_runtime_mode() {
        let mut runtime = RuntimeSignals::new_at(Mode::Observe, 100.);
        assert_eq!(
            runtime.mode.read(100.2, || -> Result<i64, ()> {
                panic!("constructor cache must cover the first phase")
            }),
            Mode::Observe
        );
        assert_eq!(runtime.mode.read(100.5, || Ok::<_, ()>(2)), Mode::Prepare);
        assert!(runtime.start.enabled());
        assert!(runtime.prepare.enabled());
    }
}
