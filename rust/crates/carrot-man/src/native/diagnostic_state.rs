use super::super::{
    parameters::{self, Writes},
    upload::{self, Upload},
};
use super::queue_exception;
use crate::Error;
use openpilot_cereal::log_capnp::event;
use openpilot_messaging::runtime::SubMaster;
use openpilot_params::Params;
use std::time::Duration;

pub(super) struct State {
    start: Option<f64>,
    sent: bool,
    captured: bool,
    next_onroad: f64,
    pending: Option<String>,
    next_pending: f64,
    can_at: Option<f64>,
    can_requested: bool,
    car_seen: bool,
    radar_seen: bool,
}
impl Default for State {
    fn default() -> Self {
        Self {
            start: None,
            sent: false,
            captured: false,
            next_onroad: 0.,
            pending: None,
            next_pending: 0.,
            can_at: None,
            can_requested: false,
            car_seen: false,
            radar_seen: false,
        }
    }
}
fn can_error(sub: &SubMaster, params: &Params, state: &State) -> Result<bool, Error> {
    let name = parameters::text(params, "CarName");
    if name.is_empty() || name.to_uppercase() == "MOCK" {
        return Ok(false);
    }
    let car = sub.state.topic("carState")?;
    let radar = sub.state.topic("radarState")?;
    let car_error = if state.car_seen && car.alive {
        let event::CarState(reader) = car.event()?.which()? else {
            return Err(Error::Contract("carState union"));
        };
        let reader = reader?;
        reader.get_can_timeout() || !reader.get_can_valid()
    } else {
        false
    };
    let radar_error = if state.radar_seen && radar.alive {
        let event::RadarState(reader) = radar.event()?.which()? else {
            return Err(Error::Contract("radarState union"));
        };
        reader?.get_radar_errors()?.get_can_error()
    } else {
        false
    };
    Ok(car_error || radar_error)
}
pub(super) fn idle(
    state: &mut State,
    sub: &mut SubMaster,
    upload: &Upload<'_>,
    writes: &Writes,
    now: f64,
    network: bool,
    automatic: bool,
    onroad_delay: f64,
    can_delay: f64,
) -> Result<(), Error> {
    sub.update(Duration::ZERO)?;
    let onroad = upload.params.get_bool("IsOnroad")?;
    if onroad {
        if state.start.is_none() {
            let pending = state.pending.take();
            let next_pending = state.next_pending;
            *state = State::default();
            state.pending = pending;
            state.next_pending = next_pending;
            state.start = Some(now);
        } else {
            state.car_seen |= sub.state.topic("carState")?.updated;
            state.radar_seen |= sub.state.topic("radarState")?.updated;
        }
    } else {
        state.start = None;
        state.sent = false;
        state.captured = false;
        state.next_onroad = 0.;
        state.can_at = None;
        state.can_requested = false;
        state.car_seen = false;
        state.radar_seen = false;
    }
    if onroad && !state.can_requested {
        if state.can_at.is_none() && can_error(sub, upload.params, state)? {
            state.can_at = Some(now);
        }
        if state.can_at.is_some_and(|at| now - at >= can_delay) {
            state.can_requested = queue_exception(upload.params, writes, "can_error");
        }
    }
    if automatic && !state.sent {
        if let Some(start) = state.start {
            if !state.captured && now - start >= onroad_delay && now >= state.next_onroad {
                if upload.capture() {
                    state.captured = true;
                    state.next_onroad = 0.;
                } else {
                    state.next_onroad = now + 60.;
                }
            }
            if state.captured && network && now >= state.next_onroad {
                let web = upload.web("onroad", true);
                let logs = upload.carrot_logs("onroad", true);
                if upload::ok(web.as_ref()) || upload::ok(logs.as_ref()) {
                    state.sent = true;
                } else {
                    state.next_onroad = now + 60.;
                }
            }
        }
    }
    let mut exception = parameters::text(upload.params, "CarrotException");
    if !onroad && (exception == "can_error" || state.pending.as_deref() == Some("can_error")) {
        if exception == "can_error" {
            upload.params.put("CarrotException", b"")?;
        }
        state.pending = None;
        state.next_pending = 0.;
        exception.clear();
    }
    if [
        "exception",
        "log",
        "tmux_send",
        "can_error",
        "spi_error",
        "egpu_error",
    ]
    .contains(&exception.as_str())
        && state.pending.is_none()
        && now >= state.next_pending
    {
        if upload.capture() {
            state.pending = Some(exception);
            state.next_pending = 0.;
        } else {
            state.next_pending = now + 60.;
        }
    }
    if let Some(reason) = state.pending.clone() {
        if network && now >= state.next_pending {
            let web = upload.web(&reason, false);
            let web_ok = upload::ok(web.as_ref());
            let logs = upload.carrot_logs(&reason, false);
            let logs_ok = upload::ok(logs.as_ref());
            let discord_ok = upload.discord(&reason, web_ok, web.as_ref(), false);
            if web_ok || logs_ok || discord_ok {
                if reason == "exception" {
                    upload.params.put_bool("CarrotExceptionSent", true)?;
                }
                upload.params.put("CarrotException", b"")?;
                state.pending = None;
                state.next_pending = 0.;
            } else {
                state.next_pending = now + 60.;
            }
        }
    }
    Ok(())
}
