use super::parameters::{Write, Writes};
use crate::{
    owner::{
        packet::{LegacyFrame, Status},
        Owner,
    },
    sources::Source,
    Error,
};
use openpilot_params::Params;

pub fn status(
    owner: &mut Owner,
    status: Status,
    session: &str,
    now: f64,
    params: &Params,
) -> Result<(), Error> {
    if let Some(index) = status.index {
        owner.serv.command.index = if index == 0 {
            owner.serv.command.index + 1
        } else {
            index
        };
    }
    if owner.serv.command.index % 60 == 0 && std::path::Path::new("/TICI").is_file() {
        if let Some((epoch, timezone)) = status.epoch {
            super::time_set::set_time(epoch.map_err(Error::Contract)?, &timezone, params)?;
        }
    }
    if let Some((command, argument, command_text, argument_text, command_hashable)) = status.command
    {
        owner.serv.command.command_index = owner.serv.command.index;
        owner.serv.command.command = command;
        owner.serv.command.argument = argument;
        owner.serv.command.command_text = command_text;
        owner.serv.command.argument_text = argument_text;
        owner.serv.command.command_hashable = command_hashable;
    }
    if let Some(fields) = status.control {
        owner.navigation.accept_legacy(fields, session, now);
    }
    if let Some(phone) = status.phone {
        owner.serv.gps.phone_angle = phone
            .heading
            .map_err(Error::Contract)?
            .unwrap_or(owner.serv.gps.angle);
        owner.serv.gps.phone_latitude = phone
            .latitude
            .map_err(Error::Contract)?
            .unwrap_or(owner.serv.gps.navi_latitude);
        owner.serv.gps.phone_longitude = phone
            .longitude
            .map_err(Error::Contract)?
            .unwrap_or(owner.serv.gps.navi_longitude);
        owner.serv.gps.phone_accuracy = phone.accuracy.map_err(Error::Contract)?;
        if owner.serv.gps.phone_accuracy < 15. {
            owner.serv.gps.phone_frame += 1;
        }
        if now - owner.serv.gps.last_navi > 3. {
            owner.serv.gps.navi_latitude = owner.serv.gps.phone_latitude;
            owner.serv.gps.navi_longitude = owner.serv.gps.phone_longitude;
            owner.serv.gps.angle = owner.serv.gps.phone_angle;
            owner.serv.gps.last_phone = now;
            owner.serv.gps.last_calculate = now;
            owner.serv.gps.speed = phone.speed.map_err(Error::Contract)?;
        }
    }
    Ok(())
}

pub fn frame(
    owner: &mut Owner,
    frame: LegacyFrame,
    session: &str,
    now: f64,
    params: &Params,
    writes: &Writes,
) -> Result<(), Error> {
    owner.events.record(frame.record);
    if let Some(status_value) = frame.status {
        if !owner.stale_rgdata(frame.timestamp, session) {
            match status_value {
                Ok(value) => {
                    if let Err(error) = status(owner, value, session, now, params) {
                        eprintln!("carrot_man legacy status: {error}");
                    }
                }
                Err(error) => {
                    eprintln!("carrot_man legacy status: {error}");
                }
            }
        }
    }
    for auxiliary in frame.auxiliary {
        owner.navigation.accept_legacy_aux(session, now, auxiliary);
    }
    if let Some(image) = frame.image {
        let selected = owner
            .navigation
            .store
            .select(now)
            .map_err(Error::Contract)?
            .snapshot;
        if selected.is_some_and(|s| s.source == Source::TmapLegacy && s.session_id == session) {
            owner.events.crossroad_summary = Some(image.summary);
            writes.send(Write::Image(image.parameter))?;
        }
    }
    if let Some(debug) = frame.debug {
        writes.send(Write::Debug(debug))?;
    }
    Ok(())
}

pub fn binary_route(
    owner: &mut Owner,
    points: Vec<(f64, f64)>,
    complete: bool,
) -> Result<Option<Vec<openpilot_navd::geometry::Coordinate>>, Error> {
    owner.route.points = points;
    if !complete {
        return Ok(None);
    }
    let coordinates: Vec<_> = owner
        .route
        .points
        .iter()
        .map(|p| openpilot_navd::geometry::Coordinate::new(p.1, p.0))
        .collect();
    owner.route.start_index = 0;
    owner.route.active = !coordinates.is_empty();
    Ok(Some(owner.route.send_routes(&coordinates, false)?))
}
pub fn route_destination(owner: &mut Owner, params: &Params) -> Result<(), Error> {
    if let Some(destination) = owner.route.points.last() {
        use openpilot_web_upload::{Fields, Value};
        let fields: Fields = [
            ("latitude".into(), Value::Float(destination.1)),
            ("longitude".into(), Value::Float(destination.0)),
            ("place_name".into(), Value::Text("External Navi".into())),
        ]
        .into_iter()
        .collect();
        params.put(
            "NavDestination",
            fields
                .to_json()
                .map_err(|_| Error::Contract("destination JSON"))?
                .as_bytes(),
        )?;
    } else {
        owner.route.points.clear();
        owner.route.start_index = 0;
        owner.route.navd_active = false;
        match params.remove("NavDestination") {
            Ok(()) => {}
            Err(openpilot_params::Error::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        };
    }
    Ok(())
}
