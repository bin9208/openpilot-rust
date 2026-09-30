use crate::{process, Options};
use openpilot_cereal::car_capnp::car_params;
use openpilot_desire::command::CommandReader;
use openpilot_driving_modeld::{
    bus::{self, Inputs},
    jetlink, parameters,
    publication::{Output, Publication, Sources},
    runtime::DrivingRuntime,
    state::State,
    Error,
};
use openpilot_messaging::{
    runtime::{PubMaster, SubMaster},
    state::Options as SubscriberOptions,
};
use openpilot_model_runtime::catalog::{Catalog, Kind};
use openpilot_modeld::{
    calibration::DrivingCalibration, camera::receive_pair, inputs::DropTracker,
    model_wire::ModelTiming,
};
use std::{
    env,
    io::Cursor,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

pub fn run(options: Options) -> Result<(), Error> {
    process::configure()?;
    let stop = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&stop))?;
    signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&stop))?;
    let params = parameters::open()?;
    let catalog = Catalog::load(&options.catalog)?;
    let Some((mut main, mut extra, main_wide)) =
        process::cameras(&stop, parameters::use_wide(&params)?)?
    else {
        return Ok(());
    };
    let mut subscribers = SubMaster::for_runtime(bus::TOPICS, SubscriberOptions::default())?;
    let mut publishers = PubMaster::for_runtime(bus::OUTPUTS)?;
    eprintln!("modeld: waiting for CarParams");
    let car_params = loop {
        if stop.load(Ordering::Relaxed) {
            return Ok(());
        }
        if let Some(bytes) = params.get("CarParams")?.filter(|bytes| !bytes.is_empty()) {
            break bytes;
        }
        thread::sleep(Duration::from_millis(100));
    };
    let car_params = capnp::serialize::read_message(
        Cursor::new(car_params),
        capnp::message::ReaderOptions::new(),
    )?;
    let longitudinal_delay = f64::from(
        car_params
            .get_root::<car_params::Reader>()?
            .get_longitudinal_actuator_delay(),
    );
    let mut state = State::new(
        longitudinal_delay,
        parameters::float(&params, "VEgoStopping")? * 0.01,
        parameters::float(&params, "CameraYawTrimDeg")? * 0.01,
    );
    let mut runtime = None;
    let mut calibration = DrivingCalibration::default();
    let mut drops = DropTracker::default();
    let mut publication = Publication::default();
    let mut commands = CommandReader::new(
        Path::new("/dev/shm/carrot-bluetooth"),
        "lane",
        process::monotonic(),
    );
    let raw = env::var_os("SEND_RAW_PRED").is_some_and(|value| !value.is_empty());
    let simulation = env::var("SIMULATION")
        .unwrap_or_else(|_| "0".to_owned())
        .trim()
        .parse::<i64>()
        .map_err(|_| Error::Contract("invalid SIMULATION"))?
        != 0;
    let mut external = openpilot_jetlink::runtime::Runtime::new(
        Path::new("/dev/shm/carrot-jetlink.sock"),
        params.get_bool("JetlinkActive")? || params.get_bool("JetlinkLossLatched")?,
    )?;
    let mut external_stored = None;
    let mut frame_count = 0;
    eprintln!("modeld: ready");
    while !stop.load(Ordering::Relaxed) {
        if state.begin_iteration() {
            state.refresh(parameters::settings(&params)?);
        }
        let Some(pair) = receive_pair(&mut main, extra.as_mut())? else {
            continue;
        };
        let frame = pair.main();
        let extra_frame = pair.extra();
        if runtime.is_none() {
            let camera = [
                u32::try_from(frame.buffer.layout.width)
                    .map_err(|_| Error::Contract("camera width overflow"))?,
                u32::try_from(frame.buffer.layout.height)
                    .map_err(|_| Error::Contract("camera height overflow"))?,
            ];
            let bundle = catalog.select(Kind::Driving, camera)?;
            let priority = if bundle.backend == "qcom-cl" {
                env::var("QCOM_PRIORITY")
                    .map_or(Ok(8), |value| value.parse::<u8>())
                    .map_err(|_| Error::Contract("invalid QCOM_PRIORITY"))?
            } else {
                8
            };
            // SAFETY: --trusted-catalog requires immutable trusted executable model artifacts.
            runtime = Some(unsafe { DrivingRuntime::load(bundle, priority) }?);
            eprintln!("modeld: models loaded");
        }
        let runtime = runtime
            .as_mut()
            .ok_or(Error::Contract("driving model not initialized"))?;
        runtime.validate_frame(&frame.buffer.layout)?;
        runtime.validate_frame(&extra_frame.buffer.layout)?;
        subscribers.update(Duration::ZERO)?;
        let sources = Inputs::read(&subscribers.state)?;
        sources.update_calibration(
            &subscribers.state,
            &mut calibration,
            state.settings.camera_yaw_trim,
            main_wide,
            extra.is_some(),
        )?;
        let dropped = drops.observe(frame.metadata.frame_id);
        let (lateral, longitudinal) = state.action_times();
        runtime.inputs.update(
            i32::from(publication.desire.desire),
            sources.monitoring.get_is_r_h_d(),
            lateral,
            longitudinal,
        );
        let start = Instant::now();
        let controls_fresh = subscribers.state.all_valid(&["carState", "carControl"])?
            && subscribers.state.all_alive(&["carState", "carControl"])?;
        let controls = openpilot_jetlink::transition::ControlState {
            standstill: controls_fresh && sources.car.get_standstill(),
            cruise_enabled: sources.car.get_cruise_state()?.get_enabled(),
            lateral_active: sources.control.get_lat_active(),
            enabled: sources.control.get_enabled(),
        };
        let mode = openpilot_jetlink::transition::Mode::from_setting(parameters::integer(
            &params,
            "JetlinkMode",
        )?);
        let validation = params.get("JetlinkValidation")?;
        let mut desire = [0.0; 8];
        let desire_index = usize::from(u16::from(publication.desire.desire));
        if let Some(value) = desire.get_mut(desire_index) {
            *value = 1.0;
        }
        let traffic = if sources.monitoring.get_is_r_h_d() {
            [0.0, 1.0]
        } else {
            [1.0, 0.0]
        };
        external.begin(
            openpilot_jetlink::runtime::FrameInput {
                mode,
                controls,
                frame: frame.metadata.frame_id,
                prepare_only: dropped.prepare_only,
                camera_ready: calibration.seen()
                    && controls_fresh
                    && !params.get_bool("UsbGpuActive")?,
                validation: validation.as_deref(),
                desire,
                traffic,
                action: [lateral as f32, longitudinal as f32],
            },
            || {
                runtime
                    .warp_for_jetlink(
                        &frame.buffer.bytes,
                        &extra_frame.buffer.bytes,
                        [calibration.main(), calibration.extra()],
                    )
                    .map_err(|error| openpilot_jetlink::Error::Native(error.to_string()))
            },
        );
        jetlink::persist(&params, &mut external_stored, &external.status)?;
        let prediction = runtime.infer(
            &frame.buffer.bytes,
            &extra_frame.buffer.bytes,
            [calibration.main(), calibration.extra()],
            dropped.prepare_only,
        )?;
        let prediction = external.finish(prediction);
        jetlink::persist(&params, &mut external_stored, &external.status)?;
        let model_execution_time = start.elapsed().as_secs_f64();
        if let Some(prediction) = prediction {
            let config = if (publication.desire.frame + 1).is_multiple_of(100) {
                Some(parameters::desire_config(&params)?)
            } else {
                None
            };
            let now = process::timestamp()?;
            let mut messages = publication.build(
                &mut state,
                Output {
                    prediction: &prediction,
                    timing: ModelTiming {
                        log_mono_time: now,
                        frame_id: frame.metadata.frame_id,
                        frame_id_extra: extra_frame.metadata.frame_id,
                        camera_state_frame_id: sources.road.get_frame_id(),
                        frame_drop: dropped.ratio,
                        timestamp_eof: frame.metadata.timestamp_eof,
                        model_execution_time,
                        valid: calibration.seen(),
                    },
                    simulation,
                    dropped: dropped.dropped,
                    raw_predictions: jetlink::raw_predictions(
                        raw,
                        external.status.decision.source,
                        runtime.raw_predictions(),
                    )?,
                },
                Sources {
                    car: sources.car,
                    navigation: sources.navigation,
                    radar: sources.radar,
                    lateral_active: sources.control.get_lat_active(),
                    live_lateral_delay: sources.delay.get_lateral_delay().into(),
                },
                || config.clone().unwrap_or_default(),
                |allowed| commands.read(allowed, process::monotonic()),
            )?;
            let fresh = external.valid_at_publish();
            jetlink::persist(&params, &mut external_stored, &external.status)?;
            jetlink::publication(&mut messages, &external.status, fresh)?;
            publishers.send("modelV2", &messages.model)?;
            publishers.send("drivingModelData", &messages.driving)?;
            publishers.send("cameraOdometry", &messages.pose)?;
        }
        frame_count += 1;
        if options.frames == Some(frame_count) {
            break;
        }
    }
    Ok(())
}
