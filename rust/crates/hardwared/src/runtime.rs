pub use crate::host::Config;
use crate::host::Params;
use crate::{
    host::{self, CarCache, Usage},
    policy::{Input, Policy},
    power::{PowerMonitoring, Shutdown},
    wire, workers, Error,
};
use openpilot_cereal::log_capnp::{
    event,
    panda_state::{HarnessStatus, PandaType},
};
use openpilot_hardware_control::{HardwareControl, LinuxPlatform, ProcessCommands};
use openpilot_logging::{
    log_site,
    producer::Factory,
    record::{Level, Record},
};
use openpilot_messaging::{
    runtime::{PubMaster, SubMaster},
    state::{Options, Poll},
};
use openpilot_statsd::producer::StatLog;
use serde_json::{json, Value};
use std::{
    io::Write,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    time::Duration,
};

pub fn run(config: Config, stop: Arc<AtomicBool>) -> Result<(), Error> {
    let factory = Factory::for_runtime()?;
    let (sender, receiver) = mpsc::sync_channel(1);
    std::thread::scope(|scope| {
        let mut handles = vec![
            scope.spawn(|| workers::network(&config, sender, &stop, &factory)),
            scope.spawn(|| hardware(&config, receiver, &stop, &factory)),
        ];
        if config.board {
            handles.push(scope.spawn(|| workers::touch(&config, &stop)));
        }
        while !stop.load(Ordering::Relaxed) {
            workers::sleep(&stop, Duration::from_secs(1));
            if handles.iter().any(|handle| handle.is_finished()) {
                break;
            }
        }
        stop.store(true, Ordering::Relaxed);
        let mut result = Ok(());
        for handle in handles {
            match handle.join() {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    if result.is_ok() {
                        result = Err(error);
                    }
                }
                Err(_) => {
                    if result.is_ok() {
                        result = Err(Error::Contract("hardware worker panicked"));
                    }
                }
            }
        }
        result
    })
}
fn panda_input(subscriber: &SubMaster, now: f64) -> Result<Input, Error> {
    let topic = subscriber.state.topic("pandaStates")?;
    let event::Which::PandaStates(pandas) = topic
        .event()?
        .which()
        .map_err(|_| Error::Contract("panda event discriminant"))?
    else {
        return Err(Error::Contract("pandaStates event required"));
    };
    let pandas = pandas?;
    let mut ignition = false;
    for panda in pandas {
        if panda
            .get_panda_type()
            .map_err(|_| Error::Contract("panda type"))?
            != PandaType::Unknown
        {
            ignition |= panda.get_ignition_line() || panda.get_ignition_can();
        }
    }
    let in_car = !pandas.is_empty()
        && pandas
            .get(0)
            .get_harness_status()
            .map_err(|_| Error::Contract("harness status"))?
            != HarnessStatus::NotConnected;
    Ok(Input {
        now,
        frame: subscriber.state.frame(),
        panda_updated: topic.updated,
        panda_present: !pandas.is_empty(),
        panda_receive_time: topic.receive_time,
        ignition,
        in_car,
        ..Input::default()
    })
}
fn hardware(
    config: &Config,
    receiver: mpsc::Receiver<Value>,
    stop: &AtomicBool,
    factory: &Factory,
) -> Result<(), Error> {
    let params = config.params()?;
    let async_params = config.params()?;
    let (writes, pending) = mpsc::channel::<(&'static str, Vec<u8>)>();
    std::thread::scope(|scope| {
        let writer = scope.spawn(move || -> Result<(), Error> {
            for (key, value) in pending {
                async_params.put(key, &value)?;
            }
            Ok(())
        });
        let result = hardware_loop(config, receiver, stop, factory, &params, &writes);
        drop(writes);
        let flushed = writer
            .join()
            .map_err(|_| Error::Contract("Params writer panicked"))?;
        result.and(flushed)
    })
}
fn queue(
    writes: &mpsc::Sender<(&'static str, Vec<u8>)>,
    key: &'static str,
    value: String,
) -> Result<(), Error> {
    writes
        .send((key, value.into_bytes()))
        .map_err(|_| Error::Contract("Params writer stopped"))
}
fn maximum(value: &Value) -> f64 {
    value.as_array().map_or(0., |list| {
        list.iter()
            .filter_map(Value::as_f64)
            .reduce(f64::max)
            .unwrap_or(0.)
    })
}
fn hardware_loop(
    config: &Config,
    receiver: mpsc::Receiver<Value>,
    stop: &AtomicBool,
    factory: &Factory,
    params: &Params,
    writes: &mpsc::Sender<(&'static str, Vec<u8>)>,
) -> Result<(), Error> {
    let mut publisher = PubMaster::for_runtime(&["deviceState"])?;
    let mut subscriber = SubMaster::for_runtime(
        &[
            "peripheralState",
            "gpsLocationExternal",
            "selfdriveState",
            "pandaStates",
        ],
        Options {
            poll: Poll::One("pandaStates".into()),
            ..Options::default()
        },
    )?;
    let hardware = config.hardware();
    let device = hardware.get_device_type()?;
    let mut control = if config.board {
        HardwareControl::board(&device)
    } else {
        HardwareControl::pc()
    };
    let mut platform = LinuxPlatform::new(
        &config.root,
        ProcessCommands {
            launcher: config.launcher.clone(),
        },
    );
    let mut logger = factory.logger();
    let mut stats = if config.root == Path::new("/") {
        StatLog::default()
    } else {
        StatLog::new(format!("ipc://{}", config.root.join("stats").display()))
    };
    let mut cache = CarCache::default();
    let mut power = PowerMonitoring::new(host::numeric(params, "CarBatteryCapacity", false)?);
    let mut policy = Policy::new(&device);
    let mut uptime_offroad = host::numeric(params, "UptimeOffroad", true)?;
    let mut uptime_onroad = host::numeric(params, "UptimeOnroad", true)?;
    let mut last_uptime = crate::monotonic();
    let mut engaged_previous = false;
    let mut previous_alert = None;
    let mut last_network = json!({"networkType":0,"networkStrength":0,"networkMetered":false,"networkStats":{"wwanTx":-1,"wwanRx":-1},"modemTempC":[]});
    let mut usage = Usage::new(&config.root)?;
    control.initialize_hardware(&mut platform)?;
    let mut thermal = hardware.get_thermal_config()?;
    while !stop.load(Ordering::Relaxed) {
        subscriber.update(Duration::from_millis(150))?;
        if stop.load(Ordering::Relaxed) {
            break;
        }
        let mut input = panda_input(&subscriber, crate::monotonic())?;
        input.cycle_requested = params.get_bool("OnroadCycleRequested")?;
        if input.cycle_requested {
            params.put_bool("OnroadCycleRequested", false)?;
        }
        let (tick, timed_out) = policy.poll(&input);
        if timed_out {
            logger.emit(
                log_site!(),
                Record::text(Level::Error, "panda timed out onroad".into()),
            )?;
        }
        if !tick {
            continue;
        }
        let mut device_state = host::hardware_json(thermal.get_msg()?)?;
        if let Ok(value) = receiver.try_recv() {
            last_network = value;
        }
        for (key, value) in last_network
            .as_object()
            .ok_or(Error::Contract("network object"))?
        {
            device_state[key] = value.clone();
        }
        let log_root = if config.root == Path::new("/") {
            std::path::PathBuf::from(openpilot_hardware_info::paths::Paths::default().log_root()?)
        } else {
            config.root.join("data/media/0/realdata")
        };
        let free = rustix::fs::statvfs(&log_root).map_or(100., |stat| {
            100. * stat.f_bavail as f64 / stat.f_blocks as f64
        });
        device_state["freeSpacePercent"] = json!(free as f32);
        device_state["deviceType"] = json!(device);
        device_state["memoryUsagePercent"] = json!(usage.memory()?.round_ties_even());
        device_state["gpuUsagePercent"] = json!(hardware
            .get_gpu_usage_percent()?
            .to_f64()?
            .round_ties_even());
        let mut cpu = usage.cpu()?;
        let cpu_count = device_state["cpuTempC"].as_array().map_or(0, Vec::len);
        if cpu.len() < cpu_count {
            cpu.resize(cpu_count, 0.);
        }
        device_state["cpuUsagePercent"] =
            json!(cpu.iter().map(|v| v.round_ties_even()).collect::<Vec<_>>());
        let brightness = hardware.get_screen_brightness()?.to_f64()?.trunc();
        device_state["screenBrightnessPercent"] = json!(brightness);
        // Python reads temperatures back from Float32 cereal fields before filtering.
        for name in ["cpuTempC", "gpuTempC", "pmicTempC"] {
            if let Some(list) = device_state.get_mut(name).and_then(Value::as_array_mut) {
                for value in list {
                    *value = json!(value.as_f64().ok_or(Error::Contract("temperature"))? as f32);
                }
            }
        }
        let memory_temp = device_state["memoryTempC"].as_f64().unwrap_or(0.) as f32;
        input.offroad_temperature = f64::from(memory_temp)
            .max(maximum(&device_state["cpuTempC"]))
            .max(maximum(&device_state["gpuTempC"]));
        input.pmic_temperature = maximum(&device_state["pmicTempC"]);
        input.startup = host::startup(params, f64::from(free as f32), !config.board)?;
        input.booted = policy.booted() || hardware.booted()?;
        cache.update(params, crate::monotonic())?;
        input.tesla = cache.tesla;
        input.brightness = brightness;
        input.now = crate::monotonic();
        let output = policy.step(&input);
        if config.board {
            openpilot_hardware_control::gpio_set(&mut platform, 49, output.fan > 0)?;
        }
        let alert = (
            output.temperature_alert,
            format!("{:.1}C", output.offroad_temperature),
        );
        if previous_alert.as_ref() != Some(&alert) {
            if alert.0 {
                let alerts: Value = serde_json::from_str(include_str!(
                    "../../../../openpilot/selfdrive/selfdrived/alerts_offroad.json"
                ))?;
                let mut value = alerts["Offroad_TemperatureTooHigh"].clone();
                value["extra"] = json!(alert.1);
                params.put("Offroad_TemperatureTooHigh", &serde_json::to_vec(&value)?)?;
            } else {
                params.remove("Offroad_TemperatureTooHigh")?;
            }
            previous_alert = Some(alert);
        }
        if output.reset_engaged {
            params.put_bool("IsEngaged", false)?;
            engaged_previous = false;
        }
        let selfdrive = subscriber.state.topic("selfdriveState")?;
        if selfdrive.updated {
            let event::Which::SelfdriveState(state) = selfdrive
                .event()?
                .which()
                .map_err(|_| Error::Contract("selfdrive event"))?
            else {
                return Err(Error::Contract("selfdriveState required"));
            };
            let engaged = state?.get_enabled();
            if engaged != engaged_previous {
                params.put_bool("IsEngaged", engaged)?;
                engaged_previous = engaged;
            }
            // kmsg failure is deliberately nonfatal, matching the source.
            match std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(config.root.join("dev/kmsg"))
                .and_then(|mut file| {
                    writeln!(
                        file,
                        "<3>[hardware] engaged: {}",
                        if engaged { "True" } else { "False" }
                    )
                }) {
                Ok(()) | Err(_) => {}
            }
        }
        if output.power_save_changed {
            control.set_power_save(&mut platform, output.power_save)?;
        }
        if output.startup_changed {
            workers::log_event(
                &mut logger,
                "Startup blocked",
                json!({"startup_conditions":policy.startup,"onroad_conditions":policy.onroad,"error":true}),
            )?;
        }
        if let Some(duration) = output.block_duration {
            workers::log_event(
                &mut logger,
                "Startup after block",
                json!({"block_duration":duration,"startup_conditions":policy.startup,"onroad_conditions":policy.onroad,"startup_conditions_prev":policy.startup_previous,"error":true}),
            )?;
        }
        let peripheral = subscriber.state.topic("peripheralState")?;
        let event::Which::PeripheralState(peripheral_state) = peripheral
            .event()?
            .which()
            .map_err(|_| Error::Contract("peripheral event"))?
        else {
            return Err(Error::Contract("peripheralState required"));
        };
        let peripheral_state = peripheral_state?;
        let voltage = if peripheral_state
            .get_panda_type()
            .map_err(|_| Error::Contract("peripheral panda type"))?
            == PandaType::Unknown
        {
            None
        } else {
            Some(f64::from(peripheral_state.get_voltage()))
        };
        let (save, error) =
            power.calculate_with(crate::monotonic(), voltage, policy.ignition, || {
                hardware
                    .get_current_power_draw()
                    .and_then(|n| n.to_f64())
                    .map_err(|error| format!("Power monitoring calculation failed: {error}"))
            });
        if voltage.is_some() {
            stats.gauge("car_voltage", power.voltage / 1e3)?;
        }
        if let Some(capacity) = save {
            queue(writes, "CarBatteryCapacity", capacity.to_string())?;
        }
        if let Some(error) = error {
            workers::log_error(&mut logger, "Power monitoring calculation failed", &error)?;
        }
        device_state["offroadPowerUsageUwh"] = json!(power.used.trunc());
        device_state["carBatteryCapacityUwh"] = json!((power.capacity as i64).max(0));
        let draw = hardware.get_current_power_draw()?.to_f64()?;
        stats.sample("power_draw", draw)?;
        device_state["powerDrawW"] = json!(draw);
        let som = hardware.get_som_power_draw()?.to_f64()?;
        stats.sample("som_power_draw", som)?;
        device_state["somPowerDrawW"] = json!(som);
        if !cache.tesla
            && policy.off_ts.is_some()
            && power.should_shutdown(&Shutdown {
                now: crate::monotonic(),
                ignition: policy.ignition,
                in_car: policy.in_car,
                off_ts: policy.off_ts,
                started_seen: policy.started_seen,
                max_offroad_minutes: i64::from(host::integer(params, "MaxTimeOffroadMin")?),
                disable: params.get_bool("DisablePowerDown")?,
                force: params.get_bool("ForcePowerDown")?,
            })
        {
            logger.emit(
                log_site!(),
                Record::text(
                    Level::Warning,
                    format!("shutting device down, offroad since {:?}", policy.off_ts),
                ),
            )?;
            params.put_bool("DoShutdown", true)?;
        }
        device_state["started"] = json!(output.started);
        device_state["startedMonoTime"] = json!((output.started_ts.unwrap_or(0.) * 1e9) as u64);
        if let Some(value) = host::last_ping(params)? {
            device_state["lastAthenaPingTime"] = value;
        }
        device_state["thermalStatus"] = json!(output.thermal);
        device_state["maxTempC"] = json!(output.max_temperature);
        device_state["fanSpeedPercentDesired"] = json!(output.fan);
        let (bytes, packet) = wire::device(&device_state, crate::monotonic())?;
        publisher.send("deviceState", &bytes)?;
        emit_stats(&mut stats, &packet["deviceState"], &last_network)?;
        let count = policy.count - 1;
        if output.rising_edge || count.is_multiple_of(1200) {
            let gps = subscriber.state.topic("gpsLocationExternal")?;
            let dat = json!({"count":count,"pandaStates":wire::json_value(subscriber.state.topic("pandaStates")?.data()?)?, "peripheralState":wire::json_value(peripheral.data()?)?, "location":if gps.alive {wire::json_value(gps.data()?)?} else {Value::Null}, "deviceState":packet});
            workers::log_event(&mut logger, "STATUS_PACKET", dat.clone())?;
            if output.rising_edge {
                if let Err(error) =
                    params.put("LastOffroadStatusPacket", &serde_json::to_vec(&dat)?)
                {
                    workers::log_error(&mut logger, "failed to save offroad status", &error)?;
                }
            }
        }
        queue(
            writes,
            "NetworkMetered",
            if device_state["networkMetered"].as_bool().unwrap_or(false) {
                "1"
            } else {
                "0"
            }
            .into(),
        )?;
        let now = crate::monotonic();
        if let Some(off) = policy.off_ts.filter(|t| *t != 0.) {
            uptime_offroad += now - last_uptime.max(off);
        } else if let Some(started) = policy.started_ts.filter(|t| *t != 0.) {
            uptime_onroad += now - last_uptime.max(started);
        }
        last_uptime = now;
        if count.is_multiple_of(120) {
            params.put("UptimeOffroad", uptime_offroad.to_string().as_bytes())?;
            params.put("UptimeOnroad", uptime_onroad.to_string().as_bytes())?;
        }
        if config.cycles.is_some_and(|limit| policy.count >= limit) {
            break;
        }
    }
    Ok(())
}
fn emit_stats(stats: &mut StatLog, state: &Value, network: &Value) -> Result<(), Error> {
    for (field, metric) in [
        ("freeSpacePercent", "free_space_percent"),
        ("gpuUsagePercent", "gpu_usage_percent"),
        ("memoryUsagePercent", "memory_usage_percent"),
        ("memoryTempC", "memory_temperature"),
        ("fanSpeedPercentDesired", "fan_speed_percent_desired"),
        ("screenBrightnessPercent", "screen_brightness_percent"),
    ] {
        if let Some(integer) = state[field].as_i64() {
            stats.gauge(metric, integer)?;
        } else {
            stats.gauge(metric, state[field].as_f64().unwrap_or(0.))?;
        }
    }
    for (field, prefix, suffix) in [
        ("cpuUsagePercent", "cpu", "_usage_percent"),
        ("cpuTempC", "cpu", "_temperature"),
        ("gpuTempC", "gpu", "_temperature"),
        ("pmicTempC", "pmic", "_temperature"),
        ("modemTempC", "modem_temperature", ""),
    ] {
        let source = if field == "modemTempC" {
            network
        } else {
            state
        };
        if let Some(values) = source[field].as_array() {
            for (index, value) in values.iter().enumerate() {
                let name = format!("{prefix}{index}{suffix}");
                if let Some(integer) = value.as_i64() {
                    stats.gauge(&name, integer)?;
                } else {
                    stats.gauge(
                        &name,
                        value.as_f64().ok_or(Error::Contract("metric value"))?,
                    )?;
                }
            }
        }
    }
    Ok(())
}
