pub mod config;
mod diagnostics;
pub mod http;
mod http_body;
mod parameters;
mod ports;
mod publisher;
mod stop;
use crate::{
    route::{Position, RouteEngine},
    Error,
};
use num_traits::ToPrimitive;
use openpilot_beepd::Clock;
use openpilot_cereal::log_capnp::event;
use openpilot_logging::{producer::Factory, record::Level, Value};
use openpilot_messaging::{
    runtime::SubMaster,
    state::{Options as SubscriptionOptions, State},
};
use openpilot_params::Params;
use ports::NativePorts;
use publisher::Publisher;
use std::{num::NonZeroU64, path::PathBuf, time::Duration};
use stop::Stop;

#[derive(Debug, Default)]
pub struct Options {
    pub frames: Option<NonZeroU64>,
    pub mapbox_host: Option<String>,
    pub persist_root: Option<PathBuf>,
}

pub fn set_destination(argument: Option<&str>) -> Result<(), Error> {
    let params = Params::for_runtime()?;
    let destination = crate::destination::Destination::from_argument(argument)?;
    if destination.default_waypoint {
        println!("Setting to Taco Bell");
    }
    let value = destination.parameter()?;
    parameters::write_status(params.put("NavDestination", value.as_bytes()))?;
    if destination.default_waypoint {
        parameters::write_status(params.put(
            "NavDestinationWaypoints",
            b"[[-117.16020713111648, 32.71997612490662]]",
        ))?;
        println!("{}", value.replace('"', "'"));
        println!("[(-117.16020713111648, 32.71997612490662)]");
    } else {
        parameters::write_status(params.remove("NavDestinationWaypoints"))?;
    }
    Ok(())
}

pub fn timestamp() -> Result<u64, Error> {
    let time = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    (openpilot_beepd::monotonic_seconds(time)? * 1e9)
        .to_u64()
        .ok_or(Error::Runtime(
            "monotonic clock outside unsigned 64-bit range",
        ))
}

fn position(state: &State) -> Result<Position, Error> {
    let event::CarrotMan(value) = state
        .topic("carrotMan")?
        .event()?
        .which()
        .map_err(capnp::Error::from)?
    else {
        return Err(Error::Runtime(
            "carrotMan service has a different event type",
        ));
    };
    let value = value?;
    Ok(Position {
        latitude: f64::from(value.get_x_pos_lat()),
        longitude: f64::from(value.get_x_pos_lon()),
        bearing: f64::from(value.get_x_pos_angle()),
    })
}

fn ui_pid(state: &State) -> Result<Option<i32>, Error> {
    let event::ManagerState(manager) = state
        .topic("managerState")?
        .event()?
        .which()
        .map_err(capnp::Error::from)?
    else {
        return Err(Error::Runtime(
            "managerState service has a different event type",
        ));
    };
    for process in manager?.get_processes()? {
        if process.get_running()
            && process
                .get_name()?
                .to_str()
                .map_err(|_| Error::Runtime("manager name is not UTF-8"))?
                == "ui"
        {
            return Ok(Some(process.get_pid()));
        }
    }
    Ok(None)
}

pub fn run(options: Options) -> Result<(), Error> {
    let stop = Stop::new()?;
    let factory = Factory::for_runtime()?;
    factory.bind_global(
        [("daemon".into(), Value::Text("navd".into()))]
            .into_iter()
            .collect(),
    )?;
    let publisher = Publisher::new(factory.clone())?;
    let mut subscriber = SubMaster::for_runtime(
        &["managerState", "carrotMan"],
        SubscriptionOptions::default(),
    )?;
    let mut ports = NativePorts {
        params: Params::for_runtime()?,
        logger: factory.logger(),
        publisher,
        stop: &stop,
    };
    let mut engine = RouteEngine::new_configured(&mut ports, |ports| {
        config::load(&ports.params, &mut ports.logger, &options)
    })?;
    let clock = openpilot_beepd::SystemClock;
    let mut next = None;
    let mut frame = 0_u64;
    let result = (|| loop {
        stop.check()?;
        subscriber.update(Duration::ZERO)?;
        if subscriber.state.topic("managerState")?.updated
            && engine.update_ui_pid(ui_pid(&subscriber.state)?)
        {
            diagnostics::text(
                &mut ports.logger,
                Level::Warning,
                "UI restarting, sending route".into(),
            );
            println!("########## UI restarting, sending route");
            ports.publisher.resend()?;
        }
        engine.update(position(&subscriber.state)?, &mut ports)?;
        let deadline = match next {
            Some(value) => value,
            None => clock.monotonic()? + 1.,
        };
        let remaining = deadline - clock.monotonic()?;
        next = Some(deadline + 1.);
        if remaining < 0. {
            println!(
                "openpilot.selfdrive.navd.navd lagging by {:.2} ms",
                -remaining * 1000.
            );
        }
        if remaining > 0. {
            stop.wait(openpilot_beepd::sleep_duration(remaining)?)?;
        }
        frame = frame.saturating_add(1);
        if options.frames.is_some_and(|limit| frame >= limit.get()) {
            return Ok(());
        }
    })();
    match result {
        Err(Error::Interrupted) => {
            diagnostics::text(
                &mut ports.logger,
                Level::Warning,
                "child openpilot.selfdrive.navd.navd got SIGINT".into(),
            );
            Ok(())
        }
        result => result,
    }
}
